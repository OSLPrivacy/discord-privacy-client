//! ML-KEM-768 (Module-Lattice-Based Key Encapsulation Mechanism).
//!
//! Spec: NIST FIPS 203 (Module-Lattice-Based Key-Encapsulation
//! Mechanism Standard). Used in `docs/design/pqxdh-double-ratchet.md`
//! "Layer 1: hybrid PQXDH handshake" as the post-quantum component
//! of the hybrid combiner:
//! `(SS_pq, ct_pq) = ML-KEM-768.Encaps(MLKEM_B_pub)`.
//!
//! Library: RustCrypto `ml-kem` 0.2.
//!
//! ## Sizes (FIPS 203 §6.1, ML-KEM-768 parameter set)
//!
//! | Item                       | Bytes |
//! | ---                        | ---   |
//! | Encapsulation key (public) | 1184  |
//! | Decapsulation key (secret) | 2400  |
//! | Ciphertext                 | 1088  |
//! | Shared secret              | 32    |
//!
//! ## Output handling
//!
//! `SharedSecret` is fed directly into the PQXDH HKDF combiner per
//! the design doc; it is **never used as a key directly without
//! HKDF**. Use [`pqxdh_combine_stub`] for the placeholder combiner
//! that exercises the hybrid shape end-to-end.
//!
//! ## Implicit rejection
//!
//! ML-KEM specifies *implicit rejection*: decapsulation with a wrong
//! decapsulation key OR a tampered ciphertext does **not** error. It
//! returns a deterministic but unrelated 32-byte value derived from
//! a per-key implicit-rejection secret. This is by design (FIPS 203
//! §6.3) to prevent decryption-failure timing oracles. Callers must
//! treat decapsulation success as "got 32 bytes" — the bytes are
//! only meaningful when the protocol confirms the recipient was the
//! intended one (e.g. via the AEAD tag downstream).
//!
//! ## ml-kem 0.2 API notes (set by the first build round)
//!
//! - `EncapsulationKey<P>` and `DecapsulationKey<P>` implement
//!   `EncodedSizeUser`, so byte serialization goes through
//!   `EncodedSizeUser::{from_bytes, as_bytes}`.
//! - `Ciphertext<P>` is itself a typed `Array<u8, N>` — it does
//!   **not** implement `EncodedSizeUser`. We round-trip its bytes
//!   through `Array`'s standard `TryFrom<&[u8]>` and `as_slice`.
//! - `KemCore::generate` and `Encapsulate::encapsulate` take a
//!   `Sized` `R` (no `?Sized`). Our `_with_rng` bounds drop the
//!   relaxed `?Sized`.
//! - `clone_from_slice` on `Array` is deprecated; use `TryFrom`.
//!
//! ## Audit posture
//!
//! `ml-kem` 0.2 audit-posture verification (per design doc) is a
//! **v1 stable** prerequisite, not a v1 alpha gate. FIPS 203 / ACVP
//! known-answer-test integration is deferred; the wrapper exposes
//! [`generate_keypair_with_rng`] / [`encapsulate_with_rng`] so KAT
//! vectors can be plugged in later via a fixed-bytes RNG. ml-kem 0.2
//! has its own KAT suite internally (not surfaced at our boundary),
//! providing upstream confidence in the underlying primitive.

use crate::error::{Error, Result};
use ml_kem::kem::{Decapsulate, Encapsulate};
use ml_kem::{Encoded, EncodedSizeUser, KemCore, MlKem768};
use rand::rngs::OsRng;
use rand_core::{CryptoRng, RngCore};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

pub const ENCAPSULATION_KEY_SIZE: usize = 1184;
pub const DECAPSULATION_KEY_SIZE: usize = 2400;
pub const CIPHERTEXT_SIZE: usize = 1088;
pub const SHARED_SECRET_SIZE: usize = 32;

type Inner768Ek = <MlKem768 as KemCore>::EncapsulationKey;
type Inner768Dk = <MlKem768 as KemCore>::DecapsulationKey;
type Inner768Ct = ml_kem::Ciphertext<MlKem768>;

/// ML-KEM-768 encapsulation key (public). Recipient publishes this;
/// senders use it to compute `(ct, ss)`.
#[derive(Clone)]
pub struct EncapsulationKey(Inner768Ek);

