use ipc::commands::{
    cmd_osl_accept_saved_friend_request, cmd_osl_block_saved_friend_request,
    cmd_osl_create_friend_request, cmd_osl_query_friends_tabs, cmd_osl_set_guild_list, GuildDto,
};
use ipc::friend_request::{StoredFriendBlockState, StoredFriendState};
use ipc::scope::Scope;
use ipc::state::AppState;
use std::path::Path;
use std::sync::Mutex;

static CONFIG_DIR_LOCK: Mutex<()> = Mutex::new(());

const LOCAL_ID: &str = "AMBER-0204";
const ONLINE_PEER: &str = "900000000000020401";
const OFFLINE_PEER: &str = "900000000000020402";
const PENDING_PEER: &str = "900000000000020403";
const BLOCKED_PEER: &str = "900000000000020404";

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
    let request_id = format!("REQ-0204-{label}");
    let scope = Scope::dm(peer_id);
    let created = cmd_osl_create_friend_request(
        state,
        request_id.clone(),
        LOCAL_ID.to_string(),
        peer_id.to_string(),
        format!("Friend {label}"),
        (&scope).into(),
    )
    .expect("create seeded friend request");
    assert_eq!(created.request_id, request_id);
    assert_eq!(created.target_id, peer_id);
    request_id
}

#[test]
fn seeded_direct_query_returns_counts_for_all_four_friend_tabs() {
    let _lock = CONFIG_DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("tempdir");
    let _config_dir = use_temp_config_dir(dir.path());
    let state = AppState::new();

    let online_request = create_request(&state, "online", ONLINE_PEER);
    let offline_request = create_request(&state, "offline", OFFLINE_PEER);
    let pending_request = create_request(&state, "pending", PENDING_PEER);
    let blocked_request = create_request(&state, "blocked", BLOCKED_PEER);

    cmd_osl_accept_saved_friend_request(&state, online_request).expect("accept online friend");
    cmd_osl_accept_saved_friend_request(&state, offline_request).expect("accept offline friend");
    cmd_osl_block_saved_friend_request(&state, blocked_request).expect("block friend");
    cmd_osl_set_guild_list(
        &state,
        vec![GuildDto {
            id: "guild-0204".to_string(),
            name: "Guild 0204".to_string(),
            member_ids: vec![ONLINE_PEER.to_string(), "not-a-saved-friend".to_string()],
            channel_ids: vec!["channel-0204".to_string()],
        }],
    )
    .expect("seed online guild member snapshot");

    let tabs = cmd_osl_query_friends_tabs(&state).expect("query friends tabs");

    assert_eq!(tabs.online.len(), 1);
    assert_eq!(tabs.all.len(), 2);
    assert_eq!(tabs.pending.len(), 1);
    assert_eq!(tabs.blocked.len(), 1);

    assert_eq!(tabs.online[0].request_id, "REQ-0204-online");
    assert_eq!(tabs.online[0].target_id, ONLINE_PEER);
    assert_eq!(tabs.online[0].state, StoredFriendState::Accepted);
    assert_eq!(
        tabs.online[0].block_state,
        StoredFriendBlockState::NotBlocked
    );
    assert!(tabs.all.iter().any(|row| row.target_id == OFFLINE_PEER));
    assert_eq!(tabs.pending[0].request_id, pending_request);
    assert_eq!(tabs.pending[0].state, StoredFriendState::Pending);
    assert_eq!(tabs.blocked[0].request_id, "REQ-0204-blocked");
    assert_eq!(tabs.blocked[0].target_id, BLOCKED_PEER);
    assert_eq!(
        tabs.blocked[0].block_state,
        StoredFriendBlockState::BlockedByLocal
    );

    println!(
        "TASK_0204_FRIENDS_TABS_QUERY online_count={} all_count={} pending_count={} blocked_count={}",
        tabs.online.len(),
        tabs.all.len(),
        tabs.pending.len(),
        tabs.blocked.len()
    );
}
