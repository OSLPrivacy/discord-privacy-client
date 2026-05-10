//! HKDF + AEAD plumbing for the message store.
//!
//! The store does **not** roll new crypto. It composes the same
//! audited primitives the keystore sealer module uses internally:
//!
//! - `crypto::hkdf::derive_32` for key derivation
//!   (HKDF-SHA256 with salt = empty, info = `b"osl-message-store-v1"`).
//! - `crypto::aead::seal` / `crypto::aead::open` for per-row
//!   encryption (XChaCha20-Poly1305 with a fresh random nonce
//!   per `seal` call).
//!
//! ## Per-row layout
//!
//! Each `messages` row stores:
//!
//! - `nonce` (24 bytes) — the random XChaCha20-Poly1305 nonce.
//! - `ciphertext` — the AEAD ciphertext + tag (so any tag failure
//!   surfaces as `StoreError::Corrupted`, not as silent garbage).
//!
//! ## AAD binds row identity
//!
//! The AAD passed to `aead::seal` is the row's
//! `discord_message_id` UTF-8 bytes. This binds the ciphertext to
//! its row-identifier so an attacker who shuffles `ciphertext` /
//! `nonce` blobs across rows produces tag failures instead of
//! cross-row plaintext recovery.

use crate::StoreError;
use crypto::{aead, hkdf, random};

/// HKDF info label. Hard-coded; never read from disk. Bumping
/// the suffix ("-v1" → "-v2") forces a re-derive and would
/// invalidate every existing row's ciphertext, so it pairs with
/// a schema migration that re-encrypts under the new key.
const HKDF_INFO: &[u8] = b"osl-message-store-v1";

/// Derive the message-store AEAD key from the caller-supplied
/// 32-byte identity secret. Returns an opaque
/// [`aead::Key`] suitable for [`seal`] / [`unseal`].
pub(crate) fn derive_key(identity_secret: &[u8; 32]) -> Result<aead::Key, StoreError> {
    let bytes = hkdf::derive_32(&[], identity_secret, HKDF_INFO)
        .map_err(|e| StoreError::Sealer(format!("HKDF derive: {e}")))?;
    Ok(aead::Key::from_bytes(bytes))
}

/// Seal `plaintext` under `key`, binding the AEAD to `aad`.
///
/// Returns `(nonce_bytes, ciphertext_bytes)` for direct insertion
/// as the row's two BLOB columns. Nonce is 24 bytes; ciphertext is
/// `plaintext.len() + 16` bytes (Poly1305 tag).
pub(crate) fn seal(
    key: &aead::Key,
    aad: &[u8],
    plaintext: &[u8],
) -> Result<(Vec<u8>, Vec<u8>), StoreError> {
    let nonce = random::random_nonce();
    let ct = aead::seal(key, &nonce, aad, plaintext)
        .map_err(|e| StoreError::Sealer(format!("AEAD seal: {e}")))?;
    Ok((nonce.as_bytes().to_vec(), ct))
}

/// Unseal a row at runtime. Tag failure produces
/// [`StoreError::Corrupted`] — the canary check at `open` should
/// have caught wrong-secret already, so a tag failure here points
/// at on-disk tampering or a per-row drift.
pub(crate) fn unseal(
    key: &aead::Key,
    aad: &[u8],
    nonce_bytes: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, StoreError> {
    if nonce_bytes.len() != aead::NONCE_SIZE {
        return Err(StoreError::Corrupted(format!(
            "nonce field has wrong length {} (want {})",
            nonce_bytes.len(),
            aead::NONCE_SIZE
        )));
    }
    let mut nb = [0u8; aead::NONCE_SIZE];
    nb.copy_from_slice(nonce_bytes);
    let nonce = aead::Nonce::from_bytes(nb);
    aead::open(key, &nonce, aad, ciphertext)
        .map_err(|_| StoreError::Corrupted("AEAD tag failure".to_string()))
}

/// Same wire as [`unseal`] but with a different error category —
/// used at `open` to validate the canary. AEAD failure here means
/// the caller-supplied secret does not match the one that originally
/// initialised the store, so we surface `Sealer` (a clear
/// "wrong identity_secret" diagnostic) rather than `Corrupted`.
pub(crate) fn unseal_canary(
    key: &aead::Key,
    aad: &[u8],
    nonce_bytes: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, StoreError> {
    if nonce_bytes.len() != aead::NONCE_SIZE {
        return Err(StoreError::Sealer(format!(
            "canary nonce length {} != {}",
            nonce_bytes.len(),
            aead::NONCE_SIZE
        )));
    }
    let mut nb = [0u8; aead::NONCE_SIZE];
    nb.copy_from_slice(nonce_bytes);
    let nonce = aead::Nonce::from_bytes(nb);
    aead::open(key, &nonce, aad, ciphertext)
        .map_err(|_| StoreError::Sealer("wrong identity_secret (canary unseal failed)".to_string()))
}
