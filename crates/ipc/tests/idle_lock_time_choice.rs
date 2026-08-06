use ipc::app_preferences::load_app_preferences;
use ipc::commands::{cmd_osl_read_idle_lock_time_choice, cmd_osl_save_idle_lock_time_choice};
use ipc::state::AppState;
use std::sync::Mutex;

static KEY_LOCK: Mutex<()> = Mutex::new(());

struct FileStorageKeyGuard;

impl Drop for FileStorageKeyGuard {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(None);
    }
}

fn install_file_storage_key() -> FileStorageKeyGuard {
    ipc::main_password::set_file_storage_key(Some([0x31; 32]));
    FileStorageKeyGuard
}

fn fresh_state_from_saved_preferences(dir: &std::path::Path) -> AppState {
    let state = AppState::new();
    let prefs = load_app_preferences(&dir.join("app_preferences.json"));
    *state.app_preferences.lock().unwrap() = prefs;
    state
}

#[test]
fn direct_command_saves_one_minute_never_and_refuses_nonpositive_values() {
    let _key_lock = KEY_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let _file_storage_key = install_file_storage_key();
    let dir = tempfile::TempDir::new().unwrap();
    let state = AppState::new();

    let saved_one_minute = cmd_osl_save_idle_lock_time_choice(
        &state,
        "one minute".to_string(),
        Some(dir.path().to_path_buf()),
    )
    .unwrap();
    assert_eq!(saved_one_minute.label, "one minute");
    assert_eq!(saved_one_minute.seconds, Some(60));

    let one_minute_state = fresh_state_from_saved_preferences(dir.path());
    let read_one_minute = cmd_osl_read_idle_lock_time_choice(&one_minute_state).unwrap();
    assert_eq!(read_one_minute.label, "one minute");
    assert_eq!(read_one_minute.seconds, Some(60));

    let saved_never = cmd_osl_save_idle_lock_time_choice(
        &state,
        "never".to_string(),
        Some(dir.path().to_path_buf()),
    )
    .unwrap();
    assert_eq!(saved_never.choice, "never");
    assert_eq!(saved_never.seconds, None);

    let never_state = fresh_state_from_saved_preferences(dir.path());
    let read_never = cmd_osl_read_idle_lock_time_choice(&never_state).unwrap();
    assert_eq!(read_never.choice, "never");
    assert_eq!(read_never.seconds, None);

    let zero_refusal =
        cmd_osl_save_idle_lock_time_choice(&state, "0".to_string(), Some(dir.path().to_path_buf()))
            .unwrap_err();
    assert_eq!(
        zero_refusal,
        "OSL: idle lock time must be positive seconds or never"
    );

    let negative_refusal = cmd_osl_save_idle_lock_time_choice(
        &state,
        "-1".to_string(),
        Some(dir.path().to_path_buf()),
    )
    .unwrap_err();
    assert_eq!(
        negative_refusal,
        "OSL: idle lock time must be positive seconds or never"
    );

    println!(
        "TASK3151 direct_command=cmd_osl_save_idle_lock_time_choice saved_label={} saved_seconds={} read_label={} read_seconds={} never_saved={} never_read={} zero_refused={} negative_refused={}",
        saved_one_minute.label,
        saved_one_minute.seconds.unwrap(),
        read_one_minute.label,
        read_one_minute.seconds.unwrap(),
        saved_never.choice,
        read_never.choice,
        zero_refusal,
        negative_refusal
    );
}
