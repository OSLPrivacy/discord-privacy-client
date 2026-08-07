use ipc::commands::{
    cmd_osl_accept_saved_friend_request, cmd_osl_list_friend_requests, SavedFriendRequestDto,
};
use ipc::friend_request::{
    save_friend_request_file_state, FriendRequestFileState, StoredFriendBlockState,
    StoredFriendRecord, StoredFriendRequestFileEntry, StoredFriendState,
};
use ipc::scope::Scope;
use ipc::state::AppState;
use std::path::Path;
use std::sync::Mutex;

static CONFIG_DIR_LOCK: Mutex<()> = Mutex::new(());

const REQUEST_ID: &str = "PINE-0226";
const RELATIONSHIP_ID: &str = "REL-0226";
const LOCAL_ID: &str = "AMBER-0226";
const PEER_ID: &str = "900000000000022601";

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

fn pending_entry(redemption_count: u32) -> StoredFriendRequestFileEntry {
    StoredFriendRequestFileEntry {
        request_id: REQUEST_ID.to_string(),
        relationship_id: RELATIONSHIP_ID.to_string(),
        requester_id: LOCAL_ID.to_string(),
        target_id: PEER_ID.to_string(),
        scope_key: Scope::dm(PEER_ID).storage_key(),
        invite_redemption_count: redemption_count,
        received_at_ms: 1_700_226_000_000,
        expires_at_ms: 1_700_226_060_000,
    }
}

fn friend_record() -> StoredFriendRecord {
    StoredFriendRecord {
        record_id: RELATIONSHIP_ID.to_string(),
        local_identity_id: LOCAL_ID.to_string(),
        remote_identity_id: PEER_ID.to_string(),
        state: StoredFriendState::Pending,
        display_name: "Pine 0226".to_string(),
        block_state: StoredFriendBlockState::NotBlocked,
        choices: Default::default(),
    }
}

fn seed_pending(dir: &Path, accepted: Vec<StoredFriendRequestFileEntry>, redemption_count: u32) {
    let state = FriendRequestFileState {
        schema_version: 1,
        friends: vec![friend_record()],
        pending: vec![pending_entry(redemption_count)],
        accepted,
        declined_or_revoked: Vec::new(),
        blocked: Vec::new(),
    };
    save_friend_request_file_state(dir, &state).expect("seed saved friend request file");
}

fn accepted_count(state: &AppState) -> usize {
    cmd_osl_list_friend_requests(state)
        .expect("saved requests are readable")
        .accepted
        .len()
}

fn accepted_row(state: &AppState) -> SavedFriendRequestDto {
    let mut rows = cmd_osl_list_friend_requests(state)
        .expect("accepted saved requests are readable")
        .accepted;
    assert_eq!(rows.len(), 1);
    rows.remove(0)
}

#[test]
fn task_0226_consumed_invite_acceptance_refuses_reuse_without_changing_relationship() {
    let _lock = CONFIG_DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("tempdir");
    let _config_dir = use_temp_config_dir(dir.path());
    let state = AppState::new();

    seed_pending(dir.path(), Vec::new(), 1);
    let before = cmd_osl_list_friend_requests(&state).expect("PINE-0226 is readable");
    assert_eq!(before.pending.len(), 1);
    assert_eq!(before.pending[0].request_id, REQUEST_ID);
    assert_eq!(before.pending[0].invite_redemption_count, 1);
    assert_eq!(before.accepted.len(), 0);
    println!(
        "TASK0226_READABLE_REQUEST_ID={}",
        before.pending[0].request_id
    );
    println!("TASK0226_ACCEPTED_COUNT_BEFORE={}", before.accepted.len());

    let accepted = cmd_osl_accept_saved_friend_request(&state, REQUEST_ID.to_string())
        .expect("first invite redemption can accept");
    assert_eq!(accepted.relationship_id, RELATIONSHIP_ID);
    assert_eq!(accepted.invite_redemption_count, 1);
    let first_count = accepted_count(&state);
    assert_eq!(first_count, 1);
    println!(
        "TASK0226_FIRST_ACCEPT_REDEMPTION_COUNT={} RELATIONSHIP_ID={} ACCEPTED_COUNT_AFTER_FIRST={} FINGERPRINT={}",
        accepted.invite_redemption_count,
        accepted.relationship_id,
        first_count,
        accepted.request_fingerprint
    );

    let accepted_snapshot = vec![pending_entry(1)];
    seed_pending(dir.path(), accepted_snapshot, 2);
    let reused_error = cmd_osl_accept_saved_friend_request(&state, REQUEST_ID.to_string())
        .expect_err("second invite redemption must be refused");
    assert_eq!(reused_error, "OSL: invite already reused");
    let after_reuse = accepted_row(&state);
    assert_eq!(after_reuse.invite_redemption_count, 1);
    assert_eq!(after_reuse.relationship_id, RELATIONSHIP_ID);
    assert_eq!(
        after_reuse.request_fingerprint,
        accepted.request_fingerprint
    );
    println!(
        "TASK0226_REUSE_REDEMPTION_COUNT=2 ERROR={} ACCEPTED_COUNT_AFTER_REUSE=1 RELATIONSHIP_ID={} FINGERPRINT_STABLE={}",
        reused_error,
        after_reuse.relationship_id,
        after_reuse.request_fingerprint
    );
}
