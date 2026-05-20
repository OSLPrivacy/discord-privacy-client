//! Persistent at-rest-encrypted SQLite cache for decrypted Discord
//! messages.
//!
//! ## Why
//!
//! Phase 5 receive decrypts DPC0::-prefixed messages on demand and
//! renders the plaintext in the live DOM. That plaintext is
//! ephemeral: a Discord refresh, a Tauri restart, or a channel
//! switch loses every prior decryption. Phase 5b adds this crate
//! so the decrypted history persists across sessions.
//!
//! ## Wire layout
//!
//! - SQLite file at `<app_data_dir>/messages.sqlite`.
//! - `messages` rows store opaque XChaCha20-Poly1305 ciphertext +
//!   per-row nonce. AAD = `discord_message_id` UTF-8 bytes (binds
//!   row identity).
//! - `_meta` holds `schema_version` and a sealed canary for
//!   wrong-`identity_secret` detection at `open()`.
//!
//! Plaintext is never persisted on disk in any form, including
//! tokenized. See `SECURITY.md` § "Search" for the rationale and
//! the v1.5 / v2 plan.
//!
//! ## Crypto
//!
//! No new crypto in this crate. The data key is HKDF-SHA256
//! derived from the caller-supplied 32-byte `identity_secret`
//! (info = `"osl-message-store-v1"`, salt empty). Per-row AEAD
//! is `crypto::aead::seal` with a fresh random 24-byte nonce.
//!
//! ## Threading
//!
//! [`MessageStore`] is `Send + Sync`. The single underlying
//! [`rusqlite::Connection`] is wrapped in a `Mutex`; concurrent
//! callers serialize at the lock. SQLite WAL mode is enabled to
//! reduce write-lock contention with future readers.
//!
//! ## What this crate does **not** do (yet)
//!
//! - It does not wire into the IPC `cmd_osl_decrypt_message` —
//!   Phase 5b1 is the crate by itself, fully tested. Phase 5b2
//!   wires the store into the decrypt path.
//! - It does not provide search. v1 ships with `get` +
//!   `list_by_channel` only. v1.5 adds a decrypt-and-scan path.
//!   v2 may add blind-indexed encrypted search if the
//!   decrypt-and-scan latency proves unworkable.

mod cipher;
mod error;
mod schema;

pub use error::StoreError;

use crypto::aead;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Mutex;

/// A single decrypted Discord message persisted in the local
/// store.
///
/// `plaintext` is **always** UTF-8 in normal operation; rows
/// whose AEAD-decrypted bytes are not valid UTF-8 surface as
/// [`StoreError::Corrupted`] rather than producing a
/// [`StoredMessage`] with mojibake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMessage {
    /// Discord-side message snowflake (the `id` from
    /// `MESSAGE_CREATE`).
    pub discord_message_id: String,

    /// Discord-side channel snowflake. Same id used by
    /// `recvExtractChannelId` / channels.json.
    pub channel_id: String,

    /// Discord-side user snowflake of the message author.
    pub sender_discord_id: String,

    /// OSL identity for the same user (e.g. `"liam"`,
    /// `"henry"`). Resolved via `peer_map.json` upstream and
    /// stored alongside so the store doesn't need to re-resolve
    /// on every read.
    pub sender_osl_user_id: String,

    /// Decrypted message body, UTF-8.
    pub plaintext: String,

    /// Unix seconds at which this message was decrypted by the
    /// receive observer. Used to order
    /// [`MessageStore::list_by_channel`] results.
    pub decrypted_at: i64,

    /// `true` after [`MessageStore::mark_burned`] has been
    /// called; burned rows are excluded from `get` and
    /// `list_by_channel`. Carried in the struct so callers
    /// iterating raw rows can see the flag.
    pub burned: bool,
}

/// At-rest-encrypted message store backed by SQLite.
///
/// Each row's plaintext is sealed with XChaCha20-Poly1305 keyed
/// off an HKDF-SHA256 derivation of the caller-supplied
/// `identity_secret`. The same secret on every open is required;
/// a canary row in `_meta` detects mismatches at `open()` time
/// (returns [`StoreError::Sealer`] without unlocking).
///
/// Plaintext never lands on disk in any form, including
/// tokenized. v1 deliberately ships without search — see
/// `SECURITY.md` § "Search".
pub struct MessageStore {
    conn: Mutex<Connection>,
    key: aead::Key,
}

