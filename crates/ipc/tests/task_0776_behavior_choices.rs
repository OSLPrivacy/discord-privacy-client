use ipc::commands::{cmd_osl_read_behaviour_choice, cmd_osl_save_behaviour_choice};
use ipc::state::AppState;
use ipc::state_reload::reload_encrypted_state_after_unlock;
use keystore::{set_active_account_dir, set_base_dir_override};
use std::path::Path;
use std::sync::Mutex;
use tempfile::tempdir;

static KEY_LOCK: Mutex<()> = Mutex::new(());

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
fn task_0776_restart_reads_all_saved_behaviour_choices_and_refuses_unknown_name() {
    let _serial = KEY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempdir().unwrap();
    let _config_dir = use_temp_config_dir(dir.path());
    let state = AppState::new();
    let saved = [
        ("position", "x=111,y=222,w=333,h=444"),
        ("remember place", "remember-last-window"),
        ("movement", "snap-to-current-screen"),
        ("tray picture", "contact-avatar"),
        ("sound", "soft-chime"),
        ("mute", "muted"),
        ("quiet hours", "22:15-06:45"),
    ];

    for (name, choice) in saved {
        let written = cmd_osl_save_behaviour_choice(
            &state,
            name.to_string(),
            choice.to_string(),
            Some(dir.path().to_path_buf()),
        )
        .unwrap();
        assert_eq!(written.name, name);
        assert_eq!(written.choice, choice);
    }

    let unknown = cmd_osl_save_behaviour_choice(
        &state,
        "unknown eighth name".to_string(),
        "should-not-store".to_string(),
        Some(dir.path().to_path_buf()),
    )
    .unwrap_err();
    assert!(unknown.contains("unknown behaviour choice"));

    ipc::main_password::set_file_storage_key(None);
    ipc::main_password::ensure_device_bound_fallback_file_storage_key(dir.path()).unwrap();
    let restarted = AppState::new();
    let report = reload_encrypted_state_after_unlock(&restarted, dir.path()).unwrap();
    assert!(report.app_prefs_loaded);
    assert!(
        report.errors.is_empty(),
        "restart reload errors: {:?}",
        report.errors
    );

    let mut proof = Vec::new();
    for (name, expected) in saved {
        let read = cmd_osl_read_behaviour_choice(&restarted, name.to_string()).unwrap();
        assert_eq!(read.name, name);
        assert_eq!(read.choice, expected);
        proof.push(format!("{name}={expected}"));
    }

    let read_unknown =
        cmd_osl_read_behaviour_choice(&restarted, "unknown eighth name".to_string()).unwrap_err();
    assert!(read_unknown.contains("unknown behaviour choice"));
    let stored_unknown = restarted
        .app_preferences
        .lock()
        .expect("app_preferences mutex poisoned")
        .behaviour_choices
        .contains_key("unknown eighth name");
    assert!(!stored_unknown);

    println!(
        "TASK_0776_BEHAVIOUR_CHOICES restart_loaded={} direct_read_count={} {} unknown_eighth_refused={} unknown_eighth_stored={}",
        report.app_prefs_loaded,
        proof.len(),
        proof.join(" | "),
        unknown,
        stored_unknown
    );
}
