//! Production [`store::MonotonicAnchor`] implementation backed by the OS
//! credential store (Windows Credential Manager / macOS Keychain / Linux
//! Secret Service via the `keyring` crate).
//!
//! ## Why not a file next to `messages.sqlite`
//!
//! `crates/store/src/anchor.rs` is explicit about this: "A second file
//! beside SQLite would be replayable with the database and is therefore not
//! an anchor." Anything that lives inside the app's own data directory is
//! captured by the same backup/restore or filesystem-snapshot operation that
//! would replay an old `messages.sqlite`, so it proves nothing about
//! staleness.
//!
//! The OS credential store is a genuinely separate subsystem: on Windows it
//! is Credential Manager (a per-user protected store outside the app's
//! folder); on macOS it is the Keychain; on Linux it is the Secret Service
//! (a D-Bus daemon backed by its own database). Restoring a copy of the
//! app's data directory does not restore these — so a generation/digest
//! pair recorded there is a real external witness, matching the same shape
//! [`crate::sealer::KeyringSealer`] already relies on for the identity
//! blob's data key.
//!
//! This is *not* a hardware monotonic counter (a TPM NV counter, for
//! example, cannot be rolled back even by an attacker with filesystem
//! access to the whole credential database). It is the strongest portable
//! primitive available through this crate's existing dependencies, and it
//! closes the concrete gap the sweep identified: zero production
//! [`store::MonotonicAnchor`] callers existed before this file. Swapping in
//! a hardware-backed provider later only requires a new
//! [`store::MonotonicAnchor`] impl; callers depend on the trait, not this
//! struct.
//!
//! ## Compare-and-swap semantics
//!
//! [`KeystoreBackedAnchor::compare_and_advance`] takes an internal
//! `std::sync::Mutex` for the lifetime of the read-compare-write sequence,
//! so two callers sharing one `KeystoreBackedAnchor` instance in the same
//! process never interleave. Durability and the actual compare come from
//! the credential store itself: the record is read fresh on every call (no
//! in-memory caching of the current generation), compared byte-for-byte
//! against `expected`, and only written if they match. A mismatch --
//! including the rollback case, where `expected` names an older generation
//! than what is currently stored -- is refused with
//! [`store::StoreError::Anchor`] and the stored record is left completely
//! untouched, because the write only happens after the compare succeeds.
//!
//! Two independent OS processes racing this same credential entry are not
//! made atomic by the in-process mutex; the underlying `keyring` crate has
//! no native compare-and-swap primitive on any of its backends. That
//! residual race is inherent to every keyring-shaped backend and is the
//! reason `crates/store/src/anchor.rs::AnchorBinding::commit` re-checks the
//! *local* SQLite record inside its own transaction before ever calling
//! `compare_and_advance` -- the caller is expected to add its own
//! serialization, not rely on this provider for it.

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use std::sync::Mutex;
use store::{AnchorRecord, MonotonicAnchor, StoreError};

/// Default credential-store service namespace for anchor records. Distinct
/// from [`crate::sealer::KeyringSealer`]'s service/user pair so the two
/// entries never collide.
const DEFAULT_SERVICE: &str = "discord-privacy-client/anchor";

const RECORD_LEN: usize = 8 + 32;

fn encode_record(record: &AnchorRecord) -> String {
    let mut bytes = Vec::with_capacity(RECORD_LEN);
    bytes.extend_from_slice(&record.generation.to_le_bytes());
    bytes.extend_from_slice(&record.digest);
    STANDARD.encode(bytes)
}

fn decode_record(raw: &str) -> Result<AnchorRecord, StoreError> {
    let bytes = STANDARD
        .decode(raw)
        .map_err(|e| StoreError::Anchor(format!("keystore anchor record base64: {e}")))?;
    if bytes.len() != RECORD_LEN {
        return Err(StoreError::Anchor(format!(
            "keystore anchor record length {} != {RECORD_LEN}",
            bytes.len()
        )));
    }
    let generation = u64::from_le_bytes(bytes[..8].try_into().expect("checked length"));
    let digest: [u8; 32] = bytes[8..].try_into().expect("checked length");
    Ok(AnchorRecord { generation, digest })
}

