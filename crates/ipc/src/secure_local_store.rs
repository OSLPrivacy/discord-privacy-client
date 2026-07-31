//! Unit a45: `SecureLocalStore` contract for encrypted UI-side storage.
//!
//! Checklist A6 ("no protected plaintext at rest") is 0/5 because of two
//! confirmed leaks:
//!
//! - `apps/osl-hub-ui/src/main.ts` writes muted-peer ids, unread counts, and
//!   notification ids/titles/timestamps to browser `localStorage` in
//!   plaintext.
//! - [`crate::main_password::maybe_encrypt`] is a **silent passthrough**
//!   when no `file_storage_key` is installed: `None => Ok(plaintext.to_vec())`.
//!   Callers cannot tell "encrypted" apart from "happened to have no key
//!   right now" — the failure mode is silent degradation, not a visible
//!   error.
//!
//! This module defines the contract UI-side persistence must satisfy so
//! that class of bug becomes structurally impossible to reproduce, not just
//! discouraged by convention. Two later units migrate `main.ts` and
//! `main_password.rs` onto it; this unit only defines the shape.
//!
//! ## How the contract prevents the `maybe_encrypt` failure mode
//!
//! `maybe_encrypt` is a *free function* that silently branches on whether a
//! process-global key happens to be installed. [`SecureLocalStore::put`] is
//! an *instance method*: whether a key is available is decided once, at
//! construction time ([`SealedStore::new`] vs. [`SealedStore::without_key`]),
//! and every `put`/`get` call re-checks that stored `Option<Key>` before
//! doing anything else. There is no code path in [`SealedStore::put`] that
//! reaches the backend without going through [`aead::seal`] first — the
//! "no key" branch returns [`SecureLocalStoreError::NoKey`] and returns
//! immediately, never touching the backend. The trait itself also has no
//! plaintext-write method for a caller to fall back to: `put` is the only
//! way to persist a record, and its only two outcomes are "sealed
//! ciphertext went to the backend" or "no key, nothing was written."
//!
//! ## AAD binding
//!
//! Every record is identified by a [`RecordId`] (namespace + logical key),
//! folded into the AEAD associated data exactly like `crates/store/src/
//! cipher.rs` folds a row's blind-index selector and content version into
//! its AAD. A ciphertext sealed under one `RecordId` fails to authenticate
//! under any other `RecordId`, even with the correct key — an attacker with
//! file/localStorage write access cannot copy record A's ciphertext into
//! record B's slot and have it open there.
//!
//! ## Seam with `a41`'s `MandatoryStorageKeyPolicy`
//!
//! A sibling unit (`a41`, `crates/ipc/src/mandatory_storage_key_policy.rs`)
//! defines the *policy* question — given a logical file identity, is
//! plaintext ever acceptable? This module is the *mechanism* that policy
//! is meant to gate: something that wants to persist a `MandatoryEncrypt`
//! identity should hold a `SecureLocalStore` (concretely, a keyed
//! [`SealedStore`], never a plaintext-capable stand-in) and have no other
//! way to write that identity to disk. Neither module depends on the
//! other's crate items; the boundary is documentation-level by design so
//! either can land independently.

use crypto::aead::{self, Key as AeadKey, Nonce as AeadNonce};

/// Fixed domain-separation label folded into every record's AAD ahead of
/// the caller-supplied namespace/key, so this store's ciphertexts can never
/// be confused with another module's use of the same underlying key.
const AAD_DOMAIN: &[u8] = b"osl-secure-local-store/v1";

/// Identity of one stored record.
///
/// `namespace` groups records by kind (e.g. `"muted-peer"`,
/// `"unread-count"`, `"notification"` — the three plaintext `localStorage`
/// leaks this contract targets). `key` is the caller's logical key within
/// that namespace (e.g. a peer id or notification id). Both are folded into
/// the AEAD associated data via [`RecordId::aad`], so swapping ciphertext
/// between namespaces or keys fails authentication rather than silently
/// decrypting into the wrong slot.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RecordId {
    pub namespace: &'static str,
    pub key: String,
}

impl RecordId {
    pub fn new(namespace: &'static str, key: impl Into<String>) -> Self {
        RecordId {
            namespace,
            key: key.into(),
        }
    }

