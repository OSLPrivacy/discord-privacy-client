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
//! ## Current schema
//!
//! Identifiers are not stored. Each row keeps:
//!
//!   - `*_bi` — keyed blind indexes, the only searchable form of an
//!     identifier. Deterministic, so equality lookups work; opaque, so an
//!     offline reader learns no id.
//!   - `meta_nonce` / `meta_ct` — every identifier and timestamp, sealed under
//!     the store key with the row's own blind index as AAD.
//!   - `seq` — an opaque monotonic counter used for ordering, replacing the
//!     plaintext `decrypted_at` index.
//!
//! Plaintext is **never** persisted on disk in any form,
//! including tokenized. v1 deliberately ships without search;
//! v1.5 will add a decrypt-and-scan path that holds plaintext
//! only in memory. See `SECURITY.md` § "Search".

use crate::cipher::{self, AttachmentMeta, MessageMeta};
use crate::StoreError;
use crypto::aead;
use rusqlite::{params, Connection, Transaction};

/// Current schema version. Bumped on every backward-incompatible
/// change; migrations between versions are dispatched in
/// [`migrate`].
///
/// History:
///   v1 — initial schema (Phase 5b1). messages(id, channel,
///        sender_*, ciphertext, nonce, decrypted_at, burned).
///   v2 — Phase 7a. Adds burned_at, wrapped_key, scope_type,
///        scope_id columns (all NULL on existing rows).
///   v3 — Beta 1.0. Adds the `attachments` table so decrypted
///        image/file bytes survive a restart.
///   v4 — 2026-07-26. Removes every plaintext identifier from disk. Ids and
///        timestamps move inside a sealed per-row blob; lookups run against
///        keyed blind indexes; ordering runs against an opaque `seq`. Closes
///        the audit finding that an offline reader could reconstruct the
///        protected social graph without the store key.
///   v5 — Message bodies use a fresh per-record content key. The store master
///        key authenticates and wraps that content key; it no longer encrypts
///        live message bodies directly.
///   v6 — Attachments use independent per-record content keys. Their metadata,
///        body, wrapper, owning message selector, order, and content version
///        form one authenticated envelope. Burned attachments remain only as
///        keyless, zeroed audit stubs.
///   v7 — Adds an encrypted, authenticated attachment inventory per message.
///        New inventories are complete; migrated inventories are explicitly
///        marked incomplete because history cannot prove what was never
///        cached.
///   v8 — Removes plaintext `burned_at` from message and attachment audit
///        stubs. The terminal `burned` bit remains queryable, but an offline
///        reader no longer learns the exact time of destructive activity.
///   v9 — Adds receipt lifecycle fields and per-device acknowledgement rows.
pub(crate) const SCHEMA_VERSION: u32 = 9;
const PRIVACY_SCHEMA_VERSION: u32 = 4;
const MESSAGE_ENVELOPE_SCHEMA_VERSION: u32 = 5;
const ATTACHMENT_ENVELOPE_SCHEMA_VERSION: u32 = 6;
const ATTACHMENT_MANIFEST_SCHEMA_VERSION: u32 = 7;
const BURN_TIMESTAMP_PRIVACY_SCHEMA_VERSION: u32 = 8;

/// Fixed canary plaintext. Hard-coded so a wrong-key unseal that
/// happens to produce non-error garbage still fails the
/// post-unseal byte-equality check.
pub(crate) const CANARY_PLAINTEXT: &[u8] = b"osl-message-store-canary-v1";

/// AAD bound to the canary so it can't be replayed against the
/// row-keyed AAD scheme used by `messages`.
pub(crate) const CANARY_AAD: &[u8] = b"osl-message-store/canary";

/// The `_meta` table alone. Created before anything else so the canary can be
/// checked — and the caller's secret proven — before a migration is allowed to
/// rewrite a single row.
const SCHEMA_META: &str = r#"
CREATE TABLE IF NOT EXISTS _meta (
    key TEXT PRIMARY KEY,
    value BLOB
);
"#;

/// Current tables. No column here holds an identifier in the clear.
const SCHEMA_CURRENT: &str = r#"
CREATE TABLE IF NOT EXISTS messages (
    mid_bi BLOB PRIMARY KEY,
    chan_bi BLOB NOT NULL,
    sender_bi BLOB NOT NULL,
    meta_nonce BLOB NOT NULL,
    meta_ct BLOB NOT NULL,
    ciphertext BLOB NOT NULL,
    nonce BLOB NOT NULL,
    seq INTEGER NOT NULL,
    burned INTEGER NOT NULL DEFAULT 0,
    content_version INTEGER NOT NULL DEFAULT 1,
    wrapped_key_nonce BLOB,
    wrapped_key BLOB,
    delivered_at BLOB,
    opened_at BLOB,
    destroyed_at BLOB,
    destruct_reason BLOB,

    -- Null downgrade-guard columns. The exact final v3 reader (adff4e45)
    -- creates its legacy indexes before checking schema_version. Keeping the
    -- columns present but permanently NULL lets that unchanged reader reach
    -- and return its explicit "newer than supported" refusal instead of
    -- failing early with "no such column". They are never selectors or data
    -- storage in v4.
    discord_message_id TEXT,
    channel_id TEXT,
    sender_discord_id TEXT,
    sender_osl_user_id TEXT,
    decrypted_at INTEGER,
    scope_type TEXT,
    scope_id TEXT,
    meta_tag BLOB
);

CREATE INDEX IF NOT EXISTS idx_messages_chan_seq
    ON messages(chan_bi, seq DESC);
CREATE INDEX IF NOT EXISTS idx_messages_channel
    ON messages(channel_id, decrypted_at DESC);

