//! TASK 6860 — story privacy settings, auto-burn, receipts and shield honesty.

use std::collections::BTreeMap;

use story_privacy::{
    observe_viewer_records, shield_state_for, Directory, Relationship, StoryAudience,
    StoryLifetime, StoryPrivacyClient, ViewOutcome, SHIELD_DISCLOSURE, SHIELD_PRIMITIVE,
    SHIELD_UNAVAILABLE_COPY, VIEW_RECEIPT_OFF_VIEWER_COPY, VIEW_RECEIPT_VIEWER_COPY,
};
use tempfile::TempDir;

const T0: i64 = 1_754_000_000_000;
const KEY: [u8; 32] = [0x5a; 32];
const VIEWER: &str = "viewer-identity-a1b2c3d4e5f60718";
const FRIEND: &str = "viewer-identity-29384756abcdef01";
const STRANGER: &str = "person-stranger-77aa11";

fn roster() -> (Directory, BTreeMap<String, [u8; 32]>) {
    let mut directory = Directory::new();
    let mut pairwise = BTreeMap::new();
    for (index, (person, friend, verified)) in [
        (STRANGER, false, false),
        (FRIEND, true, false),
        (VIEWER, true, true),
    ]
    .into_iter()
    .enumerate()
    {
        directory.insert(person, Relationship { friend, verified });
        pairwise.insert(person.to_owned(), [(index as u8) + 1; 32]);
    }
    (directory, pairwise)
}

#[test]
fn every_default_is_inherited_by_a_real_story_and_survives_restart() {
    let (directory, pairwise) = roster();
    for audience in StoryAudience::ALL {
        for lifetime in StoryLifetime::ALL {
            let temp = TempDir::new().expect("temp");
            let (mut client, _) =
                StoryPrivacyClient::open_with_shield_support(temp.path(), KEY, T0, false)
                    .expect("open");
            client.set_default_audience(audience).expect("audience");
            client.set_default_lifetime(lifetime).expect("lifetime");
            client
                .publish_story("s1", "author", b"body", None, &directory, &pairwise, T0)
                .expect("publish");
            drop(client);

            let (client, _) =
                StoryPrivacyClient::open_with_shield_support(temp.path(), KEY, T0 + 1, false)
                    .expect("reopen");
            let story = client.published("s1").expect("story");
            assert_eq!(story.audience, audience.stable_id());
            assert_eq!(story.lifetime, lifetime.stable_id());
            assert_eq!(story.audience_source, "inherited-default");
            assert_eq!(story.expires_at_ms, T0 + lifetime.seconds() * 1_000);
            assert_eq!(client.defaults().audience, audience);
            assert_eq!(client.defaults().lifetime, lifetime);
        }
    }
}

#[test]
fn each_burn_boundary_fires_after_the_app_was_closed_across_it() {
    let (directory, pairwise) = roster();
    for lifetime in StoryLifetime::ALL {
        let temp = TempDir::new().expect("temp");
        let (mut client, _) =
            StoryPrivacyClient::open_with_shield_support(temp.path(), KEY, T0, false)
                .expect("open");
        client.set_default_audience(StoryAudience::Everyone).expect("audience");
        client.set_default_lifetime(lifetime).expect("lifetime");
        let published = client
            .publish_story("s1", "author", b"burn body", None, &directory, &pairwise, T0)
            .expect("publish");
        assert!(!client.sealed_body("s1").unwrap_or_default().is_empty());
        drop(client);

        let secret = pairwise.get(VIEWER).copied().expect("secret");
        let deadline = published.expires_at_ms;

        let (client, before) =
            StoryPrivacyClient::open_with_shield_support(temp.path(), KEY, deadline - 1_000, false)
                .expect("reopen before");
        assert!(before.burned.is_empty(), "{} burned early", lifetime.label());
        assert_eq!(
            client
                .open_story("s1", VIEWER, &secret, deadline - 1_000)
                .expect("readable before the deadline"),
            b"burn body".to_vec()
        );
        drop(client);

        let (client, at) =
            StoryPrivacyClient::open_with_shield_support(temp.path(), KEY, deadline, false)
                .expect("reopen at boundary");
        assert_eq!(at.burned, vec!["s1".to_owned()], "{}", lifetime.label());
        assert!(client.is_burned("s1"));
        assert!(client.open_story("s1", VIEWER, &secret, deadline).is_err());
        assert_eq!(client.sealed_body("s1"), None, "sealed bytes survived the burn");
        drop(client);

        let (client, later) = StoryPrivacyClient::open_with_shield_support(
            temp.path(),
            KEY,
            deadline + 86_400_000,
            false,
        )
        .expect("reopen later");
        assert!(later.burned.is_empty());
        assert_eq!(later.already_burned, 1);
        assert!(client
            .open_story("s1", VIEWER, &secret, deadline + 86_400_000)
            .is_err());
    }
}

