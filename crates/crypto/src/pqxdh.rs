//! PQXDH handshake construction (Layer 1 of PQXDH + Double Ratchet).
//!
//! Spec: `docs/design/pqxdh-double-ratchet.md` "Layer 1: hybrid PQXDH
//! handshake".
//!
//! ## What this module implements
//!
//! Per the spec, on first contact between Alice (sender / initiator)
//! and Bob (recipient / responder):
//!
//! ```text
//! DH1 = X25519(IK_A_priv,  SPK_B_pub)
//! DH2 = X25519(EK_A_priv,  IK_B_pub)
//! DH3 = X25519(EK_A_priv,  SPK_B_pub)
//! DH4 = X25519(EK_A_priv,  OPK_B_pub)              // if available
//!
//! (SS_pq, ct_pq) = ML-KEM-768.Encaps(MLKEM_B_pub)
//!
//! SK = HKDF-SHA256(
//!     salt = zeros,
//!     ikm  = DH1 || DH2 || DH3 || [DH4] || SS_pq,
//!     info = "discord-privacy-client/pqxdh/v1"
//! )
//! ```
//!
//! - [`initiate`] / [`initiate_with_rng`]: Alice's side. Generates an
//!   ephemeral X25519 keypair, computes the four DHs (DH4 conditional
//!   on OPK availability), encapsulates to Bob's ML-KEM-768 public
//!   key, derives `SK`, returns `(SessionKey, InitiatorHandshake)`.
//! - [`respond`]: Bob's side. Recomputes the four DHs from Bob's own
//!   secrets and Alice's handshake material, decapsulates the
//!   ML-KEM-768 ciphertext, derives the same `SK`. Validates that
//!   the handshake's `no_opk` flag is consistent with the OPK secret
//!   the caller provides.
//! - [`derive_sk`]: the bare combiner — useful for tests that craft
//!   shared secrets directly. Calls into
//!   [`crate::ml_kem_768::pqxdh_combine_stub`] internally.
//!
//! ## OPK fallback (no-OPK handshake)
//!
//! Per the spec's "OPK exhaustion fallback" subsection: if Bob's
//! one-time prekey pool is empty, the handshake transcript collapses
//! to `DH1 || DH2 || DH3 || SS_pq` (no DH4) and the handshake header
//! carries an explicit `no_opk = true` flag. The recipient logs this
//! for transparency; the sender's UI surfaces "first message has
//! reduced forward secrecy until recipient comes online."
//!
//! ## Hybrid security
//!
//! `SK` remains secure if **either** the X25519 DLP assumption **or**
//! the ML-KEM-768 lattice assumption holds. Both must break for an
//! attacker to recover `SK` from the on-the-wire material.
//!
//! ## What this module does NOT yet implement
//!
//! - Handshake header serialization to wire bytes (canonical encoding).
//! - Associated-data binding (per the design doc's
//!   "Associated data" subsection).
//! - Initial AEAD payload encrypted under a key derived from `SK`.
//! - Symmetric initialisation of the Double Ratchet from `SK`.
//!
//! Those land in subsequent commits.

use crate::error::{Error, Result};
use crate::ml_kem_768;
use crate::x25519;
use rand::rngs::OsRng;
use rand_core::{CryptoRng, RngCore};
use zeroize::{Zeroize, ZeroizeOnDrop};

pub const SESSION_KEY_SIZE: usize = 32;

/// Domain-separation label fed to HKDF as the `info` field. Defined
/// in `docs/design/pqxdh-double-ratchet.md` "Layer 1".
pub const PQXDH_INFO: &[u8] = b"discord-privacy-client/pqxdh/v1";

/// Output of a successful PQXDH handshake. Zeroizes on drop.
#[derive(Clone, ZeroizeOnDrop)]
pub struct SessionKey([u8; SESSION_KEY_SIZE]);

impl SessionKey {
    pub fn as_bytes(&self) -> &[u8; SESSION_KEY_SIZE] {
        &self.0
    }
}

