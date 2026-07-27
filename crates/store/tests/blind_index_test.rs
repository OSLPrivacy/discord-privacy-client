//! Acceptance tests for schema v4: sealed row metadata + keyed blind indexes.
//!
//! These define what counts as proof for the defect-6 fix (plaintext
//! identifiers at rest). They were written **before** the implementation and
//! are expected to fail until it lands.
//!
//! ## The two the owner asked for by name
//!
//! - `old_v3_database_migrates_and_stays_fully_readable` — a genuine v3 file,
//!   built with real sealed bodies, must open and read after upgrade. A
//!   migration that orphans installed history is a worse defect than the one
//!   it fixes.
//! - `no_plaintext_identifier_survives_anywhere_in_the_file` — sweeps **every
//!   value in every column of every table** and asserts no identifier appears.
//!   It does not check the columns the author remembered to seal; it checks the
//!   file. That distinction is the whole point: a test that only inspects the
//!   fields you thought of can only re-assert what you already believed.
//!
//! ## Fixture honesty
//!
//! The v3 fixture seals its rows by reaching for the audited primitives
//! directly, reproducing the AAD rules the **v3 build** used, and never touches
//! `MessageStore` to produce them.
//!
//! The first version of it did the opposite: it wrote rows through the current
//! store and lifted the sealed bytes back out. That fixture was self-referential
//! — it reproduced whatever AAD the current code happened to use, so when the
//! attachment AAD was changed in a way that would have made every migrated
//! attachment undecryptable, the test still passed. It could only confirm that
//! the implementation agreed with itself.
//!
//! Both fixture builders are therefore written against the old format on
//! purpose. If someone changes an AAD, `old_v3_attachment_still_decrypts_after_migration`
//! fails — which is the entire reason it exists.

use std::collections::HashSet;
use std::path::Path;
use store::{MessageStore, StoredMessage};
use tempfile::TempDir;

const SECRET_A: &[u8; 32] = &[1u8; 32];

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

/// Every value held in every column of every user table, as raw bytes.
///
/// Deliberately schema-agnostic: it enumerates tables from `sqlite_master` and
/// columns from `PRAGMA table_info`, so a column added later is swept without
/// anyone remembering to update this helper.
fn all_stored_bytes(db_path: &Path) -> Vec<(String, String, Vec<u8>)> {
    let conn = rusqlite::Connection::open(db_path).unwrap();
    let tables: Vec<String> = {
        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            )
            .unwrap();
        let rows = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
        rows.map(|r| r.unwrap()).collect()
    };

    let mut out = Vec::new();
    for table in tables {
        let cols: Vec<String> = {
            let mut stmt = conn
                .prepare(&format!("PRAGMA table_info({table})"))
                .unwrap();
            let rows = stmt.query_map([], |r| r.get::<_, String>(1)).unwrap();
            rows.map(|r| r.unwrap()).collect()
        };
        for col in cols {
            let mut stmt = conn
                .prepare(&format!("SELECT \"{col}\" FROM \"{table}\""))
                .unwrap();
            let rows = stmt
                .query_map([], |r| {
                    // Take every value as raw bytes regardless of declared
                    // affinity, so a TEXT id stored in a BLOB column (or the
                    // reverse) cannot slip past.
                    Ok(match r.get_ref(0) {
                        Ok(rusqlite::types::ValueRef::Text(t)) => t.to_vec(),
                        Ok(rusqlite::types::ValueRef::Blob(b)) => b.to_vec(),
                        Ok(rusqlite::types::ValueRef::Integer(i)) => i.to_string().into_bytes(),
                        Ok(rusqlite::types::ValueRef::Real(f)) => f.to_string().into_bytes(),
                        _ => Vec::new(),
                    })
                })
                .unwrap();
            for row in rows {
                out.push((table.clone(), col.clone(), row.unwrap()));
            }
        }
    }
    out
}

