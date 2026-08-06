use ipc::commands::{cmd_osl_create_friend_request, cmd_osl_list_friend_requests};
use ipc::friend_request::{StoredFriendBlockState, StoredFriendState};
use ipc::scope::Scope;
use ipc::state::AppState;
use std::path::Path;
use std::sync::Mutex;

static CONFIG_DIR_LOCK: Mutex<()> = Mutex::new(());

const REQUEST_ID: &str = "REQ-0201";
const REQUESTER: &str = "AMBER-0201";
const TARGET: &str = "900000000000002001";
const DISPLAY_NAME: &str = "BIRCH-0201";

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

#[test]
fn direct_list_query_returns_one_created_pending_request() {
    let _lock = CONFIG_DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("tempdir");
    let _config_dir = use_temp_config_dir(dir.path());
    let state = AppState::new();
    let scope = Scope::dm(TARGET);

    let created = cmd_osl_create_friend_request(
        &state,
        REQUEST_ID.to_string(),
        REQUESTER.to_string(),
        TARGET.to_string(),
        DISPLAY_NAME.to_string(),
        (&scope).into(),
    )
    .expect("create friend request");
    assert_eq!(created.request_id, REQUEST_ID);
    assert_eq!(created.state, StoredFriendState::Pending);

    let listed = cmd_osl_list_friend_requests(&state).expect("list friend requests");
    assert_eq!(listed.pending.len(), 1);
    assert!(listed.accepted.is_empty());
    assert!(listed.declined.is_empty());
    assert!(listed.blocked.is_empty());

    let pending = &listed.pending[0];
    assert_eq!(pending.request_id, REQUEST_ID);
    assert_eq!(pending.requester_id, REQUESTER);
    assert_eq!(pending.target_id, TARGET);
    assert_eq!(pending.display_name, DISPLAY_NAME);
    assert_eq!(pending.scope_key, scope.storage_key());
    assert_eq!(pending.state, StoredFriendState::Pending);
    assert_eq!(pending.block_state, StoredFriendBlockState::NotBlocked);
    assert_eq!(pending.request_fingerprint, created.request_fingerprint);

    println!(
        "TASK_0201_DIRECT_LIST pending_count={} request_id={} peer={} scope={} state={:?} block_state={:?}",
        listed.pending.len(),
        pending.request_id,
        pending.target_id,
        pending.scope_key,
        pending.state,
        pending.block_state
    );
}