/// Public handshake metadata that the initiator (Alice) sends to the
/// responder (Bob). This is the on-the-wire material — combined with
/// the responder's private state and the responder's known view of
/// the initiator's identity public key, it lets the responder derive
/// the same `SessionKey`.
///
/// Wire-format serialization (canonical encoding) is **not** yet
/// implemented; callers can extract the public fields and frame them
/// per the eventual protocol wire-format companion doc.
#[derive(Clone)]
pub struct InitiatorHandshake {
    /// Alice's ephemeral X25519 public key.
    pub ek_x25519_pub: x25519::PublicKey,
    /// ML-KEM-768 ciphertext encapsulated to Bob's MLKEM public key.
    pub mlkem_ciphertext: ml_kem_768::Ciphertext,
    /// True iff Bob's OPK pool was empty at handshake time, so DH4
    /// is omitted from the transcript.
    pub no_opk: bool,
    /// Identifier of the OPK Alice consumed. `None` iff
    /// `no_opk = true`. Format is opaque to this module; the prekey
    /// infrastructure decides the encoding (e.g. a `u32` from the
    /// OPK pool index).
    pub opk_id: Option<u32>,
}

/// PQXDH HKDF combiner.
///
/// Concatenates `DH1 || DH2 || DH3 || [DH4]` (DH4 omitted if `None`),
/// appends `SS_pq`, and runs `HKDF-SHA256` with zero salt and the
/// `PQXDH_INFO` info label to produce a 32-byte session key. This is
/// the `SK` from the design doc.
pub fn derive_sk(
    dh1: &x25519::SharedSecret,
    dh2: &x25519::SharedSecret,
    dh3: &x25519::SharedSecret,
    dh4: Option<&x25519::SharedSecret>,
    ss_pq: &ml_kem_768::SharedSecret,
) -> Result<SessionKey> {
    let mut dh_concat = Vec::with_capacity(4 * 32);
    dh_concat.extend_from_slice(dh1.as_bytes());
    dh_concat.extend_from_slice(dh2.as_bytes());
    dh_concat.extend_from_slice(dh3.as_bytes());
    if let Some(d4) = dh4 {
        dh_concat.extend_from_slice(d4.as_bytes());
    }
    let sk_bytes = ml_kem_768::pqxdh_combine_stub(&dh_concat, ss_pq)?;
    dh_concat.zeroize();
    Ok(SessionKey(sk_bytes))
}

/// Initiator (Alice) side: compute the session key and produce the
/// public handshake material to send to Bob.
///
/// See module-level docs for the inputs/outputs and OPK semantics.
pub fn initiate(
    sender_ik_secret: &x25519::SecretKey,
    recipient_ik_pub: &x25519::PublicKey,
    recipient_spk_pub: &x25519::PublicKey,
    recipient_opk: Option<(u32, &x25519::PublicKey)>,
    recipient_mlkem_pub: &ml_kem_768::EncapsulationKey,
) -> Result<(SessionKey, InitiatorHandshake)> {
    initiate_with_rng(
        &mut OsRng,
        sender_ik_secret,
        recipient_ik_pub,
        recipient_spk_pub,
        recipient_opk,
        recipient_mlkem_pub,
    )
}

