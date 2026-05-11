//! 7d-FIX3 self-entry verification + snowflake-registration tests.
//!
//! These tests drive `verify_peer_map_self_entry` and the
//! sub-paths inside `cmd_osl_register_self_snowflake` that DON'T
//! touch disk. The disk write half (sealing identity.json,
//! persisting peer_map.json) is exercised via the bootstrap +
//! Tauri shell at runtime; here we only test the in-memory
//! mutation semantics so the test process stays hermetic.

use base64::{engine::general_purpose::STANDARD, Engine};
use ipc::commands::verify_peer_map_self_entry;
use ipc::peer_map::PeerEntry;
use ipc::state::AppState;
use keystore::generate_identity;

const SELF_DID: &str = "1477008451799482419";
const PEER_DID: &str = "1502770642930634812";

fn fresh_state_with_identity(snowflake: Option<&str>) -> AppState {
    let state = AppState::new();
    let mut id = generate_identity("liam".to_string());
    id.discord_snowflake = snowflake.map(|s| s.to_string());
    *state.identity.lock().unwrap() = Some(id);
    state
}

#[test]
fn verify_returns_no_discord_snowflake_when_identity_has_none() {
    let state = fresh_state_with_identity(None);
    let err = verify_peer_map_self_entry(&state).expect_err("should err");
    assert_eq!(err, "no_discord_snowflake");
}

#[test]
fn verify_returns_identity_not_loaded_when_state_empty() {
    let state = AppState::new();
    let err = verify_peer_map_self_entry(&state).expect_err("should err");
    assert_eq!(err, "identity_not_loaded");
}

#[test]
fn verify_creates_self_entry_when_peer_map_empty() {
    let state = fresh_state_with_identity(Some(SELF_DID));
    let (snowflake, repaired) = verify_peer_map_self_entry(&state).expect("verify ok");
    assert_eq!(snowflake, SELF_DID);
    assert!(repaired, "should report repair when entry was missing");

    let pm = state.peer_map.lock().unwrap();
    let entry = pm.get(SELF_DID).expect("self entry created");
    assert_eq!(entry.osl_user_id.as_deref(), Some("liam"));
    assert_eq!(entry.discord_id.as_deref(), Some(SELF_DID));
    assert_eq!(entry.is_self, Some(true));
    // pubkey should match the loaded identity's x25519 public.
    let id_guard = state.identity.lock().unwrap();
    let id = id_guard.as_ref().unwrap();
    let expected_b64 = STANDARD.encode(id.x25519_public.as_bytes());
    assert_eq!(entry.pubkey.as_deref(), Some(expected_b64.as_str()));
}

#[test]
fn verify_repairs_stale_osl_user_id() {
    let state = fresh_state_with_identity(Some(SELF_DID));
    {
        let mut pm = state.peer_map.lock().unwrap();
        let id_guard = state.identity.lock().unwrap();
        let id = id_guard.as_ref().unwrap();
        let pk_b64 = STANDARD.encode(id.x25519_public.as_bytes());
        pm.entry(SELF_DID.to_string()).or_insert_with(|| PeerEntry {
            // STALE: wrong osl_user_id.
            osl_user_id: Some("stale_name".to_string()),
            pubkey: Some(pk_b64),
            discord_id: Some(SELF_DID.to_string()),
            is_self: Some(true),
            ..Default::default()
        });
    }
    let (_, repaired) = verify_peer_map_self_entry(&state).expect("verify ok");
    assert!(repaired, "should repair stale osl_user_id");

    let pm = state.peer_map.lock().unwrap();
    let entry = pm.get(SELF_DID).unwrap();
    assert_eq!(entry.osl_user_id.as_deref(), Some("liam"));
}

#[test]
fn verify_repairs_missing_is_self_flag() {
    let state = fresh_state_with_identity(Some(SELF_DID));
    {
        let mut pm = state.peer_map.lock().unwrap();
        let id_guard = state.identity.lock().unwrap();
        let id = id_guard.as_ref().unwrap();
        let pk_b64 = STANDARD.encode(id.x25519_public.as_bytes());
        pm.entry(SELF_DID.to_string()).or_insert_with(|| PeerEntry {
            osl_user_id: Some("liam".to_string()),
            pubkey: Some(pk_b64),
            discord_id: Some(SELF_DID.to_string()),
            // is_self left as None — represents a pre-FIX3 entry.
            is_self: None,
            ..Default::default()
        });
    }
    let (_, repaired) = verify_peer_map_self_entry(&state).expect("verify ok");
    assert!(repaired, "should repair entry with missing is_self");
    let pm = state.peer_map.lock().unwrap();
    assert_eq!(pm.get(SELF_DID).unwrap().is_self, Some(true));
}

