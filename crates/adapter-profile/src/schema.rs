//! Shared adapter-profile document schema.
//!
//! A profile is accepted only after two independent checks:
//!
//! 1. the signed envelope verifier proves the canonical profile bytes were
//!    signed by a trusted release anchor; and
//! 2. this module proves the profile's structure grants no capability unless
//!    consent, binding, and authority are explicit.
//!
//! This module owns step 2. Absence, parse ambiguity, unknown fields, duplicate
//! capabilities, missing consent, missing binding, or missing authority all mean
//! refusal.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use crypto::ed25519;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;
use thiserror::Error;

pub const PROFILE_DOC_VERSION: u32 = 1;
pub const PROFILE_DOC_ENVELOPE_VERSION: u32 = 1;
pub const PROFILE_DOC_SCHEMA_VERSION: u32 = 1;
pub const PROFILE_DOC_DOMAIN: &str = "osl/adapter-profile/v1";
const MIN_CANARY_TTL_SECONDS: u64 = 30;

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

/// Signed profile document transported between release infrastructure
/// and the client.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SignedProfileDoc {
    pub envelope_version: u32,
    pub payload_b64: String,
    pub signature_b64: String,
    pub signing_key_b64: String,
}

impl fmt::Debug for SignedProfileDoc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SignedProfileDoc")
            .field("envelope_version", &self.envelope_version)
            .field("payload_b64_len", &self.payload_b64.len())
            .field("signature_b64", &"<redacted>")
            .field("signing_key_b64", &"<redacted>")
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

