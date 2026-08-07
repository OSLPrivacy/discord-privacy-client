use ipc::commands::{
    cmd_osl_burn_sender_message_records_choice, cmd_osl_chat_burn_sender_message_records_choice,
};
use ipc::AppState;
use keystore::KeyServerClient;
use std::process::ExitCode;
use store::{MessageStore, StoredMessage};

const STORE_KEY: &[u8; 32] = &[0x13; 32];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(scope_choice) = args.first() else {
        eprintln!("usage: burn-choice <scope> --keyserver-url <url> [--message-id <id> ...]");
        eprintln!(
            "       burn-choice chat-burn --open-chat-id <id> --scope <scope> --keyserver-url <url>"
        );
        return ExitCode::from(2);
    };

    if scope_choice == "chat-burn" {
        return run_chat_burn_cli(&args[1..]);
    }

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

fn run_chat_burn_cli(args: &[String]) -> ExitCode {
    let mut keyserver_url = None;
    let mut open_chat_id = None;
    let mut selected_scope = None;
    let mut index = 0;
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
            "--open-chat-id" => {
                index += 1;
                let Some(id) = args.get(index) else {
                    eprintln!("--open-chat-id requires a value");
                    return ExitCode::from(2);
                };
                open_chat_id = Some(id.clone());
            }
            "--scope" => {
                index += 1;
                let Some(scope) = args.get(index) else {
                    eprintln!("--scope requires a value");
                    return ExitCode::from(2);
                };
                selected_scope = Some(scope.clone());
            }
            other => {
                eprintln!("unknown argument: {other}");
                return ExitCode::from(2);
            }
        }
        index += 1;
    }

    let Some(keyserver_url) = keyserver_url else {
        eprintln!("--keyserver-url is required for chat-burn");
        return ExitCode::from(2);
    };
    let Some(open_chat_id) = open_chat_id else {
        eprintln!("--open-chat-id is required for chat-burn");
        return ExitCode::from(2);
    };
    let Some(selected_scope) = selected_scope else {
        eprintln!("--scope is required for chat-burn");
        return ExitCode::from(2);
    };

    match run_chat_burn(&open_chat_id, &selected_scope, &keyserver_url) {
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
                reply_parent_id: None,
                edit_revision: 0,
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

fn run_chat_burn(
    open_chat_id: &str,
    selected_scope: &str,
    keyserver_url: &str,
) -> Result<(), String> {
    const SELF: &str = "burner-0525";
    const PEER: &str = "peer-0525";
    const OTHER_CHAT: &str = "task0525-other-chat";

    let state = AppState::new();
    state.install_identity(keystore::identity_from_entropy([0x25; 16], SELF.to_owned()));
    *state.keyserver.lock().expect("keyserver mutex poisoned") =
        Some(KeyServerClient::new(keyserver_url).map_err(|error| error.to_string())?);

    let store_dir = unique_store_dir_with_label("0525-chat-burn");
    std::fs::create_dir_all(&store_dir).map_err(|error| format!("OSL: store dir: {error}"))?;
    let store = MessageStore::open(&store_dir, STORE_KEY)
        .map_err(|error| format!("OSL: message store: {error}"))?;
    let selected = [
        (
            "333333333333333331",
            open_chat_id,
            SELF,
            "TASK0525 open chat sender row 1",
        ),
        (
            "333333333333333332",
            open_chat_id,
            SELF,
            "TASK0525 open chat sender row 2",
        ),
    ];
    for (index, (message_id, channel_id, sender, plaintext)) in selected.iter().enumerate() {
        store
            .put(&StoredMessage {
                discord_message_id: (*message_id).to_owned(),
                channel_id: (*channel_id).to_owned(),
                sender_discord_id: (*sender).to_owned(),
                sender_osl_user_id: (*sender).to_owned(),
                plaintext: (*plaintext).to_owned(),
                decrypted_at: 1_900_525_000 + i64::try_from(index).unwrap_or(0),
                burned: false,
                reply_parent_id: None,
                edit_revision: 0,
            })
            .map_err(|error| format!("OSL: seed open chat sender record: {error}"))?;
    }
    for (message_id, channel_id, sender, plaintext) in [
        (
            "444444444444444441",
            open_chat_id,
            PEER,
            "TASK0525 open chat peer row",
        ),
        (
            "555555555555555551",
            OTHER_CHAT,
            SELF,
            "TASK0525 other chat sender row",
        ),
    ] {
        store
            .put(&StoredMessage {
                discord_message_id: message_id.to_owned(),
                channel_id: channel_id.to_owned(),
                sender_discord_id: sender.to_owned(),
                sender_osl_user_id: sender.to_owned(),
                plaintext: plaintext.to_owned(),
                decrypted_at: 1_900_525_100,
                burned: false,
                reply_parent_id: None,
                edit_revision: 0,
            })
            .map_err(|error| format!("OSL: seed excluded record: {error}"))?;
    }
    *state
        .message_store
        .lock()
        .expect("message store mutex poisoned") = Some(store);

    let result = cmd_osl_chat_burn_sender_message_records_choice(
        &state,
        open_chat_id.to_owned(),
        selected_scope,
    )?;
    println!(
        "TASK0525 chat_burn_result open_chat_id={} selected_scope={} action=cmd_osl_burn_sender_message_records_both_sides selected_message_ids={} requested_count={} local_removal_count={} remote_removal_count={} equal_removal_counts={}",
        result.open_chat_id,
        result.selected_scope,
        result.selected_message_ids.join(","),
        result.requested_count,
        result.local_removal_count,
        result.remote_removal_count,
        result.equal_removal_counts
    );
    let _ = std::fs::remove_dir_all(store_dir);
    Ok(())
}

fn unique_store_dir() -> std::path::PathBuf {
    unique_store_dir_with_label("0513-burn-choice")
}

fn unique_store_dir_with_label(label: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("osl-task-{label}-{}-{nanos}", std::process::id(),))
}