#[test]
fn send_to_overrides_the_default_for_one_story_only() {
    let (directory, pairwise) = roster();
    let temp = TempDir::new().expect("temp");
    let (mut client, _) =
        StoryPrivacyClient::open_with_shield_support(temp.path(), KEY, T0, false).expect("open");
    client.set_default_audience(StoryAudience::Everyone).expect("audience");

    let inherited = client
        .publish_story("inherited", "author", b"inherited", None, &directory, &pairwise, T0)
        .expect("publish inherited");
    let overridden = client
        .publish_story(
            "overridden",
            "author",
            b"overridden",
            Some(StoryAudience::Verified),
            &directory,
            &pairwise,
            T0,
        )
        .expect("publish overridden");

    assert_eq!(inherited.audience, StoryAudience::Everyone.stable_id());
    assert_eq!(inherited.audience_source, "inherited-default");
    assert_eq!(overridden.audience, StoryAudience::Verified.stable_id());
    assert_eq!(overridden.audience_source, "send-to-override");
    assert_eq!(overridden.audience_size, 1);

    let secret_of = |person: &str| *pairwise.get(person).expect("secret");
    assert!(client
        .open_story("inherited", STRANGER, &secret_of(STRANGER), T0 + 1)
        .is_ok());
    assert!(client
        .open_story("overridden", STRANGER, &secret_of(STRANGER), T0 + 1)
        .is_err());
    assert!(client
        .open_story("overridden", FRIEND, &secret_of(FRIEND), T0 + 1)
        .is_err());
    assert_eq!(
        client
            .open_story("overridden", VIEWER, &secret_of(VIEWER), T0 + 1)
            .expect("verified viewer opens"),
        b"overridden".to_vec()
    );
    // The default itself is untouched by one story's override.
    assert_eq!(client.defaults().audience, StoryAudience::Everyone);
}

#[test]
fn receipts_off_creates_and_retains_nothing_in_any_observer() {
    let (directory, pairwise) = roster();
    let temp = TempDir::new().expect("temp");
    let (mut client, _) =
        StoryPrivacyClient::open_with_shield_support(temp.path(), KEY, T0, false).expect("open");
    client.set_default_audience(StoryAudience::Everyone).expect("audience");
    client.set_view_receipts(false).expect("receipts off");
    client
        .publish_story("s1", "author", b"body", None, &directory, &pairwise, T0)
        .expect("publish");

    for (index, viewer) in [VIEWER, FRIEND, STRANGER].into_iter().enumerate() {
        let outcome = client
            .record_view("s1", viewer, &format!("open-event-{index}"), T0 + 60_000)
            .expect("view");
        assert_eq!(outcome, ViewOutcome::NoSignalRecorded);
    }
    assert!(client.view_signal("s1").is_none());
    assert_eq!(client.viewer_pre_open_copy(), VIEW_RECEIPT_OFF_VIEWER_COPY);

    let observation =
        observe_viewer_records(&client, &KEY, &[VIEWER, FRIEND, STRANGER]).expect("observe");
    assert_eq!(observation.total_viewer_records(), 0);
    assert_eq!(observation.total_identity_hits(), 0);
    assert!(observation.store_bytes_scanned > 0, "observer scanned nothing");
    drop(client);

    let (client, _) =
        StoryPrivacyClient::open_with_shield_support(temp.path(), KEY, T0 + 120_000, false)
            .expect("reopen");
    let after =
        observe_viewer_records(&client, &KEY, &[VIEWER, FRIEND, STRANGER]).expect("observe again");
    assert_eq!(after.total_viewer_records(), 0);
    assert_eq!(after.total_identity_hits(), 0);
}

