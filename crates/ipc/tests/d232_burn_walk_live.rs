//! D-232 — the sender's own burn destroys nothing on the ordinary path.
//!
//! This is the reproduction the defect record was missing. D-232 was filed as
//! "burn id 16 vs 32 hex", which reads as a parser nit. It is not. The hub's
//! panic burn (`apps/osl-hub/src/security.rs` `burn_scope`) and its app+account
//! burn walk the ids recorded by `record_peer_prose_blob` and count a blob as
//! destroyed exactly when `prose_token_burn_id(..).is_ok()`. The B0-01 bridge
//! records the deployed Worker's 16-hex server-assigned id, and
//! `prose_token_burn_id` parses through `blob_id_hex_to_bytes`, which is sized
//! by `CAPABILITY_BYTES` (32 hex). So every recorded id fails before a single
//! packet leaves the machine, `remote_blobs_deleted` is always 0, and burn is a
//! no-op for every message the product actually creates.
//!
//! The assertions below are product assertions, not parser assertions: the
//! sender must be able to destroy the object it just created, and a later
//! receive must observe that it is gone. They do not name a width, so they stay
//! honest whichever width the protocol settles on.
//!
//! REQUIRES network access — hits the real store at ciphers.oslprivacy.com.
//! Skipped unless `OSL_LIVE_TESTS=1`, exactly like `prose_token_live.rs`.
//!
//! Run with:
//!   OSL_LIVE_TESTS=1 cargo test -p ipc --test d232_burn_walk_live -- --nocapture

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use ipc::prose_token::{
    prose_token_recv_classified, prose_token_send, ProseTokenMiss, ProseTokenRecv,
    ProseTokenSendKeys,
};
use ipc::scope::{ScopeInput, ScopeKind};
use ipc::scope_blobs_file::RecordedBlob;

const MESSAGE_KEY: [u8; 32] = [0x11; 32];
const SEND_KEY: [u8; 32] = [0x22; 32];
const CONVERSATION_KEY: [u8; 32] = [0x33; 32];

fn send_keys() -> ProseTokenSendKeys<'static> {
    ProseTokenSendKeys {
        message_key: &MESSAGE_KEY,
        send_key: &SEND_KEY,
        conversation_key: &CONVERSATION_KEY,
    }
}

fn live_tests_enabled() -> bool {
    std::env::var("OSL_LIVE_TESTS").ok().as_deref() == Some("1")
}

fn config_dir() -> std::path::PathBuf {
    tempfile::tempdir().unwrap().into_path()
}

fn dm_scope() -> ScopeInput {
    ScopeInput {
        kind: ScopeKind::Dm,
        id: "999000111222333555".to_string(),
        server_id: None,
        channel_id: Some("dm:d232-burn-walk".to_string()),
    }
}

fn detection_key() -> [u8; 32] {
    ipc::prose_token::derive_detection_key(&[0x55; 32]).expect("test secret derives detector")
}

/// The hub's burn walk, transcribed. `security.rs::burn_recorded_prose_blobs`
/// counts a blob destroyed exactly when the burn call returns `Ok`, and
/// re-records every other entry into `failed_blob_ids`. Nothing here is a
/// stand-in: this is the shipping accounting, run against the shipping burn
/// entry point, over exactly what the shipping ledger stores.
fn burn_walk(dir: &std::path::Path, recorded: Vec<RecordedBlob>) -> (usize, Vec<(String, String)>) {
    let mut remote_blobs_deleted = 0usize;
    let mut failed_blob_ids = Vec::new();
    for entry in recorded {
        match ipc::prose_token::prose_token_burn_recorded(
            dir,
            &SEND_KEY,
            &entry.blob_id,
            entry.burn_capability.as_deref(),
        ) {
            Ok(()) => remote_blobs_deleted += 1,
            Err(error) => failed_blob_ids.push((entry.blob_id, error.to_string())),
        }
    }
    (remote_blobs_deleted, failed_blob_ids)
}

/// The panic button must actually destroy the sender's own objects.
///
/// Send one message the way production sends it, hand the recorded id to the
/// walk the burn button runs, and require that the walk reports the deletion it
/// performed and that the object is really gone afterwards.
#[test]
fn the_burn_walk_destroys_a_blob_the_ordinary_send_path_created() {
    if !live_tests_enabled() {
        eprintln!("skipping: set OSL_LIVE_TESTS=1 to run against the live cipher store");
        return;
    }
    let dir = config_dir();
    let scope = dm_scope();
    let key = detection_key();
    let wire = format!("DPC0::{}", B64.encode(b"d232-burn-walk-marker"));

    let sent = prose_token_send(&dir, &scope, &key, send_keys(), &wire, 3600)
        .expect("the ordinary bridge send path uploads");

    // Exactly what `record_peer_prose_blob` stores, and the only thing the walk
    // is given.
    let recorded = vec![RecordedBlob {
        blob_id: sent.blob_id.clone(),
        burn_capability: sent.burn_capability.clone(),
    }];

    let (remote_blobs_deleted, failed) = burn_walk(&dir, recorded);

    assert!(
        failed.is_empty(),
        "the burn walk refused the id its own send path recorded: {failed:?}"
    );
    assert_eq!(
        remote_blobs_deleted, 1,
        "burn reported {remote_blobs_deleted} remote deletions for 1 recorded blob"
    );

    // Accounting is not destruction. Prove the object is gone by reading the
    // cover again: a clean 404 for a real pointer is `BlobGone`.
    match prose_token_recv_classified(&dir, &scope, &key, &sent.cover_text)
        .expect("recv after burn does not error")
    {
        ProseTokenRecv::Missed(ProseTokenMiss::BlobGone) => {}
        ProseTokenRecv::Missed(ProseTokenMiss::NoToken) => {
            panic!("the cover stopped decoding, which is not evidence the blob was destroyed")
        }
        ProseTokenRecv::Recovered(_) => {
            panic!("burn reported success and the ciphertext is still retrievable")
        }
    }
}
