use ipc::commands::{
    cmd_osl_read_verification_warning_choice, cmd_osl_save_verification_warning_choice,
};
use ipc::main_password::set_file_storage_key;
use ipc::state::AppState;
use std::sync::Mutex;

static KEY_LOCK: Mutex<()> = Mutex::new(());

struct FileStorageKeyGuard;

impl Drop for FileStorageKeyGuard {
    fn drop(&mut self) {
        set_file_storage_key(None);
    }
}

fn reloaded_state(dir: &std::path::Path) -> AppState {
    let loaded_state = AppState::new();
    *loaded_state.app_preferences.lock().unwrap() =
        ipc::app_preferences::load_app_preferences(&dir.join("app_preferences.json"));
    loaded_state
}

#[test]
fn task_0744_saves_and_reads_four_verification_warning_choices_and_refuses_unknown() {
    let _lock = KEY_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    set_file_storage_key(Some([0x74u8; 32]));
    let _key_guard = FileStorageKeyGuard;

    let dir = tempfile::tempdir().expect("tempdir");
    let saving_state = AppState::new();
    let mut saved_and_read = Vec::new();

    for expected in ["every time", "once", "before sending", "never"] {
        let saved = cmd_osl_save_verification_warning_choice(
            &saving_state,
            expected.to_string(),
            Some(dir.path().to_path_buf()),
        )
        .expect("save verification warning choice");
        println!("TASK0744 verification_warning.saved={}", saved.choice);

        let loaded_state = reloaded_state(dir.path());
        let read = cmd_osl_read_verification_warning_choice(&loaded_state)
            .expect("read verification warning choice");
        println!("TASK0744 verification_warning.read_back={}", read.choice);

        assert_eq!(saved.choice, expected);
        assert_eq!(read.choice, expected);
        saved_and_read.push(read.choice);
    }

    let unknown_result = cmd_osl_save_verification_warning_choice(
        &saving_state,
        "unknown".to_string(),
        Some(dir.path().to_path_buf()),
    );
    let refusal = unknown_result.expect_err("unknown verification warning choice must be refused");
    println!("TASK0744 verification_warning.unknown_refusal={refusal}");

    let loaded_after_unknown = reloaded_state(dir.path());
    let after_unknown = cmd_osl_read_verification_warning_choice(&loaded_after_unknown)
        .expect("read verification warning choice after unknown refusal");
    println!(
        "TASK0744 verification_warning.after_unknown_refusal_still={}",
        after_unknown.choice
    );

    assert_eq!(
        saved_and_read,
        ["every time", "once", "before sending", "never"]
    );
    assert!(refusal.contains("unknown verification warning choice"));
    assert_eq!(after_unknown.choice, "never");
}
