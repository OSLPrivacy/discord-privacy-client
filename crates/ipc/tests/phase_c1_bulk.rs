//! Phase 9-C1 Stage 3: bulk whitelist commands.
//!
//! Covers `cmd_osl_bulk_set_whitelist` and
//! `cmd_osl_bulk_unwhitelist_scope`:
//!
//! 1. bulk_set adds the scope to every named peer + flips toggle.
//! 2. bulk_set returns the count of peers actually mutated (skips
//!    existing entries).
//! 3. bulk_unwhitelist removes the scope from every named peer +
//!    stamps a fresh BurnedScope on each.
//! 4. bulk_unwhitelist returns 0 for peers that never had the
//!    scope (no spurious BurnedScope entries on unrelated peers).
//! 5. bulk_set re-whitelisting a previously-burned peer clears the
//!    matching BurnedScope so future messages decrypt normally.

use ipc::commands::{cmd_osl_bulk_set_whitelist, cmd_osl_bulk_unwhitelist_scope};
use ipc::peer_map::{BurnedScope, WhitelistEntry};
use ipc::scope::{Scope, ScopeInput};
use ipc::state::AppState;
use keystore::generate_identity;

const LIAM_DID: &str = "1477008451799482419";
const HENRY_DID: &str = "1502770642930634812";
const ALICE_DID: &str = "1602770642930634812";
const BOB_DID: &str = "1702770642930634812";
const GC_ID: &str = "1234567890";

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

fn si(scope: &Scope) -> ScopeInput {
    ScopeInput::from(scope)
}

#[test]
fn bulk_set_whitelists_every_named_peer_and_flips_toggle() {
    let state = fresh_state();
    let scope = Scope::gc(GC_ID);
    let count = cmd_osl_bulk_set_whitelist(
        &state,
        si(&scope),
        vec![
            HENRY_DID.to_string(),
            ALICE_DID.to_string(),
            BOB_DID.to_string(),
        ],
    )
    .unwrap();
    assert_eq!(count, 3);

    let pm = state.peer_map.lock().unwrap();
    for did in [HENRY_DID, ALICE_DID, BOB_DID] {
        let pe = pm.get(did).expect("peer entry created by bulk-set");
        assert!(
            pe.outgoing_whitelists.iter().any(|w| matches!(
                w,
                WhitelistEntry::Gc { id, .. } if id == GC_ID
            )),
            "peer {did} missing Gc whitelist entry"
        );
    }
    drop(pm);

    let ws = state.whitelist_state.lock().unwrap();
    let entry = ws.get(&scope.storage_key()).expect("ScopeState created");
    assert!(entry.encrypt_toggle, "bulk-set must flip encrypt_toggle on");
    assert!(entry.auto_enabled);
}

#[test]
fn bulk_set_skips_already_whitelisted_peers() {
    let state = fresh_state();
    let scope = Scope::gc(GC_ID);
    // First pass: 2 peers whitelisted.
    let c1 = cmd_osl_bulk_set_whitelist(
        &state,
        si(&scope),
        vec![HENRY_DID.to_string(), ALICE_DID.to_string()],
    )
    .unwrap();
    assert_eq!(c1, 2);

    // Second pass: 3 peers, but two were already in. Only the new
    // one should count.
    let c2 = cmd_osl_bulk_set_whitelist(
        &state,
        si(&scope),
        vec![
            HENRY_DID.to_string(),
            ALICE_DID.to_string(),
            BOB_DID.to_string(),
        ],
    )
    .unwrap();
    assert_eq!(c2, 1, "bulk-set must skip peers already on the scope");

    let pm = state.peer_map.lock().unwrap();
    let henry = pm.get(HENRY_DID).unwrap();
    let gc_count = henry
        .outgoing_whitelists
        .iter()
        .filter(|w| matches!(w, WhitelistEntry::Gc { id, .. } if id == GC_ID))
        .count();
    assert_eq!(gc_count, 1, "no duplicate Gc entries on re-run");
}

