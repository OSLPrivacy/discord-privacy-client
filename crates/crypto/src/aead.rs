//! XChaCha20-Poly1305-IETF AEAD wrapper.
//!
//! Spec: `docs/design/pqxdh-double-ratchet.md` "AEAD" subsection.
//! Library: RustCrypto `chacha20poly1305` (pure Rust, audited).
//!
//! ## Why not dryoc
//!
//! The original design pinned `dryoc` 0.7 for AEAD, but `dryoc` 0.7
//! does not expose `crypto_aead_xchacha20poly1305_ietf` at all. Its
//! only XChaCha20-Poly1305 surfaces are:
//!
//! - `crypto_secretbox_xchacha20poly1305` — no AAD support.
//! - `crypto_secretstream_xchacha20poly1305` — auto-generates the
//!   header (nonce); no caller-supplied-nonce path.
//!
//! Neither satisfies the protocol requirement of AEAD with **both**
//! caller-supplied nonce **and** associated data. RustCrypto's
//! `chacha20poly1305` crate is used instead — same algorithm
//! (CFRG XChaCha20-Poly1305 IETF draft), pure-Rust and reproducible-
//! build friendly. (X25519 also moved off `dryoc` in a subsequent
//! commit when `crypto_scalarmult` turned out to be missing too;
//! `dryoc` was removed from the workspace entirely. See
//! `CHANGELOG.md` for the full swap history.)
//!
//! 192-bit nonce per message; nonce-reuse resistance is the caller's
//! responsibility (in the ratchet construction this is provided by
//! the chain advancing after each message-key derivation).

use crate::error::{Error, Result};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key as CipherKey, XChaCha20Poly1305, XNonce};
use zeroize::ZeroizeOnDrop;

pub const KEY_SIZE: usize = 32;
pub const NONCE_SIZE: usize = 24;
pub const TAG_SIZE: usize = 16;

/// 256-bit AEAD key. Zeroizes on drop.
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

/// 192-bit AEAD nonce.
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

/// Encrypt `plaintext` under `key` with `nonce` and associated-data `ad`.
/// Returns ciphertext with the 16-byte authentication tag appended.
pub fn seal(key: &Key, nonce: &Nonce, ad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new(CipherKey::from_slice(key.as_bytes()));
    let xnonce = XNonce::from_slice(nonce.as_bytes());
    cipher
        .encrypt(
            xnonce,
            Payload {
                msg: plaintext,
                aad: ad,
            },
        )
        .map_err(|e| Error::Internal(format!("XChaCha20-Poly1305 encrypt: {e}")))
}

/// Decrypt and verify `ciphertext` under `key` with `nonce` and associated-data `ad`.
/// Returns plaintext on success; any mismatch (key, nonce, AD, ciphertext, tag)
/// yields `Error::AeadFailure` without distinguishing the cause.
pub fn open(key: &Key, nonce: &Nonce, ad: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
    if ciphertext.len() < TAG_SIZE {
        return Err(Error::AeadFailure);
    }
    let cipher = XChaCha20Poly1305::new(CipherKey::from_slice(key.as_bytes()));
    let xnonce = XNonce::from_slice(nonce.as_bytes());
    cipher
        .decrypt(
            xnonce,
            Payload {
                msg: ciphertext,
                aad: ad,
            },
        )
        .map_err(|_| Error::AeadFailure)
}
