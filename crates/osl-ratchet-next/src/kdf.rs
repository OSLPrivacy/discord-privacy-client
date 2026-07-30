//! Key schedule: labels, root KDF, chain KDF, nonce derivation.
//!
//! Every HKDF invocation in the protocol appears here, so the full key
//! schedule is auditable from one file. All labels are prefixed with
//! `OSL-RN/v1/` and are pairwise distinct; no two derivations can
//! collide even if an attacker controls the salt or IKM.
//!
//! ## Root step
//!
//! ```text
//! ikm  = dh_out || ss_pq[mixed+1] || ... || ss_pq[mix_epoch]
//! okm  = HKDF-SHA256(salt = RK, ikm, info = LABEL_ROOT || LE32(mix_epoch))
//!        -> RK' (32) || CK (32) || HK_next (32)
//! ```
//!
//! The X25519 output and every PQ epoch secret being mixed enter the
//! *same* HKDF extract. Recovering `RK'` therefore requires breaking
//! X25519 **and** ML-KEM-768 for every mixed epoch — this is the
//! hybrid property. Concatenation-into-a-single-extract is the
//! combiner NIST SP 800-56C Rev.2 and PQXDH both use; it is secure
//! when HKDF-Extract is modelled as a dual-PRF, which is the standard
//! assumption PQXDH already relies on.
//!
//! `mix_epoch` is bound into `info` so that two root steps with the
//! same DH output but different PQ-mix decisions can never produce the
//! same keys.
//!
//! ## Chain step
//!
//! ```text
//! MK    = HKDF(salt = CK, ikm = LABEL_MK_IKM, info = LABEL_MK)   (32)
//! CK'   = HKDF(salt = CK, ikm = LABEL_CK_IKM, info = LABEL_CK)   (32)
//! ```
//!
//! Signal uses HMAC(CK, 0x01) / HMAC(CK, 0x02). HKDF with distinct
//! info labels is the same PRF-on-a-fixed-key construction with
//! explicit domain separation; no security difference is claimed.
//!
//! ## Body nonce
//!
//! ```text
//! body_nonce = HKDF(salt = MK, ikm = LABEL_NONCE_IKM, info = LABEL_BODY_NONCE)[0..24]
//! ```
//!
//! Each message key is used for exactly one AEAD seal, so a
//! deterministic nonce is safe and saves 24 bytes of wire per message.
//! (Nonce reuse would require message-key reuse, which the chain
//! ratchet and the skipped-key store both structurally prevent — a
//! skipped key is removed from the store the moment it is used.)

use crate::error::Result;
use crate::primitives::{hkdf, Secret32, AEAD_KEY, AEAD_NONCE};

pub const LABEL_HANDSHAKE: &[u8] = b"OSL-RN/v1/pqxdh-root";
pub const LABEL_ROOT: &[u8] = b"OSL-RN/v1/root";
pub const LABEL_MK: &[u8] = b"OSL-RN/v1/message-key";
pub const LABEL_CK: &[u8] = b"OSL-RN/v1/chain-key";
pub const LABEL_BODY_NONCE: &[u8] = b"OSL-RN/v1/body-nonce";
pub const LABEL_HEADER_INIT: &[u8] = b"OSL-RN/v1/header-key-init";
pub const LABEL_SESSION_ID: &[u8] = b"OSL-RN/v1/session-id";
pub const LABEL_STATE_KEY: &[u8] = b"OSL-RN/v1/state-export";

const IKM_MK: &[u8] = b"mk";
const IKM_CK: &[u8] = b"ck";
const IKM_NONCE: &[u8] = b"nonce";

/// Associated data prefix covering the outer framing of a OSL-RN message.
pub const AD_HEADER: &[u8] = b"OSL-RN/v1/header";
pub const AD_BODY: &[u8] = b"OSL-RN/v1/body";

/// Constant high 12 bytes of every header nonce. The low 12 bytes are
/// random and travel on the wire; see [`crate::primitives::HEADER_NONCE_WIRE`].
pub const HEADER_NONCE_PREFIX: [u8; 12] = *b"OSL-RN/hdr\x00\x01";

/// Output of a root (DH-ratchet) step.
pub struct RootStep {
    pub root_key: Secret32,
    pub chain_key: Secret32,
    /// Header key for the chain this step creates.
    pub header_key: [u8; AEAD_KEY],
}

