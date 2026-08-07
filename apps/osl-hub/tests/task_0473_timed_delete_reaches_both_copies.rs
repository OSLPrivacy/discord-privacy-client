#![cfg(feature = "core")]

use message_lifecycle::AcceptedPart;
use osl_privacy_hub::message_expiry::{
    absolute_release, note_delivered_at_path, prune_at_path, verdict_at_path, ExpiryVerdict,
};
use store::{MessageStore, StoredMessage};
use tempfile::TempDir;
use uuid::Uuid;

const KEY: [u8; 32] = [0x47; 32];
const SENT_AT: i64 = 1_800_000_000;
const EXPIRES_AT: i64 = SENT_AT + 3_600;

struct NamedCopy {
    name: &'static str,
    scope_key: String,
    message_id: String,
    ledger_path: std::path::PathBuf,
    store: MessageStore,
}

#[test]
fn task_0473_timed_delete_reaches_both_named_copies() {
    let root = TempDir::new().expect("temp two-copy root");
    let random_mark = Uuid::new_v4().to_string();
    let marked_text = format!("TASK0473-MARKED-TIMED-MESSAGE-{random_mark}");
    let message_id = format!("task-0473-{random_mark}");

    let copy_a = put_marked_timed_message(
        root.path(),
        "OSL Copy A",
        "copy:a",
        &message_id,
        &marked_text,
    );
    let copy_b = put_marked_timed_message(
        root.path(),
        "OSL Copy B",
        "copy:b",
        &message_id,
        &marked_text,
    );
    let copies = [copy_a, copy_b];

    let before_a = exact_text_count(&copies[0], &marked_text);
    let before_b = exact_text_count(&copies[1], &marked_text);
    let read_a = read_exact_text(&copies[0]);
    let read_b = read_exact_text(&copies[1]);

    assert!(
        read_a == marked_text,
        "copy A must read the exact marked text before expiry"
    );
    assert!(
        read_b == marked_text,
        "copy B must read the exact marked text before expiry"
    );
    assert_eq!(
        before_a, 1,
        "copy A must hold exactly one marked timed message before expiry"
    );
    assert_eq!(
        before_b, 1,
        "copy B must hold exactly one marked timed message before expiry"
    );

    println!("TASK0473_MARKED_TEXT={marked_text}");
    println!("TASK0473_BEFORE_COPY_A_EXACT_TEXT={read_a}");
    println!("TASK0473_BEFORE_COPY_B_EXACT_TEXT={read_b}");
    println!("TASK0473_BEFORE_COPY_A_COUNT={before_a}");
    println!("TASK0473_BEFORE_COPY_B_COUNT={before_b}");

    let expired = expire_once(&copies, EXPIRES_AT);

    let after_a = exact_text_count(&copies[0], &marked_text);
    let after_b = exact_text_count(&copies[1], &marked_text);
    let absent_a = copies[0]
        .store
        .get(&copies[0].message_id)
        .expect("read copy A after expiry")
        .is_none();
    let absent_b = copies[1]
        .store
        .get(&copies[1].message_id)
        .expect("read copy B after expiry")
        .is_none();

    assert_eq!(
        expired, 2,
        "one expiry pass must expire one row in each named copy"
    );
    assert_eq!(
        after_a, 0,
        "{} still holding marked message {marked_text} after expiry",
        copies[0].name
    );
    assert_eq!(
        after_b, 0,
        "{} still holding marked message {marked_text} after expiry",
        copies[1].name
    );
    assert!(
        absent_a,
        "{} still holding marked message {marked_text} after expiry",
        copies[0].name
    );
    assert!(
        absent_b,
        "{} still holding marked message {marked_text} after expiry",
        copies[1].name
    );

    println!("TASK0473_EXPIRY_PASS_COUNT=1");
    println!("TASK0473_EXPIRED_TOTAL={expired}");
    println!("TASK0473_AFTER_COPY_A_COUNT={after_a}");
    println!("TASK0473_AFTER_COPY_B_COUNT={after_b}");
    println!("TASK0473_AFTER_COPY_A_MARKED_MESSAGE_ABSENT={absent_a}");
    println!("TASK0473_AFTER_COPY_B_MARKED_MESSAGE_ABSENT={absent_b}");
}

