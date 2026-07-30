//! Trust anchors for reviewed adapter profiles.
//!
//! A signed profile is usable only when its envelope verifies under a shipped
//! release anchor and its sequence is at or above the compiled rollback floor.
//! The profile body remains untrusted data until this module succeeds.

use crate::envelope::{
    EnvelopeError, SignedProfile, ED25519_PUBLIC_KEY_LEN, ED25519_SIGNATURE_LEN,
};
use crypto::ed25519;
use std::fmt;
use thiserror::Error;

/// Reviewed Discord profile updates before this sequence are refused even if
/// their old signature still verifies.
pub const DISCORD_PROFILE_ROLLBACK_FLOOR: u64 = 7;

/// A compiled adapter-profile release-signing anchor.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ShippedAnchorKey {
    pub key_id: &'static str,
    pub public_key: [u8; ED25519_PUBLIC_KEY_LEN],
    pub rollback_floor: u64,
}

impl fmt::Debug for ShippedAnchorKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ShippedAnchorKey")
            .field("key_id", &self.key_id)
            .field("public_key", &"[redacted]")
            .field("rollback_floor", &self.rollback_floor)
            .finish()
    }
}

/// Public release keys accepted by this build.
///
/// The key bytes are public verification material. They are still redacted from
/// `Debug` output so diagnostics never normalize dumping cryptographic material
/// beside profile identifiers.
pub const SHIPPED_ANCHOR_KEYS: &[ShippedAnchorKey] = &[ShippedAnchorKey {
    key_id: "adapter-profile-discord-2026-07",
    public_key: [
        0x7b, 0xe8, 0xf3, 0x75, 0xbf, 0xcd, 0x1e, 0xfd, 0x25, 0x54, 0xbb, 0xde, 0x0d, 0x78, 0x7b,
        0x8e, 0x24, 0xdc, 0x7a, 0xd7, 0xe3, 0xef, 0xd0, 0x1d, 0x4b, 0x58, 0xd7, 0x3d, 0x97, 0xa8,
        0x47, 0x72,
    ],
    rollback_floor: DISCORD_PROFILE_ROLLBACK_FLOOR,
}];

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TrustError {
    #[error("adapter profile envelope is invalid")]
    Envelope,
    #[error("adapter profile signer is not a shipped anchor")]
    UnknownAnchor,
    #[error("adapter profile signer key does not match the shipped anchor")]
    AnchorKeyMismatch,
    #[error("adapter profile is not yet valid")]
    NotYetValid,
    #[error("adapter profile has expired")]
    Expired,
    #[error("adapter profile revision is below the shipped rollback floor")]
    RollbackBelowFloor { revision: u64, floor: u64 },
    #[error("adapter profile signature verification failed")]
    SignatureInvalid,
    #[error("adapter profile signature could not be checked")]
    CryptoVerify,
}

impl From<EnvelopeError> for TrustError {
    fn from(_: EnvelopeError) -> Self {
        Self::Envelope
    }
}

/// Verify `profile` against a shipped signing anchor and freshness/rollback
/// policy. On success, returns the anchor that authenticated it.
pub fn verify_signed_profile(
    profile: &SignedProfile,
    now_unix_seconds: u64,
) -> Result<&'static ShippedAnchorKey, TrustError> {
    verify_signed_profile_with_anchors(profile, now_unix_seconds, SHIPPED_ANCHOR_KEYS)
}

