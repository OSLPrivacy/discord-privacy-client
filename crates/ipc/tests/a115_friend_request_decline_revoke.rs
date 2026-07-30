//! Unit a115: `cmd_osl_decline_or_revoke_friend_request`.
//!
//! Absence of an accepted scoped grant is a refusal, not permission: declining
//! a pending or unknown request must not create peer state. Revocation is only
//! allowed to remove an already-recorded grant for the exact peer/scope.

use ipc::commands::{
    cmd_osl_decline_or_revoke_friend_request, FriendRequestDecision, FriendRequestDecisionResult,
};
use ipc::peer_map::{BurnedScope, PeerEntry, WhitelistEntry};
use ipc::scope::{Scope, ScopeInput};
use ipc::state::AppState;
use ipc::whitelist_state::ScopeState;
use std::sync::Mutex;

static IO_LOCK: Mutex<()> = Mutex::new(());

const PEER: &str = "900000000000000101";
const OTHER_PEER: &str = "900000000000000202";
const GC: &str = "700000000000000303";

fn dm(id: &str) -> ScopeInput {
    ScopeInput::from(&Scope::dm(id))
}

fn accepted_dm_state(peer: &str) -> AppState {
    let state = AppState::new();
    {
        let mut peer_map = state.peer_map.lock().unwrap();
        peer_map.insert(
            peer.to_string(),
            PeerEntry {
                discord_id: Some(peer.to_string()),
                outgoing_whitelists: vec![
                    WhitelistEntry::Dm {
                        broadened: true,
                        enabled_at: Some("2026-07-29T00:00:00Z".to_string()),
                    },
                    WhitelistEntry::Gc {
                        id: GC.to_string(),
                        user_specific: true,
                    },
                ],
                ..PeerEntry::default()
            },
        );
    }
    {
        let mut whitelist_state = state.whitelist_state.lock().unwrap();
        whitelist_state.insert(
            Scope::dm(peer).storage_key(),
            ScopeState {
                encrypt_toggle: true,
                auto_enabled: true,
                channel_whitelisted: false,
            },
        );
    }
    state
}

#[test]
fn decline_without_existing_grant_creates_no_authority() {
    let state = AppState::new();

    let result =
        cmd_osl_decline_or_revoke_friend_request(&state, PEER.to_string(), dm(PEER), false)
            .unwrap();

    assert_eq!(
        result,
        FriendRequestDecisionResult {
            decision: FriendRequestDecision::DeclinedPending,
            revoked_grant: false,
        }
    );
    assert!(state.peer_map.lock().unwrap().get(PEER).is_none());
    assert!(state.whitelist_state.lock().unwrap().is_empty());
}

#[test]
fn revoke_existing_grant_removes_only_that_friend_scope() {
    let _guard = IO_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let state = accepted_dm_state(PEER);

    let result =
        cmd_osl_decline_or_revoke_friend_request(&state, PEER.to_string(), dm(PEER), true).unwrap();

    assert_eq!(result.decision, FriendRequestDecision::RevokedAcceptedGrant);
    assert!(result.revoked_grant);

    let peer_map = state.peer_map.lock().unwrap();
    let peer = peer_map.get(PEER).unwrap();
    assert_eq!(peer.outgoing_whitelists.len(), 1);
    assert!(matches!(
        peer.outgoing_whitelists.as_slice(),
        [WhitelistEntry::Gc {
            id,
            user_specific: true
        }] if id.as_str() == GC
    ));
    assert!(peer
        .burned_scopes
        .iter()
        .any(|scope| matches!(scope, BurnedScope::Dm { .. })));

    let whitelist_state = state.whitelist_state.lock().unwrap();
    assert!(whitelist_state
        .get(&Scope::dm(PEER).storage_key())
        .map(|entry| entry.encrypt_toggle)
        .unwrap_or(false));
}

#[test]
fn mismatched_dm_scope_is_refused_without_revoking_peer_grant() {
    let state = accepted_dm_state(PEER);

    let result =
        cmd_osl_decline_or_revoke_friend_request(&state, PEER.to_string(), dm(OTHER_PEER), true)
            .unwrap();

    assert_eq!(result.decision, FriendRequestDecision::DeclinedPending);
    assert!(!result.revoked_grant);

    let peer_map = state.peer_map.lock().unwrap();
    let peer = peer_map.get(PEER).unwrap();
    assert!(peer
        .outgoing_whitelists
        .iter()
        .any(|entry| matches!(entry, WhitelistEntry::Dm { .. })));
    assert!(peer.burned_scopes.is_empty());
}

#[test]
fn result_debug_never_contains_account_identifiers() {
    let result = cmd_osl_decline_or_revoke_friend_request(
        &AppState::new(),
        PEER.to_string(),
        dm(PEER),
        false,
    )
    .unwrap();
    let debug = format!("{result:?}");

    assert!(!debug.contains(PEER));
    assert!(!debug.contains("dm:"));
}
