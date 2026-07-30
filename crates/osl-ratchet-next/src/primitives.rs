//! Thin, self-contained wrappers over the workspace's already-vendored
//! primitives. **No primitive is implemented here.**
//!
//! - X25519: `x25519-dalek` 2.0 (RFC 7748)
//! - ML-KEM-768: RustCrypto `ml-kem` 0.2 (FIPS 203)
//! - HKDF-SHA256: RustCrypto `hkdf` + `sha2` (RFC 5869)
//! - XChaCha20-Poly1305: RustCrypto `chacha20poly1305` (CFRG draft)
//!
//! These duplicate the shapes in `crates/crypto` deliberately: this
//! crate is an isolated research artifact and must not be able to
//! perturb, or be perturbed by, the shipping crypto crate. The code
//! here is a re-wrapping of the same upstream libraries, not a
//! re-implementation of any algorithm.
//!
//! ## Contributory-behaviour check
//!
//! `x25519-dalek` 2.0 returns an all-zero shared secret for low-order
//! peer points instead of erroring. [`dh`] rejects the all-zero result
//! in constant time (`subtle::ConstantTimeEq`), matching
//! `crates/crypto/src/x25519.rs`.
//!
//! ## ML-KEM implicit rejection
//!
//! FIPS 203 §6.3 decapsulation never fails: a wrong key or tampered
//! ciphertext yields a deterministic but unrelated 32 bytes. This
//! protocol therefore never treats decapsulation success as
//! authentication — the PQ secret only ever enters the root KDF, and
//! a wrong secret surfaces as an AEAD tag failure downstream.

use crate::error::{Error, Result};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key as CipherKey, XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use ml_kem::kem::{Decapsulate, Encapsulate};
use ml_kem::{Encoded, EncodedSizeUser, KemCore, MlKem768};
use rand_core::{CryptoRng, RngCore};
use sha2::Sha256;
use subtle::ConstantTimeEq;
use x25519_dalek::{PublicKey as DalekPublic, StaticSecret};
use zeroize::{Zeroize, ZeroizeOnDrop};

pub const X25519_PUB: usize = 32;
pub const X25519_SEC: usize = 32;
pub const SECRET_BYTES: usize = 32;
pub const AEAD_KEY: usize = 32;
/// XChaCha20-Poly1305 takes a 192-bit nonce. Header nonces on the wire
/// are 96 bits (see [`HEADER_NONCE_WIRE`]); the remaining 96 bits are
/// a constant domain separator so the on-wire cost is halved.
pub const AEAD_NONCE: usize = 24;
pub const AEAD_TAG: usize = 16;
pub const HEADER_NONCE_WIRE: usize = 12;

pub const MLKEM_EK: usize = 1184;
pub const MLKEM_DK: usize = 2400;
pub const MLKEM_CT: usize = 1088;

type Ek768 = <MlKem768 as KemCore>::EncapsulationKey;
type Dk768 = <MlKem768 as KemCore>::DecapsulationKey;
type Ct768 = ml_kem::Ciphertext<MlKem768>;

// ---------------------------------------------------------------
// 32-byte secret
// ---------------------------------------------------------------

/// A 32-byte secret. Zeroized on drop; compared in constant time.
#[derive(Clone, ZeroizeOnDrop)]
pub struct Secret32([u8; SECRET_BYTES]);

impl Secret32 {
    pub fn from_bytes(b: [u8; SECRET_BYTES]) -> Self {
        Secret32(b)
    }
    pub fn zero() -> Self {
        Secret32([0u8; SECRET_BYTES])
    }
    pub fn as_bytes(&self) -> &[u8; SECRET_BYTES] {
        &self.0
    }
}

impl PartialEq for Secret32 {
    fn eq(&self, other: &Self) -> bool {
        bool::from(self.0.ct_eq(&other.0))
    }
}
impl Eq for Secret32 {}

impl core::fmt::Debug for Secret32 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Secret32(redacted)")
    }
}

// ---------------------------------------------------------------
// X25519
// ---------------------------------------------------------------

/// X25519 secret scalar. Zeroized on drop.
#[derive(Clone, ZeroizeOnDrop)]
pub struct XSecret([u8; X25519_SEC]);

