//! On-disk schema + migration framework for the message store.
//!
//! ## `_meta`
//!
//! `(key, value)` blob table holding:
//!
//!   - `schema_version` — `u32` little-endian, bumped on every
//!     backward-incompatible change. Migrations between
//!     versions route through [`migrate`].
//!   - `canary_nonce` / `canary_ct` — fixed plaintext sealed at
//!     first init under the HKDF-derived data key. On reopen we
//!     attempt to unseal; failure means the caller's
//!     `identity_secret` does not match the secret that
//!     initialised the store.
//!
//! ## v1 schema (current)
//!
//! - `messages` — per-message row, `ciphertext` is sealed
//!   plaintext, `nonce` is the per-row XChaCha20-Poly1305 nonce.
//!
//! Plaintext is **never** persisted on disk in any form,
//! including tokenized. v1 deliberately ships without search;
//! v1.5 will add a decrypt-and-scan path that holds plaintext
//! only in memory. See `SECURITY.md` § "Search".

use crate::cipher;
use crate::StoreError;
use crypto::aead;
use rusqlite::{params, Connection};

/// Current schema version. Bumped on every backward-incompatible
/// change; migrations between versions are dispatched in
/// [`migrate`].
///
/// History:
///   v1 — initial schema (Phase 5b1). messages(id, channel,
///        sender_*, ciphertext, nonce, decrypted_at, burned).
///   v2 — Phase 7a. Adds burned_at, wrapped_key, scope_type,
///        scope_id columns (all NULL on existing rows). Driven
///        by the per-message ephemeral-key + scoped burn model
///        in `docs/phase-7-design.md` §§ 3, 5.4.
///   v3 — Beta 1.0. Adds the `attachments` table so decrypted
///        image/file bytes survive a restart (sealed at rest, same
///        as message plaintext) instead of being re-fetched +
///        re-decrypted from Discord's CDN on every channel re-entry.
pub(crate) const SCHEMA_VERSION: u32 = 3;

/// Fixed canary plaintext. Hard-coded so a wrong-key unseal that
/// happens to produce non-error garbage still fails the
/// post-unseal byte-equality check.
pub(crate) const CANARY_PLAINTEXT: &[u8] = b"osl-message-store-canary-v1";

/// AAD bound to the canary so it can't be replayed against the
/// row-keyed AAD scheme used by `messages`.
pub(crate) const CANARY_AAD: &[u8] = b"osl-message-store/canary";

/// SQL for creating the v1 schema. All `CREATE`s are
/// `IF NOT EXISTS` so re-running on an already-initialised store
/// is a no-op. v=2 columns are added as a separate ALTER pass in
/// [`apply_v2_columns`] so they layer on top of an existing v=1 DB
/// without rewriting it.
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

/// Schema v=3 (Beta 1.0): the `attachments` table. `ciphertext` is
/// the sealed decrypted attachment bytes (AAD = `cache_key`), `nonce`
/// the per-row XChaCha20-Poly1305 nonce. `cache_key` is
/// `"<discord_message_id>/<random_filename>"`, unique per attachment.
/// Created `IF NOT EXISTS` so it layers onto an existing v=2 DB
/// without a rewrite.
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

/// Phase 7a column additions (schema v=2). Each is appended via a
/// separate `ALTER TABLE ADD COLUMN` so the migration is purely
/// additive — existing rows pick up NULL/default values and the
/// v=1 read path continues to work against the same table.
///
/// SQLite's `ALTER TABLE ADD COLUMN` is idempotent only via a
/// "does this column already exist?" precheck, which we do in
/// [`apply_v2_columns`] using `PRAGMA table_info`.
///
/// - `burned_at` (INTEGER, nullable) — unix-seconds timestamp at
///   which `mark_burned` was called. Lets the UI distinguish
///   "burned 5 minutes ago" from "burned last week" and supports
///   future audit/recovery views.
/// - `wrapped_key` (BLOB, nullable) — the per-recipient wrapped
///   `K` from the v=2 wire format (see `wire_v2`). Stored so a
///   re-decrypt of the row's `ciphertext` is possible after
///   identity-rotation events that would otherwise lose K.
/// - `scope_type` (TEXT, nullable) — one of 'dm', 'gc',
///   'server_channel', 'server_full'. Lets scope-burn queries
///   pick out the affected rows in a single WHERE clause.
/// - `scope_id` (TEXT, nullable) — channel/GC/server id matching
///   `scope_type`. Same query-filter rationale.
const V2_COLUMNS: &[&str] = &[
    "ALTER TABLE messages ADD COLUMN burned_at INTEGER",
    "ALTER TABLE messages ADD COLUMN wrapped_key BLOB",
    "ALTER TABLE messages ADD COLUMN scope_type TEXT",
    "ALTER TABLE messages ADD COLUMN scope_id TEXT",
];

