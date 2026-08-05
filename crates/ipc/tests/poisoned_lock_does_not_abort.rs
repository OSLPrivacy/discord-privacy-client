//! Regression test for the amplifier half of defect D5.
//!
//! A background panic (D5's was the reqwest/Tokio runtime-drop panic) poisons
//! whatever `AppState` mutex the panicking frame held. Every later
//! `lock().expect(...)` on that mutex then panicked too — and when the next
//! toucher was a Tauri command invoked from the webview's FFI callback,
//! unwinding out of that frame is not permitted and the process aborted. One
//! survivable panic became a crash to desktop.
//!
//! `cmd_status` is the observed victim (it was `crates/ipc/src/commands.rs`
//! line 2122 pre-fix) and it reads two of the three whole-value slots, so it
//! is the natural probe: a poisoned `identity` or `keyserver` mutex must not
//! stop it returning, and the value it returns must still be the real one.
//!
//! Note what these tests do NOT claim: poisoning is tolerated only for the
//! `Mutex<Option<T>>` slots that are written by whole-value assignment. The
//! aggregate state (peer_map, whitelist, sender keys, message store) is
//! mutated in place and deliberately still fails loudly — see the note above
//! the accessors in `crates/ipc/src/state.rs`.

use ipc::commands::cmd_status;
use ipc::AppState;

const TEST_USER_ID: &str = "147700845179948241";

/// Poison `mutex_of(state)` by panicking while its guard is held, exactly the
/// way a panic inside a command frame does.
fn poison_with<F>(state: &AppState, lock_it: F)
where
    F: FnOnce(&AppState) + Send,
{
    let previous_hook = std::panic::take_hook();
    // The deliberate panic below would otherwise print a scary backtrace and
    // make a passing run look like a failing one.
    std::panic::set_hook(Box::new(|_| {}));
    let outcome = std::thread::scope(|scope| scope.spawn(|| lock_it(state)).join());
    std::panic::set_hook(previous_hook);
    assert!(
        outcome.is_err(),
        "test precondition: the poisoning thread must actually panic"
    );
}

#[test]
fn cmd_status_survives_a_poisoned_identity_mutex() {
    let state = AppState::new();
    state.install_identity(keystore::generate_identity(TEST_USER_ID.to_owned()));

    poison_with(&state, |state| {
        let _guard = state.identity.lock().expect("uncontended lock");
        panic!("stand-in for the runtime-drop panic that poisoned this mutex");
    });
    assert!(
        state.identity.lock().is_err(),
        "test precondition: the identity mutex must now be poisoned"
    );

    // Pre-fix this aborted the process; the abort is the thing under test, so
    // simply returning is the pass condition.
    let status = cmd_status(&state);
    assert!(
        status.identity_loaded,
        "the identity behind a poisoned guard is intact and must still be reported"
    );
    assert_eq!(
        status.user_id.as_deref(),
        Some(TEST_USER_ID),
        "recovery must hand back the real identity, not a blank one"
    );
}

#[test]
fn cmd_status_survives_a_poisoned_keyserver_mutex() {
    let state = AppState::new();
    state.install_identity(keystore::generate_identity(TEST_USER_ID.to_owned()));

    poison_with(&state, |state| {
        let _guard = state.keyserver.lock().expect("uncontended lock");
        panic!("stand-in for a panic taken while the keyserver slot was held");
    });
    assert!(
        state.keyserver.lock().is_err(),
        "test precondition: the keyserver mutex must now be poisoned"
    );

    let status = cmd_status(&state);
    assert!(
        status.identity_loaded,
        "a poisoned keyserver slot must not stop the status read"
    );
}

#[test]
fn identity_remains_writable_after_its_mutex_is_poisoned() {
    let state = AppState::new();
    state.install_identity(keystore::generate_identity(TEST_USER_ID.to_owned()));

    poison_with(&state, |state| {
        let _guard = state.identity.lock().expect("uncontended lock");
        panic!("stand-in for the runtime-drop panic that poisoned this mutex");
    });

    // The app must be able to keep working, not merely to read once: a
    // poisoned slot that cannot be cleared or replaced would still strand the
    // user on the next account switch.
    state.clear_identity();
    assert!(!cmd_status(&state).identity_loaded);

    state.install_identity(keystore::generate_identity("900000000000000031".to_owned()));
    assert_eq!(
        cmd_status(&state).user_id.as_deref(),
        Some("900000000000000031"),
        "a fresh identity must install over a poisoned slot"
    );
}
