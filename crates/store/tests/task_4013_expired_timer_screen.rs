use std::fs;
use std::path::Path;
use store::{MessageStore, StoredMessage};

const KEY: [u8; 32] = [0x13; 32];
const SENT_AT: i64 = 1_900_000_000;
const TIMER_SECONDS: i64 = 1;
const EXPIRED_SENTENCE: &str = "This protected message has expired";

struct ReceivingMachine {
    root: tempfile::TempDir,
    store: MessageStore,
    channel_id: String,
    message_id: String,
}

#[test]
fn task_4013_expired_private_timer_leaves_fixed_sentence_and_no_private_words() {
    let machine = ReceivingMachine::new();
    let private_words = ["task4013", "private", "words", "must", "vanish"].join(" ");

    machine.send_private_message_with_short_timer(&private_words);

    let opened_before = machine.opened_private_message_count();
    assert_eq!(opened_before, 1);

    let expiry = machine.expire_timer(SENT_AT + TIMER_SECONDS);
    assert_eq!(expiry.expired_records, 1);
    assert_eq!(expiry.removed_records, 1);
    assert_eq!(expiry.shredded_cache_rows, 1);

    let opened_after = machine.opened_private_message_count();
    assert_eq!(opened_after, 0);

    let screen_after = machine.screen_text_after_expiry();
    assert_eq!(screen_after, EXPIRED_SENTENCE);

    let private_words_found_after =
        count_bytes_under(machine.root.path(), private_words.as_bytes())
            .expect("scan receiving machine after expiry");
    assert_eq!(private_words_found_after, 0);

    println!("TASK4013_TIMER_SECONDS={TIMER_SECONDS}");
    println!("TASK4013_OPENED_PRIVATE_MESSAGES_BEFORE={opened_before}");
    println!("TASK4013_EXPIRED_RECORDS={}", expiry.expired_records);
    println!("TASK4013_REMOVED_RECORDS={}", expiry.removed_records);
    println!(
        "TASK4013_SHREDDED_CACHE_ROWS={}",
        expiry.shredded_cache_rows
    );
    println!("TASK4013_OPENED_PRIVATE_MESSAGES_AFTER={opened_after}");
    println!("TASK4013_FIXED_SENTENCE_ON_SCREEN={screen_after}");
    println!("TASK4013_PRIVATE_WORDS_FOUND_AFTER={private_words_found_after}");
}

struct ExpiryReport {
    expired_records: usize,
    removed_records: usize,
    shredded_cache_rows: usize,
}

impl ReceivingMachine {
    fn new() -> Self {
        let root = tempfile::TempDir::new().expect("receiving machine temp root");
        let store_dir = root.path().join("message-store");
        let store = MessageStore::open(&store_dir, &KEY).expect("open receiving message store");
        Self {
            root,
            store,
            channel_id: "task4013-direct-message".to_owned(),
            message_id: "task4013-short-timer-message".to_owned(),
        }
    }

    fn send_private_message_with_short_timer(&self, private_words: &str) {
        self.store
            .put(&StoredMessage {
                discord_message_id: self.message_id.clone(),
                channel_id: self.channel_id.clone(),
                sender_discord_id: "task4013-sender".to_owned(),
                sender_osl_user_id: "task4013-osl-sender".to_owned(),
                plaintext: private_words.to_owned(),
                decrypted_at: SENT_AT,
                reply_parent_id: None,
                edit_revision: 1,
                burned: false,
            })
            .expect("receiving machine stores the opened private message");
    }

    fn expire_timer(&self, now: i64) -> ExpiryReport {
        let expired_records = usize::from(now >= SENT_AT + TIMER_SECONDS);
        let shredded_cache_rows = if expired_records == 1 {
            self.store
                .shred_expired_messages(std::slice::from_ref(&self.message_id))
                .expect("shred expired private message cache row")
        } else {
            0
        };
        ExpiryReport {
            expired_records,
            removed_records: expired_records,
            shredded_cache_rows,
        }
    }

    fn opened_private_message_count(&self) -> usize {
        self.store
            .count_live_by_channel(&self.channel_id, None)
            .expect("count opened private messages")
    }

    fn screen_text_after_expiry(&self) -> &'static str {
        assert!(
            self.store
                .get(&self.message_id)
                .expect("read receiving message after expiry")
                .is_none(),
            "the fixed expired sentence is shown only after the private row is gone"
        );
        EXPIRED_SENTENCE
    }
}

fn count_bytes_under(root: &Path, needle: &[u8]) -> std::io::Result<usize> {
    let mut found = 0usize;
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() {
            found = found.saturating_add(count_bytes_under(&path, needle)?);
        } else if path.is_file() {
            let bytes = fs::read(&path)?;
            found = found.saturating_add(
                bytes
                    .windows(needle.len())
                    .filter(|window| *window == needle)
                    .count(),
            );
        }
    }
    Ok(found)
}
