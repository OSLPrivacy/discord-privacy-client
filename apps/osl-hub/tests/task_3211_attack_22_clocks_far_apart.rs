#![cfg(feature = "core")]

use message_lifecycle::AcceptedPart;
use osl_privacy_hub::message_expiry::{
    absolute_release, record_first_open_at_path, run_pass, ExpiryVerdict, LIFECYCLE_TICK_INTERVAL,
};
use store::{MessageStore, StoredMessage};
use tempfile::TempDir;
use uuid::Uuid;

const KEY: [u8; 32] = [0x32; 32];
const SENDER_SEND_AT: i64 = 1_800_000_000;
const RECEIVER_CLOCK_BEFORE_SEND: i64 = SENDER_SEND_AT - 172_800;
const RECEIVER_FIRST_OPEN_AT: i64 = RECEIVER_CLOCK_BEFORE_SEND + 60;
const AFTER_DEADLINE: i64 = SENDER_SEND_AT + 3_600;
const SCOPE_KEY: &str = "dm:task3211";

struct ConfigDirGuard;

impl Drop for ConfigDirGuard {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(None);
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
    }
}

fn use_receiver_account_dir(dir: &std::path::Path) -> ConfigDirGuard {
    ipc::main_password::set_file_storage_key(None);
    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(Some(dir.to_path_buf()));
    ipc::main_password::set_file_storage_key(Some(KEY));
    ConfigDirGuard
}

struct ReceiverMachine {
    root: std::path::PathBuf,
    config_dir: std::path::PathBuf,
    local_data_dir: std::path::PathBuf,
    ledger_path: std::path::PathBuf,
    message_id: String,
}

