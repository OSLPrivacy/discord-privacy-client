use ipc::commands::{
    cmd_osl_accept_saved_friend_request, cmd_osl_block_saved_friend_request,
    cmd_osl_create_friend_request, cmd_osl_decline_saved_friend_request,
    cmd_osl_list_friend_requests,
};
use ipc::friend_request::{StoredFriendBlockState, StoredFriendState};
use ipc::scope::Scope;
use ipc::state::AppState;
use std::path::Path;
use std::sync::Mutex;

static CONFIG_DIR_LOCK: Mutex<()> = Mutex::new(());

const LOCAL_ID: &str = "AMBER-0202";
const ACCEPTED_PEER: &str = "900000000000020201";
const DECLINED_PEER: &str = "900000000000020202";
const BLOCKED_PEER: &str = "900000000000020203";

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

fn create_request(state: &AppState, label: &str, peer_id: &str) -> String {
    let request_id = format!("REQ-0202-{label}");
    let scope = Scope::dm(peer_id);
    let created = cmd_osl_create_friend_request(
        state,
        request_id.clone(),
        LOCAL_ID.to_string(),
        peer_id.to_string(),
        format!("{label} 0202"),
        (&scope).into(),
    )
    .expect("create friend request");
    assert_eq!(created.request_id, request_id);
    assert_eq!(created.target_id, peer_id);
    assert_eq!(created.state, StoredFriendState::Pending);
    assert_eq!(created.block_state, StoredFriendBlockState::NotBlocked);
    request_id
}

#[test]
fn direct_list_reports_accepted_declined_and_blocked_separately() {
    let _lock = CONFIG_DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("tempdir");
    let _config_dir = use_temp_config_dir(dir.path());
    let state = AppState::new();

    let accepted_request = create_request(&state, "accepted", ACCEPTED_PEER);
    let declined_request = create_request(&state, "declined", DECLINED_PEER);
    let blocked_request = create_request(&state, "blocked", BLOCKED_PEER);

    let accepted =
        cmd_osl_accept_saved_friend_request(&state, accepted_request.clone()).expect("accept one");
    let declined = cmd_osl_decline_saved_friend_request(&state, declined_request.clone())
        .expect("decline one");
    let blocked =
        cmd_osl_block_saved_friend_request(&state, blocked_request.clone()).expect("block one");

    let listed = cmd_osl_list_friend_requests(&state).expect("direct list friend requests");

    assert!(listed.pending.is_empty());
    assert_eq!(listed.accepted.len(), 1);
    assert_eq!(listed.declined.len(), 1);
    assert_eq!(listed.blocked.len(), 1);

    assert_eq!(accepted.state, StoredFriendState::Accepted);
    assert_eq!(accepted.block_state, StoredFriendBlockState::NotBlocked);
    assert_eq!(declined.state, StoredFriendState::Declined);
    assert_eq!(declined.block_state, StoredFriendBlockState::NotBlocked);
    assert_eq!(blocked.state, StoredFriendState::Declined);
    assert_eq!(blocked.block_state, StoredFriendBlockState::BlockedByLocal);

    assert_eq!(listed.accepted[0].request_id, accepted_request);
    assert_eq!(listed.accepted[0].target_id, ACCEPTED_PEER);
    assert_eq!(listed.accepted[0].state, StoredFriendState::Accepted);
    assert_eq!(
        listed.accepted[0].block_state,
        StoredFriendBlockState::NotBlocked
    );
    assert_eq!(listed.declined[0].request_id, declined_request);
    assert_eq!(listed.declined[0].target_id, DECLINED_PEER);
    assert_eq!(listed.declined[0].state, StoredFriendState::Declined);
    assert_eq!(
        listed.declined[0].block_state,
        StoredFriendBlockState::NotBlocked
    );
    assert_eq!(listed.blocked[0].request_id, blocked_request);
    assert_eq!(listed.blocked[0].target_id, BLOCKED_PEER);
    assert_eq!(listed.blocked[0].state, StoredFriendState::Declined);
    assert_eq!(
        listed.blocked[0].block_state,
        StoredFriendBlockState::BlockedByLocal
    );

    println!(
        "TASK_0202_DIRECT_LIST accepted_count={} accepted_request={} accepted_state={:?} accepted_block_state={:?} declined_count={} declined_request={} declined_state={:?} declined_block_state={:?} blocked_count={} blocked_request={} blocked_state={:?} blocked_block_state={:?} pending_count={}",
        listed.accepted.len(),
        listed.accepted[0].request_id,
        listed.accepted[0].state,
        listed.accepted[0].block_state,
        listed.declined.len(),
        listed.declined[0].request_id,
        listed.declined[0].state,
        listed.declined[0].block_state,
        listed.blocked.len(),
        listed.blocked[0].request_id,
        listed.blocked[0].state,
        listed.blocked[0].block_state,
        listed.pending.len()
    );
}
