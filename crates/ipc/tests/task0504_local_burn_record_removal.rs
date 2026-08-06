use ipc::commands::{cmd_osl_load_channel_history, cmd_osl_remove_sender_message_records};
use ipc::state::AppState;
use store::{MessageStore, StoredMessage};
use tempfile::TempDir;

const SECRET: &[u8; 32] = &[0x50; 32];
const CHANNEL: &str = "task0504-channel";
const SENDER: &str = "sender-0504";

fn sample(id: &str, sender: &str, body: &str, at: i64) -> StoredMessage {
    StoredMessage {
        discord_message_id: id.to_string(),
        channel_id: CHANNEL.to_string(),
        sender_discord_id: sender.to_string(),
        sender_osl_user_id: sender.to_string(),
        plaintext: body.to_string(),
        decrypted_at: at,
        reply_parent_id: None,
        edit_revision: 1,
        burned: false,
    }
}

fn state_with_store(dir: &std::path::Path) -> AppState {
    let state = AppState::new();
    let store = MessageStore::open(dir, SECRET).expect("open message store");
    *state.message_store.lock().unwrap() = Some(store);
    state
}

#[test]
fn task0504_command_removes_three_chosen_sender_records_from_local_copy() {
    let tmp = TempDir::new().unwrap();
    let state = state_with_store(tmp.path());
    let chosen = vec![
        "task0504-sender-1".to_string(),
        "task0504-sender-2".to_string(),
        "task0504-sender-3".to_string(),
    ];

    {
        let guard = state.message_store.lock().unwrap();
        let store = guard.as_ref().expect("message store installed");
        for (idx, id) in chosen.iter().enumerate() {
            store
                .put(&sample(
                    id,
                    SENDER,
                    &format!("TASK0504 chosen sender record {}", idx + 1),
                    1_800_000_000 + idx as i64,
                ))
                .unwrap();
        }
        store
            .put(&sample(
                "task0504-survivor",
                "sender-0504-survivor",
                "TASK0504 survivor record",
                1_800_000_100,
            ))
            .unwrap();
    }

    let before = cmd_osl_load_channel_history(&state, CHANNEL.to_string(), Some(10)).unwrap();
    let chosen_before = before
        .iter()
        .filter(|row| chosen.iter().any(|id| id == &row.discord_message_id))
        .count();
    assert_eq!(
        chosen_before, 3,
        "positive path: the three chosen rows exist"
    );

    let result = cmd_osl_remove_sender_message_records(&state, chosen.clone()).unwrap();
    assert_eq!(result.requested_count, 3);
    assert_eq!(result.removed_count, 3);
    assert_eq!(result.remaining_local_count, 0);

    let after = cmd_osl_load_channel_history(&state, CHANNEL.to_string(), Some(10)).unwrap();
    let chosen_after = after
        .iter()
        .filter(|row| chosen.iter().any(|id| id == &row.discord_message_id))
        .count();
    let survivor_count = after
        .iter()
        .filter(|row| row.discord_message_id == "task0504-survivor")
        .count();
    assert_eq!(chosen_after, 0);
    assert_eq!(survivor_count, 1, "removal must not delete unchosen rows");

    println!(
        "TASK0504 command=osl_remove_sender_message_records chosen_records={} removed_count={} remaining_local_count={} survivor_count={}",
        result.requested_count, result.removed_count, result.remaining_local_count, survivor_count
    );
}