impl EncapsulationKey {
    /// Decode from the FIPS 203 byte serialization.
    ///
    /// `bytes` is statically sized so the inner conversion cannot
    /// fail; the `expect` documents that invariant.
    pub fn from_bytes(bytes: &[u8; ENCAPSULATION_KEY_SIZE]) -> Self {
        let encoded = Encoded::<Inner768Ek>::try_from(bytes.as_slice())
            .expect("ENCAPSULATION_KEY_SIZE matches Encoded<Inner768Ek> array size");
        EncapsulationKey(<Inner768Ek as EncodedSizeUser>::from_bytes(&encoded))
    }

    /// Serialize to the FIPS 203 byte form.
    pub fn to_bytes(&self) -> [u8; ENCAPSULATION_KEY_SIZE] {
        let encoded = self.0.as_bytes();
        let mut out = [0u8; ENCAPSULATION_KEY_SIZE];
        out.copy_from_slice(encoded.as_slice());
        out
    }
}

/// ML-KEM-768 decapsulation key (secret). Held by the recipient.
///
/// Bytes returned by [`Self::to_bytes`] are wrapped in `Zeroizing`
/// to clear them on drop. Callers should TPM-seal these bytes
/// immediately (see `keystore` crate, forthcoming) and avoid holding
/// decapsulation-key bytes in plaintext on disk.
pub struct DecapsulationKey(Inner768Dk);

impl DecapsulationKey {
    /// Decode from the FIPS 203 byte serialization.
    pub fn from_bytes(bytes: &[u8; DECAPSULATION_KEY_SIZE]) -> Self {
        let encoded = Encoded::<Inner768Dk>::try_from(bytes.as_slice())
            .expect("DECAPSULATION_KEY_SIZE matches Encoded<Inner768Dk> array size");
        DecapsulationKey(<Inner768Dk as EncodedSizeUser>::from_bytes(&encoded))
    }

    /// Serialize to the FIPS 203 byte form, zeroized on drop.
    pub fn to_bytes(&self) -> Zeroizing<[u8; DECAPSULATION_KEY_SIZE]> {
        let encoded = self.0.as_bytes();
        let mut out = Zeroizing::new([0u8; DECAPSULATION_KEY_SIZE]);
        out.copy_from_slice(encoded.as_slice());
        out
    }
}

/// ML-KEM-768 ciphertext. Sender produces this and transmits it to
/// the recipient alongside the wrapped key blob.
#[derive(Clone)]
pub struct Ciphertext([u8; CIPHERTEXT_SIZE]);

impl Ciphertext {
    pub fn from_bytes(bytes: &[u8; CIPHERTEXT_SIZE]) -> Self {
        Ciphertext(*bytes)
    }

    pub fn to_bytes(&self) -> [u8; CIPHERTEXT_SIZE] {
        self.0
    }

    /// Convert to ml-kem's typed ciphertext (= `Array<u8, U1088>`).
    ///
    /// `Inner768Ct` is itself a typed `Array` — it does **not**
    /// implement `EncodedSizeUser` (in ml-kem 0.2 only the
    /// `EncapsulationKey<P>` and `DecapsulationKey<P>` kem types do).
    /// So byte → typed conversion goes through `Array`'s standard
    /// `TryFrom<&[u8]>` impl, not `EncodedSizeUser::from_bytes`.
    fn to_inner(&self) -> Inner768Ct {
        Inner768Ct::try_from(self.0.as_slice())
            .expect("CIPHERTEXT_SIZE matches Inner768Ct array size")
    }
}

/// ML-KEM-768 shared secret. 32 bytes. Zeroizes on drop.
///
/// **Always feed this through HKDF** before using as key material.
#[derive(Clone, ZeroizeOnDrop)]
pub struct SharedSecret([u8; SHARED_SECRET_SIZE]);

impl SharedSecret {
    pub fn as_bytes(&self) -> &[u8; SHARED_SECRET_SIZE] {
        &self.0
    }
}

/// Generate a fresh ML-KEM-768 keypair from `OsRng`.
pub fn generate_keypair() -> (DecapsulationKey, EncapsulationKey) {
    generate_keypair_with_rng(&mut OsRng)
}

