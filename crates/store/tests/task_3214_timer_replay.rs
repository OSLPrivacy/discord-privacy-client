use store::{
    MessageRowDeleteOutcome, MessageStore, MessageTimerChangeOutcome, StoredMessage,
    MESSAGE_ROW_ACTION_OUT_OF_DATE_REASON,
};
use tempfile::TempDir;

const SECRET: &[u8; 32] = b"task-3214-store-secret-32-bytes!";
const ROW_NAME: &str = "timer-row-3214";
const MESSAGE_A: &str = "task-3214-a";
const MESSAGE_B: &str = "task-3214-b";
const TEXT_A: &str = "TIMER-3214-A";
const TEXT_B: &str = "TIMER-3214-B";

#[test]
fn task_3214_old_timer_and_delete_requests_are_out_of_date_after_same_row_name_reuse() {
    let tmp = TempDir::new().unwrap();
    let store = MessageStore::open(tmp.path(), SECRET).unwrap();

    put_message(&store, MESSAGE_A, TEXT_A, 1);
    store.record_message_timer_minutes(MESSAGE_A, 5).unwrap();

    let saved_old_timer_change = store
        .prepare_message_row_action_request(ROW_NAME, MESSAGE_A)
        .unwrap();
    let saved_old_delete = saved_old_timer_change.clone();
    let current_first = store
        .prepare_message_row_action_request(ROW_NAME, MESSAGE_A)
        .unwrap();
    let first = store
        .change_message_timer_minutes(&current_first, 5, 6)
        .unwrap();
    let (first_before, first_after) = applied_minutes(first);
    assert_eq!((first_before, first_after), (5, 6));

    put_message(&store, MESSAGE_B, TEXT_B, 2);
    store.record_message_timer_minutes(MESSAGE_B, 5).unwrap();
    let same_named_newer = store
        .prepare_message_row_action_request(ROW_NAME, MESSAGE_B)
        .unwrap();
    assert_eq!(same_named_newer.row_name, saved_old_timer_change.row_name);

    let old_timer = store
        .change_message_timer_minutes(&saved_old_timer_change, 5, 7)
        .unwrap();
    let old_delete = store
        .delete_message_by_row_action_request(&saved_old_delete)
        .unwrap();
    let old_timer_reason = refusal_reason(old_timer);
    let old_delete_reason = delete_refusal_reason(old_delete);
    assert_eq!(old_timer_reason, MESSAGE_ROW_ACTION_OUT_OF_DATE_REASON);
    assert_eq!(old_delete_reason, MESSAGE_ROW_ACTION_OUT_OF_DATE_REASON);

    let b_count = exact_text_count(&store, MESSAGE_B, TEXT_B);
    let b_timer = store.message_timer_minutes(MESSAGE_B).unwrap().unwrap();
    assert_eq!(b_count, 1);
    assert_eq!(b_timer, 5);

    let fresh = store
        .prepare_message_row_action_request(ROW_NAME, MESSAGE_A)
        .unwrap();
    let final_change = store.change_message_timer_minutes(&fresh, 6, 7).unwrap();
    let (fresh_before, fresh_after) = applied_minutes(final_change);
    assert_eq!((fresh_before, fresh_after), (6, 7));

    println!(
        "TASK3214_SHARED_ROW_NAME_OLD={}",
        saved_old_timer_change.row_name
    );
    println!(
        "TASK3214_SHARED_ROW_NAME_NEWER={}",
        same_named_newer.row_name
    );
    println!("TASK3214_CURRENT_A_BEFORE_MINUTES={first_before}");
    println!("TASK3214_CURRENT_A_AFTER_MINUTES={first_after}");
    println!("TASK3214_OLD_TIMER_REFUSAL_REASON={old_timer_reason}");
    println!("TASK3214_OLD_DELETE_REFUSAL_REASON={old_delete_reason}");
    println!("TASK3214_B_COUNT={b_count}");
    println!("TASK3214_B_TIMER_MINUTES={b_timer}");
    println!("TASK3214_FRESH_A_BEFORE_MINUTES={fresh_before}");
    println!("TASK3214_FRESH_A_AFTER_MINUTES={fresh_after}");
}

fn put_message(store: &MessageStore, id: &str, plaintext: &str, decrypted_at: i64) {
    store
        .put(&StoredMessage {
            discord_message_id: id.to_string(),
            channel_id: "task-3214-channel".to_string(),
            sender_discord_id: "task-3214-sender".to_string(),
            sender_osl_user_id: "task-3214-osl-sender".to_string(),
            plaintext: plaintext.to_string(),
            decrypted_at,
            reply_parent_id: None,
            edit_revision: 1,
            burned: false,
        })
        .unwrap();
}

fn applied_minutes(outcome: MessageTimerChangeOutcome) -> (u32, u32) {
    match outcome {
        MessageTimerChangeOutcome::Applied(applied) => {
            (applied.before_minutes, applied.after_minutes)
        }
        MessageTimerChangeOutcome::Refused { reason } => {
            panic!("timer change refused unexpectedly: {reason}")
        }
    }
}

fn refusal_reason(outcome: MessageTimerChangeOutcome) -> String {
    match outcome {
        MessageTimerChangeOutcome::Applied(applied) => panic!(
            "stale timer change applied unexpectedly: {} -> {}",
            applied.before_minutes, applied.after_minutes
        ),
        MessageTimerChangeOutcome::Refused { reason } => reason,
    }
}

fn delete_refusal_reason(outcome: MessageRowDeleteOutcome) -> String {
    match outcome {
        MessageRowDeleteOutcome::Deleted => panic!("stale delete applied unexpectedly"),
        MessageRowDeleteOutcome::Refused { reason } => reason,
    }
}

fn exact_text_count(store: &MessageStore, id: &str, text: &str) -> usize {
    store
        .get(id)
        .unwrap()
        .filter(|message| message.plaintext == text)
        .map(|_| 1)
        .unwrap_or(0)
}
