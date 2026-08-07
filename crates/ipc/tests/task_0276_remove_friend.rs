//! Task 0276: remove one accepted friend.
//!
//! Removing one named friend must clear exactly that friendship, exactly that
//! friend's allowed places, and exactly that friend's picture access, and must
//! leave the other accepted friend untouched.

use ipc::allowed_places::{
    add_allowed_place_record, count_allowed_place_records_for_person, AllowedPlaceRecord,
};
use ipc::commands::{
    cmd_osl_accept_saved_friend_request, cmd_osl_create_friend_request,
    cmd_osl_picture_access_allowed, cmd_osl_query_friends_tabs, cmd_osl_remove_friend,
    cmd_osl_set_friend_ids,
};
use ipc::peer_map::{PeerEntry, WhitelistEntry};
use ipc::scope::Scope;
use ipc::state::AppState;
use std::path::Path;
use std::sync::Mutex;

static CONFIG_DIR_LOCK: Mutex<()> = Mutex::new(());

const LOCAL_ID: &str = "AMBER-0276";
const REMOVED: &str = "900000000000027601";
const KEPT: &str = "900000000000027602";

struct ConfigDirGuard;

impl Drop for ConfigDirGuard {
    fn drop(&mut self) {
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
        ipc::main_password::set_file_storage_key(None);
    }
}

fn use_temp_config_dir(dir: &Path) -> ConfigDirGuard {
    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(Some(dir.to_path_buf()));
    ipc::main_password::set_file_storage_key(Some([0x76; 32]));
    ConfigDirGuard
}

/// Seed one accepted friend: the saved friend row the Friends tabs read, plus
/// the scoped grant `friendship_state` reads.
fn seed_accepted_friend(state: &AppState, label: &str, person_id: &str) -> String {
    let request_id = format!("REQ-0276-{label}");
    let scope = Scope::dm(person_id);
    cmd_osl_create_friend_request(
        state,
        request_id.clone(),
        LOCAL_ID.to_owned(),
        person_id.to_owned(),
        format!("Friend {label}"),
        (&scope).into(),
    )
    .expect("seed friend request");
    cmd_osl_accept_saved_friend_request(state, request_id.clone()).expect("accept friend request");
    state.peer_map.lock().unwrap().insert(
        person_id.to_owned(),
        PeerEntry {
            discord_id: Some(person_id.to_owned()),
            outgoing_whitelists: vec![WhitelistEntry::Dm {
                broadened: true,
                enabled_at: Some("2026-08-07T00:00:00Z".to_owned()),
            }],
            ..PeerEntry::default()
        },
    );
    request_id
}

/// Three allowed places that came from one friendship: same person, three
/// owned accounts.
fn seed_allowed_places(dir: &Path, person_id: &str) {
    for account in ["account-a", "account-b", "account-c"] {
        add_allowed_place_record(
            dir,
            AllowedPlaceRecord::discord_direct_message(account, person_id),
        )
        .expect("seed allowed place");
    }
}

fn accepted_friend_count(state: &AppState) -> usize {
    let tabs = cmd_osl_query_friends_tabs(state).expect("query friends tabs");
    tabs.online.len() + tabs.all.len()
}

fn picture_access_allowed(state: &AppState, person_id: &str) -> bool {
    cmd_osl_picture_access_allowed(state, person_id.to_owned()).expect("read picture access")
}

