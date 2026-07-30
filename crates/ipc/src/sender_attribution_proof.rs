//! Sender-attribution proof for decoded wire rows.
//!
//! A row attributed to a sender is not just "some bundle verified once".
//! The proof below binds the verified identity bundle to the exact wire
//! version being decoded. Reusing the same proof while relabeling a legacy
//! message as an RN message, or the reverse, recomputes a different commitment
//! and fails closed.

use std::fmt;

use crypto::ed25519;
use keystore::identity_bundle::{BundleVerifyError, BundleVerifyPolicy, IdentityBundle};
use sha2::{Digest, Sha256};

const SENDER_ATTRIBUTION_PROOF_DOMAIN: &[u8] = b"OSL/sender-attribution-proof/v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SenderAttributionProofError {
    Bundle(BundleVerifyError),
    WireVersionRelabel,
}

impl fmt::Display for SenderAttributionProofError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bundle(error) => write!(f, "sender attribution bundle refused: {error}"),
            Self::WireVersionRelabel => {
                f.write_str("sender attribution proof does not bind this wire version")
            }
        }
    }
}

impl std::error::Error for SenderAttributionProofError {}

impl From<BundleVerifyError> for SenderAttributionProofError {
    fn from(error: BundleVerifyError) -> Self {
        Self::Bundle(error)
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct SenderAttributionProof {
    commitment: [u8; 32],
    wire_version: u8,
    bundle_revision: u64,
    capability_bundle: u32,
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
}

impl fmt::Debug for SenderAttributionProof {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SenderAttributionProof")
            .field("commitment", &"<redacted>")
            .field("wire_version", &self.wire_version)
            .field("bundle_revision", &self.bundle_revision)
            .field("capability_bundle", &self.capability_bundle)
            .finish()
    }
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
    fn sender_attribution_proof_prevents_cross_version_relabel() {
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
}
