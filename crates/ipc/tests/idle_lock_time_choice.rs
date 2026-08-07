use ipc::app_preferences::load_app_preferences;
use ipc::commands::{cmd_osl_read_idle_lock_time_choice, cmd_osl_save_idle_lock_time_choice};
use ipc::session_lock;
use ipc::state::AppState;
use std::sync::Mutex;
use std::time::Duration;
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

fn observed_lock_state(state: &AppState) -> &'static str {
    if session_lock::session_holds_live_secrets(state) {
        "unlocked"
    } else {
        "locked"
    }
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

#[test]
#[ignore = "TASK3153 intentionally waits 70 seconds per run, three times"]
fn chosen_one_minute_idle_time_locks_after_70_seconds_three_times() {
    let _key_lock = KEY_LOCK.lock().unwrap_or_else(|error| error.into_inner());

    for run in 1..=3 {
        let _file_storage_key = install_file_storage_key();
        let dir = tempfile::TempDir::new().unwrap();
        let state = AppState::new();
        state.install_identity(keystore::generate_identity(format!(
            "task3153-idle-owner-{run}"
        )));

        let saved = cmd_osl_save_idle_lock_time_choice(
            &state,
            "one minute".to_string(),
            Some(dir.path().to_path_buf()),
        )
        .unwrap();
        assert_eq!(saved.label, "one minute");
        assert_eq!(saved.seconds, Some(60));

        session_lock::arm_idle_lock();
        std::thread::sleep(Duration::from_secs(50));
        assert!(
            !session_lock::run_idle_session_lock(&state),
            "run {run}: one minute idle lock fired before 50 seconds"
        );
        let state_at_50_seconds = observed_lock_state(&state);

        std::thread::sleep(Duration::from_secs(20));
        assert!(
            session_lock::run_idle_session_lock(&state),
            "run {run}: one minute idle lock did not fire by 70 seconds"
        );
        let state_at_70_seconds = observed_lock_state(&state);

        println!(
            "TASK3153 run={} chosen_idle_label=\"{}\" chosen_idle_seconds={} state_at_50_seconds={} state_at_70_seconds={}",
            run,
            saved.label,
            saved.seconds.unwrap(),
            state_at_50_seconds,
            state_at_70_seconds
        );

        assert_eq!(
            state_at_50_seconds, "unlocked",
            "run {run}: expected unlocked at 50 seconds"
        );
        assert_eq!(
            state_at_70_seconds, "locked",
            "run {run}: expected locked at 70 seconds"
        );

        session_lock::disarm_idle_lock();
    }
}
