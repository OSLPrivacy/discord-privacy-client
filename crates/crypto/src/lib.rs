//! Discord Privacy Client cryptographic primitives.
//!
//! Library origins (per design doc; deviations recorded in
//! `CHANGELOG.md`):
//! - AEAD (XChaCha20-Poly1305): RustCrypto `chacha20poly1305` 0.10
//!   (dryoc 0.7 lacked `crypto_aead_xchacha20poly1305_ietf`).
//! - X25519: RustCrypto `x25519-dalek` 2.0 (dryoc 0.7 lacked
//!   `crypto_scalarmult`).
//! - ML-KEM-768: RustCrypto `ml-kem` 0.2.
//! - HKDF-SHA256: RustCrypto `hkdf` + `sha2`.
//! - Random: `rand` 0.8 (`OsRng` / `getrandom` backend).
//! - Zeroization: `zeroize` 1.7.
//!
//! ## Constant-time discipline (audit findings, layer-4 review pass)
//!
//! All secret-dependent equality checks in this crate go through
//! constant-time primitives:
//!
//! - **AEAD tag verification** is the only place secret-derived bytes
//!   ever drive a comparison decision; the underlying RustCrypto
//!   `chacha20poly1305` crate verifies Poly1305 tags via the `subtle`
//!   crate (`subtle::CtOption` / `ConstantTimeEq`), not raw `==`.
//! - **ML-KEM-768 decapsulation** uses *implicit rejection* per
//!   FIPS 203 §6.3 — wrong-key or tampered-ciphertext inputs return a
//!   deterministic non-secret-revealing 32 B value rather than
//!   erroring; the protocol confirms recipient identity downstream
//!   via the ratchet AEAD.
//! - **X25519 small-subgroup rejection**: the all-zero shared-secret
//!   check in [`x25519::diffie_hellman`] uses
//!   `subtle::ConstantTimeEq::ct_eq`. (`x25519-dalek` 2.0 does not
//!   error on low-order peer points; we restore the
//!   contributory-behaviour property explicitly.)
//!
//! Secret-typed structs deliberately do **not** derive
//! [`PartialEq`]/[`Eq`], preventing accidental non-CT comparisons in
//! callers. The list (audited 2026-05-08): [`aead::Key`],
//! [`ratchet::ChainKey`], [`pqxdh::SessionKey`],
//! [`x25519::SecretKey`], [`x25519::SharedSecret`],
//! [`ml_kem_768::DecapsulationKey`], [`ml_kem_768::SharedSecret`],
//! [`ml_kem_768::Ciphertext`], plus the private `RootKey`,
//! `SenderChainKey`, and `RotationRoot` types inside [`ratchet`] /
//! [`sender_keys`].
//!
//! Public-data types ([`aead::Nonce`], [`x25519::PublicKey`],
//! [`ratchet::Header`], [`sender_keys::Header`],
//! [`attachment::StreamHeader`]) do derive `PartialEq`/`Eq`: their
//! contents are transmitted in the clear and admit no CT-relevant
//! attack.
//!
//! Skipped-message-key cache lookups in [`ratchet`] / [`sender_keys`]
//! linear-scan and early-exit on first AEAD-header match. The number
//! of iterations leaks the matched slot's position. This matches
//! Signal's reference implementation; the cache cap (1000 entries) +
//! 30-day TTL bound the leak. A constant-time scan that always
//! attempts every entry is rejected as a 1000× perf regression for
//! a leak the design doc explicitly accepts.
//!
//! The reviewing cryptographer (paid engagement, v1 stable
//! prerequisite) is the authoritative confirmation of this
//! discipline.

pub mod aead;
pub mod attachment;
pub mod ed25519;
pub mod error;
pub mod hkdf;
pub mod ml_kem_768;
pub mod padding;
pub mod pqxdh;
pub mod random;
pub mod ratchet;
pub mod sender_keys;
pub mod wire;
pub mod x25519;

pub use error::{Error, Result};
