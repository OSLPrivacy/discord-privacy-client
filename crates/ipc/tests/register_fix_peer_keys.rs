//! REGISTER-FIX: cross-machine decrypt bug.
//!
//! Root cause: post-9-C1 nothing populated a peer's keys from a
//! Discord snowflake — `cmd_osl_set_whitelist` only wrote
//! `outgoing_whitelists`, and `refresh_peer_pubkeys_from_keyserver`
//! dead-ended when `osl_user_id` was unset (which nothing set). So
//! whitelisted peers stayed keyless → send silently encrypted to
//! self only (FP-1) and receive rejected at UnknownSender (FP-2).
//!
//! These tests pin the fix's observable behavior WITHOUT a live
//! keyserver (the keyserver-dependent leg is best-effort and proven
//! to fail *open* into self-heal, not to panic / drop silently).

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ipc::commands::{cmd_osl_set_whitelist, populate_peer_from_fetch_response};
use ipc::scope::{Scope, ScopeInput};
use ipc::state::AppState;
use ipc::whitelist::{recipients_for_scope_v3, RecipientsV3Error, ScopeAuthCtx};
use keystore::client::PubkeysResponse;
use keystore::generate_identity;

const SELF_DID: &str = "900000000000000003";
const PEER_DID: &str = "900000000000000001";

fn fresh_state() -> AppState {
    let state = AppState::new();
    *state.identity.lock().unwrap() = Some(generate_identity("self".to_string()));
    {
        let mut pm = state.peer_map.lock().unwrap();
        let pe = pm.entry(SELF_DID.to_string()).or_default();
        pe.is_self = Some(true);
        pe.discord_id = Some(SELF_DID.to_string());
    }
    state
}

fn si(s: &Scope) -> ScopeInput {
    ScopeInput::from(s)
}

/// A keyserver pubkeys response for PEER_DID with real-length keys.
fn peer_pubkeys_response() -> PubkeysResponse {
    PubkeysResponse {
        user_id: PEER_DID.to_string(),
        ik_x25519_pub: STANDARD.encode([0x11u8; 32]),
        ik_ed25519_pub: STANDARD.encode([0x22u8; 32]),
        ik_mlkem768_pub: STANDARD.encode([0x33u8; 1184]),
        registered_at: "2026-05-16T00:00:00Z".to_string(),
        last_rotated_at: None,
        ik_ratchet_initial_pub: Some(STANDARD.encode([0x44u8; 32])),
    }
}

#[test]
fn whitelisting_a_peer_seeds_osl_user_id() {
    let state = fresh_state();
    // No keyserver installed → the whitelist-time fetch is
    // best-effort and fails silently; the whitelist op must still
    // succeed AND seed osl_user_id (== the snowflake in V2).
    cmd_osl_set_whitelist(
        &state,
        PEER_DID.to_string(),
        si(&Scope::dm(PEER_DID)),
        false,
    )
    .expect("whitelist must succeed even with no keyserver");

    let pm = state.peer_map.lock().unwrap();
    let pe = pm.get(PEER_DID).expect("peer entry created");
    assert_eq!(
        pe.osl_user_id.as_deref(),
        Some(PEER_DID),
        "whitelisting must seed osl_user_id with the peer snowflake"
    );
    assert!(
        pe.outgoing_whitelists
            .iter()
            .any(|w| matches!(w, ipc::peer_map::WhitelistEntry::Dm { .. })),
        "DM whitelist entry recorded"
    );
}

#[test]
fn recipients_v3_does_not_silently_drop_a_keyless_whitelisted_peer() {
    let state = fresh_state();
    cmd_osl_set_whitelist(
        &state,
        PEER_DID.to_string(),
        si(&Scope::dm(PEER_DID)),
        false,
    )
    .unwrap();

    let id = state.identity.lock().unwrap();
    let id = id.as_ref().unwrap();
    let self_mlkem = id.mlkem_encapsulation_key();
    let pm = state.peer_map.lock().unwrap();
    let ws = state.whitelist_state.lock().unwrap();
    let sd = state.server_defaults.lock().unwrap();
    let membership = state.scope_membership.lock().unwrap();
    let auth_ctx = ScopeAuthCtx {
        whitelist_state: &ws,
        server_defaults: &sd,
        membership: &membership,
    };

    let res = recipients_for_scope_v3(
        &pm,
        &auth_ctx,
        &Scope::dm(PEER_DID),
        &[SELF_DID.to_string(), PEER_DID.to_string()],
        SELF_DID,
        &id.x25519_public,
        &self_mlkem,
    );
    // BEFORE the fix this returned Ok([self]) (silent self-only).
    // Now it MUST surface a recoverable error so the caller fetches.
    match res {
        Err(RecipientsV3Error::PeerMissingKeys { discord_id }) => {
            assert_eq!(discord_id, PEER_DID);
        }
        Ok(r) => panic!(
            "expected PeerMissingKeys; got Ok with {} recipient(s) \
             (silent self-only regression!)",
            r.len()
        ),
        Err(e) => panic!("expected PeerMissingKeys, got {e:?}"),
    }
}