CREATE TABLE IF NOT EXISTS message_device_acks (
    mid_bi BLOB NOT NULL,
    device_bi BLOB NOT NULL,
    ack_kind TEXT NOT NULL CHECK (ack_kind IN (
        'delivered', 'opened', 'destroyed', 'already_absent', 'never_held'
    )),
    acknowledged_at BLOB NOT NULL,
    destruct_reason BLOB,
    PRIMARY KEY (mid_bi, device_bi, ack_kind),
    FOREIGN KEY (mid_bi) REFERENCES messages(mid_bi) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS attachments (
    ck_bi BLOB PRIMARY KEY,
    mid_bi BLOB NOT NULL,
    sender_bi BLOB,
    meta_nonce BLOB NOT NULL,
    meta_ct BLOB NOT NULL,
    ciphertext BLOB NOT NULL,
    nonce BLOB NOT NULL,
    seq INTEGER NOT NULL,
    burned INTEGER NOT NULL DEFAULT 0,
    content_version INTEGER NOT NULL DEFAULT 1,
    wrapped_key_nonce BLOB,
    wrapped_key BLOB,

    -- Same exact-v3 downgrade guard as `messages`; permanently NULL.
    cache_key TEXT,
    discord_message_id TEXT,
    random_filename TEXT,
    mime TEXT,
    byte_len INTEGER,
    created_at INTEGER,
    scope_type TEXT,
    scope_id TEXT,
    sender_discord_id TEXT
);

CREATE INDEX IF NOT EXISTS idx_attachments_mid
    ON attachments(mid_bi);

CREATE INDEX IF NOT EXISTS idx_attachments_seq
    ON attachments(seq);
CREATE INDEX IF NOT EXISTS idx_attachments_msg
    ON attachments(discord_message_id);
CREATE INDEX IF NOT EXISTS idx_attachments_created
    ON attachments(created_at);
"#;

const SCHEMA_ATTACHMENT_MANIFESTS: &str = r#"
CREATE TABLE IF NOT EXISTS attachment_manifests (
    mid_bi BLOB PRIMARY KEY,
    complete INTEGER NOT NULL CHECK (complete IN (0, 1)),
    generation INTEGER NOT NULL CHECK (generation > 0),
    nonce BLOB NOT NULL,
    ciphertext BLOB NOT NULL
);
"#;

/// Additive form of the exact-v3 downgrade guards for databases created by an
/// earlier v4 build. Fresh and newly migrated databases get the same columns
/// directly from their CREATE TABLE statements above.
const V4_MESSAGE_DOWNGRADE_COLUMNS: &[&str] = &[
    "ALTER TABLE messages ADD COLUMN discord_message_id TEXT",
    "ALTER TABLE messages ADD COLUMN channel_id TEXT",
    "ALTER TABLE messages ADD COLUMN sender_discord_id TEXT",
    "ALTER TABLE messages ADD COLUMN sender_osl_user_id TEXT",
    "ALTER TABLE messages ADD COLUMN decrypted_at INTEGER",
    "ALTER TABLE messages ADD COLUMN scope_type TEXT",
    "ALTER TABLE messages ADD COLUMN scope_id TEXT",
    "ALTER TABLE messages ADD COLUMN meta_tag BLOB",
];

const V4_ATTACHMENT_DOWNGRADE_COLUMNS: &[&str] = &[
    "ALTER TABLE attachments ADD COLUMN cache_key TEXT",
    "ALTER TABLE attachments ADD COLUMN discord_message_id TEXT",
    "ALTER TABLE attachments ADD COLUMN random_filename TEXT",
    "ALTER TABLE attachments ADD COLUMN mime TEXT",
    "ALTER TABLE attachments ADD COLUMN byte_len INTEGER",
    "ALTER TABLE attachments ADD COLUMN created_at INTEGER",
    "ALTER TABLE attachments ADD COLUMN scope_type TEXT",
    "ALTER TABLE attachments ADD COLUMN scope_id TEXT",
    "ALTER TABLE attachments ADD COLUMN sender_discord_id TEXT",
];

/// Create `_meta` only. Split out from [`migrate`] so `open` can verify the
/// canary first.
pub(crate) fn ensure_meta_table(conn: &Connection) -> Result<(), StoreError> {
    conn.execute_batch(SCHEMA_META)?;
    Ok(())
}

/// Apply the schema and resolve the on-disk version.
///
/// **Call only after the canary has verified the caller's secret.** The v3→v4
/// step rewrites every row, and doing that under an unproven key would destroy
/// the database of anyone who mistyped a password.
pub(crate) fn migrate(
    conn: &Connection,
    key: &aead::Key,
    index_key: &[u8; 32],
) -> Result<(), StoreError> {
    let on_disk: Option<u32> = read_meta_u32(conn, "schema_version")?;

    match on_disk {
        Some(v) if v > SCHEMA_VERSION => {
            return Err(StoreError::Schema(format!(
                "on-disk schema version {v} is newer than this binary supports \
                 ({SCHEMA_VERSION}); refusing to open"
            )));
        }
        None if !table_exists(conn, "messages")? => {
            // First-ever open with this DB file: straight to the current schema.
            //
            // Creating the tables and stamping the version must commit
            // together. A crash between them leaves a `messages` table with no
            // recorded version, and the next open reads that as a LEGACY
            // database and tries to migrate it by selecting plaintext columns
            // v4 does not have — so a crash during first-run initialisation
            // would leave a store that cannot be opened again at all.
            conn.execute_batch("BEGIN IMMEDIATE;")?;
            let created = (|| -> Result<(), StoreError> {
                conn.execute_batch(SCHEMA_CURRENT)?;
                conn.execute_batch(SCHEMA_ATTACHMENT_MANIFESTS)?;
                write_meta_u32(conn, "schema_version", SCHEMA_VERSION)
            })();
            if let Err(e) = created {
                let _ = conn.execute_batch("ROLLBACK;");
                return Err(e);
            }
            conn.execute_batch("COMMIT;")?;
            return Ok(());
        }
        Some(v) if v == SCHEMA_VERSION => {
            refuse_plaintext_burn_time_columns(conn)?;
            // Already current. An earlier privacy-schema build may predate the null
            // downgrade-guard columns, so add those before SCHEMA_CURRENT creates
            // the exact-v3 compatibility indexes that reference them.
            apply_columns(conn, "messages", V4_MESSAGE_DOWNGRADE_COLUMNS)?;
            apply_columns(conn, "attachments", V4_ATTACHMENT_DOWNGRADE_COLUMNS)?;
            // CREATE IF NOT EXISTS keeps this idempotent.
            conn.execute_batch(SCHEMA_CURRENT)?;
            conn.execute_batch(SCHEMA_ATTACHMENT_MANIFESTS)?;
            // Finish a scrub a previous run committed but did not complete.
            run_pending_vacuum(conn)?;
            return Ok(());
        }
        Some(BURN_TIMESTAMP_PRIVACY_SCHEMA_VERSION) => {
            migrate_v8_to_v9(conn)?;
            return Ok(());
        }
        Some(MESSAGE_ENVELOPE_SCHEMA_VERSION) => {
            migrate_v5_to_v6(conn, key, index_key)?;
            migrate_v6_to_v7(conn, key)?;
            migrate_v7_to_v8(conn)?;
            migrate_v8_to_v9(conn)?;
            return Ok(());
        }
        Some(ATTACHMENT_ENVELOPE_SCHEMA_VERSION) => {
            migrate_v6_to_v7(conn, key)?;
            migrate_v7_to_v8(conn)?;
            migrate_v8_to_v9(conn)?;
            return Ok(());
        }
        Some(ATTACHMENT_MANIFEST_SCHEMA_VERSION) => {
            migrate_v7_to_v8(conn)?;
            migrate_v8_to_v9(conn)?;
            return Ok(());
        }
        Some(PRIVACY_SCHEMA_VERSION) => {
            migrate_v4_to_v5(conn, key)?;
            migrate_v5_to_v6(conn, key, index_key)?;
            migrate_v6_to_v7(conn, key)?;
            migrate_v7_to_v8(conn)?;
            migrate_v8_to_v9(conn)?;
            return Ok(());
        }
        _ => {}
    }

    // A pre-v4 live `wrapped_key` has no authenticated format or algorithm
    // marker. Refuse before any legacy rewrite commits, so a later v5
    // migration failure cannot strand the profile halfway between schemas.
    refuse_ambiguous_legacy_wrappers(conn)?;

    // Legacy database (v1, v2 or v3, or an unstamped file that already has a
    // `messages` table). Bring the old shape up to a known v3 first so the
    // rewrite can read a predictable column set, then rewrite into v4.
    apply_legacy_columns(conn)?;
    // The version stamp is written inside the migration's own transaction, so
    // there is deliberately no stamp here — see `migrate_v3_to_v4`.
    migrate_v3_to_v4(conn, key, index_key)?;
    migrate_v4_to_v5(conn, key)?;
    migrate_v5_to_v6(conn, key, index_key)?;
    migrate_v6_to_v7(conn, key)?;
    migrate_v7_to_v8(conn)?;
    migrate_v8_to_v9(conn)?;
    Ok(())
}

/// Read the stamped schema version without creating tables, indexes, recovery
/// markers, or otherwise changing the database. Anchored open uses this to
/// decide whether an existing provider record must be reconciled before a
/// migration is allowed to mutate state.
pub(crate) fn inspect_schema_version(conn: &Connection) -> Result<Option<u32>, StoreError> {
    if !table_exists(conn, "_meta")? {
        return Ok(None);
    }
    read_meta_u32(conn, "schema_version")
}

/// Whether the existing file has a metadata table.  Anchored open uses this
/// read-only probe before it is permitted to create `_meta`: an absent table
/// must be distinguishable from an enrolled database whose anchor record was
/// removed or replayed.
pub(crate) fn has_meta_table(conn: &Connection) -> Result<bool, StoreError> {
    table_exists(conn, "_meta")
}

/// Additive columns from the v2/v3 era. Some very old files predate them, and
/// the rewrite reads them, so they must exist before it runs.
fn apply_legacy_columns(conn: &Connection) -> Result<(), StoreError> {
    if table_exists(conn, "messages")? {
        apply_columns(
            conn,
            "messages",
            &[
                "ALTER TABLE messages ADD COLUMN burned_at INTEGER",
                "ALTER TABLE messages ADD COLUMN wrapped_key BLOB",
                "ALTER TABLE messages ADD COLUMN scope_type TEXT",
                "ALTER TABLE messages ADD COLUMN scope_id TEXT",
            ],
        )?;
    }
    if table_exists(conn, "attachments")? {
        apply_columns(
            conn,
            "attachments",
            &[
                "ALTER TABLE attachments ADD COLUMN scope_type TEXT",
                "ALTER TABLE attachments ADD COLUMN scope_id TEXT",
                "ALTER TABLE attachments ADD COLUMN sender_discord_id TEXT",
            ],
        )?;
    }
    Ok(())
}

/// Rewrite a legacy database into v4.
///
/// ## Shape
///
/// Build the new tables beside the old, copy every row through the blind-index
/// and sealing functions, drop the originals, rename. All inside one
/// transaction so a crash mid-migration leaves the original database intact
/// rather than half-converted.
///
/// ## What is preserved byte-for-byte
///
/// Live rows keep `ciphertext`, `nonce` and `wrapped_key` byte-for-byte. The
/// body AAD is still `discord_message_id`, so **no live message body is ever
/// re-encrypted** — the migration cannot corrupt content it does not touch, and
/// a body sealed by any earlier build stays openable.
///
/// Burned rows are different. Older builds could set `burned = 1` while
/// leaving an intact sealed body and wrapped key behind. Copying those bytes
/// into the privacy schema would preserve a secret the row says was destroyed.
/// Migration therefore keeps the terminal flag and audit timestamp but writes
/// zeroed body/nonce blobs and no wrapped key.
///
/// ## Ordering
///
/// `seq` is assigned in `decrypted_at` order, so `list_by_channel` returns
/// exactly what it returned before the migration.
///
/// ## Why VACUUM at the end
///
/// Dropping a table frees its pages but does not necessarily scrub them. The
/// whole point of this migration is that the identifiers stop existing on disk,
/// so leaving them recoverable in free pages would defeat it. `secure_delete`
/// is on and the final `VACUUM` rewrites the file.
fn migrate_v3_to_v4(
    conn: &Connection,
    key: &aead::Key,
    index_key: &[u8; 32],
) -> Result<(), StoreError> {
    conn.execute_batch(
        r#"
BEGIN IMMEDIATE;
CREATE TABLE messages_v4 (
    mid_bi BLOB PRIMARY KEY,
    chan_bi BLOB NOT NULL,
    sender_bi BLOB NOT NULL,
    meta_nonce BLOB NOT NULL,
    meta_ct BLOB NOT NULL,
    ciphertext BLOB NOT NULL,
    nonce BLOB NOT NULL,
    seq INTEGER NOT NULL,
    burned INTEGER NOT NULL DEFAULT 0,
    burned_at INTEGER,
    wrapped_key BLOB,
    discord_message_id TEXT,
    channel_id TEXT,
    sender_discord_id TEXT,
    sender_osl_user_id TEXT,
    decrypted_at INTEGER,
    scope_type TEXT,
    scope_id TEXT,
    meta_tag BLOB
);
CREATE TABLE attachments_v4 (
    ck_bi BLOB PRIMARY KEY,
    mid_bi BLOB NOT NULL,
    sender_bi BLOB,
    meta_nonce BLOB NOT NULL,
    meta_ct BLOB NOT NULL,
    ciphertext BLOB NOT NULL,
    nonce BLOB NOT NULL,
    seq INTEGER NOT NULL,
    cache_key TEXT,
    discord_message_id TEXT,
    random_filename TEXT,
    mime TEXT,
    byte_len INTEGER,
    created_at INTEGER,
    scope_type TEXT,
    scope_id TEXT,
    sender_discord_id TEXT
);
"#,
    )?;

    let result = (|| -> Result<(), StoreError> {
        if table_exists(conn, "messages")? {
            #[allow(clippy::type_complexity)]
            let rows: Vec<(
                String,
                String,
                String,
                String,
                Vec<u8>,
                Vec<u8>,
                i64,
                i64,
                Option<i64>,
                Option<Vec<u8>>,
            )> = {
                let mut stmt = conn.prepare(
                    "SELECT discord_message_id, channel_id, sender_discord_id, \
                            sender_osl_user_id, ciphertext, nonce, decrypted_at, \
                            burned, burned_at, wrapped_key \
                       FROM messages ORDER BY decrypted_at ASC, rowid ASC",
                )?;
                let mapped = stmt.query_map([], |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                        r.get(7)?,
                        r.get(8)?,
                        r.get(9)?,
                    ))
                })?;
                let mut out = Vec::new();
                for row in mapped {
                    out.push(row?);
                }
                out
            };

            for (seq, row) in rows.into_iter().enumerate() {
                let (mid, chan, sender, osl, ct, nonce, decrypted_at, burned, burned_at, wk) = row;
                let (ct, nonce, wk) = if burned != 0 {
                    (vec![0; ct.len()], vec![0; nonce.len()], None)
                } else {
                    (ct, nonce, wk)
                };
                let meta = MessageMeta {
                    discord_message_id: mid.clone(),
                    channel_id: chan.clone(),
                    sender_discord_id: sender.clone(),
                    sender_osl_user_id: osl,
                    decrypted_at,
                };
                let mid_bi = cipher::blind_index(index_key, cipher::BI_MESSAGE_ID, &mid)?;
                let chan_bi = cipher::blind_index(index_key, cipher::BI_CHANNEL_ID, &chan)?;
                let sender_bi = cipher::blind_index(index_key, cipher::BI_SENDER_ID, &sender)?;
                let (meta_nonce, meta_ct) =
                    cipher::seal(key, &mid_bi, &cipher::encode_message_meta(&meta))?;
                conn.execute(
                    // Plain INSERT, not INSERT OR REPLACE.
                    //
                    // Two legacy rows cannot share an identifier — v1/v3 make
                    // `discord_message_id` and `cache_key` PRIMARY KEYs — so the
                    // only way two rows collide here is a blind-index collision,
                    // which is not a case to paper over. `OR REPLACE` would
                    // silently drop one of the user's messages and report a
                    // successful migration. A plain INSERT aborts inside the
                    // migration transaction instead, so the original database
                    // is rolled back intact and the failure is visible.
                    "INSERT INTO messages_v4 \
                        (mid_bi, chan_bi, sender_bi, meta_nonce, meta_ct, \
                         ciphertext, nonce, seq, burned, burned_at, wrapped_key) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    params![
                        mid_bi,
                        chan_bi,
                        sender_bi,
                        meta_nonce,
                        meta_ct,
                        ct,
                        nonce,
                        seq as i64 + 1,
                        burned,
                        burned_at,
                        wk
                    ],
                )?;
            }
        }

        if table_exists(conn, "attachments")? {
            #[allow(clippy::type_complexity)]
            let rows: Vec<(
                String,
                String,
                String,
                String,
                Vec<u8>,
                Vec<u8>,
                i64,
                i64,
                Option<String>,
                Option<String>,
                Option<String>,
            )> = {
                let mut stmt = conn.prepare(
                    "SELECT cache_key, discord_message_id, random_filename, mime, \
                            ciphertext, nonce, byte_len, created_at, \
                            scope_type, scope_id, sender_discord_id \
                       FROM attachments ORDER BY created_at ASC, rowid ASC",
                )?;
                let mapped = stmt.query_map([], |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                        r.get(7)?,
                        r.get(8)?,
                        r.get(9)?,
                        r.get(10)?,
                    ))
                })?;
                let mut out = Vec::new();
                for row in mapped {
                    out.push(row?);
                }
                out
            };

            for (seq, row) in rows.into_iter().enumerate() {
                let (ck, mid, fname, mime, ct, nonce, byte_len, created_at, st, sid, sender) = row;
                let meta = AttachmentMeta {
                    cache_key: ck.clone(),
                    discord_message_id: mid.clone(),
                    random_filename: fname,
                    mime,
                    byte_len,
                    created_at,
                    scope_type: st,
                    scope_id: sid,
                    sender_discord_id: sender.clone(),
                };
                let ck_bi = cipher::blind_index(index_key, cipher::BI_CACHE_KEY, &ck)?;
                let mid_bi = cipher::blind_index(index_key, cipher::BI_MESSAGE_ID, &mid)?;
                let sender_bi = match sender.as_deref() {
                    Some(s) => Some(cipher::blind_index(index_key, cipher::BI_SENDER_ID, s)?),
                    None => None,
                };
                let (meta_nonce, meta_ct) =
                    cipher::seal(key, &ck_bi, &cipher::encode_attachment_meta(&meta))?;
                conn.execute(
                    // Plain INSERT for the same reason as `messages_v4`.
                    "INSERT INTO attachments_v4 \
                        (ck_bi, mid_bi, sender_bi, meta_nonce, meta_ct, \
                         ciphertext, nonce, seq) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        ck_bi,
                        mid_bi,
                        sender_bi,
                        meta_nonce,
                        meta_ct,
                        ct,
                        nonce,
                        seq as i64 + 1
                    ],
                )?;
            }
        }
        // Purge attachments whose live message row is gone.
        //
        // Pre-fix builds created these: `delete_messages_in_channel` removed
        // message rows and left the cached pictures behind, after which nothing
        // linked them to a channel and no burn predicate could ever reach them.
        // A burned conversation's images could therefore outlive it
        // indefinitely.
        //
        // A burned audit stub is not a live parent. Legacy builds could leave
        // `burned = 1` over an intact message body and attachment cache. The
        // message copy above shreds that body; retaining its attachment here
        // would preserve another decrypted body belonging to the same terminal
        // message.
        //
        // Doing this at migration only, and not as an ongoing sweep, is
        // deliberate. `cmd_osl_attachment_cache_put` writes an attachment
        // without requiring its message row to exist, and the UI fetches by the
        // message id it reads from Discord's DOM rather than from this store —
        // so an orphan is legitimately reachable during normal operation and a
        // recurring sweep would evict live cache entries. Here the cost is
        // bounded and reversible: a purged row is re-fetched from the CDN and
        // re-decrypted on next view.
        conn.execute(
            "DELETE FROM attachments_v4 \
              WHERE mid_bi NOT IN ( \
                    SELECT mid_bi FROM messages_v4 WHERE burned = 0 \
              )",
            [],
        )?;
        Ok(())
    })();

    if let Err(e) = result {
        // Leave the original tables untouched.
        let _ = conn.execute_batch("ROLLBACK;");
        return Err(e);
    }

    conn.execute_batch(
        r#"
DROP TABLE IF EXISTS messages;
DROP TABLE IF EXISTS attachments;
ALTER TABLE messages_v4 RENAME TO messages;
ALTER TABLE attachments_v4 RENAME TO attachments;
"#,
    )?;
    // Stamp the privacy-schema version INSIDE the same transaction as the
    // rename. The v4→v5 message-key migration runs as a separate atomic rewrite
    // immediately after this function returns.
    //
    // Stamping after the commit leaves a window in which a crash produces a
    // database whose tables are v4 but whose recorded version is still 3. The
    // next open would then try to add legacy columns to the new schema and
    // then fail reading `discord_message_id`, so the store would not open at
    // all. Committing the shape and the version together makes the migration
    // atomic: a crash either leaves a whole v3 database or a whole v4 one.
    write_meta_u32(conn, "schema_version", PRIVACY_SCHEMA_VERSION)?;
    // The VACUUM that scrubs the old plaintext pages cannot run inside a
    // transaction, so it necessarily happens after this commit. Record that it
    // is owed. If the process dies in between, the data is already correct but
    // the freed pages may still hold identifiers, and the next open finishes
    // the job. Without this flag that scrub would simply never happen and the
    // migration would silently leave behind exactly what it exists to remove.
    write_meta_blob(conn, VACUUM_PENDING_KEY, &[1])?;
    conn.execute_batch("COMMIT;")?;

    // Recreate indexes and scrub the freed pages that still hold the old
    // plaintext identifiers.
    conn.execute_batch(SCHEMA_CURRENT)?;
    run_pending_vacuum(conn)?;
    Ok(())
}

