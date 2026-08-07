//! TASK 0210 - break a friends tab with stale state.
//!
//! A tab query that answers from a cached snapshot looks correct on the first
//! paint and lies on every one after it. This test changes a request from
//! pending to accepted *between* two `cmd_osl_query_friends_tabs` calls and
//! demands that the second call reflect the change: Pending must omit the
//! person and All must include them.

use ipc::commands::{
    cmd_osl_accept_saved_friend_request, cmd_osl_create_friend_request, cmd_osl_query_friends_tabs,
    SavedFriendRequestDto,
};
use ipc::friend_request::{StoredFriendBlockState, StoredFriendState};
use ipc::scope::Scope;
use ipc::state::AppState;
use std::path::Path;
use std::sync::Mutex;

static CONFIG_DIR_LOCK: Mutex<()> = Mutex::new(());

const LOCAL_ID: &str = "AMBER-0210";
const MOVING_PEER: &str = "900000000000021001";
const REQUEST_ID: &str = "REQ-0210-moving";

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

fn has_peer(rows: &[SavedFriendRequestDto], peer_id: &str) -> bool {
    rows.iter().any(|row| row.target_id == peer_id)
}

fn tabs_holding_peer(
    tabs: &[(&'static str, &Vec<SavedFriendRequestDto>)],
    peer_id: &str,
) -> Vec<&'static str> {
    tabs.iter()
        .filter_map(|(tab, rows)| has_peer(rows, peer_id).then_some(*tab))
        .collect()
}

#[test]
fn accepting_between_two_tab_queries_moves_the_person_out_of_pending_into_all() {
    let _lock = CONFIG_DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("tempdir");
    let _config_dir = use_temp_config_dir(dir.path());
    let state = AppState::new();

    let scope = Scope::dm(MOVING_PEER);
    let created = cmd_osl_create_friend_request(
        &state,
        REQUEST_ID.to_string(),
        LOCAL_ID.to_string(),
        MOVING_PEER.to_string(),
        "Friend 0210".to_string(),
        (&scope).into(),
    )
    .expect("create seeded friend request");
    assert_eq!(created.request_id, REQUEST_ID);
    assert_eq!(created.target_id, MOVING_PEER);
    assert_eq!(created.state, StoredFriendState::Pending);

    // --- first query: the person is pending, and only pending -------------
    let first = cmd_osl_query_friends_tabs(&state).expect("first friends tabs query");
    let first_rows = [
        ("online", &first.online),
        ("all", &first.all),
        ("pending", &first.pending),
        ("blocked", &first.blocked),
    ];
    let first_hits = tabs_holding_peer(&first_rows, MOVING_PEER);
    assert_eq!(
        first_hits,
        vec!["pending"],
        "before acceptance the person belongs to the pending tab only"
    );
    assert_eq!(first.pending.len(), 1);
    assert_eq!(first.pending[0].request_id, REQUEST_ID);
    assert_eq!(first.pending[0].state, StoredFriendState::Pending);
    assert_eq!(first.all.len(), 0);

    // --- the state change that happens between the two queries ------------
    let accepted = cmd_osl_accept_saved_friend_request(&state, REQUEST_ID.to_string())
        .expect("accept the pending friend request");
    assert_eq!(accepted.request_id, REQUEST_ID);
    assert_eq!(accepted.state, StoredFriendState::Accepted);

    // --- second query: pending must omit them, all must include them ------
    let second = cmd_osl_query_friends_tabs(&state).expect("second friends tabs query");
    let second_rows = [
        ("online", &second.online),
        ("all", &second.all),
        ("pending", &second.pending),
        ("blocked", &second.blocked),
    ];
    let second_hits = tabs_holding_peer(&second_rows, MOVING_PEER);

    assert!(
        !has_peer(&second.pending, MOVING_PEER),
        "second pending query still lists {MOVING_PEER}: the tab is serving stale state"
    );
    assert_eq!(
        second.pending.len(),
        0,
        "pending tab must be empty after the only pending request was accepted"
    );
    assert!(
        has_peer(&second.all, MOVING_PEER),
        "second all query omits {MOVING_PEER}: the accepted friend never arrived"
    );
    assert_eq!(second.all.len(), 1);
    assert_eq!(second.all[0].request_id, REQUEST_ID);
    assert_eq!(second.all[0].state, StoredFriendState::Accepted);
    assert_eq!(second.all[0].block_state, StoredFriendBlockState::NotBlocked);
    assert_eq!(
        second_hits,
        vec!["all"],
        "after acceptance the person belongs to the all tab only"
    );

    // The first result is a separate snapshot, not the same object mutated:
    // it must still show the pre-acceptance world.
    assert_eq!(first.pending.len(), 1);
    assert!(has_peer(&first.pending, MOVING_PEER));
    assert_ne!(first.pending, second.pending);

    println!(
        "TASK_0210_FRIENDS_TAB_STALE_STATE peer={MOVING_PEER} \
first_pending_count={} first_all_count={} first_peer_tabs={} \
changed=pending->accepted \
second_pending_count={} second_all_count={} second_peer_tabs={} \
second_pending_has_peer={} second_all_has_peer={}",
        first.pending.len(),
        first.all.len(),
        first_hits.join("|"),
        second.pending.len(),
        second.all.len(),
        second_hits.join("|"),
        has_peer(&second.pending, MOVING_PEER),
        has_peer(&second.all, MOVING_PEER),
    );
}