impl MessageStore {
    /// Open or create the message store at
    /// `<app_data_dir>/messages.sqlite`.
    ///
    /// Creates `app_data_dir` if it does not exist. Runs schema
    /// migrations (idempotent for the current version). On a
    /// fresh DB, seeds a sealed canary under the derived data
    /// key. On reopen, verifies the canary unseals correctly —
    /// failure returns [`StoreError::Sealer`] (the
    /// wrong-`identity_secret` signal) without exposing any
    /// plaintext.
    pub fn open(app_data_dir: &Path, identity_secret: &[u8; 32]) -> Result<Self, StoreError> {
        std::fs::create_dir_all(app_data_dir)?;
        let path = app_data_dir.join("messages.sqlite");
        let conn = Connection::open(&path)?;
        // WAL mode reduces write-lock contention vs the default
        // rollback journal; foreign_keys is on for completeness
        // (we don't use FK constraints today, but enabling early
        // means a future migration that introduces them works).
        // pragma_update returns the row count; we don't care
        // about the value, just that it doesn't fail.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        // Probe-4 fix: WAL's default synchronous=NORMAL is fast but
        // loses uncheckpointed writes on a hard kill (force-close,
        // OS crash, power loss). User reports "saves some, reverts
        // others" -- the reverted rows are the most recent before
        // close. synchronous=FULL forces an fsync per WAL frame so
        // every persisted message survives a hard kill. Cost is ~1
        // extra fsync per put, which is cheap at chat-rate writes.
        conn.pragma_update(None, "synchronous", "FULL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;

        schema::migrate(&conn)?;

        let key = cipher::derive_key(identity_secret)?;
        schema::check_canary(&conn, &key)?;

        Ok(MessageStore {
            conn: Mutex::new(conn),
            key,
        })
    }

    /// Insert or replace a message in the store.
    ///
    /// Sealing happens inside this call: `msg.plaintext` is
    /// AEAD-encrypted under the derived key, AAD =
    /// `msg.discord_message_id` bytes, with a fresh random
    /// nonce per call. The nonce + ciphertext go into the
    /// `messages` row.
    pub fn put(&self, msg: &StoredMessage) -> Result<(), StoreError> {
        let aad = msg.discord_message_id.as_bytes();
        let (nonce, ct) = cipher::seal(&self.key, aad, msg.plaintext.as_bytes())?;

        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute(
            "INSERT INTO messages \
                (discord_message_id, channel_id, sender_discord_id, \
                 sender_osl_user_id, ciphertext, nonce, decrypted_at, burned) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
             ON CONFLICT(discord_message_id) DO UPDATE SET \
                channel_id = excluded.channel_id, \
                sender_discord_id = excluded.sender_discord_id, \
                sender_osl_user_id = excluded.sender_osl_user_id, \
                ciphertext = excluded.ciphertext, \
                nonce = excluded.nonce, \
                decrypted_at = excluded.decrypted_at, \
                burned = excluded.burned",
            params![
                msg.discord_message_id,
                msg.channel_id,
                msg.sender_discord_id,
                msg.sender_osl_user_id,
                ct,
                nonce,
                msg.decrypted_at,
                if msg.burned { 1_i64 } else { 0_i64 },
            ],
        )?;
        Ok(())
    }

    /// Look up a single message by its Discord snowflake.
    ///
    /// Returns `Ok(None)` if the row does not exist OR if it is
    /// burned. Burned rows are filtered at the SQL level so
    /// callers can't accidentally surface them.
    pub fn get(&self, discord_message_id: &str) -> Result<Option<StoredMessage>, StoreError> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let row_opt = conn
            .query_row(
                "SELECT channel_id, sender_discord_id, sender_osl_user_id, \
                        ciphertext, nonce, decrypted_at, burned \
                 FROM messages WHERE discord_message_id = ?1 AND burned = 0",
                params![discord_message_id],
                row_to_tuple,
            )
            .optional()?;
        let Some(t) = row_opt else { return Ok(None) };
        Ok(Some(self.materialize(discord_message_id, t)?))
    }

