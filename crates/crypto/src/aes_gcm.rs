//! AES-256-GCM wrapper for Phase 7 wire format v=2.
//!
//! Spec: `docs/phase-7-design.md` §4.2 — every v=2 message is
//! encrypted with a freshly-generated 32-byte AES key under
//! AES-256-GCM with a 12-byte random nonce and a 16-byte
//! authentication tag. Each recipient receives that key wrapped
//! under the static-static ECDH shared secret with their public
//! key, again using AES-256-GCM.
//!
//! Library: RustCrypto `aes-gcm` 0.10 (pure Rust, audited).
//!
//! ## Why a parallel AEAD module
//!
//! The existing [`crate::aead`] module covers XChaCha20-Poly1305
//! with a 24-byte nonce — the construction used by the PQXDH /
//! Double Ratchet path (and by the Phase 4/5 wire format v=1).
//! Phase 7's wire format v=2 commits to AES-256-GCM specifically
//! because:
//!
//! - **12-byte nonce** trims four bytes per message vs.
//!   XChaCha20-Poly1305 — meaningful at multi-recipient scale where
//!   the wrapped-K list pays per-recipient framing.
//! - **GCM** is the broadly audited target for the per-recipient
//!   key-wrap pattern (matches NIST SP 800-38D recommendations).
//! - **Pre-existing reviewer familiarity**: AES-GCM is the
//!   incumbent in the audit firm's coverage set; saves a delta
//!   review pass when the v=2 spec lands.
//!
//! v=1 callers stay on `crate::aead`. v=2 callers use this module.
//! The two coexist; there is no plan to retire either.
//!
//! ## Nonce discipline
//!
//! AES-GCM is **catastrophically broken** under nonce reuse: a single
//! repeat reveals an XOR of the two plaintexts and exposes the
//! authentication subkey, enabling forgery of arbitrary further
//! messages under the same key. Callers MUST generate a fresh
//! random nonce per `seal` call. The 12-byte nonce + 2^32-call
//! safety margin per key is acceptable for the Phase 7 workload:
//! each `K` is single-use (fresh per message) and the per-recipient
//! wrap key is used exactly once per recipient per message.
//!
//! For determinism in tests, `seal_with_nonce` accepts a caller-
//! supplied nonce. Production code MUST NOT use it with a
//! predictable nonce source.

use crate::error::{Error, Result};
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Key as CipherKey, Nonce as CipherNonce};
use zeroize::ZeroizeOnDrop;

pub const KEY_SIZE: usize = 32;
pub const NONCE_SIZE: usize = 12;
pub const TAG_SIZE: usize = 16;

/// 256-bit AES-GCM key. Zeroizes on drop.
#[derive(Clone, ZeroizeOnDrop)]
pub struct Key([u8; KEY_SIZE]);

impl Key {
    pub fn from_bytes(bytes: [u8; KEY_SIZE]) -> Self {
        Key(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; KEY_SIZE] {
        &self.0
    }
}

/// 96-bit AES-GCM nonce. Public; carried on the wire alongside the
/// ciphertext.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Nonce([u8; NONCE_SIZE]);

impl Nonce {
    pub fn from_bytes(bytes: [u8; NONCE_SIZE]) -> Self {
        Nonce(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; NONCE_SIZE] {
        &self.0
    }
}

/// Encrypt `plaintext` under `key` with `nonce` and associated-data
/// `ad`. Returns ciphertext with the 16-byte authentication tag
/// appended.
///
/// Callers MUST ensure `nonce` is fresh per call under the same
/// `key` — AES-GCM is broken under nonce reuse (see module docs).
pub fn seal_with_nonce(key: &Key, nonce: &Nonce, ad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
    let cipher = Aes256Gcm::new(CipherKey::<Aes256Gcm>::from_slice(key.as_bytes()));
    let gnonce = CipherNonce::from_slice(nonce.as_bytes());
    cipher
        .encrypt(
            gnonce,
            Payload {
                msg: plaintext,
                aad: ad,
            },
        )
        .map_err(|e| Error::Internal(format!("AES-256-GCM encrypt: {e}")))
}

/// Encrypt with a fresh random nonce. Returns `(nonce, ciphertext)`.
///
/// Convenience for the common path where the caller does not have
/// a domain-specific nonce derivation.
pub fn seal(key: &Key, ad: &[u8], plaintext: &[u8]) -> Result<(Nonce, Vec<u8>)> {
    let nonce = random_nonce();
    let ct = seal_with_nonce(key, &nonce, ad, plaintext)?;
    Ok((nonce, ct))
}

/// Decrypt and verify `ciphertext` under `key` with `nonce` and
/// associated-data `ad`. Returns plaintext on success; any mismatch
/// (key, nonce, AD, ciphertext, tag) yields `Error::AeadFailure`
/// without distinguishing the cause.
pub fn open(key: &Key, nonce: &Nonce, ad: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
    if ciphertext.len() < TAG_SIZE {
        return Err(Error::AeadFailure);
    }
    let cipher = Aes256Gcm::new(CipherKey::<Aes256Gcm>::from_slice(key.as_bytes()));
    let gnonce = CipherNonce::from_slice(nonce.as_bytes());
    cipher
        .decrypt(
            gnonce,
            Payload {
                msg: ciphertext,
                aad: ad,
            },
        )
        .map_err(|_| Error::AeadFailure)
}

/// Fresh 12-byte nonce from `OsRng`. Module-private helper; callers
/// in this crate use [`seal`] which folds the nonce generation in.
fn random_nonce() -> Nonce {
    use rand::rngs::OsRng;
    use rand::RngCore;
    let mut bytes = [0u8; NONCE_SIZE];
    OsRng.fill_bytes(&mut bytes);
    Nonce::from_bytes(bytes)
}
