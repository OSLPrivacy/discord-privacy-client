//! Shared adapter-profile document schema.
//!
//! A profile is accepted only after two independent checks:
//!
//! 1. a future envelope verifier proves the canonical profile bytes were signed
//!    by a trusted release anchor; and
//! 2. this module proves the profile's structure grants no capability unless
//!    consent, binding, and authority are explicit.
//!
//! This module owns step 2. Absence, parse ambiguity, unknown fields, duplicate
//! capabilities, missing consent, missing binding, or missing authority all mean
//! refusal.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;
use thiserror::Error;

pub const PROFILE_DOC_VERSION: u32 = 1;

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProfileDoc {
    pub version: u32,
    pub profile_id: String,
    pub service: AdapterService,
    pub profile_sequence: u64,
    pub rollback_floor: u64,
    pub issued_at_unix_seconds: u64,
    pub expires_at_unix_seconds: u64,
    pub min_client_version: String,
    pub surfaces: Vec<AdapterSurface>,
    pub capabilities: Vec<CapabilityGrant>,
    pub send_outcome: SendOutcomeContract,
}

impl fmt::Debug for ProfileDoc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProfileDoc")
            .field("version", &self.version)
            .field("profile_id", &"<redacted>")
            .field("service", &self.service)
            .field("profile_sequence", &self.profile_sequence)
            .field("rollback_floor", &self.rollback_floor)
            .field("issued_at_unix_seconds", &self.issued_at_unix_seconds)
            .field("expires_at_unix_seconds", &self.expires_at_unix_seconds)
            .field("min_client_version", &"<redacted>")
            .field("surfaces", &self.surfaces)
            .field("capabilities_len", &self.capabilities.len())
            .field("send_outcome", &self.send_outcome)
            .finish()
    }
}

