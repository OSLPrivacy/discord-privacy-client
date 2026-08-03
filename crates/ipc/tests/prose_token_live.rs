//! Live end-to-end integration test for the Phase 2 prose-token
//! pipeline. Hits the production cipher-store at
//! ciphers.oslprivacy.com — REQUIRES network access. Skipped unless
//! `OSL_LIVE_TESTS=1` so plain `cargo test` doesn't break offline.
//!
//! Run with:
//!   OSL_LIVE_TESTS=1 cargo test -p ipc --test prose_token_live -- --nocapture

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use ipc::prose_token::{
    prose_token_burn_id, prose_token_recv, prose_token_send, ProseTokenSendKeys,
};
use ipc::scope::{ScopeInput, ScopeKind};

/// Stand-ins for the three roots the shipping sender threads in. They are kept
/// distinct here for the same reason they are distinct in production: a send
/// key the recipient could also derive would hand it burn authority.
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
    // Empty config dir — `resolve_cipher_store_base_url` falls
    // through to the built-in default ciphers.oslprivacy.com.
    tempfile::tempdir().unwrap().into_path()
}

fn dm_scope() -> ScopeInput {
    ScopeInput {
        kind: ScopeKind::Dm,
        id: "999000111222333444".to_string(),
        server_id: None,
        channel_id: None,
    }
}

fn fake_wire(payload: &[u8]) -> String {
    format!("DPC0::{}", B64.encode(payload))
}

fn detection_key() -> [u8; 32] {
    ipc::prose_token::derive_detection_key(&[0x44; 32]).expect("test secret derives detector")
}

#[test]
fn end_to_end_round_trip_via_live_store() {
    if !live_tests_enabled() {
        eprintln!("skipping live test (set OSL_LIVE_TESTS=1 to run)");
        return;
    }
    let dir = config_dir();
    let scope = dm_scope();

    let payload: Vec<u8> = (0u8..=200).collect();
    let wire = fake_wire(&payload);
    println!("[send] wire = {wire}");

    let key = detection_key();
    let sent =
        prose_token_send(&dir, &scope, &key, send_keys(), &wire, 86400).expect("prose_token_send");
    println!("[send] blob_id = {}", sent.blob_id);
    println!("[send] cover_text = {}", sent.cover_text);
    println!("[send] expires_at = {}", sent.expires_at);
    // The id is the 128-bit value this client derived from its own pointer,
    // in the canonical lowercase form the store indexes on. It was 16 hex
    // chars while the store assigned ids itself; that shape is now
    // unreachable, and a store that answered with one would be refused.
    assert_eq!(sent.blob_id.len(), 32);
    assert!(sent
        .blob_id
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
    assert!(!sent.cover_text.starts_with("DPC"));

    let recv = prose_token_recv(&dir, &scope, &key, &sent.cover_text)
        .expect("prose_token_recv ok")
        .expect("prose_token_recv saw a valid token");
    println!("[recv] wire = {}", recv.wire);
    println!("[recv] blob_id = {}", recv.blob_id);
    assert_eq!(recv.blob_id, sent.blob_id);
    assert_eq!(recv.wire, wire);

    // Burn — second call should be idempotent.
    prose_token_burn_id(&dir, &SEND_KEY, &sent.blob_id).expect("burn succeeds");
    prose_token_burn_id(&dir, &SEND_KEY, &sent.blob_id).expect("burn idempotent");

    // After burn, recv should map to None (server returns 404 →
    // prose_token_recv folds that to Ok(None)).
    let after_burn = prose_token_recv(&dir, &scope, &key, &sent.cover_text).expect("recv ok");
    assert!(
        after_burn.is_none(),
        "expected None after burn, got {after_burn:?}"
    );
}

/// Burn authority is the sender's alone. A party holding the recorded id but
/// not the send key -- which includes every recipient, since the pointer they
/// do hold derives the id -- must not be able to destroy the object.
#[test]
fn a_foreign_send_key_cannot_burn() {
    if !live_tests_enabled() {
        eprintln!("skipping live test (set OSL_LIVE_TESTS=1 to run)");
        return;
    }
    let dir = config_dir();
    let scope = dm_scope();
    let key = detection_key();
    let wire = fake_wire(b"burn-authority-payload");
    let sent =
        prose_token_send(&dir, &scope, &key, send_keys(), &wire, 86400).expect("prose_token_send");

    assert!(
        prose_token_burn_id(&dir, &[0x99; 32], &sent.blob_id).is_err(),
        "a foreign send key must not be able to burn"
    );
    let survived = prose_token_recv(&dir, &scope, &key, &sent.cover_text)
        .expect("recv ok")
        .expect("the object survived the unauthorized burn");
    assert_eq!(survived.wire, wire);

    prose_token_burn_id(&dir, &SEND_KEY, &sent.blob_id).expect("the sender's own burn succeeds");
    assert!(prose_token_recv(&dir, &scope, &key, &sent.cover_text)
        .expect("recv ok")
        .is_none());
}

#[test]
fn plain_english_is_not_a_token() {
    if !live_tests_enabled() {
        eprintln!("skipping live test (set OSL_LIVE_TESTS=1 to run)");
        return;
    }
    let dir = config_dir();
    let scope = dm_scope();
    let msg = "hey what's for lunch tomorrow?";
    let result = prose_token_recv(&dir, &scope, &detection_key(), msg).expect("recv ok");
    assert!(result.is_none(), "plain English must not decode as a token");
}

#[test]
fn cross_scope_does_not_decode() {
    if !live_tests_enabled() {
        eprintln!("skipping live test (set OSL_LIVE_TESTS=1 to run)");
        return;
    }
    let dir = config_dir();
    let scope_a = ScopeInput {
        kind: ScopeKind::Dm,
        id: "scope-a-id".to_string(),
        server_id: None,
        channel_id: None,
    };
    let scope_b = ScopeInput {
        kind: ScopeKind::Dm,
        id: "scope-b-id".to_string(),
        server_id: None,
        channel_id: None,
    };
    let payload = b"cross-scope-payload".to_vec();
    let wire = fake_wire(&payload);
    let key = detection_key();
    let sent = prose_token_send(&dir, &scope_a, &key, send_keys(), &wire, 86400).expect("send");
    // Decoding under scope_b with the same cover should NOT recover
    // the token (different cipher permutation + different MAC key).
    let recv_b = prose_token_recv(&dir, &scope_b, &key, &sent.cover_text).expect("recv ok");
    assert!(recv_b.is_none(), "cross-scope decode must return None");
    // Cleanup.
    let _ = prose_token_burn_id(&dir, &SEND_KEY, &sent.blob_id);
}
