//! The production [`RawBackend`] for [`ipc::secure_local_store::SealedStore`].
//!
//! # Why this file had to exist before any queue could be wired
//!
//! `SecureLocalStore` is the encrypted-at-rest contract every offline queue in
//! this tree is written against — `ipc::offline_send_queue`, `ipc::rn_outbox`,
//! `ipc::friend_request`, `ipc::rn_plaintext_cache`. Its only implementation is
//! `SealedStore<B: RawBackend>`, and until this module landed **every single
//! `impl RawBackend` in the repository sat behind `#[cfg(test)]`**:
//!
//! - `crates/ipc/src/offline_send_queue.rs:201` (`mod tests` at :192)
//! - `crates/ipc/src/rn_outbox.rs:95` (`mod tests` at :81)
//! - `crates/ipc/src/friend_request.rs:674` (`mod tests` at :642)
//! - `crates/ipc/src/rn_plaintext_cache.rs:158,196` (`mod tests` at :143)
//! - `crates/ipc/src/secure_local_store.rs:327` (`mod tests` at :307)
//! - `apps/osl-hub/tests/osl_chat_queue.rs:16` (an integration test)
//!
//! So the whole family was structurally unwireable: a caller could construct
//! `OslChatSendQueue` only by supplying a backend that does not exist outside a
//! test binary. That — not a missing call site — is why D-223's offline send
//! queue had "zero production callers". This module supplies the one missing
//! piece.
//!
//! # Shape
//!
//! One file per record, under a caller-chosen directory. The blob handed down
//! by `SealedStore` is **already** nonce-prefixed AEAD ciphertext bound to its
//! `RecordId` via associated data, so this layer adds no cryptography; it is
//! purely durable byte storage. It deliberately reuses
//! [`crate::atomic_file`] so a crash mid-write leaves either the previous
//! committed record or the new one, never a truncated file — the same
//! guarantee `peer_attachment_io`'s deletion outbox already relies on.
//!
//! The on-disk filename is the SHA-256 of the backend-facing storage key, hex
//! encoded. The storage key is `namespace \0 key` and `key` is caller-supplied,
//! so using it directly would put attacker-influenced text (including `..` and
//! path separators) into a path. Hashing makes traversal structurally
//! impossible rather than filtered.

use std::path::{Path, PathBuf};

use ipc::secure_local_store::{RawBackend, SecureLocalStoreError};
use sha2::{Digest, Sha256};

/// Largest record this backend will read back.
///
/// The offline send queue keeps *all* pending sends in a single record, so this
/// bounds the whole queue file, not one message. It is deliberately a hard
/// refusal rather than a truncation: a record larger than this is either
/// corruption or an attempt to make recovery allocate without bound, and both
/// deserve an error the caller can see.
pub const MAX_RECORD_BYTES: u64 = 4 * 1024 * 1024;

/// Durable, directory-backed [`RawBackend`].
#[derive(Debug, Clone)]
pub struct SecureDiskBackend {
    dir: PathBuf,
}

impl SecureDiskBackend {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Hash the backend-facing storage key into a traversal-proof filename.
    fn record_path(&self, storage_key: &str) -> PathBuf {
        let digest = Sha256::digest(storage_key.as_bytes());
        let mut name = String::with_capacity(digest.len() * 2 + 4);
        for byte in digest {
            name.push_str(&format!("{byte:02x}"));
        }
        name.push_str(".rec");
        self.dir.join(name)
    }
}

impl RawBackend for SecureDiskBackend {
    fn write_blob(&self, storage_key: &str, blob: &[u8]) -> Result<(), SecureLocalStoreError> {
        if blob.len() as u64 > MAX_RECORD_BYTES {
            return Err(SecureLocalStoreError::Backend(
                "record exceeds the bounded on-disk size".to_owned(),
            ));
        }
        crate::atomic_file::write_recoverable(
            &self.record_path(storage_key),
            blob,
            "OSL secure local record",
        )
        .map_err(SecureLocalStoreError::Backend)
    }

