//! Discord Privacy Client cryptographic primitives.
//!
//! Status: v1 alpha foundation. AEAD, HKDF, padding, secure random.
//! PQXDH handshake (X25519 + ML-KEM-768), Double Ratchet, sender keys,
//! prekey logic, streaming AEAD for attachments, and the canonical AD
//! encoding are pending future commits and gated on the design docs in
//! `docs/design/`.
//!
//! Library origins (per design doc):
//! - AEAD (XChaCha20-Poly1305): dryoc 0.7 (pure-Rust libsodium).
//! - HKDF-SHA256: RustCrypto `hkdf` + `sha2`.
//! - Random: `rand` 0.8 (`OsRng` / `getrandom` backend).
//! - Zeroization: `zeroize` 1.7.

pub mod aead;
pub mod error;
pub mod hkdf;
pub mod ml_kem_768;
pub mod padding;
pub mod pqxdh;
pub mod random;
pub mod ratchet;
pub mod sender_keys;
pub mod x25519;

pub use error::{Error, Result};
