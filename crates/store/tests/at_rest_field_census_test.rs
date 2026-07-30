use rusqlite::{params, Connection};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use store::{MessageStore, StoredMessage};
use tempfile::TempDir;

const SECRET: &[u8; 32] = b"field-census-secret-32-bytes!!xx";
const BOUNDARY_MESSAGE_STORE_RUNTIME: &str = "message_store_runtime";
const BOUNDARY_SCHEMA_V8_FIELD_CENSUS: &str = "schema_v8_field_census";
const BOUNDARY_PHYSICAL_MEDIA_MAIN_DB: &str = "physical_media_main_db";
const BOUNDARY_PHYSICAL_MEDIA_WAL: &str = "physical_media_wal";
const BOUNDARY_BACKUP_ROLLBACK_COPIES: &str = "backup_rollback_copies";
const CENSUS_BOUNDARY_PROOFS: [&str; 5] = [
    BOUNDARY_MESSAGE_STORE_RUNTIME,
    BOUNDARY_SCHEMA_V8_FIELD_CENSUS,
    BOUNDARY_PHYSICAL_MEDIA_MAIN_DB,
    BOUNDARY_PHYSICAL_MEDIA_WAL,
    BOUNDARY_BACKUP_ROLLBACK_COPIES,
];

fn message(id: &str, channel: &str, sender: &str, osl: &str, body: &str, at: i64) -> StoredMessage {
    StoredMessage {
        discord_message_id: id.to_string(),
        channel_id: channel.to_string(),
        sender_discord_id: sender.to_string(),
        sender_osl_user_id: osl.to_string(),
        plaintext: body.to_string(),
        decrypted_at: at,
        burned: false,
    }
}