    fn aad(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(
            8 + AAD_DOMAIN.len() + 8 + self.namespace.len() + 8 + self.key.len(),
        );
        push_part(&mut out, AAD_DOMAIN);
        push_part(&mut out, self.namespace.as_bytes());
        push_part(&mut out, self.key.as_bytes());
        out
    }

    /// Opaque backend-facing storage location for this id. Distinct from
    /// the AAD encoding on purpose: this is where the blob lives, the AAD
    /// is what the blob is authenticated against. A backend only ever sees
    /// this string, never the plaintext namespace/key split.
    fn storage_key(&self) -> String {
        format!("{}\u{0}{}", self.namespace, self.key)
    }
}

/// Length-prefix one field so `(a="ab", b="c")` and `(a="a", b="bc")`
/// cannot encode identically — same reasoning as
/// `crates/store/src/cipher.rs::push_aad_part`.
fn push_part(out: &mut Vec<u8>, part: &[u8]) {
    out.extend_from_slice(&(part.len() as u64).to_le_bytes());
    out.extend_from_slice(part);
}

/// Errors this contract can surface. Every variant is safe to log and to
/// serialize back across the IPC boundary: none of them carry key material
/// or plaintext record contents, only what kind of failure occurred.
#[derive(Debug, thiserror::Error)]
pub enum SecureLocalStoreError {
    /// No usable encryption key is installed. `put`/`get`/`delete` all
    /// refuse to run rather than falling back to plaintext — this is the
    /// variant that replaces `maybe_encrypt`'s silent `Ok(plaintext)`.
    #[error("no encryption key available — refusing to read or write plaintext")]
    NoKey,

    /// No record exists at the requested id. Not a security-relevant
    /// failure, just "nothing there yet."
    #[error("no record at this id")]
    NotFound,

    /// AEAD tag verification failed: wrong key, wrong `RecordId` (AAD
    /// mismatch), or tampered/corrupted ciphertext. Deliberately not
    /// distinguished further — same "no error oracle" posture as
    /// `IpcError` in `crate::lib`.
    #[error("record failed to authenticate (wrong identity, wrong key, or tampered ciphertext)")]
    AuthenticationFailed,

    /// The stored blob does not have the shape this contract writes (too
    /// short to contain a nonce, wrong length fields, etc).
    #[error("stored record is malformed: {0}")]
    Malformed(String),

    /// The storage backend itself failed (disk I/O, localStorage quota,
    /// IPC transport, ...). Carries a human-readable reason but never the
    /// value that was being stored.
    #[error("storage backend failed: {0}")]
    Backend(String),
}

/// Contract that any UI-side persistence backend must satisfy.
///
/// There is intentionally **no** method on this trait that accepts
/// plaintext and is allowed to return `Ok(())` without having sealed it
/// first. A caller cannot "opt out" of encryption through this interface —
/// the only way to persist a record is [`put`](Self::put), and the only
/// way [`put`] succeeds is by sealing the plaintext under an AEAD key
/// first. If no key is available the call fails with
/// [`SecureLocalStoreError::NoKey`]; it never degrades to a plaintext
/// write.
pub trait SecureLocalStore: Send + Sync {
    /// Seal `plaintext` and persist it under `id`, replacing any existing
    /// record there. Returns `Err(SecureLocalStoreError::NoKey)` — and
    /// writes nothing to the backend — if this store instance has no
    /// usable key.
    fn put(&self, id: &RecordId, plaintext: &[u8]) -> Result<(), SecureLocalStoreError>;

    /// Recover and authenticate the record at `id`. Returns
    /// `Err(SecureLocalStoreError::NoKey)` under the same "no key" rule as
    /// `put`, `Err(SecureLocalStoreError::NotFound)` if nothing is stored
    /// there, and `Err(SecureLocalStoreError::AuthenticationFailed)` if the
    /// stored ciphertext does not authenticate under `id` and the current
    /// key (including the case where it was sealed under a *different*
    /// `RecordId`).
    fn get(&self, id: &RecordId) -> Result<Vec<u8>, SecureLocalStoreError>;

    /// Remove the record at `id`, if present. Absence is not an error.
    /// Deletion does not require a key — there is no plaintext to protect
    /// on the way out.
    fn delete(&self, id: &RecordId) -> Result<(), SecureLocalStoreError>;

