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
//! - Identifiers are **not** stored. Each row carries keyed blind indexes
//!   (`mid_bi`, `chan_bi`, `sender_bi`) for equality lookup, and a sealed
//!   `meta_ct` holding the real ids and timestamp. Ordering uses an opaque
//!   `seq` rather than a plaintext timestamp.
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
//! (info = `"osl-message-store-v1"`, salt empty); the blind-index key is a
//! second, domain-separated derivation from the same secret. Per-row AEAD
//! is `crypto::aead::seal` with a fresh random 24-byte nonce.
//!
//! ## Threading
//!
//! [`MessageStore`] is `Send + Sync`. The single underlying
//! [`rusqlite::Connection`] is wrapped in a `Mutex`; concurrent
//! callers serialize at the lock. SQLite WAL mode is enabled to
//! reduce write-lock contention with future readers.
//!
//! Burn is terminal: a `put` cannot restore a row that
//! [`MessageStore::mark_burned`] has destroyed.

mod cipher;
mod error;
mod schema;

pub use error::StoreError;

use cipher::{AttachmentMeta, MessageMeta};
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

    /// Separate HKDF derivation used only for blind indexes, never for
    /// encryption.
    index_key: [u8; 32],
}

/// Columns a `get` fetches: `(meta_nonce, meta_ct, ciphertext, nonce, burned)`.
type MessageRow = (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, i64, Vec<u8>, Vec<u8>);

/// Columns an attachment fetch returns:
/// `(meta_nonce, meta_ct, ciphertext, nonce, mid_bi, sender_bi)`.
type AttachmentRow = (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, Option<Vec<u8>>);

/// Flush destructive updates out of WAL and truncate the WAL file so
/// pre-burn page images are not left recoverable beside the database.
/// A non-zero busy result means another reader prevented the security
/// checkpoint; surface that instead of claiming a completed shred.
fn checkpoint_after_shred(conn: &Connection) -> Result<(), StoreError> {
    let (busy, _, _): (i64, i64, i64) =
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
    if busy != 0 {
        return Err(StoreError::Sealer(
            "burn shred could not truncate SQLite WAL because a reader is active".to_string(),
        ));
    }
    Ok(())
}

/// Destroy one row's secret material in place, unconditionally.
///
/// Deliberately **not** predicated on `burned = 0`. A row can carry
/// `burned = 1` and still hold an intact sealed body — every store written by
/// a build before this fix can contain one — and skipping the shred because a
/// flag was already set is how a destructive call came to report success over
/// a secret it never touched.
///
/// `burned_at` is stamped only if it is not already set, so re-burning cannot
/// make an old destruction look recent in the audit trail.
///
/// The caller is responsible for the WAL checkpoint; batching one checkpoint
/// after several shreds is why it is not done here.
fn shred_row(conn: &Connection, mid_bi: &[u8]) -> Result<usize, StoreError> {
    let rows = conn.execute(
        "UPDATE messages
            SET ciphertext = zeroblob(length(ciphertext)),
                nonce = zeroblob(length(nonce)),
                wrapped_key = NULL,
                burned = 1,
                burned_at = COALESCE(burned_at, strftime('%s','now'))
          WHERE mid_bi = ?1",
        params![mid_bi],
    )?;
    Ok(rows)
}

/// Refuse an identifier that would make a cache key or an AEAD associated-data
/// value ambiguous.
///
/// Validation rather than a wider separator is deliberate: the attachment body
/// AAD *is* the cache key and the message body AAD *is* the message id, and
/// both are already sealed into rows on disk. Changing either encoding would
/// orphan every existing sealed body, because the migration copies those bytes
/// verbatim — the same mistake that would have made every migrated attachment
/// undecryptable. Rejecting the input costs no format change.
fn check_id(field: &str, value: &str) -> Result<(), StoreError> {
    if value.contains('/') || value.contains('\0') {
        return Err(StoreError::InvalidId(format!(
            "{field} may not contain '/' or NUL"
        )));
    }
    Ok(())
}

