use ipc::commands::{
    cmd_osl_accept_saved_friend_request, cmd_osl_create_friend_request, cmd_osl_query_friends_tabs,
    cmd_osl_set_guild_list, GuildDto,
};
use ipc::friend_request::{StoredFriendBlockState, StoredFriendState};
use ipc::scope::Scope;
use ipc::state::AppState;
use std::path::Path;
use std::sync::Mutex;

static CONFIG_DIR_LOCK: Mutex<()> = Mutex::new(());

const LOCAL_ID: &str = "AMBER-0205";
const ONLINE_ACCEPTED_FRIEND: &str = "900000000000020501";
const ONLINE_NON_FRIEND: &str = "900000000000020502";

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
    let request_id = format!("REQ-0205-{label}");
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
fn online_tab_contains_only_online_accepted_friends() {
    let _lock = CONFIG_DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("tempdir");
    let _config_dir = use_temp_config_dir(dir.path());
    let state = AppState::new();

    let accepted_request = create_request(&state, "accepted-online", ONLINE_ACCEPTED_FRIEND);
    cmd_osl_accept_saved_friend_request(&state, accepted_request).expect("accept online friend");

    cmd_osl_set_guild_list(
        &state,
        vec![GuildDto {
            id: "guild-0205".to_string(),
            name: "Guild 0205".to_string(),
            member_ids: vec![
                ONLINE_ACCEPTED_FRIEND.to_string(),
                ONLINE_NON_FRIEND.to_string(),
            ],
            channel_ids: vec!["channel-0205".to_string()],
        }],
    )
    .expect("seed presence snapshot");

    let tabs = cmd_osl_query_friends_tabs(&state).expect("query friends tabs");

    let accepted_friend_present = tabs
        .online
        .iter()
        .filter(|row| {
            row.target_id == ONLINE_ACCEPTED_FRIEND
                && row.state == StoredFriendState::Accepted
                && row.block_state == StoredFriendBlockState::NotBlocked
        })
        .count();
    let online_non_friend_count = tabs
        .online
        .iter()
        .filter(|row| row.target_id == ONLINE_NON_FRIEND)
        .count();

    assert_eq!(accepted_friend_present, 1);
    assert_eq!(online_non_friend_count, 0);
    assert_eq!(tabs.online.len(), 1);

    println!(
        "TASK_0205_ONLINE_FRIENDS accepted_friend_present={} online_non_friend_count={} online_count={} online_friend_id={} online_non_friend_id={}",
        accepted_friend_present,
        online_non_friend_count,
        tabs.online.len(),
        ONLINE_ACCEPTED_FRIEND,
        ONLINE_NON_FRIEND
    );
}
