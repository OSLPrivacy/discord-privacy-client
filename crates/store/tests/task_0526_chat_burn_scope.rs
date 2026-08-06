use std::path::Path;

use store::{MessageStore, StoredMessage};
use tempfile::TempDir;

const SECRET: &[u8; 32] = &[52u8; 32];
const SENDER: &str = "sender-0526";
const OTHER_SENDER: &str = "other-sender-0526";
const DIRECT_CHAT: &str = "direct-chat-0526";
const OPEN_GROUP: &str = "open-group-0526";
const CHANNEL: &str = "channel-0526";

fn open_store(dir: &Path) -> MessageStore {
    MessageStore::open(dir, SECRET).expect("task 0526 store opens")
}

fn message(id: &str, channel_id: &str, sender: &str, at: i64) -> StoredMessage {
    StoredMessage {
        discord_message_id: id.to_string(),
        channel_id: channel_id.to_string(),
        sender_discord_id: sender.to_string(),
        sender_osl_user_id: format!("osl-{sender}"),
        plaintext: format!("task 0526 {channel_id} {sender} {id}"),
        decrypted_at: at,
        burned: false,
    }
}

fn put_sender_records(store: &MessageStore, channel_id: &str, prefix: &str) {
    for idx in 0..3 {
        store
            .put(&message(
                &format!("task-0526-{prefix}-sender-{idx}"),
                channel_id,
                SENDER,
                1_775_000_000 + idx,
            ))
            .expect("seed sender record");
    }
}

fn sender_record_count(store: &MessageStore, channel_id: &str, sender: &str) -> usize {
    store
        .list_by_channel(channel_id, 20)
        .expect("list seeded channel")
        .into_iter()
        .filter(|msg| msg.sender_discord_id == sender)
        .count()
}

#[test]
fn direct_group_channel_seed_then_direct_group_burn_changes_only_group_sender_count() {
    let tmp = TempDir::new().unwrap();
    let store = open_store(tmp.path());

    put_sender_records(&store, DIRECT_CHAT, "direct");
    put_sender_records(&store, OPEN_GROUP, "group");
    put_sender_records(&store, CHANNEL, "channel");
    store
        .put(&message(
            "task-0526-group-other-0",
            OPEN_GROUP,
            OTHER_SENDER,
            1_775_000_100,
        ))
        .expect("seed other sender record");

    let before_direct_sender = sender_record_count(&store, DIRECT_CHAT, SENDER);
    let before_group_sender = sender_record_count(&store, OPEN_GROUP, SENDER);
    let before_channel_sender = sender_record_count(&store, CHANNEL, SENDER);
    let before_group_other = sender_record_count(&store, OPEN_GROUP, OTHER_SENDER);
    println!(
        "TASK0526 BEFORE direct_sender={before_direct_sender} group_sender={before_group_sender} channel_sender={before_channel_sender} group_other={before_group_other}"
    );

    let touched = store
        .wipe_wrapped_keys_in_scope("gc", OPEN_GROUP, Some(SENDER))
        .expect("burn open group sender records directly");
    println!("TASK0526 BURN touched_group_sender_records={touched}");

    let after_direct_sender = sender_record_count(&store, DIRECT_CHAT, SENDER);
    let after_group_sender = sender_record_count(&store, OPEN_GROUP, SENDER);
    let after_channel_sender = sender_record_count(&store, CHANNEL, SENDER);
    let after_group_other = sender_record_count(&store, OPEN_GROUP, OTHER_SENDER);
    println!(
        "TASK0526 AFTER direct_sender={after_direct_sender} group_sender={after_group_sender} channel_sender={after_channel_sender} group_other={after_group_other}"
    );
    println!(
        "TASK0526 FINISH_LINE group_sender_record_count={before_group_sender}->{after_group_sender}"
    );

    assert_eq!(before_direct_sender, 3);
    assert_eq!(before_group_sender, 3);
    assert_eq!(before_channel_sender, 3);
    assert_eq!(before_group_other, 1);
    assert_eq!(touched, 3);
    assert_eq!(after_direct_sender, 3);
    assert_eq!(after_group_sender, 0);
    assert_eq!(after_channel_sender, 3);
    assert_eq!(after_group_other, 1);
}