fn table_columns(conn: &Connection, table: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .unwrap();
    stmt.query_map([], |row| row.get(1))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn store_files(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap())
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("messages.sqlite")
        })
        .map(|entry| {
            (
                entry.file_name().to_string_lossy().into_owned(),
                fs::read(entry.path()).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

fn assert_absent_from_store_files(dir: &Path, label: &str, needle: &[u8]) {
    let files = store_files(dir);
    assert!(
        !files.is_empty(),
        "raw-file test did not inspect a SQLite file"
    );
    for (name, bytes) in files {
        assert!(
            !contains(&bytes, needle),
            "{label} was recoverable verbatim from {name}"
        );
    }
}

fn read_named_paths(paths: &[PathBuf]) -> Vec<(String, Vec<u8>)> {
    paths
        .iter()
        .map(|path| {
            (
                path.file_name().unwrap().to_string_lossy().into_owned(),
                fs::read(path).unwrap(),
            )
        })
        .collect()
}

fn copy_named_paths(paths: &[PathBuf], destination: &Path) -> Vec<(String, Vec<u8>)> {
    paths
        .iter()
        .map(|path| {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let copy = destination.join(&name);
            fs::copy(path, &copy).unwrap();
            (name, fs::read(copy).unwrap())
        })
        .collect()
}

fn assert_named_artifact_absent(
    boundary: &str,
    artifacts: &[(String, Vec<u8>)],
    artifact_name: &str,
    label: &str,
    needle: &[u8],
) {
    let (_, bytes) = artifacts
        .iter()
        .find(|(name, _)| name == artifact_name)
        .unwrap_or_else(|| panic!("{boundary} did not inspect {artifact_name}"));
    assert!(
        !bytes.is_empty(),
        "{boundary} artifact {artifact_name} is empty"
    );
    assert!(
        !contains(bytes, needle),
        "{label} was recoverable verbatim from {boundary} artifact {artifact_name}"
    );
}

fn assert_artifact_set_absent(
    boundary: &str,
    artifacts: &[(String, Vec<u8>)],
    label: &str,
    needle: &[u8],
) {
    assert!(
        !artifacts.is_empty(),
        "{boundary} artifact scan must inspect at least one file"
    );
    for (name, bytes) in artifacts {
        assert!(!bytes.is_empty(), "{boundary} artifact {name} is empty");
        assert!(
            !contains(bytes, needle),
            "{label} was recoverable verbatim from {boundary} artifact {name}"
        );
    }
}

fn assert_exact_boundary_proof_set(actual: &[&'static str]) {
    let expected = CENSUS_BOUNDARY_PROOFS
        .iter()
        .copied()
        .collect::<HashSet<_>>();
    let actual = actual.iter().copied().collect::<HashSet<_>>();
    assert_eq!(
        actual.len(),
        CENSUS_BOUNDARY_PROOFS.len(),
        "at-rest census closure must keep five distinct named boundary proofs"
    );
    assert_eq!(actual, expected);
}

#[test]
fn schema_v8_field_census_and_raw_wal_plaintext_guards_are_nonvacuous() {
    let tmp = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    let store = MessageStore::open(tmp.path(), SECRET).unwrap();
    let decrypted_at = 0x1122_3344_5566_7788i64;
    let msg = message(
        "msg-census-plaintext-918273645",
        "channel-census-plaintext-827364551",
        "sender-census-plaintext-736455192",
        "service-census-plaintext-645519283",
        "body-census-plaintext-554192837",
        decrypted_at,
    );
    let filename = "filename-census-plaintext-491928374.png";
    let mime = "application/x-census-plaintext-192837465";
    let attachment = b"attachment-census-plaintext-283746551";
    let scope_type = "scope-type-census-plaintext-374655192";
    let scope_id = "scope-id-census-plaintext-465519283";
    store.put(&msg).unwrap();
    store
        .put_attachment(
            &msg.discord_message_id,
            filename,
            mime,
            attachment,
            Some(scope_type),
            Some(scope_id),
            Some(&msg.sender_discord_id),
        )
        .unwrap();

    // Positive controls: the test really persisted and can recover both
    // protected objects. A no-op writer that stored nothing cannot pass merely
    // because every plaintext needle is absent.
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

    let artifacts = store.live_storage_artifacts().unwrap();
    let live_artifacts = read_named_paths(&artifacts);
    let backup_artifacts = copy_named_paths(&artifacts, backup.path());

    assert_exact_boundary_proof_set(&[
        BOUNDARY_MESSAGE_STORE_RUNTIME,
        BOUNDARY_SCHEMA_V8_FIELD_CENSUS,
        BOUNDARY_PHYSICAL_MEDIA_MAIN_DB,
        BOUNDARY_PHYSICAL_MEDIA_WAL,
        BOUNDARY_BACKUP_ROLLBACK_COPIES,
    ]);

    let conn = Connection::open(&artifacts[0]).unwrap();
    let schema_version: Vec<u8> = conn
        .query_row(
            "SELECT value FROM _meta WHERE key='schema_version'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        schema_version,
        8u32.to_le_bytes(),
        "{BOUNDARY_SCHEMA_V8_FIELD_CENSUS} must inspect schema v8"
    );
    assert_eq!(
        table_columns(&conn, "messages"),
        [
            "mid_bi",
            "chan_bi",
            "sender_bi",
            "meta_nonce",
            "meta_ct",
            "ciphertext",
            "nonce",
            "seq",
            "burned",
            "content_version",
            "wrapped_key_nonce",
            "wrapped_key",
            "discord_message_id",
            "channel_id",
            "sender_discord_id",
            "sender_osl_user_id",
            "decrypted_at",
            "scope_type",
            "scope_id",
            "meta_tag",
        ]
    );
    assert_eq!(
        table_columns(&conn, "attachments"),
        [
            "ck_bi",
            "mid_bi",
            "sender_bi",
            "meta_nonce",
            "meta_ct",
            "ciphertext",
            "nonce",
            "seq",
            "burned",
            "content_version",
            "wrapped_key_nonce",
            "wrapped_key",
            "cache_key",
            "discord_message_id",
            "random_filename",
            "mime",
            "byte_len",
            "created_at",
            "scope_type",
            "scope_id",
            "sender_discord_id",
        ]
    );
    assert_eq!(
        table_columns(&conn, "attachment_manifests"),
        ["mid_bi", "complete", "generation", "nonce", "ciphertext"]
    );
    assert_eq!(table_columns(&conn, "_meta"), ["key", "value"]);

    let message_legacy_values: i64 = conn
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
    let attachment_legacy_values: i64 = conn
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
    assert_eq!(message_legacy_values, 0);
    assert_eq!(attachment_legacy_values, 0);

    // Direct identity/no-op encryption mutations are caught by these raw
    // substring checks: message metadata, attachment metadata, and both bodies
    // would become literal SQLite/WAL bytes. This is a targeted mutation guard,
    // not a claim that a finite representation blacklist proves all possible
    // encodings absent.
    for (label, needle) in [
        ("message id", msg.discord_message_id.as_bytes()),
        ("channel id", msg.channel_id.as_bytes()),
        ("sender id", msg.sender_discord_id.as_bytes()),
        ("service id", msg.sender_osl_user_id.as_bytes()),
        ("message body", msg.plaintext.as_bytes()),
        ("attachment filename", filename.as_bytes()),
        ("attachment MIME", mime.as_bytes()),
        ("attachment bytes", &attachment[..]),
        ("attachment scope type", scope_type.as_bytes()),
        ("attachment scope id", scope_id.as_bytes()),
    ] {
        assert_absent_from_store_files(tmp.path(), label, needle);
        assert_named_artifact_absent(
            BOUNDARY_PHYSICAL_MEDIA_MAIN_DB,
            &live_artifacts,
            "messages.sqlite",
            label,
            needle,
        );
        assert_named_artifact_absent(
            BOUNDARY_PHYSICAL_MEDIA_WAL,
            &live_artifacts,
            "messages.sqlite-wal",
            label,
            needle,
        );
        assert_artifact_set_absent(
            BOUNDARY_BACKUP_ROLLBACK_COPIES,
            &backup_artifacts,
            label,
            needle,
        );
    }
    assert_absent_from_store_files(
        tmp.path(),
        "message timestamp encoding",
        &decrypted_at.to_le_bytes(),
    );
    assert_named_artifact_absent(
        BOUNDARY_PHYSICAL_MEDIA_MAIN_DB,
        &live_artifacts,
        "messages.sqlite",
        "message timestamp encoding",
        &decrypted_at.to_le_bytes(),
    );
    assert_named_artifact_absent(
        BOUNDARY_PHYSICAL_MEDIA_WAL,
        &live_artifacts,
        "messages.sqlite-wal",
        "message timestamp encoding",
        &decrypted_at.to_le_bytes(),
    );
    assert_artifact_set_absent(
        BOUNDARY_BACKUP_ROLLBACK_COPIES,
        &backup_artifacts,
        "message timestamp encoding",
        &decrypted_at.to_le_bytes(),
    );

    // The cryptographic columns are populated and cannot be a no-op alias for
    // their protected plaintext inputs.
    let (message_ct, message_wrapper): (Vec<u8>, Vec<u8>) = conn
        .query_row("SELECT ciphertext, wrapped_key FROM messages", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap();
    let (attachment_ct, attachment_wrapper): (Vec<u8>, Vec<u8>) = conn
        .query_row(
            "SELECT ciphertext, wrapped_key FROM attachments",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_ne!(message_ct, msg.plaintext.as_bytes());
    assert_ne!(attachment_ct, attachment);
    assert_ne!(message_wrapper, attachment_wrapper);
}

#[test]
fn v7_burn_timestamp_migration_scrubs_column_and_preserves_reopen_and_burn() {
    let tmp = TempDir::new().unwrap();
    let selected = message(
        "v7-selected-message",
        "v7-channel",
        "v7-selected-sender",
        "v7-selected-service",
        "selected body survives migration",
        10,
    );
    let survivor = message(
        "v7-survivor-message",
        "v7-channel",
        "v7-survivor-sender",
        "v7-survivor-service",
        "survivor body remains exact",
        20,
    );
    {
        let store = MessageStore::open(tmp.path(), SECRET).unwrap();
        store.put(&selected).unwrap();
        store.put(&survivor).unwrap();
        store
            .put_attachment(
                &selected.discord_message_id,
                "selected.bin",
                "application/octet-stream",
                b"selected attachment",
                None,
                None,
                None,
            )
            .unwrap();
        store
            .put_attachment(
                &survivor.discord_message_id,
                "survivor.bin",
                "application/octet-stream",
                b"survivor attachment",
                None,
                None,
                None,
            )
            .unwrap();
    }

    let timestamp = 0x1122_3344_5566_7788i64;
    let timestamp_disk_bytes = timestamp.to_be_bytes();
    {
        let conn = Connection::open(tmp.path().join("messages.sqlite")).unwrap();
        conn.execute_batch(
            "ALTER TABLE messages ADD COLUMN burned_at INTEGER;
             ALTER TABLE attachments ADD COLUMN burned_at INTEGER;",
        )
        .unwrap();
        conn.execute("UPDATE messages SET burned_at=?1", params![timestamp])
            .unwrap();
        conn.execute("UPDATE attachments SET burned_at=?1", params![timestamp])
            .unwrap();
        conn.execute(
            "UPDATE _meta SET value=?1 WHERE key='schema_version'",
            params![7u32.to_le_bytes().to_vec()],
        )
        .unwrap();
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .unwrap();
    }
    assert!(
        store_files(tmp.path())
            .iter()
            .any(|(_, bytes)| contains(bytes, &timestamp_disk_bytes)),
        "migration scrub test lacks a positive raw-file timestamp fixture"
    );

    {
        let store = MessageStore::open(tmp.path(), SECRET).unwrap();
        assert_eq!(
            store.get(&selected.discord_message_id).unwrap(),
            Some(selected)
        );
        assert_eq!(
            store.get(&survivor.discord_message_id).unwrap(),
            Some(survivor.clone())
        );
        store.mark_burned("v7-selected-message").unwrap();
    }
    {
        let reopened = MessageStore::open(tmp.path(), SECRET).unwrap();
        assert_eq!(reopened.get("v7-selected-message").unwrap(), None);
        assert_eq!(reopened.get("v7-survivor-message").unwrap(), Some(survivor));
        assert_eq!(
            reopened
                .get_attachment("v7-survivor-message", "survivor.bin")
                .unwrap()
                .unwrap()
                .1,
            b"survivor attachment"
        );
    }

    let conn = Connection::open(tmp.path().join("messages.sqlite")).unwrap();
    assert!(!table_columns(&conn, "messages")
        .iter()
        .any(|c| c == "burned_at"));
    assert!(!table_columns(&conn, "attachments")
        .iter()
        .any(|c| c == "burned_at"));
    let version: Vec<u8> = conn
        .query_row(
            "SELECT value FROM _meta WHERE key='schema_version'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(version, 8u32.to_le_bytes());
    drop(conn);
    for (name, bytes) in store_files(tmp.path()) {
        assert!(
            !contains(&bytes, &timestamp_disk_bytes),
            "removed burn timestamp remained recoverable in {name}"
        );
    }
}

#[test]
fn v7_to_v8_mid_migration_failure_rolls_back_both_tables_and_retries() {
    let tmp = TempDir::new().unwrap();
    let msg = message(
        "v7-atomic-message",
        "v7-atomic-channel",
        "v7-atomic-sender",
        "v7-atomic-service",
        "atomic migration body",
        30,
    );
    {
        let store = MessageStore::open(tmp.path(), SECRET).unwrap();
        store.put(&msg).unwrap();
        store
            .put_attachment(
                &msg.discord_message_id,
                "atomic.bin",
                "application/octet-stream",
                b"atomic migration attachment",
                None,
                None,
                None,
            )
            .unwrap();
    }
    {
        let conn = Connection::open(tmp.path().join("messages.sqlite")).unwrap();
        conn.execute_batch(
            "ALTER TABLE messages ADD COLUMN burned_at INTEGER;
             ALTER TABLE attachments ADD COLUMN burned_at INTEGER;
             CREATE VIEW block_v8_attachment_drop AS
                 SELECT burned_at FROM attachments;",
        )
        .unwrap();
        conn.execute(
            "UPDATE _meta SET value=?1 WHERE key='schema_version'",
            params![7u32.to_le_bytes().to_vec()],
        )
        .unwrap();
    }

    assert!(
        MessageStore::open(tmp.path(), SECRET).is_err(),
        "trigger must interrupt the second v8 DROP COLUMN"
    );
    {
        let conn = Connection::open(tmp.path().join("messages.sqlite")).unwrap();
        assert!(
            table_columns(&conn, "messages")
                .iter()
                .any(|column| column == "burned_at"),
            "first table mutation escaped the rolled-back v8 transaction"
        );
        assert!(
            table_columns(&conn, "attachments")
                .iter()
                .any(|column| column == "burned_at"),
            "attachment table unexpectedly reached a partial v8 shape"
        );
        let version: Vec<u8> = conn
            .query_row(
                "SELECT value FROM _meta WHERE key='schema_version'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            version,
            7u32.to_le_bytes(),
            "failed migration advanced its version stamp"
        );
        conn.execute_batch("DROP VIEW block_v8_attachment_drop;")
            .unwrap();
    }

    let reopened = MessageStore::open(tmp.path(), SECRET).unwrap();
    assert_eq!(reopened.get(&msg.discord_message_id).unwrap(), Some(msg));
    assert_eq!(
        reopened
            .get_attachment("v7-atomic-message", "atomic.bin")
            .unwrap()
            .unwrap()
            .1,
        b"atomic migration attachment"
    );
    let conn = Connection::open(tmp.path().join("messages.sqlite")).unwrap();
    assert!(!table_columns(&conn, "messages")
        .iter()
        .any(|column| column == "burned_at"));
    assert!(!table_columns(&conn, "attachments")
        .iter()
        .any(|column| column == "burned_at"));
}

#[test]
fn schema_v8_stamp_cannot_hide_a_plaintext_burn_timestamp_column() {
    let tmp = TempDir::new().unwrap();
    let msg = message(
        "v8-shape-message",
        "v8-shape-channel",
        "v8-shape-sender",
        "v8-shape-service",
        "v8 shape positive body",
        40,
    );
    {
        let store = MessageStore::open(tmp.path(), SECRET).unwrap();
        store.put(&msg).unwrap();
        assert_eq!(store.get(&msg.discord_message_id).unwrap(), Some(msg));
    }
    {
        let conn = Connection::open(tmp.path().join("messages.sqlite")).unwrap();
        conn.execute_batch("ALTER TABLE messages ADD COLUMN burned_at INTEGER;")
            .unwrap();
        conn.execute(
            "UPDATE messages SET burned_at=?1",
            params![0x1122_3344_5566_7788i64],
        )
        .unwrap();
    }
    let error = match MessageStore::open(tmp.path(), SECRET) {
        Ok(_) => panic!("schema-v8 stamp bypassed the plaintext-column refusal"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("schema v8 messages table still contains plaintext burned_at"),
        "wrong fail-closed diagnosis: {error}"
    );
}
