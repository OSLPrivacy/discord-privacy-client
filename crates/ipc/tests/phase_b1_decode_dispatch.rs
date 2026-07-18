//! Phase 9-B1 Task 6: decode-side Mode 1 dispatch tests.
//!
//! End-to-end: build a Mode 1 send via the post-9-B1
//! `cmd_osl_encrypt_message_v2` (with `stego_mode = Mode1`), then
//! feed each cover string to `cmd_osl_decrypt_message_v2` in turn.
//! The receiver should emit `__OSL_CONTROL_MODE1_INCOMPLETE__|...`
//! for every chunk but the last, and then return the recovered
//! plaintext after the final chunk completes reassembly.
//!
//! Negative paths exercise:
//! - tampered cover bytes → `__OSL_CONTROL_MODE1_INVALID__`
//! - conflicting `total_chunks` → `__OSL_CONTROL_MODE1_CONFLICT__`
//! - Mode 1 without `scope_input` → error
//! - Mode 0 wire still works (legacy path untouched)

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use ipc::app_preferences::StegoMode;
use ipc::commands::{
    cmd_osl_decrypt_message_v2, cmd_osl_encrypt_message_v2, OSL_RESULT_MODE1_CONFLICT,
    OSL_RESULT_MODE1_INCOMPLETE_PREFIX, OSL_RESULT_MODE1_INVALID,
};
use ipc::peer_map::WhitelistEntry;
use ipc::scope::{Scope, ScopeInput};
use ipc::state::AppState;
use ipc::whitelist_state::ScopeState;
use keystore::{generate_identity, Identity};

const LIAM_DID: &str = "900000000000000003";
const HENRY_DID: &str = "900000000000000001";
const DM_CHANNEL_ID: &str = "5550000000000000001";

fn fresh_state_with_identity(name: &str, self_did: &str) -> AppState {
    let s = AppState::new();
    *s.identity.lock().unwrap() = Some(generate_identity(name.into()));
    let mut pm = s.peer_map.lock().unwrap();
    let pe = pm.entry(self_did.to_string()).or_default();
    pe.is_self = Some(true);
    pe.discord_id = Some(self_did.to_string());
    drop(pm);
    s
}

/// Install peer with X25519 + ML-KEM but *no* ratchet pub. This keeps
/// the sender path on v=3 (PQ-hybrid wrap, recipient-list framing) so
/// self-decrypt works without the full A2 cross-AppState ratchet
/// bootstrap. Mode 1 dispatch is a layer above the wire version
/// negotiation, so v=3 is sufficient to exercise it here.
fn install_peer_full(state: &AppState, peer_did: &str, peer_id: &Identity) {
    let mut pm = state.peer_map.lock().unwrap();
    let pe = pm.entry(peer_did.to_string()).or_default();
    pe.pubkey = Some(STANDARD.encode(peer_id.x25519_public.as_bytes()));
    pe.ik_mlkem768_pub = Some(STANDARD.encode(peer_id.mlkem_public_bytes));
    pe.discord_id = Some(peer_did.to_string());
}

fn install_dm_scope(state: &AppState, peer_did: &str) {
    let scope = Scope::dm(peer_did);
    let mut ws = state.whitelist_state.lock().unwrap();
    ws.insert(
        scope.storage_key(),
        ScopeState {
            encrypt_toggle: true,
            auto_enabled: true,
            ..ScopeState::default()
        },
    );
    drop(ws);
    let mut pm = state.peer_map.lock().unwrap();
    let pe = pm.entry(peer_did.to_string()).or_default();
    pe.outgoing_whitelists.push(WhitelistEntry::Dm {
        broadened: false,
        enabled_at: None,
    });
}

fn mark_sender_accepted(state: &AppState, sender_did: &str, scope: &Scope) {
    // 9-C1: handshake gate removed; this helper is a no-op kept
    // for call-site stability. Permissive decrypt means no sender-accept
    // state needs to exist.
    let _ = (state, sender_did, scope);
}

/// Build a sender and self-receiver pair so the same AppState can
/// both encrypt and decrypt — same identity acts as both ends.
/// Avoids the cross-state ratchet bootstrap setup the A2 integration
/// tests already cover.
fn self_loop_state() -> AppState {
    let state = fresh_state_with_identity("liam", LIAM_DID);
    let henry_id = generate_identity("henry".into());
    install_peer_full(&state, HENRY_DID, &henry_id);
    install_dm_scope(&state, HENRY_DID);
    // For self-decrypt the scope acceptance is satisfied via the
    // self-sender bypass — we still mark for completeness.
    let scope = Scope::dm(HENRY_DID);
    mark_sender_accepted(&state, LIAM_DID, &scope);
    state
}