/// Initiator side with caller-supplied CSPRNG.
///
/// Useful for deterministic testing: drive a fixed-bytes RNG to
/// reproduce a specific ephemeral X25519 keypair and ML-KEM
/// encapsulation. Consumes 32 bytes for the X25519 ephemeral keygen
/// and 32 bytes for the ML-KEM-768 encapsulation = 64 bytes total
/// from the RNG.
pub fn initiate_with_rng<R>(
    rng: &mut R,
    sender_ik_secret: &x25519::SecretKey,
    recipient_ik_pub: &x25519::PublicKey,
    recipient_spk_pub: &x25519::PublicKey,
    recipient_opk: Option<(u32, &x25519::PublicKey)>,
    recipient_mlkem_pub: &ml_kem_768::EncapsulationKey,
) -> Result<(SessionKey, InitiatorHandshake)>
where
    R: RngCore + CryptoRng,
{
    // 1. Ephemeral X25519 keypair.
    let (ek_secret, ek_pub) = x25519::generate_keypair_with_rng(&mut *rng);

    // 2. The four DHs from Alice's perspective.
    let dh1 = x25519::diffie_hellman(sender_ik_secret, recipient_spk_pub)?;
    let dh2 = x25519::diffie_hellman(&ek_secret, recipient_ik_pub)?;
    let dh3 = x25519::diffie_hellman(&ek_secret, recipient_spk_pub)?;
    let dh4 = match recipient_opk {
        Some((_, opk_pub)) => Some(x25519::diffie_hellman(&ek_secret, opk_pub)?),
        None => None,
    };

    // 3. ML-KEM-768 encapsulation to Bob's public key.
    let (mlkem_ct, ss_pq) = ml_kem_768::encapsulate_with_rng(recipient_mlkem_pub, &mut *rng)?;

    // 4. HKDF combine.
    let sk = derive_sk(&dh1, &dh2, &dh3, dh4.as_ref(), &ss_pq)?;

    let handshake = InitiatorHandshake {
        ek_x25519_pub: ek_pub,
        mlkem_ciphertext: mlkem_ct,
        no_opk: dh4.is_none(),
        opk_id: recipient_opk.map(|(id, _)| id),
    };

    Ok((sk, handshake))
}

/// Responder (Bob) side: recompute the four DHs from Bob's secrets +
/// Alice's handshake material, decapsulate the ML-KEM-768 ciphertext,
/// derive the same session key.
///
/// Validates that the handshake's `no_opk` flag is consistent with
/// the `opk_id` it carries and the `recipient_opk_secret` the caller
/// supplies. Returns `Error::Internal` on mismatch.
pub fn respond(
    recipient_ik_secret: &x25519::SecretKey,
    recipient_spk_secret: &x25519::SecretKey,
    recipient_opk_secret: Option<&x25519::SecretKey>,
    recipient_mlkem_secret: &ml_kem_768::DecapsulationKey,
    sender_ik_pub: &x25519::PublicKey,
    handshake: &InitiatorHandshake,
) -> Result<SessionKey> {
    // OPK-flag consistency check.
    if handshake.no_opk {
        if handshake.opk_id.is_some() || recipient_opk_secret.is_some() {
            return Err(Error::Internal(
                "PQXDH respond: handshake.no_opk is true but opk_id or opk_secret was supplied"
                    .into(),
            ));
        }
    } else if handshake.opk_id.is_none() || recipient_opk_secret.is_none() {
        return Err(Error::Internal(
            "PQXDH respond: handshake.no_opk is false but opk_id or opk_secret is missing"
                .into(),
        ));
    }

    // The four DHs, from Bob's perspective.
    // X25519 symmetry: DH(a_priv, B_pub) == DH(b_priv, A_pub).
    let dh1 = x25519::diffie_hellman(recipient_spk_secret, sender_ik_pub)?;
    let dh2 = x25519::diffie_hellman(recipient_ik_secret, &handshake.ek_x25519_pub)?;
    let dh3 = x25519::diffie_hellman(recipient_spk_secret, &handshake.ek_x25519_pub)?;
    let dh4 = match recipient_opk_secret {
        Some(opk_secret) => Some(x25519::diffie_hellman(opk_secret, &handshake.ek_x25519_pub)?),
        None => None,
    };

    // ML-KEM-768 decapsulation.
    let ss_pq = ml_kem_768::decapsulate(recipient_mlkem_secret, &handshake.mlkem_ciphertext)?;

    // HKDF combine — identical concat order to Alice.
    derive_sk(&dh1, &dh2, &dh3, dh4.as_ref(), &ss_pq)
}