impl XSecret {
    pub fn from_bytes(b: [u8; X25519_SEC]) -> Self {
        XSecret(b)
    }
    pub fn as_bytes(&self) -> &[u8; X25519_SEC] {
        &self.0
    }
    pub fn public(&self) -> XPublic {
        let s = StaticSecret::from(self.0);
        XPublic(*DalekPublic::from(&s).as_bytes())
    }
}

impl core::fmt::Debug for XSecret {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("XSecret(redacted)")
    }
}

/// X25519 public point.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct XPublic([u8; X25519_PUB]);

impl XPublic {
    pub fn from_bytes(b: [u8; X25519_PUB]) -> Self {
        XPublic(b)
    }
    pub fn as_bytes(&self) -> &[u8; X25519_PUB] {
        &self.0
    }
}

pub fn x25519_keypair<R: RngCore + CryptoRng>(rng: &mut R) -> (XSecret, XPublic) {
    let s = StaticSecret::random_from_rng(&mut *rng);
    let p = DalekPublic::from(&s);
    (XSecret(s.to_bytes()), XPublic(*p.as_bytes()))
}

/// X25519 scalar multiplication with the RFC 7748 §6.1
/// contributory-behaviour check.
pub fn dh(secret: &XSecret, peer: &XPublic) -> Result<Secret32> {
    let s = StaticSecret::from(secret.0);
    let p = DalekPublic::from(peer.0);
    let shared = s.diffie_hellman(&p);
    let bytes = *shared.as_bytes();
    if bool::from(bytes.ct_eq(&[0u8; 32])) {
        return Err(Error::DegenerateDh);
    }
    Ok(Secret32(bytes))
}

// ---------------------------------------------------------------
// ML-KEM-768
// ---------------------------------------------------------------

/// ML-KEM-768 encapsulation key (public, 1184 bytes).
#[derive(Clone)]
pub struct KemPublic(Ek768);

impl KemPublic {
    /// Decode from the FIPS 203 byte serialization.
    ///
    /// Returns `Result` rather than panicking on a length mismatch even
    /// though the array type makes that unreachable: the crate's
    /// no-panic guarantee is easier to *verify* if there is no
    /// `unreachable!` anywhere on a parsing path at all.
    pub fn from_bytes(b: &[u8; MLKEM_EK]) -> Result<Self> {
        Self::from_slice(b.as_slice())
    }

    /// Parse from an unsized slice, returning an error on the wrong
    /// length rather than panicking. Used on the wire path.
    pub fn from_slice(b: &[u8]) -> Result<Self> {
        let enc = Encoded::<Ek768>::try_from(b)
            .map_err(|_| Error::Malformed("ML-KEM encapsulation key length"))?;
        Ok(KemPublic(<Ek768 as EncodedSizeUser>::from_bytes(&enc)))
    }

    pub fn to_bytes(&self) -> [u8; MLKEM_EK] {
        let mut out = [0u8; MLKEM_EK];
        out.copy_from_slice(self.0.as_bytes().as_slice());
        out
    }
}

impl core::fmt::Debug for KemPublic {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("KemPublic(..)")
    }
}

/// ML-KEM-768 decapsulation key (secret, 2400 bytes).
pub struct KemSecret(Dk768);

impl KemSecret {
    /// Decode from the FIPS 203 byte serialization. See
    /// [`KemPublic::from_bytes`] for why this returns `Result`.
    pub fn from_bytes(b: &[u8; MLKEM_DK]) -> Result<Self> {
        Self::from_slice(b.as_slice())
    }

    pub fn from_slice(b: &[u8]) -> Result<Self> {
        let enc = Encoded::<Dk768>::try_from(b)
            .map_err(|_| Error::Malformed("ML-KEM decapsulation key length"))?;
        Ok(KemSecret(<Dk768 as EncodedSizeUser>::from_bytes(&enc)))
    }

    /// Serialize to the FIPS 203 byte form.
    ///
    /// The returned array is **not** self-zeroizing. Its only caller is
    /// [`crate::session::Session::export_state`], whose output the
    /// integrator is required to seal at rest (see `MIGRATION.md`).
    pub fn to_bytes(&self) -> [u8; MLKEM_DK] {
        let mut out = [0u8; MLKEM_DK];
        out.copy_from_slice(self.0.as_bytes().as_slice());
        out
    }
}