/// Perform one root step.
///
/// `pq_secrets` are the PQ epoch secrets being folded in, in ascending
/// epoch order. Empty is legal (a classical-only step).
pub fn root_step(
    root_key: &Secret32,
    dh_out: &Secret32,
    pq_secrets: &[Secret32],
    mix_epoch: u32,
) -> Result<RootStep> {
    let mut ikm = Vec::with_capacity(32 * (1 + pq_secrets.len()));
    ikm.extend_from_slice(dh_out.as_bytes());
    for s in pq_secrets {
        ikm.extend_from_slice(s.as_bytes());
    }

    let mut info = Vec::with_capacity(LABEL_ROOT.len() + 4);
    info.extend_from_slice(LABEL_ROOT);
    info.extend_from_slice(&mix_epoch.to_le_bytes());

    let okm = hkdf::<96>(root_key.as_bytes(), &ikm, &info);
    // Zeroize the concatenated IKM regardless of the HKDF outcome.
    zeroize_vec(&mut ikm);
    let okm = okm?;

    let mut rk = [0u8; 32];
    let mut ck = [0u8; 32];
    let mut hk = [0u8; 32];
    rk.copy_from_slice(&okm[0..32]);
    ck.copy_from_slice(&okm[32..64]);
    hk.copy_from_slice(&okm[64..96]);

    Ok(RootStep {
        root_key: Secret32::from_bytes(rk),
        chain_key: Secret32::from_bytes(ck),
        header_key: hk,
    })
}

/// Advance a chain key, returning `(message_key, next_chain_key)`.
pub fn chain_step(chain_key: &Secret32) -> Result<(Secret32, Secret32)> {
    let mk = hkdf::<32>(chain_key.as_bytes(), IKM_MK, LABEL_MK)?;
    let ck = hkdf::<32>(chain_key.as_bytes(), IKM_CK, LABEL_CK)?;
    Ok((Secret32::from_bytes(mk), Secret32::from_bytes(ck)))
}

/// Deterministic body nonce for a one-shot message key.
pub fn body_nonce(message_key: &Secret32) -> Result<[u8; AEAD_NONCE]> {
    hkdf::<AEAD_NONCE>(message_key.as_bytes(), IKM_NONCE, LABEL_BODY_NONCE)
}

/// Expand a 12-byte wire nonce into the full 24-byte XChaCha nonce.
pub fn header_nonce(wire: &[u8; 12]) -> [u8; AEAD_NONCE] {
    let mut out = [0u8; AEAD_NONCE];
    out[..12].copy_from_slice(&HEADER_NONCE_PREFIX);
    out[12..].copy_from_slice(wire);
    out
}

fn zeroize_vec(v: &mut Vec<u8>) {
    use zeroize::Zeroize;
    v.zeroize();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_pairwise_distinct() {
        let labels = [
            LABEL_HANDSHAKE,
            LABEL_ROOT,
            LABEL_MK,
            LABEL_CK,
            LABEL_BODY_NONCE,
            LABEL_HEADER_INIT,
            LABEL_SESSION_ID,
            LABEL_STATE_KEY,
            AD_HEADER,
            AD_BODY,
        ];
        for (i, a) in labels.iter().enumerate() {
            for b in labels.iter().skip(i + 1) {
                assert_ne!(a, b, "duplicate domain-separation label");
            }
        }
    }

    #[test]
    fn root_step_is_deterministic_and_epoch_bound() {
        let rk = Secret32::from_bytes([1u8; 32]);
        let dh = Secret32::from_bytes([2u8; 32]);
        let a = root_step(&rk, &dh, &[], 0).expect("step");
        let b = root_step(&rk, &dh, &[], 0).expect("step");
        assert_eq!(a.root_key, b.root_key);
        assert_eq!(a.header_key, b.header_key);

        // Same DH output, different mix epoch -> different keys.
        let c = root_step(&rk, &dh, &[], 1).expect("step");
        assert_ne!(a.root_key, c.root_key);

        // Adding a PQ secret changes everything.
        let d = root_step(&rk, &dh, &[Secret32::from_bytes([3u8; 32])], 1).expect("step");
        assert_ne!(c.root_key, d.root_key);
        assert_ne!(c.chain_key, d.chain_key);
        assert_ne!(c.header_key, d.header_key);
    }

    #[test]
    fn root_step_outputs_are_independent() {
        let a = root_step(
            &Secret32::from_bytes([1u8; 32]),
            &Secret32::from_bytes([2u8; 32]),
            &[],
            0,
        )
        .expect("step");
        assert_ne!(a.root_key.as_bytes(), a.chain_key.as_bytes());
        assert_ne!(a.chain_key.as_bytes(), &a.header_key);
        assert_ne!(a.root_key.as_bytes(), &a.header_key);
    }

    #[test]
    fn chain_step_advances_and_separates() {
        let ck = Secret32::from_bytes([5u8; 32]);
        let (mk, ck2) = chain_step(&ck).expect("step");
        assert_ne!(mk, ck2);
        assert_ne!(ck, ck2);
        let (mk2, _) = chain_step(&ck2).expect("step");
        assert_ne!(mk, mk2);
    }

    #[test]
    fn body_nonce_is_key_bound() {
        let a = body_nonce(&Secret32::from_bytes([1u8; 32])).expect("nonce");
        let b = body_nonce(&Secret32::from_bytes([2u8; 32])).expect("nonce");
        assert_ne!(a, b);
    }

    #[test]
    fn header_nonce_expands_with_constant_prefix() {
        let n = header_nonce(&[9u8; 12]);
        assert_eq!(&n[..12], &HEADER_NONCE_PREFIX);
        assert_eq!(&n[12..], &[9u8; 12]);
    }
}
