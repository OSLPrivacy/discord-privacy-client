//! PQXDH-shaped asynchronous handshake.
//!
//! Deliberately the *same shape* as the PQXDH already in
//! `crates/crypto/src/pqxdh.rs` and used by `wire_v2::encrypt_v3`, so
//! that swapping this crate in later does not also mean swapping the
//! prekey-publication and identity-management machinery.
//!
//! ```text
//! DH1 = DH(IK_A, SPK_B)
//! DH2 = DH(EK_A, IK_B)
//! DH3 = DH(EK_A, SPK_B)
//! DH4 = DH(EK_A, OPK_B)        (omitted when no one-time prekey)
//! (ct, ss) = ML-KEM-768.Encaps(PQSPK_B)
//! SK  = HKDF(salt = 0, ikm = DH1||DH2||DH3||DH4||ss, info = LABEL_HANDSHAKE)
//! ```
//!
//! ## What is and is not in scope here
//!
//! **In scope:** deriving `SK`, the initial header keys, and epoch 0's
//! PQ secret; encoding the bootstrap preamble.
//!
//! **Not in scope:** verifying the signature over `SPK_B` / `PQSPK_B`.
//! The bundle arrives already-authenticated from the caller's trust
//! layer (TOFU + safety numbers, in this product). This crate takes a
//! [`PeerBundle`] as ground truth. That is exactly the boundary
//! `crates/crypto/src/pqxdh.rs` draws, and it is called out in
//! `THREAT-MODEL.md` because it is load-bearing: **an unauthenticated
//! bundle means an active MITM, and nothing in this crate detects it.**
//!
//! ## Deniability
//!
//! No party signs the transcript, the message, or anything derived
//! from them. `SK` is a function of DH outputs and a KEM shared secret
//! that either party could have produced with knowledge of the other's
//! secrets. Offline deniability is therefore preserved at exactly the
//! level X3DH/PQXDH provides — no better, no worse. See the claims
//! table in `DESIGN.md`.

use crate::codec::{Reader, Writer};
use crate::error::{Error, Result};
use crate::kdf;
use crate::primitives::{
    hkdf, kem_decapsulate, kem_encapsulate, KemPublic, KemSecret, Secret32, XPublic, XSecret,
    AEAD_KEY, MLKEM_CT,
};
use rand_core::{CryptoRng, RngCore};
use zeroize::Zeroize;

/// A peer's published prekey bundle. Assumed already authenticated by
/// the caller's trust layer.
#[derive(Clone, Debug)]
pub struct PeerBundle {
    /// Long-term X25519 identity key.
    pub identity: XPublic,
    /// Signed X25519 prekey. Doubles as the peer's initial ratchet key.
    pub signed_prekey: XPublic,
    /// Optional one-time X25519 prekey and its id.
    pub one_time_prekey: Option<(u32, XPublic)>,
    /// ML-KEM-768 signed prekey.
    pub pq_prekey: KemPublic,
}

/// The local side's secrets corresponding to a published bundle.
#[derive(Clone)]
pub struct LocalPrekeys {
    pub identity: XSecret,
    pub signed_prekey: XSecret,
    /// One-time prekeys by id. A real deployment deletes each entry on
    /// first use; this crate does not manage that lifecycle.
    pub one_time_prekeys: Vec<(u32, XSecret)>,
    pub pq_prekey: KemSecret,
}

impl LocalPrekeys {
    fn opk(&self, id: u32) -> Option<&XSecret> {
        self.one_time_prekeys
            .iter()
            .find(|(i, _)| *i == id)
            .map(|(_, s)| s)
    }
}

/// The bootstrap preamble carried in the clear on the initiator's
/// pre-response messages, exactly as Signal's PreKeyMessage is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preamble {
    pub initiator_identity: XPublic,
    pub initiator_ephemeral: XPublic,
    /// `0` means "no one-time prekey was used".
    pub one_time_prekey_id: u32,
    pub kem_ciphertext: [u8; MLKEM_CT],
}

/// Wire size of the bootstrap preamble: 32 + 32 + varint + 1088.
pub const PREAMBLE_MIN_BYTES: usize = 32 + 32 + 1 + MLKEM_CT;

impl Preamble {
    pub fn encode(&self, w: &mut Writer) {
        w.bytes(self.initiator_identity.as_bytes());
        w.bytes(self.initiator_ephemeral.as_bytes());
        w.varint(self.one_time_prekey_id);
        w.bytes(&self.kem_ciphertext);
    }

