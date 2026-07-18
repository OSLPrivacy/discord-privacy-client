//! Explicit live canary for the production keyserver.
//!
//! This test is ignored by default and requires `OSL_LIVE_KEYSERVER_URL` so a
//! normal test run can never create remote state. It uses fresh opaque test
//! identities and unregisters both identities even when the protocol probe
//! fails.

use keystore::{
    generate_identity, BurnScope, KeyServerClient, PrekeyConfig, PrekeyState, WrappedKeyUpload,
};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
#[ignore = "mutates the explicitly configured live test keyserver"]
fn signed_mutation_round_trip_and_cleanup() {
    let base_url = std::env::var("OSL_LIVE_KEYSERVER_URL")
        .expect("set OSL_LIVE_KEYSERVER_URL to run the live canary");
    let client = KeyServerClient::new(&base_url).expect("build live keyserver client");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let sender = generate_identity(format!("canary-sender-{nonce}"));
    let recipient = generate_identity(format!("canary-recipient-{nonce}"));
    let bounded_expiry = (SystemTime::now() + std::time::Duration::from_secs(6 * 24 * 60 * 60))
        .duration_since(UNIX_EPOCH)
        .expect("expiry after epoch")
        .as_secs();
    let bounded_expiry = chrono::DateTime::from_timestamp(bounded_expiry as i64, 0)
        .expect("valid bounded expiry")
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    let probe = (|| -> keystore::Result<()> {
        client.register(&sender)?;
        client.register(&recipient)?;

        let state = PrekeyState::new(&sender, PrekeyConfig::default(), 1_700_000_000);
        client.replenish_prekeys(&sender, Some(&state.current_spk), &state.opk_pool)?;
        let bundle = client.fetch_prekey_bundle(&recipient, &sender.user_id)?;
        assert_eq!(bundle.user_id, sender.user_id);
        assert_eq!(bundle.remaining_opk_count, 99);

        let reusable_id = format!("canary-reusable-{nonce}");
        client.post_wrapped_key(
            &sender,
            &WrappedKeyUpload {
                content_id: reusable_id.clone(),
                content_type: "text".into(),
                system_message_kind: None,
                recipient_id: recipient.user_id.clone(),
                session_version: 1,
                share_index: 0,
                wrapped_share_blob: "AQIDBA==".into(),
                blob_version: 1,
                single_use: false,
                display_duration_seconds: None,
                expires_at: bounded_expiry.clone(),
            },
        )?;

        let single_use_id = format!("canary-once-{nonce}");
        client.post_wrapped_key(
            &sender,
            &WrappedKeyUpload {
                content_id: single_use_id.clone(),
                content_type: "text".into(),
                system_message_kind: None,
                recipient_id: recipient.user_id.clone(),
                session_version: 1,
                share_index: 0,
                wrapped_share_blob: "BQYHCA==".into(),
                blob_version: 1,
                single_use: true,
                display_duration_seconds: Some(10),
                expires_at: bounded_expiry,
            },
        )?;
        let once = client.fetch_wrapped_key(&recipient, &single_use_id)?;
        assert_eq!(once.content_id, single_use_id);
        assert!(client
            .fetch_wrapped_key(&recipient, &single_use_id)
            .is_err());

        let burned = client.burn(
            &sender,
            &BurnScope::Single {
                content_id: reusable_id,
            },
        )?;
        assert_eq!(burned.deleted_count, 1);
        Ok(())
    })();

    let sender_cleanup = client.unregister(&sender);
    let recipient_cleanup = client.unregister(&recipient);
    probe.expect("live signed protocol canary failed");
    sender_cleanup.expect("sender canary cleanup failed");
    recipient_cleanup.expect("recipient canary cleanup failed");
}