impl Clone for KemSecret {
    fn clone(&self) -> Self {
        KemSecret(self.0.clone())
    }
}

impl core::fmt::Debug for KemSecret {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("KemSecret(redacted)")
    }
}

pub fn kem_keypair<R: RngCore + CryptoRng>(rng: &mut R) -> (KemSecret, KemPublic) {
    let (dk, ek) = MlKem768::generate(rng);
    (KemSecret(dk), KemPublic(ek))
}

/// Returns `(ciphertext, shared_secret)`.
pub fn kem_encapsulate<R: RngCore + CryptoRng>(
    ek: &KemPublic,
    rng: &mut R,
) -> Result<([u8; MLKEM_CT], Secret32)> {
    let (ct, mut ss) = ek
        .0
        .encapsulate(rng)
        .map_err(|_| Error::Internal("ML-KEM encapsulate"))?;
    let mut ct_bytes = [0u8; MLKEM_CT];
    ct_bytes.copy_from_slice(ct.as_slice());
    let mut ss_bytes = [0u8; SECRET_BYTES];
    ss_bytes.copy_from_slice(ss.as_slice());
    ss.zeroize();
    Ok((ct_bytes, Secret32(ss_bytes)))
}

/// Implicit-rejection decapsulation (FIPS 203 §6.3): never fails on a
/// tampered ciphertext, returns unrelated bytes instead.
pub fn kem_decapsulate(dk: &KemSecret, ct: &[u8; MLKEM_CT]) -> Result<Secret32> {
    let typed = Ct768::try_from(ct.as_slice())
        .map_err(|_| Error::Internal("MLKEM_CT length mismatch"))?;
    let mut ss = dk
        .0
        .decapsulate(&typed)
        .map_err(|_| Error::Internal("ML-KEM decapsulate"))?;
    let mut ss_bytes = [0u8; SECRET_BYTES];
    ss_bytes.copy_from_slice(ss.as_slice());
    ss.zeroize();
    Ok(Secret32(ss_bytes))
}

// ---------------------------------------------------------------
// HKDF-SHA256
// ---------------------------------------------------------------

/// HKDF-SHA256 extract-and-expand into `N` bytes.
///
/// `N` is a const generic so every call site is a fixed size and the
/// `copy_from_slice` below can never mismatch.
pub fn hkdf<const N: usize>(salt: &[u8], ikm: &[u8], info: &[u8]) -> Result<[u8; N]> {
    let salt_opt = if salt.is_empty() { None } else { Some(salt) };
    let hk = Hkdf::<Sha256>::new(salt_opt, ikm);
    let mut out = [0u8; N];
    hk.expand(info, &mut out)
        .map_err(|_| Error::Internal("HKDF expand length"))?;
    Ok(out)
}

// ---------------------------------------------------------------
// AEAD
// ---------------------------------------------------------------

/// XChaCha20-Poly1305 seal. Returns ciphertext with the 16-byte tag
/// appended.
pub fn aead_seal(
    key: &[u8; AEAD_KEY],
    nonce: &[u8; AEAD_NONCE],
    ad: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new(CipherKey::from_slice(key));
    cipher
        .encrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: plaintext,
                aad: ad,
            },
        )
        .map_err(|_| Error::Internal("AEAD seal"))
}

