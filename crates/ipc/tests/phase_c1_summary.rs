//! Phase 9-C1 Stage 2: tri-state whitelist summary command.
//!
//! Drives `cmd_osl_get_scope_whitelist_summary` against curated
//! peer_map + channel_members combos to assert the four returned
//! state strings:
//!
//! - "all"     — every non-self channel member is whitelisted
//! - "some"    — at least one but not all are whitelisted
//! - "none"    — none are whitelisted
//! - "unknown" — channel_members is empty (roster unknown to us)

use ipc::commands::cmd_osl_get_scope_whitelist_summary;
use ipc::peer_map::WhitelistEntry;
use ipc::scope::{Scope, ScopeInput};
use ipc::state::AppState;

const LIAM_DID: &str = "1477008451799482419";
const HENRY_DID: &str = "1502770642930634812";
const ALICE_DID: &str = "1602770642930634812";
const GC_ID: &str = "1234567890";

fn add_gc_whitelist_for(state: &AppState, gc_id: &str, dids: &[&str]) {
    let mut pm = state.peer_map.lock().unwrap();
    for d in dids {
        let pe = pm.entry((*d).to_string()).or_default();
        pe.discord_id = Some((*d).to_string());
        pe.outgoing_whitelists.push(WhitelistEntry::Gc {
            id: gc_id.to_string(),
            user_specific: false,
        });
    }
}

fn si(scope: &Scope) -> ScopeInput {
    ScopeInput::from(scope)
}

#[test]
fn summary_all_when_every_member_whitelisted() {
    let state = AppState::new();
    add_gc_whitelist_for(&state, GC_ID, &[HENRY_DID, ALICE_DID]);
    let summary = cmd_osl_get_scope_whitelist_summary(
        &state,
        si(&Scope::gc(GC_ID)),
        vec![
            LIAM_DID.to_string(),
            HENRY_DID.to_string(),
            ALICE_DID.to_string(),
        ],
        LIAM_DID.to_string(),
    )
    .unwrap();
    assert_eq!(summary.state, "all");
    assert_eq!(summary.total_members, 2);
    assert_eq!(summary.whitelisted_count, 2);
}

#[test]
fn summary_some_when_subset_whitelisted() {
    let state = AppState::new();
    add_gc_whitelist_for(&state, GC_ID, &[HENRY_DID]);
    let summary = cmd_osl_get_scope_whitelist_summary(
        &state,
        si(&Scope::gc(GC_ID)),
        vec![
            LIAM_DID.to_string(),
            HENRY_DID.to_string(),
            ALICE_DID.to_string(),
        ],
        LIAM_DID.to_string(),
    )
    .unwrap();
    assert_eq!(summary.state, "some");
    assert_eq!(summary.total_members, 2);
    assert_eq!(summary.whitelisted_count, 1);
}

#[test]
fn summary_none_when_no_member_whitelisted() {
    let state = AppState::new();
    // Peers exist but no Gc whitelist entries.
    {
        let mut pm = state.peer_map.lock().unwrap();
        pm.entry(HENRY_DID.to_string()).or_default();
        pm.entry(ALICE_DID.to_string()).or_default();
    }
    let summary = cmd_osl_get_scope_whitelist_summary(
        &state,
        si(&Scope::gc(GC_ID)),
        vec![
            LIAM_DID.to_string(),
            HENRY_DID.to_string(),
            ALICE_DID.to_string(),
        ],
        LIAM_DID.to_string(),
    )
    .unwrap();
    assert_eq!(summary.state, "none");
    assert_eq!(summary.total_members, 2);
    assert_eq!(summary.whitelisted_count, 0);
}

#[test]
fn summary_unknown_when_channel_members_empty() {
    let state = AppState::new();
    let summary = cmd_osl_get_scope_whitelist_summary(
        &state,
        si(&Scope::gc(GC_ID)),
        // Only self in the list — non-self filter yields 0 members.
        vec![LIAM_DID.to_string()],
        LIAM_DID.to_string(),
    )
    .unwrap();
    assert_eq!(summary.state, "unknown");
    assert_eq!(summary.total_members, 0);
    assert_eq!(summary.whitelisted_count, 0);
}
