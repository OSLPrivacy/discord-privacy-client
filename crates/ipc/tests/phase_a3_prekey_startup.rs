//! A106: persisted prekey state must hydrate AppState on startup.
//!
//! `AppState::install_identity` constructs a fresh live prekey pool so a new
//! identity can publish. Startup is different: when `prekeys.json` already
//! exists, that file is the published local authority and must replace the
//! generated placeholder before any prekey-dependent work reads AppState.

use ipc::state_reload::load_persisted_prekey_state_with_sealer;
use ipc::AppState;
use keystore::{generate_identity, save_prekey_state, NoOpSealer, PrekeyConfig, PrekeyState};
use tempfile::tempdir;

#[test]
fn startup_loads_persisted_prekey_state_into_app_state() {
    let dir = tempdir().unwrap();
    let sealer = NoOpSealer::new();
    let identity = generate_identity("startup-prekey-owner".to_owned());
    let persisted = PrekeyState::new(&identity, PrekeyConfig::default(), 123);
    save_prekey_state(&dir.path().join("prekeys.json"), &persisted, &sealer).unwrap();

    let state = AppState::new();
    state.install_identity(identity);
    assert_ne!(
        state
            .prekey_state
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .current_spk
            .rotated_at_unix_seconds,
        123,
        "test precondition: install_identity generated a fresh placeholder"
    );

    let loaded = load_persisted_prekey_state_with_sealer(&state, dir.path(), &sealer).unwrap();

    assert!(loaded);
    let guard = state.prekey_state.lock().unwrap();
    let live = guard.as_ref().expect("prekey state hydrated");
    assert_eq!(live.current_spk.rotated_at_unix_seconds, 123);
    assert_eq!(live.opk_pool.len(), persisted.opk_pool.len());
    assert_eq!(live.current_spk.public, persisted.current_spk.public);
}

#[test]
fn startup_refuses_unbound_persisted_prekey_state() {
    let dir = tempdir().unwrap();
    let sealer = NoOpSealer::new();
    let owner = generate_identity("correct-owner".to_owned());
    let other = generate_identity("other-owner".to_owned());
    let persisted_for_other = PrekeyState::new(&other, PrekeyConfig::default(), 456);
    save_prekey_state(
        &dir.path().join("prekeys.json"),
        &persisted_for_other,
        &sealer,
    )
    .unwrap();

    let state = AppState::new();
    state.install_identity(owner);
    assert!(state.has_prekey_state());

    let err = load_persisted_prekey_state_with_sealer(&state, dir.path(), &sealer)
        .expect_err("unbound prekeys must refuse");

    assert!(
        err.contains("not bound"),
        "expected binding refusal, got {err}"
    );
    assert!(
        !state.has_prekey_state(),
        "failed persisted authority must clear generated live prekeys"
    );
}