#[test]
fn verify_repairs_wrong_pubkey() {
    let state = fresh_state_with_identity(Some(SELF_DID));
    {
        let mut pm = state.peer_map.lock().unwrap();
        pm.entry(SELF_DID.to_string()).or_insert_with(|| PeerEntry {
            osl_user_id: Some("liam".to_string()),
            // STALE: empty pubkey.
            pubkey: Some(String::new()),
            discord_id: Some(SELF_DID.to_string()),
            is_self: Some(true),
            ..Default::default()
        });
    }
    let (_, repaired) = verify_peer_map_self_entry(&state).expect("verify ok");
    assert!(repaired);
    let pm = state.peer_map.lock().unwrap();
    let entry = pm.get(SELF_DID).unwrap();
    assert!(!entry.pubkey.as_deref().unwrap().is_empty());
}

#[test]
fn verify_is_noop_when_entry_already_correct() {
    let state = fresh_state_with_identity(Some(SELF_DID));
    let pk_b64 = {
        let id_guard = state.identity.lock().unwrap();
        let id = id_guard.as_ref().unwrap();
        STANDARD.encode(id.x25519_public.as_bytes())
    };
    {
        let mut pm = state.peer_map.lock().unwrap();
        pm.entry(SELF_DID.to_string()).or_insert_with(|| PeerEntry {
            osl_user_id: Some("liam".to_string()),
            pubkey: Some(pk_b64),
            discord_id: Some(SELF_DID.to_string()),
            is_self: Some(true),
            ..Default::default()
        });
    }
    let (snowflake, repaired) = verify_peer_map_self_entry(&state).expect("verify ok");
    assert_eq!(snowflake, SELF_DID);
    assert!(!repaired, "should be a no-op when entry already matches");
}

#[test]
fn verify_does_not_touch_unrelated_peer_entries() {
    let state = fresh_state_with_identity(Some(SELF_DID));
    {
        let mut pm = state.peer_map.lock().unwrap();
        // Pre-existing unrelated peer entry — verify must not
        // mutate it (per Q3: A, only self-entry gets audited).
        pm.entry(PEER_DID.to_string()).or_insert_with(|| PeerEntry {
            osl_user_id: Some("henry".to_string()),
            pubkey: Some("PEER_PUBKEY_PLACEHOLDER".to_string()),
            discord_id: Some(PEER_DID.to_string()),
            is_self: None,
            ..Default::default()
        });
    }
    let (_, repaired) = verify_peer_map_self_entry(&state).expect("verify ok");
    assert!(repaired, "self entry created (peer entry pre-existed)");
    let pm = state.peer_map.lock().unwrap();
    let peer = pm.get(PEER_DID).expect("peer still present");
    assert_eq!(peer.osl_user_id.as_deref(), Some("henry"));
    assert_eq!(peer.pubkey.as_deref(), Some("PEER_PUBKEY_PLACEHOLDER"));
    assert_eq!(peer.is_self, None, "peer entry's is_self stays None");
}

// ---- snowflake-format validation ----

#[test]
fn register_rejects_non_numeric_snowflake() {
    let state = fresh_state_with_identity(None);
    let err = ipc::commands::cmd_osl_register_self_snowflake(&state, "not-a-snowflake".to_string())
        .expect_err("should err");
    assert!(err.contains("invalid format"), "got: {err}");
}

#[test]
fn register_rejects_too_short_snowflake() {
    let state = fresh_state_with_identity(None);
    let err = ipc::commands::cmd_osl_register_self_snowflake(&state, "12345".to_string())
        .expect_err("should err");
    assert!(err.contains("invalid format"), "got: {err}");
}

#[test]
fn register_rejects_too_long_snowflake() {
    let state = fresh_state_with_identity(None);
    // 21 digits — beyond the 20-digit Discord snowflake max.
    let err =
        ipc::commands::cmd_osl_register_self_snowflake(&state, "123456789012345678901".to_string())
            .expect_err("should err");
    assert!(err.contains("invalid format"), "got: {err}");
}

#[test]
fn register_rejects_mismatch_with_existing_snowflake() {
    let state = fresh_state_with_identity(Some(SELF_DID));
    let other = "9999999999999999999"; // different 19-digit snowflake
    let err = ipc::commands::cmd_osl_register_self_snowflake(&state, other.to_string())
        .expect_err("should err");
    assert!(err.contains("mismatch"), "got: {err}");
}