/// Credential-store entry name for a given store id. Base64 (URL-safe, no
/// padding) rather than hex purely to avoid pulling in another crate; both
/// encodings are equally opaque here.
fn entry_user(store_id: [u8; 32]) -> String {
    URL_SAFE_NO_PAD.encode(store_id)
}

/// Minimal seam over the actual credential-store calls so the
/// compare-and-swap algorithm above can be exercised deterministically in
/// tests without touching the host's real Credential Manager / Keychain /
/// Secret Service. [`SystemKeyring`] is the only production implementation.
trait AnchorKeyring: Send + Sync {
    fn get(&self, service: &str, user: &str) -> Result<Option<String>, StoreError>;
    fn set(&self, service: &str, user: &str, value: &str) -> Result<(), StoreError>;
}

struct SystemKeyring;

impl AnchorKeyring for SystemKeyring {
    fn get(&self, service: &str, user: &str) -> Result<Option<String>, StoreError> {
        let entry = keyring::Entry::new(service, user)
            .map_err(|e| StoreError::Anchor(format!("keystore anchor entry: {e}")))?;
        match entry.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(StoreError::Anchor(format!(
                "keystore anchor get_password: {e}"
            ))),
        }
    }

    fn set(&self, service: &str, user: &str, value: &str) -> Result<(), StoreError> {
        let entry = keyring::Entry::new(service, user)
            .map_err(|e| StoreError::Anchor(format!("keystore anchor entry: {e}")))?;
        entry
            .set_password(value)
            .map_err(|e| StoreError::Anchor(format!("keystore anchor set_password: {e}")))
    }
}

/// `store::MonotonicAnchor` backed by the OS credential store. See the
/// module docs for why this is a real external witness and what the
/// compare-and-swap guarantees are.
///
/// `Debug` intentionally prints only the service namespace: the credential
/// store holds a generation counter and a keyed digest, neither of which is
/// identity-secret material, but no per-store state is ever surfaced here so
/// adding a secret field later can't silently leak through a derive.
pub struct KeystoreBackedAnchor {
    backend: Box<dyn AnchorKeyring>,
    service: String,
    lock: Mutex<()>,
}

impl std::fmt::Debug for KeystoreBackedAnchor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeystoreBackedAnchor")
            .field("service", &self.service)
            .finish_non_exhaustive()
    }
}

impl KeystoreBackedAnchor {
    /// Production constructor: the real OS credential store under the
    /// default service namespace.
    pub fn production() -> Self {
        Self::with_backend(Box::new(SystemKeyring), DEFAULT_SERVICE.to_string())
    }

    /// Same as [`Self::production`] but under a caller-chosen service
    /// namespace, e.g. to keep a QA/second-instance profile's anchor state
    /// (see the multi-instance-isolation notes) from colliding with a
    /// primary install's.
    pub fn with_service(service: impl Into<String>) -> Self {
        Self::with_backend(Box::new(SystemKeyring), service.into())
    }

    fn with_backend(backend: Box<dyn AnchorKeyring>, service: String) -> Self {
        Self {
            backend,
            service,
            lock: Mutex::new(()),
        }
    }

    /// Round-trip probe analogous to [`crate::sealer::verify_sealer_round_trip`]:
    /// confirms the credential-store backend this instance is bound to can
    /// actually persist and return a value before it is trusted as an
    /// anchor provider. Does not touch any real store id's record.
    pub fn verify_backend_round_trip(&self) -> Result<(), StoreError> {
        const PROBE_USER: &str = "osl-anchor-readiness-probe-v1";
        const PROBE_VALUE: &str = "osl-store-anchor/probe/v1";
        self.backend.set(&self.service, PROBE_USER, PROBE_VALUE)?;
        let got = self.backend.get(&self.service, PROBE_USER)?;
        if got.as_deref() != Some(PROBE_VALUE) {
            return Err(StoreError::Anchor(
                "keystore anchor readiness round-trip returned unexpected value".to_string(),
            ));
        }
        Ok(())
    }
}

impl MonotonicAnchor for KeystoreBackedAnchor {
    fn load(&self, store_id: [u8; 32]) -> Result<Option<AnchorRecord>, StoreError> {
        let _guard = self.lock.lock().expect("keystore anchor mutex poisoned");
        let user = entry_user(store_id);
        match self.backend.get(&self.service, &user)? {
            None => Ok(None),
            Some(raw) => Ok(Some(decode_record(&raw)?)),
        }
    }

