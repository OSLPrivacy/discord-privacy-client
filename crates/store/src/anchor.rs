//! Optional external monotonic anchor for rollback detection.
//!
//! The provider is deliberately outside this crate (TPM, keystore service,
//! hardware counter, etc.).  A second file beside SQLite would be replayable
//! with the database and is therefore not an anchor.

use crate::{cipher, StoreError};
use rusqlite::{params, types::ValueRef, Connection, OptionalExtension, Transaction};
use std::sync::{Arc, Mutex};

const GENERATION_KEY: &str = "anchor_generation";
const DIGEST_KEY: &str = "anchor_digest";

/// A provider-held, monotonic statement about one Store instance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnchorRecord {
    pub generation: u64,
    pub digest: [u8; 32],
}

/// External durable compare-and-advance authority.
///
/// Implementations must make `compare_and_advance` atomic and durable across
/// process restart.  This crate intentionally supplies no file-backed default:
/// a colocated file can be restored with `messages.sqlite` and provides no
/// rollback protection.
pub trait MonotonicAnchor: Send + Sync {
    fn load(&self, store_id: [u8; 32]) -> Result<Option<AnchorRecord>, StoreError>;
    fn compare_and_advance(
        &self,
        store_id: [u8; 32],
        expected: Option<AnchorRecord>,
        next: AnchorRecord,
    ) -> Result<(), StoreError>;
}

pub(crate) struct AnchorBinding {
    provider: Arc<dyn MonotonicAnchor>,
    store_id: [u8; 32],
    digest_key: [u8; 32],
    current: Mutex<AnchorRecord>,
}

impl AnchorBinding {
    pub(crate) fn enroll_or_reconcile(
        conn: &Connection,
        identity_secret: &[u8; 32],
        provider: Arc<dyn MonotonicAnchor>,
    ) -> Result<Self, StoreError> {
        let (store_id, digest_key) = cipher::derive_anchor_material(identity_secret)?;
        let local = record(conn, &digest_key)?;
        let remote = provider.load(store_id)?;
        let current =
            match (local, remote) {
                (None, None) => {
                    let next = AnchorRecord {
                        generation: 1,
                        digest: digest(conn, &digest_key)?,
                    };
                    let tx = conn.unchecked_transaction()?;
                    write_record(&tx, &next)?;
                    tx.commit()?;
                    // A crash after the DB commit is recovered by the next open:
                    // local is exactly one generation ahead of an absent remote.
                    provider.compare_and_advance(store_id, None, next.clone())?;
                    next
                }
                (None, Some(_)) => return Err(StoreError::Anchor(
                    "database has no anchor record but provider already has state; refusing replay"
                        .to_string(),
                )),
                (Some(local), None) => {
                    if local.generation != 1 {
                        return Err(StoreError::Anchor(
                            "provider lost anchored generation; refusing rollback".to_string(),
                        ));
                    }
                    provider.compare_and_advance(store_id, None, local.clone())?;
                    local
                }
                (Some(local), Some(remote)) => {
                    if local.generation == remote.generation && local.digest == remote.digest {
                        local
                    } else if local.generation == remote.generation.saturating_add(1) {
                        // Database COMMIT succeeded but the process died before
                        // (or during an ambiguous) external CAS.  Only this exact
                        // one-step state is recoverable.
                        provider.compare_and_advance(store_id, Some(remote), local.clone())?;
                        local
                    } else if local.generation < remote.generation {
                        return Err(StoreError::Anchor(
                        "database generation is behind external anchor; refusing coherent replay"
                            .to_string(),
                    ));
                    } else {
                        return Err(StoreError::Anchor(
                            "database and external anchor disagree".to_string(),
                        ));
                    }
                }
            };
        Ok(Self {
            provider,
            store_id,
            digest_key,
            current: Mutex::new(current),
        })
    }

    pub(crate) fn commit(&self, tx: Transaction<'_>) -> Result<(), StoreError> {
        let mut current = self.current.lock().expect("anchor mutex poisoned");
        let next = AnchorRecord {
            generation: current
                .generation
                .checked_add(1)
                .ok_or_else(|| StoreError::Anchor("generation counter overflow".to_string()))?,
            digest: digest(&tx, &self.digest_key)?,
        };
        write_record(&tx, &next)?;
        tx.commit()?;
        // Do not report operation success before the external monotonic state
        // has advanced.  If this returns an error after a durable CAS, reopen
        // observes matching records; if it errors before CAS, reopen performs
        // the exact one-step recovery above.
        self.provider
            .compare_and_advance(self.store_id, Some(current.clone()), next.clone())?;
        *current = next;
        Ok(())
    }
}