fn set_mode1(state: &AppState) {
    let mut p = state.app_preferences.lock().unwrap();
    p.stego_mode = StegoMode::Mode1;
}

fn decode(state: &AppState, content: &str) -> Result<String, String> {
    cmd_osl_decrypt_message_v2(
        state,
        None,
        DM_CHANNEL_ID.into(),
        LIAM_DID.into(),
        content.into(),
        Some(ScopeInput::from(&Scope::dm(HENRY_DID))),
        None,
    )
}

#[test]
fn mode0_wire_still_decodes_directly() {
    // Sanity: when stego_mode=Mode0 the cover IS the wire string,
    // and the receiver should round-trip it untouched.
    let state = self_loop_state();
    // stego_mode defaults to Mode 0.
    let out = cmd_osl_encrypt_message_v2(
        &state,
        "mode0 hello".into(),
        ScopeInput::from(&Scope::dm(HENRY_DID)),
        vec![HENRY_DID.into()],
        LIAM_DID.into(),
    )
    .unwrap();
    assert_eq!(out.messages.len(), 1);
    assert!(out.messages[0].starts_with("DPC0::"));
    let plain = decode(&state, &out.messages[0]).unwrap();
    assert_eq!(plain, "mode0 hello");
}

#[test]
#[ignore = "Mode 1 disabled in V2; V3 will re-enable"]
fn mode1_single_chunk_completes_on_first_cover() {
    // For a tiny plaintext the wire payload may still chunk to >1
    // pieces because of the v=3 header (35B) + body. Confirm the
    // first chunk either completes (1-chunk session) or surfaces
    // the incomplete sentinel.
    let state = self_loop_state();
    set_mode1(&state);
    let out = cmd_osl_encrypt_message_v2(
        &state,
        "hi".into(),
        ScopeInput::from(&Scope::dm(HENRY_DID)),
        vec![HENRY_DID.into()],
        LIAM_DID.into(),
    )
    .unwrap();
    assert!(!out.messages.is_empty());

    let mut last: Option<String> = None;
    for (i, cover) in out.messages.iter().enumerate() {
        let r = decode(&state, cover).unwrap();
        if i + 1 < out.messages.len() {
            assert!(
                r.starts_with(OSL_RESULT_MODE1_INCOMPLETE_PREFIX),
                "mid-stream chunk {i} should emit incomplete sentinel, got: {r}"
            );
        } else {
            last = Some(r);
        }
    }
    let plaintext = last.expect("at least one cover present");
    assert_eq!(plaintext, "hi");
}

#[test]
#[ignore = "Mode 1 disabled in V2; V3 will re-enable"]
fn mode1_multi_chunk_full_roundtrip() {
    // Plaintext large enough to definitely span multiple chunks.
    let state = self_loop_state();
    set_mode1(&state);
    let plaintext: String = "0123456789abcdef".repeat(80); // ~1280 chars
    let out = cmd_osl_encrypt_message_v2(
        &state,
        plaintext.clone(),
        ScopeInput::from(&Scope::dm(HENRY_DID)),
        vec![HENRY_DID.into()],
        LIAM_DID.into(),
    )
    .unwrap();
    assert!(
        out.messages.len() > 1,
        "long plaintext should chunk to multiple covers"
    );
    let mut last: Option<String> = None;
    for (i, cover) in out.messages.iter().enumerate() {
        let r = decode(&state, cover).unwrap();
        if i + 1 < out.messages.len() {
            assert!(r.starts_with(OSL_RESULT_MODE1_INCOMPLETE_PREFIX));
        } else {
            last = Some(r);
        }
    }
    assert_eq!(last.unwrap(), plaintext);
}

#[test]
#[ignore = "Mode 1 disabled in V2; V3 will re-enable"]
fn mode1_tampered_cover_returns_invalid_sentinel() {
    let state = self_loop_state();
    set_mode1(&state);
    let out = cmd_osl_encrypt_message_v2(
        &state,
        "tamper me".into(),
        ScopeInput::from(&Scope::dm(HENRY_DID)),
        vec![HENRY_DID.into()],
        LIAM_DID.into(),
    )
    .unwrap();
    // Truncate the first cover to break decode_mode1 or chunk parse.
    let mut tampered = out.messages[0].clone();
    tampered.push_str(" Today loud saw a apple.");
    let r = decode(&state, &tampered).unwrap();
    assert_eq!(r, OSL_RESULT_MODE1_INVALID);
}

