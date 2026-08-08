use ipc::app_preferences::{
    discovery_setting_choices, load_app_preferences, DiscoveryRepliesSwitch, DiscoverySetting,
};
use ipc::commands::{
    cmd_osl_list_discovery_setting_choices, cmd_osl_read_discovery_replies_switch,
    cmd_osl_read_discovery_setting, cmd_osl_save_discovery_setting,
    cmd_osl_set_discovery_replies_switch, cmd_osl_walk_discovery_publish_path,
};
use ipc::state::AppState;
use keystore::{set_active_account_dir, set_base_dir_override};
use std::path::Path;
use std::sync::Mutex;
use tempfile::tempdir;

static OSL_GLOBALS_LOCK: Mutex<()> = Mutex::new(());

struct ConfigDirGuard;

impl Drop for ConfigDirGuard {
    fn drop(&mut self) {
        set_active_account_dir(None);
        set_base_dir_override(None);
        ipc::main_password::set_file_storage_key(None);
    }
}

fn use_temp_config_dir(dir: &Path) -> ConfigDirGuard {
    set_active_account_dir(None);
    set_base_dir_override(Some(dir.to_path_buf()));
    ipc::main_password::set_file_storage_key(None);
    ConfigDirGuard
}

#[test]
fn task_4750_new_profile_reads_never_and_off_and_lists_choices_in_order() {
    let state = AppState::new();

    let setting = cmd_osl_read_discovery_setting(&state).expect("fresh discovery setting");
    let switch = cmd_osl_read_discovery_replies_switch(&state).expect("fresh discovery switch");
    let choices = cmd_osl_list_discovery_setting_choices().expect("discovery choices");

    println!("{setting}");
    println!("{switch}");
    println!("{}", choices.join(","));

    assert_eq!(setting, "never");
    assert_eq!(switch, "off");
    assert_eq!(
        choices,
        vec![
            "never".to_owned(),
            "allowed".to_owned(),
            "shared-room".to_owned(),
            "anyone".to_owned()
        ]
    );
}

#[test]
fn task_4750_setting_persists_and_refuses_fifth_value_with_exact_words() {
    let _serial = OSL_GLOBALS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempdir().expect("tempdir");
    let _config_dir = use_temp_config_dir(dir.path());
    let prefs_path = dir.path().join("app_preferences.json");
    let state = AppState::new();

    let saved = cmd_osl_save_discovery_setting(
        &state,
        "shared-room".to_owned(),
        Some(dir.path().to_path_buf()),
    )
    .expect("save discovery setting");
    let switch = cmd_osl_set_discovery_replies_switch(
        &state,
        "on".to_owned(),
        Some(dir.path().to_path_buf()),
    )
    .expect("save discovery replies switch");
    let loaded = load_app_preferences(&prefs_path);
    let bad =
        cmd_osl_save_discovery_setting(&state, "fifth".to_owned(), Some(dir.path().to_path_buf()))
            .expect_err("fifth value refused");

    println!("{bad}");
    println!("persisted_setting={}", loaded.discovery_setting.as_str());
    println!("persisted_switch={}", loaded.discovery_replies.as_str());

    assert_eq!(saved, "shared-room");
    assert_eq!(switch, "on");
    assert_eq!(loaded.discovery_setting, DiscoverySetting::SharedRoom);
    assert_eq!(loaded.discovery_replies, DiscoveryRepliesSwitch::On);
    assert_eq!(bad, "unknown discovery setting fifth");
}

#[test]
fn task_4750_master_off_skips_whole_publish_path_even_for_anyone() {
    let state = AppState::new();
    cmd_osl_save_discovery_setting(&state, "anyone".to_owned(), None)
        .expect("save permissive discovery setting");

    let report = cmd_osl_walk_discovery_publish_path(&state).expect("walk publish path");

    println!("{} cards", report.cards_written);
    println!("{}", report.status);

    assert_eq!(report.cards_written, 0);
    assert_eq!(report.answers_written, 0);
    assert_eq!(report.status, "skipped: discovery replies are off");
}

#[test]
fn task_4750_shipped_default_check_prints_found_value() {
    let found = DiscoverySetting::default().as_str();
    println!("{found}");
    assert_eq!(found, "never");
    assert_eq!(
        discovery_setting_choices(),
        vec![
            "never".to_owned(),
            "allowed".to_owned(),
            "shared-room".to_owned(),
            "anyone".to_owned()
        ]
    );
}
