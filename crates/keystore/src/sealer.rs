//! Identity-blob sealing strategies.
//!
//! Spec: `docs/design/unlock-and-duress.md` "Storage" subsection +
//! `docs/design/build-order.md` Group B Layer B1.
//!
//! Three production sealers (in preference order from most-secure to
//! least), plus a `MemorySealer` for tests:
//!
//! 1. [`TpmSealer`] (Windows-only) — Microsoft Platform Crypto
//!    Provider via NCrypt. RSA-wraps a per-seal random
//!    XChaCha20-Poly1305 data key, the wrapped key + AEAD ciphertext
//!    + nonce ride together as one sealed blob. The RSA key itself
//!    is TPM-resident — it never leaves the TPM. Memory-dump
//!    extraction yields no usable identity bytes.
//! 2. [`KeyringSealer`] — keyring crate (Windows Credential Manager,
//!    macOS Keychain, Linux Secret Service). 32-byte XChaCha20-Poly1305
//!    key generated on first use and stored in the OS keyring; data
//!    blob = nonce || ciphertext.
//! 3. [`NoOpSealer`] — passthrough plain-file behaviour. Emits the
//!    INSECURE banner on disk and reports
//!    `requires_insecure_banner = true` so the storage layer surfaces
//!    it loudly.
//!
//! Plus:
//!
//! - [`MemorySealer`] — test-only, in-memory random key,
//!   never persisted; suitable for unit tests only.
//!
//! [`select_best_sealer`] tries TPM → keyring → NoOp in order. The
//! caller (typically the Tauri startup or an ipc command) takes the
//! returned `Box<dyn Sealer>` and threads it through
//! [`crate::storage::save_identity`] / [`crate::storage::load_identity`].

use crypto::aead;
use crypto::random;
use thiserror::Error;
use zeroize::Zeroizing;

/// Method-tag string written into the on-disk identity blob so
/// callers (and operators) can see at a glance which sealer was used.
/// Match against [`Sealer::method_label`] on the loading side.
pub const METHOD_TPM: &str = "tpm-pcp";
pub const METHOD_KEYRING: &str = "keyring";
pub const METHOD_NOOP: &str = "noop-insecure";
pub const METHOD_MEMORY: &str = "memory-test";

#[derive(Debug, Error)]
pub enum SealerError {
    #[error("crypto: {0}")]
    Crypto(#[from] crypto::error::Error),

    #[error("keyring: {0}")]
    Keyring(String),

    #[error("TPM / NCrypt: {0}")]
    Tpm(String),

    #[error("sealed blob malformed: {0}")]
    Malformed(String),
}

pub type Result<T> = core::result::Result<T, SealerError>;

/// Sealer abstraction: bytes in, opaque bytes out (and back).
///
/// Implementations MUST authenticate plaintext (via AEAD or TPM
/// integrity) so tampered ciphertext is rejected on `unseal`.
pub trait Sealer: Send + Sync {
    fn method_label(&self) -> &'static str;
    fn is_tpm_backed(&self) -> bool;
    fn requires_insecure_banner(&self) -> bool;
    fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>>;
    fn unseal(&self, ciphertext: &[u8]) -> Result<Vec<u8>>;
}

// ---- NoOpSealer ----

/// Passthrough sealer. Plaintext on disk. Emits the INSECURE banner.
/// Used as the absolute-last-resort fallback when neither TPM nor
/// keyring is available (e.g. dev on Linux WSL with no DBus).
#[derive(Default)]
pub struct NoOpSealer;

impl NoOpSealer {
    pub fn new() -> Self {
        NoOpSealer
    }
}

impl Sealer for NoOpSealer {
    fn method_label(&self) -> &'static str {
        METHOD_NOOP
    }
    fn is_tpm_backed(&self) -> bool {
        false
    }
    fn requires_insecure_banner(&self) -> bool {
        true
    }
    fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        Ok(plaintext.to_vec())
    }
    fn unseal(&self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        Ok(ciphertext.to_vec())
    }
}

// ---- MemorySealer (test only) ----

/// In-memory random key. Encrypts via XChaCha20-Poly1305 using the
/// in-process key. **Loses access on process exit** — only useful
/// for unit tests that round-trip seal/unseal within the same
/// process.
pub struct MemorySealer {
    key: aead::Key,
}

impl Default for MemorySealer {
    fn default() -> Self {
        Self::new()
    }
}

impl MemorySealer {
    pub fn new() -> Self {
        MemorySealer {
            key: random::random_aead_key(),
        }
    }
}