fn put_marked_timed_message(
    root: &std::path::Path,
    name: &'static str,
    scope_key: &str,
    message_id: &str,
    marked_text: &str,
) -> NamedCopy {
    let copy_root = root.join(name.replace(' ', "-").to_ascii_lowercase());
    let store_root = copy_root.join("message-store");
    let ledger_root = copy_root.join("expiry-ledger");
    std::fs::create_dir_all(&ledger_root).expect("create copy ledger root");
    let ledger_path = ledger_root.join("message_open_clock.json");
    let store = MessageStore::open(&store_root, &KEY).expect("open copy store");

    store
        .put(&StoredMessage {
            discord_message_id: message_id.to_owned(),
            channel_id: format!("{scope_key}:channel"),
            sender_discord_id: "0473-sender".to_owned(),
            sender_osl_user_id: "0473-osl-sender".to_owned(),
            plaintext: marked_text.to_owned(),
            decrypted_at: SENT_AT,
            burned: false,
        })
        .expect("put marked timed message in copy store");

    note_delivered_at_path(
        &ledger_path,
        &KEY,
        scope_key,
        message_id,
        absolute_release(SENT_AT, 3_600).expect("one-hour absolute timed release"),
        vec![AcceptedPart {
            index: 0,
            sealed_bytes: 512,
            digest: [0x73; 32],
        }],
        [0x47; 32],
        Some(message_id.to_owned()),
        SENT_AT,
    )
    .expect("record message expiry ledger");

    assert_eq!(
        verdict_at_path(&ledger_path, &KEY, scope_key, message_id, SENT_AT + 1),
        ExpiryVerdict::Readable {
            effective_expires_at: EXPIRES_AT
        },
        "{name} timed message must be readable before expiry"
    );

    NamedCopy {
        name,
        scope_key: scope_key.to_owned(),
        message_id: message_id.to_owned(),
        ledger_path,
        store,
    }
}

fn read_exact_text(copy: &NamedCopy) -> String {
    copy.store
        .get(&copy.message_id)
        .unwrap_or_else(|error| panic!("{} read failed: {error}", copy.name))
        .unwrap_or_else(|| panic!("{} marked message is absent before expiry", copy.name))
        .plaintext
}

fn exact_text_count(copy: &NamedCopy, marked_text: &str) -> usize {
    copy.store
        .get(&copy.message_id)
        .unwrap_or_else(|error| panic!("{} count read failed: {error}", copy.name))
        .filter(|message| message.plaintext == marked_text)
        .map(|_| 1)
        .unwrap_or(0)
}

fn expire_once(copies: &[NamedCopy; 2], now: i64) -> usize {
    let mut expired = 0usize;
    for copy in copies {
        let prune = prune_at_path(&copy.ledger_path, &KEY, now)
            .unwrap_or_else(|error| panic!("{} prune failed: {error}", copy.name));
        assert_eq!(
            prune.shred_cache_ids,
            vec![copy.message_id.clone()],
            "{} expiry must name exactly its marked cache row",
            copy.name
        );
        let shredded = copy
            .store
            .shred_expired_messages(&prune.shred_cache_ids)
            .unwrap_or_else(|error| panic!("{} shred failed: {error}", copy.name));
        assert_eq!(shredded, 1, "{} expiry must shred one cache row", copy.name);
        assert_eq!(
            verdict_at_path(
                &copy.ledger_path,
                &KEY,
                &copy.scope_key,
                &copy.message_id,
                now
            ),
            ExpiryVerdict::Expired,
            "{} expiry ledger must no longer allow reads",
            copy.name
        );
        expired += prune.expired;
    }
    expired
}