/// XChaCha20-Poly1305 open. Every failure mode collapses to
/// [`Error::AuthFailed`] so no oracle is exposed.
pub fn aead_open(
    key: &[u8; AEAD_KEY],
    nonce: &[u8; AEAD_NONCE],
    ad: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new(CipherKey::from_slice(key));
    cipher
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad: ad,
            },
        )
        .map_err(|_| Error::AuthFailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 5869 §A.1 Test Case 1 — confirms the HKDF wiring (extract
    /// with salt + expand with info) is the standard construction and
    /// not something bespoke.
    #[test]
    fn hkdf_matches_rfc5869_case1() {
        let ikm = [0x0bu8; 22];
        let salt: [u8; 13] = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c,
        ];
        let info: [u8; 10] = [0xf0, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9];
        let okm = hkdf::<42>(&salt, &ikm, &info).expect("hkdf");
        assert_eq!(
            hex::encode(okm),
            "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865"
        );
    }

    /// RFC 7748 §6.1 — the published X25519 test vector. Confirms the
    /// dalek wiring (clamping, byte order) is the RFC construction.
    #[test]
    fn x25519_matches_rfc7748() {
        let alice_sk: [u8; 32] =
            hex_arr("77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a");
        let alice_pk: [u8; 32] =
            hex_arr("8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a");
        let bob_sk: [u8; 32] =
            hex_arr("5dab087e624a8a4b79e17f8b83800ee66f3bb1292618b6fd1c2f8b27ff88e0eb");
        let bob_pk: [u8; 32] =
            hex_arr("de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f");
        let expect: [u8; 32] =
            hex_arr("4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742");

        assert_eq!(XSecret::from_bytes(alice_sk).public().as_bytes(), &alice_pk);
        assert_eq!(XSecret::from_bytes(bob_sk).public().as_bytes(), &bob_pk);
        let ab = dh(&XSecret::from_bytes(alice_sk), &XPublic::from_bytes(bob_pk)).expect("dh");
        let ba = dh(&XSecret::from_bytes(bob_sk), &XPublic::from_bytes(alice_pk)).expect("dh");
        assert_eq!(ab.as_bytes(), &expect);
        assert_eq!(ba.as_bytes(), &expect);
    }

    #[test]
    fn dh_rejects_low_order_points() {
        // All-zero u-coordinate: the canonical low-order point.
        let sk = XSecret::from_bytes([7u8; 32]);
        assert_eq!(
            dh(&sk, &XPublic::from_bytes([0u8; 32])),
            Err(Error::DegenerateDh)
        );
    }

    #[test]
    fn kem_roundtrips_and_implicitly_rejects() {
        let mut rng = rand::rngs::OsRng;
        let (dk, ek) = kem_keypair(&mut rng);
        let (ct, ss) = kem_encapsulate(&ek, &mut rng).expect("encaps");
        assert_eq!(kem_decapsulate(&dk, &ct).expect("decaps"), ss);

        let mut bad = ct;
        bad[0] ^= 0x01;
        // FIPS 203 implicit rejection: no error, just a different secret.
        let other = kem_decapsulate(&dk, &bad).expect("implicit rejection returns bytes");
        assert_ne!(other, ss);
    }

    #[test]
    fn kem_key_serialization_roundtrips() {
        let mut rng = rand::rngs::OsRng;
        let (dk, ek) = kem_keypair(&mut rng);
        let ek2 = KemPublic::from_bytes(&ek.to_bytes()).expect("ek");
        let dk2 = KemSecret::from_bytes(&dk.to_bytes()).expect("dk");
        let (ct, ss) = kem_encapsulate(&ek2, &mut rng).expect("encaps");
        assert_eq!(kem_decapsulate(&dk2, &ct).expect("decaps"), ss);
    }

    #[test]
    fn kem_from_slice_rejects_wrong_length() {
        assert!(KemPublic::from_slice(&[0u8; 10]).is_err());
        assert!(KemSecret::from_slice(&[0u8; 10]).is_err());
    }

    #[test]
    fn aead_detects_tampering() {
        let key = [3u8; AEAD_KEY];
        let nonce = [4u8; AEAD_NONCE];
        let ct = aead_seal(&key, &nonce, b"ad", b"hello").expect("seal");
        assert_eq!(
            aead_open(&key, &nonce, b"ad", &ct).expect("open"),
            b"hello".to_vec()
        );
        let mut bad = ct.clone();
        bad[0] ^= 1;
        assert_eq!(aead_open(&key, &nonce, b"ad", &bad), Err(Error::AuthFailed));
        assert_eq!(aead_open(&key, &nonce, b"AD", &ct), Err(Error::AuthFailed));
        assert_eq!(aead_open(&key, &[9u8; AEAD_NONCE], b"ad", &ct), Err(Error::AuthFailed));
        assert_eq!(aead_open(&[9u8; AEAD_KEY], &nonce, b"ad", &ct), Err(Error::AuthFailed));
    }

    fn hex_arr(s: &str) -> [u8; 32] {
        let v = hex::decode(s).expect("hex");
        let mut out = [0u8; 32];
        out.copy_from_slice(&v);
        out
    }
}