impl ProfileDoc {
    pub fn validate_structure(&self) -> Result<ValidatedProfile, ProfileValidationError> {
        fn require_canonical_text(
            field: &'static str,
            value: &str,
        ) -> Result<(), ProfileValidationError> {
            require_non_empty(field, value)?;
            if value.trim() != value || value.chars().any(char::is_control) {
                return Err(ProfileValidationError::EmptyField(field));
            }
            Ok(())
        }

        if self.version != PROFILE_DOC_VERSION {
            return Err(ProfileValidationError::UnsupportedVersion {
                got: self.version,
                expected: PROFILE_DOC_VERSION,
            });
        }
        require_canonical_text("profile_id", &self.profile_id)?;
        require_canonical_text("min_client_version", &self.min_client_version)?;
        if self.profile_sequence < self.rollback_floor {
            return Err(ProfileValidationError::RollbackBelowFloor);
        }
        if self.issued_at_unix_seconds >= self.expires_at_unix_seconds {
            return Err(ProfileValidationError::InvalidValidityWindow);
        }
        if self.surfaces.is_empty() {
            return Err(ProfileValidationError::NoSurface);
        }
        let mut seen_surfaces = BTreeSet::new();
        for surface in &self.surfaces {
            if !seen_surfaces.insert(*surface) {
                return Err(ProfileValidationError::DuplicateSurface(*surface));
            }
        }
        if self.capabilities.is_empty() {
            return Err(ProfileValidationError::NoCapabilities);
        }
        let mut seen_capabilities = BTreeSet::new();
        for grant in &self.capabilities {
            grant.validate()?;
            if !seen_capabilities.insert(grant.capability) {
                return Err(ProfileValidationError::DuplicateCapability(
                    grant.capability,
                ));
            }
        }
        self.send_outcome.validate()?;

        Ok(ValidatedProfile { doc: self.clone() })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdapterService {
    Discord,
    Signal,
    Telegram,
    Whatsapp,
    Instagram,
    Snapchat,
    X,
    Messenger,
    Gmail,
    Outlook,
    Proton,
    Yahoo,
    Aol,
    Gmx,
    MailCom,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum AdapterSurface {
    InstalledNativeClient,
    FixedOfficialWebOrigin,
    OslFirstParty,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    InspectVisibleComposer,
    InspectVisibleTranscript,
    WarnBeforeSend,
    SanitizeAttachment,
    PrepareProtectedPayload,
    PlaceProtectedPayload,
    SendProtectedPayload,
    VerifySendOutcome,
    DeleteLocalCopy,
    RequestPlatformRemoval,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionLevel {
    LocalProtection,
    UserAssistedAction,
    AuthorizedAutomation,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdapterAuthority {
    ReviewedLocalAdapter,
    UserCompletedNativeAction,
    DocumentedPlatformApi,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BindingRequirement {
    pub account: bool,
    pub conversation: bool,
    pub recipient: bool,
}

impl BindingRequirement {
    pub const fn all() -> Self {
        Self {
            account: true,
            conversation: true,
            recipient: true,
        }
    }

    pub const fn account_only() -> Self {
        Self {
            account: true,
            conversation: false,
            recipient: false,
        }
    }

    fn satisfied_by(self, evidence: ValidationEvidence) -> bool {
        (!self.account || evidence.account_bound)
            && (!self.conversation || evidence.conversation_bound)
            && (!self.recipient || evidence.recipient_bound)
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CapabilityGrant {
    pub capability: Capability,
    pub action_level: ActionLevel,
    pub consent_required: bool,
    pub binding_required: BindingRequirement,
    pub authority: AdapterAuthority,
}

impl CapabilityGrant {
    fn validate(&self) -> Result<(), ProfileValidationError> {
        if !self.consent_required {
            return Err(ProfileValidationError::ConsentNotRequired(self.capability));
        }
        if !matches!(
            self.authority,
            AdapterAuthority::ReviewedLocalAdapter
                | AdapterAuthority::UserCompletedNativeAction
                | AdapterAuthority::DocumentedPlatformApi
        ) {
            return Err(ProfileValidationError::MissingAuthority(self.capability));
        }

        match self.capability {
            Capability::PrepareProtectedPayload
            | Capability::PlaceProtectedPayload
            | Capability::SendProtectedPayload
            | Capability::VerifySendOutcome => {
                if self.binding_required != BindingRequirement::all() {
                    return Err(ProfileValidationError::ProtectedPathMissingBinding(
                        self.capability,
                    ));
                }
            }
            Capability::InspectVisibleComposer
            | Capability::InspectVisibleTranscript
            | Capability::WarnBeforeSend
            | Capability::SanitizeAttachment
            | Capability::DeleteLocalCopy
            | Capability::RequestPlatformRemoval => {
                if !self.binding_required.account {
                    return Err(ProfileValidationError::AccountBindingMissing(
                        self.capability,
                    ));
                }
            }
        }

        if self.action_level == ActionLevel::AuthorizedAutomation
            && self.authority != AdapterAuthority::DocumentedPlatformApi
        {
            return Err(ProfileValidationError::AutomationWithoutPlatformAuthority(
                self.capability,
            ));
        }

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SendOutcomeContract {
    pub reports_sent: bool,
    pub reports_not_sent: bool,
    pub reports_unknown: bool,
    pub auto_retries_unknown: bool,
}

impl SendOutcomeContract {
    fn validate(&self) -> Result<(), ProfileValidationError> {
        if !(self.reports_sent && self.reports_not_sent && self.reports_unknown) {
            return Err(ProfileValidationError::MissingTriStateSendOutcome);
        }
        if self.auto_retries_unknown {
            return Err(ProfileValidationError::AutoRetriesUnknownSendOutcome);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ValidationEvidence {
    pub user_consented: bool,
    pub account_bound: bool,
    pub conversation_bound: bool,
    pub recipient_bound: bool,
    pub authority: Option<AdapterAuthority>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedProfile {
    doc: ProfileDoc,
}

impl ValidatedProfile {
    pub fn doc(&self) -> &ProfileDoc {
        &self.doc
    }

    pub fn grant_for(&self, capability: Capability) -> Option<&CapabilityGrant> {
        self.doc
            .capabilities
            .iter()
            .find(|grant| grant.capability == capability)
    }

    pub fn permits(&self, capability: Capability, evidence: ValidationEvidence) -> bool {
        let Some(grant) = self.grant_for(capability) else {
            return false;
        };
        evidence.user_consented
            && evidence.authority == Some(grant.authority)
            && grant.binding_required.satisfied_by(evidence)
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProfileValidationError {
    #[error("profile JSON is invalid")]
    Json,
    #[error("profile version mismatch: got {got}, expected {expected}")]
    UnsupportedVersion { got: u32, expected: u32 },
    #[error("profile field {0} is empty")]
    EmptyField(&'static str),
    #[error("profile sequence is below rollback floor")]
    RollbackBelowFloor,
    #[error("profile validity window is invalid")]
    InvalidValidityWindow,
    #[error("profile declares no adapter surface")]
    NoSurface,
    #[error("profile repeats an adapter surface")]
    DuplicateSurface(AdapterSurface),
    #[error("profile declares no capability")]
    NoCapabilities,
    #[error("profile repeats a capability")]
    DuplicateCapability(Capability),
    #[error("profile capability does not require consent")]
    ConsentNotRequired(Capability),
    #[error("profile capability lacks authority")]
    MissingAuthority(Capability),
    #[error("profile protected path lacks exact binding")]
    ProtectedPathMissingBinding(Capability),
    #[error("profile capability lacks account binding")]
    AccountBindingMissing(Capability),
    #[error("profile automation lacks documented platform authority")]
    AutomationWithoutPlatformAuthority(Capability),
    #[error("profile does not report the complete send outcome tri-state")]
    MissingTriStateSendOutcome,
    #[error("profile auto-retries unknown send outcomes")]
    AutoRetriesUnknownSendOutcome,
    #[error("profile canonicalization failed")]
    Canonicalization,
}

pub fn parse_profile_doc(json: &[u8]) -> Result<ProfileDoc, ProfileValidationError> {
    serde_json::from_slice(json).map_err(|_| ProfileValidationError::Json)
}

pub fn canonical_profile_bytes(doc: &ProfileDoc) -> Result<Vec<u8>, ProfileValidationError> {
    doc.validate_structure()?;
    serde_json::to_vec(doc).map_err(|_| ProfileValidationError::Canonicalization)
}

fn require_non_empty(field: &'static str, value: &str) -> Result<(), ProfileValidationError> {
    if value.trim().is_empty() {
        return Err(ProfileValidationError::EmptyField(field));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn profile() -> ProfileDoc {
        ProfileDoc {
            version: PROFILE_DOC_VERSION,
            profile_id: "discord-windows-reviewed-v1".to_string(),
            service: AdapterService::Discord,
            profile_sequence: 7,
            rollback_floor: 3,
            issued_at_unix_seconds: 1_700_000_000,
            expires_at_unix_seconds: 1_700_086_400,
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

    fn full_evidence() -> ValidationEvidence {
        ValidationEvidence {
            user_consented: true,
            account_bound: true,
            conversation_bound: true,
            recipient_bound: true,
            authority: Some(AdapterAuthority::ReviewedLocalAdapter),
        }
    }

    #[test]
    fn schema_valid_profile_permits_only_with_consent_binding_and_authority() {
        let validated = profile().validate_structure().unwrap();
        assert!(validated.permits(Capability::PlaceProtectedPayload, full_evidence()));

        let mut no_consent = full_evidence();
        no_consent.user_consented = false;
        assert!(!validated.permits(Capability::PlaceProtectedPayload, no_consent));

        let mut no_binding = full_evidence();
        no_binding.conversation_bound = false;
        assert!(!validated.permits(Capability::PlaceProtectedPayload, no_binding));

        let mut no_authority = full_evidence();
        no_authority.authority = None;
        assert!(!validated.permits(Capability::PlaceProtectedPayload, no_authority));
    }

    #[test]
    fn schema_absent_capability_is_refusal() {
        let validated = profile().validate_structure().unwrap();
        assert!(!validated.permits(Capability::SendProtectedPayload, full_evidence()));
    }

    #[test]
    fn schema_rejects_unknown_json_fields() {
        let mut value = serde_json::to_value(profile()).unwrap();
        value["secret_unreviewed_escape_hatch"] = serde_json::json!(true);
        let json = serde_json::to_vec(&value).unwrap();
        assert_eq!(parse_profile_doc(&json), Err(ProfileValidationError::Json));
    }

    #[test]
    fn schema_rejects_missing_required_authority_field() {
        let mut value = serde_json::to_value(profile()).unwrap();
        let first_grant = value["capabilities"][0].as_object_mut().unwrap();
        first_grant.remove("authority");
        let json = serde_json::to_vec(&value).unwrap();
        assert_eq!(parse_profile_doc(&json), Err(ProfileValidationError::Json));
    }

    #[test]
    fn validate_structure_rejects_non_canonical_text_fields() {
        let mut spaced_id = profile();
        spaced_id.profile_id = " discord-windows-reviewed-v1".to_string();
        assert_eq!(
            spaced_id.validate_structure(),
            Err(ProfileValidationError::EmptyField("profile_id"))
        );

        let mut control_version = profile();
        control_version.min_client_version = "0.0.1\n".to_string();
        assert_eq!(
            control_version.validate_structure(),
            Err(ProfileValidationError::EmptyField("min_client_version"))
        );
    }

    #[test]
    fn validate_structure_rejects_absent_consent_binding_or_authority() {
        for field in ["consent_required", "binding_required", "authority"] {
            let mut value = serde_json::to_value(profile()).unwrap();
            value["capabilities"][0]
                .as_object_mut()
                .unwrap()
                .remove(field);
            let json = serde_json::to_vec(&value).unwrap();
            assert_eq!(parse_profile_doc(&json), Err(ProfileValidationError::Json));
        }

        let mut no_consent = profile();
        no_consent.capabilities[0].consent_required = false;
        assert_eq!(
            no_consent.validate_structure(),
            Err(ProfileValidationError::ConsentNotRequired(
                Capability::InspectVisibleComposer
            ))
        );

        let mut no_account_binding = profile();
        no_account_binding.capabilities[0].binding_required.account = false;
        assert_eq!(
            no_account_binding.validate_structure(),
            Err(ProfileValidationError::AccountBindingMissing(
                Capability::InspectVisibleComposer
            ))
        );

        let validated = profile().validate_structure().unwrap();
        let mut no_authority_evidence = full_evidence();
        no_authority_evidence.authority = None;
        assert!(!validated.permits(Capability::InspectVisibleComposer, no_authority_evidence));
    }

    #[test]
    fn schema_rejects_missing_consent_requirement() {
        let mut doc = profile();
        doc.capabilities[0].consent_required = false;
        assert_eq!(
            doc.validate_structure(),
            Err(ProfileValidationError::ConsentNotRequired(
                Capability::InspectVisibleComposer
            ))
        );
    }

    #[test]
    fn schema_rejects_protected_send_without_exact_binding() {
        let mut doc = profile();
        doc.capabilities
            .iter_mut()
            .find(|grant| grant.capability == Capability::PlaceProtectedPayload)
            .unwrap()
            .binding_required
            .recipient = false;
        assert_eq!(
            doc.validate_structure(),
            Err(ProfileValidationError::ProtectedPathMissingBinding(
                Capability::PlaceProtectedPayload
            ))
        );
    }

    #[test]
    fn schema_rejects_automation_without_documented_platform_authority() {
        let mut doc = profile();
        let grant = doc
            .capabilities
            .iter_mut()
            .find(|grant| grant.capability == Capability::PlaceProtectedPayload)
            .unwrap();
        grant.action_level = ActionLevel::AuthorizedAutomation;
        grant.authority = AdapterAuthority::ReviewedLocalAdapter;
        assert_eq!(
            doc.validate_structure(),
            Err(ProfileValidationError::AutomationWithoutPlatformAuthority(
                Capability::PlaceProtectedPayload
            ))
        );
    }

    #[test]
    fn schema_rejects_incomplete_or_retrying_send_outcome_contracts() {
        let mut missing_unknown = profile();
        missing_unknown.send_outcome.reports_unknown = false;
        assert_eq!(
            missing_unknown.validate_structure(),
            Err(ProfileValidationError::MissingTriStateSendOutcome)
        );

        let mut retries_unknown = profile();
        retries_unknown.send_outcome.auto_retries_unknown = true;
        assert_eq!(
            retries_unknown.validate_structure(),
            Err(ProfileValidationError::AutoRetriesUnknownSendOutcome)
        );
    }

    #[test]
    fn schema_rejects_duplicates_and_rollback_under_floor() {
        let mut duplicate = profile();
        duplicate
            .capabilities
            .push(grant(Capability::PrepareProtectedPayload));
        assert_eq!(
            duplicate.validate_structure(),
            Err(ProfileValidationError::DuplicateCapability(
                Capability::PrepareProtectedPayload
            ))
        );

        let mut rollback = profile();
        rollback.profile_sequence = rollback.rollback_floor - 1;
        assert_eq!(
            rollback.validate_structure(),
            Err(ProfileValidationError::RollbackBelowFloor)
        );
    }

    #[test]
    fn schema_canonical_profile_bytes_are_stable() {
        let doc = profile();
        let once = canonical_profile_bytes(&doc).unwrap();
        let twice = canonical_profile_bytes(&parse_profile_doc(&once).unwrap()).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn schema_profile_debug_is_structural_only() {
        let rendered = format!("{:?}", profile());
        assert!(rendered.contains("ProfileDoc"));
        assert!(rendered.contains("capabilities_len"));
        assert!(!rendered.contains("discord-windows-reviewed-v1"));
        assert!(!rendered.contains("0.0.1"));
    }
}
