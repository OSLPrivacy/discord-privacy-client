//! Loader that composes envelope trust and profile schema validation.

use crate::envelope::{EnvelopeError, SignedProfile};
use crate::schema::{
    parse_profile_doc, AdapterService, ProfileDoc, ProfileValidationError, ValidatedProfile,
    PROFILE_DOC_VERSION,
};
use crate::trust::{
    verify_signed_profile_with_anchors, ShippedAnchorKey, TrustError, SHIPPED_ANCHOR_KEYS,
};
use std::fmt;
use thiserror::Error;

#[derive(Clone, PartialEq, Eq)]
pub struct LoadedProfile {
    signed: SignedProfile,
    profile: ValidatedProfile,
    anchor: ShippedAnchorKey,
}

impl LoadedProfile {
    pub fn signed(&self) -> &SignedProfile {
        &self.signed
    }

    pub fn profile(&self) -> &ValidatedProfile {
        &self.profile
    }

    pub fn doc(&self) -> &ProfileDoc {
        self.profile.doc()
    }

    pub fn anchor(&self) -> &ShippedAnchorKey {
        &self.anchor
    }
}

impl fmt::Debug for LoadedProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoadedProfile")
            .field(
                "profile_schema_version",
                &self.signed.profile_schema_version(),
            )
            .field("revision", &self.signed.revision())
            .field("service", &self.doc().service)
            .field("profile_sequence", &self.doc().profile_sequence)
            .field("anchor_key_id", &self.anchor.key_id)
            .finish()
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LoaderError {
    #[error("signed adapter profile wire envelope is invalid")]
    Envelope,
    #[error(transparent)]
    Trust(#[from] TrustError),
    #[error(transparent)]
    Schema(#[from] ProfileValidationError),
    #[error("signed adapter profile schema version does not match the profile document")]
    SchemaVersionMismatch { envelope: u32, document: u32 },
    #[error("signed adapter profile id does not match the profile document")]
    ProfileIdMismatch,
    #[error("signed adapter profile revision does not match the profile document sequence")]
    RevisionMismatch { envelope: u64, document: u64 },
    #[error("signed adapter profile is not for the reviewed Discord adapter")]
    NotDiscord,
    #[error("signed adapter profile document is below the shipped rollback floor")]
    RollbackBelowFloor { sequence: u64, floor: u64 },
}

impl From<EnvelopeError> for LoaderError {
    fn from(_: EnvelopeError) -> Self {
        Self::Envelope
    }
}

pub fn load_trusted_profile_from_wire_json(
    wire_json: &[u8],
    now_unix_seconds: u64,
) -> Result<LoadedProfile, LoaderError> {
    let signed = SignedProfile::from_wire_json(wire_json)?;
    load_trusted_profile(signed, now_unix_seconds)
}

pub fn load_trusted_profile(
    signed: SignedProfile,
    now_unix_seconds: u64,
) -> Result<LoadedProfile, LoaderError> {
    load_trusted_profile_with_anchors(signed, now_unix_seconds, SHIPPED_ANCHOR_KEYS)
}

/// Testable/custom-anchor form of [`load_trusted_profile`]. Production callers
/// should use the shipped-anchor wrapper above.
pub(crate) fn load_trusted_profile_with_anchors(
    signed: SignedProfile,
    now_unix_seconds: u64,
    anchors: &[ShippedAnchorKey],
) -> Result<LoadedProfile, LoaderError> {
    let anchor = *verify_signed_profile_with_anchors(&signed, now_unix_seconds, anchors)?;
    let doc = parse_profile_doc(signed.profile_bytes())?;
    if signed.profile_schema_version() != PROFILE_DOC_VERSION
        || doc.version != signed.profile_schema_version()
    {
        return Err(LoaderError::SchemaVersionMismatch {
            envelope: signed.profile_schema_version(),
            document: doc.version,
        });
    }
    if signed.profile_id() != doc.profile_id {
        return Err(LoaderError::ProfileIdMismatch);
    }
    if signed.revision() != doc.profile_sequence {
        return Err(LoaderError::RevisionMismatch {
            envelope: signed.revision(),
            document: doc.profile_sequence,
        });
    }
    if doc.service != AdapterService::Discord {
        return Err(LoaderError::NotDiscord);
    }
    if doc.profile_sequence < anchor.rollback_floor {
        return Err(LoaderError::RollbackBelowFloor {
            sequence: doc.profile_sequence,
            floor: anchor.rollback_floor,
        });
    }

    let profile = doc.validate_structure()?;
    Ok(LoadedProfile {
        signed,
        profile,
        anchor,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::ED25519_SIGNATURE_LEN;
    use crate::schema::{
        ActionLevel, AdapterAuthority, AdapterSurface, BindingRequirement, Capability,
        CapabilityGrant, SendOutcomeContract,
    };
    use crate::trust::DISCORD_PROFILE_ROLLBACK_FLOOR;
    use crypto::ed25519;

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

    fn grant(capability: Capability) -> CapabilityGrant {
        let binding_required = match capability {
            Capability::PrepareProtectedPayload
            | Capability::PlaceProtectedPayload
            | Capability::SendProtectedPayload
            | Capability::VerifySendOutcome => BindingRequirement::all(),
            _ => BindingRequirement::account_only(),
        };
        CapabilityGrant {
            capability,
            action_level: ActionLevel::UserAssistedAction,
            consent_required: true,
            binding_required,
            authority: AdapterAuthority::ReviewedLocalAdapter,
        }
    }

    fn discord_profile(sequence: u64) -> ProfileDoc {
        ProfileDoc {
            version: PROFILE_DOC_VERSION,
            profile_id: "discord/reviewed".to_string(),
            service: AdapterService::Discord,
            profile_sequence: sequence,
            rollback_floor: DISCORD_PROFILE_ROLLBACK_FLOOR,
            issued_at_unix_seconds: NOW - 60,
            expires_at_unix_seconds: NOW + 60,
            min_client_version: "0.0.1".to_string(),
            surfaces: vec![AdapterSurface::InstalledNativeClient],
            capabilities: vec![
                grant(Capability::InspectVisibleComposer),
                grant(Capability::PrepareProtectedPayload),
                grant(Capability::PlaceProtectedPayload),
                grant(Capability::VerifySendOutcome),
            ],
            send_outcome: SendOutcomeContract {
                reports_sent: true,
                reports_not_sent: true,
                reports_unknown: true,
                auto_retries_unknown: false,
            },
        }
    }

    fn signed(
        secret: &ed25519::SecretKey,
        anchor: ShippedAnchorKey,
        doc: &ProfileDoc,
        envelope_revision: u64,
    ) -> SignedProfile {
        let body = serde_json::to_vec(doc).unwrap();
        let unsigned = SignedProfile::new(
            PROFILE_DOC_VERSION,
            doc.profile_id.clone(),
            envelope_revision,
            doc.issued_at_unix_seconds,
            doc.expires_at_unix_seconds,
            body.clone(),
            anchor.key_id,
            anchor.public_key,
            [1u8; ED25519_SIGNATURE_LEN],
        )
        .unwrap();
        let signature = ed25519::sign(&secret, &unsigned.signing_payload().unwrap());
        SignedProfile::new(
            PROFILE_DOC_VERSION,
            doc.profile_id.clone(),
            envelope_revision,
            doc.issued_at_unix_seconds,
            doc.expires_at_unix_seconds,
            body,
            anchor.key_id,
            anchor.public_key,
            *signature.as_bytes(),
        )
        .unwrap()
    }

    #[test]
    fn loader_loads_only_signed_discord_profile_at_or_above_floor() {
        let (secret, anchor) = signer();
        let anchors = [anchor];
        let doc = discord_profile(DISCORD_PROFILE_ROLLBACK_FLOOR);
        let wire = signed(&secret, anchor, &doc, doc.profile_sequence)
            .to_wire_json()
            .unwrap();

        let loaded = SignedProfile::from_wire_json(&wire).unwrap();
        let loaded = load_trusted_profile_with_anchors(loaded, NOW, &anchors).unwrap();

        assert_eq!(loaded.doc(), &doc);
        assert_eq!(
            loaded.anchor().rollback_floor,
            DISCORD_PROFILE_ROLLBACK_FLOOR
        );
        assert!(loaded.profile().permits(
            crate::schema::Capability::PlaceProtectedPayload,
            crate::schema::ValidationEvidence {
                user_consented: true,
                account_bound: true,
                conversation_bound: true,
                recipient_bound: true,
                authority: Some(AdapterAuthority::ReviewedLocalAdapter),
            }
        ));
    }

    #[test]
    fn loader_refuses_unsigned_mismatch_non_discord_and_rollback_profiles() {
        let (secret, anchor) = signer();
        let anchors = [anchor];
        let doc = discord_profile(DISCORD_PROFILE_ROLLBACK_FLOOR);
        let mut bad_sig = signed(&secret, anchor, &doc, doc.profile_sequence);
        let mut signature = *bad_sig.signature();
        signature[0] ^= 0x80;
        bad_sig = SignedProfile::new(
            bad_sig.profile_schema_version(),
            bad_sig.profile_id(),
            bad_sig.revision(),
            bad_sig.issued_at_unix_seconds(),
            bad_sig.expires_at_unix_seconds(),
            bad_sig.profile_bytes().to_vec(),
            bad_sig.signer_key_id(),
            *bad_sig.signer_public_key(),
            signature,
        )
        .unwrap();
        assert_eq!(
            load_trusted_profile_with_anchors(bad_sig, NOW, &anchors),
            Err(LoaderError::Trust(TrustError::SignatureInvalid))
        );

        let mismatched = signed(&secret, anchor, &doc, doc.profile_sequence + 1);
        assert_eq!(
            load_trusted_profile_with_anchors(mismatched, NOW, &anchors),
            Err(LoaderError::RevisionMismatch {
                envelope: DISCORD_PROFILE_ROLLBACK_FLOOR + 1,
                document: DISCORD_PROFILE_ROLLBACK_FLOOR,
            })
        );

        let mut non_discord = discord_profile(DISCORD_PROFILE_ROLLBACK_FLOOR);
        non_discord.service = AdapterService::Signal;
        let non_discord = signed(&secret, anchor, &non_discord, non_discord.profile_sequence);
        assert_eq!(
            load_trusted_profile_with_anchors(non_discord, NOW, &anchors),
            Err(LoaderError::NotDiscord)
        );

        let rollback = discord_profile(DISCORD_PROFILE_ROLLBACK_FLOOR - 1);
        let rollback = signed(&secret, anchor, &rollback, rollback.profile_sequence);
        assert_eq!(
            load_trusted_profile_with_anchors(rollback, NOW, &anchors),
            Err(LoaderError::Trust(TrustError::RollbackBelowFloor {
                revision: DISCORD_PROFILE_ROLLBACK_FLOOR - 1,
                floor: DISCORD_PROFILE_ROLLBACK_FLOOR,
            }))
        );
    }

    #[test]
    fn loader_debug_is_structural_only() {
        let (secret, anchor) = signer();
        let anchors = [anchor];
        let doc = discord_profile(DISCORD_PROFILE_ROLLBACK_FLOOR);
        let loaded = load_trusted_profile_with_anchors(
            signed(&secret, anchor, &doc, doc.profile_sequence),
            NOW,
            &anchors,
        )
        .unwrap();

        let printed = format!("{loaded:?}");

        assert!(printed.contains("LoadedProfile"));
        assert!(printed.contains("profile_sequence"));
        assert!(!printed.contains("discord/reviewed"));
        assert!(!printed.contains("0.0.1"));
    }
}
