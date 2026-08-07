use ipc::app_preferences::load_app_preferences;
use ipc::commands::{
    cmd_osl_read_message_default_burn_scope, cmd_osl_read_message_default_cover_writing,
    cmd_osl_read_message_default_timer_seconds,
    cmd_osl_read_message_default_view_once_length_seconds, cmd_osl_save_message_defaults,
    MessageDefaultsDto,
};
use ipc::AppState;
use keystore::{set_active_account_dir, set_base_dir_override};
use std::path::Path;
use std::sync::Mutex;
use tempfile::tempdir;

static IO_LOCK: Mutex<()> = Mutex::new(());

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
fn task_0752_direct_reads_return_all_four_saved_message_defaults() {
    let _lock = IO_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempdir().unwrap();
    let _config_dir = use_temp_config_dir(dir.path());
    let path = dir.path().join("app_preferences.json");
    let writer = AppState::new();

    let saved = cmd_osl_save_message_defaults(
        &writer,
        MessageDefaultsDto {
            burn_scope: "app".to_owned(),
            timer_seconds: 86_400,
            view_once_length_seconds: 45,
            cover_writing: "ai_covertext".to_owned(),
        },
        Some(dir.path().to_path_buf()),
    )
    .unwrap();
    assert_eq!(saved.burn_scope, "app");
    assert_eq!(saved.timer_seconds, 86_400);
    assert_eq!(saved.view_once_length_seconds, 45);
    assert_eq!(saved.cover_writing, "ai_covertext");

    let reader = AppState::new();
    *reader.app_preferences.lock().unwrap() = load_app_preferences(&path);

    let burn_scope = cmd_osl_read_message_default_burn_scope(&reader).unwrap();
    let timer_seconds = cmd_osl_read_message_default_timer_seconds(&reader).unwrap();
    let view_once_length_seconds =
        cmd_osl_read_message_default_view_once_length_seconds(&reader).unwrap();
    let cover_writing = cmd_osl_read_message_default_cover_writing(&reader).unwrap();

    println!("TASK0752 burn_scope={burn_scope}");
    println!("TASK0752 timer_seconds={timer_seconds}");
    println!("TASK0752 view_once_length_seconds={view_once_length_seconds}");
    println!("TASK0752 cover_writing={cover_writing}");

    assert_eq!(burn_scope, "app");
    assert_eq!(timer_seconds, 86_400);
    assert_eq!(view_once_length_seconds, 45);
    assert_eq!(cover_writing, "ai_covertext");
}
