//! Cross-version sender attribution proof.
//!
//! The receive paths have different ways to learn the sender key:
//! v1/v2 resolve it out of band from the claimed sender id, while
//! v3/v4/v5 carry an authenticated sender identity key in the wire
//! or in ratchet-associated data. The common invariant is the same:
//! a message may be attributed to a claimed sender only when the
//! authenticated sender identity key equals the local pin for that
//! claimed sender.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use crypto::x25519;
use std::fmt;

/// Version-independent witness that a sender label was not relabelled.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SenderAttributionProof {
    sender_ik_pub: x25519::PublicKey,
}

impl SenderAttributionProof {
    /// The authenticated X25519 identity key covered by this proof.
    pub fn sender_ik_pub(&self) -> &x25519::PublicKey {
        &self.sender_ik_pub
    }
}

impl fmt::Debug for SenderAttributionProof {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SenderAttributionProof")
            .field("sender_ik_pub", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SenderAttributionProofError {
    MissingLocalPin,
    MalformedLocalPin,
    RelabelRejected,
}

impl fmt::Display for SenderAttributionProofError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            Self::MissingLocalPin => "sender attribution refused: missing local pin",
            Self::MalformedLocalPin => "sender attribution refused: malformed local pin",
            Self::RelabelRejected => {
                "sender attribution refused: authenticated key does not match local pin"
            }
        };
        f.write_str(msg)
    }
}

impl std::error::Error for SenderAttributionProofError {}

/// Prove attribution when the caller already resolved a local pin.
pub fn prove_resolved_sender_key(
    pinned_sender_ik_pub: &x25519::PublicKey,
    authenticated_sender_ik_pub: &x25519::PublicKey,
) -> Result<SenderAttributionProof, SenderAttributionProofError> {
    if pinned_sender_ik_pub != authenticated_sender_ik_pub {
        return Err(SenderAttributionProofError::RelabelRejected);
    }
    Ok(SenderAttributionProof {
        sender_ik_pub: *authenticated_sender_ik_pub,
    })
}

/// Prove attribution from the base64 key shape stored in `peer_map`.
pub fn prove_base64_pinned_sender_key(
    pinned_sender_ik_pub_b64: Option<&str>,
    authenticated_sender_ik_pub: &x25519::PublicKey,
) -> Result<SenderAttributionProof, SenderAttributionProofError> {
    let pinned = decode_pinned_sender_key(pinned_sender_ik_pub_b64)?;
    prove_resolved_sender_key(&pinned, authenticated_sender_ik_pub)
}

fn decode_pinned_sender_key(
    pinned_sender_ik_pub_b64: Option<&str>,
) -> Result<x25519::PublicKey, SenderAttributionProofError> {
    let encoded = pinned_sender_ik_pub_b64.ok_or(SenderAttributionProofError::MissingLocalPin)?;
    let decoded = STANDARD
        .decode(encoded)
        .map_err(|_| SenderAttributionProofError::MalformedLocalPin)?;
    if decoded.len() != x25519::PUBLIC_KEY_SIZE {
        return Err(SenderAttributionProofError::MalformedLocalPin);
    }
    let mut bytes = [0u8; x25519::PUBLIC_KEY_SIZE];
    bytes.copy_from_slice(&decoded);
    Ok(x25519::PublicKey::from_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sender_attribution_proof_prevents_cross_version_relabel() {
        let (_alice_sk, alice_pub) = x25519::generate_keypair();
        let (_mallory_sk, mallory_pub) = x25519::generate_keypair();
        let pinned_alice = STANDARD.encode(alice_pub.as_bytes());

        for version in ["v1", "v2", "v3", "v4", "v5"] {
            let proof = prove_base64_pinned_sender_key(Some(&pinned_alice), &alice_pub)
                .unwrap_or_else(|e| {
                    panic!("{version} honest attribution unexpectedly failed: {e}")
                });
            assert_eq!(proof.sender_ik_pub(), &alice_pub);

            let err =
                prove_base64_pinned_sender_key(Some(&pinned_alice), &mallory_pub).unwrap_err();
            assert_eq!(
                err,
                SenderAttributionProofError::RelabelRejected,
                "{version} must reject relabelling Mallory's authenticated key as Alice"
            );
        }

        assert_eq!(
            prove_base64_pinned_sender_key(None, &alice_pub).unwrap_err(),
            SenderAttributionProofError::MissingLocalPin
        );
        assert_eq!(
            prove_base64_pinned_sender_key(Some("not-base64"), &alice_pub).unwrap_err(),
            SenderAttributionProofError::MalformedLocalPin
        );
    }
}
