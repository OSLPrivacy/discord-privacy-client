use ipc::commands::{
    cmd_osl_accept_saved_friend_request, cmd_osl_create_friend_request, cmd_osl_query_friends_tabs,
};
use ipc::scope::Scope;
use ipc::state::AppState;
use std::path::Path;
use std::sync::Mutex;

static CONFIG_DIR_LOCK: Mutex<()> = Mutex::new(());

const LOCAL_ID: &str = "MAPLE-3673";
const FRIEND_COUNT: usize = 100;

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

#[test]
fn make_hundred_test_friends() {
    let _lock = CONFIG_DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("tempdir");
    let _config_dir = use_temp_config_dir(dir.path());
    let state = AppState::new();

    for i in 1..=FRIEND_COUNT {
        let peer_id = format!("900000000000{:08}", 3673000 + i);
        let friend_name = format!("{}-{:05}", LOCAL_ID, i);
        let request_id = format!("REQ-3673-{:05}", i);

        let scope = Scope::dm(&peer_id);
        let _ = cmd_osl_create_friend_request(
            &state,
            request_id.clone(),
            LOCAL_ID.to_string(),
            peer_id.clone(),
            friend_name,
            (&scope).into(),
        )
        .expect("create friend request");

        let _ =
            cmd_osl_accept_saved_friend_request(&state, request_id).expect("accept friend request");
    }

    let tabs = cmd_osl_query_friends_tabs(&state).expect("query friends tabs");

    println!(
        "TASK_3673_HUNDRED_FRIENDS created_count={} all_count={} online_count={} pending_count={} blocked_count={}",
        FRIEND_COUNT,
        tabs.all.len(),
        tabs.online.len(),
        tabs.pending.len(),
        tabs.blocked.len()
    );

    assert_eq!(
        tabs.all.len(),
        FRIEND_COUNT,
        "expected {} accepted friends in 'all' tab",
        FRIEND_COUNT
    );
}