/// Rewrite v4 message bodies into the v5 per-record content-key envelope.
///
/// Attachments deliberately remain byte-for-byte on their v4 construction.
/// They have a different selector and lifecycle and therefore require their own
/// schema change rather than borrowing a message wrapper by implication.
fn migrate_v4_to_v5(conn: &Connection, key: &aead::Key) -> Result<(), StoreError> {
    conn.execute_batch(
        r#"
BEGIN IMMEDIATE;
CREATE TABLE messages_v5 (
    mid_bi BLOB PRIMARY KEY,
    chan_bi BLOB NOT NULL,
    sender_bi BLOB NOT NULL,
    meta_nonce BLOB NOT NULL,
    meta_ct BLOB NOT NULL,
    ciphertext BLOB NOT NULL,
    nonce BLOB NOT NULL,
    seq INTEGER NOT NULL,
    burned INTEGER NOT NULL DEFAULT 0,
    burned_at INTEGER,
    content_version INTEGER NOT NULL,
    wrapped_key_nonce BLOB,
    wrapped_key BLOB,
    discord_message_id TEXT,
    channel_id TEXT,
    sender_discord_id TEXT,
    sender_osl_user_id TEXT,
    decrypted_at INTEGER,
    scope_type TEXT,
    scope_id TEXT,
    meta_tag BLOB
);
"#,
    )?;

    let result = (|| -> Result<(), StoreError> {
        #[allow(clippy::type_complexity)]
        let rows: Vec<(
            Vec<u8>,
            Vec<u8>,
            Vec<u8>,
            Vec<u8>,
            Vec<u8>,
            Vec<u8>,
            Vec<u8>,
            i64,
            i64,
            Option<i64>,
            Option<Vec<u8>>,
        )> = {
            let mut stmt = conn.prepare(
                "SELECT mid_bi, chan_bi, sender_bi, meta_nonce, meta_ct, \
                        ciphertext, nonce, seq, burned, burned_at, wrapped_key \
                   FROM messages ORDER BY seq ASC, rowid ASC",
            )?;
            let mapped = stmt.query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                    row.get(10)?,
                ))
            })?;
            let mut out = Vec::new();
            for row in mapped {
                out.push(row?);
            }
            out
        };

        for (
            mid_bi,
            chan_bi,
            sender_bi,
            old_meta_nonce,
            old_meta_ct,
            old_ciphertext,
            old_nonce,
            seq,
            burned,
            burned_at,
            legacy_wrapped_key,
        ) in rows
        {
            let meta_bytes = cipher::unseal(key, &mid_bi, &old_meta_nonce, &old_meta_ct)?;
            let meta = cipher::decode_message_meta(&meta_bytes)?;
            let content_version = 1i64;
            let (meta_nonce, meta_ct) = cipher::seal(
                key,
                &cipher::message_meta_aad(&mid_bi, content_version),
                &meta_bytes,
            )?;

            let (ciphertext, nonce, wrapped_key_nonce, wrapped_key) = if burned != 0 {
                (
                    vec![0; old_ciphertext.len()],
                    vec![0; old_nonce.len()],
                    None,
                    None,
                )
            } else {
                if legacy_wrapped_key.is_some() {
                    return Err(StoreError::Schema(
                        "live v4 message has an ambiguous non-NULL wrapped_key; \
                         refusing to guess its format"
                            .to_string(),
                    ));
                }
                let plaintext = cipher::unseal(
                    key,
                    meta.discord_message_id.as_bytes(),
                    &old_nonce,
                    &old_ciphertext,
                )?;
                let sealed = cipher::seal_message_body(key, &mid_bi, content_version, &plaintext)?;
                (
                    sealed.ciphertext,
                    sealed.nonce,
                    Some(sealed.wrapped_key_nonce),
                    Some(sealed.wrapped_key),
                )
            };

            conn.execute(
                "INSERT INTO messages_v5 \
                    (mid_bi, chan_bi, sender_bi, meta_nonce, meta_ct, \
                     ciphertext, nonce, seq, burned, burned_at, content_version, \
                     wrapped_key_nonce, wrapped_key) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    mid_bi,
                    chan_bi,
                    sender_bi,
                    meta_nonce,
                    meta_ct,
                    ciphertext,
                    nonce,
                    seq,
                    burned,
                    burned_at,
                    content_version,
                    wrapped_key_nonce,
                    wrapped_key
                ],
            )?;
        }

        conn.execute_batch(
            r#"
DROP TABLE messages;
ALTER TABLE messages_v5 RENAME TO messages;
"#,
        )?;
        write_meta_u32(conn, "schema_version", MESSAGE_ENVELOPE_SCHEMA_VERSION)?;
        // Old v4 pages contain bodies directly encrypted by the master key.
        // Record the post-commit scrub before committing the new shape.
        write_meta_blob(conn, VACUUM_PENDING_KEY, &[1])?;
        conn.execute_batch("COMMIT;")?;
        Ok(())
    })();

    if let Err(error) = result {
        let _ = conn.execute_batch("ROLLBACK;");
        return Err(error);
    }

    conn.execute_batch(SCHEMA_CURRENT)?;
    run_pending_vacuum(conn)?;
    Ok(())
}