fn record(conn: &Connection, digest_key: &[u8; 32]) -> Result<Option<AnchorRecord>, StoreError> {
    let generation: Option<Vec<u8>> = conn
        .query_row(
            "SELECT value FROM _meta WHERE key = ?1",
            params![GENERATION_KEY],
            |r| r.get(0),
        )
        .optional()?;
    let digest_bytes: Option<Vec<u8>> = conn
        .query_row(
            "SELECT value FROM _meta WHERE key = ?1",
            params![DIGEST_KEY],
            |r| r.get(0),
        )
        .optional()?;
    match (generation, digest_bytes) {
        (None, None) => Ok(None),
        (Some(generation), Some(digest_bytes)) => {
            let generation: [u8; 8] = generation.try_into().map_err(|_| {
                StoreError::Anchor("anchor generation has invalid length".to_string())
            })?;
            let stored_digest: [u8; 32] = digest_bytes
                .try_into()
                .map_err(|_| StoreError::Anchor("anchor digest has invalid length".to_string()))?;
            let expected = digest(conn, digest_key)?;
            if stored_digest != expected {
                return Err(StoreError::Anchor(
                    "database anchor digest does not bind current state".to_string(),
                ));
            }
            Ok(Some(AnchorRecord {
                generation: u64::from_le_bytes(generation),
                digest: stored_digest,
            }))
        }
        _ => Err(StoreError::Anchor(
            "partial anchor record; refusing crash/tamper ambiguity".to_string(),
        )),
    }
}

fn write_record(tx: &Transaction<'_>, record: &AnchorRecord) -> Result<(), StoreError> {
    tx.execute(
        "INSERT INTO _meta(key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![GENERATION_KEY, record.generation.to_le_bytes().to_vec()],
    )?;
    tx.execute(
        "INSERT INTO _meta(key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![DIGEST_KEY, record.digest.to_vec()],
    )?;
    Ok(())
}

fn push(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
    out.extend_from_slice(bytes);
}

fn push_value(out: &mut Vec<u8>, value: ValueRef<'_>) {
    match value {
        ValueRef::Null => out.push(0),
        ValueRef::Integer(value) => {
            out.push(1);
            push(out, &value.to_le_bytes());
        }
        ValueRef::Real(value) => {
            out.push(2);
            push(out, &value.to_le_bytes());
        }
        ValueRef::Text(value) => {
            out.push(3);
            push(out, value);
        }
        ValueRef::Blob(value) => {
            out.push(4);
            push(out, value);
        }
    }
}

/// Canonical semantic state digest.  It deliberately excludes only its own
/// two record fields; schema version, canary, recovery markers, every message
/// envelope, attachment envelope, and manifest are included.
fn digest(conn: &Connection, digest_key: &[u8; 32]) -> Result<[u8; 32], StoreError> {
    let mut bytes = b"osl-store-anchor/state-v1".to_vec();
    for (table, sql) in [
        ("_meta", "SELECT key, value FROM _meta WHERE key NOT IN ('anchor_generation', 'anchor_digest') ORDER BY key"),
        ("messages", "SELECT mid_bi, chan_bi, sender_bi, meta_nonce, meta_ct, ciphertext, nonce, seq, burned, content_version, wrapped_key_nonce, wrapped_key, discord_message_id, channel_id, sender_discord_id, sender_osl_user_id, decrypted_at, scope_type, scope_id, meta_tag FROM messages ORDER BY mid_bi"),
        ("attachments", "SELECT ck_bi, mid_bi, sender_bi, meta_nonce, meta_ct, ciphertext, nonce, seq, burned, content_version, wrapped_key_nonce, wrapped_key, cache_key, discord_message_id, random_filename, mime, byte_len, created_at, scope_type, scope_id, sender_discord_id FROM attachments ORDER BY ck_bi"),
        ("attachment_manifests", "SELECT mid_bi, complete, generation, nonce, ciphertext FROM attachment_manifests ORDER BY mid_bi"),
    ] {
        push(&mut bytes, table.as_bytes());
        let mut stmt = conn.prepare(sql)?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            bytes.push(0xff);
            for column in 0..row.as_ref().column_count() {
                push_value(&mut bytes, row.get_ref(column)?);
            }
        }
        bytes.push(0);
    }
    cipher::anchor_digest(digest_key, &bytes)
}
