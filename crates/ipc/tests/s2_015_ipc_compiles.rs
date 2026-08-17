//! S2-015: `crates/ipc` must compile and link as an external crate, and the
//! production duress path must actually fire at the documented threshold —
//! not merely resolve a name that never triggers.
//!
//! Before this fix, `record_wrong_password_attempt_or_duress` called
//! `wrong_password_attempt_triggers_duress`, which was stubbed to always
//! return `false`. The crate compiled, `DuressTriggered` existed as a name,
//! but ten wrong passwords in a row would never wipe the device.

use ipc::main_password::{
    record_wrong_password_attempt_or_duress, set_file_storage_key,
    wrong_password_attempt_triggers_duress, LockoutState, WrongPasswordAttemptAction,
    DURESS_WRONG_PASSWORD_ATTEMPT_LIMIT,
};
use ipc::state::AppState;

#[test]
fn wrong_password_attempt_triggers_duress_is_false_below_threshold_and_true_at_it() {
    assert!(!wrong_password_attempt_triggers_duress(
        DURESS_WRONG_PASSWORD_ATTEMPT_LIMIT - 1
    ));
    assert!(wrong_password_attempt_triggers_duress(
        DURESS_WRONG_PASSWORD_ATTEMPT_LIMIT
    ));
    assert!(wrong_password_attempt_triggers_duress(
        DURESS_WRONG_PASSWORD_ATTEMPT_LIMIT + 1
    ));
}

#[test]
fn the_tenth_consecutive_wrong_password_actually_reaches_duress_triggered() {
    set_file_storage_key(None);
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::new_with_production_duress_engine(dir.path().to_path_buf());
    state.install_identity(keystore::generate_identity(
        "s2-015-compile-proof-owner".to_owned(),
    ));

    let mut lock = LockoutState {
        password_failed_attempts: DURESS_WRONG_PASSWORD_ATTEMPT_LIMIT - 1,
        ..Default::default()
    };

    let action = record_wrong_password_attempt_or_duress(&state, &mut lock, 0)
        .expect("recording the tenth wrong attempt must not error");

    assert_eq!(
        action,
        WrongPasswordAttemptAction::DuressTriggered {
            attempts_used: DURESS_WRONG_PASSWORD_ATTEMPT_LIMIT,
        },
        "the tenth consecutive wrong password must reach the production duress engine, \
         not silently stay Wrong"
    );
    set_file_storage_key(None);
}
