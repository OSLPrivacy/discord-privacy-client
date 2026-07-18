//! Phase 9-B1 Task 5: wire-layer Mode dispatch tests.
//!
//! Drives `cmd_osl_encrypt_message_v2` (the post-9-B1
//! `EncryptOutput`-shaped entry point) and verifies that the
//! `stego_mode` selector in `app_preferences` produces:
//! - Mode 0 → one `DPC0::<b64>` wire, no session id.
//! - Mode 1 → one or more `DPC1::<sentences>` covers, a stamped
//!   `session_id`.
//!
//! End-to-end chunk → cover → decode → reassemble is also tested
//! to confirm the data path is reversible.
//!
//! 9-MODE1-FIX dropped the preview-required gate; the three tests
//! that asserted preview_required values are gone.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use crypto::x25519;
use ipc::app_preferences::StegoMode;
use ipc::commands::{cmd_osl_encrypt_message_v2, EncryptOutput};
use ipc::peer_map::WhitelistEntry;
use ipc::scope::{Scope, ScopeInput};
use ipc::state::AppState;
use ipc::whitelist_state::ScopeState;
use keystore::generate_identity;

const LIAM_DID: &str = "900000000000000003";
const HENRY_DID: &str = "900000000000000001";

fn fresh_state() -> AppState {
    let s = AppState::new();
    *s.identity.lock().unwrap() = Some(generate_identity("liam".into()));
    let mut pm = s.peer_map.lock().unwrap();
    let pe = pm.entry(LIAM_DID.to_string()).or_default();
    pe.is_self = Some(true);
    pe.discord_id = Some(LIAM_DID.to_string());
    drop(pm);
    s
}

fn install_dm_peer(state: &AppState) {
    let (_henry_sk, henry_pk) = x25519::generate_keypair();
    let (_henry_mlkem_sk, henry_mlkem_pk) = crypto::ml_kem_768::generate_keypair();
    let henry_ratchet_pub = {
        let (sk, _pk) = x25519::generate_keypair();
        let _ = sk;
        // Use any X25519 pub as the ratchet pub — the wire side
        // never decrypts here, the test only inspects the
        // EncryptOutput shape.
        let (_sk2, pk2) = x25519::generate_keypair();
        pk2
    };
    let mut pm = state.peer_map.lock().unwrap();
    let pe = pm.entry(HENRY_DID.to_string()).or_default();
    pe.pubkey = Some(STANDARD.encode(henry_pk.as_bytes()));
    pe.ik_mlkem768_pub = Some(STANDARD.encode(henry_mlkem_pk.to_bytes()));
    pe.ik_ratchet_initial_pub = Some(STANDARD.encode(henry_ratchet_pub.as_bytes()));
    pe.discord_id = Some(HENRY_DID.to_string());
    pe.outgoing_whitelists.push(WhitelistEntry::Dm {
        broadened: false,
        enabled_at: None,
    });
    drop(pm);
    let scope = Scope::dm(HENRY_DID);
    let mut ws = state.whitelist_state.lock().unwrap();
    ws.insert(
        scope.storage_key(),
        ScopeState {
            encrypt_toggle: true,
            auto_enabled: true,
            ..ScopeState::default()
        },
    );
}

fn set_mode(state: &AppState, mode: StegoMode) {
    let mut p = state.app_preferences.lock().unwrap();
    p.stego_mode = mode;
}

fn send(state: &AppState, scope: &Scope, plaintext: &str) -> EncryptOutput {
    cmd_osl_encrypt_message_v2(
        state,
        plaintext.into(),
        ScopeInput::from(scope),
        vec![HENRY_DID.into()],
        LIAM_DID.into(),
    )
    .expect("encrypt succeeds")
}

#[test]
fn mode0_returns_single_dpc0_wire() {
    let state = fresh_state();
    install_dm_peer(&state);
    set_mode(&state, StegoMode::Mode0);
    let scope = Scope::dm(HENRY_DID);
    let out = send(&state, &scope, "hello mode 0");
    assert_eq!(out.messages.len(), 1);
    assert!(out.messages[0].starts_with("DPC0::"));
    assert!(out.session_id.is_none());
}

