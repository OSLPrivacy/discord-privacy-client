#![cfg(feature = "core")]

//! NEW-3 — Settings → Account said "Identity list could not be read" seconds
//! after a first-run account creation that had demonstrably succeeded.
//!
//! `list_hub_identities` is the only command whose first call on a brand-new
//! install has to migrate the flat, pre-slot account layout that onboarding
//! writes (`<base>/identity.json`) into `hub-identities/<slot>/`. Nothing
//! covered that sequence: `windows_identity_lifecycle.rs` is
//! `#![cfg(all(windows, feature = "core"))]`, so on every other target the
//! migration ran for the first time in front of a real owner.
//!
//! This is the missing seam: create an identity and its password exactly as
//! onboarding does, then read the identity list exactly as Settings does.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::identity_registry::{self, HubIdentityRegistryState};
use osl_privacy_hub::password_lifecycle;

const PASSWORD: &str = "aB3!z9-safe-passphrase";

fn isolated_root() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("osl-hub-first-run-{}-{nonce}", std::process::id()))
}

/// The account directory onboarding writes into: the shared base, because no
/// slot is active yet on a first run.
fn account_dir(root: &Path) -> PathBuf {
    root.join("osl-core")
}

#[test]
fn settings_reads_the_identity_list_right_after_first_run_account_creation() {
    let root = isolated_root();
    let base = account_dir(&root);
    std::fs::create_dir_all(&base).expect("create isolated base dir");
    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(Some(base.clone()));

    let state = HubCoreState::bootstrap_from_disk();

    // Written only after the first bootstrap so the legacy loader cannot use
    // its `user_id` to generate an identity ahead of the lifecycle API under
    // test. The loopback address also guarantees no production contact when
    // the migration below re-runs autostart.
    std::fs::write(
        base.join("keyserver.json"),
        br#"{"base_url":"http://127.0.0.1:1","user_id":"isolated-first-run-probe"}"#,
    )
    .expect("write loopback-only keyserver override");

    let created = password_lifecycle::create_native_identity(
        &state,
        Some(password_lifecycle::IdentityCreationOwnerAuthorization::ExplicitOwnerSignoff),
    )
    .expect("first-run account creation succeeds");
    assert_eq!(
        created
            .identity_recovery_phrase
            .as_deref()
            .map(|phrase| phrase.split_whitespace().count()),
        Some(12),
        "first run hands the owner a real 12-word phrase",
    );
    password_lifecycle::setup_main_password(&state, PASSWORD.to_owned())
        .expect("first-run password setup succeeds");

    // Onboarding wrote the account flat, with no slot registry and no marker.
    assert!(
        base.join("identity.json").is_file(),
        "onboarding writes the identity flat in the base directory",
    );
    assert!(
        !base.join("hub-active-identity").exists(),
        "first run leaves no active-slot marker for the list to find",
    );

    // Settings → Account. This is the call that reported
    // "Identity list could not be read".
    let registry_state = HubIdentityRegistryState::default();
    let slots = identity_registry::list_identity_slots(&state, &registry_state)
        .expect("Settings can read the identity list of a freshly created account");

    assert_eq!(slots.len(), 1, "one identity exists, so one slot is listed");
    let slot = &slots[0];
    assert!(
        slot.active,
        "the only identity on the device is the active one"
    );
    assert_eq!(slot.osl_user_id, created.user_id);
    // The renderer rejects the whole list on an empty label or user id.
    assert!(!slot.label.is_empty());
    assert!(!slot.osl_user_id.is_empty());

    // A second read must be steady, not a one-shot that only works before the
    // migration has left its own artefacts behind.
    let again = identity_registry::list_identity_slots(&state, &registry_state)
        .expect("the identity list stays readable on every later visit");
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].slot_id, slot.slot_id);

    ipc::main_password::set_file_storage_key(None);
    keystore::set_base_dir_override(None);
    let _ = std::fs::remove_dir_all(&root);
}
