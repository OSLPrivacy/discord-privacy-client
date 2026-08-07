//! Task 3154: persisted ask-before-irreversible-actions choice.

use ipc::app_preferences::{
    load_app_preferences, AppPreferences, AskBeforeIrreversibleActionsChoice,
};
use ipc::commands::{
    cmd_osl_get_ask_before_irreversible_actions_choice,
    cmd_osl_set_ask_before_irreversible_actions_choice,
};
use ipc::AppState;
use tempfile::tempdir;

#[test]
fn task_3154_fresh_reads_on_direct_command_saves_off_and_bad_value_is_refused() {
    ipc::main_password::set_file_storage_key(Some([0x54; 32]));

    let state = AppState::new();
    let dir = tempdir().unwrap();
    let path = dir.path().join("app_preferences.json");

    let fresh = cmd_osl_get_ask_before_irreversible_actions_choice(&state).unwrap();
    assert_eq!(fresh, AskBeforeIrreversibleActionsChoice::On);

    let saved = cmd_osl_set_ask_before_irreversible_actions_choice(
        &state,
        "off".to_owned(),
        Some(dir.path().to_path_buf()),
    )
    .unwrap();
    assert_eq!(saved, AskBeforeIrreversibleActionsChoice::Off);

    let read_back = cmd_osl_get_ask_before_irreversible_actions_choice(&state).unwrap();
    let on_disk = load_app_preferences(&path).ask_before_irreversible_actions;
    assert_eq!(read_back, AskBeforeIrreversibleActionsChoice::Off);
    assert_eq!(on_disk, AskBeforeIrreversibleActionsChoice::Off);

    let restart = AppState::new();
    *restart.app_preferences.lock().unwrap() = load_app_preferences(&path);
    let restart_read_back = cmd_osl_get_ask_before_irreversible_actions_choice(&restart).unwrap();
    assert_eq!(restart_read_back, AskBeforeIrreversibleActionsChoice::Off);

    let bad = cmd_osl_set_ask_before_irreversible_actions_choice(
        &state,
        "sometimes".to_owned(),
        Some(dir.path().to_path_buf()),
    )
    .unwrap_err();
    assert_eq!(
        bad,
        "OSL: invalid ask-before-irreversible-actions choice 'sometimes'"
    );
    assert_eq!(
        cmd_osl_get_ask_before_irreversible_actions_choice(&state).unwrap(),
        AskBeforeIrreversibleActionsChoice::Off
    );
    assert_eq!(
        load_app_preferences(&path).ask_before_irreversible_actions,
        AskBeforeIrreversibleActionsChoice::Off
    );

    let missing = tempdir().unwrap().path().join("app_preferences.json");
    let fresh_setup_from_missing_file = load_app_preferences(&missing);
    assert_eq!(
        fresh_setup_from_missing_file,
        AppPreferences {
            ask_before_irreversible_actions: AskBeforeIrreversibleActionsChoice::On,
            ..Default::default()
        }
    );

    println!(
        "TASK3154 fresh_setup={} direct_command=cmd_osl_set_ask_before_irreversible_actions_choice saved={} read_back={} restart_read_back={} bad_value_refused={} bad_value_stored={}",
        fresh.as_str(),
        saved.as_str(),
        read_back.as_str(),
        restart_read_back.as_str(),
        bad,
        load_app_preferences(&path).ask_before_irreversible_actions.as_str()
    );

    ipc::main_password::set_file_storage_key(None);
}
