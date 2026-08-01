//! Sealed, once-only plaintext cache for OSL-RN transcript rendering.
//!
//! An OSL-RN ciphertext is deliberately single-use: opening it advances the
//! receiving chain, so rendering the same carrier again must never attempt a
//! second decrypt.  This module is the narrow cache boundary used by the
//! Discord overlay integration: on a cache miss it decrypts once, seals the
//! resulting application payload through [`SecureLocalStore`], and only then
//! returns it to the renderer.  A cache hit returns the sealed copy without
//! touching the ratchet.
//!
//! `RnPlaintextCache` deliberately knows nothing about view-once, expiry, or
//! burn policy.  Those owners must call [`delete`](Self::delete) when their
//! policy destroys the local copy; this type provides the cryptographic
//! persistence and once-only rendering invariant, not a policy bypass.

use crate::secure_local_store::{RecordId, SecureLocalStore, SecureLocalStoreError};
use sha2::{Digest, Sha256};
use std::sync::Mutex;

const CACHE_NAMESPACE: &str = "rn-plaintext/v1";
const CACHE_KEY_DOMAIN: &[u8] = b"osl-rn-plaintext-cache-key/v1";

/// Opaque identity for one received carrier.  The cache backend receives only
/// a domain-separated SHA-256 digest, never a Discord message id or carrier
/// body in the storage key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RnPlaintextCacheKey([u8; 32]);

impl RnPlaintextCacheKey {
    /// Bind a cache row to the exact received RN carrier.
    pub fn from_carrier(carrier: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(CACHE_KEY_DOMAIN);
        hasher.update((carrier.len() as u64).to_le_bytes());
        hasher.update(carrier);
        Self(hasher.finalize().into())
    }

    fn record_id(&self) -> RecordId {
        RecordId::new(CACHE_NAMESPACE, hex(&self.0))
    }
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

/// Cache errors deliberately distinguish a cache miss (which is handled
/// internally) from a corrupt sealed row (which must fail closed rather than
/// consume the ratchet a second time).
#[derive(Debug, thiserror::Error)]
pub enum RnPlaintextCacheError {
    #[error("sealed RN plaintext cache is unavailable: {0}")]
    Store(#[from] SecureLocalStoreError),

    #[error("RN decrypt failed: {0}")]
    Decrypt(String),
}

/// A sealed transcript cache.  The mutex spans read → decrypt → sealed write,
/// which makes a cache miss single-flight within this process: ten render
/// requests for one carrier invoke the RN decrypt closure exactly once.
pub struct RnPlaintextCache<S: SecureLocalStore> {
    store: S,
    miss_lock: Mutex<()>,
}

impl<S: SecureLocalStore> RnPlaintextCache<S> {
    pub fn new(store: S) -> Self {
        Self {
            store,
            miss_lock: Mutex::new(()),
        }
    }

    /// Return the cached application payload, or decrypt and durably seal it
    /// exactly once on a miss.  A failed cache authentication is not treated
    /// as a miss: retrying the ratchet could consume a different message.
    pub fn render_or_decrypt<F>(
        &self,
        key: &RnPlaintextCacheKey,
        decrypt_rn: F,
    ) -> Result<Vec<u8>, RnPlaintextCacheError>
    where
        F: FnOnce() -> Result<Vec<u8>, RnPlaintextCacheError>,
    {
        let _guard = self
            .miss_lock
            .lock()
            .expect("RN plaintext cache mutex poisoned");
        let record = key.record_id();
        match self.store.get(&record) {
            Ok(plaintext) => Ok(plaintext),
            Err(SecureLocalStoreError::NotFound) => {
                let plaintext = decrypt_rn()?;
                // Do not return plaintext until it is durable.  Returning it
                // first would create the old crash window: the ratchet has
                // advanced but a later re-render has no recoverable payload.
                self.store.put(&record, &plaintext)?;
                Ok(plaintext)
            }
            Err(error) => Err(error.into()),
        }
    }

