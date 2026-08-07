//! Task 0224: block a person.

use ipc::commands::{
    cmd_osl_block_friend_request, cmd_osl_create_friend_request, cmd_osl_list_blocked_people,
    cmd_osl_list_friend_requests, FriendRequestDecision,
};
use ipc::peer_map::{PeerEntry, WhitelistEntry};
use ipc::scope::Scope;
use ipc::state::AppState;
use ipc::tofu::KeyBundle;
use std::io::{self, Write};
use std::path::Path;
use std::sync::Mutex;

static CONFIG_DIR_LOCK: Mutex<()> = Mutex::new(());

const BLOCKED_PEER_DID: &str = "900000000000002224";

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
    ipc::main_password::set_file_storage_key(Some([0x24; 32]));
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
    let mut identity = keystore::generate_identity("task-0224-blocker".to_string());
    identity.discord_snowflake = Some("900000000000002299".to_string());
    state.install_identity(identity);
    state.peer_map.lock().unwrap().insert(
        BLOCKED_PEER_DID.to_string(),
        PeerEntry {
            discord_id: Some(BLOCKED_PEER_DID.to_string()),
            tofu_key_bundle: Some(trusted_peer_bundle("task-0224-peer")),
            outgoing_whitelists: vec![WhitelistEntry::Dm {
                broadened: true,
                enabled_at: Some("2026-08-06T00:00:00Z".to_string()),
            }],
            ..PeerEntry::default()
        },
    );
    state
}

fn block_peer(state: &AppState, scope: &Scope) {
    let created = cmd_osl_create_friend_request(state, BLOCKED_PEER_DID.to_string(), scope.into())
        .expect("setup pending request before blocking");
    assert_eq!(created.pending.peer_discord_id, BLOCKED_PEER_DID);
    let blocked =
        cmd_osl_block_friend_request(state, BLOCKED_PEER_DID.to_string(), scope.into()).unwrap();
    assert_eq!(
        blocked.decision,
        FriendRequestDecision::RevokedAcceptedGrant
    );
    assert!(blocked.revoked_grant);
}

#[test]
fn blocked_query_contains_person_and_future_request_is_refused() {
    let _lock = CONFIG_DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let _config_dir = use_temp_config_dir(dir.path());
    let state = friend_request_state();
    let scope = Scope::dm(BLOCKED_PEER_DID);

    block_peer(&state, &scope);

    let blocked = cmd_osl_list_blocked_people(&state).unwrap();
    let blocked_person = blocked
        .iter()
        .find(|record| record.peer_discord_id == BLOCKED_PEER_DID)
        .expect("blocked query must contain the blocked peer");
    println!(
        "TASK_0224_BLOCKED_QUERY count={} peer={} state={}",
        blocked.len(),
        blocked_person.peer_discord_id,
        blocked_person.state
    );
    assert_eq!(blocked_person.state, "Blocked");

    let request_err = match cmd_osl_create_friend_request(
        &state,
        BLOCKED_PEER_DID.to_string(),
        (&scope).into(),
    ) {
        Ok(_) => panic!("blocked peer must not be able to create a new request"),
        Err(err) => err,
    };
    let pending_after = cmd_osl_list_friend_requests(&state).unwrap();
    println!(
        "TASK_0224_NEW_REQUEST_FROM_BLOCKED err=\"{}\" pending_count={}",
        request_err,
        pending_after.len()
    );
    assert_eq!(request_err, "OSL: friend request peer is blocked");
    assert!(pending_after.is_empty());
}

#[test]
#[ignore = "task 0224 probe: exits 1 when the blocked future request is refused"]
fn blocked_future_request_probe_exits_1_when_refused() {
    let _lock = CONFIG_DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let _config_dir = use_temp_config_dir(dir.path());
    let state = friend_request_state();
    let scope = Scope::dm(BLOCKED_PEER_DID);

    block_peer(&state, &scope);

    match cmd_osl_create_friend_request(&state, BLOCKED_PEER_DID.to_string(), (&scope).into()) {
        Err(err) if err == "OSL: friend request peer is blocked" => {
            let _ = writeln!(
                io::stderr(),
                "TASK_0224_BLOCKED_REQUEST_EXIT_CODE=1 err=\"{}\"",
                err
            );
            let _ = io::stderr().flush();
            std::process::exit(1);
        }
        Err(err) => {
            let _ = writeln!(
                io::stderr(),
                "TASK_0224_BLOCKED_REQUEST_EXIT_CODE=2 err=\"{}\"",
                err
            );
            let _ = io::stderr().flush();
            std::process::exit(2);
        }
        Ok(_) => {
            let _ = writeln!(
                io::stderr(),
                "TASK_0224_BLOCKED_REQUEST_EXIT_CODE=0 accepted=true"
            );
            let _ = io::stderr().flush();
            std::process::exit(0);
        }
    }
}
