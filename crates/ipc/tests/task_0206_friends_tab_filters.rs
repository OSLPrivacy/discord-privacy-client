use ipc::commands::{
    cmd_osl_accept_saved_friend_request, cmd_osl_block_saved_friend_request,
    cmd_osl_create_friend_request, cmd_osl_query_friends_tabs, cmd_osl_set_guild_list, GuildDto,
    SavedFriendRequestDto,
};
use ipc::friend_request::{StoredFriendBlockState, StoredFriendState};
use ipc::scope::Scope;
use ipc::state::AppState;
use std::path::Path;
use std::sync::Mutex;

static CONFIG_DIR_LOCK: Mutex<()> = Mutex::new(());

const LOCAL_ID: &str = "AMBER-0206";
const ONLINE_PEER: &str = "900000000000020601";
const ALL_PEER: &str = "900000000000020602";
const PENDING_PEER: &str = "900000000000020603";
const BLOCKED_PEER: &str = "900000000000020604";

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
    let request_id = format!("REQ-0206-{label}");
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

fn rows_for_peer<'a>(
    tabs: &'a [(&'static str, &'a Vec<SavedFriendRequestDto>)],
    peer_id: &str,
) -> Vec<&'static str> {
    tabs.iter()
        .filter_map(|(tab, rows)| {
            rows.iter()
                .any(|row| row.target_id == peer_id)
                .then_some(*tab)
        })
        .collect()
}

fn assert_exact_tab(
    tabs: &[(&'static str, &Vec<SavedFriendRequestDto>)],
    peer_id: &str,
    expected_tab: &str,
) -> Vec<&'static str> {
    let hits = rows_for_peer(tabs, peer_id);
    assert_eq!(
        hits,
        vec![expected_tab],
        "{peer_id} must appear in exactly one expected tab"
    );
    hits
}

#[test]
fn seeded_friend_states_land_in_exactly_one_expected_tab() {
    let _lock = CONFIG_DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("tempdir");
    let _config_dir = use_temp_config_dir(dir.path());
    let state = AppState::new();

    let online_request = create_request(&state, "online", ONLINE_PEER);
    let all_request = create_request(&state, "all", ALL_PEER);
    let pending_request = create_request(&state, "pending", PENDING_PEER);
    let blocked_request = create_request(&state, "blocked", BLOCKED_PEER);

    let online =
        cmd_osl_accept_saved_friend_request(&state, online_request).expect("accept online friend");
    let all = cmd_osl_accept_saved_friend_request(&state, all_request).expect("accept all friend");
    let blocked =
        cmd_osl_block_saved_friend_request(&state, blocked_request).expect("block friend");

    cmd_osl_set_guild_list(
        &state,
        vec![GuildDto {
            id: "guild-0206".to_string(),
            name: "Guild 0206".to_string(),
            member_ids: vec![ONLINE_PEER.to_string()],
            channel_ids: vec!["channel-0206".to_string()],
        }],
    )
    .expect("seed online member snapshot");

    assert_eq!(online.state, StoredFriendState::Accepted);
    assert_eq!(all.state, StoredFriendState::Accepted);
    assert_eq!(blocked.state, StoredFriendState::Declined);
    assert_eq!(blocked.block_state, StoredFriendBlockState::BlockedByLocal);

    let tabs = cmd_osl_query_friends_tabs(&state).expect("query all four friends tabs");
    let tab_rows = [
        ("online", &tabs.online),
        ("all", &tabs.all),
        ("pending", &tabs.pending),
        ("blocked", &tabs.blocked),
    ];

    assert_eq!(tabs.online.len(), 1);
    assert_eq!(tabs.all.len(), 1);
    assert_eq!(tabs.pending.len(), 1);
    assert_eq!(tabs.blocked.len(), 1);
    assert_eq!(tabs.pending[0].request_id, pending_request);
    assert_eq!(tabs.pending[0].state, StoredFriendState::Pending);

    let online_hits = assert_exact_tab(&tab_rows, ONLINE_PEER, "online");
    let all_hits = assert_exact_tab(&tab_rows, ALL_PEER, "all");
    let pending_hits = assert_exact_tab(&tab_rows, PENDING_PEER, "pending");
    let blocked_hits = assert_exact_tab(&tab_rows, BLOCKED_PEER, "blocked");

    let total_hits = online_hits.len() + all_hits.len() + pending_hits.len() + blocked_hits.len();
    assert_eq!(total_hits, 4);

    println!(
        "TASK_0206_FRIENDS_TAB_FILTERS tabs_called=online,all,pending,blocked online_count={} all_count={} pending_count={} blocked_count={} total_expected_people=4 total_tab_hits={} online_peer_tabs={} all_peer_tabs={} pending_peer_tabs={} blocked_peer_tabs={}",
        tabs.online.len(),
        tabs.all.len(),
        tabs.pending.len(),
        tabs.blocked.len(),
        total_hits,
        online_hits.join("|"),
        all_hits.join("|"),
        pending_hits.join("|"),
        blocked_hits.join("|")
    );
}
