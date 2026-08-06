use ipc::allowed_places::{
    add_allowed_place_record, count_allowed_place_records_for_person, AllowedPlaceRecord,
};
use ipc::commands::{
    cmd_osl_block_friend_request, cmd_osl_list_blocked_people, cmd_osl_unblock_person,
};
use ipc::peer_map::{PeerEntry, WhitelistEntry};
use ipc::scope::Scope;
use ipc::state::AppState;
use std::path::Path;
use std::sync::Mutex;

static CONFIG_DIR_LOCK: Mutex<()> = Mutex::new(());

const TARGET: &str = "900000000000002800";
const OTHER: &str = "900000000000002801";

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
    ipc::main_password::set_file_storage_key(Some([0x28; 32]));
    ConfigDirGuard
}

fn seed_accepted_friend(state: &AppState, person_id: &str) {
    state.peer_map.lock().unwrap().insert(
        person_id.to_owned(),
        PeerEntry {
            discord_id: Some(person_id.to_owned()),
            outgoing_whitelists: vec![WhitelistEntry::Dm {
                broadened: true,
                enabled_at: Some("2026-08-06T00:00:00Z".to_owned()),
            }],
            ..PeerEntry::default()
        },
    );
}

fn seed_allowed_places(dir: &Path, person_id: &str) {
    for account in ["account-a", "account-b", "account-c"] {
        add_allowed_place_record(
            dir,
            AllowedPlaceRecord::discord_direct_message(account, person_id),
        )
        .unwrap();
    }
}

fn friendship_state(state: &AppState, person_id: &str) -> &'static str {
    let accepted = state
        .peer_map
        .lock()
        .unwrap()
        .get(person_id)
        .map(|entry| !entry.outgoing_whitelists.is_empty())
        .unwrap_or(false);
    if accepted {
        "accepted"
    } else {
        "none"
    }
}

#[test]
fn task_0280_unblocking_one_named_person_leaves_no_friendship_or_allowed_places() {
    let _lock = CONFIG_DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let allowed_dir = tempfile::tempdir().unwrap();
    let _config_dir = use_temp_config_dir(dir.path());
    let state = AppState::new();
    let target_scope = Scope::dm(TARGET);
    let other_scope = Scope::dm(OTHER);

    seed_accepted_friend(&state, TARGET);
    seed_accepted_friend(&state, OTHER);
    seed_allowed_places(allowed_dir.path(), TARGET);
    assert_eq!(
        count_allowed_place_records_for_person(allowed_dir.path(), TARGET).unwrap(),
        3
    );
    assert_eq!(friendship_state(&state, TARGET), "accepted");

    cmd_osl_block_friend_request(
        &state,
        TARGET.to_owned(),
        (&target_scope).into(),
        Some(allowed_dir.path().to_path_buf()),
    )
    .unwrap();
    cmd_osl_block_friend_request(
        &state,
        OTHER.to_owned(),
        (&other_scope).into(),
        Some(allowed_dir.path().to_path_buf()),
    )
    .unwrap();

    let blocked_before = cmd_osl_list_blocked_people().unwrap();
    assert_eq!(blocked_before.len(), 2);

    let unblocked = cmd_osl_unblock_person(
        &state,
        TARGET.to_owned(),
        Some(allowed_dir.path().to_path_buf()),
    )
    .unwrap();

    let blocked_after = cmd_osl_list_blocked_people().unwrap();
    assert_eq!(blocked_after.len(), 1);
    assert!(blocked_after
        .iter()
        .any(|record| record.peer_discord_id == OTHER));
    assert!(!blocked_after
        .iter()
        .any(|record| record.peer_discord_id == TARGET));
    assert!(unblocked.removed_from_blocked);
    assert_eq!(unblocked.blocked_count, 1);
    assert_eq!(unblocked.friendship_state, "none");
    assert_eq!(unblocked.allowed_places, 0);
    assert_eq!(friendship_state(&state, TARGET), "none");

    println!(
        "TASK_0280_BLOCKED_COUNT before={} after={} unblocked={}",
        blocked_before.len(),
        blocked_after.len(),
        TARGET
    );
    println!(
        "TASK_0280_UNBLOCKED_PERSON friendship_state={} allowed_places={}",
        unblocked.friendship_state, unblocked.allowed_places
    );
}
