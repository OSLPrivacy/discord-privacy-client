#![cfg(feature = "core")]

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use message_lifecycle::AcceptedPart;
use osl_privacy_hub::message_expiry::{
    note_delivered_at_path, record_first_open, relative_release, run_pass,
};
use store::{MessageStore, StoredMessage};

const KEY: [u8; 32] = [0x43; 32];
const STORE_SECRET: [u8; 32] = [0x71; 32];
const SCOPE: &str = "dm:task1343";
const CHANNEL: &str = "task1343-channel";
const SENDER: &str = "task1343-sender";
const SENDER_OSL: &str = "task1343-osl-user";
const SENT_AT: i64 = 1_000_000;
const OPENED_AT: i64 = SENT_AT + 11;

struct IsolatedAccount {
    root: PathBuf,
}

impl IsolatedAccount {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "osl-task-1343-timer-offline-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&root).expect("create isolated account root");
        keystore::set_base_dir_override(Some(root.clone()));
        keystore::set_active_account_dir(Some(root.clone()));
        ipc::main_password::set_file_storage_key(Some(KEY));
        Self { root }
    }

    fn ledger_path(&self) -> PathBuf {
        self.root.join("message_open_clock.json")
    }

    fn store_path(&self) -> PathBuf {
        self.root.join("history")
    }
}

impl Drop for IsolatedAccount {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(None);
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn visible_messages(store: &MessageStore) -> Vec<StoredMessage> {
    store
        .list_by_channel(CHANNEL, 20)
        .expect("read visible channel messages")
}

fn exact_count(store: &MessageStore, mark: &str) -> usize {
    visible_messages(store)
        .into_iter()
        .filter(|message| message.plaintext == mark)
        .count()
}

#[test]
fn task_1343_timer_survives_one_side_offline() {
    let account = IsolatedAccount::new();
    let message_id = format!(
        "task1343-{}",
        account
            .root
            .file_name()
            .expect("root name")
            .to_string_lossy()
            .chars()
            .filter(|character| character.is_ascii_digit())
            .collect::<String>()
    );
    let mark = format!("TASK-1343-MARK-{message_id}");
    let store_dir = account.store_path();
    let copy_a = MessageStore::open(&store_dir, &STORE_SECRET).expect("open first store copy");
    let copy_b = MessageStore::open(&store_dir, &STORE_SECRET).expect("open second store copy");

    let stored = StoredMessage {
        discord_message_id: message_id.clone(),
        channel_id: CHANNEL.to_owned(),
        sender_discord_id: SENDER.to_owned(),
        sender_osl_user_id: SENDER_OSL.to_owned(),
        plaintext: mark.clone(),
        decrypted_at: SENT_AT,
        burned: false,
    };
    copy_a
        .put(&stored)
        .expect("put marked message through first copy");
    copy_b
        .put(&stored)
        .expect("put marked message through second copy");
    note_delivered_at_path(
        &account.ledger_path(),
        &KEY,
        SCOPE,
        &message_id,
        relative_release(SENT_AT, ipc::cipher_store_client::TTL_1H).expect("one-hour open clock"),
        vec![AcceptedPart {
            index: 0,
            sealed_bytes: 512,
            digest: [0x59; 32],
        }],
        [0x68; 32],
        Some(message_id.clone()),
        SENT_AT,
    )
    .expect("record timed message");

    assert!(record_first_open(SCOPE, &message_id, [0x77; 32], OPENED_AT).is_readable());

    let before_a = exact_count(&copy_a, &mark);
    let before_b = exact_count(&copy_b, &mark);
    println!("task_1343 mark={mark}");
    println!("task_1343 before copy_a_count={before_a} copy_b_count={before_b}");
    assert_eq!(
        before_a, 1,
        "first copy must show exactly one marked message"
    );
    assert_eq!(
        before_b, 1,
        "second copy must show exactly one marked message"
    );

    drop(copy_b);

    let expired_at = OPENED_AT + i64::from(ipc::cipher_store_client::TTL_1H);
    let pass = run_pass(&account.root, Some(&copy_a), expired_at);
    println!(
        "task_1343 expire_once ran={} expired_messages={} shredded_cache_rows={}",
        pass.ran, pass.expired_messages, pass.shredded_cache_rows
    );
    assert!(pass.ran, "unlocked expiry pass must run");
    assert_eq!(pass.expired_messages, 1, "one timed message expires");
    assert_eq!(
        pass.shredded_cache_rows, 1,
        "the marked local row is shredded"
    );

    let reopened_b =
        MessageStore::open(&store_dir, &STORE_SECRET).expect("reopen second store copy");
    let after_a_messages = visible_messages(&copy_a);
    let after_b_messages = visible_messages(&reopened_b);
    let after_a = after_a_messages
        .iter()
        .filter(|message| message.plaintext == mark)
        .count();
    let after_b = after_b_messages
        .iter()
        .filter(|message| message.plaintext == mark)
        .count();
    println!("task_1343 after copy_a_count={after_a} reopened_copy_b_count={after_b}");
    println!(
        "task_1343 absent_after_reopen copy_a={} reopened_copy_b={}",
        !after_a_messages
            .iter()
            .any(|message| message.plaintext == mark),
        !after_b_messages
            .iter()
            .any(|message| message.plaintext == mark)
    );
    assert_eq!(
        after_a, 0,
        "first copy must not show the marked message after expiry"
    );
    assert_eq!(
        after_b, 0,
        "reopened second copy must not show the marked message after expiry"
    );
    assert!(!after_a_messages
        .iter()
        .any(|message| message.plaintext == mark));
    assert!(!after_b_messages
        .iter()
        .any(|message| message.plaintext == mark));
}