/// Next ordering counter. `seq` replaces the plaintext `decrypted_at` index:
/// relative order is inherent to storing rows at all, whereas wall-clock
/// timing was a leak, so the leak goes and the ordering stays.
fn next_seq(conn: &Connection, table: &str) -> Result<i64, StoreError> {
    let sql = format!("SELECT COALESCE(MAX(seq), 0) + 1 FROM {table}");
    let seq: i64 = conn.query_row(&sql, [], |r| r.get(0))?;
    Ok(seq)
}

impl MessageStore {
    /// Open or create the message store at
    /// `<app_data_dir>/messages.sqlite`.
    ///
    /// Creates `app_data_dir` if it does not exist. Verifies the canary
    /// **before** running migrations — the v3→v4 step rewrites every row, and
    /// doing that under an unproven key would destroy the database of anyone
    /// who mistyped a password. Failure returns [`StoreError::Sealer`] (the
    /// wrong-`identity_secret` signal) without exposing any plaintext.
    pub fn open(app_data_dir: &Path, identity_secret: &[u8; 32]) -> Result<Self, StoreError> {
        std::fs::create_dir_all(app_data_dir)?;
        let path = app_data_dir.join("messages.sqlite");
        let conn = Connection::open(&path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        // Probe-4 fix: WAL's default synchronous=NORMAL is fast but
        // loses uncheckpointed writes on a hard kill (force-close,
        // OS crash, power loss). synchronous=FULL forces an fsync per
        // WAL frame so every persisted message survives a hard kill.
        conn.pragma_update(None, "synchronous", "FULL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        // Burn operations overwrite or delete secret-bearing rows.
        // Ask SQLite to scrub discarded cell content rather than
        // leaving recoverable copies in free database pages.
        conn.pragma_update(None, "secure_delete", "ON")?;

        schema::ensure_meta_table(&conn)?;

        let key = cipher::derive_key(identity_secret)?;
        let index_key = cipher::derive_index_key(identity_secret)?;
        schema::check_canary(&conn, &key)?;
        schema::migrate(&conn, &key, &index_key)?;

        Ok(MessageStore {
            conn: Mutex::new(conn),
            key,
            index_key,
        })
    }

    fn bi(&self, domain: &[u8], value: &str) -> Result<Vec<u8>, StoreError> {
        cipher::blind_index(&self.index_key, domain, value)
    }

    /// Insert or replace a message in the store.
    ///
    /// Sealing happens inside this call: `msg.plaintext` is
    /// AEAD-encrypted under the derived key, AAD =
    /// `msg.discord_message_id` bytes, with a fresh random
    /// nonce per call.
    ///
    /// ## Burn is terminal
    ///
    /// A `put` targeting a row that is already burned does **not** restore it.
    /// The upsert carries `WHERE messages.burned = 0`, so a burned row keeps
    /// its zeroed body and its burn flag, and the call is a silent no-op.
    ///
    /// This matters because it is ordinary behaviour, not an attack: the
    /// receive observer re-decrypts a channel's history on every re-entry and
    /// re-`put`s the same snowflakes. Before this predicate existed, one
    /// re-entry after a burn wrote the sealed body straight back onto disk and
    /// cleared `burned`, which made `THREAT_MODEL.md`'s "the local cached
    /// plaintext of those messages is gone" false in normal use.
    ///
    /// A caller that hands in `burned: true` gets a **shredded** row: the
    /// body is never written. Writing a live body under a burned flag was the
    /// precondition that let `mark_burned` report success over an intact
    /// secret.
    ///
    /// ## Ordering on re-put
    ///
    /// An existing row keeps its original `seq`. Re-decrypting history on
    /// channel re-entry therefore no longer shuffles that history to the top
    /// of the channel view.
    pub fn put(&self, msg: &StoredMessage) -> Result<(), StoreError> {
        check_id("discord_message_id", &msg.discord_message_id)?;
        let mid_bi = self.bi(cipher::BI_MESSAGE_ID, &msg.discord_message_id)?;
        let chan_bi = self.bi(cipher::BI_CHANNEL_ID, &msg.channel_id)?;
        let sender_bi = self.bi(cipher::BI_SENDER_ID, &msg.sender_discord_id)?;
        let meta = MessageMeta {
            discord_message_id: msg.discord_message_id.clone(),
            channel_id: msg.channel_id.clone(),
            sender_discord_id: msg.sender_discord_id.clone(),
            sender_osl_user_id: msg.sender_osl_user_id.clone(),
            decrypted_at: msg.decrypted_at,
        };
        let (meta_nonce, meta_ct) =
            cipher::seal(&self.key, &mid_bi, &cipher::encode_message_meta(&meta))?;

        let conn = self.conn.lock().expect("store mutex poisoned");
        let seq = next_seq(&conn, "messages")?;

        if msg.burned {
            conn.execute(
                "INSERT INTO messages \
                    (mid_bi, chan_bi, sender_bi, meta_nonce, meta_ct, \
                     ciphertext, nonce, seq, burned, burned_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, x'', x'', ?6, 1, strftime('%s','now')) \
                 ON CONFLICT(mid_bi) DO NOTHING",
                params![mid_bi, chan_bi, sender_bi, meta_nonce, meta_ct, seq],
            )?;
            shred_row(&conn, &mid_bi)?;
            checkpoint_after_shred(&conn)?;
            return Ok(());
        }

        let (nonce, ct) = cipher::seal(
            &self.key,
            msg.discord_message_id.as_bytes(),
            msg.plaintext.as_bytes(),
        )?;
        conn.execute(
            "INSERT INTO messages \
                (mid_bi, chan_bi, sender_bi, meta_nonce, meta_ct, \
                 ciphertext, nonce, seq, burned) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0) \
             ON CONFLICT(mid_bi) DO UPDATE SET \
                chan_bi = excluded.chan_bi, \
                sender_bi = excluded.sender_bi, \
                meta_nonce = excluded.meta_nonce, \
                meta_ct = excluded.meta_ct, \
                ciphertext = excluded.ciphertext, \
                nonce = excluded.nonce \
             WHERE messages.burned = 0",
            params![mid_bi, chan_bi, sender_bi, meta_nonce, meta_ct, ct, nonce, seq],
        )?;
        Ok(())
    }

    /// Look up a single message by its Discord snowflake.
    ///
    /// Returns `Ok(None)` if the row does not exist OR if it is
    /// burned. Burned rows are filtered at the SQL level so
    /// callers can't accidentally surface them.
    pub fn get(&self, discord_message_id: &str) -> Result<Option<StoredMessage>, StoreError> {
        check_id("discord_message_id", discord_message_id)?;
        let mid_bi = self.bi(cipher::BI_MESSAGE_ID, discord_message_id)?;
        let conn = self.conn.lock().expect("store mutex poisoned");
        let row_opt: Option<MessageRow> = conn
            .query_row(
                "SELECT meta_nonce, meta_ct, ciphertext, nonce, burned, chan_bi, sender_bi \
                 FROM messages WHERE mid_bi = ?1 AND burned = 0",
                params![mid_bi],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                    ))
                },
            )
            .optional()?;
        let Some(row) = row_opt else { return Ok(None) };
        Ok(Some(self.materialize(&mid_bi, row)?))
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
        let chan_bi = self.bi(cipher::BI_CHANNEL_ID, channel_id)?;
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT mid_bi, meta_nonce, meta_ct, ciphertext, nonce, burned, chan_bi, sender_bi \
             FROM messages \
             WHERE chan_bi = ?1 AND burned = 0 \
             ORDER BY seq DESC, mid_bi DESC \
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![chan_bi, i64::from(limit)], |r| {
            Ok((
                r.get::<_, Vec<u8>>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, Vec<u8>>(2)?,
                r.get::<_, Vec<u8>>(3)?,
                r.get::<_, Vec<u8>>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, Vec<u8>>(6)?,
                r.get::<_, Vec<u8>>(7)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (mid_bi, meta_nonce, meta_ct, ct, nonce, burned, chan_bi, sender_bi) = row?;
            out.push(self.materialize(
                &mid_bi,
                (meta_nonce, meta_ct, ct, nonce, burned, chan_bi, sender_bi),
            )?);
        }
        Ok(out)
    }

    /// Mark a message burned and cryptographically shred the local
    /// ciphertext. Subsequent `get` returns `Ok(None)` and
    /// `list_by_channel` filters the audit-stub row out.
    ///
    /// Returns [`StoreError::NotFound`] if no row exists for
    /// `discord_message_id`.
    ///
    /// ## Why this does not short-circuit on `burned = 1`
    ///
    /// It used to return `Ok(())` as soon as the flag was set, on the
    /// assumption that a burned row had already been shredded. That assumption
    /// does not hold: a row can carry `burned = 1` over an intact sealed body,
    /// and every database written by a build before this fix may contain one.
    /// The call then reported a completed destruction while the secret sat on
    /// disk. Shredding unconditionally is cheap and removes the assumption
    /// rather than documenting it.
    pub fn mark_burned(&self, discord_message_id: &str) -> Result<(), StoreError> {
        let mid_bi = self.bi(cipher::BI_MESSAGE_ID, discord_message_id)?;
        let conn = self.conn.lock().expect("store mutex poisoned");
        let exists: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM messages WHERE mid_bi = ?1",
                params![mid_bi],
                |r| r.get(0),
            )
            .optional()?;
        if exists.is_none() {
            return Err(StoreError::NotFound(discord_message_id.to_string()));
        }
        shred_row(&conn, &mid_bi)?;
        // The row's cached attachment plaintext is part of the message, so a
        // burn that left the picture behind would not be a burn.
        conn.execute("DELETE FROM attachments WHERE mid_bi = ?1", params![mid_bi])?;
        checkpoint_after_shred(&conn)?;
        Ok(())
    }

