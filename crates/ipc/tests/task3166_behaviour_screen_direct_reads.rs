use ipc::app_preferences::load_app_preferences;
use ipc::commands::{
    cmd_osl_get_ask_before_irreversible_actions_choice, cmd_osl_get_follow_active_app_choice,
    cmd_osl_get_language_choice, cmd_osl_get_start_with_windows_choice,
    cmd_osl_read_alert_mode_choice, cmd_osl_read_idle_lock_time_choice,
    cmd_osl_save_alert_mode_choice, cmd_osl_save_idle_lock_time_choice,
    cmd_osl_save_language_choice, cmd_osl_save_start_with_windows_choice,
    cmd_osl_set_ask_before_irreversible_actions_choice, cmd_osl_set_follow_active_app_choice,
};
use ipc::state::AppState;
use std::sync::Mutex;

static KEY_LOCK: Mutex<()> = Mutex::new(());

struct FileStorageKeyGuard;

impl Drop for FileStorageKeyGuard {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(None);
    }
}

fn fresh_state_from_saved_preferences(dir: &std::path::Path) -> AppState {
    let state = AppState::new();
    let prefs = load_app_preferences(&dir.join("app_preferences.json"));
    *state.app_preferences.lock().unwrap() = prefs;
    state
}

/// TASK 3166 - the Behaviour screen must show all six settings with their
/// saved values, and those values must match what each setting's direct
/// read command returns. This test saves a chosen (non-default) value for
/// each of the six settings through its direct save/set command, then reads
/// each back through a *fresh* AppState loaded straight from the saved
/// preferences file (the same path the screen takes on open), and asserts
/// the two match. The printed `TASK3166 *.saved=` / `*.read=` pairs are the
/// exact values the Behaviour screen test below renders and asserts against.
#[test]
fn task3166_six_behaviour_settings_read_back_what_was_saved() {
    let _key_lock = KEY_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    ipc::main_password::set_file_storage_key(Some([0x66u8; 32]));
    let _key_guard = FileStorageKeyGuard;

    let dir = tempfile::TempDir::new().unwrap();
    let write_state = AppState::new();
    let config_dir = Some(dir.path().to_path_buf());

    let saved_start_with_windows = cmd_osl_save_start_with_windows_choice(
        &write_state,
        "on".to_string(),
        config_dir.clone(),
    )
    .unwrap();
    let saved_idle = cmd_osl_save_idle_lock_time_choice(
        &write_state,
        "never".to_string(),
        config_dir.clone(),
    )
    .unwrap();
    let saved_ask_before = cmd_osl_set_ask_before_irreversible_actions_choice(
        &write_state,
        "off".to_string(),
        config_dir.clone(),
    )
    .unwrap();
    let saved_alert_mode = cmd_osl_save_alert_mode_choice(
        &write_state,
        "quiet".to_string(),
        config_dir.clone(),
    )
    .unwrap();
    let saved_language =
        cmd_osl_save_language_choice(&write_state, "es".to_string(), config_dir.clone()).unwrap();
    let saved_follow_active_app =
        cmd_osl_set_follow_active_app_choice(&write_state, "on", config_dir.clone()).unwrap();

    println!("TASK3166 start_with_windows.saved={saved_start_with_windows}");
    println!(
        "TASK3166 idle_lock_time_choice.saved={}:{}:{}",
        saved_idle.choice,
        saved_idle
            .seconds
            .map(|s| s.to_string())
            .unwrap_or_else(|| "none".to_string()),
        saved_idle.label
    );
    println!(
        "TASK3166 ask_before_irreversible_actions.saved={}",
        saved_ask_before.as_str()
    );
    println!("TASK3166 alert_mode_choice.saved={}", saved_alert_mode.mode);
    println!("TASK3166 language.saved={saved_language}");
    println!("TASK3166 follow_active_app_choice.saved={saved_follow_active_app}");

    // Fresh state loaded from disk, exactly as the Behaviour screen would
    // read on open, via each setting's own direct read command.
    let read_state = fresh_state_from_saved_preferences(dir.path());

    let read_start_with_windows =
        cmd_osl_get_start_with_windows_choice(&read_state).unwrap();
    let read_idle = cmd_osl_read_idle_lock_time_choice(&read_state).unwrap();
    let read_ask_before =
        cmd_osl_get_ask_before_irreversible_actions_choice(&read_state).unwrap();
    let read_alert_mode = cmd_osl_read_alert_mode_choice(&read_state).unwrap();
    let read_language = cmd_osl_get_language_choice(&read_state).unwrap();
    let read_follow_active_app = cmd_osl_get_follow_active_app_choice(&read_state).unwrap();

    println!("TASK3166 start_with_windows.read={read_start_with_windows}");
    println!(
        "TASK3166 idle_lock_time_choice.read={}:{}:{}",
        read_idle.choice,
        read_idle
            .seconds
            .map(|s| s.to_string())
            .unwrap_or_else(|| "none".to_string()),
        read_idle.label
    );
    println!(
        "TASK3166 ask_before_irreversible_actions.read={}",
        read_ask_before.as_str()
    );
    println!("TASK3166 alert_mode_choice.read={}", read_alert_mode.mode);
    println!("TASK3166 language.read={read_language}");
    println!("TASK3166 follow_active_app_choice.read={read_follow_active_app}");

    assert_eq!(read_start_with_windows, saved_start_with_windows);
    assert_eq!(read_idle.choice, saved_idle.choice);
    assert_eq!(read_idle.seconds, saved_idle.seconds);
    assert_eq!(read_idle.label, saved_idle.label);
    assert_eq!(read_ask_before, saved_ask_before);
    assert_eq!(read_alert_mode.mode, saved_alert_mode.mode);
    assert_eq!(read_language, saved_language);
    assert_eq!(read_follow_active_app, saved_follow_active_app);

    println!("TASK3166 all_six_reads_match_saved=true");
}
