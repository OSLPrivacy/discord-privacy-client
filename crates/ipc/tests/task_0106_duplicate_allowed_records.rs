use ipc::commands::{cmd_osl_add_allowed_place_record, cmd_osl_list_allowed_place_records};
use ipc::main_password::set_file_storage_key;
use ipc::state::AppState;
use std::sync::Mutex;

static KEY_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn task_0106_adding_same_allowed_record_twice_keeps_one_stored_record() {
    let _guard = KEY_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    set_file_storage_key(Some([0x06u8; 32]));

    let dir = tempfile::tempdir().expect("tempdir");
    let state = AppState::new();
    let app = "discord";
    let account = "account-0106";
    let kind = "direct_message";
    let stable_id = "discord:account-0106:direct_message:stable-0106";

    let first = cmd_osl_add_allowed_place_record(
        &state,
        app.to_string(),
        account.to_string(),
        kind.to_string(),
        stable_id.to_string(),
        Some(dir.path().to_path_buf()),
    )
    .expect("first add succeeds");
    let second = cmd_osl_add_allowed_place_record(
        &state,
        app.to_string(),
        account.to_string(),
        kind.to_string(),
        stable_id.to_string(),
        Some(dir.path().to_path_buf()),
    )
    .expect("duplicate add succeeds idempotently");

    let reloaded_state = AppState::new();
    *reloaded_state.app_preferences.lock().unwrap() =
        ipc::app_preferences::load_app_preferences(&dir.path().join("app_preferences.json"));
    let stored = cmd_osl_list_allowed_place_records(&reloaded_state).expect("list stored records");

    println!("TASK0106 duplicate_allowed_records.command_add_count=2");
    println!("TASK0106 duplicate_allowed_records.app={app}");
    println!("TASK0106 duplicate_allowed_records.account={account}");
    println!("TASK0106 duplicate_allowed_records.kind={kind}");
    println!("TASK0106 duplicate_allowed_records.stable_id={stable_id}");
    println!(
        "TASK0106 duplicate_allowed_records.stored_record_count={}",
        stored.len()
    );

    assert_eq!(first, second);
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].app, app);
    assert_eq!(stored[0].account, account);
    assert_eq!(stored[0].kind, kind);
    assert_eq!(stored[0].stable_id, stable_id);

    set_file_storage_key(None);
}
