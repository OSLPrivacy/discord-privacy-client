//! Phase 9-A1b keyserver ML-KEM populate tests.
//!
//! Tests `populate_peer_from_fetch_response` directly — the pure
//! function that mutates peer_map from a keyserver `PubkeysResponse`.
//! Driving via mock HTTP would require a test-only fake of
//! `KeyServerClient` (currently a concrete reqwest-backed struct
//! with no trait abstraction), so we keep the unit test surface
//! synchronous and assert peer_map mutations directly. The
//! refresh-on-error retry inside `cmd_osl_encrypt_message_v2` is
//! covered by the manual acceptance criteria in the phase spec.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use crypto::{ml_kem_768, x25519};
use ipc::commands::populate_peer_from_fetch_response;
use ipc::state::AppState;
use keystore::client::PubkeysResponse;
use keystore::generate_identity;

const PEER_DID: &str = "900000000000000001";

fn fresh_state() -> AppState {
    let state = AppState::new();
    *state.identity.lock().unwrap() = Some(generate_identity("liam".to_string()));
    state
}

fn fixture_response(x25519_b64: &str, mlkem_b64: &str) -> PubkeysResponse {
    let identity = generate_identity("peer".to_string());
    let ed25519_b64 = STANDARD.encode(identity.ed25519_public.as_bytes());
    let msg =
        keystore::client::reg_msg(&identity.user_id, x25519_b64, &ed25519_b64, mlkem_b64, None);
    let sig = crypto::ed25519::sign(&identity.ed25519_secret, &msg);

    PubkeysResponse {
        user_id: identity.user_id.clone(),
        ik_x25519_pub: x25519_b64.to_string(),
        ik_ed25519_pub: ed25519_b64,
        ik_mlkem768_pub: mlkem_b64.to_string(),
        registered_at: "2026-05-15T00:00:00Z".to_string(),
        last_rotated_at: None,
        ik_ratchet_initial_pub: None,
        rn_capabilities: None,
        registration_sig: Some(STANDARD.encode(sig.as_bytes())),
        identity_scheme: None,
        identity_bundle_version: None,
        identity_revision: None,
        ik_root_ed25519_pub: None,
        identity_bundle_proof_sig: None,
    }
}

#[test]
fn fixture_response_carries_valid_registration_proof() {
    let (_x_sk, x_pk) = x25519::generate_keypair();
    let (_m_sk, m_pk) = ml_kem_768::generate_keypair();
    let mut resp = fixture_response(
        &STANDARD.encode(x_pk.as_bytes()),
        &STANDARD.encode(m_pk.to_bytes()),
    );

    assert!(keystore::client::verify_peer_bundle(&resp));
    assert_eq!(resp.identity_scheme, None);
    assert_eq!(resp.identity_bundle_version, None);
    assert_eq!(resp.identity_revision, None);
    assert_eq!(resp.ik_root_ed25519_pub, None);
    assert_eq!(resp.identity_bundle_proof_sig, None);

    resp.ik_x25519_pub = STANDARD.encode([0x42u8; 32]);
    assert!(!keystore::client::verify_peer_bundle(&resp));
}

#[test]
fn populate_writes_x25519_and_mlkem_to_peer_map() {
    let state = fresh_state();
    let (_x_sk, x_pk) = x25519::generate_keypair();
    let (_m_sk, m_pk) = ml_kem_768::generate_keypair();
    let resp = fixture_response(
        &STANDARD.encode(x_pk.as_bytes()),
        &STANDARD.encode(m_pk.to_bytes()),
    );
    let added = populate_peer_from_fetch_response(&state, PEER_DID, &resp).unwrap();
    assert!(added, "ML-KEM was newly added (entry didn't exist)");
    let pm = state.peer_map.lock().unwrap();
    let entry = pm.get(PEER_DID).expect("entry created");
    assert_eq!(
        entry.pubkey.as_deref(),
        Some(STANDARD.encode(x_pk.as_bytes()).as_str())
    );
    assert_eq!(
        entry.ik_mlkem768_pub.as_deref(),
        Some(STANDARD.encode(m_pk.to_bytes()).as_str())
    );
    assert_eq!(entry.discord_id.as_deref(), Some(PEER_DID));
}