/// Burn follow-up: scope + sender on the attachment cache. The burn
/// open-gate already refuses to RE-decrypt a burned sender's
/// attachment, but a cache hit serves already-decrypted bytes without
/// hitting that gate — so a burn must also wipe the cached rows. These
/// columns let `wipe_attachments_in_scope` delete exactly the burner's
/// cached attachments in a scope. Additive + pre-checked like V2; old
/// rows get NULL (they predate the fix and won't match a scope wipe,
/// which is acceptable).
/// Row-metadata authentication (audit finding: security-relevant metadata sat
/// outside AEAD authentication).
///
/// `meta_tag` (BLOB, nullable) holds a 40-byte `nonce || tag` authenticator
/// over the row's `discord_message_id`, `channel_id`, `sender_discord_id`,
/// `sender_osl_user_id` and `decrypted_at`. It carries no secret, so it adds
/// nothing to what the file discloses.
///
/// ## Why this is an additive ALTER and does **not** bump `SCHEMA_VERSION`
///
/// Bumping the version makes an older binary refuse to open the database
/// (`migrate` returns `Schema` for any on-disk version it does not know), which
/// would orphan an installed user's entire history the moment they rolled back
/// — the same hazard `shred_expired_messages` is documented to avoid. A
/// nullable column that an older binary simply never selects costs that binary
/// nothing, so upgrade and downgrade both keep every row readable.
const META_AUTH_COLUMNS: &[&str] = &["ALTER TABLE messages ADD COLUMN meta_tag BLOB"];

/// `_meta` key holding the sealed marker that latches strict metadata
/// authentication on.
const META_AUTH_STRICT_KEY: &str = "meta_auth_strict";

/// AAD for the strict marker, domain-separated from the canary.
const META_AUTH_STRICT_AAD: &[u8] = b"osl-message-store/meta-auth-strict";

const ATTACHMENT_SCOPE_COLUMNS: &[&str] = &[
    "ALTER TABLE attachments ADD COLUMN scope_type TEXT",
    "ALTER TABLE attachments ADD COLUMN scope_id TEXT",
    "ALTER TABLE attachments ADD COLUMN sender_discord_id TEXT",
];

