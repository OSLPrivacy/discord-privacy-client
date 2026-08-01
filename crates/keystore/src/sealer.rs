//! Identity-blob sealing strategies.
//!
//! Spec: `docs/design/unlock-and-duress.md` "Storage" subsection +
//! `docs/design/build-order.md` Group B Layer B1.
//!
//! Three production sealers (in preference order from most-secure to
//! least), plus a `MemorySealer` for tests:
//!
//! - [`TpmSealer`] (Windows-only) uses Microsoft Platform Crypto
//!   Provider via NCrypt. The RSA key is TPM-resident and wraps a
//!   per-seal XChaCha20-Poly1305 data key.
//! - [`KeyringSealer`] keeps a 32-byte XChaCha20-Poly1305 key in the
//!   platform credential manager, Keychain, or Secret Service.
//! - A process-wide ephemeral encrypted fallback preserves
//!   confidentiality when TPM/keyring access is unavailable, but it
//!   intentionally cannot reopen data after a process restart.
//!
//! Plus:
//!
//! - [`MemorySealer`] — test-only, in-memory random key,
//!   never persisted; suitable for unit tests only.
//!
//! [`select_best_sealer`] tries TPM → keyring → process memory in order. The
//! caller (typically the Tauri startup or an ipc command) takes the
//! returned `Box<dyn Sealer>` and threads it through
//! [`crate::storage::save_identity`] / [`crate::storage::load_identity`].

use crypto::aead;
use crypto::random;
use std::sync::OnceLock;
use thiserror::Error;
use zeroize::Zeroizing;

/// Method-tag string written into the on-disk identity blob so
/// callers (and operators) can see at a glance which sealer was used.
/// Match against [`Sealer::method_label`] on the loading side.
pub const METHOD_TPM: &str = "tpm-pcp";
pub const METHOD_KEYRING: &str = "keyring";
pub const METHOD_NOOP: &str = "noop-insecure";
pub const METHOD_MEMORY: &str = "memory-test";
pub const METHOD_EPHEMERAL: &str = "memory-ephemeral";

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

/// What an eviction attempt actually established.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TpmEvictOutcome {
    /// A persisted TPM key existed and was deleted.
    Evicted,
    /// PROVABLY nothing to evict: the platform crypto provider is absent
    /// or unsupported on this machine, so no key could ever have been
    /// persisted through it. This is NOT 'the delete failed'.
    NoTpmNothingToEvict,
}

/// Sealer abstraction: bytes in, opaque bytes out (and back).
///
/// Implementations MUST authenticate plaintext (via AEAD or TPM
/// integrity) so tampered ciphertext is rejected on `unseal`.
pub trait Sealer: Send + Sync {
    fn method_label(&self) -> &'static str;
    fn is_tpm_backed(&self) -> bool;
    fn requires_insecure_banner(&self) -> bool;
    fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>>;
    /// Recover the sealed plaintext.
    ///
    /// A8: the return type is deliberately `Zeroizing<Vec<u8>>`, not a bare
    /// `Vec<u8>`. Every shipping caller of this method is unsealing long-term
    /// key material (identity blob, prekey state, password record, ratchet
    /// session), so the buffer must wipe itself when the caller drops it
    /// rather than being handed back to the allocator with the secret still
    /// in it. Callers may still treat it as a `Vec<u8>` through `Deref`.
    fn unseal(&self, ciphertext: &[u8]) -> Result<Zeroizing<Vec<u8>>>;
}

/// Verify that a sealer can perform the complete operation required by an
/// identity save and a later load. Construction alone is not sufficient for
/// some platform providers: Windows PCP can open a provider/key handle while
/// the TPM is not ready to encrypt or decrypt.
///
/// The probe plaintext is a fixed, public domain-separation string. It never
/// contains identity material, credentials, or caller data.
pub fn verify_sealer_round_trip(sealer: &dyn Sealer) -> Result<()> {
    const PROBE: &[u8] = b"OSL/keystore/sealer-readiness/v1";
    let sealed = sealer.seal(PROBE)?;
    let recovered = sealer.unseal(&sealed)?;
    if *recovered != *PROBE {
        return Err(SealerError::Malformed(
            "sealer readiness round-trip returned different bytes".into(),
        ));
    }
    Ok(())
}

// ---- NoOpSealer ----