/// Generate a fresh ML-KEM-768 keypair using a caller-supplied CSPRNG.
///
/// Useful for known-answer testing: drive a deterministic RNG seeded
/// with the FIPS 203 keygen seed `(d || z)`.
///
/// The bound is `R: RngCore + CryptoRng` (no `?Sized`) because
/// ml-kem's `KemCore::generate` requires `Sized`.
pub fn generate_keypair_with_rng<R>(rng: &mut R) -> (DecapsulationKey, EncapsulationKey)
where
    R: RngCore + CryptoRng,
{
    let (dk, ek) = MlKem768::generate(rng);
    (DecapsulationKey(dk), EncapsulationKey(ek))
}

/// Sender side: compute `(ct, ss)` for the recipient's encapsulation key.
pub fn encapsulate(public: &EncapsulationKey) -> Result<(Ciphertext, SharedSecret)> {
    encapsulate_with_rng(public, &mut OsRng)
}

/// Sender side with caller-supplied CSPRNG. Useful for KAT testing.
pub fn encapsulate_with_rng<R>(
    public: &EncapsulationKey,
    rng: &mut R,
) -> Result<(Ciphertext, SharedSecret)>
where
    R: RngCore + CryptoRng,
{
    let (ct_inner, mut ss_array) = public
        .0
        .encapsulate(rng)
        .map_err(|e| Error::Internal(format!("ML-KEM-768 encapsulate: {e:?}")))?;

    // ct_inner is Inner768Ct (= Array<u8, U1088>). Use Array's
    // `as_slice` directly — Ciphertext does NOT implement
    // EncodedSizeUser in ml-kem 0.2.
    let mut ct_bytes = [0u8; CIPHERTEXT_SIZE];
    ct_bytes.copy_from_slice(ct_inner.as_slice());

    // ss_array is the kem shared key (= Array<u8, U32>).
    let mut ss_bytes = [0u8; SHARED_SECRET_SIZE];
    ss_bytes.copy_from_slice(ss_array.as_slice());
    ss_array.zeroize();

    Ok((Ciphertext(ct_bytes), SharedSecret(ss_bytes)))
}

/// Recipient side: recover `ss` using own decapsulation key + received `ct`.
///
/// **Implicit rejection**: per FIPS 203 §6.3, this never errors on a
/// wrong-key or tampered-ciphertext input — it returns a deterministic
/// but unrelated 32-byte value. The protocol must confirm the
/// recipient was the intended one downstream (e.g. via an AEAD tag).
pub fn decapsulate(secret: &DecapsulationKey, ct: &Ciphertext) -> Result<SharedSecret> {
    let inner_ct = ct.to_inner();
    let mut ss_array = secret
        .0
        .decapsulate(&inner_ct)
        .map_err(|e| Error::Internal(format!("ML-KEM-768 decapsulate: {e:?}")))?;
    let mut ss_bytes = [0u8; SHARED_SECRET_SIZE];
    ss_bytes.copy_from_slice(ss_array.as_slice());
    ss_array.zeroize();
    Ok(SharedSecret(ss_bytes))
}

/// Domain-separated PQXDH HKDF combiner stub.
///
/// Per [`docs/design/pqxdh-double-ratchet.md`](../../docs/design/pqxdh-double-ratchet.md)
/// "Layer 1":
///
/// ```text
/// SK = HKDF-SHA256(
///     salt = zeros,
///     ikm  = DH1 || DH2 || DH3 || DH4 || SS_pq,
///     info = "discord-privacy-client/pqxdh/v1"
/// )
/// ```
///
/// This stub takes the concatenated X25519 DH outputs (`dh_concat`)
/// and the ML-KEM-768 shared secret (`ss_pq`) and produces the 32-
/// byte `SK`. The caller is responsible for concatenating DH outputs
/// in the order specified by the spec — including the no-OPK
/// fallback case (DH4 omitted). The full PQXDH handshake construction
/// (with transcript framing and AD encoding) lands in a subsequent
/// commit; this stub exercises the combiner end-to-end so the hybrid
/// property can be sanity-checked while surrounding machinery is
/// built.
pub fn pqxdh_combine_stub(dh_concat: &[u8], ss_pq: &SharedSecret) -> Result<[u8; 32]> {
    let mut ikm = Vec::with_capacity(dh_concat.len() + SHARED_SECRET_SIZE);
    ikm.extend_from_slice(dh_concat);
    ikm.extend_from_slice(ss_pq.as_bytes());
    let out = crate::hkdf::derive_32(&[], &ikm, b"discord-privacy-client/pqxdh/v1")?;
    ikm.zeroize();
    Ok(out)
}
