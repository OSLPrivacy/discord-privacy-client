//! Exact schema-open path extracted from pre-v4 revision
//! `adff4e45d014d98e5fc51a118b8ba6ef4aa3e8be`.
//!
//! Source provenance:
//! `crates/store/src/schema.rs` at that revision, blob SHA-256
//! `1d0a258a2a29d6c9c40e74c0d48edcd58e34164d7a669560258d6c666e2325f0`.
//!
//! The only adaptation is importing the public `store::StoreError` because
//! this historical source is compiled as an integration-test module. Constants
//! and functions below retain the old ordering: legacy DDL and additive column
//! work execute before `schema_version` is read. That ordering is the behavior
//! the downgrade regression must exercise.

use rusqlite::{params, Connection};
use std::path::Path;
use store::StoreError;

pub(crate) const SCHEMA_VERSION: u32 = 3;

const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS _meta (
    key TEXT PRIMARY KEY,
    value BLOB
);

CREATE TABLE IF NOT EXISTS messages (
    discord_message_id TEXT PRIMARY KEY,
    channel_id TEXT NOT NULL,
    sender_discord_id TEXT NOT NULL,
    sender_osl_user_id TEXT NOT NULL,
    ciphertext BLOB NOT NULL,
    nonce BLOB NOT NULL,
    decrypted_at INTEGER NOT NULL,
    burned INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_messages_channel
    ON messages(channel_id, decrypted_at DESC);
"#;

const SCHEMA_V3: &str = r#"
CREATE TABLE IF NOT EXISTS attachments (
    cache_key TEXT PRIMARY KEY,
    discord_message_id TEXT NOT NULL,
    random_filename TEXT NOT NULL,
    mime TEXT NOT NULL,
    ciphertext BLOB NOT NULL,
    nonce BLOB NOT NULL,
    byte_len INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_attachments_msg
    ON attachments(discord_message_id);

CREATE INDEX IF NOT EXISTS idx_attachments_created
    ON attachments(created_at);
"#;

const V2_COLUMNS: &[&str] = &[
    "ALTER TABLE messages ADD COLUMN burned_at INTEGER",
    "ALTER TABLE messages ADD COLUMN wrapped_key BLOB",
    "ALTER TABLE messages ADD COLUMN scope_type TEXT",
    "ALTER TABLE messages ADD COLUMN scope_id TEXT",
];

const META_AUTH_COLUMNS: &[&str] = &["ALTER TABLE messages ADD COLUMN meta_tag BLOB"];

const ATTACHMENT_SCOPE_COLUMNS: &[&str] = &[
    "ALTER TABLE attachments ADD COLUMN scope_type TEXT",
    "ALTER TABLE attachments ADD COLUMN scope_id TEXT",
    "ALTER TABLE attachments ADD COLUMN sender_discord_id TEXT",
];

/// Exact `MessageStore::open` prefix from adff4e45 through its schema call.
/// A v4 file must return the historical `StoreError::Schema` from `migrate`
/// here, before the old reader reaches canary or row-dependent code.
pub(crate) fn open_schema(app_data_dir: &Path) -> Result<(), StoreError> {
    std::fs::create_dir_all(app_data_dir)?;
    let path = app_data_dir.join("messages.sqlite");
    let conn = Connection::open(&path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "FULL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "secure_delete", "ON")?;
    migrate(&conn)
}

/// This is the exact pre-v4 migration/open order. In particular, do not move
/// `read_meta_u32` ahead of the DDL: doing so would recreate the idealized
/// helper that the independent A8 review rejected.
pub(crate) fn migrate(conn: &Connection) -> Result<(), StoreError> {
    conn.execute_batch(SCHEMA_V1)?;
    apply_v2_columns(conn)?;
    conn.execute_batch(SCHEMA_V3)?;
    apply_attachment_scope_columns(conn)?;
    apply_columns(conn, "messages", META_AUTH_COLUMNS)?;
    let on_disk: Option<u32> = read_meta_u32(conn, "schema_version")?;
    match on_disk {
        None => {
            write_meta_u32(conn, "schema_version", SCHEMA_VERSION)?;
        }
        Some(v) if v == SCHEMA_VERSION => {}
        Some(v) if v < SCHEMA_VERSION => {
            write_meta_u32(conn, "schema_version", SCHEMA_VERSION)?;
        }
        Some(v) => {
            return Err(StoreError::Schema(format!(
                "on-disk schema version {v} is newer than this binary supports \
                 ({SCHEMA_VERSION}); refusing to open"
            )));
        }
    }
    Ok(())
}

fn apply_v2_columns(conn: &Connection) -> Result<(), StoreError> {
    let existing = existing_columns(conn, "messages")?;
    for sql in V2_COLUMNS {
        let after = sql
            .strip_prefix("ALTER TABLE messages ADD COLUMN ")
            .ok_or_else(|| {
                StoreError::Schema(format!("internal: unexpected V2 column SQL shape: {sql}"))
            })?;
        let name = after.split_whitespace().next().ok_or_else(|| {
            StoreError::Schema(format!("internal: cannot parse column name from {sql}"))
        })?;
        if existing.iter().any(|c| c == name) {
            continue;
        }
        conn.execute(sql, [])?;
    }
    Ok(())
}

fn apply_attachment_scope_columns(conn: &Connection) -> Result<(), StoreError> {
    let existing = existing_columns(conn, "attachments")?;
    for sql in ATTACHMENT_SCOPE_COLUMNS {
        let after = sql
            .strip_prefix("ALTER TABLE attachments ADD COLUMN ")
            .ok_or_else(|| {
                StoreError::Schema(format!("internal: unexpected attachment column SQL: {sql}"))
            })?;
        let name = after.split_whitespace().next().ok_or_else(|| {
            StoreError::Schema(format!("internal: cannot parse column name from {sql}"))
        })?;
        if existing.iter().any(|c| c == name) {
            continue;
        }
        conn.execute(sql, [])?;
    }
    Ok(())
}

fn apply_columns(conn: &Connection, table: &str, statements: &[&str]) -> Result<(), StoreError> {
    let existing = existing_columns(conn, table)?;
    let prefix = format!("ALTER TABLE {table} ADD COLUMN ");
    for sql in statements {
        let after = sql.strip_prefix(prefix.as_str()).ok_or_else(|| {
            StoreError::Schema(format!("internal: unexpected column SQL shape: {sql}"))
        })?;
        let name = after.split_whitespace().next().ok_or_else(|| {
            StoreError::Schema(format!("internal: cannot parse column name from {sql}"))
        })?;
        if existing.iter().any(|c| c == name) {
            continue;
        }
        conn.execute(sql, [])?;
    }
    Ok(())
}

fn existing_columns(conn: &Connection, table: &str) -> Result<Vec<String>, StoreError> {
    let sql = format!("PRAGMA table_info({table})");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

fn read_meta_u32(conn: &Connection, key: &str) -> Result<Option<u32>, StoreError> {
    let bytes_opt: Option<Vec<u8>> = read_meta_blob(conn, key)?;
    let Some(bytes) = bytes_opt else {
        return Ok(None);
    };
    if bytes.len() != 4 {
        return Err(StoreError::Schema(format!(
            "_meta[{key}] has length {} (want 4)",
            bytes.len()
        )));
    }
    let mut buf = [0u8; 4];
    buf.copy_from_slice(&bytes);
    Ok(Some(u32::from_le_bytes(buf)))
}

fn write_meta_u32(conn: &Connection, key: &str, val: u32) -> Result<(), StoreError> {
    let bytes = val.to_le_bytes();
    write_meta_blob(conn, key, &bytes)
}

fn read_meta_blob(conn: &Connection, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
    let mut stmt = conn.prepare("SELECT value FROM _meta WHERE key = ?1")?;
    let mut rows = stmt.query(params![key])?;
    if let Some(row) = rows.next()? {
        Ok(Some(row.get(0)?))
    } else {
        Ok(None)
    }
}

fn write_meta_blob(conn: &Connection, key: &str, value: &[u8]) -> Result<(), StoreError> {
    conn.execute(
        "INSERT INTO _meta(key, value) VALUES(?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}
