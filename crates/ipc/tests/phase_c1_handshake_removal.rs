//! Phase 9-C1 Stage 2: permissive decrypt + legacy handshake suppression.
//!
//! Pre-C1, decrypting a content message required prior in-band
//! handshake acceptance (`should_decrypt_from` walked the
//! `incoming_decrypt_accepted` map). C1 removes the gate: if we
//! have the keys, we decrypt. The wire bytes for the now-removed
//! invitation (0x02) + response (0x03) message types still surface
//! on the wire from pre-C1 peers; the dispatcher silently swallows
//! them and returns `OSL_RESULT_LEGACY_HANDSHAKE_IGNORED` so JS can
//! drop them rather than render an error placeholder.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use crypto::x25519;
use ipc::commands::{cmd_osl_decrypt_message_v2, OSL_RESULT_LEGACY_HANDSHAKE_IGNORED};
use ipc::state::AppState;
use ipc::wire_v2::{encrypt_v2, MSG_TYPE_CONTENT};
use keystore::generate_identity;

const HENRY_DID: &str = "1502770642930634812";

fn fresh_state_with_self_identity() -> AppState {
    let state = AppState::new();
    *state.identity.lock().unwrap() = Some(generate_identity("liam".to_string()));
    state
}

fn install_peer_x25519(state: &AppState, did: &str, pk: x25519::PublicKey) {
    let mut pm = state.peer_map.lock().unwrap();
    let pe = pm.entry(did.to_string()).or_default();
    pe.pubkey = Some(STANDARD.encode(pk.as_bytes()));
    pe.discord_id = Some(did.to_string());
}

fn self_x25519_pub(state: &AppState) -> x25519::PublicKey {
    state
        .identity
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .x25519_public
}

#[test]
fn v2_content_decrypts_without_prior_accept_state() {
    // Permissive-decrypt: no accept gate. Henry's v=2 CONTENT
    // wire decrypts on liam's side even though liam has never
    // exchanged any handshake with him.
    let state = fresh_state_with_self_identity();
    let (henry_sk, henry_pk) = x25519::generate_keypair();
    install_peer_x25519(&state, HENRY_DID, henry_pk);

    let liam_pub = self_x25519_pub(&state);
    let wire = encrypt_v2(
        b"permissive decrypt works",
        &[liam_pub],
        MSG_TYPE_CONTENT,
        &henry_sk,
    )
    .unwrap();

    let plaintext = cmd_osl_decrypt_message_v2(
        &state,
        None,
        "channel".to_string(),
        HENRY_DID.to_string(),
        wire,
        None,
        None,
    )
    .expect("decrypt should succeed without prior accept-state");
    assert_eq!(plaintext, "permissive decrypt works");
}

#[test]
fn v2_legacy_invitation_msg_type_returns_sentinel() {
    // 0x02 was MSG_TYPE_WHITELIST_INVITATION pre-C1. The dispatcher
    // must swallow it (no error, no rendered ciphertext).
    let state = fresh_state_with_self_identity();
    let (henry_sk, henry_pk) = x25519::generate_keypair();
    install_peer_x25519(&state, HENRY_DID, henry_pk);

    let liam_pub = self_x25519_pub(&state);
    let wire = encrypt_v2(
        b"legacy invitation payload (ignored)",
        &[liam_pub],
        0x02, // legacy MSG_TYPE_WHITELIST_INVITATION
        &henry_sk,
    )
    .unwrap();
    let result = cmd_osl_decrypt_message_v2(
        &state,
        None,
        "channel".to_string(),
        HENRY_DID.to_string(),
        wire,
        None,
        None,
    )
    .unwrap();
    assert_eq!(result, OSL_RESULT_LEGACY_HANDSHAKE_IGNORED);
}

#[test]
fn v2_legacy_response_msg_type_returns_sentinel() {
    let state = fresh_state_with_self_identity();
    let (henry_sk, henry_pk) = x25519::generate_keypair();
    install_peer_x25519(&state, HENRY_DID, henry_pk);

    let liam_pub = self_x25519_pub(&state);
    let wire = encrypt_v2(
        b"legacy response payload (ignored)",
        &[liam_pub],
        0x03, // legacy MSG_TYPE_WHITELIST_RESPONSE
        &henry_sk,
    )
    .unwrap();
    let result = cmd_osl_decrypt_message_v2(
        &state,
        None,
        "channel".to_string(),
        HENRY_DID.to_string(),
        wire,
        None,
        None,
    )
    .unwrap();
    assert_eq!(result, OSL_RESULT_LEGACY_HANDSHAKE_IGNORED);
}