/// The database file plus anything SQLite left beside it.
///
/// A row is not sealed if its plaintext predecessor is still sitting in the
/// write-ahead log, and a migration has not scrubbed anything if the old table's
/// pages are still recoverable in the file's free space. Reading the bytes is
/// the only way to see either — the SQL layer will not show you a page it has
/// stopped pointing at.
fn raw_file_bytes(db_path: &Path) -> Vec<u8> {
    let mut blob = std::fs::read(db_path).unwrap_or_default();
    for ext in ["sqlite-wal", "sqlite-shm", "sqlite-journal"] {
        let side = db_path.with_extension(ext);
        if side.exists() {
            blob.extend_from_slice(&std::fs::read(side).unwrap());
        }
    }
    blob
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
}

// ---- The owner's second requirement ----

/// No plaintext identifier may be recoverable from the file without the key.
///
/// Identifiers chosen to be long and distinctive so an incidental byte match is
/// not credible.
#[test]
fn no_plaintext_identifier_survives_anywhere_in_the_file() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.sqlite");
    let store = open_a(tmp.path());

    let msg_id = "998877665544332211";
    let channel_id = "112233445566778899";
    let sender_did = "555000555000555000";
    let sender_osl = "distinctive-osl-handle";
    let filename = "distinctive-attachment-name.png";
    let mime = "image/distinctive-type";

    store
        .put(&sample(
            msg_id,
            channel_id,
            sender_did,
            sender_osl,
            "body text",
            1_700_000_000,
        ))
        .unwrap();
    store
        .put_attachment(
            msg_id,
            filename,
            mime,
            b"PIXELS",
            Some("dm"),
            Some(channel_id),
            Some(sender_did),
        )
        .unwrap();

    // Positive path: the store itself must still be able to read all of this
    // back, or "nothing leaked" would be trivially true of an empty database.
    assert_eq!(
        store.get(msg_id).unwrap().unwrap().sender_osl_user_id,
        sender_osl
    );
    assert!(store.get_attachment(msg_id, filename).unwrap().is_some());
    assert_eq!(store.list_by_channel(channel_id, 10).unwrap().len(), 1);
    drop(store);

    let needles: Vec<(&str, &[u8])> = vec![
        ("discord_message_id", msg_id.as_bytes()),
        ("channel_id", channel_id.as_bytes()),
        ("sender_discord_id", sender_did.as_bytes()),
        ("sender_osl_user_id", sender_osl.as_bytes()),
        ("attachment filename", filename.as_bytes()),
        ("attachment mime", mime.as_bytes()),
    ];

    let stored = all_stored_bytes(&db_path);
    assert!(
        !stored.is_empty(),
        "positive path: the sweep must actually have found values to inspect"
    );

    for (label, needle) in &needles {
        for (table, col, value) in &stored {
            assert!(
                !contains(value, needle),
                "plaintext {label} is recoverable at {table}.{col} without the store key"
            );
        }
    }
}

/// The same sweep against the raw file on disk, including any WAL beside it.
/// Column-level checks miss content that SQLite left in free pages.
#[test]
fn no_plaintext_identifier_survives_in_the_raw_file_bytes() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.sqlite");
    let store = open_a(tmp.path());

    let msg_id = "998877665544332211";
    let channel_id = "112233445566778899";
    let sender_did = "555000555000555000";
    let sender_osl = "distinctive-osl-handle";
    let filename = "distinctive-attachment-name.png";
    let mime = "image/distinctive-type";
    store
        .put(&sample(
            msg_id,
            channel_id,
            sender_did,
            sender_osl,
            "body text",
            1_700_000_000,
        ))
        .unwrap();
    store
        .put_attachment(
            msg_id,
            filename,
            mime,
            b"PIXELS",
            Some("dm"),
            Some(channel_id),
            Some(sender_did),
        )
        .unwrap();
    // Positive path: everything must be readable before we claim it is hidden.
    assert!(store.get(msg_id).unwrap().is_some());
    assert!(store.get_attachment(msg_id, filename).unwrap().is_some());
    drop(store);

    let blob = raw_file_bytes(&db_path);
    assert!(
        !blob.is_empty(),
        "positive path: the file sweep must have bytes to inspect"
    );

    for (label, needle) in [
        ("channel id", channel_id.as_bytes()),
        ("OSL user id", sender_osl.as_bytes()),
        ("message id", msg_id.as_bytes()),
        ("sender Discord id", sender_did.as_bytes()),
        ("attachment filename", filename.as_bytes()),
        ("attachment MIME", mime.as_bytes()),
    ] {
        assert!(
            !contains(&blob, needle),
            "plaintext {label} is recoverable from the raw database file"
        );
    }
}

