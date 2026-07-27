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

fn v3_key() -> crypto::aead::Key {
    let bytes = crypto::hkdf::derive_32(&[], SECRET_A, b"osl-message-store-v1").unwrap();
    crypto::aead::Key::from_bytes(bytes)
}

fn v3_seal(aad: &[u8], plaintext: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let key = v3_key();
    let nonce = crypto::random::random_nonce();
    let ct = crypto::aead::seal(&key, &nonce, aad, plaintext).unwrap();
    (nonce.as_bytes().to_vec(), ct)
}

fn build_v3_fixture(dir: &Path, rows: &[StoredMessage]) {
    let sealed: Vec<(String, Vec<u8>, Vec<u8>)> = rows
        .iter()
        .map(|row| {
            let (nonce, ct) = v3_seal(row.discord_message_id.as_bytes(), row.plaintext.as_bytes());
            (row.discord_message_id.clone(), ct, nonce)
        })
        .collect();

    let (canary_nonce, canary_ct) =
        v3_seal(b"osl-message-store/canary", b"osl-message-store-canary-v1");
    let canary: Vec<(String, Vec<u8>)> = vec![
        ("canary_nonce".to_string(), canary_nonce),
        ("canary_ct".to_string(), canary_ct),
    ];

    let db_path = dir.join("messages.sqlite");
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute_batch(
        r#"
CREATE TABLE _meta (key TEXT PRIMARY KEY, value BLOB);
CREATE TABLE messages (
    discord_message_id TEXT PRIMARY KEY,
    channel_id TEXT NOT NULL,
    sender_discord_id TEXT NOT NULL,
    sender_osl_user_id TEXT NOT NULL,
    ciphertext BLOB NOT NULL,
    nonce BLOB NOT NULL,
    decrypted_at INTEGER NOT NULL,
    burned INTEGER NOT NULL DEFAULT 0,
    burned_at INTEGER,
    wrapped_key BLOB,
    scope_type TEXT,
    scope_id TEXT
);
CREATE INDEX idx_messages_channel ON messages(channel_id, decrypted_at DESC);
CREATE TABLE attachments (
    cache_key TEXT PRIMARY KEY,
    discord_message_id TEXT NOT NULL,
    random_filename TEXT NOT NULL,
    mime TEXT NOT NULL,
    ciphertext BLOB NOT NULL,
    nonce BLOB NOT NULL,
    byte_len INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    scope_type TEXT,
    scope_id TEXT,
    sender_discord_id TEXT
);
"#,
    )
    .unwrap();
    for (key, value) in canary {
        conn.execute(
            "INSERT INTO _meta(key, value) VALUES(?1, ?2)",
            params![key, value],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO _meta(key, value) VALUES('schema_version', ?1)",
        params![3u32.to_le_bytes().to_vec()],
    )
    .unwrap();
    for (row, (_, ct, nonce)) in rows.iter().zip(sealed.iter()) {
        conn.execute(
            "INSERT INTO messages (discord_message_id, channel_id, sender_discord_id, \
                sender_osl_user_id, ciphertext, nonce, decrypted_at, burned) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
            params![
                row.discord_message_id,
                row.channel_id,
                row.sender_discord_id,
                row.sender_osl_user_id,
                ct,
                nonce,
                row.decrypted_at,
            ],
        )
        .unwrap();
    }
}

fn schema_version(dir: &Path) -> u32 {
    let conn = rusqlite::Connection::open(dir.join("messages.sqlite")).unwrap();
    let version: Vec<u8> = conn
        .query_row(
            "SELECT value FROM _meta WHERE key = 'schema_version'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    u32::from_le_bytes(version.try_into().unwrap())
}

fn assert_migrated_rows(store: &MessageStore, rows: &[StoredMessage]) {
    for row in rows {
        let out = store
            .get(&row.discord_message_id)
            .unwrap()
            .expect("migrated row should be present");
        assert_eq!(&out, row);
    }
    let listed = store.list_by_channel("v3-chan-a", 10).unwrap();
    let ids: Vec<&str> = listed
        .iter()
        .map(|m| m.discord_message_id.as_str())
        .collect();
    assert_eq!(ids, vec!["v3-newer", "v3-older"]);
}

// ---- roundtrip ----

#[test]
fn roundtrip_put_get_returns_same_plaintext() {
    let tmp = TempDir::new().unwrap();
    let store = open_a(tmp.path());
    let msg = sample(
        "1502771310428819569",
        "1502771310428819560",
        "900000000000000003",
        "liam",
        "hello phase 5b",
        1_700_000_000,
    );
    let other = sample(
        "1502771310428819579",
        "1502771310428820000",
        "900000000000000004",
        "mira",
        "not the first body",
        1_700_000_005,
    );
    // A single-row fixture cannot detect a lookup that ignores its key.
    store.put(&msg).unwrap();
    store.put(&other).unwrap();
    let out = store
        .get("1502771310428819569")
        .unwrap()
        .expect("row should be present");
    assert_eq!(out, msg);
    assert_eq!(out.discord_message_id, "1502771310428819569");
    assert_eq!(out.sender_discord_id, "900000000000000003");
    assert_eq!(out.plaintext, "hello phase 5b");
    let other_out = store
        .get("1502771310428819579")
        .unwrap()
        .expect("second row should be present");
    assert_eq!(other_out, other);
    assert_eq!(other_out.discord_message_id, "1502771310428819579");
    assert_eq!(other_out.sender_discord_id, "900000000000000004");
    assert_eq!(other_out.plaintext, "not the first body");
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
        "900000000000000003",
        "liam",
        &plaintext,
        1_700_000_001,
    );
    let ascii = sample(
        "1502771310428819571",
        "1502771310428819561",
        "900000000000000004",
        "mira",
        "ordinary ascii body",
        1_700_000_002,
    );
    // A single-row fixture cannot detect a lookup that ignores its key.
    store.put(&msg).unwrap();
    store.put(&ascii).unwrap();
    let out = store.get("1502771310428819570").unwrap().unwrap();
    assert_eq!(out.plaintext, plaintext);
    assert_eq!(out.plaintext.as_bytes(), plaintext.as_bytes());
    let ascii_out = store.get("1502771310428819571").unwrap().unwrap();
    assert_eq!(ascii_out.plaintext, "ordinary ascii body");
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
    // This catches an implementation that ignores the channel predicate.
    let listed_ids: Vec<&str> = listed
        .iter()
        .map(|m| m.discord_message_id.as_str())
        .collect();
    assert_eq!(listed_ids, vec!["a-5", "a-4", "a-3"]);
    assert!(!listed_ids.contains(&"b-1"));
    // Sanity: timestamps strictly descending.
    assert!(listed[0].decrypted_at > listed[1].decrypted_at);
    assert!(listed[1].decrypted_at > listed[2].decrypted_at);

    // Limit 100 returns all five (channel B excluded).
    let all = store.list_by_channel("ch-a", 100).unwrap();
    assert_eq!(all.len(), 5);
    assert!(all.iter().all(|m| m.channel_id == "ch-a"));
    let all_ids: Vec<&str> = all.iter().map(|m| m.discord_message_id.as_str()).collect();
    assert_eq!(all_ids, vec!["a-5", "a-4", "a-3", "a-2", "a-1"]);
}

// ---- mark_burned ----

#[test]
fn mark_burned_makes_get_return_none() {
    let tmp = TempDir::new().unwrap();
    let store = open_a(tmp.path());
    let msg = sample("vanish", "ch", "s", "alice", "to be burned", 1);
    let survivor = sample(
        "survives",
        "other-ch",
        "other-s",
        "bob",
        "still readable",
        2,
    );
    // A single-row fixture cannot detect a lookup that ignores its key.
    store.put(&msg).unwrap();
    assert!(store.get("vanish").unwrap().is_some());
    let db_path = tmp.path().join("messages.sqlite");
    // Schema v4 stores no plaintext identifier, so capture this blob before
    // adding the second row that proves key discrimination.
    let before: (Vec<u8>, Vec<u8>) = {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.query_row("SELECT ciphertext, nonce FROM messages", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap()
    };
    store.put(&survivor).unwrap();
    let live_before = store
        .get("survives")
        .unwrap()
        .expect("second row should be readable before burn");
    assert_eq!(live_before, survivor);
    store.mark_burned("vanish").unwrap();
    assert!(store.get("vanish").unwrap().is_none());
    let live_after = store
        .get("survives")
        .unwrap()
        .expect("burn must not touch unrelated row");
    assert_eq!(live_after, survivor);
    let after: (Vec<u8>, Vec<u8>, i64) = {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.query_row(
            "SELECT ciphertext, nonce, burned FROM messages WHERE burned = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap()
    };
    assert_ne!(after.0, before.0, "burn must overwrite local ciphertext");
    assert_ne!(after.1, before.1, "burn must overwrite the AEAD nonce");
    assert!(after.0.iter().all(|byte| *byte == 0));
    assert!(after.1.iter().all(|byte| *byte == 0));
    assert_eq!(after.2, 1);
    let (live_bodies, zeroed_bodies): (usize, usize) = {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let mut stmt = conn.prepare("SELECT ciphertext FROM messages").unwrap();
        let rows = stmt.query_map([], |row| row.get::<_, Vec<u8>>(0)).unwrap();
        let mut live = 0;
        let mut zeroed = 0;
        for row in rows {
            let ct = row.unwrap();
            if ct.iter().all(|byte| *byte == 0) {
                zeroed += 1;
            } else {
                live += 1;
            }
        }
        (live, zeroed)
    };
    assert_eq!(live_bodies, 1);
    assert_eq!(zeroed_bodies, 1);
    let wal = db_path.with_extension("sqlite-wal");
    assert!(
        !wal.exists() || std::fs::metadata(wal).unwrap().len() == 0,
        "burn must truncate WAL page images"
    );
}

#[test]
fn mark_burned_unknown_id_returns_not_found() {
    let tmp = TempDir::new().unwrap();
    let store = open_a(tmp.path());
    let msg = sample("known", "ch", "s", "alice", "stays live", 1);
    store.put(&msg).unwrap();
    // Without it, 'nothing came back' is indistinguishable from 'nothing was ever stored'.
    let before = store
        .get("known")
        .unwrap()
        .expect("positive control row should be present");
    assert_eq!(before, msg);

    let err = store.mark_burned("never-existed").unwrap_err();
    assert!(matches!(err, StoreError::NotFound(_)), "got {err:?}");
    let after = store
        .get("known")
        .unwrap()
        .expect("unknown-id burn must not touch a real row");
    assert_eq!(after, msg);
    assert!(!after.burned);
}

#[test]
fn mark_burned_is_idempotent() {
    let tmp = TempDir::new().unwrap();
    let store = open_a(tmp.path());
    let msg = sample("twice", "ch", "s", "alice", "double burn", 1);
    let other = sample("untouched", "ch", "s", "bob", "must remain", 2);
    store.put(&msg).unwrap();
    store.put(&other).unwrap();
    // Without it, 'nothing came back' is indistinguishable from 'nothing was ever stored'.
    let before = store
        .get("twice")
        .unwrap()
        .expect("row should be present before burn");
    assert_eq!(before, msg);
    store.mark_burned("twice").unwrap();
    // Second mark is a no-op (already burned). Exercises the
    // idempotency branch.
    store.mark_burned("twice").unwrap();
    assert!(store.get("twice").unwrap().is_none());
    let still_live = store
        .get("untouched")
        .unwrap()
        .expect("burn must not wipe unrelated rows");
    assert_eq!(still_live, other);
}

// ---- corruption ----

#[test]
fn corrupted_ciphertext_returns_corrupted_not_panic() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().to_path_buf();
    let first = sample("corrupt-a", "ch", "s", "alice", "tampered", 1);
    let second = sample("corrupt-b", "ch", "s", "bob", "still intact", 2);
    {
        let store = open_a(&path);
        store.put(&first).unwrap();
        store.put(&second).unwrap();
    }
    // Reach into the SQLite file directly and flip a byte in
    // the ciphertext. Use a fresh rusqlite handle (not the
    // store) so we bypass the store's seal/unseal and write a
    // raw bad blob. Tag will fail on next read.
    {
        let conn = rusqlite::Connection::open(path.join("messages.sqlite")).unwrap();
        // Schema v4 stores no plaintext identifier, so corrupt exactly one row
        // by rowid and let public lookups prove which message it was.
        let (rowid, mut ct): (i64, Vec<u8>) = conn
            .query_row(
                "SELECT rowid, ciphertext FROM messages ORDER BY rowid LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        // Flip the last byte (Poly1305 tag tail). Any single-bit
        // flip in either ciphertext or tag invalidates AEAD.
        let last = ct.len() - 1;
        ct[last] ^= 0x01;
        conn.execute(
            "UPDATE messages SET ciphertext = ?1 WHERE rowid = ?2",
            params![ct, rowid],
        )
        .unwrap();
    }
    let store = open_a(&path);
    // Without it, 'nothing came back' is indistinguishable from 'nothing was ever stored'.
    let mut corrupted = 0;
    let mut readable = 0;
    for msg in [&first, &second] {
        match store.get(&msg.discord_message_id) {
            Ok(Some(out)) => {
                assert_eq!(out, *msg);
                readable += 1;
            }
            Err(StoreError::Corrupted(_)) => corrupted += 1,
            other => panic!("expected one corrupted and one readable row, got {other:?}"),
        }
    }
    assert_eq!(corrupted, 1);
    assert_eq!(readable, 1);
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
    let rows = vec![
        sample(
            "v3-older",
            "v3-chan-a",
            "sender-1",
            "alice",
            "old v3 body",
            100,
        ),
        sample(
            "v3-newer",
            "v3-chan-a",
            "sender-2",
            "bob",
            "newer v3 body",
            200,
        ),
        sample(
            "v3-other",
            "v3-chan-b",
            "sender-3",
            "carol",
            "other channel body",
            300,
        ),
    ];
    build_v3_fixture(tmp.path(), &rows);
    assert_eq!(schema_version(tmp.path()), 3);

    // This catches a reopen path that only handles current-schema databases.
    let store = MessageStore::open(tmp.path(), SECRET_A).unwrap();
    assert_migrated_rows(&store, &rows);
    let first_open_version = schema_version(tmp.path());
    drop(store);

    let store2 = MessageStore::open(tmp.path(), SECRET_A).unwrap();
    assert_migrated_rows(&store2, &rows);
    let second_open_version = schema_version(tmp.path());
    assert_eq!(first_open_version, 4);
    assert_eq!(second_open_version, first_open_version);
    store2
        .put(&sample(
            "across-runs",
            "ch",
            "s",
            "alice",
            "see you next session",
            1,
        ))
        .unwrap();
    drop(store2);
    let store3 = MessageStore::open(tmp.path(), SECRET_A).unwrap();
    let m = store3.get("across-runs").unwrap().unwrap();
    assert_eq!(m.plaintext, "see you next session");
    assert_migrated_rows(&store3, &rows);
    assert_eq!(schema_version(tmp.path()), first_open_version);
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
    let other_bytes = b"plain text attachment bytes".to_vec();
    // A single-row fixture cannot detect a lookup that ignores its key.
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
    store
        .put_attachment(
            "1502771310428819570",
            "a1b2c3d4.bin",
            "text/plain",
            &other_bytes,
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
    let other_out = store
        .get_attachment("1502771310428819570", "a1b2c3d4.bin")
        .unwrap()
        .expect("second attachment should be present");
    assert_eq!(other_out.0, "text/plain");
    assert_eq!(other_out.1, other_bytes);
    assert!(store
        .get_attachment("1502771310428819571", "a1b2c3d4.bin")
        .unwrap()
        .is_none());
}

#[test]
fn attachment_get_miss_is_none() {
    let tmp = TempDir::new().unwrap();
    let store = open_a(tmp.path());
    let bytes = b"cached attachment bytes".to_vec();
    store
        .put_attachment(
            "msg1",
            "present.bin",
            "text/plain",
            &bytes,
            None,
            None,
            None,
        )
        .unwrap();
    // Without it, 'nothing came back' is indistinguishable from 'nothing was ever stored'.
    let out = store
        .get_attachment("msg1", "present.bin")
        .unwrap()
        .expect("positive control attachment should be present");
    assert_eq!(out.0, "text/plain");
    assert_eq!(out.1, bytes);
    assert!(store
        .get_attachment("msg1", "missing.bin")
        .unwrap()
        .is_none());
    assert!(store
        .get_attachment("other-msg", "present.bin")
        .unwrap()
        .is_none());
}

#[test]
fn attachment_survives_reopen() {
    let tmp = TempDir::new().unwrap();
    let bytes = b"decrypted image bytes".to_vec();
    let other_bytes = b"reopened text bytes".to_vec();
    {
        let store = open_a(tmp.path());
        // A single-row fixture cannot detect a lookup that ignores its key.
        store
            .put_attachment("msg1", "f.bin", "image/jpeg", &bytes, None, None, None)
            .unwrap();
        store
            .put_attachment(
                "msg2",
                "f.bin",
                "text/plain",
                &other_bytes,
                None,
                None,
                None,
            )
            .unwrap();
    }
    // Reopen with the same secret: the row + its seal must survive.
    let store = open_a(tmp.path());
    let out = store.get_attachment("msg1", "f.bin").unwrap().unwrap();
    assert_eq!(out.0, "image/jpeg");
    assert_eq!(out.1, bytes);
    let other_out = store.get_attachment("msg2", "f.bin").unwrap().unwrap();
    assert_eq!(other_out.0, "text/plain");
    assert_eq!(other_out.1, other_bytes);
    assert!(store.get_attachment("msg3", "f.bin").unwrap().is_none());
}

#[test]
fn attachment_wrong_secret_cannot_unseal() {
    let tmp = TempDir::new().unwrap();
    let original = b"secret bytes".to_vec();
    {
        let store = open_a(tmp.path());
        store
            .put_attachment("msg1", "f.bin", "image/jpeg", &original, None, None, None)
            .unwrap();
        let out = store
            .get_attachment("msg1", "f.bin")
            .unwrap()
            .expect("attachment should read with the correct secret");
        assert_eq!(out.0, "image/jpeg");
        assert_eq!(out.1, original);
    }
    // A different secret fails the canary at open(), so we never
    // even reach get_attachment — assert the open itself refuses.
    match MessageStore::open(tmp.path(), SECRET_B) {
        Ok(_) => panic!("wrong secret must refuse to open"),
        Err(e) => assert!(matches!(e, StoreError::Sealer(_)), "got {e:?}"),
    }

    let (source_ct, source_nonce): (Vec<u8>, Vec<u8>) = {
        let conn = rusqlite::Connection::open(tmp.path().join("messages.sqlite")).unwrap();
        conn.query_row(
            "SELECT ciphertext, nonce FROM attachments ORDER BY rowid LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
    };
    let second = TempDir::new().unwrap();
    let store_b = MessageStore::open(second.path(), SECRET_B).unwrap();
    store_b
        .put_attachment(
            "msg1",
            "f.bin",
            "text/plain",
            b"other store bytes",
            None,
            None,
            None,
        )
        .unwrap();
    let own = store_b
        .get_attachment("msg1", "f.bin")
        .unwrap()
        .expect("second store attachment should be present before transplant");
    assert_eq!(own.1, b"other store bytes");
    {
        let conn = rusqlite::Connection::open(second.path().join("messages.sqlite")).unwrap();
        // Schema v4 has no plaintext cache key, so transplant by rowid.
        let changed = conn
            .execute(
                "UPDATE attachments SET ciphertext = ?1, nonce = ?2 \
                 WHERE rowid = (SELECT rowid FROM attachments ORDER BY rowid LIMIT 1)",
                params![source_ct, source_nonce],
            )
            .unwrap();
        assert_eq!(changed, 1);
    }
    match store_b.get_attachment("msg1", "f.bin") {
        Err(StoreError::Corrupted(_)) | Ok(None) => {}
        Err(e) => panic!("transplanted attachment failed with unexpected error: {e:?}"),
        Ok(Some((_, got))) => {
            panic!("wrong secret returned transplanted attachment bytes: {got:?}")
        }
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
    // This catches trim keeping any three rows instead of the newest three.
    for i in 7..10 {
        let out = store
            .get_attachment(&format!("msg{i}"), "f.bin")
            .unwrap()
            .expect("newest attachment should survive trim");
        assert_eq!(out.0, "image/png");
        assert_eq!(out.1, vec![i as u8; 8]);
    }
    assert!(store.get_attachment("msg6", "f.bin").unwrap().is_none());
}

// ---- timed deletion: sweeper-named batch shred ----

#[test]
fn shred_expired_messages_destroys_named_rows_and_their_attachments() {
    let tmp = TempDir::new().unwrap();
    let store = open_a(tmp.path());
    store
        .put(&sample("expired-1", "ch", "s", "alice", "gone", 1))
        .unwrap();
    store
        .put(&sample("expired-2", "ch", "s", "alice", "also gone", 2))
        .unwrap();
    store
        .put(&sample("still-live", "ch", "s", "alice", "kept", 3))
        .unwrap();
    let still_live_2 = sample("still-live-2", "other-ch", "s2", "bob", "also kept", 4);
    // A single surviving row cannot detect a lookup that ignores its key.
    store.put(&still_live_2).unwrap();
    store
        .put_attachment(
            "expired-1",
            "pic.png",
            "image/png",
            &[7u8; 64],
            None,
            None,
            None,
        )
        .unwrap();

    let shredded = store
        .shred_expired_messages(&["expired-1".to_string(), "expired-2".to_string()])
        .unwrap();
    assert_eq!(shredded, 2);
    assert!(store.get("expired-1").unwrap().is_none());
    assert!(store.get("expired-2").unwrap().is_none());
    assert!(
        store
            .get_attachment("expired-1", "pic.png")
            .unwrap()
            .is_none(),
        "expiry must destroy the decrypted attachment cache too"
    );
    assert!(
        store.get("still-live").unwrap().is_some(),
        "a sweep must never touch a row it did not name"
    );
    let live_two = store
        .get("still-live-2")
        .unwrap()
        .expect("a sweep must not touch a second unnamed row");
    assert_eq!(live_two, still_live_2);

    let db_path = tmp.path().join("messages.sqlite");
    // Schema v4 stores no plaintext identifier, so the shredded rows cannot be
    // named in SQL. Assert over the whole table instead, which is a stronger
    // claim than naming one row: of the four messages, exactly the two expired
    // ones must be burned with a zeroed body, and exactly two must still hold
    // live body.
    let (burned_zeroed, live_bodies, any_wrapped_key): (usize, usize, bool) = {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let mut stmt = conn
            .prepare("SELECT ciphertext, nonce, wrapped_key, burned FROM messages")
            .unwrap();
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Option<Vec<u8>>>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .unwrap();
        let mut burned_zeroed = 0;
        let mut live = 0;
        let mut wrapped = false;
        for row in rows {
            let (ct, nonce, wk, burned) = row.unwrap();
            if wk.is_some() {
                wrapped = true;
            }
            if burned == 1 && ct.iter().all(|b| *b == 0) && nonce.iter().all(|b| *b == 0) {
                burned_zeroed += 1;
            }
            if ct.iter().any(|b| *b != 0) {
                live += 1;
            }
        }
        (burned_zeroed, live, wrapped)
    };
    assert_eq!(
        burned_zeroed, 2,
        "both expired rows must be burned and zeroed"
    );
    assert_eq!(
        live_bodies, 2,
        "the unexpired rows must keep their live bodies"
    );
    assert!(!any_wrapped_key, "a shred must null wrapped_key");
    let wal = db_path.with_extension("sqlite-wal");
    assert!(
        !wal.exists() || std::fs::metadata(wal).unwrap().len() == 0,
        "a shred must truncate WAL page images"
    );
}

#[test]
fn shred_expired_messages_tolerates_unknown_ids_and_is_idempotent() {
    let tmp = TempDir::new().unwrap();
    let store = open_a(tmp.path());
    let known = sample("known", "ch", "s", "alice", "once", 1);
    let untouched = sample("untouched", "ch", "s", "bob", "still here", 2);
    store.put(&known).unwrap();
    store.put(&untouched).unwrap();
    // Without it, 'nothing came back' is indistinguishable from 'nothing was ever stored'.
    let before = store
        .get("known")
        .unwrap()
        .expect("row should be present before shred");
    assert_eq!(before, known);

    // A sweeper legitimately names rows this device never cached. That is not
    // an error, unlike `mark_burned`.
    assert_eq!(
        store
            .shred_expired_messages(&["known".to_string(), "never-cached".to_string()])
            .unwrap(),
        1
    );
    assert!(store.get("known").unwrap().is_none());
    let still_live = store
        .get("untouched")
        .unwrap()
        .expect("sweep must not touch unnamed rows");
    assert_eq!(still_live, untouched);
    // A second sweep reports zero rather than re-stamping burned_at and making
    // an old destruction look fresh.
    assert_eq!(
        store
            .shred_expired_messages(&["known".to_string()])
            .unwrap(),
        0
    );
    let still_live_after_second = store
        .get("untouched")
        .unwrap()
        .expect("idempotent sweep must not remove unnamed rows");
    assert_eq!(still_live_after_second, untouched);
    assert_eq!(store.shred_expired_messages(&[]).unwrap(), 0);
}
