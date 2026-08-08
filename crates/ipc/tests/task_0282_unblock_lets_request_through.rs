//! Task 0282: unblocking lets a request through again.
//!
//! Block a person, prove their request is refused, unblock them, then have them
//! send a new request. The refusal has to name the block, the request that
//! follows the unblock has to land as pending with count 1, and the person has
//! to still hold zero allowed places -- an unblock restores the ability to ask,
//! not any of the reach the block took away.

use ipc::allowed_places::{
    add_allowed_place_record, count_allowed_place_records_for_person, AllowedPlaceRecord,
};
use ipc::commands::{
    cmd_osl_block_friend_request, cmd_osl_count_pending_friend_requests,
    cmd_osl_list_blocked_people, cmd_osl_send_friend_request, cmd_osl_unblock_person,
};
use ipc::peer_map::{PeerEntry, WhitelistEntry};
use ipc::scope::Scope;
use ipc::state::AppState;
use ipc::tofu::KeyBundle;
use std::io::{self, Write};
use std::path::Path;
use std::sync::Mutex;

static CONFIG_DIR_LOCK: Mutex<()> = Mutex::new(());

const PERSON: &str = "900000000000002820";
const NEIGHBOUR: &str = "900000000000002821";
const SELF_DID: &str = "900000000000002899";

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
    ipc::main_password::set_file_storage_key(Some([0x82; 32]));
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

fn seed_peer(state: &AppState, person_id: &str) {
    state.peer_map.lock().unwrap().insert(
        person_id.to_string(),
        PeerEntry {
            discord_id: Some(person_id.to_string()),
            tofu_key_bundle: Some(trusted_peer_bundle(&format!("task-0282-{person_id}"))),
            outgoing_whitelists: vec![WhitelistEntry::Dm {
                broadened: true,
                enabled_at: Some("2026-08-06T00:00:00Z".to_string()),
            }],
            ..PeerEntry::default()
        },
    );
}

fn task_0282_state() -> AppState {
    let state = AppState::new();
    let mut identity = keystore::generate_identity("task-0282-owner".to_string());
    identity.discord_snowflake = Some(SELF_DID.to_string());
    state.install_identity(identity);
    seed_peer(&state, PERSON);
    state
}

/// Three real allowed places for the person, so "still has 0 allowed places" is
/// a number the block had to take away rather than one that was never there.
fn seed_allowed_places(allowed_dir: &Path, person_id: &str) {
    for account in ["account-a", "account-b", "account-c"] {
        add_allowed_place_record(
            allowed_dir,
            AllowedPlaceRecord::discord_direct_message(account, person_id),
        )
        .unwrap();
    }
}

fn allowed_places(allowed_dir: &Path, person_id: &str) -> usize {
    count_allowed_place_records_for_person(allowed_dir, person_id).unwrap()
}

