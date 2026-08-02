//! Behavioural proof for the browser-history consent gate (TI-6).
//!
//! This deliberately uses only the public scan-state API: it proves the
//! native flow, independently of the consent grid that presents it.

use std::fs;

use osl_privacy_hub::browser_profile_scan::{
    BrowserProfileRoots, BrowserProfileScanState, BrowserProfileTransition, CONSENT_GRANT_TTL,
};
use osl_privacy_hub::native_apps::BrowserImportId;
use tempfile::TempDir;

const OWNER: &str = "consent-flow-owner";
const PROFILE: &str = "Default";
const NOW: u64 = 1_700_000_000_000;

fn fixture() -> (TempDir, BrowserProfileRoots, BrowserProfileScanState) {
    let workspace = tempfile::tempdir().expect("create isolated browser fixture");
    let browser_root = workspace.path().join("Chrome");
    let profile = browser_root.join(PROFILE);
    fs::create_dir_all(&profile).expect("create browser profile");

    // The consent covers browsing-history sites. Login Data is intentionally
    // present but must never be read or reported by this flow.
    fs::write(
        profile.join("History"),
        b"https://public.example/path\nhttps://second.example/\n",
    )
    .expect("write history fixture");
    fs::write(
        profile.join("Login Data"),
        b"https://credential-origin.invalid/private\n",
    )
    .expect("write out-of-scope login fixture");

    let roots = BrowserProfileRoots {
        chrome: Some(browser_root),
        ..BrowserProfileRoots::default()
    };
    let state = BrowserProfileScanState::load(workspace.path().join("snapshots"))
        .expect("initialise scan state");
    (workspace, roots, state)
}

fn inventory(state: &mut BrowserProfileScanState, roots: &BrowserProfileRoots) {
    let profiles = state.list_profiles(roots).expect("list profile labels");
    assert_eq!(profiles.len(), 1, "inventory exposes labels, not contents");
    assert_eq!(profiles[0].browser_id, BrowserImportId::Chrome);
    assert_eq!(profiles[0].profile, PROFILE);
}

#[test]
fn ti_6_consent_flow_is_bound_one_shot_expiring_revocable_and_history_only() {
    let (_workspace, roots, mut state) = fixture();

    // An inventory is required before consent may be granted; it exposes only
    // the browser/profile label that the person chose.
    assert!(state
        .grant_profile_consent(OWNER, BrowserImportId::Chrome, PROFILE, NOW)
        .is_err());
    inventory(&mut state, &roots);

    let grant = state
        .grant_profile_consent(OWNER, BrowserImportId::Chrome, PROFILE, NOW)
        .expect("grant explicit consent for the inventoried profile");
    assert_eq!(
        grant.expires_at_unix_ms,
        NOW + CONSENT_GRANT_TTL.as_millis() as u64,
        "the grant is limited to the promised five-minute window"
    );

    let receipt = state
        .scan_consented_profile(
            OWNER,
            &roots,
            BrowserImportId::Chrome,
            PROFILE,
            &grant.grant_id,
            NOW + 1,
        )
        .expect("the exact fresh grant scans the selected history file");
    assert_eq!(receipt.browser_id, BrowserImportId::Chrome);
    assert_eq!(receipt.profile, PROFILE);
    assert_eq!(receipt.account, "browser-history");
    assert_eq!(receipt.scope, "history-footprint");
    assert_eq!(receipt.observation_count, 2);
    assert!(receipt.snapshot_deleted, "the temporary history copy is removed");

    // Sabotage proof: changing the implementation to retain a consumed grant
    // makes this replay succeed and this test red.
    assert!(state
        .scan_consented_profile(
            OWNER,
            &roots,
            BrowserImportId::Chrome,
            PROFILE,
            &grant.grant_id,
            NOW + 2,
        )
        .is_err(), "a consumed grant must never be replayable");

    inventory(&mut state, &roots);
    let expired = state
        .grant_profile_consent(OWNER, BrowserImportId::Chrome, PROFILE, NOW)
        .expect("mint expiring grant");
    assert!(state
        .scan_consented_profile(
            OWNER,
            &roots,
            BrowserImportId::Chrome,
            PROFILE,
            &expired.grant_id,
            expired.expires_at_unix_ms,
        )
        .is_err(), "a five-minute grant must not work at its expiry");

    for transition in [
        BrowserProfileTransition::Lock,
        BrowserProfileTransition::Burn,
        BrowserProfileTransition::IdentitySwitch,
    ] {
        inventory(&mut state, &roots);
        let revoked = state
            .grant_profile_consent(OWNER, BrowserImportId::Chrome, PROFILE, NOW + 10)
            .expect("mint grant before security transition");
        state.apply_transition(transition);
        assert!(state
            .scan_consented_profile(
                OWNER,
                &roots,
                BrowserImportId::Chrome,
                PROFILE,
                &revoked.grant_id,
                NOW + 11,
            )
            .is_err(), "security transition must revoke browser-read consent");
    }
}