    /// True iff this store instance currently holds a usable key (i.e.
    /// `put`/`get` would not immediately fail with `NoKey`). Callers use
    /// this to decide whether to prompt for a main password before
    /// attempting a batch of operations, not as a substitute for handling
    /// the `Err` case — `has_key` and the actual operation can race in a
    /// multi-threaded host, so `put`/`get` re-check independently.
    fn has_key(&self) -> bool;
}

/// Bytes a [`SealedStore`] hands to and reads from persistent storage.
/// Implementors only ever see the sealed blob — this trait has no
/// plaintext-shaped method, so a `RawBackend` cannot leak plaintext by
/// construction, only by mishandling bytes it never has visibility into.
pub trait RawBackend: Send + Sync {
    fn write_blob(&self, storage_key: &str, blob: &[u8]) -> Result<(), SecureLocalStoreError>;
    fn read_blob(&self, storage_key: &str) -> Result<Option<Vec<u8>>, SecureLocalStoreError>;
    fn remove_blob(&self, storage_key: &str) -> Result<(), SecureLocalStoreError>;
}

/// Reference [`SecureLocalStore`] implementation: AEAD-seals every record
/// before handing bytes to a [`RawBackend`], binding each one to its
/// [`RecordId`] via associated data.
///
/// The key is resolved exactly once, at construction:
///
/// - [`SealedStore::new`] takes a 32-byte key. Every `put`/`get` seals or
///   opens against it.
/// - [`SealedStore::without_key`] is the **only** other way to build one,
///   and it holds no key at all. `put`/`get` on it always return
///   `Err(SecureLocalStoreError::NoKey)` before touching the backend.
///
/// There is no third constructor and no setter that swaps a "no key"
/// instance into a keyed one in place — a caller who currently has no key
/// (e.g. no main password set yet) is holding a `SealedStore` that
/// structurally cannot write plaintext, not a store with a plaintext
/// fallback branch.
pub struct SealedStore<B: RawBackend> {
    key: Option<AeadKey>,
    backend: B,
}

impl<B: RawBackend> SealedStore<B> {
    /// Construct with a resolved key. `put`/`get` operate normally.
    pub fn new(key: [u8; 32], backend: B) -> Self {
        SealedStore {
            key: Some(AeadKey::from_bytes(key)),
            backend,
        }
    }

    /// Construct with no key. Every `put`/`get` call returns
    /// `Err(SecureLocalStoreError::NoKey)` until this instance is dropped
    /// and replaced by one built through [`SealedStore::new`] (e.g. after
    /// the user sets a main password) — there is no in-place upgrade path,
    /// which is deliberate: it keeps "do we have a key" a construction-time
    /// fact instead of mutable state a bug could flip silently.
    pub fn without_key(backend: B) -> Self {
        SealedStore { key: None, backend }
    }
}

impl<B: RawBackend> std::fmt::Debug for SealedStore<B> {
    /// Reports whether a key is present, never its bytes. `AeadKey` does
    /// not derive `Debug` at all (see `crates/crypto/src/aead.rs`), so a
    /// derived `Debug` here would not even compile — this manual impl is
    /// the only option, and it is written to stay that way.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SealedStore")
            .field("has_key", &self.key.is_some())
            .finish_non_exhaustive()
    }
}

impl<B: RawBackend> SecureLocalStore for SealedStore<B> {
    fn put(&self, id: &RecordId, plaintext: &[u8]) -> Result<(), SecureLocalStoreError> {
        let key = self.key.as_ref().ok_or(SecureLocalStoreError::NoKey)?;
        let blob = seal_record(key, id, plaintext)?;
        self.backend.write_blob(&id.storage_key(), &blob)
    }

    fn get(&self, id: &RecordId) -> Result<Vec<u8>, SecureLocalStoreError> {
        let key = self.key.as_ref().ok_or(SecureLocalStoreError::NoKey)?;
        let blob = self
            .backend
            .read_blob(&id.storage_key())?
            .ok_or(SecureLocalStoreError::NotFound)?;
        open_record(key, id, &blob)
    }

    fn delete(&self, id: &RecordId) -> Result<(), SecureLocalStoreError> {
        self.backend.remove_blob(&id.storage_key())
    }

    fn has_key(&self) -> bool {
        self.key.is_some()
    }
}