/// Testable/custom-anchor form of [`verify_signed_profile`]. Production callers
/// should use the shipped-anchor wrapper above.
pub(crate) fn verify_signed_profile_with_anchors<'a>(
    profile: &SignedProfile,
    now_unix_seconds: u64,
    anchors: &'a [ShippedAnchorKey],
) -> Result<&'a ShippedAnchorKey, TrustError> {
    profile.validate()?;

    if profile.issued_at_unix_seconds() > now_unix_seconds {
        return Err(TrustError::NotYetValid);
    }
    if profile.expires_at_unix_seconds() <= now_unix_seconds {
        return Err(TrustError::Expired);
    }

    let anchor = anchors
        .iter()
        .find(|anchor| anchor.key_id == profile.signer_key_id())
        .ok_or(TrustError::UnknownAnchor)?;

    if profile.signer_public_key() != &anchor.public_key {
        return Err(TrustError::AnchorKeyMismatch);
    }
    if profile.revision() < anchor.rollback_floor {
        return Err(TrustError::RollbackBelowFloor {
            revision: profile.revision(),
            floor: anchor.rollback_floor,
        });
    }

    let public_key = ed25519::PublicKey::from_bytes(anchor.public_key);
    let signature = ed25519::Signature::from_bytes(*profile.signature());
    let payload = profile.signing_payload()?;
    match ed25519::verify(&public_key, &payload, &signature) {
        Ok(true) => Ok(anchor),
        Ok(false) => Err(TrustError::SignatureInvalid),
        Err(_) => Err(TrustError::CryptoVerify),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::SignedProfile;

    const NOW: u64 = 1_780_000_000;

    fn signer() -> (ed25519::SecretKey, ShippedAnchorKey) {
        let (secret, public) = ed25519::generate_keypair();
        (
            secret,
            ShippedAnchorKey {
                key_id: "test-adapter-profile-anchor",
                public_key: *public.as_bytes(),
                rollback_floor: DISCORD_PROFILE_ROLLBACK_FLOOR,
            },
        )
    }

    fn signed_profile(
        secret: &ed25519::SecretKey,
        anchor: ShippedAnchorKey,
        revision: u64,
        body: &[u8],
    ) -> SignedProfile {
        let unsigned = SignedProfile::new(
            1,
            "discord/reviewed",
            revision,
            NOW - 60,
            NOW + 60,
            body.to_vec(),
            anchor.key_id,
            anchor.public_key,
            [1u8; ED25519_SIGNATURE_LEN],
        )
        .unwrap();
        let signature = ed25519::sign(&secret, &unsigned.signing_payload().unwrap());
        SignedProfile::new(
            1,
            "discord/reviewed",
            revision,
            NOW - 60,
            NOW + 60,
            body.to_vec(),
            anchor.key_id,
            anchor.public_key,
            *signature.as_bytes(),
        )
        .unwrap()
    }

    #[test]
    fn trust_shipped_anchor_publishes_discord_floor() {
        const EXPECTED_PUBLIC_KEY: [u8; ED25519_PUBLIC_KEY_LEN] = [
            0x7b, 0xe8, 0xf3, 0x75, 0xbf, 0xcd, 0x1e, 0xfd, 0x25, 0x54, 0xbb, 0xde, 0x0d, 0x78,
            0x7b, 0x8e, 0x24, 0xdc, 0x7a, 0xd7, 0xe3, 0xef, 0xd0, 0x1d, 0x4b, 0x58, 0xd7, 0x3d,
            0x97, 0xa8, 0x47, 0x72,
        ];
        let anchor = SHIPPED_ANCHOR_KEYS[0];

        assert_eq!(anchor.key_id, "adapter-profile-discord-2026-07");
        assert_eq!(anchor.public_key, EXPECTED_PUBLIC_KEY);
        assert_eq!(anchor.rollback_floor, DISCORD_PROFILE_ROLLBACK_FLOOR);
    }

    #[test]
    fn trust_anchor_verifies_signature_and_floor() {
        let (secret, anchor) = signer();
        let profile = signed_profile(
            &secret,
            anchor,
            DISCORD_PROFILE_ROLLBACK_FLOOR,
            br#"{"ok":true}"#,
        );
        let anchors = [anchor];

        let verified = verify_signed_profile_with_anchors(&profile, NOW, &anchors).unwrap();

        assert_eq!(verified.key_id, anchor.key_id);
        assert_eq!(verified.rollback_floor, DISCORD_PROFILE_ROLLBACK_FLOOR);
    }

    #[test]
    fn trust_refuses_tampered_signature_unknown_anchor_and_rollback() {
        let (secret, anchor) = signer();
        let anchors = [anchor];
        let mut tampered = signed_profile(
            &secret,
            anchor,
            DISCORD_PROFILE_ROLLBACK_FLOOR,
            br#"{"ok":true}"#,
        );
        let mut signature = *tampered.signature();
        signature[0] ^= 0x80;
        tampered = SignedProfile::new(
            tampered.profile_schema_version(),
            tampered.profile_id(),
            tampered.revision(),
            tampered.issued_at_unix_seconds(),
            tampered.expires_at_unix_seconds(),
            tampered.profile_bytes().to_vec(),
            tampered.signer_key_id(),
            *tampered.signer_public_key(),
            signature,
        )
        .unwrap();
        assert_eq!(
            verify_signed_profile_with_anchors(&tampered, NOW, &anchors),
            Err(TrustError::SignatureInvalid)
        );

        let unknown = SignedProfile::new(
            1,
            "discord/reviewed",
            DISCORD_PROFILE_ROLLBACK_FLOOR,
            NOW - 60,
            NOW + 60,
            br#"{"ok":true}"#.to_vec(),
            "adapter-profile-discord-unshipped",
            anchor.public_key,
            [1u8; ED25519_SIGNATURE_LEN],
        )
        .unwrap();
        assert_eq!(
            verify_signed_profile_with_anchors(&unknown, NOW, &anchors),
            Err(TrustError::UnknownAnchor)
        );

        let rollback = signed_profile(
            &secret,
            anchor,
            DISCORD_PROFILE_ROLLBACK_FLOOR - 1,
            br#"{"ok":true}"#,
        );
        assert_eq!(
            verify_signed_profile_with_anchors(&rollback, NOW, &anchors),
            Err(TrustError::RollbackBelowFloor {
                revision: DISCORD_PROFILE_ROLLBACK_FLOOR - 1,
                floor: DISCORD_PROFILE_ROLLBACK_FLOOR,
            })
        );
    }

    #[test]
    fn trust_debug_redacts_anchor_key_material() {
        let printed = format!("{:?}", SHIPPED_ANCHOR_KEYS[0]);
        assert!(printed.contains("adapter-profile-discord-2026-07"));
        assert!(printed.contains("[redacted]"));
        assert!(!printed.contains("7be8f375"));
    }
}