/// Apply the schema and resolve the on-disk version.
///
/// On a fresh DB: runs the v1 schema, writes
/// `schema_version = 1`. On a v1 DB: idempotent — re-runs the
/// `CREATE IF NOT EXISTS` block, leaves the version intact. On a
/// future-version DB: returns [`StoreError::Schema`] rather than
/// risking forward-incompat. Migration dispatch for v2+ goes
/// here when the schema changes.
pub(crate) fn migrate(conn: &Connection) -> Result<(), StoreError> {
    conn.execute_batch(SCHEMA_V1)?;
    // Apply v=2 columns unconditionally; idempotent under
    // `apply_v2_columns`'s pre-check. A fresh-init run lands on
    // v=2 directly because we stamp `schema_version = SCHEMA_VERSION`
    // below.
    apply_v2_columns(conn)?;
    // v=3: the attachments table. CREATE IF NOT EXISTS is idempotent
    // so this is safe to run on a fresh DB and on an existing v=2 DB.
    conn.execute_batch(SCHEMA_V3)?;
    // Burn follow-up: scope/sender columns on the attachment cache.
    // Pre-checked additive ALTERs (idempotent), like the v=2 columns.
    apply_attachment_scope_columns(conn)?;
    // Row-metadata authenticator. Additive and version-neutral on purpose —
    // see META_AUTH_COLUMNS.
    apply_columns(conn, "messages", META_AUTH_COLUMNS)?;
    let on_disk: Option<u32> = read_meta_u32(conn, "schema_version")?;
    match on_disk {
        None => {
            // First-ever open with this DB file. Stamp the version.
            write_meta_u32(conn, "schema_version", SCHEMA_VERSION)?;
        }
        Some(v) if v == SCHEMA_VERSION => {
            // Already current.
        }
        Some(v) if v < SCHEMA_VERSION => {
            // v1 → v2 columns were applied above. Stamp the new
            // version. (For v=3+, dispatch per-step migrations
            // here.)
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

/// Apply v=2 column additions to the `messages` table. Each
/// column is wrapped in a `PRAGMA table_info` pre-check so re-runs
/// are idempotent — SQLite's `ALTER TABLE ADD COLUMN` errors if
/// the column already exists, unlike `CREATE IF NOT EXISTS`.
fn apply_v2_columns(conn: &Connection) -> Result<(), StoreError> {
    let existing = existing_columns(conn, "messages")?;
    for sql in V2_COLUMNS {
        // Parse "ALTER TABLE messages ADD COLUMN <name> <type>"
        // to recover the column name. Stable string layout above
        // makes this safe.
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

/// Idempotent additive ALTERs for the attachment-cache scope/sender
/// columns, pre-checked via `PRAGMA table_info` like [`apply_v2_columns`].
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

/// Apply a list of `ALTER TABLE <table> ADD COLUMN <name> <type>` statements,
/// skipping any column that already exists.
///
/// SQLite has no `ADD COLUMN IF NOT EXISTS`, so idempotence comes from a
/// `PRAGMA table_info` pre-check. The column name is recovered from the
/// statement text, which is safe because every statement is a hard-coded
/// constant in this file.
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

/// Decide whether this store may refuse rows that carry no metadata
/// authenticator.
///
/// A store that still holds rows written before the authenticator existed must
/// keep serving them, or upgrading would erase the user's history from their
/// own client. So the rule latches instead of switching:
///
/// - Once the database contains **no** untagged rows, seal a marker.
/// - While the marker is present, an untagged row is refused as tampering.
///
/// A database created by this build is strict from its first open, because an
/// empty table trivially has no untagged rows. An upgraded database becomes
/// strict once its legacy rows have been rewritten or burned away.
///
/// The marker is sealed under the store key so it cannot be forged by someone
/// who can write the file but does not hold the key. It can still be *deleted*
/// by such a writer, which downgrades this store to lenient — `_meta` is no
/// better protected than the rest of the file. Closing that gap needs a
/// whole-database MAC anchored outside SQLite; it is written up in
/// `docs/reports/store-lane-2026-07-26.md` rather than half-built here.
pub(crate) fn resolve_meta_auth_strict(
    conn: &Connection,
    key: &aead::Key,
) -> Result<bool, StoreError> {
    if let Some(blob) = read_meta_blob(conn, META_AUTH_STRICT_KEY)? {
        if blob.len() <= aead::NONCE_SIZE {
            return Err(StoreError::Schema(
                "meta_auth_strict marker is truncated".to_string(),
            ));
        }
        let (nonce, ct) = blob.split_at(aead::NONCE_SIZE);
        cipher::unseal_canary(key, META_AUTH_STRICT_AAD, nonce, ct)?;
        return Ok(true);
    }

    // Burned rows are excluded deliberately. A burned row is an audit stub:
    // its body is zeroed and both read paths filter `burned = 0`, so its
    // metadata is never authenticated against anything and never materialised.
    // Counting them would mean a store that ever held legacy history could
    // never latch strict, because burned stubs persist forever.
    let untagged: i64 = conn.query_row(
        "SELECT COUNT(*) FROM messages WHERE meta_tag IS NULL AND burned = 0",
        [],
        |r| r.get(0),
    )?;
    if untagged > 0 {
        return Ok(false);
    }

    let (nonce, ct) = cipher::seal(key, META_AUTH_STRICT_AAD, b"1")?;
    let mut marker = nonce;
    marker.extend_from_slice(&ct);
    write_meta_blob(conn, META_AUTH_STRICT_KEY, &marker)?;
    Ok(true)
}

/// Return the set of column names on a given table via
/// `PRAGMA table_info`. SQLite returns an empty result for a
/// missing table; we surface that as an empty Vec.
fn existing_columns(conn: &Connection, table: &str) -> Result<Vec<String>, StoreError> {
    // `pragma_query` would be cleaner but rusqlite 0.32 doesn't
    // accept dynamic table names through that API without
    // table-name escaping. Direct prepare is fine because `table`
    // is a hard-coded internal constant.
    let sql = format!("PRAGMA table_info({table})");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Read a `u32` little-endian value from `_meta` by key.
pub(crate) fn read_meta_u32(conn: &Connection, key: &str) -> Result<Option<u32>, StoreError> {
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

/// Insert-or-replace a `u32` little-endian value at `_meta[key]`.
pub(crate) fn write_meta_u32(conn: &Connection, key: &str, val: u32) -> Result<(), StoreError> {
    let bytes = val.to_le_bytes();
    write_meta_blob(conn, key, &bytes)
}

/// Read an opaque blob from `_meta` by key.
pub(crate) fn read_meta_blob(conn: &Connection, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
    let mut stmt = conn.prepare("SELECT value FROM _meta WHERE key = ?1")?;
    let mut rows = stmt.query(params![key])?;
    if let Some(row) = rows.next()? {
        Ok(Some(row.get(0)?))
    } else {
        Ok(None)
    }
}

/// Insert-or-replace an opaque blob at `_meta[key]`.
pub(crate) fn write_meta_blob(
    conn: &Connection,
    key: &str,
    value: &[u8],
) -> Result<(), StoreError> {
    conn.execute(
        "INSERT INTO _meta(key, value) VALUES(?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

/// Initialise (first run) or verify (subsequent runs) the
/// canary used to detect wrong-`identity_secret` at open time.
///
/// First-run path: seal `CANARY_PLAINTEXT` under the derived
/// key + `CANARY_AAD`, write the (nonce, ciphertext) pair into
/// `_meta`. Returns `Ok(())` so the caller can proceed.
///
/// Verify path: read the stored (nonce, ciphertext), unseal,
/// require the resulting plaintext to byte-equal
/// `CANARY_PLAINTEXT`. AEAD failure → `StoreError::Sealer`
/// (clear "wrong identity_secret" diagnostic). Plaintext
/// mismatch (would only occur under disk corruption that
/// happened to leave the AEAD tag valid — vanishingly
/// unlikely) → also `StoreError::Sealer` with a different
/// inner string.
pub(crate) fn check_canary(conn: &Connection, key: &aead::Key) -> Result<(), StoreError> {
    let nonce_opt = read_meta_blob(conn, "canary_nonce")?;
    let ct_opt = read_meta_blob(conn, "canary_ct")?;
    match (nonce_opt, ct_opt) {
        (None, None) => {
            let (nonce, ct) = cipher::seal(key, CANARY_AAD, CANARY_PLAINTEXT)?;
            write_meta_blob(conn, "canary_nonce", &nonce)?;
            write_meta_blob(conn, "canary_ct", &ct)?;
            Ok(())
        }
        (Some(nonce), Some(ct)) => {
            let pt = cipher::unseal_canary(key, CANARY_AAD, &nonce, &ct)?;
            if pt != CANARY_PLAINTEXT {
                return Err(StoreError::Sealer(
                    "canary plaintext mismatch (corruption?)".to_string(),
                ));
            }
            Ok(())
        }
        _ => Err(StoreError::Schema(
            "canary partially present (nonce or ct missing)".to_string(),
        )),
    }
}
