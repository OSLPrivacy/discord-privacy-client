use ipc::commands::{
    cmd_osl_accept_friend_request, cmd_osl_decline_or_revoke_friend_request,
    cmd_osl_get_friend_ids, cmd_osl_send_friend_request, FriendRequestDecision,
};
use ipc::peer_map::PeerEntry;
use ipc::scope::{Scope, ScopeInput};
use ipc::state::AppState;
use ipc::tofu::KeyBundle;
use std::path::Path;
use std::sync::Mutex;

static CONFIG_DIR_LOCK: Mutex<()> = Mutex::new(());

const SELF_ID: &str = "900000000000022301";
const PEER_ID: &str = "900000000000022302";

fn bundle(label: &str) -> KeyBundle {
    KeyBundle {
        ed25519_pub: format!("{label}-ed25519"),
        x25519_pub: format!("{label}-x25519"),
        mlkem768_pub: format!("{label}-mlkem768"),
        ratchet_initial_pub: Some(format!("{label}-ratchet")),
    }
}

struct ConfigDirGuard;

impl Drop for ConfigDirGuard {
    fn drop(&mut self) {
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
    }
}

fn use_temp_config_dir(dir: &Path) -> ConfigDirGuard {
    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(Some(dir.to_path_buf()));
    ConfigDirGuard
}

fn state_with_identity() -> AppState {
    let state = AppState::new();
    let mut identity = keystore::generate_identity("task-0223-self".to_owned());
    identity.discord_snowflake = Some(SELF_ID.to_owned());
    state.install_identity(identity);
    state.peer_map.lock().unwrap().insert(
        PEER_ID.to_owned(),
        PeerEntry {
            discord_id: Some(PEER_ID.to_owned()),
            tofu_key_bundle: Some(bundle("task-0223-peer")),
            ..PeerEntry::default()
        },
    );
    state
}

#[test]
fn task_0223_accept_adds_both_people_to_all_and_decline_adds_none() {
    let _lock = CONFIG_DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let _config_dir = use_temp_config_dir(dir.path());
    let accepted_state = state_with_identity();
    let scope = Scope::dm(PEER_ID);
    let sent = cmd_osl_send_friend_request(
        &accepted_state,
        PEER_ID.to_owned(),
        ScopeInput::from(&scope),
    )
    .expect("public command mints a typed friend request");
    cmd_osl_accept_friend_request(&accepted_state, PEER_ID.to_owned(), sent.request)
        .expect("accepting a typed friend request succeeds");

    let mut accepted_all = cmd_osl_get_friend_ids(&accepted_state).expect("read All after accept");
    accepted_all.sort();
    let accepted_joined = accepted_all.join(",");
    println!("TASK0223 accept_all.count={}", accepted_all.len());
    println!("TASK0223 accept_all.identities={accepted_joined}");
    assert_eq!(accepted_all.len(), 2);
    assert!(accepted_all.iter().any(|id| id == SELF_ID));
    assert!(accepted_all.iter().any(|id| id == PEER_ID));

    let declined_state = state_with_identity();
    let declined = cmd_osl_decline_or_revoke_friend_request(
        &declined_state,
        PEER_ID.to_owned(),
        ScopeInput::from(&Scope::dm(PEER_ID)),
        false,
    )
    .expect("declining a pending request is a closed no-op");
    assert_eq!(declined.decision, FriendRequestDecision::DeclinedPending);
    assert!(!declined.revoked_grant);

    let declined_all = cmd_osl_get_friend_ids(&declined_state).expect("read All after decline");
    let decline_contains_self = declined_all.iter().any(|id| id == SELF_ID);
    let decline_contains_peer = declined_all.iter().any(|id| id == PEER_ID);
    println!("TASK0223 decline_all.count={}", declined_all.len());
    println!("TASK0223 decline_all.contains_self={decline_contains_self}");
    println!("TASK0223 decline_all.contains_peer={decline_contains_peer}");
    assert_eq!(declined_all.len(), 0);
    assert!(!decline_contains_self);
    assert!(!decline_contains_peer);
}
