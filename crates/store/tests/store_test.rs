//! Integration tests for `crates/store`.
//!
//! Each test owns a fresh `tempfile::TempDir` so concurrent test
//! runs don't share a SQLite file. The 32-byte
//! `identity_secret` test vector is `[1u8; 32]` (or a deliberate
//! variant) — the key derivation is deterministic, so a fixed
//! input gives a stable derived AEAD key across runs.

use rusqlite::params;
use std::path::Path;
use store::{MessageStore, StoreError, StoredMessage};
use tempfile::TempDir;

const SECRET_A: &[u8; 32] = &[1u8; 32];
const SECRET_B: &[u8; 32] = &[2u8; 32];

fn open_a(dir: &Path) -> MessageStore {
    MessageStore::open(dir, SECRET_A).expect("open with SECRET_A should succeed")
}

fn sample(
    msg_id: &str,
    channel_id: &str,
    sender_did: &str,
    sender_osl: &str,
    plaintext: &str,
    decrypted_at: i64,
) -> StoredMessage {
    StoredMessage {
        discord_message_id: msg_id.to_string(),
        channel_id: channel_id.to_string(),
        sender_discord_id: sender_did.to_string(),
        sender_osl_user_id: sender_osl.to_string(),
        plaintext: plaintext.to_string(),
        decrypted_at,
        burned: false,
    }
}

// ---- roundtrip ----

#[test]
fn roundtrip_put_get_returns_same_plaintext() {
    let tmp = TempDir::new().unwrap();
    let store = open_a(tmp.path());
    let msg = sample(
        "1502771310428819569",
        "1502771310428819560",
        "1477008451799482419",
        "liam",
        "hello phase 5b",
        1_700_000_000,
    );
    store.put(&msg).unwrap();
    let out = store
        .get("1502771310428819569")
        .unwrap()
        .expect("row should be present");
    assert_eq!(out, msg);
}

#[test]
fn roundtrip_unicode_and_long_plaintext() {
    let tmp = TempDir::new().unwrap();
    let store = open_a(tmp.path());
    // Unicode + control bytes + a long string to exercise AEAD
    // bulk path. Discord's per-message cap is 2000 chars; this
    // is well under.
    let plaintext = "héllo 🌐 — multi-line\nbody\twith\rcontrol chars".repeat(5);
    let msg = sample(
        "1502771310428819570",
        "1502771310428819560",
        "1477008451799482419",
        "liam",
        &plaintext,
        1_700_000_001,
    );
    store.put(&msg).unwrap();
    let out = store.get("1502771310428819570").unwrap().unwrap();
    assert_eq!(out.plaintext, plaintext);
}

// ---- list_by_channel ----

#[test]
fn list_by_channel_returns_desc_by_decrypted_at_respects_limit() {
    let tmp = TempDir::new().unwrap();
    let store = open_a(tmp.path());

    // Put 5 messages in channel A (with mixed-order
    // decrypted_at) plus one in channel B (should never appear).
    let plan = [
        ("a-1", "ch-a", "sender", "alice", "first", 1_700_000_010),
        ("a-2", "ch-a", "sender", "alice", "second", 1_700_000_020),
        ("a-3", "ch-a", "sender", "alice", "third", 1_700_000_030),
        ("a-4", "ch-a", "sender", "alice", "fourth", 1_700_000_040),
        ("a-5", "ch-a", "sender", "alice", "fifth", 1_700_000_050),
        (
            "b-1",
            "ch-b",
            "sender",
            "alice",
            "other-channel",
            1_700_000_999,
        ),
    ];
    for (mid, cid, sdid, sosl, pt, t) in &plan {
        store.put(&sample(mid, cid, sdid, sosl, pt, *t)).unwrap();
    }

    // Limit 3 should return the three newest from ch-a.
    let listed = store.list_by_channel("ch-a", 3).unwrap();
    assert_eq!(listed.len(), 3);
    assert_eq!(listed[0].discord_message_id, "a-5");
    assert_eq!(listed[1].discord_message_id, "a-4");
    assert_eq!(listed[2].discord_message_id, "a-3");
    // Sanity: timestamps strictly descending.
    assert!(listed[0].decrypted_at > listed[1].decrypted_at);
    assert!(listed[1].decrypted_at > listed[2].decrypted_at);

    // Limit 100 returns all five (channel B excluded).
    let all = store.list_by_channel("ch-a", 100).unwrap();
    assert_eq!(all.len(), 5);
    assert!(all.iter().all(|m| m.channel_id == "ch-a"));
}