    /// Shred every message in a scope, marking the rows burned.
    ///
    /// `only_sender_discord_id`: when `Some(id)`, ONLY rows from that sender
    /// are wiped — so a burn destroys the burner's OWN messages without nuking
    /// everyone else's in the channel. When `None`, every row in the scope is
    /// wiped (full-scope destruction, e.g. account burn).
    ///
    /// Returns the number of rows touched.
    ///
    /// ## What `scope_id` matches
    ///
    /// Nothing in this repository has ever written the old `scope_type` /
    /// `scope_id` columns, so a predicate over them matched **zero**
    /// normally-written rows: this call destroyed nothing and returned 0 while
    /// its caller reported a successful burn. Those columns are gone in v4 —
    /// a column that is never written is a lie about capability — and matching
    /// runs against the channel blind index, whose value `scope_id` is for the
    /// channel-shaped scopes (`dm`, `gc`, `server_channel`).
    ///
    /// A server-wide `scope_id` matches no channel, so this cannot
    /// over-delete: snowflakes are unique. `scope_type` is retained in the
    /// signature for caller compatibility.
    ///
    /// This does not make burn cryptographic. It destroys the **local cached
    /// plaintext**, which is what `THREAT_MODEL.md:160-162` claims burn
    /// achieves locally and no more.
    pub fn wipe_wrapped_keys_in_scope(
        &self,
        _scope_type: &str,
        scope_id: &str,
        only_sender_discord_id: Option<&str>,
    ) -> Result<usize, StoreError> {
        let chan_bi = self.bi(cipher::BI_CHANNEL_ID, scope_id)?;
        let conn = self.conn.lock().expect("store mutex poisoned");
        let rows = match only_sender_discord_id {
            Some(sender) => {
                let sender_bi = self.bi(cipher::BI_SENDER_ID, sender)?;
                conn.execute(
                    "UPDATE messages \
                        SET ciphertext = zeroblob(length(ciphertext)), \
                            nonce = zeroblob(length(nonce)), wrapped_key = NULL, \
                            burned = 1, \
                            burned_at = COALESCE(burned_at, strftime('%s','now')) \
                      WHERE chan_bi = ?1 AND sender_bi = ?2",
                    params![chan_bi, sender_bi],
                )?
            }
            None => conn.execute(
                "UPDATE messages \
                    SET ciphertext = zeroblob(length(ciphertext)), \
                        nonce = zeroblob(length(nonce)), wrapped_key = NULL, \
                        burned = 1, \
                        burned_at = COALESCE(burned_at, strftime('%s','now')) \
                  WHERE chan_bi = ?1",
                params![chan_bi],
            )?,
        };
        checkpoint_after_shred(&conn)?;
        Ok(rows)
    }

