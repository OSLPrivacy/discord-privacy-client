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
pub(crate) const SCHEMA_VERSION: u32 = 1;

/// Fixed canary plaintext. Hard-coded so a wrong-key unseal that
/// happens to produce non-error garbage still fails the
/// post-unseal byte-equality check.
pub(crate) const CANARY_PLAINTEXT: &[u8] = b"osl-message-store-canary-v1";

/// AAD bound to the canary so it can't be replayed against the
/// row-keyed AAD scheme used by `messages`.
pub(crate) const CANARY_AAD: &[u8] = b"osl-message-store/canary";

/// SQL for creating the v1 schema. All `CREATE`s are
/// `IF NOT EXISTS` so re-running on an already-initialised store
/// is a no-op.
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
            // Future: dispatch per-step migrations here.
            // v1 is the initial version, so no older→newer steps
            // exist yet; we just stamp the version.
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