// ---- mark_burned ----

#[test]
fn mark_burned_makes_get_return_none() {
    let tmp = TempDir::new().unwrap();
    let store = open_a(tmp.path());
    store
        .put(&sample("vanish", "ch", "s", "alice", "to be burned", 1))
        .unwrap();
    assert!(store.get("vanish").unwrap().is_some());
    store.mark_burned("vanish").unwrap();
    assert!(store.get("vanish").unwrap().is_none());
}

#[test]
fn mark_burned_unknown_id_returns_not_found() {
    let tmp = TempDir::new().unwrap();
    let store = open_a(tmp.path());
    let err = store.mark_burned("never-existed").unwrap_err();
    assert!(matches!(err, StoreError::NotFound(_)), "got {err:?}");
}

#[test]
fn mark_burned_is_idempotent() {
    let tmp = TempDir::new().unwrap();
    let store = open_a(tmp.path());
    store
        .put(&sample("twice", "ch", "s", "alice", "double burn", 1))
        .unwrap();
    store.mark_burned("twice").unwrap();
    // Second mark is a no-op (already burned). Exercises the
    // idempotency branch.
    store.mark_burned("twice").unwrap();
    assert!(store.get("twice").unwrap().is_none());
}

// ---- corruption ----

#[test]
fn corrupted_ciphertext_returns_corrupted_not_panic() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().to_path_buf();
    {
        let store = open_a(&path);
        store
            .put(&sample("corrupt-me", "ch", "s", "alice", "tampered", 1))
            .unwrap();
    }
    // Reach into the SQLite file directly and flip a byte in
    // the ciphertext. Use a fresh rusqlite handle (not the
    // store) so we bypass the store's seal/unseal and write a
    // raw bad blob. Tag will fail on next read.
    {
        let conn = rusqlite::Connection::open(path.join("messages.sqlite")).unwrap();
        let mut ct: Vec<u8> = conn
            .query_row(
                "SELECT ciphertext FROM messages WHERE discord_message_id = ?1",
                params!["corrupt-me"],
                |r| r.get(0),
            )
            .unwrap();
        // Flip the last byte (Poly1305 tag tail). Any single-bit
        // flip in either ciphertext or tag invalidates AEAD.
        let last = ct.len() - 1;
        ct[last] ^= 0x01;
        conn.execute(
            "UPDATE messages SET ciphertext = ?1 WHERE discord_message_id = ?2",
            params![ct, "corrupt-me"],
        )
        .unwrap();
    }
    let store = open_a(&path);
    let err = store.get("corrupt-me").unwrap_err();
    assert!(matches!(err, StoreError::Corrupted(_)), "got {err:?}");
}

// ---- wrong-secret rejection ----

#[test]
fn open_with_wrong_secret_returns_sealer_error() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().to_path_buf();
    {
        let store_a = MessageStore::open(&path, SECRET_A).unwrap();
        store_a
            .put(&sample(
                "present",
                "ch",
                "s",
                "alice",
                "shouldn't unlock",
                1,
            ))
            .unwrap();
    }
    match MessageStore::open(&path, SECRET_B) {
        Ok(_) => panic!("wrong secret must not unlock"),
        Err(e) => assert!(matches!(e, StoreError::Sealer(_)), "got {e:?}"),
    }
}

// ---- migration framework ----

#[test]
fn reopen_with_correct_secret_migration_idempotent() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().to_path_buf();
    {
        let store = MessageStore::open(&path, SECRET_A).unwrap();
        store
            .put(&sample(
                "across-runs",
                "ch",
                "s",
                "alice",
                "see you next session",
                1,
            ))
            .unwrap();
    }
    // Drop the first store, re-open. Migration runs again
    // (it's idempotent); canary verifies. Data survives.
    let store2 = MessageStore::open(&path, SECRET_A).unwrap();
    let m = store2.get("across-runs").unwrap().unwrap();
    assert_eq!(m.plaintext, "see you next session");
}

