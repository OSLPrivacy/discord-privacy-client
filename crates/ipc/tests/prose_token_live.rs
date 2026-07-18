//! Live end-to-end integration test for the Phase 2 prose-token
//! pipeline. Hits the production cipher-store at
//! ciphers.oslprivacy.com — REQUIRES network access. Skipped unless
//! `OSL_LIVE_TESTS=1` so plain `cargo test` doesn't break offline.
//!
//! Run with:
//!   OSL_LIVE_TESTS=1 cargo test -p ipc --test prose_token_live -- --nocapture

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use ipc::prose_token::{prose_token_burn_id, prose_token_recv, prose_token_send};
use ipc::scope::{ScopeInput, ScopeKind};

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

    let sent = prose_token_send(&dir, &scope, &wire, 86400).expect("prose_token_send");
    println!("[send] blob_id = {}", sent.blob_id);
    println!("[send] cover_text = {}", sent.cover_text);
    println!("[send] expires_at = {}", sent.expires_at);
    assert_eq!(sent.blob_id.len(), 16);
    assert!(!sent.cover_text.starts_with("DPC"));

    let recv = prose_token_recv(&dir, &scope, &sent.cover_text)
        .expect("prose_token_recv ok")
        .expect("prose_token_recv saw a valid token");
    println!("[recv] wire = {}", recv.wire);
    println!("[recv] blob_id = {}", recv.blob_id);
    assert_eq!(recv.blob_id, sent.blob_id);
    assert_eq!(recv.wire, wire);

    // Burn — second call should be idempotent.
    prose_token_burn_id(&dir, &scope, &sent.blob_id).expect("burn succeeds");
    prose_token_burn_id(&dir, &scope, &sent.blob_id).expect("burn idempotent");

    // After burn, recv should map to None (server returns 404 →
    // prose_token_recv folds that to Ok(None)).
    let after_burn = prose_token_recv(&dir, &scope, &sent.cover_text).expect("recv ok");
    assert!(
        after_burn.is_none(),
        "expected None after burn, got {after_burn:?}"
    );
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
    let result = prose_token_recv(&dir, &scope, msg).expect("recv ok");
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
    let sent = prose_token_send(&dir, &scope_a, &wire, 86400).expect("send");
    // Decoding under scope_b with the same cover should NOT recover
    // the token (different cipher permutation + different MAC key).
    let recv_b = prose_token_recv(&dir, &scope_b, &sent.cover_text).expect("recv ok");
    assert!(recv_b.is_none(), "cross-scope decode must return None");
    // Cleanup.
    let _ = prose_token_burn_id(&dir, &scope_a, &sent.blob_id);
}
