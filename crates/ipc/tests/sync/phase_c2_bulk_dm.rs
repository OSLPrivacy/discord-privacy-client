//! Phase 9-C2: bulk DM-whitelist command tests.
//!
//! Covers `cmd_osl_bulk_set_dm_whitelist`:
//!
//! 1. Three fresh peers get DM scopes added in a single call.
//! 2. Idempotent — second run on the same set returns 0 affected.
//! 3. Empty input returns 0; no state mutation.

use ipc::commands::cmd_osl_bulk_set_dm_whitelist;
use ipc::peer_map::WhitelistEntry;
use ipc::scope::Scope;
use ipc::state::AppState;
use keystore::generate_identity;

const LIAM_DID: &str = "900000000000000003";
const HENRY_DID: &str = "900000000000000001";
const ALICE_DID: &str = "1602770642930634812";
const BOB_DID: &str = "1702770642930634812";

fn fresh_state() -> AppState {
    let state = AppState::new();
    *state.identity.lock().unwrap() = Some(generate_identity("liam".to_string()));
    {
        let mut pm = state.peer_map.lock().unwrap();
        let pe = pm.entry(LIAM_DID.to_string()).or_default();
        pe.is_self = Some(true);
        pe.discord_id = Some(LIAM_DID.to_string());
    }
    state
}

#[test]
fn bulk_dm_whitelists_three_peers_single_persistence() {
    let state = fresh_state();
    let dids = vec![
        HENRY_DID.to_string(),
        ALICE_DID.to_string(),
        BOB_DID.to_string(),
    ];
    let n = cmd_osl_bulk_set_dm_whitelist(&state, dids).unwrap();
    assert_eq!(n, 3);

    let pm = state.peer_map.lock().unwrap();
    for did in [HENRY_DID, ALICE_DID, BOB_DID] {
        let pe = pm.get(did).expect("peer entry created");
        assert!(
            pe.outgoing_whitelists
                .iter()
                .any(|w| matches!(w, WhitelistEntry::Dm { .. })),
            "peer {did} missing Dm entry"
        );
    }
    drop(pm);

    // Each DM scope's encrypt_toggle is set true.
    let ws = state.whitelist_state.lock().unwrap();
    for did in [HENRY_DID, ALICE_DID, BOB_DID] {
        let key = Scope::dm(did).storage_key();
        let entry = ws.get(&key).expect("ScopeState created");
        assert!(
            entry.encrypt_toggle,
            "DM scope {key} encrypt_toggle must be true"
        );
        assert!(entry.auto_enabled);
    }
}

#[test]
fn bulk_dm_idempotent_on_existing_whitelists() {
    let state = fresh_state();
    let dids = vec![HENRY_DID.to_string(), ALICE_DID.to_string()];
    let first = cmd_osl_bulk_set_dm_whitelist(&state, dids.clone()).unwrap();
    assert_eq!(first, 2);

    // Second run with one overlapping + one new peer. Only the new
    // one counts.
    let second_dids = vec![ALICE_DID.to_string(), BOB_DID.to_string()];
    let second = cmd_osl_bulk_set_dm_whitelist(&state, second_dids).unwrap();
    assert_eq!(second, 1, "alice already had Dm entry; only bob is new");

    // Alice still has exactly one Dm entry — no duplicates.
    let pm = state.peer_map.lock().unwrap();
    let alice = pm.get(ALICE_DID).unwrap();
    let dm_count = alice
        .outgoing_whitelists
        .iter()
        .filter(|w| matches!(w, WhitelistEntry::Dm { .. }))
        .count();
    assert_eq!(dm_count, 1);
}

#[test]
fn bulk_dm_empty_list_no_op() {
    let state = fresh_state();
    let n = cmd_osl_bulk_set_dm_whitelist(&state, vec![]).unwrap();
    assert_eq!(n, 0);

    let pm = state.peer_map.lock().unwrap();
    // Only self may be present; no peers got added.
    let non_self = pm.values().filter(|pe| pe.is_self != Some(true)).count();
    assert_eq!(non_self, 0);
}
