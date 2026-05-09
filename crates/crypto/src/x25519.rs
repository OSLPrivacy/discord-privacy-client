//! X25519 key exchange wrapper.
//!
//! Spec: `docs/design/pqxdh-double-ratchet.md` "Layer 1: hybrid PQXDH
//! handshake" — DH1, DH2, DH3, DH4 are X25519 scalar multiplications
//! between identity keys, ephemeral keys, signed prekeys, and one-time
//! prekeys.
//!
//! Library: RustCrypto `x25519-dalek` 2.0 (pure Rust, audited).
//!
//! ## Why not dryoc
//!
//! `dryoc` 0.7's `classic` module does not contain `crypto_scalarmult`
//! (confirmed via Windows cargo check). With AEAD already on RustCrypto
//! `chacha20poly1305` and X25519 now on `x25519-dalek`, `dryoc` has no
//! remaining consumers and was removed from the workspace. See
//! `CHANGELOG.md`.
//!
//! ## Output handling
//!
//! `diffie_hellman` returns the raw X25519 shared point in Montgomery
//! u-coordinate form (32 bytes). It is fed directly into the PQXDH
//! HKDF combiner per the design doc; it is **never used as a key
//! directly without HKDF**. RFC 7748 §6.1 calls out that the raw
//! output has known biases and was designed to be used only via a KDF.
//!
//! ## Contributory-behaviour check (manual, required for x25519-dalek 2.0)
//!
//! `x25519-dalek` 2.0 returns an all-zero shared secret when the peer's
//! public point is low-order (identity, order-2/4/8 subgroup elements,
//! Montgomery-form twist points), rather than erroring. To preserve
//! the contributory-behaviour property documented in RFC 7748 §6.1,
//! we explicitly check the shared-secret bytes for all-zero after the
//! scalar-mult and reject with `Error::Internal`. The check uses
//! constant-time comparison via `subtle::ConstantTimeEq` for hygiene,
//! though the only datum revealed by a non-CT check would be "shared
//! secret is all-zero," which is the rejection condition itself.
//!
//! ## Bit clamping
//!
//! Secret-key bytes are stored unclamped. `x25519-dalek` clamps
//! internally per RFC 7748 §5 on each scalar-mult operation. This is
//! also why the RFC 7748 §6.1 test vectors round-trip even though the
//! published private-key bytes are not pre-clamped.

use crate::error::{Error, Result};
use rand::rngs::OsRng;
use rand_core::{CryptoRng, RngCore};
use subtle::ConstantTimeEq;
use x25519_dalek::{PublicKey as DalekPublic, StaticSecret};
use zeroize::ZeroizeOnDrop;

pub const PUBLIC_KEY_SIZE: usize = 32;
pub const SECRET_KEY_SIZE: usize = 32;
pub const SHARED_SECRET_SIZE: usize = 32;

/// X25519 secret scalar (32 bytes). Zeroizes on drop.
#[derive(Clone, ZeroizeOnDrop)]
pub struct SecretKey([u8; SECRET_KEY_SIZE]);

impl SecretKey {
    pub fn from_bytes(bytes: [u8; SECRET_KEY_SIZE]) -> Self {
        SecretKey(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; SECRET_KEY_SIZE] {
        &self.0
    }
}

/// X25519 public point (Montgomery u-coordinate, 32 bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublicKey([u8; PUBLIC_KEY_SIZE]);

impl PublicKey {
    pub fn from_bytes(bytes: [u8; PUBLIC_KEY_SIZE]) -> Self {
        PublicKey(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; PUBLIC_KEY_SIZE] {
        &self.0
    }
}

/// X25519 shared secret. Zeroizes on drop.
///
/// **Always feed this through HKDF** before using as key material.
#[derive(Clone, ZeroizeOnDrop)]
pub struct SharedSecret([u8; SHARED_SECRET_SIZE]);

impl SharedSecret {
    pub fn as_bytes(&self) -> &[u8; SHARED_SECRET_SIZE] {
        &self.0
    }
}

/// Generate a fresh X25519 keypair from `OsRng`.
pub fn generate_keypair() -> (SecretKey, PublicKey) {
    let dalek_secret = StaticSecret::random_from_rng(OsRng);
    let dalek_public = DalekPublic::from(&dalek_secret);
    (
        SecretKey::from_bytes(dalek_secret.to_bytes()),
        PublicKey::from_bytes(*dalek_public.as_bytes()),
    )
}

/// Generate a fresh X25519 keypair using a caller-supplied CSPRNG.
///
/// Used by the PQXDH handshake module for deterministic testing
/// (driving a fixed-bytes RNG to produce known ephemeral keys).
/// `StaticSecret::random_from_rng` consumes 32 bytes from the RNG.
pub fn generate_keypair_with_rng<R>(rng: &mut R) -> (SecretKey, PublicKey)
where
    R: RngCore + CryptoRng,
{
    let dalek_secret = StaticSecret::random_from_rng(&mut *rng);
    let dalek_public = DalekPublic::from(&dalek_secret);
    (
        SecretKey::from_bytes(dalek_secret.to_bytes()),
        PublicKey::from_bytes(*dalek_public.as_bytes()),
    )
}

/// Derive the public X25519 point from a secret scalar.
pub fn derive_public(secret: &SecretKey) -> PublicKey {
    let dalek_secret = StaticSecret::from(*secret.as_bytes());
    let dalek_public = DalekPublic::from(&dalek_secret);
    PublicKey::from_bytes(*dalek_public.as_bytes())
}

/// Compute the X25519 Diffie-Hellman shared secret `s · P`.
///
/// Returns `Error::Internal` if the result is all-zero (the
/// small-subgroup contributory-behaviour check). `x25519-dalek` 2.0
/// produces all-zero output for low-order input points without
/// erroring, so we perform the rejection here.
pub fn diffie_hellman(secret: &SecretKey, peer_public: &PublicKey) -> Result<SharedSecret> {
    let dalek_secret = StaticSecret::from(*secret.as_bytes());
    let dalek_peer = DalekPublic::from(*peer_public.as_bytes());
    let shared = dalek_secret.diffie_hellman(&dalek_peer);
    let shared_bytes = *shared.as_bytes();

    // Constant-time all-zero check; rejects identity and other
    // low-order peer points.
    let zero = [0u8; SHARED_SECRET_SIZE];
    if bool::from(shared_bytes.ct_eq(&zero)) {
        return Err(Error::Internal(
            "X25519 produced all-zero shared secret (low-order peer point)".into(),
        ));
    }

    Ok(SharedSecret(shared_bytes))
}
