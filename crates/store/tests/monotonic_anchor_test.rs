//! Failure-capable tests for the optional external rollback anchor.
//!
//! `TestAnchor` is intentionally an in-memory test double.  Production must
//! provide a TPM/keystore-backed implementation of `MonotonicAnchor`; writing
//! another local file would be replayable with SQLite and is forbidden.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use store::{AnchorRecord, MessageStore, MonotonicAnchor, StoreError, StoredMessage};
use tempfile::TempDir;

const SECRET_A: &[u8; 32] = b"monotonic-anchor-secret-a-32byte";
const SECRET_B: &[u8; 32] = b"monotonic-anchor-secret-b-32byte";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Fault {
    None,
    BeforeAdvance,
    AfterAdvance,
}

impl Default for Fault {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Default)]
struct TestAnchor {
    records: Mutex<BTreeMap<[u8; 32], AnchorRecord>>,
    fault: Mutex<Fault>,
    calls: Mutex<usize>,
    loads: Mutex<usize>,
    fault_call: Mutex<Option<usize>>,
}

impl TestAnchor {
    fn set_fault(&self, fault: Fault) {
        *self.fault.lock().unwrap() = fault;
        *self.fault_call.lock().unwrap() = None;
        *self.calls.lock().unwrap() = 0;
    }

    fn set_fault_on_call(&self, fault: Fault, call: usize) {
        *self.fault.lock().unwrap() = fault;
        *self.fault_call.lock().unwrap() = Some(call);
        *self.calls.lock().unwrap() = 0;
    }

    fn record_count(&self) -> usize {
        self.records.lock().unwrap().len()
    }

    fn load_calls(&self) -> usize {
        *self.loads.lock().unwrap()
    }

    fn compare_calls(&self) -> usize {
        *self.calls.lock().unwrap()
    }

    fn generation(&self) -> u64 {
        self.records
            .lock()
            .unwrap()
            .values()
            .next()
            .expect("anchor must have enrolled")
            .generation
    }
}

impl MonotonicAnchor for TestAnchor {
    fn load(&self, store_id: [u8; 32]) -> Result<Option<AnchorRecord>, StoreError> {
        *self.loads.lock().unwrap() += 1;
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
        let configured = *self.fault.lock().unwrap();
        let fault = match *self.fault_call.lock().unwrap() {
            Some(wanted) if wanted == call => configured,
            Some(_) => Fault::None,
            None => configured,
        };
        if fault == Fault::BeforeAdvance {
            return Err(StoreError::Anchor(
                "test: crash before provider CAS".to_string(),
            ));
        }
        let mut records = self.records.lock().unwrap();
        if records.get(&store_id).cloned() != expected {
            return Err(StoreError::Anchor(
                "test: stale compare-and-advance".to_string(),
            ));
        }
        records.insert(store_id, next);
        if fault == Fault::AfterAdvance {
            return Err(StoreError::Anchor(
                "test: crash after provider CAS".to_string(),
            ));
        }
        Ok(())
    }
}

fn message(id: &str, body: &str) -> StoredMessage {
    StoredMessage {
        discord_message_id: id.to_string(),
        channel_id: "anchor-channel".to_string(),
        sender_discord_id: "anchor-sender".to_string(),
        sender_osl_user_id: "anchor-service".to_string(),
        plaintext: body.to_string(),
        decrypted_at: 42,
        burned: false,
    }
}

fn open(dir: &Path, anchor: Arc<TestAnchor>) -> MessageStore {
    MessageStore::open_anchored(dir, SECRET_A, anchor).unwrap()
}

fn checkpoint(dir: &Path) {
    let conn = rusqlite::Connection::open(dir.join("messages.sqlite")).unwrap();
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .unwrap();
}

