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
use std::path::{Path, PathBuf};
use store::{MessageStore, StoredMessage};
use tempfile::TempDir;

#[path = "fixtures/adff4e45_schema_reader.rs"]
mod adff4e45_schema_reader;

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
fn raw_store_artifacts(db_path: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let parent = db_path.parent().expect("database has a parent");
    let prefix = db_path
        .file_name()
        .expect("database has a filename")
        .to_string_lossy();
    let mut artifacts = std::fs::read_dir(parent)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().starts_with(prefix.as_ref()))
                .unwrap_or(false)
        })
        .filter(|path| path.is_file())
        .map(|path| {
            let bytes = std::fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect::<Vec<_>>();
    artifacts.sort_by(|left, right| left.0.cmp(&right.0));
    artifacts
}

fn raw_file_bytes(db_path: &Path) -> Vec<u8> {
    raw_store_artifacts(db_path)
        .into_iter()
        .flat_map(|(_, bytes)| bytes)
        .collect()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
}

/// Encodings that preserve a plaintext exactly enough for an offline reader to
/// recover it without the store key.  Searching only the literal sentinel is
/// a false green: a no-op writer that base64s, hexes, UTF-16 encodes, or
/// trivially run-length encodes a body still leaves the body at rest.
///
/// This is intentionally a detector rather than a second encryption scheme.
/// The production assertion below scans the actual SQLite database, WAL, and
/// SHM bytes with every representation.  The companion mutation control feeds
/// it each representation and proves that the detector rejects a reversible
/// identity/no-op persistence change.
fn reversible_plaintext_representations(plaintext: &[u8]) -> Vec<(&'static str, Vec<u8>)> {
    const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    const B64_URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

    fn b64(input: &[u8], alphabet: &[u8; 64], padded: bool) -> Vec<u8> {
        let mut out = Vec::with_capacity(input.len().div_ceil(3) * 4);
        for chunk in input.chunks(3) {
            let a = chunk[0];
            let b = *chunk.get(1).unwrap_or(&0);
            let c = *chunk.get(2).unwrap_or(&0);
            out.push(alphabet[(a >> 2) as usize]);
            out.push(alphabet[(((a & 0x03) << 4) | (b >> 4)) as usize]);
            if chunk.len() > 1 {
                out.push(alphabet[(((b & 0x0f) << 2) | (c >> 6)) as usize]);
            } else if padded {
                out.push(b'=');
            }
            if chunk.len() > 2 {
                out.push(alphabet[(c & 0x3f) as usize]);
            } else if padded {
                out.push(b'=');
            }
        }
        out
    }

    fn hex(input: &[u8], digits: &[u8; 16]) -> Vec<u8> {
        input
            .iter()
            .flat_map(|byte| [digits[(byte >> 4) as usize], digits[(byte & 0x0f) as usize]])
            .collect()
    }

    // A deliberately simple unencrypted compressor.  It is enough to prove
    // the detector is not restricted to text encodings; an implementation
    // that persisted this reversible byte stream would be rejected.
    fn rle(input: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut at = 0;
        while at < input.len() {
            let byte = input[at];
            let mut count = 1u8;
            while at + usize::from(count) < input.len()
                && input[at + usize::from(count)] == byte
                && count < u8::MAX
            {
                count += 1;
            }
            out.extend_from_slice(&[count, byte]);
            at += usize::from(count);
        }
        out
    }

    let mut utf16_le = Vec::with_capacity(plaintext.len() * 2);
    let mut utf16_be = Vec::with_capacity(plaintext.len() * 2);
    for unit in String::from_utf8_lossy(plaintext).encode_utf16() {
        utf16_le.extend_from_slice(&unit.to_le_bytes());
        utf16_be.extend_from_slice(&unit.to_be_bytes());
    }

    vec![
        ("identity/no-op", plaintext.to_vec()),
        ("hex lowercase", hex(plaintext, b"0123456789abcdef")),
        ("hex uppercase", hex(plaintext, b"0123456789ABCDEF")),
        ("base64", b64(plaintext, B64, true)),
        ("base64url unpadded", b64(plaintext, B64_URL, false)),
        ("UTF-16LE", utf16_le),
        ("UTF-16BE", utf16_be),
        ("unencrypted RLE", rle(plaintext)),
    ]
}

fn persisted_plaintext_findings(
    artifacts: &[(PathBuf, Vec<u8>)],
    secrets: &[(&str, &[u8])],
) -> Vec<String> {
    let mut findings = Vec::new();
    for (path, bytes) in artifacts {
        for (secret_label, secret) in secrets {
            for (encoding, representation) in reversible_plaintext_representations(secret) {
                if contains(bytes, &representation) {
                    findings.push(format!(
                        "{secret_label} survived as {encoding} in {}",
                        path.display()
                    ));
                }
            }
        }
    }
    findings
}

fn assert_artifacts_exclude(artifacts: &[(PathBuf, Vec<u8>)], needles: &[(&str, &[u8])]) {
    assert!(!artifacts.is_empty(), "store artifacts must exist");
    for (path, bytes) in artifacts {
        for (label, needle) in needles {
            assert!(
                !contains(bytes, needle),
                "{label} survived in {}",
                path.display()
            );
        }
    }
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

    let needles = [
        ("channel id", channel_id.as_bytes()),
        ("OSL user id", sender_osl.as_bytes()),
        ("message id", msg_id.as_bytes()),
        ("sender Discord id", sender_did.as_bytes()),
        ("attachment filename", filename.as_bytes()),
        ("attachment MIME", mime.as_bytes()),
    ];

    // Keep the connection open for the first sweep. WAL and SHM are live
    // persistence surfaces, and dropping the last connection commonly removes
    // them. A post-close-only test can therefore report success without ever
    // inspecting either file.
    let synthetic_sidecar = tmp.path().join("messages.sqlite-a8-inventory-control");
    std::fs::write(&synthetic_sidecar, b"audited arbitrary sidecar").unwrap();
    let active_artifacts = raw_store_artifacts(&db_path);
    let active_names = active_artifacts
        .iter()
        .map(|(path, _)| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect::<HashSet<_>>();
    for required in [
        "messages.sqlite",
        "messages.sqlite-wal",
        "messages.sqlite-shm",
        "messages.sqlite-a8-inventory-control",
    ] {
        assert!(
            active_names.contains(required),
            "active artifact inventory missed {required}"
        );
    }
    assert_artifacts_exclude(&active_artifacts, &needles);

    drop(store);
    assert_artifacts_exclude(&raw_store_artifacts(&db_path), &needles);
}

/// The representation detector must be capable of failing.  This is a
/// negative mutation control: it supplies the bytes an identity/no-op writer
/// (and several equally reversible encoders) would have persisted, then
/// proves every one is reported.  Without it, the production test below could
/// pass because its detector was accidentally reduced to an empty scan.
#[test]
fn reversible_plaintext_detector_rejects_identity_and_equivalent_mutations() {
    let message = b"A6-message-sentinel::zzzzzzzzzz::no-key-recovery";
    let attachment = b"A6-attachment-sentinel::yyyyyyyy::no-key-recovery";
    let mut mutant_bytes = Vec::new();
    for (_, representation) in reversible_plaintext_representations(message) {
        mutant_bytes.extend_from_slice(&representation);
        mutant_bytes.push(0xff);
    }
    for (_, representation) in reversible_plaintext_representations(attachment) {
        mutant_bytes.extend_from_slice(&representation);
        mutant_bytes.push(0xff);
    }
    let artifacts = vec![(PathBuf::from("messages.sqlite-wal"), mutant_bytes)];
    let findings = persisted_plaintext_findings(
        &artifacts,
        &[("message", message), ("attachment", attachment)],
    );

    let required = [
        "identity/no-op",
        "hex lowercase",
        "hex uppercase",
        "base64",
        "base64url unpadded",
        "UTF-16LE",
        "UTF-16BE",
        "unencrypted RLE",
    ];
    for encoding in required {
        assert!(
            findings.iter().any(|finding| finding.contains(encoding)),
            "detector missed the {encoding} reversible persistence mutation: {findings:?}"
        );
    }
    assert!(
        findings
            .iter()
            .any(|finding| finding.contains("message survived as identity/no-op")),
        "identity/no-op mutation must fail the at-rest detector: {findings:?}"
    );
}

/// A populated store must round-trip exactly across a real close/reopen while
/// none of its persistence surfaces contain a recoverable representation of
/// either body.  This combines the two properties that weak tests split apart:
/// an empty DB can trivially contain no plaintext, and a no-op encoder can
/// round-trip perfectly.  Both messages and attachment bytes are nonempty.
#[test]
fn nonempty_bodies_roundtrip_after_restart_without_reversible_at_rest_leaks() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.sqlite");
    let message = "A6-message-sentinel::zzzzzzzzzz::no-key-recovery";
    let attachment = b"A6-attachment-sentinel::yyyyyyyy::no-key-recovery";
    let msg = sample(
        "a6-message-id",
        "a6-channel-id",
        "a6-sender-id",
        "a6-sender-osl",
        message,
        1_700_123_456,
    );
    let filename = "a6-attachment.bin";
    let mime = "application/a6-test";

    let store = open_a(tmp.path());
    store.put(&msg).unwrap();
    store
        .put_attachment(
            &msg.discord_message_id,
            filename,
            mime,
            attachment,
            Some("dm"),
            Some(&msg.channel_id),
            Some(&msg.sender_discord_id),
        )
        .unwrap();
    assert_eq!(
        store.get(&msg.discord_message_id).unwrap(),
        Some(msg.clone())
    );
    assert_eq!(
        store
            .get_attachment(&msg.discord_message_id, filename)
            .unwrap(),
        Some((mime.to_string(), attachment.to_vec()))
    );
    drop(store);

    // The restart is material: a writer which only holds plaintext in memory
    // or never commits its rows cannot satisfy these exact retrievals.
    let reopened = open_a(tmp.path());
    assert_eq!(
        reopened.get(&msg.discord_message_id).unwrap(),
        Some(msg.clone())
    );
    assert_eq!(
        reopened
            .get_attachment(&msg.discord_message_id, filename)
            .unwrap(),
        Some((mime.to_string(), attachment.to_vec()))
    );

    // Keep the second connection live so all three SQLite persistence
    // surfaces are present and inspected, not silently removed on close.
    let artifacts = raw_store_artifacts(&db_path);
    let names = artifacts
        .iter()
        .map(|(path, _)| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect::<HashSet<_>>();
    for required in [
        "messages.sqlite",
        "messages.sqlite-wal",
        "messages.sqlite-shm",
    ] {
        assert!(
            names.contains(required),
            "nonempty persistence proof did not inspect {required}"
        );
    }
    let findings = persisted_plaintext_findings(
        &artifacts,
        &[
            ("message body", message.as_bytes()),
            ("attachment body", attachment),
        ],
    );
    assert!(
        findings.is_empty(),
        "reversible plaintext-equivalent at-rest leak: {findings:?}"
    );
}

/// `crates/store` has one local database persistence authority: SQLite writing
/// `messages.sqlite`.  The optional monotonic-anchor provider is deliberately
/// external and is not a second local file surface. SQLite may create any
/// same-prefix sidecar; the raw-byte tests sweep all of them, not a
/// hand-maintained extension list.
///
/// This inventory is intentionally store-local. Other product crates own
/// additional persistence surfaces and require their own at-rest proofs; this
/// test must not imply that merely naming those files audits their call sites.
#[test]
fn store_persistence_surface_inventory_is_closed() {
    let mut production_files = std::fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("src"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    production_files.sort();
    assert_eq!(
        production_files,
        ["anchor.rs", "cipher.rs", "error.rs", "lib.rs", "schema.rs"],
        "the production source inventory changed; audit the new file for persistence"
    );

    let sources = [
        ("anchor.rs", include_str!("../src/anchor.rs")),
        ("lib.rs", include_str!("../src/lib.rs")),
        ("schema.rs", include_str!("../src/schema.rs")),
        ("cipher.rs", include_str!("../src/cipher.rs")),
        ("error.rs", include_str!("../src/error.rs")),
    ];
    let connection_sites = sources
        .iter()
        .flat_map(|(name, source)| {
            let production_source = source
                .split_once("\n#[cfg(test)]")
                .map_or(*source, |(production, _)| production);
            production_source
                .lines()
                .filter(|line| line.contains("Connection::open"))
                .map(|text| (*name, text.trim().to_owned()))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        connection_sites,
        vec![
            ("lib.rs", "Ok(Connection::open(path)?)".to_string()),
            ("lib.rs", "Ok(Connection::open(path)?)".to_string()),
        ],
        "a new SQLite persistence surface was added without an A8 privacy proof"
    );

    let filesystem_sites = sources
        .iter()
        .flat_map(|(name, source)| {
            source
                .lines()
                .filter(|line| line.contains("std::fs::"))
                .map(|text| (*name, text.trim().to_owned()))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        filesystem_sites,
        vec![(
            "lib.rs",
            "std::fs::create_dir_all(app_data_dir)?;".to_string()
        )],
        "a new direct filesystem surface was added without an A8 privacy proof"
    );

    let writable_file_apis = [
        "File::create",
        "File::options",
        "OpenOptions",
        "std::fs::write",
        "std::fs::copy",
        "std::io::copy",
        "io::copy",
        ".create_new(",
        ".append(",
        ".write(",
        ".write_all(",
        ".write_vectored(",
        "BufWriter",
        "serde_json::to_writer",
        "serde_cbor::to_writer",
        "ATTACH DATABASE",
        "VACUUM INTO",
    ];
    for (name, source) in sources {
        for api in writable_file_apis {
            assert!(
                !source.contains(api),
                "{name} added persistence through {api}; inventory and prove that surface"
            );
        }
    }
    assert!(
        include_str!("../src/lib.rs")
            .contains("let path = app_data_dir.join(\"messages.sqlite\");"),
        "the inventoried database path disappeared"
    );
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

/// Build the v2-era message table, either with its version stamp or as an
/// installed unstamped legacy database. Both shapes existed before attachments
/// were introduced, and both must take the conservative legacy migration.
fn build_v2_shape_fixture(dir: &Path, rows: &[StoredMessage], stamp: Option<u32>) {
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
    burned INTEGER NOT NULL DEFAULT 0,
    burned_at INTEGER,
    wrapped_key BLOB,
    scope_type TEXT,
    scope_id TEXT
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
    if let Some(version) = stamp {
        conn.execute(
            "INSERT INTO _meta(key, value) VALUES('schema_version', ?1)",
            rusqlite::params![version.to_le_bytes().to_vec()],
        )
        .unwrap();
    }
    for row in rows {
        let (nonce, ct) = v3_seal(row.discord_message_id.as_bytes(), row.plaintext.as_bytes());
        conn.execute(
            "INSERT INTO messages (discord_message_id, channel_id, sender_discord_id, \
                sender_osl_user_id, ciphertext, nonce, decrypted_at, burned, wrapped_key) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8)",
            rusqlite::params![
                row.discord_message_id,
                row.channel_id,
                row.sender_discord_id,
                row.sender_osl_user_id,
                ct,
                nonce,
                row.decrypted_at,
                Option::<Vec<u8>>::None,
            ],
        )
        .unwrap();
    }
}

fn mark_legacy_row_burned_with_body(dir: &Path, id: &str, burned_body: &[u8]) {
    let conn = rusqlite::Connection::open(dir.join("messages.sqlite")).unwrap();
    let changed = conn
        .execute(
            "UPDATE messages \
                SET burned = 1, burned_at = 123456789, ciphertext = ?1, \
                    nonce = ?2, wrapped_key = ?3 \
              WHERE discord_message_id = ?4",
            rusqlite::params![
                burned_body,
                vec![0x6eu8; crypto::aead::NONCE_SIZE],
                vec![0x77u8; 32],
                id,
            ],
        )
        .unwrap();
    assert_eq!(changed, 1, "burned fixture row must exist");
}

fn schema_version(db_path: &Path) -> u32 {
    let conn = rusqlite::Connection::open(db_path).unwrap();
    let raw: Vec<u8> = conn
        .query_row(
            "SELECT value FROM _meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    u32::from_le_bytes(raw.try_into().unwrap())
}

fn assert_legacy_privacy_migration(stamped: bool) {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.sqlite");
    let live = sample(
        "legacy-live-message-998877",
        "legacy-live-channel-776655",
        "legacy-live-sender-554433",
        "legacy-live-osl-user",
        "live body survives",
        10,
    );
    let burned = sample(
        "legacy-burned-message-112233",
        "legacy-burned-channel-223344",
        "legacy-burned-sender-334455",
        "legacy-burned-osl-user",
        "this API body is replaced below",
        20,
    );
    let burned_body = b"BURNED-PLAINTEXT-MUST-NOT-SURVIVE-778899";
    build_v2_shape_fixture(
        tmp.path(),
        &[live.clone(), burned.clone()],
        stamped.then_some(2),
    );
    mark_legacy_row_burned_with_body(tmp.path(), &burned.discord_message_id, burned_body);

    let before = raw_file_bytes(&db_path);
    assert!(contains(&before, live.channel_id.as_bytes()));
    assert!(contains(&before, burned.discord_message_id.as_bytes()));
    assert!(contains(&before, burned_body));

    let store = open_a(tmp.path());
    assert_eq!(
        store.get(&live.discord_message_id).unwrap().unwrap(),
        live,
        "live legacy row must survive the privacy migration"
    );
    assert!(
        store.get(&burned.discord_message_id).unwrap().is_none(),
        "burned legacy row must remain terminal"
    );
    drop(store);

    assert_eq!(schema_version(&db_path), 8);
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let burned_rows: Vec<(Vec<u8>, Vec<u8>, Option<Vec<u8>>, i64)> = {
        let mut stmt = conn
            .prepare("SELECT ciphertext, nonce, wrapped_key, burned FROM messages WHERE burned = 1")
            .unwrap();
        stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .map(|row| row.unwrap())
        .collect()
    };
    assert_eq!(burned_rows.len(), 1, "one burned audit stub must remain");
    let (ct, nonce, wrapped_key, burned_flag) = &burned_rows[0];
    assert_eq!(*burned_flag, 1);
    assert!(
        !ct.is_empty(),
        "positive path: zeroed body retains its length"
    );
    assert!(ct.iter().all(|byte| *byte == 0));
    assert!(
        !nonce.is_empty(),
        "positive path: zeroed nonce retains its length"
    );
    assert!(nonce.iter().all(|byte| *byte == 0));
    assert!(wrapped_key.is_none());
    drop(conn);

    let artifacts = raw_store_artifacts(&db_path);
    assert!(!artifacts.is_empty(), "store artifacts must exist");
    for (path, bytes) in artifacts {
        for (label, needle) in [
            ("live message id", live.discord_message_id.as_bytes()),
            ("live channel id", live.channel_id.as_bytes()),
            ("live sender id", live.sender_discord_id.as_bytes()),
            ("live OSL id", live.sender_osl_user_id.as_bytes()),
            ("burned message id", burned.discord_message_id.as_bytes()),
            ("burned channel id", burned.channel_id.as_bytes()),
            ("burned sender id", burned.sender_discord_id.as_bytes()),
            ("burned OSL id", burned.sender_osl_user_id.as_bytes()),
            ("burned body", burned_body.as_slice()),
        ] {
            assert!(
                !contains(&bytes, needle),
                "{label} survived in {}",
                path.display()
            );
        }
    }
}

#[test]
fn v2_database_migrates_without_identifiers_or_burned_bodies() {
    assert_legacy_privacy_migration(true);
}

#[test]
fn unstamped_legacy_database_migrates_without_identifiers_or_burned_bodies() {
    assert_legacy_privacy_migration(false);
}

#[test]
fn ambiguous_live_legacy_wrapper_refuses_before_any_migration_mutation() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.sqlite");
    let live = sample(
        "legacy-ambiguous-wrapper",
        "legacy-wrapper-channel",
        "legacy-wrapper-sender",
        "legacy-wrapper-osl",
        "legacy wrapper body",
        10,
    );
    build_v2_shape_fixture(tmp.path(), std::slice::from_ref(&live), Some(2));
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    assert_eq!(
        conn.execute(
            "UPDATE messages SET wrapped_key = ?1 WHERE discord_message_id = ?2",
            rusqlite::params![vec![0x5au8; 32], live.discord_message_id],
        )
        .unwrap(),
        1
    );
    drop(conn);

    let before = raw_file_bytes(&db_path);
    let refusal = match MessageStore::open(tmp.path(), SECRET_A) {
        Ok(_) => panic!("an unauthenticated legacy wrapper format was accepted"),
        Err(error) => error,
    };
    assert!(matches!(
        refusal,
        store::StoreError::Schema(ref message)
            if message == "live legacy message has an ambiguous non-NULL wrapped_key; refusing to guess its format"
    ));

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let version: Vec<u8> = conn
        .query_row(
            "SELECT value FROM _meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(u32::from_le_bytes(version.try_into().unwrap()), 2);
    let legacy_columns: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('messages') \
              WHERE name = 'discord_message_id'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(legacy_columns, 1);
    let wrapper: Vec<u8> = conn
        .query_row(
            "SELECT wrapped_key FROM messages WHERE discord_message_id = ?1",
            rusqlite::params![live.discord_message_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(wrapper, vec![0x5au8; 32]);
    drop(conn);
    assert_eq!(
        raw_file_bytes(&db_path),
        before,
        "refusal must not rewrite the legacy database"
    );
}

/// A v1 profile — no v2 columns, no `attachments` table — must migrate straight
/// to the current schema without losing anything.
#[test]
fn v1_database_migrates_all_the_way_to_v8() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.sqlite");
    let rows = vec![
        sample(
            "v1-message-998877",
            "v1-channel-776655",
            "v1-sender-554433",
            "v1-osl-alice-332211",
            "ancient history",
            10,
        ),
        sample(
            "v1-message-887766",
            "v1-channel-776655",
            "v1-sender-443322",
            "v1-osl-bob-221100",
            "also ancient",
            20,
        ),
    ];
    build_v1_fixture(tmp.path(), &rows);

    let store = open_a(tmp.path());

    assert_eq!(
        store.get("v1-message-998877").unwrap().unwrap().plaintext,
        "ancient history",
        "a v1 row did not survive the migration"
    );
    let listed = store.list_by_channel("v1-channel-776655", 10).unwrap();
    assert_eq!(listed.len(), 2, "v1 channel membership was lost");
    assert_eq!(
        listed[0].discord_message_id, "v1-message-887766",
        "v1 migration did not preserve newest-first ordering"
    );

    // And it must land on v8, not stall at an intermediate version.
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let version: Vec<u8> = conn
        .query_row(
            "SELECT value FROM _meta WHERE key = 'schema_version'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(u32::from_le_bytes(version.try_into().unwrap()), 8);
    let (ciphertext, nonce, wrapped_key_nonce, wrapped_key, content_version): (
        Vec<u8>,
        Vec<u8>,
        Option<Vec<u8>>,
        Option<Vec<u8>>,
        i64,
    ) = conn
        .query_row(
            "SELECT ciphertext, nonce, wrapped_key_nonce, wrapped_key, content_version \
               FROM messages ORDER BY seq ASC LIMIT 1",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(content_version, 1);
    assert!(
        wrapped_key_nonce.is_some() && wrapped_key.is_some(),
        "migration must create an authenticated per-record wrapper"
    );
    let nonce_array: [u8; crypto::aead::NONCE_SIZE] =
        nonce.try_into().expect("migrated body nonce length");
    assert!(
        crypto::aead::open(
            &v3_key(),
            &crypto::aead::Nonce::from_bytes(nonce_array),
            b"v1-message-998877",
            &ciphertext,
        )
        .is_err(),
        "a no-op v5 migration left the body directly decryptable by the master key"
    );

    // The store must be usable afterwards, not merely readable.
    store
        .put(&sample(
            "v1-message-current-665544",
            "v1-channel-776655",
            "v1-sender-554433",
            "v1-osl-alice-332211",
            "new message",
            30,
        ))
        .unwrap();
    assert!(store.get("v1-message-current-665544").unwrap().is_some());
    drop(conn);
    drop(store);

    for (path, bytes) in raw_store_artifacts(&db_path) {
        for (label, needle) in [
            ("v1 message id", b"v1-message-998877".as_slice()),
            ("v1 channel id", b"v1-channel-776655".as_slice()),
            ("v1 sender id", b"v1-sender-554433".as_slice()),
            ("v1 OSL id", b"v1-osl-alice-332211".as_slice()),
        ] {
            assert!(
                !contains(&bytes, needle),
                "{label} survived in {}",
                path.display()
            );
        }
    }
}

/// Migration purges cached attachments whose live message row is gone —
/// residue the pre-fix `delete_messages_in_channel` left behind, plus cached
/// bodies belonging to terminal burned audit stubs.
///
/// The negative control is the point: an attachment whose live message still
/// exists must survive. A purge that simply emptied the table would pass the
/// destruction assertions alone.
#[test]
fn migration_purges_orphaned_and_burned_attachments_but_keeps_live_ones() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.sqlite");
    let rows = vec![
        sample("kept-msg", "chan-a", "sender-1", "alice", "still here", 100),
        sample(
            "burned-msg-887766554433",
            "burned-channel-776655443322",
            "burned-sender-665544332211",
            "burned-osl-user-554433221100",
            "replaced below",
            200,
        ),
    ];
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
    add_v3_attachment(
        tmp.path(),
        "burned-msg-887766554433",
        "burned-cache-body-443322110099.png",
        "image/burned-cache-332211009988",
        b"BURNED-CACHED-PLAINTEXT-221100998877",
    );
    let burned_body = b"BURNED-MESSAGE-BODY-110099887766";
    mark_legacy_row_burned_with_body(tmp.path(), "burned-msg-887766554433", burned_body);

    let burned_attachment_ct: Vec<u8> = {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.query_row(
            "SELECT ciphertext FROM attachments \
              WHERE discord_message_id = 'burned-msg-887766554433'",
            [],
            |row| row.get(0),
        )
        .unwrap()
    };

    // Positive path: all three are really in the v3 file before migration.
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM attachments", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            n, 3,
            "positive path: the fixture must hold all three attachments"
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
    assert!(
        store
            .get_attachment(
                "burned-msg-887766554433",
                "burned-cache-body-443322110099.png",
            )
            .unwrap()
            .is_none(),
        "migration retained a decrypted attachment body for a burned message"
    );
    drop(store);

    assert_artifacts_exclude(
        &raw_store_artifacts(&db_path),
        &[
            ("burned message identifier", b"burned-msg-887766554433"),
            ("burned channel identifier", b"burned-channel-776655443322"),
            ("burned sender identifier", b"burned-sender-665544332211"),
            ("burned OSL identifier", b"burned-osl-user-554433221100"),
            ("burned message body", burned_body),
            (
                "burned attachment filename",
                b"burned-cache-body-443322110099.png",
            ),
            ("burned attachment MIME", b"image/burned-cache-332211009988"),
            (
                "burned attachment sealed body",
                burned_attachment_ct.as_slice(),
            ),
        ],
    );
}

/// A cached attachment written by a v3 build must still be decryptable after
/// the migration.
///
/// This test exists because it caught a real data-loss bug during development.
/// The v3→v4 step must open metadata under v3's rules, and the v5→v6 step must
/// open the legacy direct-master body before replacing it with a wrapped DEK.
/// An earlier draft silently changed the legacy attachment AAD and made every
/// migrated attachment unreadable. The message-only fixture never exercised an
/// attachment, so the gap was invisible rather than covered.
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
    let db_path = tmp.path().join("messages.sqlite");
    let (version, wrapper_nonce, wrapper, burned): (i64, Option<Vec<u8>>, Option<Vec<u8>>, i64) =
        rusqlite::Connection::open(&db_path)
            .unwrap()
            .query_row(
                "SELECT content_version, wrapped_key_nonce, wrapped_key, burned \
               FROM attachments",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
    assert_eq!(version, 1);
    assert_eq!(burned, 0);
    assert!(
        wrapper_nonce.is_some() && wrapper.is_some(),
        "a no-op v5→v6 migration left the legacy direct-master attachment body in place"
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
    drop(store);

    let conn = rusqlite::Connection::open(tmp.path().join("messages.sqlite")).unwrap();
    let coverage: Vec<i64> = conn
        .prepare("SELECT complete FROM attachment_manifests ORDER BY mid_bi")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(coverage, vec![0, 0, 0]);
    // This kills an implementation that infers historical emptiness as a
    // complete authoritative inventory during v6→v7 migration.
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

/// Defined downgrade against the exact pre-v4 source from revision adff4e45.
///
/// That reader executes its legacy CREATE INDEX / additive ALTER statements
/// before it reads `schema_version`. The null v4 downgrade-guard columns make
/// those exact old statements idempotent, so the unchanged reader reaches its
/// own explicit newer-version refusal. A handwritten gate that checked the
/// stamp first would miss the real ordering and is deliberately not used.
#[test]
fn exact_adff4e45_reader_reaches_explicit_version_refusal() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.sqlite");
    build_v3_fixture(
        tmp.path(),
        &[sample(
            "downgrade-message-998877",
            "downgrade-channel-887766",
            "downgrade-sender-776655",
            "downgrade-osl-665544",
            "body",
            1,
        )],
    );
    assert!(
        adff4e45_schema_reader::open_schema(tmp.path()).is_ok(),
        "positive path: the exact pre-v4 schema-open path must accept v3"
    );

    let store = open_a(tmp.path());
    assert!(
        store.get("downgrade-message-998877").unwrap().is_some(),
        "current migration must preserve the v3 row"
    );
    drop(store);

    let refusal = adff4e45_schema_reader::open_schema(tmp.path())
        .expect_err("the exact pre-v4 reader opened schema v8");
    match refusal {
        store::StoreError::Schema(message) => assert_eq!(
            message,
            "on-disk schema version 8 is newer than this binary supports (3); refusing to open",
            "the exact reader refused for a reason other than its version gate"
        ),
        other => panic!("exact pre-v4 reader did not reach its version refusal: {other}"),
    }

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let non_null_legacy_message_values: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM messages WHERE \
                discord_message_id IS NOT NULL OR channel_id IS NOT NULL OR \
                sender_discord_id IS NOT NULL OR sender_osl_user_id IS NOT NULL OR \
                decrypted_at IS NOT NULL OR scope_type IS NOT NULL OR \
                scope_id IS NOT NULL OR meta_tag IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        non_null_legacy_message_values, 0,
        "downgrade guards must never hold plaintext message metadata"
    );
    let non_null_legacy_attachment_values: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM attachments WHERE \
                cache_key IS NOT NULL OR discord_message_id IS NOT NULL OR \
                random_filename IS NOT NULL OR mime IS NOT NULL OR \
                byte_len IS NOT NULL OR created_at IS NOT NULL OR \
                scope_type IS NOT NULL OR scope_id IS NOT NULL OR \
                sender_discord_id IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        non_null_legacy_attachment_values, 0,
        "downgrade guards must never hold plaintext attachment metadata"
    );
    drop(conn);

    assert_artifacts_exclude(
        &raw_store_artifacts(&db_path),
        &[
            ("downgrade message id", b"downgrade-message-998877"),
            ("downgrade channel id", b"downgrade-channel-887766"),
            ("downgrade sender id", b"downgrade-sender-776655"),
            ("downgrade OSL id", b"downgrade-osl-665544"),
        ],
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
    let db_path = tmp.path().join("messages.sqlite");
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

    // A hidden row is not sufficient evidence of destruction: an
    // implementation could set only `burned` (or clear the legacy
    // always-NULL `wrapped_key`) and leave a decryptable body on disk. Check
    // the two selected rows directly. This is deliberately a two-row fixture:
    // a blanket zeroing implementation must also fail because q2 survives.
    let (zeroed, live): (usize, usize) = {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let mut stmt = conn
            .prepare("SELECT ciphertext, nonce, burned FROM messages")
            .unwrap();
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .unwrap();
        let mut zeroed = 0;
        let mut live = 0;
        for row in rows {
            let (ciphertext, nonce, burned) = row.unwrap();
            if burned == 1
                && !ciphertext.is_empty()
                && !nonce.is_empty()
                && ciphertext.iter().all(|byte| *byte == 0)
                && nonce.iter().all(|byte| *byte == 0)
            {
                zeroed += 1;
            }
            if burned == 0
                && ciphertext.iter().any(|byte| *byte != 0)
                && nonce.iter().any(|byte| *byte != 0)
            {
                live += 1;
            }
        }
        (zeroed, live)
    };
    assert_eq!(zeroed, 1, "scope burn must zero the selected sealed body");
    assert_eq!(live, 1, "scope burn must preserve the other sender's body");
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
    assert!(
        matches!(
            store.list_by_channel("public", 10),
            Err(store::StoreError::Corrupted(_))
        ),
        "retargeted selector must produce Corrupted, not an unrelated failure \
         or a hidden row"
    );
}

fn assert_attachment_selector_tamper_is_rejected(column: &str) {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.sqlite");
    let store = open_a(tmp.path());
    store
        .put(&sample(
            "attachment-parent",
            "private-channel",
            "sender-one",
            "alice",
            "body",
            1,
        ))
        .unwrap();
    store
        .put_attachment(
            "attachment-parent",
            "private.png",
            "image/png",
            b"PRIVATE-PIXELS",
            Some("dm"),
            Some("private-channel"),
            Some("sender-one"),
        )
        .unwrap();
    assert_eq!(
        store
            .get_attachment("attachment-parent", "private.png")
            .unwrap()
            .unwrap()
            .1,
        b"PRIVATE-PIXELS",
        "positive path: honest selectors must read"
    );
    drop(store);

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let sql = format!("UPDATE attachments SET {column} = zeroblob(length({column}))");
    assert_eq!(conn.execute(&sql, []).unwrap(), 1);
    drop(conn);

    match MessageStore::open(tmp.path(), SECRET_A) {
        Err(store::StoreError::Corrupted(_)) => {}
        Ok(store) => assert!(
            matches!(
                store.get_attachment("attachment-parent", "private.png"),
                Err(store::StoreError::Corrupted(_))
            ),
            "attachment {column} was not cross-checked against sealed metadata"
        ),
        Err(other) => panic!("selector tamper returned unrelated error: {other}"),
    }
}

/// Every attachment selector that drives a read or a later destruction is
/// recomputed from the authenticated metadata before bytes are returned.
#[test]
fn attachment_blind_selectors_match_decrypted_selected_row_metadata() {
    assert_attachment_selector_tamper_is_rejected("mid_bi");
    assert_attachment_selector_tamper_is_rejected("sender_bi");
}

/// Deleting the canary must not turn a populated store into one that opens
/// under any secret.
///
/// `check_canary` treats a missing canary as first-run. Applied
/// unconditionally that means anyone able to delete two `_meta` rows can open
/// the database with a secret of their choosing — and because `migrate` runs
/// next, a legacy file would then be rewritten with its metadata sealed under
/// the attacker's key while its message bodies stay under the owner's,
/// committing a store neither secret can fully open.
#[test]
fn a_populated_store_with_its_canary_deleted_refuses_to_open() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.sqlite");
    let store = open_a(tmp.path());
    store
        .put(&sample("m", "chan", "s", "alice", "secret body", 1))
        .unwrap();
    assert!(store.get("m").unwrap().is_some(), "positive path");
    drop(store);

    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let n = conn
            .execute(
                "DELETE FROM _meta WHERE key IN ('canary_nonce','canary_ct')",
                [],
            )
            .unwrap();
        assert_eq!(
            n, 2,
            "positive path: the canary must have been there to delete"
        );
    }

    // A missing canary on a populated store is a fail-closed `Sealer` refusal,
    // regardless of which secret is supplied. `is_err()` would let an
    // unrelated I/O/SQL failure prove this test without exercising the
    // refusal classification.
    for secret in [&[9u8; 32], SECRET_A] {
        assert!(
            matches!(
                MessageStore::open(tmp.path(), secret),
                Err(store::StoreError::Sealer(_))
            ),
            "a populated store with a deleted canary must refuse as a canary \
             failure rather than opening or returning an unrelated error"
        );
    }
}

/// The message body itself must not be readable in the file.
///
/// Every other sweep in this file hunts for *identifiers*. Nothing asserted the
/// thing the store exists to protect — the message text — is actually encrypted
/// at rest. Without this, an implementation that stored the body verbatim, or
/// stored it alongside its ciphertext, would pass the entire suite: the
/// round-trip test returns what it was given either way.
///
/// The attachment bytes are checked for the same reason.
#[test]
fn message_bodies_and_attachment_bytes_are_not_readable_in_the_file() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.sqlite");
    let message_a = sample(
        "mb1",
        "chan-a",
        "sender-a",
        "alice",
        "SENTINEL-BODY-the-quick-brown-fox-said-something-private-A",
        1_700_000_000,
    );
    let message_b = sample(
        "mb2",
        "chan-b",
        "sender-b",
        "bob",
        "SENTINEL-BODY-the-quick-brown-fox-said-something-private-B",
        1_700_000_001,
    );
    let attachment_a = b"SENTINEL-ATTACHMENT-BYTES-not-a-real-png-A";
    let attachment_b = b"SENTINEL-ATTACHMENT-BYTES-not-a-real-png-B";

    {
        let store = open_a(tmp.path());
        store.put(&message_a).unwrap();
        store.put(&message_b).unwrap();
        store
            .put_attachment("mb1", "a.png", "image/png", attachment_a, None, None, None)
            .unwrap();
        store
            .put_attachment("mb2", "b.png", "image/png", attachment_b, None, None, None)
            .unwrap();

        // Positive path: the store must genuinely be holding distinct records
        // before we claim they disappear from the database files.
        assert_eq!(
            store.get("mb1").unwrap().unwrap(),
            message_a,
            "positive path: the first body must round-trip before we claim it is hidden"
        );
        assert_eq!(
            store.get("mb2").unwrap().unwrap(),
            message_b,
            "positive path: the second body must round-trip before we claim it is hidden"
        );
        assert_eq!(
            store.get_attachment("mb1", "a.png").unwrap().unwrap().1,
            attachment_a,
            "positive path: the first attachment must round-trip first"
        );
        assert_eq!(
            store.get_attachment("mb2", "b.png").unwrap().unwrap().1,
            attachment_b,
            "positive path: the second attachment must round-trip first"
        );
    }

    // Restart the store before inspecting the persisted files, so this proof
    // covers the reopen path and the live SQLite sidecars the reopened
    // connection keeps around.
    let store = open_a(tmp.path());
    assert_eq!(
        store.get("mb1").unwrap().unwrap(),
        message_a,
        "restart path: the first body must still round-trip after reopen"
    );
    assert_eq!(
        store.get("mb2").unwrap().unwrap(),
        message_b,
        "restart path: the second body must still round-trip after reopen"
    );
    assert_eq!(
        store.get_attachment("mb1", "a.png").unwrap().unwrap().1,
        attachment_a,
        "restart path: the first attachment must still round-trip after reopen"
    );
    assert_eq!(
        store.get_attachment("mb2", "b.png").unwrap().unwrap().1,
        attachment_b,
        "restart path: the second attachment must still round-trip after reopen"
    );

    let artifacts = raw_store_artifacts(&db_path);
    assert_artifacts_exclude(
        &artifacts,
        &[
            ("message body 1", message_a.plaintext.as_bytes()),
            ("message body 2", message_b.plaintext.as_bytes()),
            ("attachment bytes 1", attachment_a),
            ("attachment bytes 2", attachment_b),
        ],
    );
}

/// An attachment's sealed metadata must not be transplantable into a message
/// row.
///
/// The two encodings begin identically — four length-prefixed strings then an
/// `i64` — and the AEAD only ever sees opaque bytes. Copying an attachment's
/// `ck_bi` into a message row's `mid_bi`, along with its metadata and body
/// blobs, therefore produced a row whose AEAD verified and whose fields were
/// reinterpreted: cache key read as a message id, MIME read as an OSL
/// identity. A UTF-8 attachment could surface as an authenticated message in an
/// attacker-chosen conversation.
#[test]
fn attachment_metadata_cannot_be_transplanted_into_a_message_row() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.sqlite");
    let store = open_a(tmp.path());

    store
        .put(&sample(
            "real",
            "target",
            "s1",
            "alice",
            "genuine message",
            1,
        ))
        .unwrap();
    store
        .put_attachment(
            "real",
            "note.txt",
            "text/plain",
            b"attacker text",
            None,
            None,
            None,
        )
        .unwrap();
    assert_eq!(
        store.list_by_channel("target", 10).unwrap().len(),
        1,
        "positive path: the channel holds exactly its one real message"
    );
    drop(store);

    // Forge a message row entirely out of the attachment's authenticated parts,
    // aimed at the real channel.
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let n = conn
            .execute(
                "INSERT INTO messages \
                    (mid_bi, chan_bi, sender_bi, meta_nonce, meta_ct, ciphertext, nonce, seq, burned) \
                 SELECT a.ck_bi, m.chan_bi, m.sender_bi, a.meta_nonce, a.meta_ct, \
                        a.ciphertext, a.nonce, 9999, 0 \
                   FROM attachments a, messages m",
                [],
            )
            .unwrap();
        assert_eq!(
            n, 1,
            "positive path: the forged row must have been inserted"
        );
    }

    match MessageStore::open(tmp.path(), SECRET_A) {
        Err(store::StoreError::Corrupted(_)) => {}
        Ok(store) => assert!(
            matches!(
                store.list_by_channel("target", 10),
                Err(store::StoreError::Corrupted(_))
            ),
            "attachment metadata transplanted into a message row must produce \
             Corrupted, not an unrelated error or a hidden row"
        ),
        Err(other) => panic!("type transplant returned unrelated error: {other}"),
    }
}

/// Two different (message id, filename) pairs must not collapse to one cached
/// attachment.
///
/// The cache key is `"{discord_message_id}/{random_filename}"`, so id `a/b`
/// with filename `c` and id `a` with filename `b/c` both produce `a/b/c` —
/// one key, one blind index, one row. Without validation the second write
/// silently overwrites the first, and one message's cached attachment is served
/// for another's. Both values arrive unvalidated from the IPC boundary, so
/// nothing upstream enforces the digits-only snowflake shape that makes this
/// unreachable in practice.
#[test]
fn ambiguous_cache_key_components_are_refused() {
    let tmp = TempDir::new().unwrap();
    let store = open_a(tmp.path());

    // Positive path: an ordinary pair is accepted and round-trips, so the
    // refusals below cannot pass because attachments never work at all.
    store
        .put_attachment("111", "ok.png", "image/png", b"FINE", None, None, None)
        .unwrap();
    assert_eq!(
        store.get_attachment("111", "ok.png").unwrap().unwrap().1,
        b"FINE"
    );

    // The colliding pair: both would key on "a/b/c".
    assert!(
        store
            .put_attachment("a/b", "c", "image/png", b"FIRST", None, None, None)
            .is_err(),
        "a message id containing '/' was accepted, making the cache key ambiguous"
    );
    assert!(
        store
            .put_attachment("a", "b/c", "image/png", b"SECOND", None, None, None)
            .is_err(),
        "a filename containing '/' was accepted, making the cache key ambiguous"
    );
    // Reads are refused on the same rule, so a lookup cannot reach a row a
    // write was not allowed to create.
    assert!(store.get_attachment("a/b", "c").is_err());
    assert!(store.get_attachment("a", "b/c").is_err());

    // A message id containing '/' would also collide with an attachment cache
    // key in the body AAD namespace.
    let mut bad = sample("a/b", "chan", "s", "alice", "body", 1);
    bad.discord_message_id = "a/b".to_string();
    assert!(
        store.put(&bad).is_err(),
        "a message id containing '/' was stored"
    );
}

/// A migration that cannot complete must leave the original database intact.
///
/// The v3→v4 rewrite runs inside one transaction, but nothing exercised the
/// failure arm: every test migrated a well-formed fixture. This one forces the
/// abort by handing the migration two rows that map to the same blind index —
/// constructed by building the legacy table *without* its primary key so the
/// duplicate can exist at all.
///
/// The property under test is not the duplicate. It is that a failed migration
/// rolls back: the user's original rows must still be there afterwards, because
/// the alternative is a half-converted database and a lost history.
#[test]
fn a_failed_migration_rolls_back_and_leaves_the_legacy_data_intact() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.sqlite");

    let (canary_nonce, canary_ct) =
        v3_seal(b"osl-message-store/canary", b"osl-message-store-canary-v1");
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    // Deliberately no PRIMARY KEY, so two rows can share an identifier.
    conn.execute_batch(
        r#"
CREATE TABLE _meta (key TEXT PRIMARY KEY, value BLOB);
CREATE TABLE messages (
    discord_message_id TEXT NOT NULL,
    channel_id TEXT NOT NULL,
    sender_discord_id TEXT NOT NULL,
    sender_osl_user_id TEXT NOT NULL,
    ciphertext BLOB NOT NULL,
    nonce BLOB NOT NULL,
    decrypted_at INTEGER NOT NULL,
    burned INTEGER NOT NULL DEFAULT 0
);
"#,
    )
    .unwrap();
    for (k, v) in [("canary_nonce", canary_nonce), ("canary_ct", canary_ct)] {
        conn.execute(
            "INSERT INTO _meta(key, value) VALUES(?1, ?2)",
            rusqlite::params![k, v],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO _meta(key, value) VALUES('schema_version', ?1)",
        rusqlite::params![3u32.to_le_bytes().to_vec()],
    )
    .unwrap();
    for body in ["first copy", "second copy"] {
        let (nonce, ct) = v3_seal(b"dup-id", body.as_bytes());
        conn.execute(
            "INSERT INTO messages (discord_message_id, channel_id, sender_discord_id, \
                sender_osl_user_id, ciphertext, nonce, decrypted_at, burned) \
             VALUES ('dup-id', 'chan', 's', 'alice', ?1, ?2, 1, 0)",
            rusqlite::params![ct, nonce],
        )
        .unwrap();
    }
    drop(conn);

    // Positive path: the legacy rows really are there before the attempt.
    let before: i64 = {
        let c = rusqlite::Connection::open(&db_path).unwrap();
        c.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))
            .unwrap()
    };
    assert_eq!(before, 2, "positive path: the fixture must hold both rows");

    // The migration must refuse rather than silently drop one of them.
    assert!(
        MessageStore::open(tmp.path(), SECRET_A).is_err(),
        "a migration that cannot represent both rows reported success"
    );

    // And the original data must survive the failure.
    let c = rusqlite::Connection::open(&db_path).unwrap();
    let cols: Vec<String> = {
        let mut stmt = c.prepare("PRAGMA table_info(messages)").unwrap();
        let rows = stmt.query_map([], |r| r.get::<_, String>(1)).unwrap();
        rows.map(|r| r.unwrap()).collect()
    };
    assert!(
        cols.iter().any(|col| col == "discord_message_id"),
        "rollback did not restore the legacy table shape"
    );
    let after: i64 = c
        .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        after, before,
        "a failed migration destroyed rows it could not convert"
    );
}