/// Rewrite v5 attachment bodies into independent v6 content-key envelopes.
///
/// Every legacy body and its sealed metadata are authenticated before a new
/// table is substituted. The table replacement, version stamp, and post-commit
/// scrub marker commit together. Any malformed row, duplicate selector, or
/// sealing failure rolls the entire transaction back to the byte-compatible v5
/// logical state; no mixture of direct-master and wrapped attachment rows is
/// accepted.
fn migrate_v5_to_v6(
    conn: &Connection,
    key: &aead::Key,
    index_key: &[u8; 32],
) -> Result<(), StoreError> {
    conn.execute_batch(
        r#"
BEGIN IMMEDIATE;
CREATE TABLE attachments_v6 (
    ck_bi BLOB PRIMARY KEY,
    mid_bi BLOB NOT NULL,
    sender_bi BLOB,
    meta_nonce BLOB NOT NULL,
    meta_ct BLOB NOT NULL,
    ciphertext BLOB NOT NULL,
    nonce BLOB NOT NULL,
    seq INTEGER NOT NULL,
    burned INTEGER NOT NULL DEFAULT 0,
    burned_at INTEGER,
    content_version INTEGER NOT NULL,
    wrapped_key_nonce BLOB,
    wrapped_key BLOB,
    cache_key TEXT,
    discord_message_id TEXT,
    random_filename TEXT,
    mime TEXT,
    byte_len INTEGER,
    created_at INTEGER,
    scope_type TEXT,
    scope_id TEXT,
    sender_discord_id TEXT
);
"#,
    )?;

    let result = (|| -> Result<(), StoreError> {
        #[allow(clippy::type_complexity)]
        let rows: Vec<(
            Vec<u8>,
            Vec<u8>,
            Option<Vec<u8>>,
            Vec<u8>,
            Vec<u8>,
            Vec<u8>,
            Vec<u8>,
            i64,
        )> = {
            let mut stmt = conn.prepare(
                "SELECT ck_bi, mid_bi, sender_bi, meta_nonce, meta_ct, \
                        ciphertext, nonce, seq \
                   FROM attachments ORDER BY seq ASC, rowid ASC",
            )?;
            let mapped = stmt.query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                ))
            })?;
            let mut out = Vec::new();
            for row in mapped {
                out.push(row?);
            }
            out
        };

        for (ck_bi, mid_bi, sender_bi, old_meta_nonce, old_meta_ct, old_ct, old_nonce, seq) in rows
        {
            let metadata = cipher::unseal(key, &ck_bi, &old_meta_nonce, &old_meta_ct)?;
            let meta = cipher::decode_attachment_meta(&metadata)?;
            if meta.cache_key != format!("{}/{}", meta.discord_message_id, meta.random_filename) {
                return Err(StoreError::Schema(
                    "v5 attachment cache key is not the canonical owner/filename pair".to_string(),
                ));
            }
            let expected_ck =
                cipher::blind_index(index_key, cipher::BI_CACHE_KEY, &meta.cache_key)?;
            let expected_mid =
                cipher::blind_index(index_key, cipher::BI_MESSAGE_ID, &meta.discord_message_id)?;
            let expected_sender = match meta.sender_discord_id.as_deref() {
                Some(sender) => Some(cipher::blind_index(
                    index_key,
                    cipher::BI_SENDER_ID,
                    sender,
                )?),
                None => None,
            };
            if ck_bi != expected_ck || mid_bi != expected_mid || sender_bi != expected_sender {
                return Err(StoreError::Schema(
                    "v5 attachment selector does not match its sealed metadata".to_string(),
                ));
            }
            let plaintext = cipher::unseal(key, meta.cache_key.as_bytes(), &old_nonce, &old_ct)?;
            if meta.byte_len < 0 || meta.byte_len as usize != plaintext.len() {
                return Err(StoreError::Schema(
                    "v5 attachment byte length does not match its authenticated body".to_string(),
                ));
            }
            let content_version = 1i64;
            let sealed = cipher::seal_attachment_body(
                key,
                &ck_bi,
                &mid_bi,
                seq,
                content_version,
                &metadata,
                &plaintext,
            )?;
            conn.execute(
                "INSERT INTO attachments_v6 \
                    (ck_bi, mid_bi, sender_bi, meta_nonce, meta_ct, ciphertext, nonce, \
                     seq, burned, burned_at, content_version, wrapped_key_nonce, wrapped_key) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, NULL, ?9, ?10, ?11)",
                params![
                    ck_bi,
                    mid_bi,
                    sender_bi,
                    sealed.meta_nonce,
                    sealed.meta_ct,
                    sealed.ciphertext,
                    sealed.nonce,
                    seq,
                    content_version,
                    sealed.wrapped_key_nonce,
                    sealed.wrapped_key,
                ],
            )?;
        }

        conn.execute_batch(
            r#"
DROP TABLE attachments;
ALTER TABLE attachments_v6 RENAME TO attachments;
"#,
        )?;
        write_meta_u32(conn, "schema_version", ATTACHMENT_ENVELOPE_SCHEMA_VERSION)?;
        write_meta_blob(conn, VACUUM_PENDING_KEY, &[1])?;
        conn.execute_batch("COMMIT;")?;
        Ok(())
    })();

    if let Err(error) = result {
        let _ = conn.execute_batch("ROLLBACK;");
        return Err(error);
    }

    conn.execute_batch(SCHEMA_CURRENT)?;
    run_pending_vacuum(conn)?;
    Ok(())
}

