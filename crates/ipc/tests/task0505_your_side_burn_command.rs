use ipc::commands::{cmd_osl_burn_your_side_selected_records, cmd_osl_load_channel_history};
use ipc::state::AppState;
use store::{MessageStore, StoredMessage};
use tempfile::TempDir;

const SECRET: &[u8; 32] = &[0x55; 32];
const CHANNEL: &str = "task0505-channel";
const SENDER: &str = "sender-0505";
const RECIPIENT: &str = "recipient-0505";

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
fn task0505_your_side_choice_passes_only_selected_sender_records_to_local_burn() {
    let tmp = TempDir::new().unwrap();
    let state = state_with_store(tmp.path());
    let selected_sender_ids = vec![
        "task0505-sender-1".to_string(),
        "task0505-sender-2".to_string(),
    ];
    let selected_recipient_ids = vec![
        "task0505-recipient-1".to_string(),
        "task0505-recipient-2".to_string(),
    ];

    {
        let guard = state.message_store.lock().unwrap();
        let store = guard.as_ref().expect("message store installed");
        for (idx, id) in selected_sender_ids.iter().enumerate() {
            store
                .put(&sample(
                    id,
                    SENDER,
                    &format!("TASK0505 selected sender record {}", idx + 1),
                    1_800_001_000 + idx as i64,
                ))
                .unwrap();
        }
        for (idx, id) in selected_recipient_ids.iter().enumerate() {
            store
                .put(&sample(
                    id,
                    RECIPIENT,
                    &format!("TASK0505 selected recipient record {}", idx + 1),
                    1_800_001_100 + idx as i64,
                ))
                .unwrap();
        }
    }

    let result = cmd_osl_burn_your_side_selected_records(
        &state,
        "your-side".to_string(),
        selected_sender_ids.clone(),
        selected_recipient_ids.clone(),
    )
    .unwrap();

    assert_eq!(
        result.local_burn_command,
        "osl_remove_sender_message_records"
    );
    assert_eq!(result.passed_sender_record_ids, selected_sender_ids);
    assert_eq!(result.passed_recipient_record_ids, Vec::<String>::new());
    assert_eq!(result.requested_count, 2);
    assert_eq!(result.removed_count, 2);
    assert_eq!(result.remaining_local_count, 0);

    let after = cmd_osl_load_channel_history(&state, CHANNEL.to_string(), Some(10)).unwrap();
    let selected_sender_remaining = after
        .iter()
        .filter(|row| {
            result
                .passed_sender_record_ids
                .iter()
                .any(|id| id == &row.discord_message_id)
        })
        .count();
    let selected_recipient_remaining = after
        .iter()
        .filter(|row| {
            selected_recipient_ids
                .iter()
                .any(|id| id == &row.discord_message_id)
        })
        .count();
    assert_eq!(selected_sender_remaining, 0);
    assert_eq!(selected_recipient_remaining, 2);

    println!(
        "TASK0505 choice={} local_burn_command={} passed_sender_record_ids={:?} passed_recipient_record_ids={:?} requested_count={} removed_count={} remaining_local_count={} selected_sender_remaining={} selected_recipient_remaining={}",
        result.choice,
        result.local_burn_command,
        result.passed_sender_record_ids,
        result.passed_recipient_record_ids,
        result.requested_count,
        result.removed_count,
        result.remaining_local_count,
        selected_sender_remaining,
        selected_recipient_remaining
    );
}
