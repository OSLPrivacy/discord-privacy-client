use ipc::commands::{
    cmd_osl_get_new_friend_defaults, cmd_osl_save_new_friend_defaults, NewFriendDefaultsDto,
};
use ipc::AppState;
use keystore::{set_active_account_dir, set_base_dir_override};
use std::fs;
use std::path::Path;
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
    assert_eq!(defaults.verification_warnings, "always");
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
        NewFriendDefaultsDto {
            account_reach: "all_shared_chats".to_string(),
            auto_whitelist: "always".to_string(),
            verification_warnings: "never".to_string(),
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
    assert_eq!(changed.verification_warnings, "never");
}

fn read_decrypted_preferences(path: &Path) -> Vec<u8> {
    let blob = fs::read(path).expect("read encrypted app_preferences.json");
    ipc::main_password::maybe_decrypt_file(path, &blob).expect("decrypt app_preferences.json")
}

fn warning_default_count(bytes: &[u8]) -> usize {
    String::from_utf8_lossy(bytes)
        .matches("\"new_friend_verification_warnings\"")
        .count()
}

fn readable_json_field(bytes: &[u8], field: &str) -> String {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).expect("app_preferences.json is JSON");
    value[field]
        .as_str()
        .expect("field is a readable string")
        .to_string()
}

#[test]
fn task_0251_invalid_warning_choice_refuses_and_preserves_default_record() {
    let _osl_serial = OSL_PROCESS_GLOBALS_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("tempdir");
    let _config_dir = use_temp_config_dir(dir.path());
    ipc::main_password::set_file_storage_key(Some([0x25; 32]));
    let path = dir.path().join("app_preferences.json");
    let seed = serde_json::json!({
        "audit_marker": "MINT-0251",
        "new_friend_verification_warnings": "always"
    });
    let seed_bytes = serde_json::to_vec_pretty(&seed).expect("serialize seed");
    let encrypted_seed = ipc::main_password::maybe_encrypt(&seed_bytes).expect("encrypt seed");
    fs::write(&path, encrypted_seed).expect("write encrypted seed");

    let state = AppState::new();
    *state
        .app_preferences
        .lock()
        .expect("app_preferences mutex poisoned") =
        ipc::app_preferences::load_app_preferences(&path);

    let before_bytes = read_decrypted_preferences(&path);
    let marker_before = readable_json_field(&before_bytes, "audit_marker");
    let count_before = warning_default_count(&before_bytes);
    let before_defaults = cmd_osl_get_new_friend_defaults(&state).expect("read seeded defaults");
    println!("TASK0251 marker.readable={marker_before}");
    println!(
        "TASK0251 seed.warning_choice={}",
        before_defaults.verification_warnings
    );
    println!("TASK0251 default_count.before={count_before}");

    assert_eq!(marker_before, "MINT-0251");
    assert_eq!(before_defaults.verification_warnings, "always");
    assert_eq!(count_before, 1);

    let good = cmd_osl_save_new_friend_defaults(
        &state,
        NewFriendDefaultsDto {
            account_reach: before_defaults.account_reach.clone(),
            auto_whitelist: before_defaults.auto_whitelist.clone(),
            verification_warnings: "never".to_string(),
        },
        Some(dir.path().to_path_buf()),
    )
    .expect("save never warning choice");
    let after_good_bytes = read_decrypted_preferences(&path);
    let count_after_good = warning_default_count(&after_good_bytes);
    println!(
        "TASK0251 good_save.warning_choice={}",
        good.verification_warnings
    );
    println!("TASK0251 default_count.after_good={count_after_good}");

    assert_eq!(good.verification_warnings, "never");
    assert_eq!(count_after_good, 1);
    assert!(String::from_utf8_lossy(&after_good_bytes).contains("MINT-0251"));
    assert!(String::from_utf8_lossy(&after_good_bytes).contains("\"never\""));

    let bad = cmd_osl_save_new_friend_defaults(
        &state,
        NewFriendDefaultsDto {
            account_reach: good.account_reach.clone(),
            auto_whitelist: good.auto_whitelist.clone(),
            verification_warnings: "unknown".to_string(),
        },
        Some(dir.path().to_path_buf()),
    );
    let refusal = bad.expect_err("unknown warning choice must be refused");
    let after_bad_bytes = read_decrypted_preferences(&path);
    let count_after_bad = warning_default_count(&after_bad_bytes);
    let unchanged_after_bad = after_bad_bytes == after_good_bytes;
    let marker_after_bad = readable_json_field(&after_bad_bytes, "audit_marker");
    let warning_after_bad =
        readable_json_field(&after_bad_bytes, "new_friend_verification_warnings");
    println!("TASK0251 bad_refusal={refusal}");
    println!("TASK0251 default_count.after_bad={count_after_bad}");
    println!("TASK0251 bytes_after_bad_equal_after_good={unchanged_after_bad}");
    println!("TASK0251 final.warning_choice={warning_after_bad}");
    println!("TASK0251 final.marker={marker_after_bad}");

    assert!(refusal.contains("unknown warning choice"));
    assert_eq!(count_after_bad, 1);
    assert!(unchanged_after_bad);
    assert_eq!(warning_after_bad, "never");
    assert_eq!(marker_after_bad, "MINT-0251");
}