    /// Destroy the locally cached copy for a carrier.  Policy code for burn,
    /// expiry, and view-once owns when this must be called.
    pub fn delete(&self, key: &RnPlaintextCacheKey) -> Result<(), RnPlaintextCacheError> {
        self.store.delete(&key.record_id())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secure_local_store::{RawBackend, SealedStore};
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;

    #[derive(Clone, Default)]
    struct MemoryBackend {
        blobs: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    }

    impl RawBackend for MemoryBackend {
        fn write_blob(&self, storage_key: &str, blob: &[u8]) -> Result<(), SecureLocalStoreError> {
            self.blobs
                .lock()
                .expect("backend mutex poisoned")
                .insert(storage_key.to_owned(), blob.to_vec());
            Ok(())
        }

        fn read_blob(&self, storage_key: &str) -> Result<Option<Vec<u8>>, SecureLocalStoreError> {
            Ok(self
                .blobs
                .lock()
                .expect("backend mutex poisoned")
                .get(storage_key)
                .cloned())
        }

        fn remove_blob(&self, storage_key: &str) -> Result<(), SecureLocalStoreError> {
            self.blobs
                .lock()
                .expect("backend mutex poisoned")
                .remove(storage_key);
            Ok(())
        }
    }

    struct FileBackend {
        root: PathBuf,
    }

    impl FileBackend {
        fn path_for(&self, storage_key: &str) -> PathBuf {
            let digest = Sha256::digest(storage_key.as_bytes());
            self.root.join(hex(&digest))
        }
    }

    impl RawBackend for FileBackend {
        fn write_blob(&self, storage_key: &str, blob: &[u8]) -> Result<(), SecureLocalStoreError> {
            fs::create_dir_all(&self.root)
                .map_err(|error| SecureLocalStoreError::Backend(error.to_string()))?;
            fs::write(self.path_for(storage_key), blob)
                .map_err(|error| SecureLocalStoreError::Backend(error.to_string()))
        }

        fn read_blob(&self, storage_key: &str) -> Result<Option<Vec<u8>>, SecureLocalStoreError> {
            match fs::read(self.path_for(storage_key)) {
                Ok(blob) => Ok(Some(blob)),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(SecureLocalStoreError::Backend(error.to_string())),
            }
        }

        fn remove_blob(&self, storage_key: &str) -> Result<(), SecureLocalStoreError> {
            match fs::remove_file(self.path_for(storage_key)) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(SecureLocalStoreError::Backend(error.to_string())),
            }
        }
    }

    #[test]
    fn t19_t29_render_ten_times_decrypts_once_and_persists_only_sealed_bytes() {
        let root = TempDir::new().expect("temporary cache directory");
        let key = RnPlaintextCacheKey::from_carrier(b"received-rn-wire");
        let plaintext = b"a transcript body that must be sealed at rest";
        let decrypt_calls = AtomicUsize::new(0);

        for render_count in [5, 5] {
            let backend = FileBackend {
                root: root.path().to_path_buf(),
            };
            let cache = RnPlaintextCache::new(SealedStore::new([0x42; 32], backend));
            for _ in 0..render_count {
                let rendered = cache
                    .render_or_decrypt(&key, || {
                        decrypt_calls.fetch_add(1, Ordering::SeqCst);
                        Ok(plaintext.to_vec())
                    })
                    .expect("each render should return the cached transcript");
                assert_eq!(rendered, plaintext);
            }
        }

        assert_eq!(decrypt_calls.load(Ordering::SeqCst), 1);
        let cache_files = fs::read_dir(root.path())
            .expect("sealed cache directory")
            .collect::<Result<Vec<_>, _>>()
            .expect("one cache entry");
        assert_eq!(cache_files.len(), 1);
        let blob = fs::read(cache_files[0].path()).expect("sealed cache blob");
        assert!(!blob
            .windows(plaintext.len())
            .any(|window| window == plaintext));
    }

    #[test]
    fn tampered_cache_fails_closed_without_a_second_decrypt() {
        let backend = MemoryBackend::default();
        let inspect_backend = backend.clone();
        let cache = RnPlaintextCache::new(SealedStore::new([0x24; 32], backend));
        let key = RnPlaintextCacheKey::from_carrier(b"received-rn-wire");
        cache
            .render_or_decrypt(&key, || Ok(b"first decrypt".to_vec()))
            .expect("seed sealed cache");
        let mut blobs = inspect_backend
            .blobs
            .lock()
            .expect("backend mutex poisoned");
        let blob = blobs.values_mut().next().expect("seeded blob");
        blob[0] ^= 1;
        drop(blobs);

        let attempted_second_decrypt = AtomicUsize::new(0);
        let result = cache.render_or_decrypt(&key, || {
            attempted_second_decrypt.fetch_add(1, Ordering::SeqCst);
            Ok(b"must not decrypt again".to_vec())
        });

        assert!(matches!(result, Err(RnPlaintextCacheError::Store(_))));
        assert_eq!(attempted_second_decrypt.load(Ordering::SeqCst), 0);
    }
}