    pub fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let initiator_identity = XPublic::from_bytes(r.array::<32>()?);
        let initiator_ephemeral = XPublic::from_bytes(r.array::<32>()?);
        let one_time_prekey_id = r.varint()?;
        let kem_ciphertext = r.array::<MLKEM_CT>()?;
        Ok(Preamble {
            initiator_identity,
            initiator_ephemeral,
            one_time_prekey_id,
            kem_ciphertext,
        })
    }
}

/// Root secret plus the two shared header keys the header-encrypted
/// ratchet needs to bootstrap.
pub struct HandshakeOutput {
    pub root: Secret32,
    /// Header key for the initiator's first sending chain.
    pub initiator_header_key: [u8; AEAD_KEY],
    /// Header key for the responder's first sending chain.
    pub responder_header_key: [u8; AEAD_KEY],
    /// Epoch 0's PQ secret.
    pub pq_epoch0: Secret32,
    /// Stable session identifier. Not secret; useful for storage keys
    /// and for correlating both sides in tests and logs.
    pub session_id: [u8; 16],
}

fn expand(sk: &[u8; 32]) -> Result<HandshakeOutput> {
    let root = hkdf::<32>(&[], sk, kdf::LABEL_HANDSHAKE)?;
    let hks = hkdf::<64>(&[], sk, kdf::LABEL_HEADER_INIT)?;
    let sid = hkdf::<16>(&[], sk, kdf::LABEL_SESSION_ID)?;
    let mut a = [0u8; 32];
    let mut b = [0u8; 32];
    a.copy_from_slice(&hks[0..32]);
    b.copy_from_slice(&hks[32..64]);
    Ok(HandshakeOutput {
        root: Secret32::from_bytes(root),
        initiator_header_key: a,
        responder_header_key: b,
        // Epoch 0's PQ secret is `SK` itself domain-separated: the KEM
        // shared secret is already inside `SK`, and re-deriving here
        // keeps the epoch-secret map uniform.
        pq_epoch0: Secret32::from_bytes(hkdf::<32>(&[], sk, b"OSL-RN/v1/pq-epoch-0")?),
        session_id: sid,
    })
}

/// HKDF `info` for the `SK` derivation.
///
/// `None` reproduces the original, unbound derivation byte-for-byte
/// (`info == LABEL_HANDSHAKE`), which is why adding negotiation binding
/// did not change any frozen test vector. `Some(d)` appends the 32-byte
/// negotiation digest from [`crate::negotiate`]. Both operands are
/// fixed-length, so the concatenation is unambiguous.
///
/// Application code must always pass `Some`; see
/// [`initiate_bound`] for why the unbound form still exists.
fn handshake_info(binding: Option<&[u8; 32]>) -> Vec<u8> {
    let mut info = Vec::with_capacity(kdf::LABEL_HANDSHAKE.len() + 32);
    info.extend_from_slice(kdf::LABEL_HANDSHAKE);
    if let Some(d) = binding {
        info.extend_from_slice(d);
    }
    info
}

/// Initiator side. Returns the handshake output and the preamble to
/// attach to pre-response messages.
///
/// Equivalent to [`initiate_bound`] with no negotiation binding.
/// **Kept for the crate's own test vectors and for protocol-only
/// experimentation.** Application code must use [`initiate_bound`]: an
/// unbound session has no cryptographic record of which wire version
/// was negotiated, which is exactly the property `negotiate` exists to
/// provide.
pub fn initiate<R: RngCore + CryptoRng>(
    local_identity: &XSecret,
    peer: &PeerBundle,
    rng: &mut R,
) -> Result<(HandshakeOutput, Preamble, XPublic)> {
    initiate_bound(local_identity, peer, None, rng)
}