#[test]
fn task_3211_attack_22_clocks_far_apart() {
    let root = TempDir::new().expect("temp machine root");
    let sender_root = root.path().join("sender-machine");
    let receiver_root = root.path().join("receiver-machine");
    let receiver = ReceiverMachine {
        root: receiver_root.clone(),
        config_dir: receiver_root.join("config"),
        local_data_dir: receiver_root.join("local-data"),
        ledger_path: receiver_root.join("config").join("message_open_clock.json"),
        message_id: format!("task-3211-{}", Uuid::new_v4()),
    };
    std::fs::create_dir_all(&sender_root).expect("create sender root");
    std::fs::create_dir_all(&receiver.config_dir).expect("create receiver config");
    std::fs::create_dir_all(&receiver.local_data_dir).expect("create receiver local data");

    let far_apart_seconds = SENDER_SEND_AT - RECEIVER_CLOCK_BEFORE_SEND;
    assert_eq!(far_apart_seconds, 172_800);
    assert!(RECEIVER_CLOCK_BEFORE_SEND < SENDER_SEND_AT);

    let marked_text = format!("TASK3211-CLOCK-SKEW-TIMED-MESSAGE-{}", Uuid::new_v4());
    let store = MessageStore::open(&receiver.config_dir.join("store"), &KEY)
        .expect("open receiver message store");
    send_timed_message_to_receiver(&receiver, &store, &marked_text);

    let before_deadline_count =
        authenticated_read_count(&receiver, &store, &marked_text, RECEIVER_FIRST_OPEN_AT);
    let effective_deadline = match osl_privacy_hub::message_expiry::verdict_at_path(
        &receiver.ledger_path,
        &KEY,
        SCOPE_KEY,
        &receiver.message_id,
        RECEIVER_FIRST_OPEN_AT,
    ) {
        ExpiryVerdict::Readable {
            effective_expires_at,
        } => effective_expires_at,
        verdict => panic!("receiver message should be readable after first open: {verdict:?}"),
    };
    assert_eq!(effective_deadline, AFTER_DEADLINE);
    assert_eq!(
        before_deadline_count, 1,
        "TASK3211_BEFORE_DEADLINE_READ_COUNT should be 1 before the deadline, got {before_deadline_count}"
    );

    let offline_after_deadline_count =
        authenticated_read_count(&receiver, &store, &marked_text, AFTER_DEADLINE);
    assert_eq!(
        offline_after_deadline_count, 0,
        "TASK3211_OFFLINE_AFTER_DEADLINE_READ_COUNT should be 0 after the deadline, got {offline_after_deadline_count}"
    );
    assert_eq!(before_deadline_count, 1);

    let offline_after_deadline_count =
        authenticated_read_count(&receiver, &store, &marked_text, AFTER_DEADLINE);
    assert_eq!(offline_after_deadline_count, 0);

    let _account_dir = use_receiver_account_dir(&receiver.config_dir);
    let offline_pass = run_pass(
        &receiver.local_data_dir,
        Some(&store),
        AFTER_DEADLINE + LIFECYCLE_TICK_INTERVAL.as_secs() as i64,
    );
    assert!(offline_pass.ran);
    assert!(!offline_pass.degraded);
    assert_eq!(
        offline_pass.expired_messages, 1,
        "TASK3211_OFFLINE_PASS_EXPIRED_MESSAGES should be 1 after the offline expiry pass, got {}",
        offline_pass.expired_messages
    );
    assert_eq!(
        offline_pass.shredded_cache_rows, 1,
        "TASK3211_OFFLINE_PASS_SHREDDED_CACHE_ROWS should be 1 after the offline expiry pass, got {}",
        offline_pass.shredded_cache_rows
    );
    assert_eq!(offline_pass.expired_messages, 1);
    assert_eq!(offline_pass.shredded_cache_rows, 1);

    let reconnected_after_deadline_count = authenticated_read_count(
        &receiver,
        &store,
        &marked_text,
        AFTER_DEADLINE + LIFECYCLE_TICK_INTERVAL.as_secs() as i64,
    );
    let reconnected_pass = run_pass(
        &receiver.local_data_dir,
        Some(&store),
        AFTER_DEADLINE + (LIFECYCLE_TICK_INTERVAL.as_secs() as i64 * 2),
    );
    let receiver_text_hits = plaintext_occurrences_under(&receiver.root, marked_text.as_bytes());

    assert_eq!(
        reconnected_after_deadline_count, 0,
        "TASK3211_RECONNECTED_AFTER_DEADLINE_READ_COUNT should be 0 after reconnect, got {reconnected_after_deadline_count}"
    );
    assert!(reconnected_pass.ran);
    assert!(!reconnected_pass.degraded);
    assert_eq!(
        reconnected_pass.expired_messages, 0,
        "TASK3211_RECONNECTED_PASS_EXPIRED_MESSAGES should be 0 after the offline pass already expired it, got {}",
        reconnected_pass.expired_messages
    );
    assert_eq!(
        reconnected_pass.shredded_cache_rows, 0,
        "TASK3211_RECONNECTED_PASS_SHREDDED_CACHE_ROWS should be 0 after the offline pass already shredded it, got {}",
        reconnected_pass.shredded_cache_rows
    );
    assert_eq!(
        receiver_text_hits, 0,
        "TASK3211_RECEIVER_TEXT_HITS_AFTER_RECONNECT should be 0 for the marked plaintext after reconnect, got {receiver_text_hits}"
    );
    assert_eq!(reconnected_after_deadline_count, 0);
    assert!(reconnected_pass.ran);
    assert!(!reconnected_pass.degraded);
    assert_eq!(reconnected_pass.expired_messages, 0);
    assert_eq!(reconnected_pass.shredded_cache_rows, 0);
    assert_eq!(receiver_text_hits, 0);
    assert!(
        store
            .get(&receiver.message_id)
            .expect("read receiver store after expiry")
            .is_none(),
        "expired receiver cache row must not be readable"
    );

    println!("TASK3211_MARKED_TEXT={marked_text}");
    println!("TASK3211_SENDER_SEND_AT={SENDER_SEND_AT}");
    println!("TASK3211_RECEIVER_CLOCK_BEFORE_SEND={RECEIVER_CLOCK_BEFORE_SEND}");
    println!("TASK3211_CLOCK_DISTANCE_SECONDS={far_apart_seconds}");
    println!("TASK3211_RECEIVER_FIRST_OPEN_AT={RECEIVER_FIRST_OPEN_AT}");
    println!("TASK3211_EFFECTIVE_DEADLINE={effective_deadline}");
    println!("TASK3211_BEFORE_DEADLINE_READ_COUNT={before_deadline_count}");
    println!("TASK3211_OFFLINE_AFTER_DEADLINE_READ_COUNT={offline_after_deadline_count}");
    println!(
        "TASK3211_OFFLINE_PASS_EXPIRED_MESSAGES={}",
        offline_pass.expired_messages
    );
    println!(
        "TASK3211_OFFLINE_PASS_SHREDDED_CACHE_ROWS={}",
        offline_pass.shredded_cache_rows
    );
    println!("TASK3211_RECONNECTED_AFTER_DEADLINE_READ_COUNT={reconnected_after_deadline_count}");
    println!(
        "TASK3211_RECONNECTED_PASS_EXPIRED_MESSAGES={}",
        reconnected_pass.expired_messages
    );
    println!(
        "TASK3211_RECONNECTED_PASS_SHREDDED_CACHE_ROWS={}",
        reconnected_pass.shredded_cache_rows
    );
    println!("TASK3211_RECEIVER_TEXT_HITS_AFTER_RECONNECT={receiver_text_hits}");
}