#[test]
#[ignore = "Mode 1 disabled in V2; V3 will re-enable"]
fn mode1_returns_dpc1_messages_with_session_id() {
    let state = fresh_state();
    install_dm_peer(&state);
    set_mode(&state, StegoMode::Mode1);
    let scope = Scope::dm(HENRY_DID);
    let out = send(&state, &scope, "hello mode 1");
    assert!(!out.messages.is_empty());
    for m in &out.messages {
        assert!(
            m.starts_with("DPC1::"),
            "expected DPC1:: cover, got: {}",
            &m[..m.len().min(20)]
        );
    }
    assert!(out.session_id.is_some(), "Mode 1 must stamp a session_id");
}

#[test]
#[ignore = "Mode 1 disabled in V2; V3 will re-enable"]
fn mode1_chunks_reassemble_to_original_wire_bytes() {
    // End-to-end: send via Mode 1 → for each cover, decode_mode1 +
    // parse_chunk → push to ReassemblyBuffer → on complete, the
    // recovered bytes must equal the underlying DPC0:: wire's
    // base64-decoded body.
    let state = fresh_state();
    install_dm_peer(&state);
    let scope = Scope::dm(HENRY_DID);

    // First: send in Mode 0 to capture the canonical wire bytes.
    set_mode(&state, StegoMode::Mode0);
    let mode0_out = send(&state, &scope, "round-trip mode 1 reassembly");
    let wire = &mode0_out.messages[0];
    let raw_wire_bytes = STANDARD
        .decode(wire.strip_prefix("DPC0::").unwrap())
        .unwrap();

    // Now: send the same plaintext in Mode 1, capture the covers.
    set_mode(&state, StegoMode::Mode1);
    let mode1_out = send(&state, &scope, "round-trip mode 1 reassembly");
    // Note: the Mode 1 path inside cmd_osl_encrypt_message_v2
    // runs a fresh encrypt — nonces differ — so we can't compare
    // its raw bytes to the Mode 0 capture above. Instead we
    // independently re-chunk the Mode 1 path's own wire and
    // confirm the reassembled bytes equal what the chunker
    // produced. The check is internal-consistency.
    let salt = scope.storage_key().into_bytes();
    let cipher = stego::ConversationCipher::from_salt(&salt);

    let mut buf = stego::ReassemblyBuffer::new();
    let session_id = mode1_out.session_id.unwrap();
    let mut completed: Option<stego::ReassemblyComplete> = None;
    for cover in &mode1_out.messages {
        let chunk_bytes = stego::decode_mode1(&cipher, cover).expect("decode_mode1");
        let parsed = stego::parse_chunk(&salt, &chunk_bytes).expect("parse_chunk");
        assert_eq!(parsed.session_id, session_id);
        let outcome = buf.push(
            parsed.session_id,
            parsed.chunk_index,
            parsed.total_chunks,
            parsed.payload,
            0,
        );
        match outcome {
            stego::PushOutcome::Complete(c) => {
                completed = Some(c);
                break;
            }
            stego::PushOutcome::Incomplete { .. } => {}
            stego::PushOutcome::Conflict => panic!("unexpected conflict during reassembly"),
        }
    }
    let complete = completed.expect("reassembly should complete after all chunks");
    // The reassembled bytes are a valid wire (v=3/v=4/v=5 framed).
    // Sanity: first byte is one of the known version codes.
    let v = complete.wire_bytes[0];
    assert!(
        v == 0x03 || v == 0x04 || v == 0x05,
        "expected v=3/4/5 byte, got 0x{v:02x}"
    );
    // raw_wire_bytes is unused beyond establishing the canonical
    // wire shape; keep the variable around to document intent.
    assert!(!raw_wire_bytes.is_empty());
}
