//! Ed25519 signature primitive (RFC 8032).
//!
//! Used by B4's prekey infrastructure for the SPK signature and the
//! replenish-batch signature. **Deviation from design doc**: the
//! design specifies "detached signature over `SPK` by `IK_X25519`",
//! which in Signal-style construction implies xeddsa (signing with
//! an X25519 key via Curve25519 mapping). v1 alpha ships a separate
//! Ed25519 identity-signing key alongside `IK_X25519` because:
//!
//! 1. xeddsa isn't in any maintained Rust crate; we'd hand-roll it
//!    and that's audit territory we don't want for prototype work.
//! 2. Ed25519 is the standard signing primitive in `ed25519-dalek`
//!    and Node's built-in `crypto.verify` — both ends of the wire
//!    talk to it natively.
//!
//! v1 stable migrates to xeddsa per the design doc; the
//! implementation lives behind this module's API so call sites don't
//! change.
//!
//! ## Library
//!
//! `ed25519-dalek` 2.x — pure Rust, audited (TLS-WG audit 2020),
//! reproducible-build friendly.

use crate::error::{Error, Result};
use ed25519_dalek::{Signature as DalekSignature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use rand_core::{CryptoRng, RngCore};
use zeroize::ZeroizeOnDrop;

pub const SECRET_KEY_SIZE: usize = 32;
pub const PUBLIC_KEY_SIZE: usize = 32;
pub const SIGNATURE_SIZE: usize = 64;

/// Ed25519 secret seed (32 bytes). The full Ed25519 expanded secret
/// derives from this via SHA-512.
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

/// Ed25519 public point (compressed Edwards Y, 32 bytes).
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

/// Ed25519 signature (R || s, 64 bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Signature([u8; SIGNATURE_SIZE]);

impl Signature {
    pub fn from_bytes(bytes: [u8; SIGNATURE_SIZE]) -> Self {
        Signature(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; SIGNATURE_SIZE] {
        &self.0
    }
}

pub fn generate_keypair() -> (SecretKey, PublicKey) {
    generate_keypair_with_rng(&mut OsRng)
}

pub fn generate_keypair_with_rng<R>(rng: &mut R) -> (SecretKey, PublicKey)
where
    R: RngCore + CryptoRng,
{
    let signing = SigningKey::generate(rng);
    let verifying = signing.verifying_key();
    (
        SecretKey::from_bytes(signing.to_bytes()),
        PublicKey::from_bytes(verifying.to_bytes()),
    )
}

pub fn derive_public(secret: &SecretKey) -> PublicKey {
    let signing = SigningKey::from_bytes(secret.as_bytes());
    PublicKey::from_bytes(signing.verifying_key().to_bytes())
}

pub fn sign(secret: &SecretKey, message: &[u8]) -> Signature {
    let signing = SigningKey::from_bytes(secret.as_bytes());
    let sig = signing.sign(message);
    Signature::from_bytes(sig.to_bytes())
}

pub fn verify(public: &PublicKey, message: &[u8], signature: &Signature) -> Result<bool> {
    let verifying = VerifyingKey::from_bytes(public.as_bytes())
        .map_err(|e| Error::Internal(format!("ed25519 VerifyingKey::from_bytes: {e}")))?;
    let sig = DalekSignature::from_bytes(signature.as_bytes());
    Ok(verifying.verify(message, &sig).is_ok())
}
