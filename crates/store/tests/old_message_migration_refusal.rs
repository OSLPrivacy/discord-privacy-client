use std::path::Path;

use rusqlite::params;
use store::{MessageStore, StoreError, StoredMessage};
use tempfile::TempDir;

const SECRET: &[u8; 32] = &[0x42; 32];
const TARGET: &str = "EMBER-0425";
const BURN_MARK: &str = "BURN-0425";
const OLD_SCHEMA_VERSION: u32 = 8;

fn message(id: &str, sender: &str, plaintext: &str, decrypted_at: i64) -> StoredMessage {
    StoredMessage {
        discord_message_id: id.to_string(),
        channel_id: TARGET.to_string(),
        sender_discord_id: sender.to_string(),
        sender_osl_user_id: format!("osl-{sender}"),
        plaintext: plaintext.to_string(),
        decrypted_at,
        burned: false,
    }
}

fn seed_store(dir: &Path) -> MessageStore {
    let store = MessageStore::open(dir, SECRET).expect("fresh store opens");
    store
        .put(&message(
            "EMBER-0425-left",
            "side-left",
            "EMBER-0425 payload from side-left",
            1,
        ))
        .expect("seed left side");
    store
        .put(&message(
            "EMBER-0425-right",
            "side-right",
            "EMBER-0425 payload from side-right",
            2,
        ))
        .expect("seed right side");
    store
}

fn payload_fingerprint(msg: &StoredMessage) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in msg.plaintext.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{}:{:016x}", msg.discord_message_id, hash)
}

fn readable_payload_fingerprints(store: &MessageStore) -> Vec<String> {
    let mut fingerprints = store
        .list_by_channel(TARGET, 10)
        .expect("list target channel")
        .iter()
        .map(payload_fingerprint)
        .collect::<Vec<_>>();
    fingerprints.sort();
    fingerprints
}

fn stored_count(store: &MessageStore) -> usize {
    store
        .list_by_channel(TARGET, 10)
        .expect("count readable rows")
        .len()
}

fn set_schema_version(dir: &Path, version: u32) {
    let conn = rusqlite::Connection::open(dir.join("messages.sqlite")).expect("open sqlite");
    conn.execute(
        "UPDATE _meta SET value = ?1 WHERE key = 'schema_version'",
        params![version.to_le_bytes().to_vec()],
    )
    .expect("change only schema format version");
}

#[test]
fn old_format_message_store_refuses_remote_burn_without_mutation() {
    let good_dir = TempDir::new().expect("good tempdir");
    let old_dir = TempDir::new().expect("old tempdir");
    let good = seed_store(good_dir.path());
    let old = seed_store(old_dir.path());

    let good_before = readable_payload_fingerprints(&good);
    let old_before = readable_payload_fingerprints(&old);
    let good_count_before = stored_count(&good);
    let old_count_before = stored_count(&old);
    println!("0425 good before fingerprints: {good_before:?}");
    println!("0425 old before fingerprints: {old_before:?}");
    println!("0425 good before count: {good_count_before}");
    println!("0425 old before count: {old_count_before}");
    assert_eq!(good_before.len(), 2);
    assert_eq!(old_before.len(), 2);
    assert_eq!(good_count_before, 2);
    assert_eq!(old_count_before, 2);

    let good_burned = good
        .wipe_wrapped_keys_in_scope("remote", TARGET, None)
        .expect("current-format remote burn succeeds");
    let good_count_after = stored_count(&good);
    println!("0425 good burn target: {TARGET}");
    println!("0425 good burn mark: {BURN_MARK}");
    println!("0425 good burn rows: {good_burned}");
    println!("0425 good after count: {good_count_after}");
    assert_eq!(good_burned, 2);
    assert_eq!(good_count_after, 0);

    set_schema_version(old_dir.path(), OLD_SCHEMA_VERSION);
    let old_error = old
        .wipe_wrapped_keys_in_scope("remote", TARGET, None)
        .expect_err("old-format remote burn must be refused");
    let old_refusal = match old_error {
        StoreError::Schema(message) => message,
        other => panic!("wrong old-format refusal: {other}"),
    };
    let old_after = readable_payload_fingerprints(&old);
    let old_count_after = stored_count(&old);
    let good_count_final = stored_count(&good);
    println!("0425 old burn target: {TARGET}");
    println!("0425 old refusal: {old_refusal}");
    println!("0425 old after fingerprints: {old_after:?}");
    println!("0425 old after count: {old_count_after}");
    println!("0425 good final count: {good_count_final}");
    println!("0425 final burn mark: {BURN_MARK}");

    assert_eq!(old_refusal, "old message cannot remote burn");
    assert_eq!(old_count_after, 2);
    assert_eq!(old_after, old_before);
    assert_eq!(good_count_final, 0);
    assert_eq!(BURN_MARK, "BURN-0425");
}