impl Sealer for MemorySealer {
    fn method_label(&self) -> &'static str {
        METHOD_MEMORY
    }
    fn is_tpm_backed(&self) -> bool {
        false
    }
    fn requires_insecure_banner(&self) -> bool {
        false
    }
    fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        seal_with_aead_key(&self.key, plaintext)
    }
    fn unseal(&self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        unseal_with_aead_key(&self.key, ciphertext)
    }
}

// ---- KeyringSealer ----

/// Sealer using the keyring crate (Windows Credential Manager /
/// macOS Keychain / Linux Secret Service) to persist a 32-byte
/// XChaCha20-Poly1305 key.
///
/// On first use [`Self::new`] fetches the existing key or generates
/// + writes a fresh one. Subsequent constructions read the same key.
///
/// On Linux WSL without DBus the keyring crate's secret-service
/// backend errors out at this point — the caller falls through to
/// [`NoOpSealer`].
pub struct KeyringSealer {
    key: aead::Key,
}

const KEYRING_SERVICE: &str = "discord-privacy-client";
const KEYRING_USER: &str = "identity-data-key.v1";

impl KeyringSealer {
    pub fn new() -> Result<Self> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
            .map_err(|e| SealerError::Keyring(format!("Entry::new: {e}")))?;
        let key = match entry.get_password() {
            Ok(b64) => {
                use base64::engine::general_purpose::STANDARD;
                use base64::Engine;
                let bytes = STANDARD
                    .decode(&b64)
                    .map_err(|e| SealerError::Malformed(format!("keyring b64: {e}")))?;
                if bytes.len() != aead::KEY_SIZE {
                    return Err(SealerError::Malformed(format!(
                        "keyring key size {} != {}",
                        bytes.len(),
                        aead::KEY_SIZE
                    )));
                }
                let mut k = [0u8; aead::KEY_SIZE];
                k.copy_from_slice(&bytes);
                aead::Key::from_bytes(k)
            }
            Err(keyring::Error::NoEntry) => {
                let k = random::random_aead_key();
                use base64::engine::general_purpose::STANDARD;
                use base64::Engine;
                let b64 = STANDARD.encode(k.as_bytes());
                entry
                    .set_password(&b64)
                    .map_err(|e| SealerError::Keyring(format!("set_password: {e}")))?;
                k
            }
            Err(e) => {
                return Err(SealerError::Keyring(format!("get_password: {e}")));
            }
        };
        // Self-probe: read back the key we believe we have via a
        // fresh Entry. If the keyring backend is broken / not
        // persistent (e.g. WSL with a mock DBus session that drops
        // state across calls), this catches it here so
        // `select_best_sealer` can fall through to NoOp instead of
        // silently returning a sealer whose state vanishes between
        // operations.
        let probe = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
            .map_err(|e| SealerError::Keyring(format!("probe Entry: {e}")))?;
        let stored = probe
            .get_password()
            .map_err(|e| SealerError::Keyring(format!("probe get_password: {e}")))?;
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine;
        let stored_bytes = STANDARD
            .decode(&stored)
            .map_err(|e| SealerError::Malformed(format!("probe b64: {e}")))?;
        if stored_bytes != key.as_bytes() {
            return Err(SealerError::Keyring(
                "probe round-trip failed: keyring backend not persistent".into(),
            ));
        }
        Ok(KeyringSealer { key })
    }

    /// Test/operations helper: delete the keyring entry so the next
    /// `KeyringSealer::new` regenerates a fresh key. Used by the
    /// duress-flow strip path (B3) and by integration tests.
    pub fn purge_keyring_entry() -> Result<()> {
        match keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
            .and_then(|e| e.delete_credential())
        {
            Ok(_) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(SealerError::Keyring(format!("delete_credential: {e}"))),
        }
    }
}

impl Sealer for KeyringSealer {
    fn method_label(&self) -> &'static str {
        METHOD_KEYRING
    }
    fn is_tpm_backed(&self) -> bool {
        false
    }
    fn requires_insecure_banner(&self) -> bool {
        false
    }
    fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        seal_with_aead_key(&self.key, plaintext)
    }
    fn unseal(&self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        unseal_with_aead_key(&self.key, ciphertext)
    }
}

// ---- TpmSealer (Windows-only) ----