    /// Full data destruction for a channel: remove every row for `channel_id`
    /// entirely, along with the cached attachments those rows own.
    ///
    /// `secure_delete=ON` scrubs freed database cells and an immediate
    /// truncating checkpoint removes pre-delete page images from WAL.
    ///
    /// Returns the message row count for diagnostic logging.
    pub fn delete_messages_in_channel(&self, channel_id: &str) -> Result<usize, StoreError> {
        let chan_bi = self.bi(cipher::BI_CHANNEL_ID, channel_id)?;
        let conn = self.conn.lock().expect("store mutex poisoned");
        // Drop the cached attachment plaintext FIRST, while the message rows
        // that identify it still exist. Deleting the messages first orphans
        // those attachment rows: nothing then links them to a channel, so no
        // later burn predicate can find them and `get_attachment` keeps
        // serving the decrypted bytes of a message that no longer exists.
        conn.execute(
            "DELETE FROM attachments WHERE mid_bi IN \
                (SELECT mid_bi FROM messages WHERE chan_bi = ?1)",
            params![chan_bi],
        )?;
        let rows = conn.execute("DELETE FROM messages WHERE chan_bi = ?1", params![chan_bi])?;
        checkpoint_after_shred(&conn)?;
        Ok(rows)
    }

    /// Persist a decrypted attachment's bytes, sealed at rest
    /// (AAD = the cache key's blind index), so a channel re-entry or app
    /// restart can rehydrate the image/file without re-fetching from the CDN
    /// and re-decrypting. `cache_key` is
    /// `"<discord_message_id>/<random_filename>"`. Insert-or-replace keyed on
    /// that key. The caller is expected to cap the size it hands in.
    #[allow(clippy::too_many_arguments)]
    pub fn put_attachment(
        &self,
        discord_message_id: &str,
        random_filename: &str,
        mime: &str,
        plaintext: &[u8],
        // Scope + sender so a burn can wipe the burner's cached
        // attachments precisely. `None` is tolerated (legacy callers /
        // unknown context); such rows are still reached through their
        // parent message.
        scope_type: Option<&str>,
        scope_id: Option<&str>,
        sender_discord_id: Option<&str>,
    ) -> Result<(), StoreError> {
        check_id("discord_message_id", discord_message_id)?;
        check_id("random_filename", random_filename)?;
        let cache_key = format!("{discord_message_id}/{random_filename}");
        let ck_bi = self.bi(cipher::BI_CACHE_KEY, &cache_key)?;
        let mid_bi = self.bi(cipher::BI_MESSAGE_ID, discord_message_id)?;
        let sender_bi = match sender_discord_id {
            Some(s) => Some(self.bi(cipher::BI_SENDER_ID, s)?),
            None => None,
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let meta = AttachmentMeta {
            cache_key: cache_key.clone(),
            discord_message_id: discord_message_id.to_string(),
            random_filename: random_filename.to_string(),
            mime: mime.to_string(),
            byte_len: plaintext.len() as i64,
            created_at: now,
            scope_type: scope_type.map(str::to_string),
            scope_id: scope_id.map(str::to_string),
            sender_discord_id: sender_discord_id.map(str::to_string),
        };
        let (meta_nonce, meta_ct) =
            cipher::seal(&self.key, &ck_bi, &cipher::encode_attachment_meta(&meta))?;
        // The attachment BODY keeps `cache_key` as its AAD, exactly as message
        // bodies keep `discord_message_id`. Switching it to `ck_bi` would have
        // been tidier and would have silently destroyed every cached
        // attachment on upgrade: the v3→v4 migration copies these bytes
        // verbatim, so a body sealed under the old AAD must stay openable
        // under the old AAD.
        let (nonce, ct) = cipher::seal(&self.key, cache_key.as_bytes(), plaintext)?;

        let conn = self.conn.lock().expect("store mutex poisoned");
        let seq = next_seq(&conn, "attachments")?;
        conn.execute(
            "INSERT INTO attachments \
                (ck_bi, mid_bi, sender_bi, meta_nonce, meta_ct, ciphertext, nonce, seq) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
             ON CONFLICT(ck_bi) DO UPDATE SET \
                mid_bi = excluded.mid_bi, \
                sender_bi = excluded.sender_bi, \
                meta_nonce = excluded.meta_nonce, \
                meta_ct = excluded.meta_ct, \
                ciphertext = excluded.ciphertext, \
                nonce = excluded.nonce",
            params![ck_bi, mid_bi, sender_bi, meta_nonce, meta_ct, ct, nonce, seq],
        )?;
        Ok(())
    }

    /// Burn: delete cached attachments for a scope, optionally limited
    /// to one sender (the burner).
    ///
    /// ## Why it resolves through the message rows
    ///
    /// `put_attachment` accepts `None` for scope and sender, and the shipping
    /// caller passes `None` whenever it cannot resolve the scope. Those rows
    /// match no scope predicate of their own, so a scope burn used to leave the
    /// decrypted picture in the cache where `get_attachment` still served it —
    /// the text was destroyed and the image was not.
    ///
    /// An attachment therefore belongs to a scope if its parent message does.
    /// That covers every legacy row whose message is still present, and keeps
    /// the sender restriction honest because the sender is read off the
    /// message rather than the attachment's own (possibly absent) column.
    ///
    /// Residue this does not reach: an attachment whose message row was
    /// already deleted has no remaining link to any scope. Nothing can
    /// attribute it. [`Self::delete_messages_in_channel`] deletes attachments
    /// before their messages so that orphan is no longer created.
    pub fn wipe_attachments_in_scope(
        &self,
        _scope_type: &str,
        scope_id: &str,
        only_sender_discord_id: Option<&str>,
    ) -> Result<usize, StoreError> {
        let chan_bi = self.bi(cipher::BI_CHANNEL_ID, scope_id)?;
        let conn = self.conn.lock().expect("store mutex poisoned");
        let rows = match only_sender_discord_id {
            Some(sender) => {
                let sender_bi = self.bi(cipher::BI_SENDER_ID, sender)?;
                conn.execute(
                    "DELETE FROM attachments WHERE mid_bi IN ( \
                        SELECT mid_bi FROM messages \
                         WHERE chan_bi = ?1 AND sender_bi = ?2)",
                    params![chan_bi, sender_bi],
                )?
            }
            None => conn.execute(
                "DELETE FROM attachments WHERE mid_bi IN ( \
                    SELECT mid_bi FROM messages WHERE chan_bi = ?1)",
                params![chan_bi],
            )?,
        };
        checkpoint_after_shred(&conn)?;
        Ok(rows)
    }

    /// Fetch a previously-persisted decrypted attachment.
    /// Returns `(mime, plaintext_bytes)` or `None` if not cached.
    pub fn get_attachment(
        &self,
        discord_message_id: &str,
        random_filename: &str,
    ) -> Result<Option<(String, Vec<u8>)>, StoreError> {
        check_id("discord_message_id", discord_message_id)?;
        check_id("random_filename", random_filename)?;
        let cache_key = format!("{discord_message_id}/{random_filename}");
        let ck_bi = self.bi(cipher::BI_CACHE_KEY, &cache_key)?;
        let conn = self.conn.lock().expect("store mutex poisoned");
        let row_opt: Option<AttachmentRow> = conn
            .query_row(
                "SELECT meta_nonce, meta_ct, ciphertext, nonce, mid_bi, sender_bi \
                   FROM attachments \
                 WHERE ck_bi = ?1",
                params![ck_bi],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                    ))
                },
            )
            .optional()?;
        let Some((meta_nonce, meta_ct, ct, nonce, mid_bi, sender_bi)) = row_opt else {
            return Ok(None);
        };
        let meta_bytes = cipher::unseal(&self.key, &ck_bi, &meta_nonce, &meta_ct)?;
        let meta = cipher::decode_attachment_meta(&meta_bytes)?;

        let expect_ck = self.bi(cipher::BI_CACHE_KEY, &meta.cache_key)?;
        if expect_ck != ck_bi || meta.cache_key != cache_key {
            return Err(StoreError::Corrupted(
                "attachment cache selector does not match its sealed metadata".to_string(),
            ));
        }
        let expect_mid = self.bi(cipher::BI_MESSAGE_ID, &meta.discord_message_id)?;
        if expect_mid != mid_bi {
            return Err(StoreError::Corrupted(
                "attachment message selector does not match its sealed metadata".to_string(),
            ));
        }
        let expect_sender = match meta.sender_discord_id.as_deref() {
            Some(sender) => Some(self.bi(cipher::BI_SENDER_ID, sender)?),
            None => None,
        };
        if expect_sender != sender_bi {
            return Err(StoreError::Corrupted(
                "attachment sender selector does not match its sealed metadata".to_string(),
            ));
        }
        // Body AAD is the plaintext cache key — see `put_attachment`.
        let pt = cipher::unseal(&self.key, cache_key.as_bytes(), &nonce, &ct)?;
        Ok(Some((meta.mime, pt)))
    }

    /// Bound the attachments table's disk footprint by trimming oldest rows
    /// beyond `keep`. Best-effort; called occasionally by the caller. Returns
    /// rows deleted.
    pub fn trim_attachments(&self, keep: u32) -> Result<usize, StoreError> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let rows = conn.execute(
            "DELETE FROM attachments WHERE ck_bi NOT IN \
                (SELECT ck_bi FROM attachments ORDER BY seq DESC, ck_bi DESC LIMIT ?1)",
            params![keep],
        )?;
        Ok(rows)
    }

    /// Delete cached attachments for one message (used by scope burn so
    /// burned images don't linger in the cache).
    pub fn delete_attachments_for_message(
        &self,
        discord_message_id: &str,
    ) -> Result<usize, StoreError> {
        let mid_bi = self.bi(cipher::BI_MESSAGE_ID, discord_message_id)?;
        let conn = self.conn.lock().expect("store mutex poisoned");
        let rows = conn.execute("DELETE FROM attachments WHERE mid_bi = ?1", params![mid_bi])?;
        checkpoint_after_shred(&conn)?;
        Ok(rows)
    }

    /// Shred the cached plaintext of every named row whose timed-deletion
    /// deadline has elapsed, and drop its cached attachments.
    ///
    /// ## Why the caller names the rows
    ///
    /// Expiry lives in the receiver's sealed open-clock ledger, which is the
    /// only place that knows both clocks and the first-open timestamp. So the
    /// sweeper decides *which* rows died and this method destroys them.
    ///
    /// ## Semantics
    ///
    /// Same destruction as [`Self::mark_burned`] — zero the ciphertext and
    /// nonce in place, null the wrapped key, and stamp `burned`/`burned_at` —
    /// followed by a single WAL truncation for the whole batch.
    ///
    /// Unlike `mark_burned`, an id that is absent or already burned is *not* an
    /// error: a sweeper legitimately names rows this device never cached. It
    /// returns the number of rows it actually shredded, so a caller can report
    /// real work rather than an intention.
    ///
    /// `ids` is bounded by the caller. Passing an empty slice touches nothing
    /// and does not checkpoint.
    pub fn shred_expired_messages(&self, ids: &[String]) -> Result<usize, StoreError> {
        if ids.is_empty() {
            return Ok(0);
        }
        let blinded: Vec<Vec<u8>> = ids
            .iter()
            .map(|id| self.bi(cipher::BI_MESSAGE_ID, id))
            .collect::<Result<_, _>>()?;
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut shredded = 0usize;
        for mid_bi in &blinded {
            // `burned = 0` in the predicate keeps this idempotent: a second
            // sweep over the same id reports zero rather than re-stamping
            // `burned_at` and making an old destruction look fresh.
            shredded += conn.execute(
                "UPDATE messages
                    SET ciphertext = zeroblob(length(ciphertext)),
                        nonce = zeroblob(length(nonce)),
                        wrapped_key = NULL,
                        burned = 1,
                        burned_at = strftime('%s','now')
                  WHERE mid_bi = ?1 AND burned = 0",
                params![mid_bi],
            )?;
            // Expiry destroys local plaintext caches, and a decrypted
            // attachment is one.
            conn.execute("DELETE FROM attachments WHERE mid_bi = ?1", params![mid_bi])?;
        }
        checkpoint_after_shred(&conn)?;
        Ok(shredded)
    }

    /// Turn a stored row back into a [`StoredMessage`], recovering the
    /// identifiers from the sealed metadata blob.
    ///
    /// The metadata is sealed with the row's own `mid_bi` as AAD, so it is
    /// authenticated as well as hidden: an offline editor who moves a blob
    /// between rows, or edits one, produces a tag failure rather than a
    /// forged attribution.
    fn materialize(&self, mid_bi: &[u8], row: MessageRow) -> Result<StoredMessage, StoreError> {
        let (meta_nonce, meta_ct, ct, nonce, burned_flag, chan_bi, sender_bi) = row;
        let meta_bytes = cipher::unseal(&self.key, mid_bi, &meta_nonce, &meta_ct)?;
        let meta = cipher::decode_message_meta(&meta_bytes)?;

        // Re-derive the selector columns and check them against what is on
        // disk.
        //
        // Sealing the metadata with `mid_bi` as AAD authenticates the blob and
        // the message id, but it authenticates NOTHING ELSE: `chan_bi` and
        // `sender_bi` are separate columns that no AEAD covers. Without this
        // check, someone who can write the file can retarget a row — move a
        // message into another conversation's view by rewriting `chan_bi`, or
        // make a sender-scoped burn silently skip a row by rewriting
        // `sender_bi`. They cannot read it, but they can misfile it and they
        // can defeat a burn.
        //
        // No format change is needed to close this: after unsealing we hold
        // both the plaintext identifiers and the index key, so the honest
        // values are recomputable and a mismatch is proof of tampering.
        let expect_mid = self.bi(cipher::BI_MESSAGE_ID, &meta.discord_message_id)?;
        if expect_mid != mid_bi {
            return Err(StoreError::Corrupted(
                "row message selector does not match its sealed metadata — \
                 the row was retargeted on disk"
                    .to_string(),
            ));
        }
        let expect_chan = self.bi(cipher::BI_CHANNEL_ID, &meta.channel_id)?;
        if expect_chan != chan_bi {
            return Err(StoreError::Corrupted(
                "row channel selector does not match its sealed metadata — \
                 the row was retargeted on disk"
                    .to_string(),
            ));
        }
        let expect_sender = self.bi(cipher::BI_SENDER_ID, &meta.sender_discord_id)?;
        if expect_sender != sender_bi {
            return Err(StoreError::Corrupted(
                "row sender selector does not match its sealed metadata — \
                 the row was retargeted on disk"
                    .to_string(),
            ));
        }
        let pt = cipher::unseal(&self.key, meta.discord_message_id.as_bytes(), &nonce, &ct)?;
        let plaintext = String::from_utf8(pt).map_err(|_| {
            StoreError::Corrupted("decoded plaintext is not valid UTF-8".to_string())
        })?;
        Ok(StoredMessage {
            discord_message_id: meta.discord_message_id,
            channel_id: meta.channel_id,
            sender_discord_id: meta.sender_discord_id,
            sender_osl_user_id: meta.sender_osl_user_id,
            plaintext,
            decrypted_at: meta.decrypted_at,
            burned: burned_flag != 0,
        })
    }
}