#[test]
fn populate_returns_false_when_mlkem_already_present() {
    let state = fresh_state();
    let (_x_sk, x_pk) = x25519::generate_keypair();
    let (_m_sk, m_pk) = ml_kem_768::generate_keypair();
    let resp = fixture_response(
        &STANDARD.encode(x_pk.as_bytes()),
        &STANDARD.encode(m_pk.to_bytes()),
    );
    // First populate sets it; second call should report not-newly-added.
    populate_peer_from_fetch_response(&state, PEER_DID, &resp).unwrap();
    let added = populate_peer_from_fetch_response(&state, PEER_DID, &resp).unwrap();
    assert!(!added, "second populate reports false (no change)");
}

#[test]
fn populate_rejects_empty_mlkem_without_writing_x25519() {
    let state = fresh_state();
    let (_x_sk, x_pk) = x25519::generate_keypair();
    let resp = fixture_response(&STANDARD.encode(x_pk.as_bytes()), "");
    let err = populate_peer_from_fetch_response(&state, PEER_DID, &resp).unwrap_err();
    assert!(
        err.contains("keyserver bundle incomplete"),
        "expected incomplete-bundle error, got: {err}"
    );
    let pm = state.peer_map.lock().unwrap();
    assert!(
        pm.get(PEER_DID).is_none(),
        "incomplete bundle must not create a peer entry"
    );
}

#[test]
fn populate_rejects_wrong_length_mlkem() {
    let state = fresh_state();
    let (_x_sk, x_pk) = x25519::generate_keypair();
    // 1183 bytes — one short of ENCAPSULATION_KEY_SIZE (1184).
    let bad_mlkem = vec![0u8; 1183];
    let resp = fixture_response(
        &STANDARD.encode(x_pk.as_bytes()),
        &STANDARD.encode(&bad_mlkem),
    );
    let err = populate_peer_from_fetch_response(&state, PEER_DID, &resp).unwrap_err();
    assert!(
        err.contains("wrong length"),
        "expected length error, got: {err}"
    );
    // Peer entry must NOT be partially mutated when validation fails.
    let pm = state.peer_map.lock().unwrap();
    assert!(
        pm.get(PEER_DID).is_none(),
        "validation failure rolls back peer creation"
    );
}

#[test]
fn populate_rejects_wrong_length_x25519() {
    let state = fresh_state();
    // 33 bytes — one over PUBLIC_KEY_SIZE (32).
    let bad_x = vec![0u8; 33];
    let (_m_sk, m_pk) = ml_kem_768::generate_keypair();
    let resp = fixture_response(&STANDARD.encode(&bad_x), &STANDARD.encode(m_pk.to_bytes()));
    let err = populate_peer_from_fetch_response(&state, PEER_DID, &resp).unwrap_err();
    assert!(
        err.contains("X25519 pubkey") && err.contains("wrong length"),
        "expected X25519 length error, got: {err}"
    );
}

#[test]
fn populate_rejects_empty_x25519() {
    let state = fresh_state();
    let (_m_sk, m_pk) = ml_kem_768::generate_keypair();
    let resp = fixture_response("", &STANDARD.encode(m_pk.to_bytes()));
    let err = populate_peer_from_fetch_response(&state, PEER_DID, &resp).unwrap_err();
    assert!(
        err.contains("keyserver bundle incomplete"),
        "expected incomplete-bundle error, got: {err}"
    );
}

#[test]
fn populate_refreshes_existing_entry_with_ml_kem() {
    // Simulate the legacy-entry-upgrade flow: a peer already exists
    // in peer_map with only X25519 (pre-9-A1 schema). After A1b's
    // keyserver refresh, ML-KEM should be added.
    let state = fresh_state();
    let (_x_sk, x_pk) = x25519::generate_keypair();
    let (_m_sk, m_pk) = ml_kem_768::generate_keypair();
    {
        let mut pm = state.peer_map.lock().unwrap();
        let entry = pm.entry(PEER_DID.to_string()).or_default();
        entry.pubkey = Some(STANDARD.encode(x_pk.as_bytes()));
        entry.discord_id = Some(PEER_DID.to_string());
        // Note: no ik_mlkem768_pub set.
    }
    let resp = fixture_response(
        &STANDARD.encode(x_pk.as_bytes()),
        &STANDARD.encode(m_pk.to_bytes()),
    );
    let added = populate_peer_from_fetch_response(&state, PEER_DID, &resp).unwrap();
    assert!(added, "legacy entry got newly-added ML-KEM");
    let pm = state.peer_map.lock().unwrap();
    assert!(pm.get(PEER_DID).unwrap().ik_mlkem768_pub.is_some());
}
