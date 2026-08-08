//! Task 0278: removing a friend is not blocking them.
//!
//! A removed friend may send a fresh request, but accepting that request must
//! not restore the allowed places held by the old friendship.

use crate::allowed_places::{
    add_allowed_place_record, count_allowed_place_records_for_person, AllowedPlaceRecord,
};
use crate::commands::{
    cmd_osl_accept_saved_friend_request, cmd_osl_create_friend_request,
    cmd_osl_list_blocked_people, cmd_osl_list_friend_requests, cmd_osl_query_friends_tabs,
    cmd_osl_remove_friend,
};
use crate::friend_request::{StoredFriendBlockState, StoredFriendState};
use crate::peer_map::{PeerEntry, WhitelistEntry};
use crate::scope::Scope;
use crate::state::AppState;
use std::path::Path;
use std::sync::Mutex;

static CONFIG_DIR_LOCK: Mutex<()> = Mutex::new(());

const LOCAL_ID: &str = "AMBER-0278";
const REMOVED_PERSON: &str = "900000000000027801";
const INITIAL_REQUEST: &str = "REQ-0278-INITIAL";
const NEW_REQUEST: &str = "REQ-0278-SECOND";

struct ConfigDirGuard;

impl Drop for ConfigDirGuard {
    fn drop(&mut self) {
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
        crate::main_password::set_file_storage_key(None);
    }
}

fn use_temp_config_dir(dir: &Path) -> ConfigDirGuard {
    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(Some(dir.to_path_buf()));
    crate::main_password::set_file_storage_key(Some([0x78; 32]));
    ConfigDirGuard
}

fn create_request(state: &AppState, request_id: &str) {
    let scope = Scope::dm(REMOVED_PERSON);
    let created = cmd_osl_create_friend_request(
        state,
        request_id.to_owned(),
        REMOVED_PERSON.to_owned(),
        LOCAL_ID.to_owned(),
        "Removed Friend".to_owned(),
        (&scope).into(),
    )
    .expect("the removed person can send a friend request");
    assert_eq!(created.request_id, request_id);
    assert_eq!(created.state, StoredFriendState::Pending);
    assert_eq!(created.block_state, StoredFriendBlockState::NotBlocked);
}

fn seed_accepted_friend(state: &AppState) {
    create_request(state, INITIAL_REQUEST);
    cmd_osl_accept_saved_friend_request(state, INITIAL_REQUEST.to_owned())
        .expect("accept the original friendship");
    state.peer_map.lock().unwrap().insert(
        REMOVED_PERSON.to_owned(),
        PeerEntry {
            discord_id: Some(REMOVED_PERSON.to_owned()),
            outgoing_whitelists: vec![WhitelistEntry::Dm {
                broadened: true,
                enabled_at: Some("2026-08-08T00:00:00Z".to_owned()),
            }],
            ..PeerEntry::default()
        },
    );
}

fn seed_three_allowed_places(dir: &Path) {
    for account in ["account-a", "account-b", "account-c"] {
        add_allowed_place_record(
            dir,
            AllowedPlaceRecord::discord_direct_message(account, REMOVED_PERSON),
        )
        .expect("seed old allowed place");
    }
}

#[test]
fn task_0278_removed_friend_can_request_again_without_old_allowed_places() {
    let _lock = CONFIG_DIR_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let config_dir = tempfile::tempdir().unwrap();
    let allowed_place_dir = tempfile::tempdir().unwrap();
    let _config_dir = use_temp_config_dir(config_dir.path());
    let state = AppState::new();

    seed_accepted_friend(&state);
    seed_three_allowed_places(allowed_place_dir.path());
    let old_allowed_places =
        count_allowed_place_records_for_person(allowed_place_dir.path(), REMOVED_PERSON).unwrap();
    assert_eq!(
        old_allowed_places, 3,
        "fixture must start with three old places"
    );

    let removed = cmd_osl_remove_friend(
        &state,
        REMOVED_PERSON.to_owned(),
        Some(allowed_place_dir.path().to_path_buf()),
    )
    .expect("remove accepted friend");
    assert_eq!(removed.allowed_places_removed, 3);
    assert_eq!(removed.allowed_places, 0);
    assert_eq!(removed.friendship_state, "none");

    let blocked_people = cmd_osl_list_blocked_people(&state).expect("list blocked people");
    let tabs_after_removal = cmd_osl_query_friends_tabs(&state).expect("query friends tabs");
    assert!(blocked_people
        .iter()
        .all(|person| person.peer_discord_id != REMOVED_PERSON));
    assert!(tabs_after_removal.blocked.iter().all(|request| {
        request.requester_id != REMOVED_PERSON && request.target_id != REMOVED_PERSON
    }));
    let blocked_count = blocked_people.len() + tabs_after_removal.blocked.len();
    assert_eq!(blocked_count, 0);

    create_request(&state, NEW_REQUEST);
    let after_new_request = cmd_osl_list_friend_requests(&state).expect("list new request");
    let pending_count = after_new_request.pending.len();
    assert_eq!(pending_count, 1);
    assert_eq!(after_new_request.pending[0].request_id, NEW_REQUEST);
    assert_eq!(after_new_request.pending[0].requester_id, REMOVED_PERSON);
    assert_eq!(after_new_request.pending[0].target_id, LOCAL_ID);
    assert_eq!(
        after_new_request.pending[0].block_state,
        StoredFriendBlockState::NotBlocked
    );

    let accepted = cmd_osl_accept_saved_friend_request(&state, NEW_REQUEST.to_owned())
        .expect("accept the fresh request");
    assert_eq!(accepted.state, StoredFriendState::Accepted);
    assert_eq!(accepted.block_state, StoredFriendBlockState::NotBlocked);

    let after_accept = cmd_osl_list_friend_requests(&state).expect("list accepted request");
    assert!(after_accept.pending.is_empty());
    assert_eq!(after_accept.accepted.len(), 1);
    assert_eq!(after_accept.accepted[0].request_id, NEW_REQUEST);
    let allowed_places_after_accept =
        count_allowed_place_records_for_person(allowed_place_dir.path(), REMOVED_PERSON).unwrap();
    assert_eq!(allowed_places_after_accept, 0);

    println!(
        "TASK_0278 removed_person={} blocked_people_count={} blocked_tab_count={} blocked_count={} pending_count={} accepted_count={} old_allowed_places={} allowed_places_after_accept={}",
        REMOVED_PERSON,
        blocked_people.len(),
        tabs_after_removal.blocked.len(),
        blocked_count,
        pending_count,
        after_accept.accepted.len(),
        old_allowed_places,
        allowed_places_after_accept,
    );
}