/// Add the v7 encrypted attachment inventory without pretending that a
/// historical cache is authoritative.
///
/// Every live attachment present at migration is authenticated and recorded.
/// `complete = 0` is deliberate: the old schema had no inventory, so an empty
/// historical set cannot distinguish "never cached" from "row was deleted
/// before migration". The transaction either creates every observed manifest
/// and stamps v7, or leaves the whole v6 database unchanged.
fn migrate_v6_to_v7(conn: &Connection, key: &aead::Key) -> Result<(), StoreError> {
    conn.execute_batch(
        r#"
BEGIN IMMEDIATE;
CREATE TABLE attachment_manifests (
    mid_bi BLOB PRIMARY KEY,
    complete INTEGER NOT NULL CHECK (complete IN (0, 1)),
    generation INTEGER NOT NULL CHECK (generation > 0),
    nonce BLOB NOT NULL,
    ciphertext BLOB NOT NULL
);
"#,
    )?;

    let result = (|| -> Result<(), StoreError> {
        let mids: Vec<Vec<u8>> = {
            let mut stmt = conn.prepare(
                "SELECT mid_bi FROM messages WHERE burned = 0 \
                 UNION SELECT mid_bi FROM attachments WHERE burned = 0 \
                 ORDER BY mid_bi",
            )?;
            let rows = stmt.query_map([], |row| row.get(0))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            out
        };

        for mid_bi in mids {
            let mut entries = Vec::new();
            let mut stmt = conn.prepare(
                "SELECT ck_bi, meta_nonce, meta_ct, ciphertext, nonce, seq, \
                        content_version, wrapped_key_nonce, wrapped_key \
                   FROM attachments \
                  WHERE mid_bi = ?1 AND burned = 0 \
                  ORDER BY seq ASC, ck_bi ASC",
            )?;
            let rows = stmt.query_map(params![&mid_bi], |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, Option<Vec<u8>>>(7)?,
                    row.get::<_, Option<Vec<u8>>>(8)?,
                ))
            })?;
            for row in rows {
                let (
                    ck_bi,
                    meta_nonce,
                    meta_ct,
                    ciphertext,
                    nonce,
                    seq,
                    content_version,
                    wrapped_key_nonce,
                    wrapped_key,
                ) = row?;
                if content_version < 1 {
                    return Err(StoreError::Schema(
                        "v6 attachment content version must be positive".to_string(),
                    ));
                }
                let metadata = cipher::unseal(
                    key,
                    &cipher::attachment_meta_aad(&ck_bi, &mid_bi, seq, content_version),
                    &meta_nonce,
                    &meta_ct,
                )?;
                let wrapper_nonce = wrapped_key_nonce.as_deref().ok_or_else(|| {
                    StoreError::Schema("v6 live attachment has no wrapped-key nonce".to_string())
                })?;
                let wrapper = wrapped_key.as_deref().ok_or_else(|| {
                    StoreError::Schema("v6 live attachment has no wrapped key".to_string())
                })?;
                cipher::unseal_attachment_body(
                    key,
                    &ck_bi,
                    &mid_bi,
                    seq,
                    content_version,
                    &metadata,
                    wrapper_nonce,
                    wrapper,
                    &nonce,
                    &ciphertext,
                )?;
                entries.push(cipher::attachment_manifest_entry(
                    ck_bi,
                    seq,
                    content_version,
                    &metadata,
                    &nonce,
                    &ciphertext,
                    wrapper_nonce,
                    wrapper,
                ));
            }
            let manifest = cipher::AttachmentManifest {
                complete: false,
                generation: 1,
                entries,
            };
            let (nonce, ciphertext) = cipher::seal_attachment_manifest(key, &mid_bi, &manifest)?;
            conn.execute(
                "INSERT INTO attachment_manifests \
                    (mid_bi, complete, generation, nonce, ciphertext) \
                 VALUES (?1, 0, 1, ?2, ?3)",
                params![mid_bi, nonce, ciphertext],
            )?;
        }
        write_meta_u32(conn, "schema_version", ATTACHMENT_MANIFEST_SCHEMA_VERSION)?;
        conn.execute_batch("COMMIT;")?;
        Ok(())
    })();

    if let Err(error) = result {
        let _ = conn.execute_batch("ROLLBACK;");
        return Err(error);
    }
    conn.execute_batch(SCHEMA_CURRENT)?;
    Ok(())
}

