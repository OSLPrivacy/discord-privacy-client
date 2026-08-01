//! Phase 9-TD1.4 regression tests.
//!
//! Locks the contract: `cmd_osl_take_last_persist_error` reads + clears
//! `state.last_persist_error`, so the JS layer can surface
//! "couldn't save to disk" toasts after a silent persist failure.
//!
//! End-to-end disk-write-failure tests are hard at this layer because
//! the production `persist_*_now` functions resolve `keystore::osl_config_dir`
//! internally (no test injection seam). We exercise the take-and-clear
//! contract directly: a persist failure (modelled as a state write
//! into the sentinel slot) is surfaced to the next take call and is
//! gone after that. The persist hooks themselves are simple
//! `if let Err = write { record_persist_error(...) }` patterns
//! verified by inspection.

use ipc::commands::cmd_osl_take_last_persist_error;
use ipc::AppState;

#[test]
fn take_returns_none_when_no_error_recorded() {
    let state = AppState::new();
    assert_eq!(cmd_osl_take_last_persist_error(&state), None);
}

#[test]
fn take_returns_recorded_error_then_clears() {
    let state = AppState::new();
    *state.last_persist_error.lock().unwrap() = Some("peer_map.json: disk full".to_string());

    let first = cmd_osl_take_last_persist_error(&state);
    assert_eq!(first.as_deref(), Some("peer_map.json: disk full"));

    let second = cmd_osl_take_last_persist_error(&state);
    assert_eq!(second, None, "second take must return None (read-once)");
}

#[test]
fn last_write_wins_when_multiple_persist_failures_stack() {
    let state = AppState::new();
    *state.last_persist_error.lock().unwrap() = Some("first failure".to_string());
    *state.last_persist_error.lock().unwrap() = Some("second failure".to_string());

    let observed = cmd_osl_take_last_persist_error(&state);
    assert_eq!(observed.as_deref(), Some("second failure"));
    // Single-slot design intentionally collapses bursts; the UX
    // signal is "something failed, please retry," not a forensic log.
}

/// AppState's default initialisation must give us an empty error
/// slot — otherwise the first JS poll on every launch would surface
/// a phantom error to the user.
#[test]
fn appstate_default_starts_with_no_persist_error() {
    let state = AppState::default();
    assert!(state.last_persist_error.lock().unwrap().is_none());
}