#[test]
fn receipts_on_produces_only_the_aggregate_and_off_erases_it_retroactively() {
    let (directory, pairwise) = roster();
    let temp = TempDir::new().expect("temp");
    let (mut client, _) =
        StoryPrivacyClient::open_with_shield_support(temp.path(), KEY, T0, false).expect("open");
    client.set_default_audience(StoryAudience::Everyone).expect("audience");
    client.set_view_receipts(true).expect("receipts on");
    client
        .publish_story("s1", "author", b"body", None, &directory, &pairwise, T0)
        .expect("publish");
    assert_eq!(client.viewer_pre_open_copy(), VIEW_RECEIPT_VIEWER_COPY);

    for (index, viewer) in [VIEWER, FRIEND, STRANGER].into_iter().enumerate() {
        client
            .record_view("s1", viewer, &format!("open-event-{index}"), T0 + 60_000)
            .expect("view");
    }
    assert_eq!(client.view_signal("s1").expect("signal").view_count, 3);
    // Replaying the same event ids cannot inflate the number.
    for (index, viewer) in [VIEWER, FRIEND, STRANGER].into_iter().enumerate() {
        let outcome = client
            .record_view("s1", viewer, &format!("open-event-{index}"), T0 + 90_000)
            .expect("replay");
        assert!(matches!(outcome, ViewOutcome::ReplayIgnored(_)));
    }
    assert_eq!(client.view_signal("s1").expect("signal").view_count, 3);
    // A genuinely new open may move it, exactly as the viewer copy discloses.
    client
        .record_view("s1", VIEWER, "open-event-new", T0 + 120_000)
        .expect("repeat open");
    assert_eq!(client.view_signal("s1").expect("signal").view_count, 4);

    // The whole signal is two fields.
    let signal = serde_json::to_value(client.view_signal("s1").expect("signal")).expect("json");
    let mut fields: Vec<String> = signal
        .as_object()
        .expect("object")
        .keys()
        .cloned()
        .collect();
    fields.sort();
    assert_eq!(fields, vec!["story_id".to_owned(), "view_count".to_owned()]);

    let on = observe_viewer_records(&client, &KEY, &[VIEWER, FRIEND, STRANGER]).expect("observe");
    assert_eq!(on.total_identity_hits(), 0, "an identity reached an observer");
    assert!(on.total_viewer_records() > 0);

    let erasure = client.set_view_receipts(false).expect("receipts off");
    assert_eq!(erasure.rows_destroyed, 1);
    assert!(erasure.records_destroyed >= 5);
    assert!(client.view_signal("s1").is_none());
    let after = observe_viewer_records(&client, &KEY, &[VIEWER, FRIEND, STRANGER]).expect("observe");
    assert_eq!(after.total_viewer_records(), 0);
    assert_eq!(after.total_identity_hits(), 0);
    drop(client);

    let (client, _) =
        StoryPrivacyClient::open_with_shield_support(temp.path(), KEY, T0 + 180_000, false)
            .expect("reopen");
    let restarted =
        observe_viewer_records(&client, &KEY, &[VIEWER, FRIEND, STRANGER]).expect("observe");
    assert_eq!(restarted.total_viewer_records(), 0);
    assert_eq!(restarted.total_identity_hits(), 0);
}

#[test]
fn the_shield_claims_nothing_without_a_supported_primitive() {
    let unsupported = shield_state_for(false, true);
    assert!(!unsupported.supported);
    assert!(!unsupported.control_enabled);
    assert!(!unsupported.setting_on);
    assert!(!unsupported.claims_protection);
    assert_eq!(unsupported.primitive, None);
    assert_eq!(unsupported.disclosure, None);
    assert_eq!(unsupported.unavailable_copy, Some(SHIELD_UNAVAILABLE_COPY));

    let supported = shield_state_for(true, true);
    assert!(supported.control_enabled);
    assert!(supported.claims_protection);
    assert_eq!(supported.primitive, Some(SHIELD_PRIMITIVE));
    assert_eq!(supported.disclosure, Some(SHIELD_DISCLOSURE));
    assert_eq!(supported.unavailable_copy, None);

    let temp = TempDir::new().expect("temp");
    let (mut client, _) =
        StoryPrivacyClient::open_with_shield_support(temp.path(), KEY, T0, false).expect("open");
    assert_eq!(
        client.set_screenshot_shield(true).unwrap_err(),
        SHIELD_UNAVAILABLE_COPY
    );
    assert!(!client.defaults().shield.setting_on);
    drop(client);

    // A profile that turned the shield on where it works must not carry the
    // claim to a machine where it does not.
    let (mut client, _) =
        StoryPrivacyClient::open_with_shield_support(temp.path(), KEY, T0, true).expect("open");
    assert!(client.set_screenshot_shield(true).expect("shield on").claims_protection);
    drop(client);
    let (client, _) =
        StoryPrivacyClient::open_with_shield_support(temp.path(), KEY, T0 + 1, false)
            .expect("reopen elsewhere");
    assert!(!client.defaults().shield.setting_on);
    assert!(!client.defaults().shield.claims_protection);
    assert_eq!(
        client.defaults().shield.unavailable_copy,
        Some(SHIELD_UNAVAILABLE_COPY)
    );
}

#[test]
fn the_real_platform_answer_decides_what_may_be_claimed_here() {
    let native = story_privacy::shield_state(true);
    assert_eq!(native.supported, cfg!(windows));
    if !cfg!(windows) {
        assert!(!native.claims_protection);
        assert_eq!(native.unavailable_copy, Some(SHIELD_UNAVAILABLE_COPY));
        let application = story_privacy::apply_shield_to_window(0, true);
        assert!(!application.enforced);
        assert!(!application.claims_protection);
        assert!(application.error.is_some());
    }
}