/// Remove exact burn timestamps from the queryable schema.
///
/// `burned_at` was never read by the Store API. It exposed the wall-clock time
/// of destructive activity to anyone who could inspect SQLite, while the
/// terminal `burned` bit is sufficient for every Store-owned invariant.
///
/// Both tables and the version stamp change inside one transaction. The
/// committed `vacuum_pending` marker makes removal of the old column bytes
/// retryable across a crash between COMMIT and VACUUM.
fn migrate_v7_to_v8(conn: &Connection) -> Result<(), StoreError> {
    let tx = conn.unchecked_transaction()?;
    apply_v7_to_v8_tx(&tx)?;
    tx.commit()?;
    conn.execute_batch(SCHEMA_CURRENT)?;
    run_pending_vacuum(conn)?;
    Ok(())
}

/// Add receipt lifecycle fields and the independent per-device acknowledgement
/// ledger. Receipt metadata is stored as opaque blobs: the receipt writer owns
/// sealing and no wall-clock event time or destruction reason is exposed to an
/// offline SQLite reader.
fn migrate_v8_to_v9(conn: &Connection) -> Result<(), StoreError> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(
        "ALTER TABLE messages ADD COLUMN delivered_at BLOB;
         ALTER TABLE messages ADD COLUMN opened_at BLOB;
         ALTER TABLE messages ADD COLUMN destroyed_at BLOB;
         ALTER TABLE messages ADD COLUMN destruct_reason BLOB;
         CREATE TABLE IF NOT EXISTS message_device_acks (
             mid_bi BLOB NOT NULL,
             device_bi BLOB NOT NULL,
             ack_kind TEXT NOT NULL CHECK (ack_kind IN (
                 'delivered', 'opened', 'destroyed', 'already_absent', 'never_held'
             )),
             acknowledged_at BLOB NOT NULL,
             destruct_reason BLOB,
             PRIMARY KEY (mid_bi, device_bi, ack_kind),
             FOREIGN KEY (mid_bi) REFERENCES messages(mid_bi) ON DELETE CASCADE
         );",
    )?;
    write_meta_u32(&tx, "schema_version", SCHEMA_VERSION)?;
    tx.commit()?;
    Ok(())
}

