use ipc::app_preferences::load_app_preferences;
use ipc::commands::{
    cmd_osl_read_next_generation_message_policy, cmd_osl_save_next_generation_message_policy,
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

fn install_file_storage_key() -> FileStorageKeyGuard {
    ipc::main_password::set_file_storage_key(Some([0x71; 32]));
    FileStorageKeyGuard
}

fn fresh_state_from_saved_preferences(dir: &std::path::Path) -> AppState {
    let state = AppState::new();
    let prefs = load_app_preferences(&dir.join("app_preferences.json"));
    let next_generation_message_policy = prefs.next_generation_message_policy;
    *state.app_preferences.lock().unwrap() = prefs;
    state.set_rn_wire_in_enabled(next_generation_message_policy.rn_wire_in_enabled());
    state
}

#[test]
fn fresh_direct_read_returns_saved_next_generation_policy_and_refuses_unknown_choice() {
    let _key_lock = KEY_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let _file_storage_key = install_file_storage_key();
    let dir = tempfile::TempDir::new().unwrap();
    let state = AppState::new();

    let never_saved = cmd_osl_read_next_generation_message_policy(&AppState::new()).unwrap();
    assert_eq!(never_saved.choice, "off");

    let saved_on = cmd_osl_save_next_generation_message_policy(
        &state,
        "on".to_string(),
        Some(dir.path().to_path_buf()),
    )
    .unwrap();
    assert_eq!(saved_on.choice, "on");
    let fresh_on_state = fresh_state_from_saved_preferences(dir.path());
    let fresh_on = cmd_osl_read_next_generation_message_policy(&fresh_on_state).unwrap();
    assert_eq!(fresh_on.choice, "on");
    assert!(fresh_on_state.rn_wire_in_enabled());

    let invalid = cmd_osl_save_next_generation_message_policy(
        &state,
        "neither-on-nor-off".to_string(),
        Some(dir.path().to_path_buf()),
    )
    .unwrap_err();
    assert!(invalid.contains("unknown next-generation message policy"));
    let fresh_after_invalid_state = fresh_state_from_saved_preferences(dir.path());
    let fresh_after_invalid =
        cmd_osl_read_next_generation_message_policy(&fresh_after_invalid_state).unwrap();
    assert_eq!(fresh_after_invalid.choice, "on");
    assert!(fresh_after_invalid_state.rn_wire_in_enabled());

    let saved_off = cmd_osl_save_next_generation_message_policy(
        &state,
        "off".to_string(),
        Some(dir.path().to_path_buf()),
    )
    .unwrap();
    assert_eq!(saved_off.choice, "off");
    let fresh_off_state = fresh_state_from_saved_preferences(dir.path());
    let fresh_off = cmd_osl_read_next_generation_message_policy(&fresh_off_state).unwrap();
    assert_eq!(fresh_off.choice, "off");
    assert!(!fresh_off_state.rn_wire_in_enabled());

    println!(
        "TASK0712 never_saved={} saved_on_fresh={} invalid_refused={} after_invalid={} saved_off_fresh={}",
        never_saved.choice,
        fresh_on.choice,
        invalid,
        fresh_after_invalid.choice,
        fresh_off.choice
    );
}
