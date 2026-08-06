use ipc::commands::cmd_osl_burn_sender_message_records_choice;
use ipc::AppState;
use keystore::KeyServerClient;
use std::process::ExitCode;
use store::{MessageStore, StoredMessage};

const STORE_KEY: &[u8; 32] = &[0x13; 32];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(scope_choice) = args.first() else {
        eprintln!("usage: burn-choice <scope> --keyserver-url <url> [--message-id <id> ...]");
        return ExitCode::from(2);
    };

    if scope_choice != "both-sides" {
        eprintln!("OSL: unknown scope: {scope_choice}");
        return ExitCode::from(1);
    }

    let mut keyserver_url = None;
    let mut message_ids = Vec::new();
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--keyserver-url" => {
                index += 1;
                let Some(url) = args.get(index) else {
                    eprintln!("--keyserver-url requires a value");
                    return ExitCode::from(2);
                };
                keyserver_url = Some(url.clone());
            }
            "--message-id" => {
                index += 1;
                let Some(id) = args.get(index) else {
                    eprintln!("--message-id requires a value");
                    return ExitCode::from(2);
                };
                message_ids.push(id.clone());
            }
            other => {
                eprintln!("unknown argument: {other}");
                return ExitCode::from(2);
            }
        }
        index += 1;
    }

    if message_ids.is_empty() {
        message_ids.push("task0513-message-1".to_owned());
        message_ids.push("task0513-message-2".to_owned());
    }

    let Some(keyserver_url) = keyserver_url else {
        eprintln!("--keyserver-url is required for both-sides");
        return ExitCode::from(2);
    };

    match run_both_sides(scope_choice, &keyserver_url, message_ids) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn run_both_sides(
    scope_choice: &str,
    keyserver_url: &str,
    message_ids: Vec<String>,
) -> Result<(), String> {
    let state = AppState::new();
    state.install_identity(keystore::identity_from_entropy(
        [0x13; 16],
        "burner-0513".to_owned(),
    ));
    *state.keyserver.lock().expect("keyserver mutex poisoned") =
        Some(KeyServerClient::new(keyserver_url).map_err(|error| error.to_string())?);

    let store_dir = unique_store_dir();
    std::fs::create_dir_all(&store_dir).map_err(|error| format!("OSL: store dir: {error}"))?;
    let store = MessageStore::open(&store_dir, STORE_KEY)
        .map_err(|error| format!("OSL: message store: {error}"))?;
    for (index, message_id) in message_ids.iter().enumerate() {
        store
            .put(&StoredMessage {
                discord_message_id: message_id.clone(),
                channel_id: "task0513-channel".to_owned(),
                sender_discord_id: "sender-0513".to_owned(),
                sender_osl_user_id: "burner-0513".to_owned(),
                plaintext: format!("TASK0513 selected sender record {}", index + 1),
                decrypted_at: 1_900_513_000 + i64::try_from(index).unwrap_or(0),
                burned: false,
            })
            .map_err(|error| format!("OSL: seed selected record: {error}"))?;
    }
    *state
        .message_store
        .lock()
        .expect("message store mutex poisoned") = Some(store);

    let result =
        cmd_osl_burn_sender_message_records_choice(&state, scope_choice, message_ids.clone())?;
    println!(
        "TASK0513 scope_choice={scope_choice} action=cmd_osl_burn_sender_message_records_both_sides accepted=true selected_records={} local_removal_count={} remote_removal_count={} equal_removal_counts={}",
        result.requested_count,
        result.local_removal_count,
        result.remote_removal_count,
        result.equal_removal_counts
    );
    let _ = std::fs::remove_dir_all(store_dir);
    Ok(())
}

fn unique_store_dir() -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "osl-task0513-burn-choice-{}-{nanos}",
        std::process::id()
    ))
}
