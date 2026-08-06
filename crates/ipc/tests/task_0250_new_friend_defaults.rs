use ipc::commands::{
    cmd_osl_accept_saved_friend_request, cmd_osl_create_friend_request,
    cmd_osl_list_friend_requests, cmd_osl_read_behaviour_choice, cmd_osl_save_behaviour_choice,
};
use ipc::scope::Scope;
use ipc::state::AppState;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

static CONFIG_DIR_LOCK: Mutex<()> = Mutex::new(());

const LOCAL_ID: &str = "AMBER-0250";
const FRIEND_A_ID: &str = "900000000000025001";
const FRIEND_B_ID: &str = "900000000000025002";
const FRIEND_A_NAME: &str = "Friend A";
const FRIEND_B_NAME: &str = "Friend B";

struct ConfigDirGuard;

impl Drop for ConfigDirGuard {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(None);
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
    }
}

fn use_temp_config_dir(dir: &Path) -> ConfigDirGuard {
    ipc::main_password::set_file_storage_key(None);
    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(Some(dir.to_path_buf()));
    ConfigDirGuard
}

fn save_defaults(state: &AppState, position: &str, mute: &str) {
    cmd_osl_save_behaviour_choice(state, "position".to_string(), position.to_string(), None)
        .expect("save position default");
    cmd_osl_save_behaviour_choice(state, "mute".to_string(), mute.to_string(), None)
        .expect("save mute default");
}

fn create_request(state: &AppState, request_id: &str, peer_id: &str, display_name: &str) {
    let scope = Scope::dm(peer_id);
    let created = cmd_osl_create_friend_request(
        state,
        request_id.to_string(),
        LOCAL_ID.to_string(),
        peer_id.to_string(),
        display_name.to_string(),
        (&scope).into(),
    )
    .expect("create pending friend request");
    assert_eq!(created.request_id, request_id);
    assert_eq!(created.display_name, display_name);
}

fn choices_for(
    rows: &[ipc::commands::SavedFriendRequestDto],
    display_name: &str,
) -> BTreeMap<String, String> {
    rows.iter()
        .find(|row| row.display_name == display_name)
        .unwrap_or_else(|| panic!("{display_name} accepted row missing"))
        .choices
        .clone()
}

#[test]
fn accepted_friend_keeps_old_defaults_and_next_friend_gets_new_defaults() {
    let _lock = CONFIG_DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("tempdir");
    let _config_dir = use_temp_config_dir(dir.path());
    let state = AppState::new();

    save_defaults(&state, "old-position-0250", "old-mute-0250");
    create_request(&state, "REQ-0250-A", FRIEND_A_ID, FRIEND_A_NAME);
    create_request(&state, "REQ-0250-B", FRIEND_B_ID, FRIEND_B_NAME);

    let accepted_a = cmd_osl_accept_saved_friend_request(&state, "REQ-0250-A".to_string())
        .expect("accept Friend A");
    assert_eq!(accepted_a.display_name, FRIEND_A_NAME);
    assert_eq!(
        accepted_a.choices.get("position").map(String::as_str),
        Some("old-position-0250")
    );
    assert_eq!(
        accepted_a.choices.get("mute").map(String::as_str),
        Some("old-mute-0250")
    );

    save_defaults(&state, "new-position-0250", "new-mute-0250");

    let accepted_b = cmd_osl_accept_saved_friend_request(&state, "REQ-0250-B".to_string())
        .expect("accept Friend B");
    assert_eq!(accepted_b.display_name, FRIEND_B_NAME);
    assert_eq!(
        accepted_b.choices.get("position").map(String::as_str),
        Some("new-position-0250")
    );
    assert_eq!(
        accepted_b.choices.get("mute").map(String::as_str),
        Some("new-mute-0250")
    );

    let listed = cmd_osl_list_friend_requests(&state).expect("list accepted friends");
    let friend_a_choices = choices_for(&listed.accepted, FRIEND_A_NAME);
    let friend_b_choices = choices_for(&listed.accepted, FRIEND_B_NAME);
    let current_position = cmd_osl_read_behaviour_choice(&state, "position".to_string())
        .expect("read current position default");
    let current_mute = cmd_osl_read_behaviour_choice(&state, "mute".to_string())
        .expect("read current mute default");

    assert_eq!(listed.accepted.len(), 2);
    assert!(listed.pending.is_empty());
    assert_eq!(
        friend_a_choices.get("position").map(String::as_str),
        Some("old-position-0250")
    );
    assert_eq!(
        friend_a_choices.get("mute").map(String::as_str),
        Some("old-mute-0250")
    );
    assert_eq!(
        friend_b_choices.get("position").map(String::as_str),
        Some("new-position-0250")
    );
    assert_eq!(
        friend_b_choices.get("mute").map(String::as_str),
        Some("new-mute-0250")
    );
    assert_eq!(current_position.choice, "new-position-0250");
    assert_eq!(current_mute.choice, "new-mute-0250");

    println!(
        "TASK_0250_NEW_FRIEND_DEFAULTS accepted_count={} pending_count={} friend_a=\"{}\" friend_a_position={} friend_a_mute={} friend_b=\"{}\" friend_b_position={} friend_b_mute={} current_position={} current_mute={}",
        listed.accepted.len(),
        listed.pending.len(),
        FRIEND_A_NAME,
        friend_a_choices.get("position").unwrap(),
        friend_a_choices.get("mute").unwrap(),
        FRIEND_B_NAME,
        friend_b_choices.get("position").unwrap(),
        friend_b_choices.get("mute").unwrap(),
        current_position.choice,
        current_mute.choice
    );
}
