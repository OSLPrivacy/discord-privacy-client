use ipc::commands::{cmd_osl_load_channel_history, cmd_osl_persist_inbound};
use ipc::state::AppState;
use store::MessageStore;
use tempfile::TempDir;

const SECRET: &[u8; 32] = &[0x36; 32];
const CONVERSATION_ID: &str = "osl-chat:task-3670-maple-conversation";
const SENDER_OSL_USER_ID: &str = "task-3670-maple-peer";
const MARK_PREFIX: &str = "MAPLE-3670";
const MESSAGE_COUNT: usize = 10_000;

fn fresh_state(dir: &std::path::Path) -> AppState {
    let state = AppState::new();
    let store = MessageStore::open(dir, SECRET).expect("open fresh OSL Chats store");
    *state.message_store.lock().unwrap() = Some(store);
    state
}

fn marked_count(state: &AppState) -> usize {
    cmd_osl_load_channel_history(
        state,
        CONVERSATION_ID.to_owned(),
        Some(MESSAGE_COUNT as u32),
    )
    .expect("load OSL Chat history")
    .into_iter()
    .filter(|message| message.plaintext.starts_with(MARK_PREFIX))
    .count()
}

fn mark_number(plaintext: &str) -> &str {
    plaintext
        .strip_prefix(MARK_PREFIX)
        .and_then(|rest| rest.strip_prefix(' '))
        .expect("message has MAPLE-3670 marker")
}

#[test]
fn task_3670_creates_ten_thousand_ordered_marked_osl_chat_messages() {
    let tmp = TempDir::new().unwrap();
    let state = fresh_state(tmp.path());

    let before = marked_count(&state);
    println!("TASK3670 before.prefix={MARK_PREFIX} count={before}");
    assert_eq!(before, 0, "fresh store must start with no MAPLE-3670 rows");

    for number in 1..=MESSAGE_COUNT {
        cmd_osl_persist_inbound(
            &state,
            CONVERSATION_ID.to_owned(),
            format!("task-3670-maple-message-{number:05}"),
            SENDER_OSL_USER_ID.to_owned(),
            format!("{MARK_PREFIX} {number:05}"),
        )
        .expect("persist inbound OSL Chat message");
    }

    let mut after = cmd_osl_load_channel_history(
        &state,
        CONVERSATION_ID.to_owned(),
        Some(MESSAGE_COUNT as u32),
    )
    .expect("load populated OSL Chat history");
    let after_marked = after
        .iter()
        .filter(|message| message.plaintext.starts_with(MARK_PREFIX))
        .count();

    after.reverse();
    let first_number = mark_number(&after.first().expect("first MAPLE row").plaintext);
    let last_number = mark_number(&after.last().expect("last MAPLE row").plaintext);

    println!("TASK3670 conversation={CONVERSATION_ID}");
    println!("TASK3670 after.prefix={MARK_PREFIX} count={after_marked}");
    println!("TASK3670 first.number={first_number}");
    println!("TASK3670 last.number={last_number}");
    println!(
        "TASK3670 finish_line prefix={MARK_PREFIX} before={before} after={after_marked} first={first_number} last={last_number}"
    );

    assert_eq!(after_marked, MESSAGE_COUNT);
    assert!(after
        .iter()
        .all(|message| message.channel_id == CONVERSATION_ID));
    assert!(after
        .iter()
        .all(|message| message.plaintext.starts_with(MARK_PREFIX)));
    assert_eq!(first_number, "00001");
    assert_eq!(last_number, "10000");

    for (index, message) in after.iter().enumerate() {
        assert_eq!(
            message.plaintext,
            format!("{MARK_PREFIX} {:05}", index + 1),
            "MAPLE-3670 order broke at row {}",
            index + 1
        );
    }
}
