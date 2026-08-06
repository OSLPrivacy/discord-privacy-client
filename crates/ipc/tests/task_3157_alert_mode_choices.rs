use ipc::commands::{cmd_osl_read_alert_mode_choice, cmd_osl_save_alert_mode_choice};
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
fn task_3157_saves_and_reads_three_alert_modes_and_refuses_unknown() {
    let _lock = KEY_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    set_file_storage_key(Some([0x57u8; 32]));
    let _key_guard = FileStorageKeyGuard;

    let dir = tempfile::tempdir().expect("tempdir");
    let saving_state = AppState::new();
    let mut saved_and_read = Vec::new();

    for expected in ["silent", "quiet", "normal"] {
        let saved = cmd_osl_save_alert_mode_choice(
            &saving_state,
            expected.to_string(),
            Some(dir.path().to_path_buf()),
        )
        .expect("save alert mode choice");
        println!("TASK3157 alert_mode.saved={}", saved.mode);

        let loaded_state = reloaded_state(dir.path());
        let read = cmd_osl_read_alert_mode_choice(&loaded_state).expect("read alert mode choice");
        println!("TASK3157 alert_mode.read_back={}", read.mode);

        assert_eq!(saved.mode, expected);
        assert_eq!(read.mode, expected);
        saved_and_read.push(read.mode);
    }

    let made_up = "loud";
    let unknown_result = cmd_osl_save_alert_mode_choice(
        &saving_state,
        made_up.to_string(),
        Some(dir.path().to_path_buf()),
    );
    let refusal = unknown_result.expect_err("made-up alert mode must be refused");
    println!("TASK3157 alert_mode.made_up={made_up}");
    println!("TASK3157 alert_mode.unknown_refusal={refusal}");

    let loaded_after_unknown = reloaded_state(dir.path());
    let after_unknown =
        cmd_osl_read_alert_mode_choice(&loaded_after_unknown).expect("read after refusal");
    println!(
        "TASK3157 alert_mode.after_unknown_refusal_still={}",
        after_unknown.mode
    );

    assert_eq!(saved_and_read, ["silent", "quiet", "normal"]);
    assert!(refusal.contains("unknown alert mode choice"));
    assert_eq!(after_unknown.mode, "normal");
}