#[test]
fn reopen_with_future_schema_version_refuses() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().to_path_buf();
    {
        // Initialise normally so _meta and the canary are set up.
        let _ = MessageStore::open(&path, SECRET_A).unwrap();
    }
    // Manually stamp a future schema_version to simulate a DB
    // written by a later binary. The migration framework
    // should refuse to open rather than proceed under
    // unknown-future semantics.
    {
        let conn = rusqlite::Connection::open(path.join("messages.sqlite")).unwrap();
        let bytes = (999u32).to_le_bytes();
        conn.execute(
            "INSERT INTO _meta(key, value) VALUES('schema_version', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![&bytes[..]],
        )
        .unwrap();
    }
    match MessageStore::open(&path, SECRET_A) {
        Ok(_) => panic!("future schema must refuse"),
        Err(e) => assert!(matches!(e, StoreError::Schema(_)), "got {e:?}"),
    }
}

// ---- Beta 1.0: attachment cache ----

#[test]
fn attachment_put_get_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let store = open_a(tmp.path());
    let bytes: Vec<u8> = (0..4096).map(|i| (i % 256) as u8).collect();
    store
        .put_attachment(
            "1502771310428819569",
            "a1b2c3d4.bin",
            "image/png",
            &bytes,
            None,
            None,
            None,
        )
        .unwrap();
    let out = store
        .get_attachment("1502771310428819569", "a1b2c3d4.bin")
        .unwrap()
        .expect("attachment should be present");
    assert_eq!(out.0, "image/png");
    assert_eq!(out.1, bytes);
}

#[test]
fn attachment_get_miss_is_none() {
    let tmp = TempDir::new().unwrap();
    let store = open_a(tmp.path());
    assert!(store
        .get_attachment("nope", "nope.bin")
        .unwrap()
        .is_none());
}

#[test]
fn attachment_survives_reopen() {
    let tmp = TempDir::new().unwrap();
    let bytes = b"decrypted image bytes".to_vec();
    {
        let store = open_a(tmp.path());
        store
            .put_attachment("msg1", "f.bin", "image/jpeg", &bytes, None, None, None)
            .unwrap();
    }
    // Reopen with the same secret: the row + its seal must survive.
    let store = open_a(tmp.path());
    let out = store.get_attachment("msg1", "f.bin").unwrap().unwrap();
    assert_eq!(out.1, bytes);
}

#[test]
fn attachment_wrong_secret_cannot_unseal() {
    let tmp = TempDir::new().unwrap();
    {
        let store = open_a(tmp.path());
        store
            .put_attachment("msg1", "f.bin", "image/jpeg", b"secret bytes", None, None, None)
            .unwrap();
    }
    // A different secret fails the canary at open(), so we never
    // even reach get_attachment — assert the open itself refuses.
    match MessageStore::open(tmp.path(), SECRET_B) {
        Ok(_) => panic!("wrong secret must refuse to open"),
        Err(e) => assert!(matches!(e, StoreError::Sealer(_)), "got {e:?}"),
    }
}

#[test]
fn attachment_trim_keeps_newest() {
    let tmp = TempDir::new().unwrap();
    let store = open_a(tmp.path());
    for i in 0..10 {
        store
            .put_attachment(
                &format!("msg{i}"),
                "f.bin",
                "image/png",
                &[i as u8; 8],
                None,
                None,
                None,
            )
            .unwrap();
        // Space out created_at so ordering is deterministic.
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let deleted = store.trim_attachments(3).unwrap();
    assert_eq!(deleted, 7);
    // Newest (msg9) should remain; oldest (msg0) should be gone.
    assert!(store.get_attachment("msg9", "f.bin").unwrap().is_some());
    assert!(store.get_attachment("msg0", "f.bin").unwrap().is_none());
}
