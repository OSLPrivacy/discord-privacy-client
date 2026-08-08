use ipc::commands::{
    cmd_osl_accept_saved_friend_request, cmd_osl_create_friend_request, cmd_osl_query_friends_tabs,
};
use ipc::scope::Scope;
use ipc::AppState;
use std::process::ExitCode;

const LOCAL_ID: &str = "MAPLE-3673";
const FRIEND_COUNT: usize = 10_000;

fn main() -> ExitCode {
    let temp_dir = std::env::temp_dir().join(format!("osl-3673-{}", std::process::id()));
    if let Err(e) = std::fs::create_dir_all(&temp_dir) {
        eprintln!("OSL: failed to create temp dir: {e}");
        return ExitCode::from(1);
    }

    ipc::main_password::set_file_storage_key(None);
    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(Some(temp_dir.clone()));

    let state = AppState::new();

    match create_and_accept_friends(&state) {
        Ok(count) => {
            let tabs = match cmd_osl_query_friends_tabs(&state) {
                Ok(tabs) => tabs,
                Err(e) => {
                    eprintln!("OSL: failed to query friends tabs: {e}");
                    let _ = std::fs::remove_dir_all(&temp_dir);
                    return ExitCode::from(1);
                }
            };

            println!(
                "TASK_3673 action=create_ten_thousand_friends total_created={} all_count={} local_id={}",
                count, tabs.all.len(), LOCAL_ID
            );

            let _ = std::fs::remove_dir_all(&temp_dir);

            if tabs.all.len() == FRIEND_COUNT {
                ExitCode::SUCCESS
            } else {
                eprintln!(
                    "OSL: expected {} friends in all tab, found {}",
                    FRIEND_COUNT,
                    tabs.all.len()
                );
                ExitCode::from(1)
            }
        }
        Err(error) => {
            eprintln!("OSL: {error}");
            let _ = std::fs::remove_dir_all(&temp_dir);
            ExitCode::from(1)
        }
    }
}

fn create_and_accept_friends(state: &AppState) -> Result<usize, String> {
    let mut created = 0;

    for i in 1..=FRIEND_COUNT {
        let peer_id = format!("900000000000{:08}", 3673000 + i);
        let friend_name = format!("{}-{:05}", LOCAL_ID, i);
        let request_id = format!("REQ-3673-{:05}", i);

        let scope = Scope::dm(&peer_id);
        let _ = cmd_osl_create_friend_request(
            state,
            request_id.clone(),
            LOCAL_ID.to_string(),
            peer_id.clone(),
            friend_name,
            (&scope).into(),
        )
        .map_err(|e| format!("failed to create request {}: {}", request_id, e))?;

        let _ = cmd_osl_accept_saved_friend_request(state, request_id)
            .map_err(|e| format!("failed to accept request for {}: {}", peer_id, e))?;

        created += 1;

        if i % 1000 == 0 {
            eprintln!("OSL: created {} friends...", i);
        }
    }

    Ok(created)
}