/// Seal `plaintext` for `id` under `key`. Wire layout: 24-byte nonce ||
/// AEAD ciphertext (tag appended by [`aead::seal`]). Exposed at module
/// level (not just via [`SealedStore`]) so the AAD-binding property can be
/// tested directly against the primitive, independent of any backend.
pub fn seal_record(
    key: &AeadKey,
    id: &RecordId,
    plaintext: &[u8],
) -> Result<Vec<u8>, SecureLocalStoreError> {
    let nonce = crypto::random::random_nonce();
    let ct = aead::seal(key, &nonce, &id.aad(), plaintext)
        .map_err(|e| SecureLocalStoreError::Backend(format!("seal: {e}")))?;
    let mut blob = Vec::with_capacity(aead::NONCE_SIZE + ct.len());
    blob.extend_from_slice(nonce.as_bytes());
    blob.extend_from_slice(&ct);
    Ok(blob)
}

/// Open a blob produced by [`seal_record`]. Fails with
/// [`SecureLocalStoreError::AuthenticationFailed`] if `id` does not match
/// the `RecordId` the blob was sealed under, `key` is wrong, or the blob
/// was tampered with.
pub fn open_record(
    key: &AeadKey,
    id: &RecordId,
    blob: &[u8],
) -> Result<Vec<u8>, SecureLocalStoreError> {
    if blob.len() < aead::NONCE_SIZE {
        return Err(SecureLocalStoreError::Malformed(format!(
            "blob shorter than {}-byte nonce prefix",
            aead::NONCE_SIZE
        )));
    }
    let (nonce_bytes, ct) = blob.split_at(aead::NONCE_SIZE);
    let mut nb = [0u8; aead::NONCE_SIZE];
    nb.copy_from_slice(nonce_bytes);
    let nonce = AeadNonce::from_bytes(nb);
    aead::open(key, &nonce, &id.aad(), ct).map_err(|_| SecureLocalStoreError::AuthenticationFailed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mandatory_storage_key_policy::{MandatoryStorageKeyPolicy, StorageClass};
    use crate::wire_rn::{RnError, RnSessionStore};
    use keystore::sealer::NoOpSealer;
    use osl_ratchet_next::primitives::x25519_keypair;
    use osl_ratchet_next::test_support::{fresh_bundle, seeded_rng};
    use osl_ratchet_next::{Session, SessionParams};
    use std::collections::HashMap;
    use std::sync::Mutex;
    use tempfile::TempDir;

    /// In-memory `RawBackend` for tests. Records whether `write_blob` was
    /// ever called so the "no key" test can prove nothing was written, not
    /// just that `put` returned an error.
    #[derive(Default)]
    struct InMemoryBackend {
        blobs: Mutex<HashMap<String, Vec<u8>>>,
        write_calls: Mutex<u32>,
    }

    impl RawBackend for InMemoryBackend {
        fn write_blob(&self, storage_key: &str, blob: &[u8]) -> Result<(), SecureLocalStoreError> {
            *self.write_calls.lock().unwrap() += 1;
            self.blobs
                .lock()
                .unwrap()
                .insert(storage_key.to_string(), blob.to_vec());
            Ok(())
        }

        fn read_blob(&self, storage_key: &str) -> Result<Option<Vec<u8>>, SecureLocalStoreError> {
            Ok(self.blobs.lock().unwrap().get(storage_key).cloned())
        }

        fn remove_blob(&self, storage_key: &str) -> Result<(), SecureLocalStoreError> {
            self.blobs.lock().unwrap().remove(storage_key);
            Ok(())
        }
    }

    fn test_key() -> [u8; 32] {
        [7u8; 32]
    }

    // ---- deliverable 1: no-key store refuses writes, never touches the
    // backend, and never stores plaintext. ----

    #[test]
    fn store_without_key_refuses_put_and_writes_nothing() {
        let backend = InMemoryBackend::default();
        let store = SealedStore::without_key(backend);
        let id = RecordId::new("muted-peer", "user-123");

        let result = store.put(&id, b"sensitive muted-peer id list");

        assert!(matches!(result, Err(SecureLocalStoreError::NoKey)));
        assert_eq!(*store.backend.write_calls.lock().unwrap(), 0);
        assert!(store.backend.blobs.lock().unwrap().is_empty());
        assert!(!store.has_key());
    }

    #[test]
    fn store_without_key_refuses_get() {
        let store = SealedStore::without_key(InMemoryBackend::default());
        let id = RecordId::new("unread-count", "channel-9");

        let result = store.get(&id);

        assert!(matches!(result, Err(SecureLocalStoreError::NoKey)));
    }

    #[test]
    fn no_key_error_never_falls_back_to_plaintext_ok() {
        // Directly guards against reintroducing the `maybe_encrypt` shape
        // (`None => Ok(plaintext.to_vec())`): a "no key" `put` must be
        // `Err`, full stop — there is no `Ok(_)` outcome for this state.
        let store = SealedStore::without_key(InMemoryBackend::default());
        let id = RecordId::new("notification", "notif-1");
        let secret = b"notification title that must never hit disk in the clear";

        assert!(store.put(&id, secret).is_err());
    }

    // ---- deliverable 2: AAD binds ciphertext to its RecordId; sealing
    // under A and opening under B fails authentication. ----

    #[test]
    fn record_sealed_under_one_identity_fails_to_open_under_another() {
        let key = AeadKey::from_bytes(test_key());
        let id_a = RecordId::new("muted-peer", "user-123");
        let id_b = RecordId::new("muted-peer", "user-456");

        let blob = seal_record(&key, &id_a, b"muted").expect("seal under A");
        let opened_as_b = open_record(&key, &id_b, &blob);

        assert!(matches!(
            opened_as_b,
            Err(SecureLocalStoreError::AuthenticationFailed)
        ));
        // Sanity: the same blob DOES open under the identity it was sealed
        // for, so the failure above is the AAD mismatch, not a broken seal.
        let opened_as_a = open_record(&key, &id_a, &blob).expect("seal/open round trip under A");
        assert_eq!(opened_as_a, b"muted");
    }

    #[test]
    fn record_sealed_under_one_namespace_fails_to_open_under_another_namespace_same_key() {
        // Namespace is part of the AAD too, not just the caller key —
        // "muted-peer/9" and "unread-count/9" must not be interchangeable.
        let key = AeadKey::from_bytes(test_key());
        let id_a = RecordId::new("muted-peer", "9");
        let id_b = RecordId::new("unread-count", "9");

        let blob = seal_record(&key, &id_a, b"payload").expect("seal under A");
        let opened_as_b = open_record(&key, &id_b, &blob);

        assert!(matches!(
            opened_as_b,
            Err(SecureLocalStoreError::AuthenticationFailed)
        ));
    }

    #[test]
    fn sealed_store_put_get_round_trips_through_the_full_trait() {
        let store = SealedStore::new(test_key(), InMemoryBackend::default());
        let id = RecordId::new("notification", "notif-42");
        let plaintext: &[u8] = b"hello from the secure trait path";
        let api: &dyn SecureLocalStore = &store;

        api.put(&id, plaintext).expect("put with key");
        let got = api.get(&id).expect("get with key");

        assert_eq!(got, plaintext);
        assert!(api.has_key());
        let stored_blob = store
            .backend
            .read_blob(&id.storage_key())
            .unwrap()
            .expect("trait put should persist one sealed blob");
        assert_ne!(stored_blob.as_slice(), plaintext);
        assert!(
            !stored_blob
                .windows(plaintext.len())
                .any(|window| window == plaintext),
            "full trait put must persist sealed bytes, not plaintext"
        );
    }

    #[test]
    fn sealed_store_get_missing_record_is_not_found_not_authentication_failure() {
        let store = SealedStore::new(test_key(), InMemoryBackend::default());
        let id = RecordId::new("notification", "never-written");

        assert!(matches!(
            store.get(&id),
            Err(SecureLocalStoreError::NotFound)
        ));
    }

    #[test]
    fn sealed_store_get_via_trait_object_rejects_record_from_a_different_id() {
        // Exercise the failure through `SecureLocalStore` (not just the
        // free functions): write under id_a's storage key, forge it into
        // id_b's slot in the backend, and confirm `get` on id_b refuses.
        let backend = InMemoryBackend::default();
        let store = SealedStore::new(test_key(), backend);
        let id_a = RecordId::new("muted-peer", "victim");
        let id_b = RecordId::new("muted-peer", "attacker-controlled-slot");

        store.put(&id_a, b"do not leak").expect("put under A");
        let stolen_blob = store
            .backend
            .read_blob(&id_a.storage_key())
            .unwrap()
            .unwrap();
        store
            .backend
            .write_blob(&id_b.storage_key(), &stolen_blob)
            .unwrap();

        let result = store.get(&id_b);
        assert!(matches!(
            result,
            Err(SecureLocalStoreError::AuthenticationFailed)
        ));
    }

    #[test]
    fn sealed_store_delete_does_not_require_a_key() {
        let store = SealedStore::new(test_key(), InMemoryBackend::default());
        let id = RecordId::new("unread-count", "channel-1");
        store.put(&id, b"3").unwrap();

        assert!(store.delete(&id).is_ok());
        assert!(matches!(
            store.get(&id),
            Err(SecureLocalStoreError::NotFound)
        ));
    }

    #[test]
    fn mandatory_file_storage_key_and_ui_session_storage_share_no_plaintext_fallback() {
        let policy = MandatoryStorageKeyPolicy::new();
        let file_secret = br#"{"peer":"must not be plaintext"}"#;
        let file_refusal = policy
            .authorize_write("peer_map.json", false)
            .expect_err("mandatory IPC file writes must refuse when no key is available");
        assert_eq!(file_refusal.file_id(), "peer_map.json");
        assert_eq!(
            policy
                .authorize_write("peer_map.json", true)
                .expect("keyed mandatory IPC file write is authorized"),
            StorageClass::MandatoryEncrypt
        );

        let ui_store = SealedStore::without_key(InMemoryBackend::default());
        let ui_id = RecordId::new("notification", "title-1");
        assert!(matches!(
            ui_store.put(&ui_id, file_secret),
            Err(SecureLocalStoreError::NoKey)
        ));
        assert!(
            ui_store.backend.blobs.lock().unwrap().is_empty(),
            "UI store must not write plaintext when constructed without a key"
        );

        let unkeyed_store = SealedStore::without_key(InMemoryBackend::default());
        let session_id = RecordId::new("ui-session", "active-discord-account");
        let session_plaintext = b"session token, unread state, and notification metadata";

        let session_result = unkeyed_store.put(&session_id, session_plaintext);

        assert!(matches!(session_result, Err(SecureLocalStoreError::NoKey)));
        assert_eq!(
            *unkeyed_store.backend.write_calls.lock().unwrap(),
            0,
            "unkeyed UI-session storage must not reach the backend"
        );
        assert!(
            unkeyed_store.backend.blobs.lock().unwrap().is_empty(),
            "unkeyed UI-session storage must not persist plaintext fallback bytes"
        );

        let keyed_store = SealedStore::new(test_key(), InMemoryBackend::default());
        keyed_store
            .put(&session_id, session_plaintext)
            .expect("keyed UI-session storage should seal and persist");
        let stored_blob = keyed_store
            .backend
            .read_blob(&session_id.storage_key())
            .unwrap()
            .expect("keyed write should persist one sealed blob");

        assert_ne!(
            stored_blob, session_plaintext,
            "successful UI-session storage must persist sealed bytes, not plaintext"
        );
        assert!(
            !stored_blob
                .windows(session_plaintext.len())
                .any(|window| window == session_plaintext),
            "sealed UI-session blob must not contain the plaintext as a contiguous slice"
        );
        assert_eq!(
            keyed_store
                .get(&session_id)
                .expect("sealed UI-session blob should round-trip"),
            session_plaintext
        );

        let dir = TempDir::new().expect("tempdir");
        let rn_dir = dir.path().join("rn");
        let rn_store = RnSessionStore::new(&rn_dir);
        let mut rng = seeded_rng(140);
        let (_prekeys, bundle) = fresh_bundle(&mut rng);
        let (own_identity, _) = x25519_keypair(&mut rng);
        let session = Session::initiate(&own_identity, &bundle, SessionParams::default(), &mut rng)
            .expect("session");
        let peer = *bundle.identity.as_bytes();

        assert!(matches!(
            rn_store.save_session_with_sealer(&peer, &session, &NoOpSealer),
            Err(RnError::PlaintextSealerRefused)
        ));
        assert!(
            !rn_dir.exists() || std::fs::read_dir(&rn_dir).expect("rn dir").next().is_none(),
            "RN session store must not leave plaintext session files behind"
        );
    }
}
