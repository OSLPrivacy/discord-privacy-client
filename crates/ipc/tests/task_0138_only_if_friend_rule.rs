use ipc::allowed_places::{allowed_places_db_path, AllowedPlaceRecord};
use ipc::commands::{
    cmd_osl_accept_friend_request, cmd_osl_new_place, cmd_osl_save_auto_whitelist_rule,
    cmd_osl_send_friend_request,
};
use ipc::peer_map::PeerEntry;
use ipc::scope::{Scope, ScopeInput};
use ipc::state::AppState;
use ipc::tofu::KeyBundle;
use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;

static CONFIG_DIR_LOCK: Mutex<()> = Mutex::new(());

const SELF_ID: &str = "900000000000013800";
const ACCEPTED_FRIEND_ID: &str = "900000000000013801";
const NON_FRIEND_ID: &str = "900000000000013802";

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
    ipc::main_password::set_file_storage_key(Some([0x38; 32]));
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

fn state_with_peer_binding() -> AppState {
    let state = AppState::new();
    let mut identity = keystore::generate_identity("task-0138-self".to_owned());
    identity.discord_snowflake = Some(SELF_ID.to_owned());
    state.install_identity(identity);
    state.peer_map.lock().unwrap().insert(
        ACCEPTED_FRIEND_ID.to_owned(),
        PeerEntry {
            discord_id: Some(ACCEPTED_FRIEND_ID.to_owned()),
            tofu_key_bundle: Some(trusted_peer_bundle("task-0138-accepted")),
            ..PeerEntry::default()
        },
    );
    state
}

fn accept_friend(state: &AppState) {
    let scope = Scope::dm(ACCEPTED_FRIEND_ID);
    let sent = cmd_osl_send_friend_request(
        state,
        ACCEPTED_FRIEND_ID.to_owned(),
        ScopeInput::from(&scope),
    )
    .expect("accepted-friend fixture request is created");
    cmd_osl_accept_friend_request(state, ACCEPTED_FRIEND_ID.to_owned(), sent.request)
        .expect("accepted-friend fixture is accepted");
}

fn allowed_place_count(dir: &Path) -> i64 {
    let path = allowed_places_db_path(dir);
    if !path.exists() {
        return 0;
    }
    let conn = Connection::open(path).expect("open allowed places db");
    conn.query_row("SELECT COUNT(*) FROM allowed_places", [], |row| row.get(0))
        .expect("count allowed places")
}

#[test]
fn only_if_friend_allows_accepted_friend_and_skips_non_friend() {
    let _lock = CONFIG_DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let _config_dir = use_temp_config_dir(dir.path());
    let state = state_with_peer_binding();
    accept_friend(&state);
    cmd_osl_save_auto_whitelist_rule(
        &state,
        "discord".to_owned(),
        "only if a friend".to_owned(),
        None,
    )
    .expect("save only-if-friend rule");

    let before = allowed_place_count(dir.path());
    let accepted = cmd_osl_new_place(
        &state,
        AllowedPlaceRecord::discord_direct_message(SELF_ID, ACCEPTED_FRIEND_ID),
    )
    .expect("accepted-friend fixture is evaluated");
    let after_friend = allowed_place_count(dir.path());
    let non_friend = cmd_osl_new_place(
        &state,
        AllowedPlaceRecord::discord_direct_message(SELF_ID, NON_FRIEND_ID),
    )
    .expect("non-friend fixture is evaluated");
    let after_non_friend = allowed_place_count(dir.path());

    println!(
        "TASK0138 accepted_friend.result={} stable_id={} count_before={} count_after={}",
        accepted.result, accepted.stable_id, before, after_friend
    );
    println!(
        "TASK0138 non_friend.result={} stable_id={} count_after={}",
        non_friend.result, non_friend.stable_id, after_non_friend
    );

    assert_eq!(accepted.rule_choice, "only if a friend");
    assert_eq!(accepted.result, "allowed");
    assert_eq!(after_friend - before, 1);
    assert_eq!(non_friend.rule_choice, "only if a friend");
    assert_eq!(non_friend.result, "skipped");
    assert_eq!(after_non_friend, after_friend);
}