#[test]
fn task_0282_unblocking_lets_a_refused_request_through_again() {
    let _lock = CONFIG_DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let allowed_dir = tempfile::tempdir().unwrap();
    let _config_dir = use_temp_config_dir(dir.path());
    let state = task_0282_state();
    let scope = Scope::dm(PERSON);

    seed_allowed_places(allowed_dir.path(), PERSON);
    assert_eq!(
        allowed_places(allowed_dir.path(), PERSON),
        3,
        "the fixture must start with reach for the block to remove"
    );

    // 1. Block the person.
    let blocked = cmd_osl_block_friend_request(
        &state,
        PERSON.to_string(),
        (&scope).into(),
        Some(allowed_dir.path().to_path_buf()),
    )
    .unwrap();
    println!(
        "TASK_0282_BLOCK person={} blocked_count={} friendship_state={} allowed_places={}",
        blocked.person_id, blocked.blocked_count, blocked.friendship_state, blocked.allowed_places
    );
    assert_eq!(blocked.blocked_count, 1);
    assert_eq!(blocked.friendship_state, "blocked");
    assert_eq!(blocked.allowed_places, 0);
    assert!(cmd_osl_list_blocked_people(&state)
        .unwrap()
        .iter()
        .any(|record| record.peer_discord_id == PERSON && record.state == "Blocked"));

    // 2. The first request, sent while the block stands, is refused by name.
    let first_error =
        match cmd_osl_send_friend_request(&state, PERSON.to_string(), (&scope).into()) {
            Ok(_) => panic!("a blocked person's request must not be accepted"),
            Err(error) => error,
        };
    let after_refusal = cmd_osl_count_pending_friend_requests(&state, PERSON.to_string()).unwrap();
    println!(
        "TASK_0282_FIRST_REQUEST exit=1 err=\"{}\" pending_for_person={} pending_total={}",
        first_error, after_refusal.for_person, after_refusal.total
    );
    assert_eq!(first_error, "OSL: friend request peer is blocked");
    assert!(
        first_error.contains("blocked"),
        "the refusal has to name the block: {first_error}"
    );
    assert_eq!(after_refusal.for_person, 0);
    assert_eq!(after_refusal.total, 0);

    // 3. Unblock the person.
    let unblocked = cmd_osl_unblock_person(
        &state,
        PERSON.to_string(),
        Some(allowed_dir.path().to_path_buf()),
    )
    .unwrap();
    println!(
        "TASK_0282_UNBLOCK person={} removed_from_blocked={} blocked_count={} friendship_state={} allowed_places={}",
        unblocked.person_id,
        unblocked.removed_from_blocked,
        unblocked.blocked_count,
        unblocked.friendship_state,
        unblocked.allowed_places
    );
    assert!(unblocked.removed_from_blocked);
    assert_eq!(unblocked.blocked_count, 0);
    assert_eq!(unblocked.friendship_state, "none");
    assert!(cmd_osl_list_blocked_people(&state)
        .unwrap()
        .iter()
        .all(|record| record.peer_discord_id != PERSON));

    // 4. The same person sends a new request, and this one arrives as pending.
    let second = cmd_osl_send_friend_request(&state, PERSON.to_string(), (&scope).into())
        .expect("an unblocked person's request must be let through");
    let after_second = cmd_osl_count_pending_friend_requests(&state, PERSON.to_string()).unwrap();
    println!(
        "TASK_0282_SECOND_REQUEST exit=0 pending_peer={} pending_for_person={} pending_total={}",
        second.pending.peer_discord_id, after_second.for_person, after_second.total
    );
    assert_eq!(second.pending.peer_discord_id, PERSON);
    assert_eq!(second.pending.scope_storage_key, scope.storage_key());
    assert_eq!(after_second.for_person, 1);
    assert_eq!(after_second.total, 1);

    // 5. The unblock returned the ability to ask, not the reach.
    let places = allowed_places(allowed_dir.path(), PERSON);
    println!("TASK_0282_ALLOWED_PLACES person={PERSON} allowed_places={places}");
    assert_eq!(
        places, 0,
        "an unblock plus a fresh request must not restore any allowed place"
    );
}

#[test]
fn task_0282_unblocking_one_person_leaves_a_neighbours_block_standing() {
    let _lock = CONFIG_DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let allowed_dir = tempfile::tempdir().unwrap();
    let _config_dir = use_temp_config_dir(dir.path());
    let state = task_0282_state();
    seed_peer(&state, NEIGHBOUR);

    for person in [PERSON, NEIGHBOUR] {
        cmd_osl_block_friend_request(
            &state,
            person.to_string(),
            (&Scope::dm(person)).into(),
            Some(allowed_dir.path().to_path_buf()),
        )
        .unwrap();
    }

    cmd_osl_unblock_person(
        &state,
        PERSON.to_string(),
        Some(allowed_dir.path().to_path_buf()),
    )
    .unwrap();

    cmd_osl_send_friend_request(&state, PERSON.to_string(), (&Scope::dm(PERSON)).into())
        .expect("the unblocked person is let through");
    let neighbour_error = match cmd_osl_send_friend_request(
        &state,
        NEIGHBOUR.to_string(),
        (&Scope::dm(NEIGHBOUR)).into(),
    ) {
        Ok(_) => panic!("the neighbour's block must still stand"),
        Err(error) => error,
    };

    let counted = cmd_osl_count_pending_friend_requests(&state, PERSON.to_string()).unwrap();
    println!(
        "TASK_0282_NEIGHBOUR unblocked_pending={} neighbour_err=\"{}\" pending_total={}",
        counted.for_person, neighbour_error, counted.total
    );
    assert_eq!(neighbour_error, "OSL: friend request peer is blocked");
    assert_eq!(counted.for_person, 1);
    assert_eq!(counted.total, 1);
}