#[test]
fn recipients_v3_includes_a_keycomplete_peer_not_self_only() {
    let state = fresh_state();
    cmd_osl_set_whitelist(
        &state,
        PEER_DID.to_string(),
        si(&Scope::dm(PEER_DID)),
        false,
    )
    .unwrap();
    // Simulate the keyserver fetch having populated the peer.
    populate_peer_from_fetch_response(&state, PEER_DID, &peer_pubkeys_response())
        .expect("populate ok");

    let id = state.identity.lock().unwrap();
    let id = id.as_ref().unwrap();
    let self_mlkem = id.mlkem_encapsulation_key();
    let pm = state.peer_map.lock().unwrap();
    let ws = state.whitelist_state.lock().unwrap();
    let sd = state.server_defaults.lock().unwrap();
    let membership = state.scope_membership.lock().unwrap();
    let auth_ctx = ScopeAuthCtx {
        whitelist_state: &ws,
        server_defaults: &sd,
        membership: &membership,
    };

    let recips = recipients_for_scope_v3(
        &pm,
        &auth_ctx,
        &Scope::dm(PEER_DID),
        &[SELF_DID.to_string(), PEER_DID.to_string()],
        SELF_DID,
        &id.x25519_public,
        &self_mlkem,
    )
    .expect("key-complete peer resolves");
    assert_eq!(
        recips.len(),
        2,
        "must include self + the whitelisted peer (NOT self-only)"
    );
}

#[test]
fn populate_sets_osl_user_id_and_ratchet_and_no_false_tofu_alert() {
    let state = fresh_state();
    // Keyless entry (as a wiped+rewhitelisted entry would be).
    cmd_osl_set_whitelist(
        &state,
        PEER_DID.to_string(),
        si(&Scope::dm(PEER_DID)),
        false,
    )
    .unwrap();

    populate_peer_from_fetch_response(&state, PEER_DID, &peer_pubkeys_response())
        .expect("populate ok");

    let pm = state.peer_map.lock().unwrap();
    let pe = pm.get(PEER_DID).unwrap();
    assert_eq!(pe.osl_user_id.as_deref(), Some(PEER_DID));
    assert!(pe.pubkey.is_some(), "x25519 populated");
    assert!(pe.ik_mlkem768_pub.is_some(), "ML-KEM populated");
    assert_eq!(
        pe.ik_ratchet_initial_pub.as_deref(),
        Some(STANDARD.encode([0x44u8; 32]).as_str()),
        "ratchet bootstrap pub populated (enables v=4 DM)"
    );
    assert_eq!(
        pe.tofu_ed25519_pub.as_deref(),
        Some(STANDARD.encode([0x22u8; 32]).as_str()),
        "TOFU baseline recorded on first populate"
    );
    drop(pm);
    // FIRST populate of a previously-keyless peer must NOT raise a
    // key-change alert (it's first-use, not a rotation).
    assert!(
        state.key_change_alerts.lock().unwrap().is_empty(),
        "no false TOFU 'key changed' alert on initial populate"
    );
}

// NOTE: a 5th test (`receive_defaults_to_snowflake_instead_of_unknown_sender`)
// was removed in the receive-side revert. It asserted the
// now-reverted behavior that an unmapped sender defaults
// osl_user_id to the snowflake and consults the keyserver. The
// deliberate receive-path guarantee (unmapped sender →
// UnknownSender, NO keyserver call) is restored and is covered
// AS WRITTEN by tests/peer_map_decrypt.rs. The cross-machine fix
// is entirely send-side (the four tests above) + v3/v4 wire, so
// nothing of the fix is lost by this removal.