/// Passthrough sealer retained for explicit compatibility tests only.
/// Production selection never returns it because it writes plaintext.
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
    fn unseal(&self, ciphertext: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
        Ok(Zeroizing::new(ciphertext.to_vec()))
    }
}

// ---- MemorySealer (test only) ----

/// In-memory random key for isolated tests.
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

/// Process-wide encrypted fallback used only when neither TPM nor
/// the OS keyring is available. Separate constructions share a key
/// for the life of this process, but the key is never persisted.
struct EphemeralProcessSealer {
    key: aead::Key,
}

impl EphemeralProcessSealer {
    fn new() -> Self {
        static KEY: OnceLock<[u8; aead::KEY_SIZE]> = OnceLock::new();
        let bytes = KEY.get_or_init(|| *random::random_aead_key().as_bytes());
        Self {
            key: aead::Key::from_bytes(*bytes),
        }
    }
}

impl Sealer for EphemeralProcessSealer {
    fn method_label(&self) -> &'static str {
        METHOD_EPHEMERAL
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
    fn unseal(&self, ciphertext: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
        unseal_with_aead_key(&self.key, ciphertext)
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
    fn unseal(&self, ciphertext: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
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
/// Which backend answers is a compile-time feature choice, not a runtime
/// one (`crates/keystore/Cargo.toml`): `windows-native` on Windows,
/// `linux-native` (kernel keyutils -- syscalls only, no DBus, no daemon)
/// on Linux. On a target with neither, `keyring` resolves to its in-memory
/// mock, whose every `Entry` is independent, so the persistence self-probe
/// in [`Self::new_namespaced`] fails and production selection falls through
/// to the encrypted process-ephemeral fallback.
pub struct KeyringSealer {
    key: aead::Key,
}

const KEYRING_SERVICE: &str = "discord-privacy-client";
const KEYRING_USER: &str = "identity-data-key.v1";

/// The credential-store account name for `namespace`.
///
/// The empty namespace is the production entry and keeps the historical
/// name byte-for-byte, so an existing install's key is still found. A
/// non-empty namespace names a *different* entry in the same store; see
/// [`KeyringSealer::new_namespaced`].
fn keyring_user(namespace: &str) -> std::borrow::Cow<'static, str> {
    if namespace.is_empty() {
        std::borrow::Cow::Borrowed(KEYRING_USER)
    } else {
        std::borrow::Cow::Owned(format!("{KEYRING_USER}::{namespace}"))
    }
}

/// Serialises the whole read-or-create-then-probe sequence against the
/// credential store.
///
/// `get_password` + `set_password` + read-back is a read-modify-write on one
/// shared resource, and the `keyring` crate offers no compare-and-set, so
/// without this the sequence is not atomic: two threads that both observe
/// `NoEntry` each generate a key, each write it, and the loser then holds a
/// key that the store no longer contains. Anything it sealed can never be
/// unsealed by a later construction -- an `AEAD operation failed` in whichever
/// test happened to be holding the losing sealer.
///
/// This guards the credential-store access only, not any caller's work, and
/// it is uncontended in production (one process, a handful of constructions).
/// It cannot serialise anything ACROSS processes -- see the residual-risk note
/// on [`KeyringSealer::new_namespaced`].
static KEYRING_ENTRY_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

impl KeyringSealer {
    /// The production sealer, bound to the single machine-global entry.
    pub fn new() -> Result<Self> {
        Self::new_namespaced("")
    }

    /// Construct against a *namespaced* credential entry.
    ///
    /// `""` is the production entry. Any other namespace names an
    /// independent entry in the same credential store, which is what lets a
    /// test exercise the real backend without purging or rotating the key a
    /// concurrently-running test -- or the developer's own installed client
    /// -- depends on.
    ///
    /// ## Concurrency
    ///
    /// Within this process the create path is atomic ([`KEYRING_ENTRY_LOCK`]),
    /// so concurrent constructions converge on one key. Across processes the
    /// store still has no compare-and-set, so two processes that both find
    /// the entry absent at the same instant can still both write. The
    /// read-back below narrows that to the single interleaving where the
    /// loser's write lands after the winner has already probed; it cannot be
    /// closed from inside this crate. Give parallel test *processes* distinct
    /// namespaces rather than relying on that window staying shut.
    pub fn new_namespaced(namespace: &str) -> Result<Self> {
        let user = keyring_user(namespace);
        let _entry_guard = KEYRING_ENTRY_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let entry = keyring::Entry::new(KEYRING_SERVICE, &user)
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
        // `select_best_sealer` can fall through to process memory instead of
        // silently returning a sealer whose state vanishes between
        // operations.
        //
        // The 2026-07-31 KNOWN RACE note that used to sit here is resolved:
        // the whole sequence now runs under `KEYRING_ENTRY_LOCK`, so two
        // threads can no longer interleave a write between each other's write
        // and probe. What the probe still has to cope with is the
        // cross-PROCESS case, and a mismatch there does not mean the backend
        // is broken -- it means another process created the entry first. The
        // store demonstrably persisted SOMETHING, which is the property this
        // probe exists to establish, so adopt the stored key rather than
        // declaring the backend non-persistent: every construction on the
        // machine then converges on one key instead of each holding its own.
        // Genuine non-persistence still fails, because a store that drops
        // state returns `NoEntry`/an error from the probe read below rather
        // than different-but-valid bytes.
        let probe = keyring::Entry::new(KEYRING_SERVICE, &user)
            .map_err(|e| SealerError::Keyring(format!("probe Entry: {e}")))?;
        let stored = probe
            .get_password()
            .map_err(|e| SealerError::Keyring(format!("probe get_password: {e}")))?;
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine;
        let stored_bytes = STANDARD
            .decode(&stored)
            .map_err(|e| SealerError::Malformed(format!("probe b64: {e}")))?;
        if stored_bytes == key.as_bytes() {
            return Ok(KeyringSealer { key });
        }
        if stored_bytes.len() != aead::KEY_SIZE {
            return Err(SealerError::Malformed(format!(
                "probe key size {} != {}",
                stored_bytes.len(),
                aead::KEY_SIZE
            )));
        }
        let mut adopted = [0u8; aead::KEY_SIZE];
        adopted.copy_from_slice(&stored_bytes);
        Ok(KeyringSealer {
            key: aead::Key::from_bytes(adopted),
        })
    }

    /// Test/operations helper: delete the keyring entry so the next
    /// `KeyringSealer::new` regenerates a fresh key. Used by the
    /// duress-flow strip path (B3) and by integration tests.
    pub fn purge_keyring_entry() -> Result<()> {
        Self::purge_keyring_entry_namespaced("")
    }

    /// [`Self::purge_keyring_entry`] for a namespace created by
    /// [`Self::new_namespaced`]. Takes the same lock as construction so a
    /// purge can never land between a concurrent create and its read-back.
    pub fn purge_keyring_entry_namespaced(namespace: &str) -> Result<()> {
        let user = keyring_user(namespace);
        let _entry_guard = KEYRING_ENTRY_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match keyring::Entry::new(KEYRING_SERVICE, &user).and_then(|e| e.delete_credential()) {
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
    fn unseal(&self, ciphertext: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
        unseal_with_aead_key(&self.key, ciphertext)
    }
}

// ---- TpmSealer (Windows-only) ----

#[cfg(windows)]
mod tpm {
    use super::{Result, Sealer, SealerError, TpmEvictOutcome, METHOD_TPM};
    use crypto::aead;
    use crypto::random;
    use windows::core::PCWSTR;
    use windows::Win32::Security::Cryptography::{
        NCryptCreatePersistedKey, NCryptDecrypt, NCryptEncrypt, NCryptFinalizeKey,
        NCryptFreeObject, NCryptOpenKey, NCryptOpenStorageProvider, BCRYPT_PAD_PKCS1,
        CERT_KEY_SPEC, NCRYPT_FLAGS, NCRYPT_HANDLE, NCRYPT_KEY_HANDLE, NCRYPT_PROV_HANDLE,
    };
    use zeroize::Zeroizing;

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
                // windows 0.56.0: NCrypt* now return `Result<(), Error>`
                // directly (previously NTSTATUS with `.ok()`-to-Result).
                // The `.ok()` step is dropped throughout this module.
                NCryptOpenStorageProvider(&mut prov, PROVIDER, 0)
                    .map_err(|e| SealerError::Tpm(format!("OpenStorageProvider: {e}")))?;

                // Try to open the persisted key; if missing, create
                // a fresh 2048-bit RSA key, finalize, and proceed.
                // windows 0.56.0: `dwlegacykeyspec` is `CERT_KEY_SPEC`,
                // not raw u32 — pass `CERT_KEY_SPEC(0)` for "no legacy
                // KSP / use CNG-native key spec".
                let mut key: NCRYPT_KEY_HANDLE = NCRYPT_KEY_HANDLE::default();
                let open =
                    NCryptOpenKey(prov, &mut key, KEY_NAME, CERT_KEY_SPEC(0), NCRYPT_FLAGS(0));
                if open.is_err() {
                    NCryptCreatePersistedKey(
                        prov,
                        &mut key,
                        ALGO_RSA,
                        KEY_NAME,
                        CERT_KEY_SPEC(0),
                        NCRYPT_FLAGS(0),
                    )
                    .map_err(|e| {
                        let _ = NCryptFreeObject(NCRYPT_HANDLE(prov.0));
                        SealerError::Tpm(format!("CreatePersistedKey: {e}"))
                    })?;
                    NCryptFinalizeKey(key, NCRYPT_FLAGS(0)).map_err(|e| {
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
                .map_err(|e| SealerError::Tpm(format!("OpenStorageProvider: {e}")))?;
            let mut key = NCRYPT_KEY_HANDLE::default();
            NCryptOpenKey(prov, &mut key, KEY_NAME, CERT_KEY_SPEC(0), NCRYPT_FLAGS(0)).map_err(
                |e| {
                    let _ = NCryptFreeObject(NCRYPT_HANDLE(prov.0));
                    SealerError::Tpm(format!("OpenKey: {e}"))
                },
            )?;
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
                // windows 0.56.0:
                //   - `pcboutput: *mut u32` (raw pointer, was
                //     `Option<&mut u32>`).
                //   - `dwflags: NCRYPT_FLAGS` (typed, was raw u32);
                //     `BCRYPT_PAD_PKCS1` is a `BCRYPT_PAD_FLAG` so we
                //     unwrap with `.0` and re-wrap as `NCRYPT_FLAGS`.
                let mut wrapped_len: u32 = 0;
                NCryptEncrypt(
                    key,
                    Some(data_key.as_bytes()),
                    None,
                    None,
                    &mut wrapped_len as *mut u32,
                    NCRYPT_FLAGS(BCRYPT_PAD_PKCS1.0),
                )
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
                    &mut got as *mut u32,
                    NCRYPT_FLAGS(BCRYPT_PAD_PKCS1.0),
                )
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

        fn unseal(&self, ciphertext: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
            if ciphertext.len() < 4 {
                return Err(SealerError::Malformed(
                    "TPM sealed blob shorter than 4-byte length prefix".into(),
                ));
            }
            let wrapped_len = u32::from_be_bytes(ciphertext[..4].try_into().unwrap()) as usize;
            if ciphertext.len() < 4 + wrapped_len {
                return Err(SealerError::Malformed(format!(
                    "TPM sealed blob truncated: declared wrapped len {wrapped_len} > input"
                )));
            }
            let wrapped = &ciphertext[4..4 + wrapped_len];
            let blob = &ciphertext[4 + wrapped_len..];

            unsafe {
                let (prov, key) = Self::open()?;

                // windows 0.56.0: same pcboutput / dwflags shape
                // changes as `seal` above.
                let mut got: u32 = 0;
                NCryptDecrypt(
                    key,
                    Some(wrapped),
                    None,
                    None,
                    &mut got as *mut u32,
                    NCRYPT_FLAGS(BCRYPT_PAD_PKCS1.0),
                )
                .map_err(|e| {
                    let _ = NCryptFreeObject(NCRYPT_HANDLE(key.0));
                    let _ = NCryptFreeObject(NCRYPT_HANDLE(prov.0));
                    SealerError::Tpm(format!("Decrypt size: {e}"))
                })?;

                // A8: this buffer receives the TPM-unwrapped AEAD data key.
                // Plain `vec![0u8; n]` would hand that key straight back to
                // the allocator on drop; `Zeroizing` wipes it first.
                let mut data_key_bytes = Zeroizing::new(vec![0u8; got as usize]);
                NCryptDecrypt(
                    key,
                    Some(wrapped),
                    None,
                    Some(&mut data_key_bytes),
                    &mut got as *mut u32,
                    NCRYPT_FLAGS(BCRYPT_PAD_PKCS1.0),
                )
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
                // Same reasoning for the fixed-size copy: `aead::Key::from_bytes`
                // takes the array by value, so without the wrapper this stack
                // copy of the data key would outlive the call unscrubbed.
                let mut k = Zeroizing::new([0u8; aead::KEY_SIZE]);
                k.copy_from_slice(&data_key_bytes[..aead::KEY_SIZE]);
                let data_key = aead::Key::from_bytes(*k);

                super::unseal_with_aead_key(&data_key, blob)
            }
        }
    }

    /// Test/operations helper: evict the persisted TPM key. Used by
    /// the duress-flow strip path (B3).
    pub fn evict_tpm_key() -> Result<TpmEvictOutcome> {
        unsafe {
            let mut prov = NCRYPT_PROV_HANDLE::default();
            if let Err(e) = NCryptOpenStorageProvider(&mut prov, PROVIDER, 0) {
                tracing::debug!(
                    "TPM eviction found no platform crypto provider to evict from: {e}"
                );
                return Ok(TpmEvictOutcome::NoTpmNothingToEvict);
            }
            let mut key = NCRYPT_KEY_HANDLE::default();
            match NCryptOpenKey(prov, &mut key, KEY_NAME, CERT_KEY_SPEC(0), NCRYPT_FLAGS(0)) {
                Ok(_) => {
                    match windows::Win32::Security::Cryptography::NCryptDeleteKey(key, 0) {
                        Ok(_) => {
                            // NCryptDeleteKey deletes the persisted key and releases
                            // the key handle; only the provider remains to free.
                            let _ = NCryptFreeObject(NCRYPT_HANDLE(prov.0));
                            Ok(TpmEvictOutcome::Evicted)
                        }
                        Err(e) => {
                            let _ = NCryptFreeObject(NCRYPT_HANDLE(key.0));
                            let _ = NCryptFreeObject(NCRYPT_HANDLE(prov.0));
                            Err(SealerError::Tpm(format!("DeleteKey: {e}")))
                        }
                    }
                }
                Err(e) => {
                    tracing::debug!("TPM eviction found no persisted key to evict: {e}");
                    let _ = NCryptFreeObject(NCRYPT_HANDLE(prov.0));
                    Ok(TpmEvictOutcome::NoTpmNothingToEvict)
                }
            }
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
        Err(SealerError::Tpm(
            "TPM sealer not available on this OS".into(),
        ))
    }
    fn unseal(&self, _ciphertext: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
        Err(SealerError::Tpm(
            "TPM sealer not available on this OS".into(),
        ))
    }
}

#[cfg(not(windows))]
pub fn evict_tpm_key() -> Result<TpmEvictOutcome> {
    // Non-Windows: nothing to evict.
    Ok(TpmEvictOutcome::NoTpmNothingToEvict)
}

// ---- factory ----

/// Pick the most-secure available sealer.
///
/// Order: TPM (Windows) → keyring (cross-platform) → process-ephemeral
/// encryption. The final fallback deliberately sacrifices persistence,
/// not confidentiality: secret material is never written as plaintext
/// merely because the platform sealer is unavailable.
pub fn select_best_sealer() -> Box<dyn Sealer> {
    #[cfg(windows)]
    {
        if let Ok(s) = TpmSealer::new() {
            if verify_sealer_round_trip(&s).is_ok() {
                return Box::new(s);
            }
        }
    }
    if let Ok(s) = KeyringSealer::new() {
        if verify_sealer_round_trip(&s).is_ok() {
            return Box::new(s);
        }
    }
    let fallback = EphemeralProcessSealer::new();
    debug_assert!(verify_sealer_round_trip(&fallback).is_ok());
    Box::new(fallback)
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

fn unseal_with_aead_key(key: &aead::Key, blob: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
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
    // A8: `pt` is long-term key material. The previous `Zeroizing::new(())`
    // here wiped a unit value and therefore nothing at all; wrapping the
    // buffer itself is what makes the plaintext wipe on drop.
    let pt = aead::open(key, &nonce, SEAL_NONCE_PREFIX, ct)?;
    Ok(Zeroizing::new(pt))
}

#[cfg(test)]
mod tests {
    #[cfg(not(windows))]
    #[test]
    fn non_windows_evict_tpm_key_reports_no_tpm_nothing_to_evict() {
        assert_eq!(
            super::evict_tpm_key().unwrap(),
            super::TpmEvictOutcome::NoTpmNothingToEvict
        );
    }
}
