//! Cross-version sender attribution proof.
//!
//! A row attributed to a sender is not just "some bundle verified once" or
//! "some claimed sender id". The proof below keeps two independent refusal
//! points:
//!
//! - a verified identity bundle is committed to the exact wire version being
//!   decoded, so a legacy message cannot be relabelled as RN or the reverse;
//! - an authenticated X25519 sender identity key must equal the local pin for
//!   the claimed sender, so an attacker cannot relabel one sender's wire row as
//!   another sender's account.
//!
//! The receive paths have different ways to learn the sender key: v1/v2
//! resolve it out of band from the claimed sender id, while v3/v4/v5 carry an
//! authenticated sender identity key in the wire or in ratchet-associated data.
//! Bundle-backed proofs also bind the verified identity bundle to the exact
//! wire version being decoded so a proof cannot be relabelled across wire
//! versions or bundle revisions.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use crypto::{ed25519, x25519};
use keystore::identity_bundle::{BundleVerifyError, BundleVerifyPolicy, IdentityBundle};
use sha2::{Digest, Sha256};
use std::fmt;

const SENDER_ATTRIBUTION_PROOF_DOMAIN: &[u8] = b"OSL/sender-attribution-proof/v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SenderAttributionProofError {
    Bundle(BundleVerifyError),
    WireVersionRelabel,
    MissingLocalPin,
    MalformedLocalPin,
    RelabelRejected,
}

impl fmt::Display for SenderAttributionProofError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bundle(error) => write!(f, "sender attribution bundle refused: {error}"),
            Self::WireVersionRelabel => {
                f.write_str("sender attribution proof does not bind this wire version")
            }
            Self::MissingLocalPin => f.write_str("sender attribution refused: missing local pin"),
            Self::MalformedLocalPin => {
                f.write_str("sender attribution refused: malformed local pin")
            }
            Self::RelabelRejected => f.write_str(
                "sender attribution refused: authenticated key does not match local pin",
            ),
        }
    }
}

impl std::error::Error for SenderAttributionProofError {}

impl From<BundleVerifyError> for SenderAttributionProofError {
    fn from(error: BundleVerifyError) -> Self {
        Self::Bundle(error)
    }
}

/// Version-independent witness that a sender label was not relabelled.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct SenderAttributionProof {
    commitment: [u8; 32],
    wire_version: u8,
    bundle_revision: u64,
    capability_bundle: u32,
    sender_ik_pub: x25519::PublicKey,
}

impl SenderAttributionProof {
    pub fn create(
        bundle: &IdentityBundle,
        pinned_signer: &ed25519::PublicKey,
        last_known_revision: Option<u64>,
        wire_version: u8,
    ) -> Result<Self, SenderAttributionProofError> {
        let revision =
            BundleVerifyPolicy::new().verify(bundle, pinned_signer, last_known_revision)?;
        Ok(Self {
            commitment: proof_commitment(bundle, wire_version),
            wire_version,
            bundle_revision: revision,
            capability_bundle: bundle.capability_bundle,
            sender_ik_pub: x25519::PublicKey::from_bytes(bundle.x25519_identity_pub),
        })
    }

    pub fn verify_for(
        &self,
        bundle: &IdentityBundle,
        pinned_signer: &ed25519::PublicKey,
        last_known_revision: Option<u64>,
        wire_version: u8,
    ) -> Result<(), SenderAttributionProofError> {
        let revision =
            BundleVerifyPolicy::new().verify(bundle, pinned_signer, last_known_revision)?;
        if self.wire_version == wire_version
            && self.bundle_revision == revision
            && self.capability_bundle == bundle.capability_bundle
            && self.sender_ik_pub.as_bytes() == &bundle.x25519_identity_pub
            && self.commitment == proof_commitment(bundle, wire_version)
        {
            Ok(())
        } else {
            Err(SenderAttributionProofError::WireVersionRelabel)
        }
    }

    pub fn wire_version(&self) -> u8 {
        self.wire_version
    }

    pub fn bundle_revision(&self) -> u64 {
        self.bundle_revision
    }

    /// The authenticated X25519 identity key covered by this proof.
    pub fn sender_ik_pub(&self) -> &x25519::PublicKey {
        &self.sender_ik_pub
    }
}

impl fmt::Debug for SenderAttributionProof {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SenderAttributionProof")
            .field("commitment", &"<redacted>")
            .field("wire_version", &self.wire_version)
            .field("bundle_revision", &self.bundle_revision)
            .field("capability_bundle", &self.capability_bundle)
            .field("sender_ik_pub", &"<redacted>")
            .finish()
    }
}

/// Prove attribution when the caller already resolved a local pin.
pub fn prove_resolved_sender_key(
    pinned_sender_ik_pub: &x25519::PublicKey,
    authenticated_sender_ik_pub: &x25519::PublicKey,
) -> Result<SenderAttributionProof, SenderAttributionProofError> {
    if pinned_sender_ik_pub != authenticated_sender_ik_pub {
        return Err(SenderAttributionProofError::RelabelRejected);
    }
    Ok(SenderAttributionProof {
        commitment: [0u8; 32],
        wire_version: 0,
        bundle_revision: 0,
        capability_bundle: 0,
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

fn proof_commitment(bundle: &IdentityBundle, wire_version: u8) -> [u8; 32] {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(SENDER_ATTRIBUTION_PROOF_DOMAIN);
    bytes.push(wire_version);
    bytes.extend_from_slice(&bundle.signed_bytes());
    Sha256::digest(bytes).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signed_bundle(
        signer_secret: &ed25519::SecretKey,
        signer_public: &ed25519::PublicKey,
        revision: u64,
        capability_bundle: u32,
    ) -> IdentityBundle {
        let mut bundle = IdentityBundle {
            ed25519_identity_pub: *signer_public.as_bytes(),
            x25519_identity_pub: [0x11; 32],
            mlkem768_identity_pub: [0x22; crypto::ml_kem_768::ENCAPSULATION_KEY_SIZE],
            capability_bundle,
            revision,
            signature: [0u8; ed25519::SIGNATURE_SIZE],
        };
        let signature = ed25519::sign(signer_secret, &bundle.signed_bytes());
        bundle.signature = *signature.as_bytes();
        bundle
    }

    #[test]
    fn sender_attribution_proof_prevents_bundle_wire_version_relabel() {
        let (sender_secret, sender_public) = ed25519::generate_keypair();
        let bundle = signed_bundle(&sender_secret, &sender_public, 5, 0b0000_0011);
        let legacy_wire_version = crate::wire_rn::LEGACY_WIRE_VERSION_V3;
        let rn_wire_version = osl_ratchet_next::WIRE_VERSION_RN;

        let proof =
            SenderAttributionProof::create(&bundle, &sender_public, Some(4), legacy_wire_version)
                .expect("signed monotonic bundle should produce a proof");

        assert_eq!(
            proof.verify_for(&bundle, &sender_public, Some(4), legacy_wire_version),
            Ok(())
        );
        assert_eq!(
            proof.verify_for(&bundle, &sender_public, Some(4), rn_wire_version),
            Err(SenderAttributionProofError::WireVersionRelabel)
        );

        let relabeled_bundle = signed_bundle(&sender_secret, &sender_public, 6, 0b0000_0011);
        assert_eq!(
            proof.verify_for(
                &relabeled_bundle,
                &sender_public,
                Some(5),
                legacy_wire_version
            ),
            Err(SenderAttributionProofError::WireVersionRelabel)
        );
    }

    #[test]
    fn sender_attribution_proof_prevents_pinned_key_relabel() {
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