#[test]
fn full_anchor_coverage_load_compare_and_restore() {
    let tmp = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    let anchor = Arc::new(TestAnchor::default());
    {
        let store = open(tmp.path(), anchor.clone());
        assert!(
            anchor.load_calls() > 0,
            "anchored first open must consult the provider before enrollment"
        );
        assert_eq!(
            anchor.compare_calls(),
            1,
            "anchored first open must enroll with compare-and-advance"
        );
        store
            .put(&message("coverage-first", "first generation"))
            .unwrap();
    }
    assert_eq!(
        anchor.compare_calls(),
        2,
        "message mutation must advance the provider anchor"
    );
    checkpoint(tmp.path());
    fs::copy(
        tmp.path().join("messages.sqlite"),
        backup.path().join("messages.sqlite"),
    )
    .unwrap();

    let compare_calls_after_backup = anchor.compare_calls();
    let load_calls_after_backup = anchor.load_calls();
    {
        let reopened = open(tmp.path(), anchor.clone());
        assert_eq!(
            reopened.get("coverage-first").unwrap(),
            Some(message("coverage-first", "first generation"))
        );
    }
    assert!(
        anchor.load_calls() > load_calls_after_backup,
        "anchored reopen must re-load the provider record"
    );
    assert_eq!(
        anchor.compare_calls(),
        compare_calls_after_backup,
        "matching reopen must not advance an already-current provider record"
    );

    {
        let store = open(tmp.path(), anchor.clone());
        store
            .put(&message("coverage-second", "newer than backup"))
            .unwrap();
    }
    checkpoint(tmp.path());
    assert!(
        anchor.compare_calls() > compare_calls_after_backup,
        "post-backup mutation must publish a newer provider generation"
    );
    let compare_calls_before_restore = anchor.compare_calls();
    fs::copy(
        backup.path().join("messages.sqlite"),
        tmp.path().join("messages.sqlite"),
    )
    .unwrap();
    let error = match MessageStore::open_anchored(tmp.path(), SECRET_A, anchor.clone()) {
        Ok(_) => panic!("stale restored backup was accepted"),
        Err(error) => error,
    };
    assert!(
        matches!(&error, StoreError::Anchor(message) if message.contains("behind external anchor")),
        "wrong restored-backup refusal: {error}"
    );
    assert_eq!(
        anchor.compare_calls(),
        compare_calls_before_restore,
        "stale restore must fail from load comparison before any provider advance"
    );
}

#[test]
fn anchored_restart_attachment_and_burn_advance_one_external_generation_each() {
    let tmp = TempDir::new().unwrap();
    let anchor = Arc::new(TestAnchor::default());
    {
        let store = open(tmp.path(), anchor.clone());
        assert_eq!(anchor.generation(), 1, "enrollment generation");
        store.put(&message("anchor-message", "body")).unwrap();
        store
            .put_attachment(
                "anchor-message",
                "anchor.bin",
                "application/octet-stream",
                b"attachment",
                None,
                None,
                None,
            )
            .unwrap();
        store.mark_burned("anchor-message").unwrap();
        // Burn commits the destructive state, then commits the cleared
        // checkpoint-recovery marker as a second anchored generation.
        assert_eq!(anchor.generation(), 5);
    }
    let reopened = open(tmp.path(), anchor.clone());
    assert_eq!(reopened.get("anchor-message").unwrap(), None);
    assert_eq!(
        anchor.generation(),
        5,
        "restart must not silently re-enroll"
    );
}

#[test]
fn anchored_open_refuses_a_coherent_stale_database_replay() {
    let tmp = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    let anchor = Arc::new(TestAnchor::default());
    {
        let store = open(tmp.path(), anchor.clone());
        store.put(&message("first", "first body")).unwrap();
    }
    checkpoint(tmp.path());
    fs::copy(
        tmp.path().join("messages.sqlite"),
        backup.path().join("messages.sqlite"),
    )
    .unwrap();
    {
        let store = open(tmp.path(), anchor.clone());
        store.put(&message("second", "second body")).unwrap();
    }
    checkpoint(tmp.path());
    fs::copy(
        backup.path().join("messages.sqlite"),
        tmp.path().join("messages.sqlite"),
    )
    .unwrap();
    let error = match MessageStore::open_anchored(tmp.path(), SECRET_A, anchor) {
        Ok(_) => panic!("coherent stale replay was accepted"),
        Err(error) => error,
    };
    assert!(
        matches!(&error, StoreError::Anchor(message) if message.contains("behind external anchor"))
    );
}

#[test]
fn anchored_v7_migration_enrolls_post_migration_state_then_refuses_replay() {
    let tmp = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    let anchor = Arc::new(TestAnchor::default());
    {
        let store = MessageStore::open(tmp.path(), SECRET_A).unwrap();
        store.put(&message("v7", "migration body")).unwrap();
    }
    {
        let conn = rusqlite::Connection::open(tmp.path().join("messages.sqlite")).unwrap();
        conn.execute_batch(
            "ALTER TABLE messages ADD COLUMN burned_at INTEGER;
             ALTER TABLE attachments ADD COLUMN burned_at INTEGER;",
        )
        .unwrap();
        conn.execute(
            "UPDATE _meta SET value = ?1 WHERE key = 'schema_version'",
            rusqlite::params![7u32.to_le_bytes().to_vec()],
        )
        .unwrap();
    }
    {
        let store = open(tmp.path(), anchor.clone());
        assert_eq!(
            store.get("v7").unwrap(),
            Some(message("v7", "migration body"))
        );
        assert_eq!(
            anchor.generation(),
            1,
            "migration enrolls its post-v8 state"
        );
    }
    checkpoint(tmp.path());
    fs::copy(
        tmp.path().join("messages.sqlite"),
        backup.path().join("messages.sqlite"),
    )
    .unwrap();
    {
        let store = open(tmp.path(), anchor.clone());
        store
            .put(&message("after", "advanced after migration"))
            .unwrap();
    }
    checkpoint(tmp.path());
    fs::copy(
        backup.path().join("messages.sqlite"),
        tmp.path().join("messages.sqlite"),
    )
    .unwrap();
    let error = match MessageStore::open_anchored(tmp.path(), SECRET_A, anchor) {
        Ok(_) => panic!("post-migration stale replay was accepted"),
        Err(error) => error,
    };
    assert!(
        matches!(&error, StoreError::Anchor(message) if message.contains("behind external anchor"))
    );
}

