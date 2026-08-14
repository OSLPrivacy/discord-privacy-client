//! Ed25519 with the same shape as `crypto::ed25519`, so a call site reads the
//! same whichever of the two it is looking at.
//!
//! See this crate's `Cargo.toml` for why the primitive is reached directly
//! rather than through `crates/crypto`.

use ed25519_dalek::{Signature as DalekSignature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use zeroize::ZeroizeOnDrop;

pub const SECRET_KEY_SIZE: usize = 32;
pub const PUBLIC_KEY_SIZE: usize = 32;
pub const SIGNATURE_SIZE: usize = 64;

/// Ed25519 secret seed. The expanded secret derives from this via SHA-512.
#[derive(Clone, ZeroizeOnDrop)]
pub struct SecretKey([u8; SECRET_KEY_SIZE]);

impl SecretKey {
    pub fn from_bytes(bytes: [u8; SECRET_KEY_SIZE]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; SECRET_KEY_SIZE] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublicKey([u8; PUBLIC_KEY_SIZE]);

impl PublicKey {
    pub fn from_bytes(bytes: [u8; PUBLIC_KEY_SIZE]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; PUBLIC_KEY_SIZE] {
        &self.0
    }
}

pub fn generate_keypair() -> (SecretKey, PublicKey) {
    let signing = SigningKey::generate(&mut OsRng);
    let public = PublicKey(signing.verifying_key().to_bytes());
    (SecretKey(signing.to_bytes()), public)
}

pub fn derive_public(secret: &SecretKey) -> PublicKey {
    PublicKey(SigningKey::from_bytes(&secret.0).verifying_key().to_bytes())
}

pub fn sign(secret: &SecretKey, message: &[u8]) -> [u8; SIGNATURE_SIZE] {
    SigningKey::from_bytes(&secret.0).sign(message).to_bytes()
}

/// Verify `signature` over `message`. A malformed key is a failed
/// verification, not a panic and not an error the caller has to distinguish:
/// either way this signature did not come from this key.
pub fn verify(public: &PublicKey, message: &[u8], signature: &[u8; SIGNATURE_SIZE]) -> bool {
    let Ok(key) = VerifyingKey::from_bytes(&public.0) else {
        return false;
    };
    key.verify(message, &DalekSignature::from_bytes(signature))
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_signature_verifies_only_under_its_own_key_and_message() {
        let (secret, public) = generate_keypair();
        let (_, other) = generate_keypair();
        let signature = sign(&secret, b"capture event");
        assert!(verify(&public, b"capture event", &signature));
        assert!(!verify(&public, b"capture event.", &signature));
        assert!(!verify(&other, b"capture event", &signature));
        assert_eq!(derive_public(&secret), public);
    }
}