    fn read_blob(&self, storage_key: &str) -> Result<Option<Vec<u8>>, SecureLocalStoreError> {
        crate::atomic_file::read_recoverable_bounded(
            &self.record_path(storage_key),
            MAX_RECORD_BYTES,
            "OSL secure local record",
        )
        .map_err(SecureLocalStoreError::Backend)
    }

    fn remove_blob(&self, storage_key: &str) -> Result<(), SecureLocalStoreError> {
        let path = self.record_path(storage_key);
        for candidate in [path.clone(), path.with_extension("bak")] {
            match std::fs::remove_file(&candidate) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => {
                    return Err(SecureLocalStoreError::Backend(
                        "record could not be removed".to_owned(),
                    ))
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ipc::secure_local_store::{RecordId, SealedStore, SecureLocalStore};

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "osl-secure-disk-{label}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn a_record_survives_being_reopened_by_a_fresh_store() {
        let dir = temp_dir("roundtrip");
        let id = RecordId::new("offline-send-queue", "outbound-v1");

        let written = SealedStore::new([7u8; 32], SecureDiskBackend::new(&dir));
        written.put(&id, b"durable bytes").expect("put");
        drop(written);

        // A different instance, as after a restart.
        let reopened = SealedStore::new([7u8; 32], SecureDiskBackend::new(&dir));
        assert_eq!(reopened.get(&id).expect("get"), b"durable bytes");
    }

    #[test]
    fn a_caller_supplied_key_cannot_escape_the_directory() {
        let dir = temp_dir("traversal");
        let backend = SecureDiskBackend::new(&dir);
        let escape = RecordId::new("offline-send-queue", "../../../../etc/osl-escape");
        let store = SealedStore::new([9u8; 32], backend.clone());

        store.put(&escape, b"contained").expect("put");

        // Everything this backend wrote is a direct child of `dir`.
        let children: Vec<_> = std::fs::read_dir(&dir)
            .expect("read dir")
            .map(|entry| entry.expect("entry").path())
            .collect();
        assert_eq!(children.len(), 1, "one record file, got {children:?}");
        assert_eq!(children[0].parent(), Some(dir.as_path()));
        assert!(!std::path::Path::new("/etc/osl-escape").exists());
    }

    #[test]
    fn a_record_sealed_under_one_id_does_not_open_under_another() {
        let dir = temp_dir("aad");
        let store = SealedStore::new([3u8; 32], SecureDiskBackend::new(&dir));
        let mine = RecordId::new("offline-send-queue", "outbound-v1");
        let theirs = RecordId::new("offline-send-queue", "someone-else");
        store.put(&mine, b"mine").expect("put");

        // Copy my ciphertext into their slot and prove it refuses to open.
        let backend = SecureDiskBackend::new(&dir);
        let blob = backend
            .read_blob(&format!("offline-send-queue\u{0}outbound-v1"))
            .expect("read")
            .expect("present");
        backend
            .write_blob(&format!("offline-send-queue\u{0}someone-else"), &blob)
            .expect("write");

        assert!(matches!(
            store.get(&theirs),
            Err(SecureLocalStoreError::AuthenticationFailed)
        ));
    }

    #[test]
    fn an_oversized_record_is_refused_rather_than_written() {
        let dir = temp_dir("bounded");
        let backend = SecureDiskBackend::new(&dir);
        let oversized = vec![0u8; MAX_RECORD_BYTES as usize + 1];
        assert!(matches!(
            backend.write_blob("offline-send-queue\u{0}outbound-v1", &oversized),
            Err(SecureLocalStoreError::Backend(_))
        ));
        assert!(backend
            .read_blob("offline-send-queue\u{0}outbound-v1")
            .expect("read")
            .is_none());
    }

    #[test]
    fn removing_an_absent_record_is_not_an_error() {
        let dir = temp_dir("remove");
        let backend = SecureDiskBackend::new(&dir);
        backend
            .remove_blob("offline-send-queue\u{0}never-written")
            .expect("absent remove is Ok");
    }
}