/// Initiator side, with the negotiated version bound into `SK`.
///
/// `binding` is [`crate::negotiate::Negotiation::digest`]. The
/// responder must arrive at the identical digest via
/// [`respond_bound`]; if it does not — because an attacker altered the
/// peer record the initiator consumed, or because the two sides
/// disagree about the version floor — the derived `SK` differs and the
/// first message fails to authenticate. That is the intended behaviour:
/// **fail closed, never fall back.**
pub fn initiate_bound<R: RngCore + CryptoRng>(
    local_identity: &XSecret,
    peer: &PeerBundle,
    binding: Option<&[u8; 32]>,
    rng: &mut R,
) -> Result<(HandshakeOutput, Preamble, XPublic)> {
    let (ek_sec, ek_pub) = crate::primitives::x25519_keypair(rng);

    let dh1 = crate::primitives::dh(local_identity, &peer.signed_prekey)?;
    let dh2 = crate::primitives::dh(&ek_sec, &peer.identity)?;
    let dh3 = crate::primitives::dh(&ek_sec, &peer.signed_prekey)?;
    let dh4 = match &peer.one_time_prekey {
        Some((_, opk)) => Some(crate::primitives::dh(&ek_sec, opk)?),
        None => None,
    };
    let (ct, ss) = kem_encapsulate(&peer.pq_prekey, rng)?;

    let mut ikm = Vec::with_capacity(160);
    ikm.extend_from_slice(dh1.as_bytes());
    ikm.extend_from_slice(dh2.as_bytes());
    ikm.extend_from_slice(dh3.as_bytes());
    if let Some(d) = &dh4 {
        ikm.extend_from_slice(d.as_bytes());
    }
    ikm.extend_from_slice(ss.as_bytes());
    let sk = hkdf::<32>(&[], &ikm, &handshake_info(binding));
    ikm.zeroize();
    let mut sk = sk?;

    let out = expand(&sk);
    sk.zeroize();

    let preamble = Preamble {
        initiator_identity: local_identity.public(),
        initiator_ephemeral: ek_pub,
        one_time_prekey_id: peer.one_time_prekey.map(|(id, _)| id).unwrap_or(0),
        kem_ciphertext: ct,
    };
    Ok((out?, preamble, peer.signed_prekey))
}

/// Responder side. Recomputes the same `SK` from the preamble.
///
/// Equivalent to [`respond_bound`] with no negotiation binding. See
/// [`initiate`] for why application code must not use this form.
pub fn respond(local: &LocalPrekeys, preamble: &Preamble) -> Result<HandshakeOutput> {
    respond_bound(local, preamble, None)
}

/// Responder side, with the negotiated version bound into `SK`.
///
/// Note the OPK convention: id `0` means "none". A deployment that
/// wants a one-time prekey with id 0 must renumber; this is called out
/// rather than silently handled.
pub fn respond_bound(
    local: &LocalPrekeys,
    preamble: &Preamble,
    binding: Option<&[u8; 32]>,
) -> Result<HandshakeOutput> {
    let dh1 = crate::primitives::dh(&local.signed_prekey, &preamble.initiator_identity)?;
    let dh2 = crate::primitives::dh(&local.identity, &preamble.initiator_ephemeral)?;
    let dh3 = crate::primitives::dh(&local.signed_prekey, &preamble.initiator_ephemeral)?;
    let dh4 = if preamble.one_time_prekey_id != 0 {
        let opk = local
            .opk(preamble.one_time_prekey_id)
            .ok_or(Error::Malformed("unknown one-time prekey id"))?;
        Some(crate::primitives::dh(opk, &preamble.initiator_ephemeral)?)
    } else {
        None
    };
    let ss = kem_decapsulate(&local.pq_prekey, &preamble.kem_ciphertext)?;

    let mut ikm = Vec::with_capacity(160);
    ikm.extend_from_slice(dh1.as_bytes());
    ikm.extend_from_slice(dh2.as_bytes());
    ikm.extend_from_slice(dh3.as_bytes());
    if let Some(d) = &dh4 {
        ikm.extend_from_slice(d.as_bytes());
    }
    ikm.extend_from_slice(ss.as_bytes());
    let sk = hkdf::<32>(&[], &ikm, &handshake_info(binding));
    ikm.zeroize();
    let mut sk = sk?;
    let out = expand(&sk);
    sk.zeroize();
    out
}