#[cfg(windows)]
mod tpm {
    use super::{Result, Sealer, SealerError, METHOD_TPM};
    use crypto::aead;
    use crypto::random;
    use std::ffi::c_void;
    use windows::core::PCWSTR;
    use windows::Win32::Security::Cryptography::{
        NCryptCreatePersistedKey, NCryptDecrypt, NCryptEncrypt, NCryptFinalizeKey,
        NCryptFreeObject, NCryptOpenKey, NCryptOpenStorageProvider, BCRYPT_PAD_PKCS1,
        NCRYPT_FLAGS, NCRYPT_HANDLE, NCRYPT_KEY_HANDLE, NCRYPT_PROV_HANDLE,
    };

    const PROVIDER: PCWSTR = windows::core::w!("Microsoft Platform Crypto Provider");
    const KEY_NAME: PCWSTR = windows::core::w!("DiscordPrivacyClientIdentityKeyV1");
    const ALGO_RSA: PCWSTR = windows::core::w!("RSA");

    pub struct TpmSealer {
        // Provider is per-seal/unseal — opening + closing on each
        // operation is acceptable for a low-frequency identity-blob
        // codepath. Avoids holding a TBS context across the lifetime
        // of the program.
        _private: (),
    }

    impl TpmSealer {
        /// Open the PCP provider and ensure our identity-keyring RSA
        /// key exists. Returns an error if the TPM is not present /
        /// not provisioned / PCP is unavailable.
        pub fn new() -> Result<Self> {
            unsafe {
                let mut prov: NCRYPT_PROV_HANDLE = NCRYPT_PROV_HANDLE::default();
                NCryptOpenStorageProvider(&mut prov, PROVIDER, 0)
                    .ok()
                    .map_err(|e| SealerError::Tpm(format!("OpenStorageProvider: {e}")))?;

                // Try to open the persisted key; if missing, create
                // a fresh 2048-bit RSA key, finalize, and proceed.
                let mut key: NCRYPT_KEY_HANDLE = NCRYPT_KEY_HANDLE::default();
                let open = NCryptOpenKey(prov, &mut key, KEY_NAME, 0, NCRYPT_FLAGS(0));
                if open.is_err() {
                    NCryptCreatePersistedKey(prov, &mut key, ALGO_RSA, KEY_NAME, 0, NCRYPT_FLAGS(0))
                        .ok()
                        .map_err(|e| {
                            let _ = NCryptFreeObject(NCRYPT_HANDLE(prov.0));
                            SealerError::Tpm(format!("CreatePersistedKey: {e}"))
                        })?;
                    NCryptFinalizeKey(key, NCRYPT_FLAGS(0))
                        .ok()
                        .map_err(|e| {
                            let _ = NCryptFreeObject(NCRYPT_HANDLE(key.0));
                            let _ = NCryptFreeObject(NCRYPT_HANDLE(prov.0));
                            SealerError::Tpm(format!("FinalizeKey: {e}"))
                        })?;
                }

                let _ = NCryptFreeObject(NCRYPT_HANDLE(key.0));
                let _ = NCryptFreeObject(NCRYPT_HANDLE(prov.0));
                Ok(TpmSealer { _private: () })
            }
        }

        /// Acquire a fresh provider+key handle pair for one operation.
        /// Caller is responsible for freeing both handles.
        unsafe fn open() -> Result<(NCRYPT_PROV_HANDLE, NCRYPT_KEY_HANDLE)> {
            let mut prov = NCRYPT_PROV_HANDLE::default();
            NCryptOpenStorageProvider(&mut prov, PROVIDER, 0)
                .ok()
                .map_err(|e| SealerError::Tpm(format!("OpenStorageProvider: {e}")))?;
            let mut key = NCRYPT_KEY_HANDLE::default();
            NCryptOpenKey(prov, &mut key, KEY_NAME, 0, NCRYPT_FLAGS(0))
                .ok()
                .map_err(|e| {
                    let _ = NCryptFreeObject(NCRYPT_HANDLE(prov.0));
                    SealerError::Tpm(format!("OpenKey: {e}"))
                })?;
            Ok((prov, key))
        }
    }

    impl Sealer for TpmSealer {
        fn method_label(&self) -> &'static str {
            METHOD_TPM
        }
        fn is_tpm_backed(&self) -> bool {
            true
        }
        fn requires_insecure_banner(&self) -> bool {
            false
        }

        fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
            // Hybrid wrap: random 32-byte XChaCha20-Poly1305 key,
            // RSA-wrap it via the TPM-resident key, AEAD-encrypt the
            // payload with the random key.
            let data_key = random::random_aead_key();
            unsafe {
                let (prov, key) = Self::open()?;

                // RSA-wrap the data key using PKCS#1 v1.5 padding.
                let mut wrapped_len: u32 = 0;
                NCryptEncrypt(
                    key,
                    Some(data_key.as_bytes()),
                    None,
                    None,
                    Some(&mut wrapped_len),
                    BCRYPT_PAD_PKCS1.0,
                )
                .ok()
                .map_err(|e| {
                    let _ = NCryptFreeObject(NCRYPT_HANDLE(key.0));
                    let _ = NCryptFreeObject(NCRYPT_HANDLE(prov.0));
                    SealerError::Tpm(format!("Encrypt size: {e}"))
                })?;

                let mut wrapped = vec![0u8; wrapped_len as usize];
                let mut got: u32 = 0;
                NCryptEncrypt(
                    key,
                    Some(data_key.as_bytes()),
                    None,
                    Some(&mut wrapped),
                    Some(&mut got),
                    BCRYPT_PAD_PKCS1.0,
                )
                .ok()
                .map_err(|e| {
                    let _ = NCryptFreeObject(NCRYPT_HANDLE(key.0));
                    let _ = NCryptFreeObject(NCRYPT_HANDLE(prov.0));
                    SealerError::Tpm(format!("Encrypt: {e}"))
                })?;
                wrapped.truncate(got as usize);

                let _ = NCryptFreeObject(NCRYPT_HANDLE(key.0));
                let _ = NCryptFreeObject(NCRYPT_HANDLE(prov.0));

                // AEAD-wrap the payload with the random data key.
                let blob = super::seal_with_aead_key(&data_key, plaintext)?;

                // Wire layout: u32 BE wrapped_len || wrapped || blob.
                let mut out = Vec::with_capacity(4 + wrapped.len() + blob.len());
                out.extend_from_slice(&(wrapped.len() as u32).to_be_bytes());
                out.extend_from_slice(&wrapped);
                out.extend_from_slice(&blob);
                Ok(out)
            }
        }

        fn unseal(&self, ciphertext: &[u8]) -> Result<Vec<u8>> {
            if ciphertext.len() < 4 {
                return Err(SealerError::Malformed(
                    "TPM sealed blob shorter than 4-byte length prefix".into(),
                ));
            }
            let wrapped_len =
                u32::from_be_bytes(ciphertext[..4].try_into().unwrap()) as usize;
            if ciphertext.len() < 4 + wrapped_len {
                return Err(SealerError::Malformed(format!(
                    "TPM sealed blob truncated: declared wrapped len {wrapped_len} > input"
                )));
            }
            let wrapped = &ciphertext[4..4 + wrapped_len];
            let blob = &ciphertext[4 + wrapped_len..];

            unsafe {
                let (prov, key) = Self::open()?;

                let mut got: u32 = 0;
                NCryptDecrypt(
                    key,
                    Some(wrapped),
                    None,
                    None,
                    Some(&mut got),
                    BCRYPT_PAD_PKCS1.0,
                )
                .ok()
                .map_err(|e| {
                    let _ = NCryptFreeObject(NCRYPT_HANDLE(key.0));
                    let _ = NCryptFreeObject(NCRYPT_HANDLE(prov.0));
                    SealerError::Tpm(format!("Decrypt size: {e}"))
                })?;

                let mut data_key_bytes = vec![0u8; got as usize];
                NCryptDecrypt(
                    key,
                    Some(wrapped),
                    None,
                    Some(&mut data_key_bytes),
                    Some(&mut got),
                    BCRYPT_PAD_PKCS1.0,
                )
                .ok()
                .map_err(|e| {
                    let _ = NCryptFreeObject(NCRYPT_HANDLE(key.0));
                    let _ = NCryptFreeObject(NCRYPT_HANDLE(prov.0));
                    SealerError::Tpm(format!("Decrypt: {e}"))
                })?;

                let _ = NCryptFreeObject(NCRYPT_HANDLE(key.0));
                let _ = NCryptFreeObject(NCRYPT_HANDLE(prov.0));

                if got as usize != aead::KEY_SIZE {
                    return Err(SealerError::Malformed(format!(
                        "TPM-unwrapped data-key size {got} != {} (corrupt blob?)",
                        aead::KEY_SIZE
                    )));
                }
                let mut k = [0u8; aead::KEY_SIZE];
                k.copy_from_slice(&data_key_bytes[..aead::KEY_SIZE]);
                let data_key = aead::Key::from_bytes(k);

                super::unseal_with_aead_key(&data_key, blob)
            }
        }
    }

    /// Test/operations helper: evict the persisted TPM key. Used by
    /// the duress-flow strip path (B3).
    pub fn evict_tpm_key() -> Result<()> {
        unsafe {
            let mut prov = NCRYPT_PROV_HANDLE::default();
            NCryptOpenStorageProvider(&mut prov, PROVIDER, 0)
                .ok()
                .map_err(|e| SealerError::Tpm(format!("OpenStorageProvider: {e}")))?;
            let mut key = NCRYPT_KEY_HANDLE::default();
            match NCryptOpenKey(prov, &mut key, KEY_NAME, 0, NCRYPT_FLAGS(0)) {
                Ok(_) => {
                    let _ =
                        windows::Win32::Security::Cryptography::NCryptDeleteKey(key, 0);
                }
                Err(_) => {
                    // Already gone — nothing to do.
                }
            }
            let _ = NCryptFreeObject(NCRYPT_HANDLE(prov.0));
            Ok(())
        }
    }
}