/// Apply the semantic v7→v8 step inside a caller-owned transaction.
///
/// The anchored migration coordinator uses this to make the schema version,
/// post-VACUUM marker, migration journal phase, and external-anchor local
/// record one atomic SQLite state before it compares-and-advances the provider.
pub(crate) fn apply_v7_to_v8_tx(tx: &Transaction<'_>) -> Result<(), StoreError> {
    tx.execute_batch(
        "ALTER TABLE messages DROP COLUMN burned_at;
         ALTER TABLE attachments DROP COLUMN burned_at;",
    )?;
    tx.execute(
        "INSERT INTO _meta(key, value) VALUES ('schema_version', ?1) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![BURN_TIMESTAMP_PRIVACY_SCHEMA_VERSION.to_le_bytes().to_vec()],
    )?;
    tx.execute(
        "INSERT INTO _meta(key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![VACUUM_PENDING_KEY, vec![1u8]],
    )?;
    Ok(())
}

/// Add the current-schema indexes and manifest table within an existing
/// migration transaction. This is deliberately separate from the ordinary
/// idempotent migration path so an anchored step cannot publish a digest for a
/// table shape that changes afterwards.
pub(crate) fn install_current_schema_tx(tx: &Transaction<'_>) -> Result<(), StoreError> {
    tx.execute_batch(SCHEMA_CURRENT)?;
    tx.execute_batch(SCHEMA_ATTACHMENT_MANIFESTS)?;
    Ok(())
}

pub(crate) fn clear_vacuum_pending_tx(tx: &Transaction<'_>) -> Result<(), StoreError> {
    tx.execute(
        "DELETE FROM _meta WHERE key = ?1",
        params![VACUUM_PENDING_KEY],
    )?;
    Ok(())
}

fn refuse_plaintext_burn_time_columns(conn: &Connection) -> Result<(), StoreError> {
    for table in ["messages", "attachments"] {
        if existing_columns(conn, table)?
            .iter()
            .any(|column| column == "burned_at")
        {
            return Err(StoreError::Schema(format!(
                "schema v8 {table} table still contains plaintext burned_at"
            )));
        }
    }
    Ok(())
}

/// `_meta` key recording that a post-migration `VACUUM` is owed.
const VACUUM_PENDING_KEY: &str = "vacuum_pending";
const SHRED_CHECKPOINT_PENDING_KEY: &str = "shred_checkpoint_pending";

/// Scrub freed pages if a migration committed but did not finish vacuuming.
///
/// Idempotent and cheap when nothing is owed: one `_meta` read.
fn run_pending_vacuum(conn: &Connection) -> Result<(), StoreError> {
    if read_meta_blob(conn, VACUUM_PENDING_KEY)?.is_none() {
        return Ok(());
    }
    conn.execute_batch("VACUUM;")?;
    conn.execute(
        "DELETE FROM _meta WHERE key = ?1",
        params![VACUUM_PENDING_KEY],
    )?;
    Ok(())
}

/// Record, inside the same transaction as destructive row updates, that a
/// truncating WAL checkpoint is still owed. A crash after COMMIT but before
/// the checkpoint therefore cannot silently turn a completed logical burn
/// into a completed physical-shred claim.
pub(crate) fn mark_shred_checkpoint_pending(conn: &Connection) -> Result<(), StoreError> {
    write_meta_blob(conn, SHRED_CHECKPOINT_PENDING_KEY, &[1])
}

pub(crate) fn shred_checkpoint_pending(conn: &Connection) -> Result<bool, StoreError> {
    Ok(read_meta_blob(conn, SHRED_CHECKPOINT_PENDING_KEY)?.is_some())
}