/// The exit-code probe for the finish line's "the first request exits 1".
/// Ignored by default because it ends the test process; run it on its own with
/// `--ignored --exact` and read `$?`.
#[test]
#[ignore = "task 0282 probe: exits 1 when the blocked person's first request is refused"]
fn task_0282_first_request_probe_exits_1_while_blocked() {
    let _lock = CONFIG_DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let allowed_dir = tempfile::tempdir().unwrap();
    let _config_dir = use_temp_config_dir(dir.path());
    let state = task_0282_state();
    let scope = Scope::dm(PERSON);

    seed_allowed_places(allowed_dir.path(), PERSON);
    cmd_osl_block_friend_request(
        &state,
        PERSON.to_string(),
        (&scope).into(),
        Some(allowed_dir.path().to_path_buf()),
    )
    .unwrap();

    match cmd_osl_send_friend_request(&state, PERSON.to_string(), (&scope).into()) {
        Err(error) if error == "OSL: friend request peer is blocked" => {
            let _ = writeln!(
                io::stderr(),
                "TASK_0282_FIRST_REQUEST_EXIT_CODE=1 err=\"{error}\""
            );
            let _ = io::stderr().flush();
            std::process::exit(1);
        }
        Err(error) => {
            let _ = writeln!(
                io::stderr(),
                "TASK_0282_FIRST_REQUEST_EXIT_CODE=2 err=\"{error}\""
            );
            let _ = io::stderr().flush();
            std::process::exit(2);
        }
        Ok(_) => {
            let _ = writeln!(
                io::stderr(),
                "TASK_0282_FIRST_REQUEST_EXIT_CODE=0 accepted=true"
            );
            let _ = io::stderr().flush();
            std::process::exit(0);
        }
    }
}

/// The matching probe for the request that follows the unblock: same person,
/// same scope, and this one has to exit 0 with a pending count of 1 and no
/// allowed places handed back.
#[test]
#[ignore = "task 0282 probe: exits 0 when the unblocked person's second request lands"]
fn task_0282_second_request_probe_exits_0_after_unblock() {
    let _lock = CONFIG_DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let allowed_dir = tempfile::tempdir().unwrap();
    let _config_dir = use_temp_config_dir(dir.path());
    let state = task_0282_state();
    let scope = Scope::dm(PERSON);

    seed_allowed_places(allowed_dir.path(), PERSON);
    cmd_osl_block_friend_request(
        &state,
        PERSON.to_string(),
        (&scope).into(),
        Some(allowed_dir.path().to_path_buf()),
    )
    .unwrap();
    let _refused = cmd_osl_send_friend_request(&state, PERSON.to_string(), (&scope).into());
    cmd_osl_unblock_person(
        &state,
        PERSON.to_string(),
        Some(allowed_dir.path().to_path_buf()),
    )
    .unwrap();

    match cmd_osl_send_friend_request(&state, PERSON.to_string(), (&scope).into()) {
        Ok(sent) => {
            let counted =
                cmd_osl_count_pending_friend_requests(&state, PERSON.to_string()).unwrap();
            let places = allowed_places(allowed_dir.path(), PERSON);
            let _ = writeln!(
                io::stderr(),
                "TASK_0282_SECOND_REQUEST_EXIT_CODE=0 peer={} pending_count={} allowed_places={}",
                sent.pending.peer_discord_id,
                counted.for_person,
                places
            );
            let _ = io::stderr().flush();
            if counted.for_person != 1 || places != 0 {
                std::process::exit(3);
            }
            std::process::exit(0);
        }
        Err(error) => {
            let _ = writeln!(
                io::stderr(),
                "TASK_0282_SECOND_REQUEST_EXIT_CODE=1 err=\"{error}\""
            );
            let _ = io::stderr().flush();
            std::process::exit(1);
        }
    }
}