#[cfg(windows)]
pub use tpm::{evict_tpm_key, TpmSealer};

// On non-Windows, expose stub equivalents so cross-platform code can
// reference the symbols without `cfg`-gates everywhere.
#[cfg(not(windows))]
pub struct TpmSealer;

#[cfg(not(windows))]
impl TpmSealer {
    pub fn new() -> Result<Self> {
        Err(SealerError::Tpm(
            "TPM sealing is Windows-only (Microsoft Platform Crypto Provider)".into(),
        ))
    }
}

#[cfg(not(windows))]
impl Sealer for TpmSealer {
    fn method_label(&self) -> &'static str {
        METHOD_TPM
    }
    fn is_tpm_backed(&self) -> bool {
        true
    }
    fn requires_insecure_banner(&self) -> bool {
        false
    }
    fn seal(&self, _plaintext: &[u8]) -> Result<Vec<u8>> {
        Err(SealerError::Tpm("TPM sealer not available on this OS".into()))
    }
    fn unseal(&self, _ciphertext: &[u8]) -> Result<Vec<u8>> {
        Err(SealerError::Tpm("TPM sealer not available on this OS".into()))
    }
}

#[cfg(not(windows))]
pub fn evict_tpm_key() -> Result<()> {
    // Non-Windows: nothing to evict.
    Ok(())
}

// ---- factory ----

/// Pick the most-secure available sealer in this dev environment.
///
/// Order: TPM (Windows) → keyring (cross-platform) → no-op (always
/// works, emits the INSECURE banner). Each layer is tried in turn;
/// failures fall through silently except for the (always-succeeding)
/// `NoOpSealer` terminator.
pub fn select_best_sealer() -> Box<dyn Sealer> {
    #[cfg(windows)]
    {
        if let Ok(s) = TpmSealer::new() {
            return Box::new(s);
        }
    }
    if let Ok(s) = KeyringSealer::new() {
        return Box::new(s);
    }
    Box::new(NoOpSealer::new())
}

// ---- helpers ----

const SEAL_NONCE_PREFIX: &[u8] = b"discord-privacy-client/keystore/seal/v1";

/// Common AEAD wrap used by Memory / Keyring / Tpm sealers (the
/// latter pairs it with a TPM-RSA-wrap of the data key). The nonce
/// is fully random per-seal; AAD is a fixed domain-separation label
/// so seal/unseal are bound to this module's purpose.
fn seal_with_aead_key(key: &aead::Key, plaintext: &[u8]) -> Result<Vec<u8>> {
    let nonce = random::random_nonce();
    let ct = aead::seal(key, &nonce, SEAL_NONCE_PREFIX, plaintext)?;
    let mut out = Vec::with_capacity(aead::NONCE_SIZE + ct.len());
    out.extend_from_slice(nonce.as_bytes());
    out.extend_from_slice(&ct);
    Ok(out)
}

fn unseal_with_aead_key(key: &aead::Key, blob: &[u8]) -> Result<Vec<u8>> {
    if blob.len() < aead::NONCE_SIZE {
        return Err(SealerError::Malformed(format!(
            "blob shorter than {}-byte nonce prefix",
            aead::NONCE_SIZE
        )));
    }
    let mut n = [0u8; aead::NONCE_SIZE];
    n.copy_from_slice(&blob[..aead::NONCE_SIZE]);
    let nonce = aead::Nonce::from_bytes(n);
    let ct = &blob[aead::NONCE_SIZE..];
    let pt = aead::open(key, &nonce, SEAL_NONCE_PREFIX, ct)?;
    let _ = Zeroizing::new(()); // marker: pt is sensitive (caller's responsibility)
    Ok(pt)
}
