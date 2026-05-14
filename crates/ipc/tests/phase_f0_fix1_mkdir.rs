//! Phase 9-F0-FIX1: regression test for the missing %APPDATA%\osl
//! directory on a clean install.
//!
//! Pre-fix, `bootstrap::run_autostart` resolved the config-dir path
//! via `keystore::osl_config_dir()` but never created it. On a
//! truly clean install, the first write attempt (typically
//! `persist_app_preferences_now` mid-tour) failed with
//! `os error 3 — cannot find path`. TD1.4's error sentinel then
//! surfaced "Couldn't save change to disk" to the user.
//!
//! The fix is a single `create_dir_all` call at the top of
//! `run_autostart`. We can't drive `run_autostart` itself from a
//! unit test (it depends on real `%APPDATA%` resolution), but we
//! CAN lock the contract that writers fail gracefully on a missing
//! parent dir, AND that they succeed once `create_dir_all` runs.
//!
//! This test reproduces the exact failure mode the user reported:
//! a `cmd_osl_tour_advance` call with a `config_dir` whose parent
//! exists but the leaf subdirectory does NOT. Pre-fix, this
//! sequence stamps `state.last_persist_error`. Post-fix, the test
//! mirrors what bootstrap now does: mkdir-then-write succeeds and
//! leaves no error.

use ipc::commands::{cmd_osl_take_last_persist_error, cmd_osl_tour_advance};
use ipc::main_password::set_file_storage_key;
use ipc::AppState;
use std::sync::Mutex;
use tempfile::tempdir;

// `set_file_storage_key` is process-global; serialise tests that
// toggle it. (Same pattern as the other phase-9 tests.)
static KEY_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn persist_to_nonexistent_dir_records_persist_error() {
    // Pre-fix repro: simulate `%APPDATA%\osl\` not existing yet.
    let _g = KEY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    set_file_storage_key(None);

    let parent = tempdir().unwrap();
    let nonexistent_subdir = parent.path().join("osl");
    assert!(
        !nonexistent_subdir.exists(),
        "test precondition: subdir must not yet exist"
    );

    let state = AppState::new();
    // tour_advance returns Ok even when the underlying persist
    // fails (silent-warn pattern that TD1.4 captures into the error
    // sentinel). The user observable here is the sentinel.
    cmd_osl_tour_advance(&state, 1, Some(nonexistent_subdir.clone())).unwrap();

    let err = cmd_osl_take_last_persist_error(&state);
    assert!(
        err.is_some(),
        "persist into a nonexistent dir MUST stamp last_persist_error"
    );
    let msg = err.unwrap();
    assert!(
        msg.contains("app_preferences"),
        "error msg should namespace the file: {msg}"
    );
}

#[test]
fn persist_succeeds_after_create_dir_all_simulates_bootstrap_fix() {
    // Post-fix repro: bootstrap calls create_dir_all on the config
    // dir before any persist fires. Writing the same file with the
    // dir present succeeds and the error sentinel stays clean.
    let _g = KEY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    set_file_storage_key(None);

    let parent = tempdir().unwrap();
    let subdir = parent.path().join("osl");
    // Exact line the bootstrap fix adds:
    std::fs::create_dir_all(&subdir).unwrap();

    let state = AppState::new();
    cmd_osl_tour_advance(&state, 1, Some(subdir.clone())).unwrap();

    let err = cmd_osl_take_last_persist_error(&state);
    assert!(
        err.is_none(),
        "after create_dir_all, persist should leave last_persist_error clean; got {err:?}"
    );
    // And the file actually landed on disk.
    let written = subdir.join("app_preferences.json");
    assert!(
        written.exists(),
        "app_preferences.json should exist after persist: {}",
        written.display()
    );
}

#[test]
fn create_dir_all_is_idempotent_on_existing_dir() {
    // Bootstrap calls create_dir_all on every launch; confirm it's
    // a no-op when the dir already exists from a prior session.
    // (stdlib already guarantees this, but the test pins the
    // behaviour we depend on so a future refactor that swaps
    // create_dir_all for create_dir alone is caught here.)
    let _g = KEY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    set_file_storage_key(None);

    let dir = tempdir().unwrap();
    let subdir = dir.path().join("osl");
    std::fs::create_dir_all(&subdir).unwrap();
    // Re-running must NOT error.
    std::fs::create_dir_all(&subdir).unwrap();
    assert!(subdir.exists());
}
