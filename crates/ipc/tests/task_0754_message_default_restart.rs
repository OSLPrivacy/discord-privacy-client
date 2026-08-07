use ipc::app_preferences::{MessageDefaults, MessageScopeDefault, MessageWriterDefault, StegoMode};
use ipc::commands::{
    cmd_osl_set_app_preferences, cmd_osl_start_direct_new_message_plan, AppPreferencesDto,
};
use ipc::main_password::set_file_storage_key;
use ipc::state_reload::reload_encrypted_state_after_unlock;
use ipc::AppState;
use keystore::{set_active_account_dir, set_base_dir_override};
use serde::Serialize;
use std::path::Path;
use std::sync::Mutex;
use tempfile::tempdir;

static PROCESS_GLOBALS: Mutex<()> = Mutex::new(());

struct ConfigDirGuard;

impl Drop for ConfigDirGuard {
    fn drop(&mut self) {
        set_active_account_dir(None);
        set_base_dir_override(None);
        set_file_storage_key(None);
    }
}

fn use_temp_config_dir(dir: &Path) -> ConfigDirGuard {
    set_active_account_dir(None);
    set_base_dir_override(Some(dir.to_path_buf()));
    set_file_storage_key(Some([0x54; 32]));
    ConfigDirGuard
}

fn serialized_id<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value)
        .unwrap()
        .trim_matches('"')
        .to_owned()
}

#[test]
fn task_0754_direct_plan_reports_saved_message_defaults_after_restart() {
    let _serial = PROCESS_GLOBALS
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    let dir = tempdir().unwrap();
    let _config_dir = use_temp_config_dir(dir.path());

    let saved = MessageDefaults {
        scope: MessageScopeDefault::App,
        timer_seconds: 86_400,
        display_length_seconds: 45,
        writer: MessageWriterDefault::AiCovertext,
    };

    let first_process = AppState::new();
    cmd_osl_set_app_preferences(
        &first_process,
        AppPreferencesDto {
            stego_mode: StegoMode::Mode1,
            message_defaults: saved.clone(),
        },
        Some(dir.path().to_path_buf()),
    )
    .unwrap();

    let restarted_process = AppState::new();
    assert_eq!(
        restarted_process
            .app_preferences
            .lock()
            .unwrap()
            .message_defaults,
        MessageDefaults::default()
    );

    let reload = reload_encrypted_state_after_unlock(&restarted_process, dir.path()).unwrap();
    assert!(reload.app_prefs_loaded);
    assert!(
        reload.errors.is_empty(),
        "restart reload errors: {:?}",
        reload.errors
    );

    let plan = cmd_osl_start_direct_new_message_plan(&restarted_process).unwrap();
    println!("TASK0754 direct_new_message_plan.kind={}", plan.kind);
    println!(
        "TASK0754 direct_new_message_plan.scope={}",
        serialized_id(&plan.scope)
    );
    println!(
        "TASK0754 direct_new_message_plan.timer_seconds={}",
        plan.timer_seconds
    );
    println!(
        "TASK0754 direct_new_message_plan.display_length_seconds={}",
        plan.display_length_seconds
    );
    println!(
        "TASK0754 direct_new_message_plan.writer={}",
        serialized_id(&plan.writer)
    );

    assert_eq!(plan.kind, "direct");
    assert_eq!(plan.scope, saved.scope);
    assert_eq!(plan.timer_seconds, saved.timer_seconds);
    assert_eq!(plan.display_length_seconds, saved.display_length_seconds);
    assert_eq!(plan.writer, saved.writer);
}
