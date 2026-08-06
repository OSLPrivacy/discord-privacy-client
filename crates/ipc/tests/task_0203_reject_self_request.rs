use ipc::commands::{cmd_osl_create_friend_request, cmd_osl_list_friend_requests};
use ipc::friend_request::{StoredFriendBlockState, StoredFriendState};
use ipc::scope::Scope;
use ipc::state::AppState;
use std::path::Path;
use std::sync::Mutex;

static CONFIG_DIR_LOCK: Mutex<()> = Mutex::new(());

const REQUEST_ID: &str = "REQ-0203";
const REQUESTER: &str = "AMBER-0203";
const RECIPIENT: &str = "BIRCH-0203";
const DISPLAY_NAME: &str = "BIRCH-0203";

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
fn create_request_refuses_when_recipient_is_requester() {
    let _lock = CONFIG_DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("tempdir");
    let _config_dir = use_temp_config_dir(dir.path());
    let state = AppState::new();
    let good_scope = Scope::dm(RECIPIENT);

    let before = cmd_osl_list_friend_requests(&state).expect("list before");
    assert_eq!(before.pending.len(), 0);

    let created = cmd_osl_create_friend_request(
        &state,
        REQUEST_ID.to_string(),
        REQUESTER.to_string(),
        RECIPIENT.to_string(),
        DISPLAY_NAME.to_string(),
        (&good_scope).into(),
    )
    .expect("create non-self friend request");
    assert_eq!(created.request_id, REQUEST_ID);
    assert_eq!(created.requester_id, REQUESTER);
    assert_eq!(created.target_id, RECIPIENT);
    assert_eq!(created.display_name, DISPLAY_NAME);
    assert_eq!(created.state, StoredFriendState::Pending);
    assert_eq!(created.block_state, StoredFriendBlockState::NotBlocked);

    let after_good = cmd_osl_list_friend_requests(&state).expect("list after good request");
    assert_eq!(after_good.pending.len(), 1);
    let pending = &after_good.pending[0];
    assert_eq!(pending.request_id, REQUEST_ID);
    assert_eq!(pending.requester_id, REQUESTER);
    assert_eq!(pending.target_id, RECIPIENT);
    assert_eq!(pending.display_name, DISPLAY_NAME);
    assert_eq!(pending.request_fingerprint, created.request_fingerprint);
    let fingerprint_before_self_retry = pending.request_fingerprint.clone();

    let self_scope = Scope::dm(REQUESTER);
    let self_error = cmd_osl_create_friend_request(
        &state,
        REQUEST_ID.to_string(),
        REQUESTER.to_string(),
        REQUESTER.to_string(),
        DISPLAY_NAME.to_string(),
        (&self_scope).into(),
    )
    .expect_err("self request must be refused");
    assert_eq!(self_error, "OSL: cannot request yourself");

    let after_self = cmd_osl_list_friend_requests(&state).expect("list after self request");
    assert_eq!(after_self.pending.len(), 1);
    assert_eq!(
        after_self.pending[0].request_fingerprint,
        fingerprint_before_self_retry
    );
    assert_eq!(after_self.pending[0].request_id, REQUEST_ID);
    assert_eq!(after_self.pending[0].requester_id, REQUESTER);
    assert_eq!(after_self.pending[0].target_id, RECIPIENT);

    println!(
        "TASK_0203_REJECT_SELF_REQUEST requester={} recipient={} before_pending_count={} good_request_id={} good_pending_count={} self_retry_only_changed_field=recipient self_retry_recipient={} self_error=\"{}\" after_self_pending_count={} fingerprint_before_self_retry={} fingerprint_after_self_retry={}",
        pending.requester_id,
        pending.target_id,
        before.pending.len(),
        created.request_id,
        after_good.pending.len(),
        REQUESTER,
        self_error,
        after_self.pending.len(),
        fingerprint_before_self_retry,
        after_self.pending[0].request_fingerprint
    );
}
