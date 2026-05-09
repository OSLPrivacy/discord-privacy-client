//! Identity keypair: X25519 + Ed25519 + ML-KEM-768 long-term identity
//! keys.
//!
//! Held in memory as raw bytes (with `zeroize` on the secret halves)
//! and reconstituted into typed `crypto::*` keys on demand.
//!
//! See the crate-level docs for the (un)safety story.

use crypto::{ed25519, ml_kem_768, x25519};
use zeroize::Zeroizing;

/// On-disk identity blob format version. Bumped every time the field
/// shape changes; loaders reject mismatched versions with a clear
/// upgrade error.
///
/// History:
/// - v1: plain-file JSON with base64 fields, INSECURE banner.
/// - v2: sealed two-layer JSON; outer wrapper carries a method tag
///   (`tpm-pcp` / `keyring` / `noop-insecure` / `memory-test`) and
///   the sealed inner blob bytes are AEAD-protected. (B1.)
/// - v3: inner blob gains an Ed25519 identity-signing keypair for
///   B4's prekey-bundle signature path. (Deviation from design doc:
///   v1 stable migrates to xeddsa over `IK_X25519`; this prototype
///   uses a separate Ed25519 key.)
pub const IDENTITY_BLOB_VERSION: u32 = 3;

/// One device's long-term identity. Holds:
/// - the user identifier (Discord ID, pseudonym, etc.),
/// - X25519 identity secret + public (PQXDH DH leg),
/// - Ed25519 identity-signing secret + public (B4 prekey signing),
/// - ML-KEM-768 identity decapsulation-key bytes (zeroized on drop)
///   + encapsulation-key bytes (public).
///
/// The ML-KEM half is stored as raw bytes because RustCrypto
/// `ml-kem` 0.2's `DecapsulationKey` does not implement `Clone`. We
/// reconstruct the typed key via [`Self::mlkem_decapsulation_key`]
/// when needed.
pub struct Identity {
    pub user_id: String,
    pub x25519_secret: x25519::SecretKey,
    pub x25519_public: x25519::PublicKey,
    pub ed25519_secret: ed25519::SecretKey,
    pub ed25519_public: ed25519::PublicKey,
    /// FIPS 203 byte serialization of the ML-KEM-768 decapsulation
    /// key (2400 bytes). `Zeroizing` clears on drop.
    mlkem_secret_bytes: Zeroizing<[u8; ml_kem_768::DECAPSULATION_KEY_SIZE]>,
    /// FIPS 203 byte serialization of the ML-KEM-768 encapsulation
    /// key (1184 bytes). Public; safe to expose.
    pub mlkem_public_bytes: [u8; ml_kem_768::ENCAPSULATION_KEY_SIZE],
}

impl Identity {
    /// Build an identity from raw bytes (e.g. loaded from storage).
    #[allow(clippy::too_many_arguments)]
    pub fn from_bytes(
        user_id: String,
        x25519_secret_bytes: [u8; x25519::SECRET_KEY_SIZE],
        x25519_public_bytes: [u8; x25519::PUBLIC_KEY_SIZE],
        ed25519_secret_bytes: [u8; ed25519::SECRET_KEY_SIZE],
        ed25519_public_bytes: [u8; ed25519::PUBLIC_KEY_SIZE],
        mlkem_secret_bytes: [u8; ml_kem_768::DECAPSULATION_KEY_SIZE],
        mlkem_public_bytes: [u8; ml_kem_768::ENCAPSULATION_KEY_SIZE],
    ) -> Self {
        Identity {
            user_id,
            x25519_secret: x25519::SecretKey::from_bytes(x25519_secret_bytes),
            x25519_public: x25519::PublicKey::from_bytes(x25519_public_bytes),
            ed25519_secret: ed25519::SecretKey::from_bytes(ed25519_secret_bytes),
            ed25519_public: ed25519::PublicKey::from_bytes(ed25519_public_bytes),
            mlkem_secret_bytes: Zeroizing::new(mlkem_secret_bytes),
            mlkem_public_bytes,
        }
    }

    /// Reconstitute the typed ML-KEM-768 decapsulation key from the
    /// stored bytes. Use the returned key once and drop it; do not
    /// hold it longer than necessary.
    pub fn mlkem_decapsulation_key(&self) -> ml_kem_768::DecapsulationKey {
        ml_kem_768::DecapsulationKey::from_bytes(&self.mlkem_secret_bytes)
    }

    /// Reconstitute the typed ML-KEM-768 encapsulation key.
    pub fn mlkem_encapsulation_key(&self) -> ml_kem_768::EncapsulationKey {
        ml_kem_768::EncapsulationKey::from_bytes(&self.mlkem_public_bytes)
    }

    /// Borrow the raw decapsulation-key bytes (used by storage). Caller
    /// must not hold this past a save/load boundary.
    pub fn mlkem_secret_bytes(&self) -> &[u8; ml_kem_768::DECAPSULATION_KEY_SIZE] {
        &self.mlkem_secret_bytes
    }
}

/// Generate a fresh identity keypair using `OsRng`.
pub fn generate_identity(user_id: String) -> Identity {
    let (x25519_secret, x25519_public) = x25519::generate_keypair();
    let (ed25519_secret, ed25519_public) = ed25519::generate_keypair();
    let (mlkem_decap, mlkem_encap) = ml_kem_768::generate_keypair();

    let mlkem_secret_bytes = {
        let z = mlkem_decap.to_bytes();
        let mut out = [0u8; ml_kem_768::DECAPSULATION_KEY_SIZE];
        out.copy_from_slice(&*z);
        Zeroizing::new(out)
    };

    Identity {
        user_id,
        x25519_secret,
        x25519_public,
        ed25519_secret,
        ed25519_public,
        mlkem_secret_bytes,
        mlkem_public_bytes: mlkem_encap.to_bytes(),
    }
}