pub(crate) fn clear_shred_checkpoint_pending(conn: &Connection) -> Result<(), StoreError> {
    conn.execute(
        "DELETE FROM _meta WHERE key = ?1",
        params![SHRED_CHECKPOINT_PENDING_KEY],
    )?;
    Ok(())
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool, StoreError> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        params![table],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

/// Refuse live wrappers written before v5, whose bytes have no authenticated
/// format marker. v5 itself is excluded by its `content_version` column.
///
/// `MessageStore::open` calls this before persistent pragmas so refusal leaves
/// the input file byte-for-byte unchanged. `migrate` calls it again to keep the
/// migration entry point fail-closed if its call order is ever reused.
pub(crate) fn refuse_ambiguous_legacy_wrappers(conn: &Connection) -> Result<(), StoreError> {
    let on_disk = if table_exists(conn, "_meta")? {
        read_meta_u32(conn, "schema_version")?
    } else {
        None
    };
    if table_exists(conn, "attachment_manifests")?
        && !matches!(
            on_disk,
            Some(ATTACHMENT_MANIFEST_SCHEMA_VERSION) | Some(SCHEMA_VERSION)
        )
    {
        return Err(StoreError::Schema(
            "pre-v7 database already contains an attachment_manifests table; \
             refusing ambiguous partial migration state"
                .to_string(),
        ));
    }

    if table_exists(conn, "messages")? {
        let columns = existing_columns(conn, "messages")?;
        if !columns.iter().any(|column| column == "content_version")
            && columns.iter().any(|column| column == "wrapped_key")
        {
            let ambiguous_live_wrappers: i64 = conn.query_row(
                "SELECT COUNT(*) FROM messages \
                  WHERE burned = 0 AND wrapped_key IS NOT NULL",
                [],
                |row| row.get(0),
            )?;
            if ambiguous_live_wrappers != 0 {
                return Err(StoreError::Schema(
                    "live legacy message has an ambiguous non-NULL wrapped_key; \
                     refusing to guess its format"
                        .to_string(),
                ));
            }
        }
    }

    if table_exists(conn, "attachments")? {
        let columns = existing_columns(conn, "attachments")?;
        let has_version = columns.iter().any(|column| column == "content_version");
        let has_wrapper_nonce = columns.iter().any(|column| column == "wrapped_key_nonce");
        let has_wrapper = columns.iter().any(|column| column == "wrapped_key");
        let any_envelope_column = has_version || has_wrapper_nonce || has_wrapper;
        let complete_envelope = has_version && has_wrapper_nonce && has_wrapper;
        if any_envelope_column
            && (!complete_envelope
                || !matches!(
                    on_disk,
                    Some(ATTACHMENT_ENVELOPE_SCHEMA_VERSION)
                        | Some(ATTACHMENT_MANIFEST_SCHEMA_VERSION)
                        | Some(SCHEMA_VERSION)
                ))
        {
            let row_count: i64 =
                conn.query_row("SELECT COUNT(*) FROM attachments", [], |row| row.get(0))?;
            if row_count != 0 {
                return Err(StoreError::Schema(
                    "live legacy attachment has ambiguous wrapped-key columns; \
                     refusing to guess their format"
                        .to_string(),
                ));
            }
        }
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

/// Return the set of column names on a given table via
/// `PRAGMA table_info`. SQLite returns an empty result for a
/// missing table; we surface that as an empty Vec.
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
pub(crate) fn check_canary(conn: &Connection, key: &aead::Key) -> Result<(), StoreError> {
    let nonce_opt = read_meta_blob(conn, "canary_nonce")?;
    let ct_opt = read_meta_blob(conn, "canary_ct")?;
    match (nonce_opt, ct_opt) {
        (None, None) => {
            // A missing canary is only legitimate on a genuinely new file.
            //
            // Treating it as first-run unconditionally means anyone who can
            // delete two `_meta` rows makes the store open under ANY secret:
            // this function would seal a fresh canary under the caller's key
            // and report success. On a legacy file that is destructive, not
            // just a bypass — `migrate` runs next, and the v3→v4 rewrite would
            // seal metadata under the wrong key while copying message bodies
            // byte-for-byte under the old one, committing a store whose two
            // halves can never be opened by the same secret again.
            //
            // Every version of this schema stamps `schema_version`, and any
            // populated store has a `messages` table, so either being present
            // means the canary was removed rather than never written.
            if read_meta_u32(conn, "schema_version")?.is_some() || table_exists(conn, "messages")? {
                return Err(StoreError::Sealer(
                    "message store has data but no canary — refusing to open. \
                     The canary was removed; opening would re-seal it under \
                     whatever secret was supplied and could destroy the store"
                        .to_string(),
                ));
            }
            let (nonce, ct) = cipher::seal(key, CANARY_AAD, CANARY_PLAINTEXT)?;
            // One transaction: a crash between the two writes would leave a
            // half-canary, and every later open would refuse the file below.
            conn.execute_batch("BEGIN IMMEDIATE;")?;
            let seeded = (|| -> Result<(), StoreError> {
                write_meta_blob(conn, "canary_nonce", &nonce)?;
                write_meta_blob(conn, "canary_ct", &ct)
            })();
            if let Err(e) = seeded {
                let _ = conn.execute_batch("ROLLBACK;");
                return Err(e);
            }
            conn.execute_batch("COMMIT;")?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v8_to_v9_adds_receipt_fields_and_per_device_ack_rows() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE _meta (key TEXT PRIMARY KEY, value BLOB);
             CREATE TABLE messages (mid_bi BLOB PRIMARY KEY);",
        )
        .unwrap();
        write_meta_u32(
            &conn,
            "schema_version",
            BURN_TIMESTAMP_PRIVACY_SCHEMA_VERSION,
        )
        .unwrap();

        migrate_v8_to_v9(&conn).unwrap();

        let columns = existing_columns(&conn, "messages").unwrap();
        for column in [
            "delivered_at",
            "opened_at",
            "destroyed_at",
            "destruct_reason",
        ] {
            assert!(columns.iter().any(|existing| existing == column));
        }
        assert!(table_exists(&conn, "message_device_acks").unwrap());
        assert_eq!(
            read_meta_u32(&conn, "schema_version").unwrap(),
            Some(SCHEMA_VERSION)
        );

        // Annotated as bytes: an unannotated `vec![0xA1]` infers Vec<i32>, which
        // has no ToSql impl, so this test target failed to compile -- and a
        // single uncompilable test target makes `cargo test --workspace`
        // produce no results at all, for every crate.
        let message: Vec<u8> = vec![0xA1];
        let device: Vec<u8> = vec![0xB2];
        let acked_at: Vec<u8> = vec![0xC3];
        let reason: Vec<u8> = vec![0xD4];
        conn.execute("INSERT INTO messages(mid_bi) VALUES (?1)", params![message])
            .unwrap();
        conn.execute(
            "INSERT INTO message_device_acks(
                mid_bi, device_bi, ack_kind, acknowledged_at, destruct_reason
             ) VALUES (?1, ?2, 'destroyed', ?3, ?4)",
            params![message, device, acked_at, reason],
        )
        .unwrap();
        let acknowledgements: i64 = conn
            .query_row("SELECT COUNT(*) FROM message_device_acks", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(acknowledgements, 1);
    }
}