/// Signed profile content. This is intentionally data-only: no
/// script bodies, shell commands, executable paths, or arbitrary
/// native hooks can be represented.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProfilePayload {
    pub domain: String,
    pub schema_version: u32,
    pub adapter_id: String,
    pub app: AppDescriptor,
    pub revision: ProfileRevision,
    pub issued_at_unix_seconds: u64,
    pub expires_at_unix_seconds: u64,
    pub support: SupportLevel,
    pub authority: AuthorityRequirements,
    pub selectors: Vec<TypedSelector>,
    pub fallbacks: Vec<FallbackStrategy>,
    pub canary: HarmlessCanary,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AppDescriptor {
    pub stable_id: String,
    pub display_name: String,
    pub service_family: String,
    pub min_app_version: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProfileRevision {
    pub number: u64,
    pub label: String,
}

/// Explicit requirements that callers must satisfy before a profile
/// can be used. Required bool fields are intentional: absence refuses
/// at deserialization, and `false` refuses at use-validation.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AuthorityRequirements {
    pub user_consent_required: bool,
    pub account_binding_required: bool,
    pub release_authority_required: bool,
    pub harmless_canary_required: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SupportLevel {
    Supported,
    Experimental,
    ComingSoon,
    ExternallyBlocked,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TypedSelector {
    pub kind: SelectorKind,
    pub strategy: SelectorStrategy,
    pub required: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SelectorKind {
    AppRoot,
    AccountBadge,
    ConversationTitle,
    MessageList,
    MessageRow,
    ComposerInput,
    SendButton,
    SentState,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SelectorStrategy {
    Accessibility {
        role: String,
        name: Option<String>,
        automation_id: Option<String>,
    },
    Css {
        selector: String,
    },
    TextAnchor {
        starts_with: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FallbackStrategy {
    pub selector: SelectorKind,
    pub condition: FallbackCondition,
    pub replacement: SelectorStrategy,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FallbackCondition {
    MissingRequiredSelector,
    AppVersionAtLeast,
    CanaryMismatch,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HarmlessCanary {
    pub selector: SelectorKind,
    pub expected_text: String,
    pub max_age_seconds: u64,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProfileError {
    #[error("profile envelope version mismatch")]
    EnvelopeVersion,
    #[error("profile schema version mismatch")]
    SchemaVersion,
    #[error("profile domain mismatch")]
    Domain,
    #[error("profile field {field} is empty")]
    EmptyField { field: &'static str },
    #[error("profile revision must be nonzero")]
    EmptyRevision,
    #[error("profile expiry is not after issue time")]
    InvalidExpiry,
    #[error("profile is not yet valid")]
    Future,
    #[error("profile has expired")]
    Expired,
    #[error("profile support level refuses use")]
    Unsupported,
    #[error("profile authority requirement {field} is not explicit")]
    MissingAuthority { field: &'static str },
    #[error("profile has no required message row selector")]
    MissingMessageRowSelector,
    #[error("profile harmless canary is invalid")]
    InvalidCanary,
    #[error("base64 decode error in profile field {field}")]
    Base64 { field: &'static str },
    #[error("profile signing key length mismatch")]
    SigningKeyLength,
    #[error("profile signature length mismatch")]
    SignatureLength,
    #[error("profile signing key mismatch")]
    SigningKeyMismatch,
    #[error("profile signature verification failed")]
    BadSignature,
    #[error("profile crypto verification failed")]
    CryptoVerify,
    #[error("profile JSON error")]
    Json,
}

impl ProfilePayload {
    pub fn validate_document(&self, now_unix_seconds: u64) -> Result<(), ProfileError> {
        if self.domain != PROFILE_DOC_DOMAIN {
            return Err(ProfileError::Domain);
        }
        if self.schema_version != PROFILE_DOC_SCHEMA_VERSION {
            return Err(ProfileError::SchemaVersion);
        }
        require_nonempty("adapter_id", &self.adapter_id)?;
        require_nonempty("app.stable_id", &self.app.stable_id)?;
        require_nonempty("app.display_name", &self.app.display_name)?;
        require_nonempty("app.service_family", &self.app.service_family)?;
        require_nonempty("revision.label", &self.revision.label)?;
        if self.revision.number == 0 {
            return Err(ProfileError::EmptyRevision);
        }
        if self.expires_at_unix_seconds <= self.issued_at_unix_seconds {
            return Err(ProfileError::InvalidExpiry);
        }
        if self.issued_at_unix_seconds > now_unix_seconds {
            return Err(ProfileError::Future);
        }
        if self.expires_at_unix_seconds <= now_unix_seconds {
            return Err(ProfileError::Expired);
        }
        self.validate_canary()?;
        Ok(())
    }

    pub fn validate_for_use(&self, now_unix_seconds: u64) -> Result<(), ProfileError> {
        self.validate_document(now_unix_seconds)?;
        if !matches!(
            self.support,
            SupportLevel::Supported | SupportLevel::Experimental
        ) {
            return Err(ProfileError::Unsupported);
        }
        self.authority.validate_for_use()?;
        if !self
            .selectors
            .iter()
            .any(|selector| selector.required && selector.kind == SelectorKind::MessageRow)
        {
            return Err(ProfileError::MissingMessageRowSelector);
        }
        Ok(())
    }

    fn validate_canary(&self) -> Result<(), ProfileError> {
        if self.canary.expected_text.trim().is_empty()
            || self.canary.max_age_seconds < MIN_CANARY_TTL_SECONDS
        {
            return Err(ProfileError::InvalidCanary);
        }
        Ok(())
    }
}

impl AuthorityRequirements {
    pub fn validate_for_use(&self) -> Result<(), ProfileError> {
        if !self.user_consent_required {
            return Err(ProfileError::MissingAuthority {
                field: "user_consent_required",
            });
        }
        if !self.account_binding_required {
            return Err(ProfileError::MissingAuthority {
                field: "account_binding_required",
            });
        }
        if !self.release_authority_required {
            return Err(ProfileError::MissingAuthority {
                field: "release_authority_required",
            });
        }
        if !self.harmless_canary_required {
            return Err(ProfileError::MissingAuthority {
                field: "harmless_canary_required",
            });
        }
        Ok(())
    }
}

/// Canonical bytes signed by the profile authority.
pub fn canonical_profile_payload_bytes(payload: &ProfilePayload) -> Result<Vec<u8>, ProfileError> {
    serde_json::to_vec(payload).map_err(|_| ProfileError::Json)
}

pub fn sign_profile_doc(
    signer_secret: &ed25519::SecretKey,
    signer_public: &ed25519::PublicKey,
    payload: &ProfilePayload,
) -> Result<SignedProfileDoc, ProfileError> {
    let bytes = canonical_profile_payload_bytes(payload)?;
    let signature = ed25519::sign(signer_secret, &bytes);
    Ok(SignedProfileDoc {
        envelope_version: PROFILE_DOC_ENVELOPE_VERSION,
        payload_b64: STANDARD.encode(bytes),
        signature_b64: STANDARD.encode(signature.as_bytes()),
        signing_key_b64: STANDARD.encode(signer_public.as_bytes()),
    })
}

pub fn verify_profile_doc(
    doc: &SignedProfileDoc,
    trusted_signing_key_b64: &str,
    now_unix_seconds: u64,
) -> Result<ProfilePayload, ProfileError> {
    if doc.envelope_version != PROFILE_DOC_ENVELOPE_VERSION {
        return Err(ProfileError::EnvelopeVersion);
    }
    if doc.signing_key_b64 != trusted_signing_key_b64 {
        return Err(ProfileError::SigningKeyMismatch);
    }

    let signing_key = decode_b64("signing_key_b64", trusted_signing_key_b64)?;
    if signing_key.len() != ed25519::PUBLIC_KEY_SIZE {
        return Err(ProfileError::SigningKeyLength);
    }
    let signature = decode_b64("signature_b64", &doc.signature_b64)?;
    if signature.len() != ed25519::SIGNATURE_SIZE {
        return Err(ProfileError::SignatureLength);
    }
    let payload_bytes = decode_b64("payload_b64", &doc.payload_b64)?;

    let mut signing_key_array = [0u8; ed25519::PUBLIC_KEY_SIZE];
    signing_key_array.copy_from_slice(&signing_key);
    let public_key = ed25519::PublicKey::from_bytes(signing_key_array);
    let mut signature_array = [0u8; ed25519::SIGNATURE_SIZE];
    signature_array.copy_from_slice(&signature);
    let signature = ed25519::Signature::from_bytes(signature_array);

    let ok = ed25519::verify(&public_key, &payload_bytes, &signature)
        .map_err(|_| ProfileError::CryptoVerify)?;
    if !ok {
        return Err(ProfileError::BadSignature);
    }

    let payload: ProfilePayload =
        serde_json::from_slice(&payload_bytes).map_err(|_| ProfileError::Json)?;
    let recomputed = canonical_profile_payload_bytes(&payload)?;
    if recomputed != payload_bytes {
        return Err(ProfileError::BadSignature);
    }
    payload.validate_for_use(now_unix_seconds)?;
    Ok(payload)
}

fn decode_b64(field: &'static str, value: &str) -> Result<Vec<u8>, ProfileError> {
    STANDARD
        .decode(value)
        .map_err(|_| ProfileError::Base64 { field })
}

fn require_nonempty(field: &'static str, value: &str) -> Result<(), ProfileError> {
    if value.trim().is_empty() {
        return Err(ProfileError::EmptyField { field });
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

    const NOW: u64 = 1_780_000_000;

    fn sample_payload() -> ProfilePayload {
        ProfilePayload {
            domain: PROFILE_DOC_DOMAIN.to_string(),
            schema_version: PROFILE_DOC_SCHEMA_VERSION,
            adapter_id: "discord.desktop".to_string(),
            app: AppDescriptor {
                stable_id: "discord.desktop".to_string(),
                display_name: "Discord".to_string(),
                service_family: "chat".to_string(),
                min_app_version: Some("1.0.0".to_string()),
            },
            revision: ProfileRevision {
                number: 1,
                label: "2026-07-30".to_string(),
            },
            issued_at_unix_seconds: NOW - 60,
            expires_at_unix_seconds: NOW + 3_600,
            support: SupportLevel::Supported,
            authority: AuthorityRequirements {
                user_consent_required: true,
                account_binding_required: true,
                release_authority_required: true,
                harmless_canary_required: true,
            },
            selectors: vec![
                TypedSelector {
                    kind: SelectorKind::MessageRow,
                    required: true,
                    strategy: SelectorStrategy::Accessibility {
                        role: "listitem".to_string(),
                        name: None,
                        automation_id: Some("message-row".to_string()),
                    },
                },
                TypedSelector {
                    kind: SelectorKind::ComposerInput,
                    required: true,
                    strategy: SelectorStrategy::Css {
                        selector: "[data-slate-editor=true]".to_string(),
                    },
                },
            ],
            fallbacks: vec![FallbackStrategy {
                selector: SelectorKind::ComposerInput,
                condition: FallbackCondition::MissingRequiredSelector,
                replacement: SelectorStrategy::Accessibility {
                    role: "textbox".to_string(),
                    name: None,
                    automation_id: None,
                },
            }],
            canary: HarmlessCanary {
                selector: SelectorKind::AppRoot,
                expected_text: "Friends".to_string(),
                max_age_seconds: 300,
            },
        }
    }

    fn signer() -> (ed25519::SecretKey, ed25519::PublicKey, String) {
        let (secret, public) = ed25519::generate_keypair();
        let trusted = STANDARD.encode(public.as_bytes());
        (secret, public, trusted)
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
    fn schema() {
        let payload = sample_payload();
        let (secret, public, trusted) = signer();
        let doc = sign_profile_doc(&secret, &public, &payload).unwrap();

        assert_eq!(verify_profile_doc(&doc, &trusted, NOW).unwrap(), payload);

        let mut tampered = doc.clone();
        let mut bytes = STANDARD.decode(&tampered.payload_b64).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        tampered.payload_b64 = STANDARD.encode(bytes);
        assert_eq!(
            verify_profile_doc(&tampered, &trusted, NOW),
            Err(ProfileError::BadSignature)
        );

        let mut missing_authority = payload.clone();
        missing_authority.authority.release_authority_required = false;
        let unsigned_refusal = missing_authority.validate_for_use(NOW);
        assert_eq!(
            unsigned_refusal,
            Err(ProfileError::MissingAuthority {
                field: "release_authority_required"
            })
        );

        let mut script_field = serde_json::to_value(&payload).unwrap();
        script_field["script"] = serde_json::json!("native-hook.exe");
        assert!(
            serde_json::from_value::<ProfilePayload>(script_field).is_err(),
            "signed profile payloads must not carry arbitrary executable hooks"
        );
    }

    #[test]
    fn validate_structure() {
        let validated = profile().validate_structure().unwrap();
        assert!(validated.permits(Capability::PlaceProtectedPayload, full_evidence()));

        let mut no_consent = full_evidence();
        no_consent.user_consented = false;
        assert!(!validated.permits(Capability::PlaceProtectedPayload, no_consent));

        let mut no_recipient_binding = full_evidence();
        no_recipient_binding.recipient_bound = false;
        assert!(!validated.permits(
            Capability::PlaceProtectedPayload,
            no_recipient_binding
        ));

        let mut wrong_authority = full_evidence();
        wrong_authority.authority = Some(AdapterAuthority::DocumentedPlatformApi);
        assert!(!validated.permits(Capability::PlaceProtectedPayload, wrong_authority));

        let mut absent_binding = serde_json::to_value(profile()).unwrap();
        absent_binding["capabilities"][0]
            .as_object_mut()
            .unwrap()
            .remove("binding_required");
        let json = serde_json::to_vec(&absent_binding).unwrap();
        assert_eq!(parse_profile_doc(&json), Err(ProfileValidationError::Json));

        let mut false_consent = profile();
        false_consent.capabilities[0].consent_required = false;
        assert_eq!(
            false_consent.validate_structure(),
            Err(ProfileValidationError::ConsentNotRequired(
                Capability::InspectVisibleComposer
            ))
        );

        let mut incomplete_outcome = profile();
        incomplete_outcome.send_outcome.reports_unknown = false;
        assert_eq!(
            incomplete_outcome.validate_structure(),
            Err(ProfileValidationError::MissingTriStateSendOutcome)
        );
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
    fn schema_round_trips_signed_profile_doc() {
        let payload = sample_payload();
        let (secret, public, trusted) = signer();
        let doc = sign_profile_doc(&secret, &public, &payload).unwrap();

        let verified = verify_profile_doc(&doc, &trusted, NOW).unwrap();

        assert_eq!(verified, payload);
        verified.validate_for_use(NOW).unwrap();
    }

    #[test]
    fn schema_rejects_unknown_script_field() {
        let json = serde_json::json!({
            "domain": PROFILE_DOC_DOMAIN,
            "schema_version": PROFILE_DOC_SCHEMA_VERSION,
            "adapter_id": "discord.desktop",
            "app": {
                "stable_id": "discord.desktop",
                "display_name": "Discord",
                "service_family": "chat",
                "min_app_version": null
            },
            "revision": { "number": 1, "label": "rev1" },
            "issued_at_unix_seconds": NOW - 1,
            "expires_at_unix_seconds": NOW + 1,
            "support": "supported",
            "authority": {
                "user_consent_required": true,
                "account_binding_required": true,
                "release_authority_required": true,
                "harmless_canary_required": true
            },
            "selectors": [],
            "fallbacks": [],
            "canary": {
                "selector": "app_root",
                "expected_text": "Friends",
                "max_age_seconds": 300
            },
            "script": "sh -c echo unsafe"
        });

        let parsed = serde_json::from_value::<ProfilePayload>(json);

        assert!(parsed.is_err());
    }

    #[test]
    fn schema_missing_authority_field_refuses_by_not_deserializing() {
        let json = serde_json::json!({
            "user_consent_required": true,
            "account_binding_required": true,
            "release_authority_required": true
        });

        let parsed = serde_json::from_value::<AuthorityRequirements>(json);

        assert!(parsed.is_err());
    }

    #[test]
    fn schema_false_authority_field_refuses_use() {
        let mut payload = sample_payload();
        payload.authority.account_binding_required = false;

        let err = payload.validate_for_use(NOW).unwrap_err();

        assert_eq!(
            err,
            ProfileError::MissingAuthority {
                field: "account_binding_required"
            }
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

    #[test]
    fn schema_blocked_profile_is_valid_document_but_not_usable() {
        let mut payload = sample_payload();
        payload.support = SupportLevel::ExternallyBlocked;

        payload.validate_document(NOW).unwrap();
        let err = payload.validate_for_use(NOW).unwrap_err();

        assert_eq!(err, ProfileError::Unsupported);
    }

    #[test]
    fn schema_requires_message_row_for_use() {
        let mut payload = sample_payload();
        payload
            .selectors
            .retain(|selector| selector.kind != SelectorKind::MessageRow);

        let err = payload.validate_for_use(NOW).unwrap_err();

        assert_eq!(err, ProfileError::MissingMessageRowSelector);
    }

    #[test]
    fn schema_rejects_tampered_signed_payload() {
        let payload = sample_payload();
        let (secret, public, trusted) = signer();
        let mut doc = sign_profile_doc(&secret, &public, &payload).unwrap();
        let mut bytes = STANDARD.decode(&doc.payload_b64).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        doc.payload_b64 = STANDARD.encode(bytes);

        let err = verify_profile_doc(&doc, &trusted, NOW).unwrap_err();

        assert_eq!(err, ProfileError::BadSignature);
    }

    #[test]
    fn schema_rejects_wrong_signing_key() {
        let payload = sample_payload();
        let (secret, public, _) = signer();
        let (_, _, wrong_trusted) = signer();
        let doc = sign_profile_doc(&secret, &public, &payload).unwrap();

        let err = verify_profile_doc(&doc, &wrong_trusted, NOW).unwrap_err();

        assert_eq!(err, ProfileError::SigningKeyMismatch);
    }

    #[test]
    fn schema_signed_profile_doc_debug_redacts_signature_and_key() {
        let payload = sample_payload();
        let (secret, public, _) = signer();
        let doc = sign_profile_doc(&secret, &public, &payload).unwrap();

        let printed = format!("{doc:?}");

        assert!(printed.contains("payload_b64_len"));
        assert!(!printed.contains(&doc.signature_b64));
        assert!(!printed.contains(&doc.signing_key_b64));
    }
}
