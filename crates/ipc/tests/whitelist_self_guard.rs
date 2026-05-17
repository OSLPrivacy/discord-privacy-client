//! Defense-in-depth: cmd_osl_set_whitelist / cmd_osl_bulk_set_whitelist
//! must REJECT a peer id equal to the loaded identity's own Discord
//! snowflake (the Symptom-2 bug class — a UI peer-resolution
//! regression handing back the local user's id). The whitelist must
//! NOT be written and a clear error must be surfaced. A legitimate
//! peer (different id) must still succeed.

use ipc::commands::{cmd_osl_bulk_set_whitelist, cmd_osl_set_whitelist};
use ipc::scope::{Scope, ScopeInput};
use ipc::state::AppState;
use keystore::generate_identity;

const SELF_DID: &str = "1477008451799482419";
const PEER_DID: &str = "1502770642930634812";

fn state_with_self_snowflake() -> AppState {
    let state = AppState::new();
    let mut id = generate_identity("self".to_string());
    id.discord_snowflake = Some(SELF_DID.to_string());
    *state.identity.lock().unwrap() = Some(id);
    state
}

fn dm(id: &str) -> ScopeInput {
    ScopeInput::from(&Scope::dm(id))
}

#[test]
fn set_whitelist_rejects_self_with_clear_error() {
    let state = state_with_self_snowflake();
    let err = cmd_osl_set_whitelist(&state, SELF_DID.to_string(), dm(SELF_DID), false)
        .expect_err("whitelisting self must be rejected");
    assert!(
        err.contains("refusing to whitelist yourself")
            && err.contains(SELF_DID),
        "error must be clear + name the offending id (got: {err})"
    );
    // The whitelist must NOT have been written.
    assert!(
        state.peer_map.lock().unwrap().get(SELF_DID).is_none(),
        "no peer_map entry may be created for self"
    );
}

#[test]
fn bulk_set_whitelist_rejects_a_roster_containing_self() {
    let state = state_with_self_snowflake();
    let err = cmd_osl_bulk_set_whitelist(
        &state,
        ScopeInput::from(&Scope::gc("123456789")),
        vec![PEER_DID.to_string(), SELF_DID.to_string()],
    )
    .expect_err("a roster containing self must be rejected");
    assert!(
        err.contains("refusing to whitelist yourself"),
        "clear error (got: {err})"
    );
    // Fail-closed: nothing written for either id.
    let pm = state.peer_map.lock().unwrap();
    assert!(pm.get(SELF_DID).is_none());
    assert!(pm.get(PEER_DID).is_none());
}

#[test]
fn legit_peer_still_succeeds_guard_does_not_overblock() {
    let state = state_with_self_snowflake();
    // No keyserver installed → whitelist-time fetch is best-effort
    // and fails silently; the op must still succeed for a real peer.
    cmd_osl_set_whitelist(&state, PEER_DID.to_string(), dm(PEER_DID), false)
        .expect("whitelisting a real peer must still work");
    let pm = state.peer_map.lock().unwrap();
    let pe = pm.get(PEER_DID).expect("peer entry created");
    assert_eq!(pe.osl_user_id.as_deref(), Some(PEER_DID));
}

#[test]
fn guard_is_noop_when_identity_has_no_snowflake() {
    // Pre-snowflake state (generate_identity leaves discord_snowflake
    // = None). The guard must not block a normal whitelist here.
    let state = AppState::new();
    *state.identity.lock().unwrap() = Some(generate_identity("self".to_string()));
    cmd_osl_set_whitelist(&state, PEER_DID.to_string(), dm(PEER_DID), false)
        .expect("no snowflake → guard is a no-op, whitelist proceeds");
    assert!(state.peer_map.lock().unwrap().get(PEER_DID).is_some());
}