#[test]
fn task_0276_removing_one_named_friend_clears_only_that_friendship() {
    let _lock = CONFIG_DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let allowed_dir = tempfile::tempdir().unwrap();
    let _config_dir = use_temp_config_dir(dir.path());
    let state = AppState::new();

    seed_accepted_friend(&state, "removed", REMOVED);
    seed_accepted_friend(&state, "kept", KEPT);
    seed_allowed_places(allowed_dir.path(), REMOVED);
    seed_allowed_places(allowed_dir.path(), KEPT);
    cmd_osl_set_friend_ids(&state, vec![REMOVED.to_owned(), KEPT.to_owned()])
        .expect("seed friend-id snapshot the picture guard reads");

    let accepted_before = accepted_friend_count(&state);
    let removed_places_before =
        count_allowed_place_records_for_person(allowed_dir.path(), REMOVED).unwrap();
    let kept_places_before =
        count_allowed_place_records_for_person(allowed_dir.path(), KEPT).unwrap();
    let removed_picture_before = picture_access_allowed(&state, REMOVED);
    let kept_picture_before = picture_access_allowed(&state, KEPT);
    println!(
        "TASK_0276_BEFORE accepted_friends={accepted_before} \
         removed_allowed_places={removed_places_before} \
         kept_allowed_places={kept_places_before} \
         removed_picture_access={removed_picture_before} \
         kept_picture_access={kept_picture_before}"
    );
    assert_eq!(accepted_before, 2);
    assert_eq!(removed_places_before, 3);
    assert_eq!(kept_places_before, 3);
    assert!(removed_picture_before);
    assert!(kept_picture_before);

    let result = cmd_osl_remove_friend(
        &state,
        REMOVED.to_owned(),
        Some(allowed_dir.path().to_path_buf()),
    )
    .expect("remove one accepted friend");

    let accepted_after = accepted_friend_count(&state);
    let removed_places_after =
        count_allowed_place_records_for_person(allowed_dir.path(), REMOVED).unwrap();
    let kept_places_after =
        count_allowed_place_records_for_person(allowed_dir.path(), KEPT).unwrap();
    let removed_picture_after = picture_access_allowed(&state, REMOVED);
    let kept_picture_after = picture_access_allowed(&state, KEPT);
    println!(
        "TASK_0276_AFTER removed_person={} accepted_friends={accepted_after} \
         removed_allowed_places={removed_places_after} \
         kept_allowed_places={kept_places_after} \
         removed_picture_access={removed_picture_after} \
         kept_picture_access={kept_picture_after}",
        result.person_id
    );
    println!(
        "TASK_0276_RESULT accepted_friend_count={} friendship_state={} grants_removed={} \
         allowed_places_removed={} allowed_places={} picture_access_allowed={} request_id={}",
        result.accepted_friend_count,
        result.friendship_state,
        result.grants_removed,
        result.allowed_places_removed,
        result.allowed_places,
        result.picture_access_allowed,
        result.removed_request_id
    );

    // accepted friend count 2 -> 1, and the survivor is the other person.
    assert_eq!(accepted_after, 1);
    assert_eq!(result.accepted_friend_count, 1);
    let tabs = cmd_osl_query_friends_tabs(&state).expect("query friends tabs");
    let survivors = tabs
        .online
        .iter()
        .chain(tabs.all.iter())
        .map(|row| row.target_id.clone())
        .collect::<Vec<_>>();
    println!("TASK_0276_SURVIVING_FRIENDS ids={}", survivors.join(","));
    assert_eq!(survivors, vec![KEPT.to_owned()]);

    // the removed person's friendship, allowed places and picture access are gone
    assert_eq!(removed_places_after, 0);
    assert_eq!(result.allowed_places, 0);
    assert_eq!(result.allowed_places_removed, 3);
    assert_eq!(result.friendship_state, "none");
    assert_eq!(result.grants_removed, 1);
    assert!(!removed_picture_after);
    assert!(!result.picture_access_allowed);
    assert!(state
        .peer_map
        .lock()
        .unwrap()
        .get(REMOVED)
        .map(|entry| entry.outgoing_whitelists.is_empty())
        .unwrap_or(true));

    // the other friend is untouched
    assert_eq!(kept_places_after, 3);
    assert!(kept_picture_after);
    assert!(!state
        .peer_map
        .lock()
        .unwrap()
        .get(KEPT)
        .map(|entry| entry.outgoing_whitelists.is_empty())
        .unwrap_or(true));
}

#[test]
fn task_0276_removing_a_person_who_is_not_an_accepted_friend_is_refused() {
    let _lock = CONFIG_DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let allowed_dir = tempfile::tempdir().unwrap();
    let _config_dir = use_temp_config_dir(dir.path());
    let state = AppState::new();

    seed_accepted_friend(&state, "kept", KEPT);
    seed_allowed_places(allowed_dir.path(), KEPT);

    let error = cmd_osl_remove_friend(
        &state,
        REMOVED.to_owned(),
        Some(allowed_dir.path().to_path_buf()),
    )
    .expect_err("a stranger is not a removable friend");
    let kept_places = count_allowed_place_records_for_person(allowed_dir.path(), KEPT).unwrap();
    println!(
        "TASK_0276_STRANGER err=\"{error}\" accepted_friends={} kept_allowed_places={kept_places}",
        accepted_friend_count(&state)
    );
    assert_eq!(error, "OSL: friend is not accepted");
    assert_eq!(accepted_friend_count(&state), 1);
    assert_eq!(kept_places, 3);
}