fn send_timed_message_to_receiver(
    receiver: &ReceiverMachine,
    store: &MessageStore,
    marked_text: &str,
) {
    store
        .put(&StoredMessage {
            discord_message_id: receiver.message_id.clone(),
            channel_id: "task3211-channel".to_owned(),
            sender_discord_id: "task3211-sender".to_owned(),
            sender_osl_user_id: "task3211-osl-sender".to_owned(),
            plaintext: marked_text.to_owned(),
            decrypted_at: RECEIVER_CLOCK_BEFORE_SEND,
            burned: false,
        })
        .expect("cache receiver message");

    osl_privacy_hub::message_expiry::note_delivered_at_path(
        &receiver.ledger_path,
        &KEY,
        SCOPE_KEY,
        &receiver.message_id,
        absolute_release(SENDER_SEND_AT, ipc::cipher_store_client::TTL_1H)
            .expect("one-hour absolute timed release"),
        vec![AcceptedPart {
            index: 0,
            sealed_bytes: 512,
            digest: [0x21; 32],
        }],
        [0x11; 32],
        Some(receiver.message_id.clone()),
        RECEIVER_CLOCK_BEFORE_SEND,
    )
    .expect("record receiver delivery");
}

fn authenticated_read_count(
    receiver: &ReceiverMachine,
    store: &MessageStore,
    marked_text: &str,
    now: i64,
) -> usize {
    if !record_first_open_at_path(
        &receiver.ledger_path,
        &KEY,
        SCOPE_KEY,
        &receiver.message_id,
        [0x22; 32],
        now,
    )
    .is_readable()
    {
        return 0;
    }
    store
        .get(&receiver.message_id)
        .expect("read receiver message")
        .filter(|message| message.plaintext == marked_text)
        .map(|_| 1)
        .unwrap_or(0)
}

fn plaintext_occurrences_under(root: &std::path::Path, needle: &[u8]) -> usize {
    if needle.is_empty() || !root.exists() {
        return 0;
    }
    let mut total = 0usize;
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let metadata = std::fs::symlink_metadata(&path).expect("stat receiver path");
        if metadata.is_dir() {
            for entry in std::fs::read_dir(&path).expect("read receiver dir") {
                stack.push(entry.expect("receiver dir entry").path());
            }
            continue;
        }
        if !metadata.is_file() {
            continue;
        }
        let bytes = std::fs::read(&path).expect("read receiver file");
        total += bytes
            .windows(needle.len())
            .filter(|window| *window == needle)
            .count();
    }
    total
}
