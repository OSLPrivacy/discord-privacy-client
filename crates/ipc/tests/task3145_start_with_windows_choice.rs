use ipc::app_preferences::{load_app_preferences, StartWithWindowsChoice};
use ipc::commands::{
    cmd_osl_get_start_with_windows_choice, cmd_osl_save_start_with_windows_choice,
};
use ipc::AppState;
use keystore::{set_active_account_dir, set_base_dir_override};
use std::sync::Mutex;

static OSL_PROCESS_GLOBALS_LOCK: Mutex<()> = Mutex::new(());

struct ConfigDirGuard;

impl Drop for ConfigDirGuard {
    fn drop(&mut self) {
        set_active_account_dir(None);
        set_base_dir_override(None);
        ipc::main_password::set_file_storage_key(None);
    }
}

fn use_temp_config_dir(dir: &std::path::Path) -> ConfigDirGuard {
    set_active_account_dir(None);
    set_base_dir_override(Some(dir.to_path_buf()));
    ipc::main_password::set_file_storage_key(None);
    ConfigDirGuard
}

#[test]
fn task3145_direct_commands_save_read_and_refuse_start_with_windows_choice() {
    let _osl_serial = OSL_PROCESS_GLOBALS_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("tempdir");
    let _config_dir = use_temp_config_dir(dir.path());
    let prefs_path = dir.path().join("app_preferences.json");
    let state = AppState::new();

    let saved_on = cmd_osl_save_start_with_windows_choice(
        &state,
        "on".to_string(),
        Some(dir.path().to_path_buf()),
    )
    .expect("save on");
    println!("TASK3145 direct_command=cmd_osl_save_start_with_windows_choice");
    println!("TASK3145 save_on.return={saved_on}");
    assert_eq!(saved_on, "on");
    assert_eq!(
        load_app_preferences(&prefs_path).start_with_windows,
        StartWithWindowsChoice::On
    );

    let reloaded_state = AppState::new();
    *reloaded_state
        .app_preferences
        .lock()
        .expect("app_preferences mutex poisoned") = load_app_preferences(&prefs_path);
    let read_on = cmd_osl_get_start_with_windows_choice(&reloaded_state).expect("read after save");
    println!("TASK3145 read_after_save={read_on}");
    assert_eq!(read_on, "on");

    let saved_off = cmd_osl_save_start_with_windows_choice(
        &reloaded_state,
        "off".to_string(),
        Some(dir.path().to_path_buf()),
    )
    .expect("save off");
    println!("TASK3145 save_off.return={saved_off}");
    assert_eq!(saved_off, "off");
    assert_eq!(
        load_app_preferences(&prefs_path).start_with_windows,
        StartWithWindowsChoice::Off
    );

    let bad_error = cmd_osl_save_start_with_windows_choice(
        &reloaded_state,
        "sometimes".to_string(),
        Some(dir.path().to_path_buf()),
    )
    .expect_err("bad value refused");
    let after_bad =
        cmd_osl_get_start_with_windows_choice(&reloaded_state).expect("read after bad value");
    println!("TASK3145 bad_value_refused=true");
    println!("TASK3145 bad_value.error={bad_error}");
    println!("TASK3145 after_bad.read={after_bad}");
    assert!(
        bad_error.contains("valid choices: on, off"),
        "bad-value error must name the valid choices: {bad_error}"
    );
    assert_eq!(after_bad, "off");
}
