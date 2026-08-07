use ipc::commands::{cmd_osl_create_friend_request, cmd_osl_list_friend_requests};
use ipc::friend_request::{StoredFriendBlockState, StoredFriendState};
use ipc::scope::Scope;
use ipc::state::AppState;
//! Task 0201: save friend requests.

use ipc::commands::{
    cmd_osl_block_friend_request, cmd_osl_create_friend_request, cmd_osl_decline_friend_request,
    cmd_osl_list_friend_requests, FriendRequestDecision,
};
use ipc::peer_map::{PeerEntry, WhitelistEntry};
use ipc::scope::Scope;
use ipc::state::AppState;
use ipc::tofu::KeyBundle;
use std::path::Path;
use std::sync::Mutex;

static CONFIG_DIR_LOCK: Mutex<()> = Mutex::new(());

const REQUEST_ID: &str = "REQ-0201";
const REQUESTER: &str = "AMBER-0201";
const TARGET: &str = "900000000000002001";
const DISPLAY_NAME: &str = "BIRCH-0201";
const PEER_DID: &str = "900000000000002001";

struct ConfigDirGuard;

impl Drop for ConfigDirGuard {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(None);
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
        ipc::main_password::set_file_storage_key(None);
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
    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(Some(dir.to_path_buf()));
    ipc::main_password::set_file_storage_key(Some([0x20; 32]));
    ConfigDirGuard
}

fn trusted_peer_bundle(label: &str) -> KeyBundle {
    KeyBundle {
        ed25519_pub: format!("{label}-ed25519"),
        x25519_pub: format!("{label}-x25519"),
        mlkem768_pub: format!("{label}-mlkem768"),
        ratchet_initial_pub: Some(format!("{label}-ratchet")),
    }
}

fn friend_request_state() -> AppState {
    let state = AppState::new();
    let mut identity = keystore::generate_identity("task-0201-requester".to_string());
    identity.discord_snowflake = Some("900000000000002099".to_string());
    state.install_identity(identity);
    state.peer_map.lock().unwrap().insert(
        PEER_DID.to_string(),
        PeerEntry {
            discord_id: Some(PEER_DID.to_string()),
            tofu_key_bundle: Some(trusted_peer_bundle("task-0201-peer")),
            ..PeerEntry::default()
        },
    );
    state
}

#[test]
fn direct_list_query_returns_one_created_pending_request() {
    let _lock = CONFIG_DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let _config_dir = use_temp_config_dir(dir.path());
    let state = friend_request_state();
    let scope = Scope::dm(PEER_DID);

    let created =
        cmd_osl_create_friend_request(&state, PEER_DID.to_string(), (&scope).into()).unwrap();
    let listed = cmd_osl_list_friend_requests(&state).unwrap();

    println!(
        "TASK_0201_DIRECT_LIST pending_count={} peer={} scope={}",
        listed.len(),
        listed[0].peer_discord_id,
        listed[0].scope_storage_key
    );

    assert_eq!(listed, vec![created.pending]);
}

#[test]
fn decline_and_block_friend_request_commands_are_closed_local_actions() {
    let _lock = CONFIG_DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let _config_dir = use_temp_config_dir(dir.path());
    let state = friend_request_state();
    let scope = Scope::dm(PEER_DID);

    let declined =
        cmd_osl_decline_friend_request(&state, PEER_DID.to_string(), (&scope).into()).unwrap();
    assert_eq!(declined.decision, FriendRequestDecision::DeclinedPending);
    assert!(!declined.revoked_grant);

    state
        .peer_map
        .lock()
        .unwrap()
        .get_mut(PEER_DID)
        .unwrap()
        .outgoing_whitelists = vec![WhitelistEntry::Dm {
        broadened: true,
        enabled_at: Some("2026-08-06T00:00:00Z".to_string()),
    }];
    let blocked =
        cmd_osl_block_friend_request(&state, PEER_DID.to_string(), (&scope).into()).unwrap();
    assert_eq!(
        blocked.decision,
        FriendRequestDecision::RevokedAcceptedGrant
    );
    assert!(blocked.revoked_grant);

    let peer_map = state.peer_map.lock().unwrap();
    let peer = peer_map.get(PEER_DID).unwrap();
    assert!(peer.outgoing_whitelists.is_empty());
    assert!(peer
        .burned_scopes
        .iter()
        .any(|burn| matches!(burn, ipc::peer_map::BurnedScope::Dm { .. })));
}
