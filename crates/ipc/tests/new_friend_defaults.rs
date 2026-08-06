use ipc::commands::{cmd_osl_get_new_friend_defaults, cmd_osl_save_new_friend_defaults};
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
fn direct_defaults_query_returns_all_three_new_friend_values() {
    let state = AppState::new();

    let defaults = cmd_osl_get_new_friend_defaults(&state).expect("direct defaults query");
    println!(
        "TASK0247 new_friend_defaults.account_reach={}",
        defaults.account_reach
    );
    println!(
        "TASK0247 new_friend_defaults.auto_whitelist={}",
        defaults.auto_whitelist
    );
    println!(
        "TASK0247 new_friend_defaults.verification_warnings={}",
        defaults.verification_warnings
    );

    assert_eq!(defaults.account_reach, "approved_chats_only");
    assert_eq!(defaults.auto_whitelist, "never");
    assert_eq!(defaults.verification_warnings, "enabled");
}

#[test]
fn changed_direct_query_returns_exact_saved_new_friend_values() {
    let _osl_serial = OSL_PROCESS_GLOBALS_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("tempdir");
    let _config_dir = use_temp_config_dir(dir.path());
    let state = AppState::new();

    let saved = cmd_osl_save_new_friend_defaults(
        &state,
        ipc::commands::NewFriendDefaultsDto {
            account_reach: "all_shared_chats".to_string(),
            auto_whitelist: "always".to_string(),
            verification_warnings: "disabled".to_string(),
        },
        Some(dir.path().to_path_buf()),
    )
    .expect("save changed new-friend defaults");

    println!("TASK0248 saved.account_reach={}", saved.account_reach);
    println!("TASK0248 saved.auto_whitelist={}", saved.auto_whitelist);
    println!(
        "TASK0248 saved.verification_warnings={}",
        saved.verification_warnings
    );

    let reloaded_state = AppState::new();
    *reloaded_state
        .app_preferences
        .lock()
        .expect("app_preferences mutex poisoned") =
        ipc::app_preferences::load_app_preferences(&dir.path().join("app_preferences.json"));

    let changed =
        cmd_osl_get_new_friend_defaults(&reloaded_state).expect("changed direct defaults query");
    println!(
        "TASK0248 changed_query.account_reach={}",
        changed.account_reach
    );
    println!(
        "TASK0248 changed_query.auto_whitelist={}",
        changed.auto_whitelist
    );
    println!(
        "TASK0248 changed_query.verification_warnings={}",
        changed.verification_warnings
    );

    assert_eq!(changed.account_reach, "all_shared_chats");
    assert_eq!(changed.auto_whitelist, "always");
    assert_eq!(changed.verification_warnings, "disabled");
}