#[test]
fn bulk_unwhitelist_removes_scope_and_stamps_burn() {
    let state = fresh_state();
    let scope = Scope::gc(GC_ID);
    cmd_osl_bulk_set_whitelist(
        &state,
        si(&scope),
        vec![HENRY_DID.to_string(), ALICE_DID.to_string()],
    )
    .unwrap();

    let count = cmd_osl_bulk_unwhitelist_scope(
        &state,
        si(&scope),
        vec![HENRY_DID.to_string(), ALICE_DID.to_string()],
    )
    .unwrap();
    assert_eq!(count, 2);

    let pm = state.peer_map.lock().unwrap();
    for did in [HENRY_DID, ALICE_DID] {
        let pe = pm.get(did).unwrap();
        let has_gc = pe
            .outgoing_whitelists
            .iter()
            .any(|w| matches!(w, WhitelistEntry::Gc { id, .. } if id == GC_ID));
        assert!(!has_gc, "peer {did} should no longer have Gc entry");
        let has_burn = pe
            .burned_scopes
            .iter()
            .any(|b| matches!(b, BurnedScope::Gc { id, .. } if id == GC_ID));
        assert!(has_burn, "peer {did} should have a Gc BurnedScope stamp");
    }
}

#[test]
fn bulk_unwhitelist_skips_peers_without_the_scope() {
    let state = fresh_state();
    let scope = Scope::gc(GC_ID);
    // Henry IS on the scope.
    cmd_osl_bulk_set_whitelist(&state, si(&scope), vec![HENRY_DID.to_string()]).unwrap();
    // Try to bulk-unwhitelist a wider set including alice, who was
    // never on the scope. Alice must NOT gain a spurious BurnedScope.
    let count = cmd_osl_bulk_unwhitelist_scope(
        &state,
        si(&scope),
        vec![HENRY_DID.to_string(), ALICE_DID.to_string()],
    )
    .unwrap();
    assert_eq!(count, 1);

    let pm = state.peer_map.lock().unwrap();
    let alice = pm.get(ALICE_DID);
    // Alice may not even have a peer entry. If she does, she must
    // not carry a spurious Gc burn.
    if let Some(pe) = alice {
        assert!(
            !pe.burned_scopes
                .iter()
                .any(|b| matches!(b, BurnedScope::Gc { id, .. } if id == GC_ID)),
            "alice was never on the scope — must not be burned"
        );
    }
}

#[test]
fn bulk_set_clears_existing_burned_scope() {
    let state = fresh_state();
    let scope = Scope::gc(GC_ID);
    // Cycle: set → unwhitelist → set again on the same peer.
    cmd_osl_bulk_set_whitelist(&state, si(&scope), vec![HENRY_DID.to_string()]).unwrap();
    cmd_osl_bulk_unwhitelist_scope(&state, si(&scope), vec![HENRY_DID.to_string()]).unwrap();
    {
        let pm = state.peer_map.lock().unwrap();
        let henry = pm.get(HENRY_DID).unwrap();
        assert!(
            henry
                .burned_scopes
                .iter()
                .any(|b| matches!(b, BurnedScope::Gc { id, .. } if id == GC_ID)),
            "post-unwhitelist henry should be burned"
        );
    }
    let c2 = cmd_osl_bulk_set_whitelist(&state, si(&scope), vec![HENRY_DID.to_string()]).unwrap();
    assert_eq!(c2, 1, "re-whitelist after burn counts as a fresh mutation");
    let pm = state.peer_map.lock().unwrap();
    let henry = pm.get(HENRY_DID).unwrap();
    let still_burned = henry
        .burned_scopes
        .iter()
        .any(|b| matches!(b, BurnedScope::Gc { id, .. } if id == GC_ID));
    assert!(
        !still_burned,
        "bulk-set must clear the matching BurnedScope (decision-B)"
    );
}