#[test]
#[ignore = "Mode 1 disabled in V2; V3 will re-enable"]
fn mode1_conflicting_total_returns_conflict_sentinel() {
    // Push one chunk under session S with total=3, then a chunk
    // under session S with total=5. The reassembly buffer drops
    // the session and the dispatcher surfaces the conflict.
    let state = self_loop_state();
    set_mode1(&state);
    let plaintext: String = "x".repeat(400); // ensures multi-chunk
    let out_a = cmd_osl_encrypt_message_v2(
        &state,
        plaintext.clone(),
        ScopeInput::from(&Scope::dm(HENRY_DID)),
        vec![HENRY_DID.into()],
        LIAM_DID.into(),
    )
    .unwrap();
    let session_a = out_a.session_id.unwrap();
    let total_a = out_a.messages.len();
    assert!(total_a >= 2);

    // First chunk: pushed under session A.
    let r1 = decode(&state, &out_a.messages[0]).unwrap();
    assert!(r1.starts_with(OSL_RESULT_MODE1_INCOMPLETE_PREFIX));

    // Now hand-forge a chunk with the same session_id but a
    // different total. Build it by manipulating a freshly-chunked
    // payload's session bytes.
    let salt = Scope::dm(HENRY_DID).storage_key().into_bytes();
    let cipher = stego::ConversationCipher::from_salt(&salt);
    let bogus_wire = vec![0u8; 200]; // forces 2-chunk session
    let chunks = stego::chunk_payload(&salt, session_a, &bogus_wire);
    assert_ne!(chunks[0].total_chunks as usize, total_a);
    let bogus_cover = stego::encode_mode1(&cipher, &chunks[0].bytes).unwrap();
    let r2 = decode(&state, &bogus_cover).unwrap();
    assert_eq!(r2, OSL_RESULT_MODE1_CONFLICT);
}

#[test]
#[ignore = "Mode 1 disabled in V2; V3 will re-enable"]
fn mode1_decode_without_scope_returns_error() {
    let state = self_loop_state();
    set_mode1(&state);
    let out = cmd_osl_encrypt_message_v2(
        &state,
        "scope needed".into(),
        ScopeInput::from(&Scope::dm(HENRY_DID)),
        vec![HENRY_DID.into()],
        LIAM_DID.into(),
    )
    .unwrap();
    let err = cmd_osl_decrypt_message_v2(
        &state,
        None,
        DM_CHANNEL_ID.into(),
        LIAM_DID.into(),
        out.messages[0].clone(),
        None, // no scope_input — Mode 1 decode needs it
        None,
    )
    .unwrap_err();
    assert!(
        err.contains("Mode 1 decode needs scope_input"),
        "expected scope-required error, got: {err}"
    );
}

#[test]
#[ignore = "Mode 1 disabled in V2; V3 will re-enable"]
fn mode1_session_isolated_per_channel() {
    // Two different channel_ids → each has its own reassembly
    // buffer, so the same session_id stays isolated.
    let state = self_loop_state();
    set_mode1(&state);
    let plaintext: String = "y".repeat(400);
    let out = cmd_osl_encrypt_message_v2(
        &state,
        plaintext.clone(),
        ScopeInput::from(&Scope::dm(HENRY_DID)),
        vec![HENRY_DID.into()],
        LIAM_DID.into(),
    )
    .unwrap();
    assert!(out.messages.len() >= 2);

    // First cover goes to channel A.
    let r1 = cmd_osl_decrypt_message_v2(
        &state,
        None,
        "CHANNEL_A".into(),
        LIAM_DID.into(),
        out.messages[0].clone(),
        Some(ScopeInput::from(&Scope::dm(HENRY_DID))),
        None,
    )
    .unwrap();
    assert!(r1.starts_with(OSL_RESULT_MODE1_INCOMPLETE_PREFIX));

    // Same cover repeated on channel B is *also* the first chunk
    // there — independent buffer.
    let r2 = cmd_osl_decrypt_message_v2(
        &state,
        None,
        "CHANNEL_B".into(),
        LIAM_DID.into(),
        out.messages[0].clone(),
        Some(ScopeInput::from(&Scope::dm(HENRY_DID))),
        None,
    )
    .unwrap();
    assert!(r2.starts_with(OSL_RESULT_MODE1_INCOMPLETE_PREFIX));
}
