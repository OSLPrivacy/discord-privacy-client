//! Phase 5 v1: peer_map.json + cmd_osl_decrypt_message integration.
//!
//! Pure-decoder coverage lives in `osl_phase5_decrypt.rs`. This file
//! covers the receive-side wiring around the decoder:
//!
//! - `peer_map.json` loader behaviour (delegates to `peer_map`
//!   module's own unit tests for the small cases; here we focus on
//!   the AppState integration).
//! - `cmd_osl_decrypt_message` resolution path: an unmapped
//!   discord_id returns the typed `UnknownSender` error string and
//!   never touches the keyserver.
//! - The cmd never invokes `fetch_pubkeys` when peer_map says the
//!   sender is unknown, so a missing keyserver is fine in that
//!   branch.
//!
//! What's NOT covered here: the cache → keyserver → cache-insert
//! happy path. That requires a live keyserver (or a fake), which
//! lives in the keystore crate's e2e tests; once we add a fake
//! keyserver harness for IPC-level tests we can test the resolved
//! happy path here.

use ipc::commands::cmd_osl_decrypt_message;
use ipc::state::AppState;
use keystore::generate_identity;

fn fresh_state_with_identity(user_id: &str) -> AppState {
    let state = AppState::new();
    let id = generate_identity(user_id.to_string());
    *state.identity.lock().unwrap() = Some(id);
    state
}

fn install_peer_map(state: &AppState, entries: &[(&str, &str)]) {
    let mut guard = state.peer_map.lock().unwrap();
    guard.clear();
    for (k, v) in entries {
        guard.insert((*k).to_string(), ipc::peer_map::legacy_entry(*v));
    }
}

// ---- UnknownSender path ----

#[test]
fn unmapped_discord_id_returns_unknown_sender() {
    let state = fresh_state_with_identity("liam");
    // peer_map intentionally empty.
    let err = cmd_osl_decrypt_message(
        &state,
        "channel-id".to_string(),
        "900000000000000003".to_string(),
        "DPC0::doesnt-matter-we-error-before-decoding".to_string(),
    )
    .expect_err("unmapped discord_id should error");
    assert!(
        err.contains("no peer mapping for discord_id=900000000000000003"),
        "expected UnknownSender, got: {err}"
    );
}

#[test]
fn unmapped_discord_id_does_not_consult_keyserver() {
    // The state has identity AND peer_map populated for ONE
    // discord_id but not the one we ask about. Keyserver is
    // intentionally absent so any code path that tried to call it
    // would surface "key-server not initialised" rather than
    // UnknownSender — proving the resolution gate runs first.
    let state = fresh_state_with_identity("liam");
    install_peer_map(&state, &[("900000000000000001", "henry")]);
    let err = cmd_osl_decrypt_message(
        &state,
        "channel-id".to_string(),
        "9999999999999999999".to_string(),
        "DPC0::cover".to_string(),
    )
    .expect_err("unmapped discord_id should error");
    assert!(
        err.contains("no peer mapping for discord_id=9999999999999999999"),
        "expected UnknownSender to short-circuit before keyserver, got: {err}"
    );
    assert!(
        !err.contains("key-server not initialised"),
        "UnknownSender should fire before keyserver-missing check; got: {err}"
    );
}

#[test]
fn empty_peer_map_returns_unknown_sender_for_every_id() {
    let state = fresh_state_with_identity("liam");
    // Default-constructed AppState has an empty peer_map.
    for discord_id in &["1", "900000000000000003", ""] {
        let err = cmd_osl_decrypt_message(
            &state,
            "ch".to_string(),
            (*discord_id).to_string(),
            "DPC0::body".to_string(),
        )
        .expect_err("empty peer_map should reject every id");
        assert!(
            err.contains(&format!("no peer mapping for discord_id={discord_id}")),
            "expected UnknownSender for discord_id={discord_id}, got: {err}"
        );
    }
}

// ---- Identity gating still fires before peer_map ----

#[test]
fn missing_identity_errors_before_peer_map_consulted() {
    // No identity loaded — cmd should reject with "identity not
    // loaded" *before* it gets to peer_map resolution.
    let state = AppState::new();
    install_peer_map(&state, &[("900000000000000003", "liam")]);
    let err = cmd_osl_decrypt_message(
        &state,
        "ch".to_string(),
        "900000000000000003".to_string(),
        "DPC0::body".to_string(),
    )
    .expect_err("no identity should error");
    assert!(
        err.contains("identity not loaded"),
        "expected identity-missing to fire before peer_map check, got: {err}"
    );
}

// ---- Mapping is by exact key match (no normalization) ----

#[test]
fn peer_map_lookup_is_exact_match_no_trim_no_case_fold() {
    let state = fresh_state_with_identity("liam");
    install_peer_map(&state, &[("900000000000000003", "henry")]);

    // Trailing whitespace in the lookup key — should NOT match.
    let err = cmd_osl_decrypt_message(
        &state,
        "ch".to_string(),
        " 900000000000000003 ".to_string(),
        "DPC0::body".to_string(),
    )
    .expect_err("trimmed key should not match exact entry");
    assert!(
        err.contains("no peer mapping for discord_id= 900000000000000003 "),
        "expected exact-match miss, got: {err}"
    );
}