// The crate denies clippy::indexing_slicing (lib.rs:49) because a panic on
// attacker-controlled input in a ratchet is a DoS. That policy is for
// PRODUCTION code. These tests index deliberately and provably in bounds
// (truncation loops over 0..bytes.len(), a bit-flip on a non-empty
// ciphertext); rewriting them with get() would obscure what they prove.
#[allow(clippy::indexing_slicing)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::{kem_keypair, x25519_keypair};
    use rand::SeedableRng;
    use rand_chacha::ChaCha20Rng;

    fn bundle(rng: &mut ChaCha20Rng, with_opk: bool) -> (PeerBundle, LocalPrekeys) {
        let (ik_s, ik_p) = x25519_keypair(rng);
        let (spk_s, spk_p) = x25519_keypair(rng);
        let (opk_s, opk_p) = x25519_keypair(rng);
        let (pq_s, pq_p) = kem_keypair(rng);
        (
            PeerBundle {
                identity: ik_p,
                signed_prekey: spk_p,
                one_time_prekey: if with_opk { Some((7, opk_p)) } else { None },
                pq_prekey: pq_p,
            },
            LocalPrekeys {
                identity: ik_s,
                signed_prekey: spk_s,
                one_time_prekeys: vec![(7, opk_s)],
                pq_prekey: pq_s,
            },
        )
    }

    #[test]
    fn both_sides_agree_with_opk() {
        let mut rng = ChaCha20Rng::seed_from_u64(11);
        let (bob_bundle, bob_local) = bundle(&mut rng, true);
        let (alice_ik, _) = x25519_keypair(&mut rng);
        let (a, preamble, _) = initiate(&alice_ik, &bob_bundle, &mut rng).expect("initiate");
        let b = respond(&bob_local, &preamble).expect("respond");
        assert_eq!(a.root, b.root);
        assert_eq!(a.initiator_header_key, b.initiator_header_key);
        assert_eq!(a.responder_header_key, b.responder_header_key);
        assert_eq!(a.pq_epoch0, b.pq_epoch0);
        assert_eq!(a.session_id, b.session_id);
    }

    #[test]
    fn both_sides_agree_without_opk() {
        let mut rng = ChaCha20Rng::seed_from_u64(12);
        let (bob_bundle, bob_local) = bundle(&mut rng, false);
        let (alice_ik, _) = x25519_keypair(&mut rng);
        let (a, preamble, _) = initiate(&alice_ik, &bob_bundle, &mut rng).expect("initiate");
        assert_eq!(preamble.one_time_prekey_id, 0);
        let b = respond(&bob_local, &preamble).expect("respond");
        assert_eq!(a.root, b.root);
    }

    #[test]
    fn opk_and_no_opk_produce_different_roots() {
        let mut rng = ChaCha20Rng::seed_from_u64(13);
        let (mut bundle_with, local) = bundle(&mut rng, true);
        let (alice_ik, _) = x25519_keypair(&mut rng);
        let (a, _, _) = initiate(&alice_ik, &bundle_with, &mut rng).expect("initiate");
        bundle_with.one_time_prekey = None;
        let (b, _, _) = initiate(&alice_ik, &bundle_with, &mut rng).expect("initiate");
        assert_ne!(a.root, b.root);
        let _ = local;
    }

    #[test]
    fn unknown_opk_id_is_rejected_not_panicked() {
        let mut rng = ChaCha20Rng::seed_from_u64(14);
        let (bob_bundle, bob_local) = bundle(&mut rng, true);
        let (alice_ik, _) = x25519_keypair(&mut rng);
        let (_, mut preamble, _) = initiate(&alice_ik, &bob_bundle, &mut rng).expect("initiate");
        preamble.one_time_prekey_id = 999;
        assert!(matches!(
            respond(&bob_local, &preamble),
            Err(Error::Malformed("unknown one-time prekey id"))
        ));
    }

    #[test]
    fn tampered_kem_ciphertext_yields_a_different_root() {
        // ML-KEM implicit rejection means this must NOT error; the
        // divergence has to surface downstream as an AEAD failure.
        let mut rng = ChaCha20Rng::seed_from_u64(15);
        let (bob_bundle, bob_local) = bundle(&mut rng, true);
        let (alice_ik, _) = x25519_keypair(&mut rng);
        let (a, mut preamble, _) = initiate(&alice_ik, &bob_bundle, &mut rng).expect("initiate");
        preamble.kem_ciphertext[0] ^= 0x01;
        let b = respond(&bob_local, &preamble).expect("implicit rejection, no error");
        assert_ne!(a.root, b.root);
    }

    #[test]
    fn preamble_roundtrips_and_rejects_truncation() {
        let mut rng = ChaCha20Rng::seed_from_u64(16);
        let (bob_bundle, _) = bundle(&mut rng, true);
        let (alice_ik, _) = x25519_keypair(&mut rng);
        let (_, preamble, _) = initiate(&alice_ik, &bob_bundle, &mut rng).expect("initiate");
        let mut w = Writer::default();
        preamble.encode(&mut w);
        let bytes = w.into_vec();
        assert!(bytes.len() >= PREAMBLE_MIN_BYTES);
        let mut r = Reader::new(&bytes);
        assert_eq!(Preamble::decode(&mut r).expect("decode"), preamble);
        r.finish().expect("exact");

        for cut in 0..bytes.len() {
            let mut r = Reader::new(&bytes[..cut]);
            assert!(Preamble::decode(&mut r).is_err(), "truncation at {cut}");
        }
    }
}