#[test]
fn anchored_crash_before_or_after_provider_advance_recovers_without_successful_rollback() {
    for fault in [Fault::BeforeAdvance, Fault::AfterAdvance] {
        let tmp = TempDir::new().unwrap();
        let anchor = Arc::new(TestAnchor::default());
        let store = open(tmp.path(), anchor.clone());
        anchor.set_fault(fault);
        let error = store
            .put(&message("crash", "recoverable ambiguity"))
            .unwrap_err();
        assert!(matches!(error, StoreError::Anchor(_)));
        drop(store);
        anchor.set_fault(Fault::None);
        let reopened = open(tmp.path(), anchor.clone());
        assert_eq!(
            reopened.get("crash").unwrap(),
            Some(message("crash", "recoverable ambiguity"))
        );
        assert_eq!(anchor.generation(), 2);
    }
}

#[test]
fn anchored_pending_shred_reconciles_before_marker_clear_after_first_cas_crash() {
    let tmp = TempDir::new().unwrap();
    let anchor = Arc::new(TestAnchor::default());
    let store = open(tmp.path(), anchor.clone());
    store.put(&message("pending", "body")).unwrap();
    // The destructive transaction is externally anchored, then its caller
    // dies before checkpoint/marker clear. A restart must first reconcile that
    // pending anchor, not mutate metadata under an unverified state.
    anchor.set_fault_on_call(Fault::AfterAdvance, 1);
    assert!(matches!(
        store.mark_burned("pending"),
        Err(StoreError::Anchor(_))
    ));
    drop(store);
    anchor.set_fault(Fault::None);
    let reopened = open(tmp.path(), anchor.clone());
    assert_eq!(reopened.get("pending").unwrap(), None);
}

#[test]
fn anchored_marker_clear_cas_crash_reconciles_on_repeated_restart() {
    let tmp = TempDir::new().unwrap();
    let anchor = Arc::new(TestAnchor::default());
    let store = open(tmp.path(), anchor.clone());
    store.put(&message("burn", "body")).unwrap();
    // Call 1 anchors pending destruction; call 2 is the cleared-marker CAS.
    anchor.set_fault_on_call(Fault::BeforeAdvance, 2);
    assert!(matches!(
        store.mark_burned("burn"),
        Err(StoreError::Anchor(_))
    ));
    drop(store);
    anchor.set_fault(Fault::None);
    for _ in 0..2 {
        let reopened = open(tmp.path(), anchor.clone());
        assert_eq!(reopened.get("burn").unwrap(), None);
        drop(reopened);
    }
}

#[test]
fn anchored_provider_is_store_domain_separated_and_stale_concurrent_writer_fails_closed() {
    let first = TempDir::new().unwrap();
    let second = TempDir::new().unwrap();
    let anchor = Arc::new(TestAnchor::default());
    let _first = open(first.path(), anchor.clone());
    let _second = MessageStore::open_anchored(second.path(), SECRET_B, anchor.clone()).unwrap();
    assert_eq!(
        anchor.record_count(),
        2,
        "different identity secrets must not share anchor state"
    );

    let tmp = TempDir::new().unwrap();
    let concurrent_anchor = Arc::new(TestAnchor::default());
    let a = open(tmp.path(), concurrent_anchor.clone());
    let b = open(tmp.path(), concurrent_anchor.clone());
    a.put(&message("a", "first writer")).unwrap();
    let error = b.put(&message("b", "stale writer")).unwrap_err();
    assert!(
        matches!(&error, StoreError::Anchor(message) if message.contains("stale local anchor generation")),
        "the stale writer must be refused before it can commit a conflicting local generation"
    );
    drop(a);
    drop(b);
    let recovered = open(tmp.path(), concurrent_anchor);
    assert_eq!(
        recovered.get("a").unwrap(),
        Some(message("a", "first writer")),
        "the winning writer must remain readable after a stale writer is refused"
    );
    assert_eq!(
        recovered.get("b").unwrap(),
        None,
        "the stale writer must leave no locally committed message behind"
    );
}