    /// List the most-recently-decrypted messages for a channel,
    /// newest-first, capped at `limit`.
    ///
    /// Burned rows are filtered out. Each row's plaintext is
    /// unsealed inside this call; corruption (any AEAD tag
    /// failure) surfaces as [`StoreError::Corrupted`] for the
    /// whole `list_by_channel` operation rather than a partial
    /// result with mojibake.
    pub fn list_by_channel(
        &self,
        channel_id: &str,
        limit: u32,
    ) -> Result<Vec<StoredMessage>, StoreError> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT discord_message_id, channel_id, sender_discord_id, \
                    sender_osl_user_id, ciphertext, nonce, decrypted_at, burned \
             FROM messages \
             WHERE channel_id = ?1 AND burned = 0 \
             ORDER BY decrypted_at DESC \
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![channel_id, i64::from(limit)], full_row_to_tuple)?;
        let mut out = Vec::new();
        for row in rows {
            let t = row?;
            let mid = t.0.clone();
            out.push(materialize_full(&self.key, t, &mid)?);
        }
        Ok(out)
    }

    /// Mark a message burned. Subsequent `get` returns
    /// `Ok(None)` and `list_by_channel` filters the row out.
    /// The encrypted row stays in `messages` (with `burned = 1`)
    /// so the audit trail of "we once held this id" is intact
    /// for forensic / burn-acknowledgment flows. v1 keeps the
    /// ciphertext blob in place; a future `burn-with-shred`
    /// mode could overwrite it with a fresh random blob.
    ///
    /// Returns [`StoreError::NotFound`] if no row exists for
    /// `discord_message_id`. Callers can distinguish "I burned
    /// it" from "there was nothing to burn."
    pub fn mark_burned(&self, discord_message_id: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let burned_flag: Option<i64> = conn
            .query_row(
                "SELECT burned FROM messages WHERE discord_message_id = ?1",
                params![discord_message_id],
                |r| r.get(0),
            )
            .optional()?;
        let Some(_) = burned_flag else {
            return Err(StoreError::NotFound(discord_message_id.to_string()));
        };
        // Idempotent: re-burning an already-burned row is a
        // no-op. The UPDATE below is harmless either way; the
        // explicit early return makes the contract obvious.
        conn.execute(
            "UPDATE messages SET burned = 1 WHERE discord_message_id = ?1",
            params![discord_message_id],
        )?;
        Ok(())
    }

    /// Phase 7b: wipe the `wrapped_key` column on every row that
    /// matches `(scope_type, scope_id)`, marking them burned at
    /// the same time.
    ///
    /// `wrapped_key` is the per-recipient wrapped AES-GCM K from
    /// the v=2 wire format ([`crate::wire_v2`]). Nulling it
    /// removes the ability to re-derive K and thus to re-decrypt
    /// the row's `ciphertext` after the fact — which is what
    /// gives scope burns "real teeth" per
    /// `docs/phase-7-design.md` §3.2.
    ///
    /// Returns the number of rows touched (useful for the caller
    /// to log "burned N messages" without a separate count
    /// query).
    pub fn wipe_wrapped_keys_in_scope(
        &self,
        scope_type: &str,
        scope_id: &str,
    ) -> Result<usize, StoreError> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let rows = conn.execute(
            "UPDATE messages \
                SET wrapped_key = NULL, burned = 1, burned_at = strftime('%s','now') \
              WHERE scope_type = ?1 AND scope_id = ?2",
            params![scope_type, scope_id],
        )?;
        Ok(rows)
    }

    /// 7d-FIX1: full data destruction for a channel. Unlike
    /// `wipe_wrapped_keys_in_scope` which leaves the encrypted
    /// `ct` column intact and merely marks rows burned, this
    /// DELETE removes every row for `channel_id` entirely. The
    /// receive observer's re-decrypt pass then has no history
    /// row to materialize — combined with the JS-side
    /// `__oslBurnedScopes` cache that skips dispatch, the user's
    /// view of the channel becomes pure ciphertext.
    ///
    /// WAL-mode sqlite is fine; the WAL syncs on next checkpoint.
    /// We don't `VACUUM` — deleted rows hold encrypted plaintext
    /// (not raw plaintext), so leaving them in deleted-but-not-
    /// vacuumed state is acceptable for v1.
    ///
    /// Returns the row count for diagnostic logging.
    pub fn delete_messages_in_channel(&self, channel_id: &str) -> Result<usize, StoreError> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let rows = conn.execute(
            "DELETE FROM messages WHERE channel_id = ?1",
            params![channel_id],
        )?;
        Ok(rows)
    }

    /// Materialize a (channel_id, sender_discord_id,
    /// sender_osl_user_id, ct, nonce, decrypted_at, burned)
    /// row tuple into a [`StoredMessage`] using the supplied
    /// `discord_message_id`. The id is the AAD for unsealing.
    fn materialize(
        &self,
        discord_message_id: &str,
        t: GetRowTuple,
    ) -> Result<StoredMessage, StoreError> {
        let (
            channel_id,
            sender_discord_id,
            sender_osl_user_id,
            ct,
            nonce,
            decrypted_at,
            burned_flag,
        ) = t;
        let aad = discord_message_id.as_bytes();
        let pt = cipher::unseal(&self.key, aad, &nonce, &ct)?;
        let plaintext = String::from_utf8(pt).map_err(|_| {
            StoreError::Corrupted("decoded plaintext is not valid UTF-8".to_string())
        })?;
        Ok(StoredMessage {
            discord_message_id: discord_message_id.to_string(),
            channel_id,
            sender_discord_id,
            sender_osl_user_id,
            plaintext,
            decrypted_at,
            burned: burned_flag != 0,
        })
    }
}

