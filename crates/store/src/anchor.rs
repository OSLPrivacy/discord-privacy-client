//! Optional external monotonic anchor for rollback detection.
//!
//! The provider is deliberately outside this crate (TPM, keystore service,
//! hardware counter, etc.).  A second file beside SQLite would be replayable
//! with the database and is therefore not an anchor.

use crate::{cipher, schema, StoreError};
use crypto::random;
use rusqlite::{params, types::ValueRef, Connection, OptionalExtension, Transaction};
use std::sync::{Arc, Mutex};

const GENERATION_KEY: &str = "anchor_generation";
const DIGEST_KEY: &str = "anchor_digest";
const MIGRATION_JOURNAL_KEY: &str = "anchor_migration_v1";
const MIGRATION_MAGIC: &[u8; 4] = b"AMJ1";
const MIGRATION_PLAN_DOMAIN: &[u8] = b"osl-store-anchor/migration-plan/v1";
const MIGRATION_FROM_V7: u32 = 7;
const MIGRATION_TO_V8: u32 = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MigrationPhase {
    Prepared,
    /// The v7→v8 schema transaction is externally anchored and the physical
    /// VACUUM is still owed.
    VacuumV8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MigrationJournal {
    store_id: [u8; 32],
    from: u32,
    to: u32,
    plan: [u8; 32],
    nonce: [u8; 32],
    phase: MigrationPhase,
}

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
    pub(crate) fn migration_pending(conn: &Connection) -> Result<bool, StoreError> {
        Ok(read_journal(conn)?.is_some())
    }

    /// Reconcile an already-enrolled Store without creating a record. `None`
    /// means both local and provider records are absent, which callers must
    /// treat as unanchored compatibility mode rather than evidence of an old
    /// generation.
    pub(crate) fn reconcile_existing(
        conn: &Connection,
        identity_secret: &[u8; 32],
        provider: Arc<dyn MonotonicAnchor>,
    ) -> Result<Option<Self>, StoreError> {
        let (store_id, digest_key) = cipher::derive_anchor_material(identity_secret)?;
        let local = record(conn, &digest_key)?;
        let remote = provider.load(store_id)?;
        let current =
            match (local, remote) {
                (None, None) => return Ok(None),
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
        Ok(Some(Self {
            provider,
            store_id,
            digest_key,
            current: Mutex::new(current),
        }))
    }

    pub(crate) fn enroll_or_reconcile(
        conn: &Connection,
        identity_secret: &[u8; 32],
        provider: Arc<dyn MonotonicAnchor>,
    ) -> Result<Self, StoreError> {
        if let Some(binding) = Self::reconcile_existing(conn, identity_secret, provider.clone())? {
            return Ok(binding);
        }
        let (store_id, digest_key) = cipher::derive_anchor_material(identity_secret)?;
        let next = AnchorRecord {
            generation: 1,
            digest: digest(conn, &digest_key)?,
        };
        let tx = conn.unchecked_transaction()?;
        write_record(&tx, &next)?;
        tx.commit()?;
        provider.compare_and_advance(store_id, None, next.clone())?;
        Ok(Self {
            provider,
            store_id,
            digest_key,
            current: Mutex::new(next),
        })
    }

    /// Resume the only currently supported journal-aware anchored migration.
    /// The provider has already attested the pre-migration v7 state when this
    /// method is entered. Each phase writes its journal state and local anchor
    /// record in the same transaction before the external CAS.
    pub(crate) fn migrate_v7_to_v8(&self, conn: &Connection) -> Result<(), StoreError> {
        let expected_plan = self.plan_digest(MIGRATION_FROM_V7, MIGRATION_TO_V8)?;
        let journal = read_journal(conn)?;
        let journal = match journal {
            None => {
                if schema::inspect_schema_version(conn)? != Some(MIGRATION_FROM_V7) {
                    return Err(StoreError::Anchor(
                        "anchored migration needs v7 source state before journal preparation"
                            .to_string(),
                    ));
                }
                let nonce: [u8; 32] = random::random_bytes(32)
                    .try_into()
                    .expect("random nonce length");
                let journal = MigrationJournal {
                    store_id: self.store_id,
                    from: MIGRATION_FROM_V7,
                    to: MIGRATION_TO_V8,
                    plan: expected_plan,
                    nonce,
                    phase: MigrationPhase::Prepared,
                };
                let tx = conn.unchecked_transaction()?;
                write_journal(&tx, &journal)?;
                self.commit(tx)?;
                journal
            }
            Some(journal) => {
                self.validate_journal(&journal, expected_plan)?;
                journal
            }
        };

        match journal.phase {
            MigrationPhase::Prepared => {
                if schema::inspect_schema_version(conn)? != Some(MIGRATION_FROM_V7) {
                    return Err(StoreError::Anchor(
                        "prepared migration journal does not match source schema".to_string(),
                    ));
                }
                let mut next = journal;
                next.phase = MigrationPhase::VacuumV8;
                let tx = conn.unchecked_transaction()?;
                schema::apply_v7_to_v8_tx(&tx)?;
                schema::install_current_schema_tx(&tx)?;
                write_journal(&tx, &next)?;
                self.commit(tx)?;
            }
            MigrationPhase::VacuumV8 => {}
        }

        if schema::inspect_schema_version(conn)? != Some(MIGRATION_TO_V8) {
            return Err(StoreError::Anchor(
                "anchored migration step does not match target schema".to_string(),
            ));
        }
        // The pending marker was part of the externally anchored StepV8 state
        // before this physical rewrite. A crash repeats VACUUM and cannot
        // clear the journal without a final external advance.
        conn.execute_batch("VACUUM;")?;
        let tx = conn.unchecked_transaction()?;
        schema::clear_vacuum_pending_tx(&tx)?;
        delete_journal(&tx)?;
        self.commit(tx)?;
        Ok(())
    }

    fn plan_digest(&self, from: u32, to: u32) -> Result<[u8; 32], StoreError> {
        let mut bytes = MIGRATION_PLAN_DOMAIN.to_vec();
        bytes.extend_from_slice(&from.to_le_bytes());
        bytes.extend_from_slice(&to.to_le_bytes());
        bytes.extend_from_slice(b"v7-drop-burned-at/v1");
        cipher::anchor_digest(&self.digest_key, &bytes)
    }

    fn validate_journal(
        &self,
        journal: &MigrationJournal,
        plan: [u8; 32],
    ) -> Result<(), StoreError> {
        if journal.store_id != self.store_id
            || journal.from != MIGRATION_FROM_V7
            || journal.to != MIGRATION_TO_V8
            || journal.plan != plan
        {
            return Err(StoreError::Anchor(
                "migration journal store, version, or plan does not match this anchor".to_string(),
            ));
        }
        Ok(())
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

fn encode_journal(journal: &MigrationJournal) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + 1 + 32 + 4 + 4 + 32 + 32 + 1);
    out.extend_from_slice(MIGRATION_MAGIC);
    out.push(1);
    out.extend_from_slice(&journal.store_id);
    out.extend_from_slice(&journal.from.to_le_bytes());
    out.extend_from_slice(&journal.to.to_le_bytes());
    out.extend_from_slice(&journal.plan);
    out.extend_from_slice(&journal.nonce);
    out.push(match journal.phase {
        MigrationPhase::Prepared => 1,
        MigrationPhase::VacuumV8 => 2,
    });
    out
}

fn read_journal(conn: &Connection) -> Result<Option<MigrationJournal>, StoreError> {
    let Some(bytes) = conn
        .query_row(
            "SELECT value FROM _meta WHERE key = ?1",
            params![MIGRATION_JOURNAL_KEY],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()?
    else {
        return Ok(None);
    };
    const LEN: usize = 4 + 1 + 32 + 4 + 4 + 32 + 32 + 1;
    if bytes.len() != LEN || &bytes[..4] != MIGRATION_MAGIC || bytes[4] != 1 {
        return Err(StoreError::Anchor(
            "migration journal has invalid format".to_string(),
        ));
    }
    let mut offset = 5;
    let take32 = |bytes: &[u8], offset: &mut usize| -> [u8; 32] {
        let value = bytes[*offset..*offset + 32]
            .try_into()
            .expect("validated journal length");
        *offset += 32;
        value
    };
    let store_id = take32(&bytes, &mut offset);
    let from = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    offset += 4;
    let to = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    offset += 4;
    let plan = take32(&bytes, &mut offset);
    let nonce = take32(&bytes, &mut offset);
    let phase = match bytes[offset] {
        1 => MigrationPhase::Prepared,
        2 => MigrationPhase::VacuumV8,
        _ => {
            return Err(StoreError::Anchor(
                "migration journal has unknown phase".to_string(),
            ))
        }
    };
    Ok(Some(MigrationJournal {
        store_id,
        from,
        to,
        plan,
        nonce,
        phase,
    }))
}

fn write_journal(tx: &Transaction<'_>, journal: &MigrationJournal) -> Result<(), StoreError> {
    tx.execute(
        "INSERT INTO _meta(key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![MIGRATION_JOURNAL_KEY, encode_journal(journal)],
    )?;
    Ok(())
}

fn delete_journal(tx: &Transaction<'_>) -> Result<(), StoreError> {
    tx.execute(
        "DELETE FROM _meta WHERE key = ?1",
        params![MIGRATION_JOURNAL_KEY],
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MessageStore, StoredMessage};
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;

    const SECRET: &[u8; 32] = &[0xa5; 32];

    #[derive(Clone, Copy, Debug)]
    enum Fault {
        None,
        Before(usize),
        After(usize),
    }

    struct Provider {
        records: Mutex<BTreeMap<[u8; 32], AnchorRecord>>,
        calls: Mutex<usize>,
        fault: Mutex<Fault>,
    }

    impl Provider {
        fn new() -> Self {
            Self {
                records: Mutex::new(BTreeMap::new()),
                calls: Mutex::new(0),
                fault: Mutex::new(Fault::None),
            }
        }

        fn fault(&self, fault: Fault) {
            *self.calls.lock().unwrap() = 0;
            *self.fault.lock().unwrap() = fault;
        }
    }

    impl MonotonicAnchor for Provider {
        fn load(&self, store_id: [u8; 32]) -> Result<Option<AnchorRecord>, StoreError> {
            Ok(self.records.lock().unwrap().get(&store_id).cloned())
        }

        fn compare_and_advance(
            &self,
            store_id: [u8; 32],
            expected: Option<AnchorRecord>,
            next: AnchorRecord,
        ) -> Result<(), StoreError> {
            let call = {
                let mut calls = self.calls.lock().unwrap();
                *calls += 1;
                *calls
            };
            match *self.fault.lock().unwrap() {
                Fault::Before(wanted) if wanted == call => {
                    return Err(StoreError::Anchor("test crash before CAS".to_string()))
                }
                _ => {}
            }
            let mut records = self.records.lock().unwrap();
            if records.get(&store_id).cloned() != expected {
                return Err(StoreError::Anchor("test stale CAS".to_string()));
            }
            records.insert(store_id, next);
            match *self.fault.lock().unwrap() {
                Fault::After(wanted) if wanted == call => {
                    Err(StoreError::Anchor("test crash after CAS".to_string()))
                }
                _ => Ok(()),
            }
        }
    }

    fn message() -> StoredMessage {
        StoredMessage {
            discord_message_id: "journal-message".to_string(),
            channel_id: "journal-channel".to_string(),
            sender_discord_id: "journal-sender".to_string(),
            sender_osl_user_id: "journal-service".to_string(),
            plaintext: "journal body".to_string(),
            decrypted_at: 1,
            burned: false,
        }
    }

    /// Create a nonempty v7 database with a real v7 local/provider record.
    /// This simulates a previous anchored binary; it never treats an
    /// unanchored v7 profile as historical anchor evidence.
    fn anchored_v7(provider: Arc<Provider>) -> TempDir {
        let tmp = TempDir::new().unwrap();
        {
            let store = MessageStore::open(tmp.path(), SECRET).unwrap();
            store.put(&message()).unwrap();
            store
                .put_attachment(
                    "journal-message",
                    "journal.bin",
                    "application/octet-stream",
                    b"journal attachment",
                    None,
                    None,
                    None,
                )
                .unwrap();
        }
        let conn = Connection::open(tmp.path().join("messages.sqlite")).unwrap();
        conn.execute_batch(
            "ALTER TABLE messages ADD COLUMN burned_at INTEGER;
             ALTER TABLE attachments ADD COLUMN burned_at INTEGER;",
        )
        .unwrap();
        conn.execute(
            "UPDATE _meta SET value = ?1 WHERE key = 'schema_version'",
            params![7u32.to_le_bytes().to_vec()],
        )
        .unwrap();
        let (store_id, digest_key) = cipher::derive_anchor_material(SECRET).unwrap();
        let record = AnchorRecord {
            generation: 1,
            digest: digest(&conn, &digest_key).unwrap(),
        };
        let tx = conn.unchecked_transaction().unwrap();
        write_record(&tx, &record).unwrap();
        tx.commit().unwrap();
        provider.records.lock().unwrap().insert(store_id, record);
        tmp
    }

    fn assert_v8(dir: &std::path::Path, provider: Arc<Provider>, context: &str) {
        for _ in 0..2 {
            let store = MessageStore::open_anchored(dir, SECRET, provider.clone()).unwrap();
            assert_eq!(store.get("journal-message").unwrap(), Some(message()));
            assert_eq!(
                store
                    .get_attachment("journal-message", "journal.bin")
                    .unwrap(),
                Some((
                    "application/octet-stream".to_string(),
                    b"journal attachment".to_vec()
                ))
            );
        }
        let conn = Connection::open(dir.join("messages.sqlite")).unwrap();
        assert_eq!(schema::inspect_schema_version(&conn).unwrap(), Some(8));
        assert!(read_journal(&conn).unwrap().is_none(), "{context}");
        assert!(schema::read_meta_blob(&conn, "vacuum_pending")
            .unwrap()
            .is_none());
    }

    #[test]
    fn anchored_v7_journal_crash_boundaries_resume_without_rollback() {
        // Calls are Prepared CAS, Step CAS, then final CAS. Before/after each
        // models every durable local/provider boundary in the v7→v8 protocol.
        for fault in [
            Fault::Before(1),
            Fault::After(1),
            Fault::Before(2),
            Fault::After(2),
            Fault::Before(3),
            Fault::After(3),
        ] {
            let provider = Arc::new(Provider::new());
            let tmp = anchored_v7(provider.clone());
            provider.fault(fault);
            assert!(MessageStore::open_anchored(tmp.path(), SECRET, provider.clone()).is_err());
            provider.fault(Fault::None);
            assert_v8(tmp.path(), provider, &format!("{fault:?}"));
        }
    }

    #[test]
    fn anchored_v7_stale_replay_and_wrong_plan_refuse_before_mutation() {
        let provider = Arc::new(Provider::new());
        let tmp = anchored_v7(provider.clone());
        let backup = TempDir::new().unwrap();
        std::fs::copy(
            tmp.path().join("messages.sqlite"),
            backup.path().join("messages.sqlite"),
        )
        .unwrap();
        assert_v8(tmp.path(), provider.clone(), "baseline");
        std::fs::copy(
            backup.path().join("messages.sqlite"),
            tmp.path().join("messages.sqlite"),
        )
        .unwrap();
        let error = match MessageStore::open_anchored(tmp.path(), SECRET, provider.clone()) {
            Ok(_) => panic!("stale pre-migration replay was accepted"),
            Err(error) => error,
        };
        assert!(matches!(error, StoreError::Anchor(_)));
        let conn = Connection::open(tmp.path().join("messages.sqlite")).unwrap();
        assert_eq!(schema::inspect_schema_version(&conn).unwrap(), Some(7));
        assert!(conn
            .prepare("PRAGMA table_info(messages)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .any(|name| name.unwrap() == "burned_at"));
    }

    #[test]
    fn anchored_v7_wrong_plan_or_store_journal_refuses_before_schema_step() {
        for mutate_store_id in [false, true] {
            let provider = Arc::new(Provider::new());
            let tmp = anchored_v7(provider.clone());
            // Leave a durable Prepared local state without its provider CAS.
            provider.fault(Fault::Before(1));
            assert!(MessageStore::open_anchored(tmp.path(), SECRET, provider.clone()).is_err());
            provider.fault(Fault::None);

            let conn = Connection::open(tmp.path().join("messages.sqlite")).unwrap();
            let mut journal = conn
                .query_row(
                    "SELECT value FROM _meta WHERE key = ?1",
                    params![MIGRATION_JOURNAL_KEY],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .unwrap();
            // Fixed encoding offsets: store id begins at 5 and plan at 45.
            journal[if mutate_store_id { 5 } else { 45 }] ^= 1;
            conn.execute(
                "UPDATE _meta SET value = ?1 WHERE key = ?2",
                params![journal, MIGRATION_JOURNAL_KEY],
            )
            .unwrap();
            // Make the altered local state/provider pair internally coherent
            // so refusal reaches exact journal validation rather than merely a
            // generic digest mismatch.
            let (store_id, digest_key) = cipher::derive_anchor_material(SECRET).unwrap();
            let record = AnchorRecord {
                generation: 2,
                digest: digest(&conn, &digest_key).unwrap(),
            };
            let tx = conn.unchecked_transaction().unwrap();
            write_record(&tx, &record).unwrap();
            tx.commit().unwrap();
            provider.records.lock().unwrap().insert(store_id, record);
            drop(conn);

            let error = match MessageStore::open_anchored(tmp.path(), SECRET, provider.clone()) {
                Ok(_) => panic!("changed journal was accepted"),
                Err(error) => error,
            };
            assert!(
                matches!(error, StoreError::Anchor(message) if message.contains("journal store, version, or plan"))
            );
            let conn = Connection::open(tmp.path().join("messages.sqlite")).unwrap();
            assert_eq!(schema::inspect_schema_version(&conn).unwrap(), Some(7));
        }
    }

    #[test]
    fn concurrent_recovery_writer_loses_stale_final_cas_without_extra_generation() {
        let provider = Arc::new(Provider::new());
        let tmp = anchored_v7(provider.clone());
        // Prepared CAS succeeds; the Step local transaction commits but its
        // provider CAS fails. This is the recovery state two openers contend.
        provider.fault(Fault::Before(2));
        assert!(MessageStore::open_anchored(tmp.path(), SECRET, provider.clone()).is_err());
        provider.fault(Fault::None);
        let conn = Connection::open(tmp.path().join("messages.sqlite")).unwrap();
        let a = AnchorBinding::reconcile_existing(&conn, SECRET, provider.clone())
            .unwrap()
            .unwrap();
        let b = AnchorBinding::reconcile_existing(&conn, SECRET, provider.clone())
            .unwrap()
            .unwrap();
        b.migrate_v7_to_v8(&conn).unwrap();
        let error = a.migrate_v7_to_v8(&conn).unwrap_err();
        assert!(
            matches!(error, StoreError::Anchor(message) if message.contains("needs v7 source")),
            "a stale recovery binding must refuse after the winner clears the journal"
        );
        drop(conn);
        assert_v8(tmp.path(), provider, "concurrent recovery loser");
    }
}