// ---- The owner's first requirement: forward migration ----

/// Seal the way the **v3 build** sealed, using the audited primitives directly.
///
/// Deliberately independent of `crates/store`. An earlier version of this
/// fixture lifted sealed bytes out of a store built from current code, which
/// made it self-referential: it reproduced whatever AAD the code happened to
/// use, so it could not detect a change to that AAD — and a change to the
/// attachment AAD is exactly the migration bug this file exists to catch. A
/// fixture that follows the implementation can only confirm the implementation
/// agrees with itself.
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

/// Build a genuine v3 database: real v3 schema, bodies sealed under v3's own
/// AAD rules, plaintext metadata columns as v3 had them.
fn build_v3_fixture(dir: &Path, rows: &[StoredMessage]) {
    // v3 sealed a message body with AAD = the plaintext discord_message_id.
    let sealed: Vec<(String, Vec<u8>, Vec<u8>)> = rows
        .iter()
        .map(|row| {
            let (nonce, ct) = v3_seal(row.discord_message_id.as_bytes(), row.plaintext.as_bytes());
            (row.discord_message_id.clone(), ct, nonce)
        })
        .collect();

    // The canary as v3 wrote it, so the fixture opens under the same secret.
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
            rusqlite::params![key, value],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO _meta(key, value) VALUES('schema_version', ?1)",
        rusqlite::params![3u32.to_le_bytes().to_vec()],
    )
    .unwrap();
    for (row, (_, ct, nonce)) in rows.iter().zip(sealed.iter()) {
        conn.execute(
            "INSERT INTO messages (discord_message_id, channel_id, sender_discord_id, \
                sender_osl_user_id, ciphertext, nonce, decrypted_at, burned) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
            rusqlite::params![
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

/// Append a v3-shaped attachment row to an existing v3 fixture, carrying a
/// genuine sealed body lifted from a real store.
fn add_v3_attachment(dir: &Path, msg_id: &str, filename: &str, mime: &str, bytes: &[u8]) {
    // v3 sealed an attachment body with AAD = the plaintext cache key. Sealing
    // it here, rather than lifting it from a current-code store, is what makes
    // this fixture able to fail.
    let cache_key = format!("{msg_id}/{filename}");
    let (nonce, ct) = v3_seal(cache_key.as_bytes(), bytes);

    let conn = rusqlite::Connection::open(dir.join("messages.sqlite")).unwrap();
    conn.execute(
        "INSERT INTO attachments (cache_key, discord_message_id, random_filename, mime, \
            ciphertext, nonce, byte_len, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            cache_key,
            msg_id,
            filename,
            mime,
            ct,
            nonce,
            bytes.len() as i64,
            1_700_000_000i64,
        ],
    )
    .unwrap();
}

/// Build a **v1** database: the original schema, before the v2 columns and
/// before the `attachments` table existed at all.
///
/// An installed profile can be sitting at any older version, not just the one
/// immediately before current. A migration tested only against v3 is a
/// migration whose other inputs are assumptions.
fn build_v1_fixture(dir: &Path, rows: &[StoredMessage]) {
    let (canary_nonce, canary_ct) =
        v3_seal(b"osl-message-store/canary", b"osl-message-store-canary-v1");

    let conn = rusqlite::Connection::open(dir.join("messages.sqlite")).unwrap();
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
    burned INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_messages_channel ON messages(channel_id, decrypted_at DESC);
"#,
    )
    .unwrap();
    conn.execute(
        "INSERT INTO _meta(key, value) VALUES('canary_nonce', ?1)",
        rusqlite::params![canary_nonce],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO _meta(key, value) VALUES('canary_ct', ?1)",
        rusqlite::params![canary_ct],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO _meta(key, value) VALUES('schema_version', ?1)",
        rusqlite::params![1u32.to_le_bytes().to_vec()],
    )
    .unwrap();
    for row in rows {
        let (nonce, ct) = v3_seal(row.discord_message_id.as_bytes(), row.plaintext.as_bytes());
        conn.execute(
            "INSERT INTO messages (discord_message_id, channel_id, sender_discord_id, \
                sender_osl_user_id, ciphertext, nonce, decrypted_at, burned) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
            rusqlite::params![
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

/// A v1 profile — no v2 columns, no `attachments` table — must migrate straight
/// to v4 without losing anything.
#[test]
fn v1_database_migrates_all_the_way_to_v4() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.sqlite");
    let rows = vec![
        sample("v1-a", "chan", "sender-1", "alice", "ancient history", 10),
        sample("v1-b", "chan", "sender-2", "bob", "also ancient", 20),
    ];
    build_v1_fixture(tmp.path(), &rows);

    let store = open_a(tmp.path());

    assert_eq!(
        store.get("v1-a").unwrap().unwrap().plaintext,
        "ancient history",
        "a v1 row did not survive the migration"
    );
    let listed = store.list_by_channel("chan", 10).unwrap();
    assert_eq!(listed.len(), 2, "v1 channel membership was lost");
    assert_eq!(
        listed[0].discord_message_id, "v1-b",
        "v1 migration did not preserve newest-first ordering"
    );

    // And it must land on v4, not stall at an intermediate version.
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let version: Vec<u8> = conn
        .query_row(
            "SELECT value FROM _meta WHERE key = 'schema_version'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(u32::from_le_bytes(version.try_into().unwrap()), 4);

    // The store must be usable afterwards, not merely readable.
    store
        .put(&sample(
            "v1-c",
            "chan",
            "sender-1",
            "alice",
            "new message",
            30,
        ))
        .unwrap();
    assert!(store.get("v1-c").unwrap().is_some());
}

/// Migration purges cached attachments whose message row is already gone —
/// residue the pre-fix `delete_messages_in_channel` left behind, which no burn
/// predicate could ever reach again.
///
/// The negative control is the point: an attachment whose message still exists
/// must survive. A purge that simply emptied the table would pass the first
/// assertion alone.
#[test]
fn migration_purges_orphaned_attachments_but_keeps_linked_ones() {
    let tmp = TempDir::new().unwrap();
    let rows = vec![sample(
        "kept-msg",
        "chan-a",
        "sender-1",
        "alice",
        "still here",
        100,
    )];
    build_v3_fixture(tmp.path(), &rows);
    add_v3_attachment(tmp.path(), "kept-msg", "kept.png", "image/png", b"KEEP-ME");
    // An orphan: no `messages` row named `gone-msg` exists.
    add_v3_attachment(
        tmp.path(),
        "gone-msg",
        "orphan.png",
        "image/png",
        b"ORPHANED",
    );

    // Positive path: both are really in the v3 file before the migration runs.
    {
        let conn = rusqlite::Connection::open(tmp.path().join("messages.sqlite")).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM attachments", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            n, 2,
            "positive path: the fixture must hold both attachments"
        );
    }

    let store = open_a(tmp.path());

    assert!(
        store
            .get_attachment("kept-msg", "kept.png")
            .unwrap()
            .is_some(),
        "migration purged an attachment whose message still exists"
    );
    assert!(
        store
            .get_attachment("gone-msg", "orphan.png")
            .unwrap()
            .is_none(),
        "migration kept a cached picture whose message was already deleted — \
         residue no burn can reach"
    );
}

/// A cached attachment written by a v3 build must still be decryptable after
/// the migration.
///
/// This test exists because it caught a real data-loss bug during development.
/// The migration copies attachment `ciphertext` byte-for-byte, and an earlier
/// draft of v4 sealed attachment bodies under the cache key's *blind index*
/// instead of the plaintext cache key. Every migrated attachment would have
/// failed to decrypt, and nothing detected it: the message-only fixture never
/// exercised an attachment, so the gap was invisible rather than covered.
#[test]
fn old_v3_attachment_still_decrypts_after_migration() {
    let tmp = TempDir::new().unwrap();
    let rows = vec![sample(
        "m-att-1",
        "chan-a",
        "sender-1",
        "alice",
        "has a picture",
        100,
    )];
    build_v3_fixture(tmp.path(), &rows);
    add_v3_attachment(
        tmp.path(),
        "m-att-1",
        "pic.png",
        "image/png",
        b"REAL-PIXELS",
    );

    // The upgrade.
    let store = open_a(tmp.path());

    let got = store
        .get_attachment("m-att-1", "pic.png")
        .expect("migrated attachment must not error")
        .expect("migrated attachment must still be present");
    assert_eq!(
        got.0, "image/png",
        "migration lost the attachment MIME type"
    );
    assert_eq!(
        got.1, b"REAL-PIXELS",
        "migration made the cached attachment undecryptable"
    );
}

/// A real v3 database must survive the upgrade with every message readable and
/// the channel view unchanged.
#[test]
fn old_v3_database_migrates_and_stays_fully_readable() {
    let tmp = TempDir::new().unwrap();
    let rows = vec![
        sample("m-old-1", "chan-a", "sender-1", "alice", "oldest", 100),
        sample("m-old-2", "chan-a", "sender-2", "bob", "middle", 200),
        sample(
            "m-old-3",
            "chan-b",
            "sender-1",
            "alice",
            "other channel",
            300,
        ),
    ];
    build_v3_fixture(tmp.path(), &rows);

    // The upgrade.
    let store = open_a(tmp.path());

    assert_eq!(
        store.get("m-old-1").unwrap().unwrap().plaintext,
        "oldest",
        "migration orphaned a v3 row"
    );
    assert_eq!(
        store.get("m-old-2").unwrap().unwrap().sender_osl_user_id,
        "bob",
        "migration lost v3 row metadata"
    );

    let listed = store.list_by_channel("chan-a", 10).unwrap();
    assert_eq!(listed.len(), 2, "migration lost channel membership");
    assert_eq!(
        listed[0].discord_message_id, "m-old-2",
        "migration did not preserve newest-first ordering"
    );

    assert_eq!(store.list_by_channel("chan-b", 10).unwrap().len(), 1);
}

/// The migration must also scrub what it migrated: a v3 file full of plaintext
/// identifiers must not still contain them afterwards.
#[test]
fn migrating_a_v3_database_removes_its_plaintext_identifiers() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.sqlite");
    let rows = vec![sample(
        "998877665544332211",
        "112233445566778899",
        "555000555000555000",
        "distinctive-osl-handle",
        "body",
        100,
    )];
    build_v3_fixture(tmp.path(), &rows);

    {
        let raw = std::fs::read(&db_path).unwrap();
        assert!(
            contains(&raw, b"distinctive-osl-handle"),
            "positive path: the v3 fixture must actually contain the plaintext \
             identifier this test claims to remove"
        );
    }

    let store = open_a(tmp.path());
    assert!(store.get("998877665544332211").unwrap().is_some());
    drop(store);

    // Scan the RAW FILE, not the live SQL values.
    //
    // Querying live rows only proves the new table is clean. The v3 table's
    // pages are still in the file until `VACUUM` rewrites it, so a version of
    // this test that inspected live values would pass with the scrub removed
    // entirely — and the scrub is the whole point of the migration.
    for (label, needle) in [
        ("OSL handle", b"distinctive-osl-handle".as_slice()),
        ("message id", b"998877665544332211".as_slice()),
        ("channel id", b"112233445566778899".as_slice()),
        ("sender id", b"555000555000555000".as_slice()),
    ] {
        assert!(
            !contains(&raw_file_bytes(&db_path), needle),
            "migration left the plaintext {label} recoverable in the database file"
        );
    }
}

/// Defined downgrade: after migration the on-disk version is 4, and an older
/// binary — which recognises at most 3 — refuses to open rather than reading a
/// schema it does not understand.
#[test]
fn migration_bumps_schema_version_so_older_binaries_refuse_cleanly() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.sqlite");
    let store = open_a(tmp.path());
    store
        .put(&sample("v", "chan", "sender", "alice", "body", 1))
        .unwrap();
    drop(store);

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let version: Vec<u8> = conn
        .query_row(
            "SELECT value FROM _meta WHERE key = 'schema_version'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        u32::from_le_bytes(version.try_into().unwrap()),
        4,
        "the blind-index migration must bump the schema version, so an older \
         binary refuses the file instead of misreading it"
    );
}

// ---- Blind index properties ----

/// The same string used in two different roles must not produce the same index,
/// or an observer could tell that a channel id equals a sender id and re-link
/// the graph across columns.
#[test]
fn blind_indexes_are_domain_separated_across_fields() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.sqlite");
    let store = open_a(tmp.path());

    let shared = "424242424242424242";
    store
        .put(&sample("mid-1", shared, shared, "alice", "body", 1))
        .unwrap();
    drop(store);

    // Assert on the NAMED columns, pairwise.
    //
    // Collecting every 32-byte value and asserting "at least two distinct" is
    // not the same claim: if channel and sender shared a domain then
    // `chan_bi == sender_bi`, but `mid_bi` is derived from a different input and
    // supplies a second distinct value, so that weaker form passes under the
    // exact regression it is named for.
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let (mid_bi, chan_bi, sender_bi): (Vec<u8>, Vec<u8>, Vec<u8>) = conn
        .query_row("SELECT mid_bi, chan_bi, sender_bi FROM messages", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .unwrap();

    assert_eq!(
        chan_bi.len(),
        32,
        "positive path: blind indexes must be present"
    );
    assert_ne!(
        chan_bi, sender_bi,
        "channel and sender blind indexes are identical for the same input — \
         the per-field domain separation is missing, and an offline reader can \
         tell a channel id equals a sender id"
    );
    assert_ne!(mid_bi, chan_bi, "message and channel domains collide");
    assert_ne!(mid_bi, sender_bi, "message and sender domains collide");
}

/// A different store secret must produce different blind indexes for the same
/// identifiers, or two databases could be correlated against each other.
#[test]
fn blind_indexes_differ_under_a_different_secret() {
    let dir_a = TempDir::new().unwrap();
    let dir_b = TempDir::new().unwrap();

    let row = sample("mid-x", "chan-x", "sender-x", "alice", "body", 1);
    MessageStore::open(dir_a.path(), SECRET_A)
        .unwrap()
        .put(&row)
        .unwrap();
    MessageStore::open(dir_b.path(), &[9u8; 32])
        .unwrap()
        .put(&row)
        .unwrap();

    let idx = |p: &Path| -> HashSet<Vec<u8>> {
        all_stored_bytes(&p.join("messages.sqlite"))
            .into_iter()
            .filter(|(table, _, v)| table == "messages" && v.len() == 32)
            .map(|(_, _, v)| v)
            .collect()
    };

    let a = idx(dir_a.path());
    let b = idx(dir_b.path());
    assert!(
        !a.is_empty(),
        "positive path: store A must have blind indexes"
    );
    assert!(
        a.is_disjoint(&b),
        "two stores with different secrets produced identical blind indexes, \
         so their databases can be correlated"
    );
}

// ---- Equality lookups must still work through the blinded columns ----

/// Everything the store does by exact match must keep working, or the fix has
/// traded a privacy defect for a functional one.
#[test]
fn all_equality_lookups_still_work_after_blinding() {
    let tmp = TempDir::new().unwrap();
    let store = open_a(tmp.path());

    store
        .put(&sample("q1", "chan-q", "sender-q", "alice", "one", 1))
        .unwrap();
    store
        .put(&sample("q2", "chan-q", "other", "bob", "two", 2))
        .unwrap();
    store
        .put_attachment("q1", "f.png", "image/png", b"PIX", None, None, None)
        .unwrap();

    assert!(store.get("q1").unwrap().is_some(), "get by id");
    assert_eq!(
        store.list_by_channel("chan-q", 10).unwrap().len(),
        2,
        "list by channel"
    );
    assert!(
        store.get_attachment("q1", "f.png").unwrap().is_some(),
        "attachment by cache key"
    );

    // Sender-scoped burn must still select exactly one sender.
    let n = store
        .wipe_wrapped_keys_in_scope("dm", "chan-q", Some("sender-q"))
        .unwrap();
    assert_eq!(
        n, 1,
        "sender-scoped wipe matched {n} rows, expected exactly 1"
    );
    assert!(
        store.get("q1").unwrap().is_none(),
        "burned row still readable"
    );
    assert!(
        store.get("q2").unwrap().is_some(),
        "wipe hit the wrong sender"
    );
}

/// Burn must remain terminal through the blinded schema — the defect-1 fix must
/// not be undone by the migration.
#[test]
fn burn_stays_terminal_after_blinding() {
    let tmp = TempDir::new().unwrap();
    let store = open_a(tmp.path());

    store
        .put(&sample("b1", "chan", "sender", "alice", "secret", 1))
        .unwrap();
    assert!(store.get("b1").unwrap().is_some(), "positive path");
    store.mark_burned("b1").unwrap();
    store
        .put(&sample("b1", "chan", "sender", "alice", "secret", 2))
        .unwrap();

    assert!(
        store.get("b1").unwrap().is_none(),
        "the blind-index migration re-opened the burn resurrection hole"
    );
}

/// Rewriting a row's `chan_bi` must not move the message into another
/// conversation.
///
/// Sealing the metadata with `mid_bi` as AAD authenticates the blob and the
/// message id, but no AEAD covers `chan_bi` or `sender_bi` — they are separate
/// selector columns. Someone who can write the file cannot read a message, but
/// without this check they could retarget one: surface it in a conversation it
/// was never part of, or rewrite `sender_bi` so a sender-scoped burn skips it.
///
/// The attacker does not need to compute a blind index: they can copy one from
/// any row already in the channel they are aiming at, which is exactly what
/// this test does.
#[test]
fn retargeting_a_rows_channel_selector_is_rejected() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.sqlite");
    let store = open_a(tmp.path());

    store
        .put(&sample(
            "secret",
            "private",
            "s1",
            "alice",
            "private business",
            1,
        ))
        .unwrap();
    store
        .put(&sample("decoy", "public", "s2", "bob", "public chatter", 2))
        .unwrap();

    // Positive path: each message starts in its own channel.
    assert_eq!(store.list_by_channel("private", 10).unwrap().len(), 1);
    assert_eq!(store.list_by_channel("public", 10).unwrap().len(), 1);
    drop(store);

    // Steal the public channel's selector and staple it onto the private row.
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let public_chan: Vec<u8> = conn
            .query_row(
                "SELECT chan_bi FROM messages ORDER BY seq DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let n = conn
            .execute(
                "UPDATE messages SET chan_bi = ?1 WHERE seq = 1",
                rusqlite::params![public_chan],
            )
            .unwrap();
        assert_eq!(
            n, 1,
            "positive path: the tamper must have hit exactly one row"
        );
    }

    let store = open_a(tmp.path());
    match store.list_by_channel("public", 10) {
        Err(_) => {}
        Ok(rows) => assert!(
            rows.iter().all(|m| m.discord_message_id != "secret"),
            "a row retargeted on disk was served as belonging to the \
             attacker's channel"
        ),
    }
}