/// Row tuple shape for the seven-column `messages`-only fetch
/// (used by [`MessageStore::get`]). Columns:
/// `(channel_id, sender_discord_id, sender_osl_user_id,
///   ciphertext, nonce, decrypted_at, burned)`.
type GetRowTuple = (String, String, String, Vec<u8>, Vec<u8>, i64, i64);

/// Row tuple shape for the eight-column fetch (used by
/// [`MessageStore::list_by_channel`], which carries the
/// `discord_message_id` as column 0). Columns:
/// `(discord_message_id, channel_id, sender_discord_id,
///   sender_osl_user_id, ciphertext, nonce, decrypted_at, burned)`.
type FullRowTuple = (String, String, String, String, Vec<u8>, Vec<u8>, i64, i64);

/// Row mapper for the seven-column `messages`-only fetch (used
/// by `get`).
fn row_to_tuple(r: &rusqlite::Row<'_>) -> rusqlite::Result<GetRowTuple> {
    Ok((
        r.get::<_, String>(0)?,
        r.get::<_, String>(1)?,
        r.get::<_, String>(2)?,
        r.get::<_, Vec<u8>>(3)?,
        r.get::<_, Vec<u8>>(4)?,
        r.get::<_, i64>(5)?,
        r.get::<_, i64>(6)?,
    ))
}

/// Row mapper for the eight-column fetch (used by
/// `list_by_channel`, which carries the `discord_message_id`
/// as column 0).
fn full_row_to_tuple(r: &rusqlite::Row<'_>) -> rusqlite::Result<FullRowTuple> {
    Ok((
        r.get::<_, String>(0)?,
        r.get::<_, String>(1)?,
        r.get::<_, String>(2)?,
        r.get::<_, String>(3)?,
        r.get::<_, Vec<u8>>(4)?,
        r.get::<_, Vec<u8>>(5)?,
        r.get::<_, i64>(6)?,
        r.get::<_, i64>(7)?,
    ))
}

/// Materialize the eight-column tuple into a [`StoredMessage`].
fn materialize_full(
    key: &aead::Key,
    t: FullRowTuple,
    aad_id: &str,
) -> Result<StoredMessage, StoreError> {
    let (
        discord_message_id,
        channel_id,
        sender_discord_id,
        sender_osl_user_id,
        ct,
        nonce,
        decrypted_at,
        burned_flag,
    ) = t;
    let pt = cipher::unseal(key, aad_id.as_bytes(), &nonce, &ct)?;
    let plaintext = String::from_utf8(pt)
        .map_err(|_| StoreError::Corrupted("decoded plaintext is not valid UTF-8".to_string()))?;
    Ok(StoredMessage {
        discord_message_id,
        channel_id,
        sender_discord_id,
        sender_osl_user_id,
        plaintext,
        decrypted_at,
        burned: burned_flag != 0,
    })
}