    fn compare_and_advance(
        &self,
        store_id: [u8; 32],
        expected: Option<AnchorRecord>,
        next: AnchorRecord,
    ) -> Result<(), StoreError> {
        let _guard = self.lock.lock().expect("keystore anchor mutex poisoned");
        let user = entry_user(store_id);
        let current = match self.backend.get(&self.service, &user)? {
            None => None,
            Some(raw) => Some(decode_record(&raw)?),
        };
        if current != expected {
            return Err(StoreError::Anchor(
                "keystore anchor compare-and-advance refused a stale or rolled-back generation"
                    .to_string(),
            ));
        }
        self.backend
            .set(&self.service, &user, &encode_record(&next))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;

    /// In-memory stand-in for the OS credential store. Shares state across
    /// clones via the inner `Arc<Mutex<..>>`, which is how the tests below
    /// simulate a second `KeystoreBackedAnchor` reopening the same
    /// physical backend after a process restart.
    #[derive(Clone, Default)]
    struct InMemoryKeyring {
        entries: Arc<Mutex<HashMap<(String, String), String>>>,
    }

    impl AnchorKeyring for InMemoryKeyring {
        fn get(&self, service: &str, user: &str) -> Result<Option<String>, StoreError> {
            Ok(self
                .entries
                .lock()
                .unwrap()
                .get(&(service.to_string(), user.to_string()))
                .cloned())
        }

        fn set(&self, service: &str, user: &str, value: &str) -> Result<(), StoreError> {
            self.entries
                .lock()
                .unwrap()
                .insert((service.to_string(), user.to_string()), value.to_string());
            Ok(())
        }
    }

    fn anchor_on(backend: InMemoryKeyring) -> KeystoreBackedAnchor {
        KeystoreBackedAnchor::with_backend(Box::new(backend), "test-service".to_string())
    }

    fn record(generation: u64, fill: u8) -> AnchorRecord {
        AnchorRecord {
            generation,
            digest: [fill; 32],
        }
    }

    #[test]
    fn fresh_store_id_has_no_record() {
        let anchor = anchor_on(InMemoryKeyring::default());
        assert_eq!(anchor.load([1u8; 32]).unwrap(), None);
    }

    #[test]
    fn monotonic_advance_persists_across_a_new_instance_on_the_same_backend() {
        let backend = InMemoryKeyring::default();
        let store_id = [7u8; 32];
        let first = anchor_on(backend.clone());

        first
            .compare_and_advance(store_id, None, record(1, 0xaa))
            .unwrap();
        assert_eq!(first.load(store_id).unwrap(), Some(record(1, 0xaa)));

        first
            .compare_and_advance(store_id, Some(record(1, 0xaa)), record(2, 0xbb))
            .unwrap();
        assert_eq!(first.load(store_id).unwrap(), Some(record(2, 0xbb)));

        // A brand-new `KeystoreBackedAnchor` bound to the same physical
        // backend (i.e. a fresh process reopening the same credential
        // store) must observe the advanced generation, not a cached one.
        let reopened = anchor_on(backend);
        assert_eq!(reopened.load(store_id).unwrap(), Some(record(2, 0xbb)));
    }

    #[test]
    fn keystore_backed_anchor_load_reads_last_recorded_generation_and_digest() {
        let backend = InMemoryKeyring::default();
        let store_id = [0x42u8; 32];
        let writer = anchor_on(backend.clone());
        writer
            .compare_and_advance(store_id, None, record(7, 0x71))
            .unwrap();
        writer
            .compare_and_advance(store_id, Some(record(7, 0x71)), record(8, 0x82))
            .unwrap();

        let reader = anchor_on(backend);
        assert_eq!(reader.load(store_id).unwrap(), Some(record(8, 0x82)));
        assert_ne!(reader.load(store_id).unwrap(), Some(record(7, 0x71)));
    }

    #[test]
    fn keystore_backed_anchor_load_decodes_current_backend_record() {
        let backend = InMemoryKeyring::default();
        let anchor = anchor_on(backend.clone());
        let store_id = [0x52; 32];
        let user = entry_user(store_id);

        backend
            .set("test-service", &user, &encode_record(&record(4, 0x44)))
            .unwrap();
        assert_eq!(anchor.load(store_id).unwrap(), Some(record(4, 0x44)));

        backend
            .set("test-service", &user, &encode_record(&record(9, 0x99)))
            .unwrap();
        assert_eq!(
            anchor.load(store_id).unwrap(),
            Some(record(9, 0x99)),
            "load must decode the currently recorded generation and digest, not a cached value"
        );
    }

    #[test]
    fn stale_expected_generation_is_refused() {
        let anchor = anchor_on(InMemoryKeyring::default());
        let store_id = [9u8; 32];
        anchor
            .compare_and_advance(store_id, None, record(1, 0x01))
            .unwrap();

        // Wrong "expected": claims no record exists when generation 1 does.
        let error = anchor
            .compare_and_advance(store_id, None, record(2, 0x02))
            .unwrap_err();
        assert!(matches!(error, StoreError::Anchor(ref m) if m.contains("stale")));
    }

    #[test]
    fn rolled_back_generation_is_refused_without_destroying_current_state() {
        let anchor = anchor_on(InMemoryKeyring::default());
        let store_id = [3u8; 32];
        anchor
            .compare_and_advance(store_id, None, record(1, 0x10))
            .unwrap();
        anchor
            .compare_and_advance(store_id, Some(record(1, 0x10)), record(2, 0x20))
            .unwrap();
        anchor
            .compare_and_advance(store_id, Some(record(2, 0x20)), record(3, 0x30))
            .unwrap();

        // Attempt a replay: present generation 2 as "expected" (a rollback of
        // one step) with a would-be successor generation 4. The CAS must
        // refuse because the durable current record is generation 3, not 2.
        let error = anchor
            .compare_and_advance(store_id, Some(record(2, 0x20)), record(4, 0x40))
            .unwrap_err();
        assert!(matches!(&error, StoreError::Anchor(_)));

        // The refusal must not have mutated, cleared, or truncated the
        // durable record: generation 3 is exactly what was there before the
        // refused call, byte for byte.
        assert_eq!(anchor.load(store_id).unwrap(), Some(record(3, 0x30)));
    }

    #[test]
    fn replaying_the_original_none_expected_after_enrollment_is_refused() {
        let anchor = anchor_on(InMemoryKeyring::default());
        let store_id = [5u8; 32];
        anchor
            .compare_and_advance(store_id, None, record(1, 0xf0))
            .unwrap();

        // A rollback attacker who kept the pre-enrollment expectation
        // (`None`) and tries to re-run the very first CAS must be refused
        // once a record exists, and the existing generation must survive.
        let error = anchor
            .compare_and_advance(store_id, None, record(1, 0xf0))
            .unwrap_err();
        assert!(matches!(&error, StoreError::Anchor(_)));
        assert_eq!(anchor.load(store_id).unwrap(), Some(record(1, 0xf0)));
    }

    #[test]
    fn different_store_ids_are_independent() {
        let backend = InMemoryKeyring::default();
        let anchor = anchor_on(backend);
        let a = [1u8; 32];
        let b = [2u8; 32];

        anchor
            .compare_and_advance(a, None, record(1, 0xa1))
            .unwrap();
        assert_eq!(anchor.load(b).unwrap(), None);

        anchor
            .compare_and_advance(b, None, record(1, 0xb1))
            .unwrap();
        assert_eq!(anchor.load(a).unwrap(), Some(record(1, 0xa1)));
        assert_eq!(anchor.load(b).unwrap(), Some(record(1, 0xb1)));
    }

    #[test]
    fn record_round_trip_through_base64_matches_exactly() {
        let original = record(u64::MAX, 0xcd);
        let decoded = decode_record(&encode_record(&original)).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn malformed_stored_record_surfaces_as_anchor_error_not_a_panic() {
        let backend = InMemoryKeyring::default();
        backend
            .set("test-service", &entry_user([4u8; 32]), "not-base64!!")
            .unwrap();
        let anchor = anchor_on(backend);
        let error = anchor.load([4u8; 32]).unwrap_err();
        assert!(matches!(&error, StoreError::Anchor(_)));
    }

    #[test]
    fn readiness_round_trip_passes_against_the_in_memory_backend() {
        let anchor = anchor_on(InMemoryKeyring::default());
        anchor.verify_backend_round_trip().unwrap();
        // The probe must not have created a record for any real store id.
        assert_eq!(anchor.load([0u8; 32]).unwrap(), None);
    }
}
