//! Fail-closed consent contract for optional cloud AutoScrub processing.
//!
//! This module only models consent and the external security-review gate. It
//! does not send data, select providers, or offer a path to enable cloud work.
//! Cloud AutoScrub is treated as a high-sensitivity flow: ordinary scope
//! consent is not enough unless the user has acknowledged the plain privacy
//! warning for the exact scope and finding set.

use std::collections::BTreeSet;
use std::fmt;

use sha2::{Digest, Sha256};

const ACCOUNT_DOMAIN: &[u8] = b"OSL/cloud-autoscrub-account/v1";
const DATA_SCOPE_DOMAIN: &[u8] = b"OSL/cloud-autoscrub-data-scope/v1";
const FINDING_DOMAIN: &[u8] = b"OSL/cloud-autoscrub-finding-set/v1";
const GRANT_DOMAIN: &[u8] = b"OSL/cloud-autoscrub-grant/v1";
const SCOPE_DOMAIN: &[u8] = b"OSL/cloud-autoscrub-scope/v1";
const EXECUTION_CONSENT_DOMAIN: &[u8] = b"OSL/execution-consent-scope/v1";
const MAX_SCOPE_BINDING_BYTES: usize = 256;
const MAX_EXPLICIT_SCOPE_CONSENTS: usize = 128;
const HIGH_SENSITIVITY_REQUIRED_ACKNOWLEDGEMENT_COUNT: usize = 5;

pub const CLOUD_AUTOSCRUB_HIGH_SENSITIVITY_TITLE: &str = "Cloud AutoScrub needs separate consent";
pub const CLOUD_AUTOSCRUB_HIGH_SENSITIVITY_WARNING: &str =
    "OSL temporarily receives sensitive credentials/data for this operation.";
pub const CLOUD_AUTOSCRUB_HIGH_SENSITIVITY_NOT_PRIVATE_WARNING: &str =
    "This operation is not fully end-to-end private.";
pub const CLOUD_AUTOSCRUB_HIGH_SENSITIVITY_RETENTION_WARNING: &str =
    "Temporary data is minimized, isolated, expired, wiped after completion, and followed by a deletion receipt.";
pub const CLOUD_AUTOSCRUB_HIGH_SENSITIVITY_REVOCATION_WARNING: &str =
    "OSL recommends revoking temporary credentials after the run.";
pub const CLOUD_AUTOSCRUB_HIGH_SENSITIVITY_SCOPE_WARNING: &str =
    "Consent applies only to this reviewed cleanup area and the findings shown.";

#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CloudAutoScrubScope {
    commitment: [u8; 32],
    account_commitment: [u8; 32],
    data_scope_commitment: [u8; 32],
}

impl CloudAutoScrubScope {
    pub fn derive(
        owner_binding: &[u8],
        service_binding: &[u8],
        account_binding: &[u8],
        data_scope_binding: &[u8],
    ) -> Result<Self, CloudAutoScrubConsentError> {
        validate_scope_binding(owner_binding)?;
        validate_scope_binding(service_binding)?;
        validate_scope_binding(account_binding)?;
        validate_scope_binding(data_scope_binding)?;

        let mut bytes = Vec::new();
        bytes.extend_from_slice(SCOPE_DOMAIN);
        write_lp(&mut bytes, owner_binding)?;
        write_lp(&mut bytes, service_binding)?;
        write_lp(&mut bytes, account_binding)?;
        write_lp(&mut bytes, data_scope_binding)?;
        Ok(Self {
            commitment: Sha256::digest(bytes).into(),
            account_commitment: binding_commitment(ACCOUNT_DOMAIN, account_binding)?,
            data_scope_commitment: binding_commitment(DATA_SCOPE_DOMAIN, data_scope_binding)?,
        })
    }

    pub fn commitment(&self) -> [u8; 32] {
        self.commitment
    }

    pub fn account_commitment(&self) -> [u8; 32] {
        self.account_commitment
    }

    pub fn data_scope_commitment(&self) -> [u8; 32] {
        self.data_scope_commitment
    }
}

impl fmt::Debug for CloudAutoScrubScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CloudAutoScrubScope")
            .field("commitment", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CloudAutoScrubSecurityReviewGate {
    Unmet,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CloudAutoScrubScopeConsent {
    Explicit,
    Refused,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct CloudAutoScrubHighSensitivityConsentPrompt {
    scope: CloudAutoScrubScope,
    finding_set: CloudAutoScrubFindingSet,
}

impl CloudAutoScrubHighSensitivityConsentPrompt {
    pub fn new(scope: CloudAutoScrubScope, finding_set: CloudAutoScrubFindingSet) -> Self {
        Self { scope, finding_set }
    }

    pub fn title(&self) -> &'static str {
        CLOUD_AUTOSCRUB_HIGH_SENSITIVITY_TITLE
    }

    pub fn warnings(&self) -> [&'static str; HIGH_SENSITIVITY_REQUIRED_ACKNOWLEDGEMENT_COUNT] {
        [
            CLOUD_AUTOSCRUB_HIGH_SENSITIVITY_WARNING,
            CLOUD_AUTOSCRUB_HIGH_SENSITIVITY_NOT_PRIVATE_WARNING,
            CLOUD_AUTOSCRUB_HIGH_SENSITIVITY_RETENTION_WARNING,
            CLOUD_AUTOSCRUB_HIGH_SENSITIVITY_REVOCATION_WARNING,
            CLOUD_AUTOSCRUB_HIGH_SENSITIVITY_SCOPE_WARNING,
        ]
    }

    pub fn required_acknowledgement_count(&self) -> usize {
        HIGH_SENSITIVITY_REQUIRED_ACKNOWLEDGEMENT_COUNT
    }

    pub fn scope_commitment(&self) -> [u8; 32] {
        self.scope.commitment()
    }

    pub fn finding_set_commitment(&self) -> [u8; 32] {
        self.finding_set.commitment()
    }
}

impl fmt::Debug for CloudAutoScrubHighSensitivityConsentPrompt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CloudAutoScrubHighSensitivityConsentPrompt")
            .field("scope_commitment", &"<redacted>")
            .field("finding_set_commitment", &"<redacted>")
            .field(
                "required_acknowledgement_count",
                &HIGH_SENSITIVITY_REQUIRED_ACKNOWLEDGEMENT_COUNT,
            )
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CloudAutoScrubHighSensitivityAcknowledgement {
    pub sensitive_credentials_or_data_leave_device: bool,
    pub not_fully_end_to_end_private: bool,
    pub retention_wipe_and_receipt: bool,
    pub revoke_temporary_credentials_after_run: bool,
    pub exact_scope_and_findings_only: bool,
}

impl CloudAutoScrubHighSensitivityAcknowledgement {
    pub const fn all() -> Self {
        Self {
            sensitive_credentials_or_data_leave_device: true,
            not_fully_end_to_end_private: true,
            retention_wipe_and_receipt: true,
            revoke_temporary_credentials_after_run: true,
            exact_scope_and_findings_only: true,
        }
    }

    pub fn all_required_acknowledged(&self) -> bool {
        self.sensitive_credentials_or_data_leave_device
            && self.not_fully_end_to_end_private
            && self.retention_wipe_and_receipt
            && self.revoke_temporary_credentials_after_run
            && self.exact_scope_and_findings_only
    }
}

#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CloudAutoScrubFindingSet {
    commitment: [u8; 32],
}

impl CloudAutoScrubFindingSet {
    pub fn derive(bindings: &[&[u8]]) -> Result<Self, CloudAutoScrubConsentError> {
        if bindings.is_empty() || bindings.len() > MAX_EXPLICIT_SCOPE_CONSENTS {
            return Err(CloudAutoScrubConsentError::InvalidFindingSet);
        }

        let mut bytes = Vec::new();
        bytes.extend_from_slice(FINDING_DOMAIN);
        for binding in bindings {
            validate_scope_binding(binding)?;
            write_lp(&mut bytes, binding)?;
        }
        Ok(Self {
            commitment: Sha256::digest(bytes).into(),
        })
    }

    pub fn commitment(&self) -> [u8; 32] {
        self.commitment
    }
}

impl fmt::Debug for CloudAutoScrubFindingSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CloudAutoScrubFindingSet")
            .field("commitment", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExecutionScopeFingerprint {
    commitment: [u8; 32],
}

impl ExecutionScopeFingerprint {
    pub fn derive(
        provider_binding: &[u8],
        account_binding: &[u8],
        execution_scope_binding: &[u8],
        finding_category_bindings: &[&[u8]],
    ) -> Result<Self, CloudAutoScrubConsentError> {
        validate_scope_binding(provider_binding)?;
        validate_scope_binding(account_binding)?;
        validate_scope_binding(execution_scope_binding)?;
        if finding_category_bindings.is_empty()
            || finding_category_bindings.len() > MAX_EXPLICIT_SCOPE_CONSENTS
        {
            return Err(CloudAutoScrubConsentError::InvalidFindingSet);
        }

        let mut findings = finding_category_bindings.to_vec();
        findings.sort();
        if findings.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(CloudAutoScrubConsentError::InvalidFindingSet);
        }

        let mut bytes = Vec::new();
        bytes.extend_from_slice(EXECUTION_CONSENT_DOMAIN);
        write_lp(&mut bytes, provider_binding)?;
        write_lp(&mut bytes, account_binding)?;
        write_lp(&mut bytes, execution_scope_binding)?;
        let finding_count = u32::try_from(findings.len())
            .map_err(|_| CloudAutoScrubConsentError::InvalidFindingSet)?;
        bytes.extend_from_slice(&finding_count.to_be_bytes());
        for finding in findings {
            validate_scope_binding(finding)?;
            write_lp(&mut bytes, finding)?;
        }
        Ok(Self {
            commitment: Sha256::digest(bytes).into(),
        })
    }

    pub fn commitment(&self) -> [u8; 32] {
        self.commitment
    }
}

impl fmt::Debug for ExecutionScopeFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExecutionScopeFingerprint")
            .field("commitment", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ExecutionConsent {
    scope_fingerprint: ExecutionScopeFingerprint,
    auth_epoch: u64,
}

impl ExecutionConsent {
    pub fn grant(
        scope_fingerprint: ExecutionScopeFingerprint,
        auth_epoch: u64,
    ) -> Result<Self, CloudAutoScrubConsentError> {
        if auth_epoch == 0 {
            return Err(CloudAutoScrubConsentError::LiveAuthEpochRequired);
        }
        Ok(Self {
            scope_fingerprint,
            auth_epoch,
        })
    }

    pub fn require(
        &self,
        scope_fingerprint: ExecutionScopeFingerprint,
        auth_epoch: u64,
    ) -> Result<(), CloudAutoScrubConsentError> {
        if auth_epoch == 0 || self.auth_epoch != auth_epoch {
            return Err(CloudAutoScrubConsentError::LiveAuthEpochRequired);
        }
        if self.scope_fingerprint != scope_fingerprint {
            return Err(CloudAutoScrubConsentError::ExplicitScopeConsentRequired);
        }
        Ok(())
    }

    pub fn auth_epoch(&self) -> u64 {
        self.auth_epoch
    }

    pub fn scope_fingerprint(&self) -> ExecutionScopeFingerprint {
        self.scope_fingerprint
    }
}

impl fmt::Debug for ExecutionConsent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExecutionConsent")
            .field("scope_fingerprint", &"<redacted>")
            .field("auth_epoch", &self.auth_epoch)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CloudAutoScrubAuthority {
    Capability,
    Run,
    Attended,
}

#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct CloudAutoScrubGrantKey {
    scope: CloudAutoScrubScope,
    finding_set: CloudAutoScrubFindingSet,
    authority: CloudAutoScrubAuthority,
}

impl CloudAutoScrubGrantKey {
    fn new(
        scope: CloudAutoScrubScope,
        finding_set: CloudAutoScrubFindingSet,
        authority: CloudAutoScrubAuthority,
    ) -> Self {
        Self {
            scope,
            finding_set,
            authority,
        }
    }

    fn commitment(&self) -> [u8; 32] {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(GRANT_DOMAIN);
        bytes.extend_from_slice(&self.scope.commitment);
        bytes.extend_from_slice(&self.finding_set.commitment);
        bytes.push(match self.authority {
            CloudAutoScrubAuthority::Capability => 1,
            CloudAutoScrubAuthority::Run => 2,
            CloudAutoScrubAuthority::Attended => 3,
        });
        Sha256::digest(bytes).into()
    }
}

impl fmt::Debug for CloudAutoScrubGrantKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CloudAutoScrubGrantKey")
            .field("commitment", &"<redacted>")
            .field("authority", &self.authority)
            .finish()
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct CloudAutoScrubCapability {
    grant_commitment: [u8; 32],
    auth_epoch: u64,
}

impl CloudAutoScrubCapability {
    pub fn auth_epoch(&self) -> u64 {
        self.auth_epoch
    }

    pub fn grant_commitment(&self) -> [u8; 32] {
        self.grant_commitment
    }
}

impl fmt::Debug for CloudAutoScrubCapability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CloudAutoScrubCapability")
            .field("grant_commitment", &"<redacted>")
            .field("auth_epoch", &self.auth_epoch)
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct CloudAutoScrubConsentTier {
    explicit_scope_consents: BTreeSet<CloudAutoScrubScope>,
    security_review_gate: CloudAutoScrubSecurityReviewGate,
    live_auth_epoch: Option<u64>,
    high_sensitivity_acknowledgements: BTreeSet<CloudAutoScrubGrantKey>,
    capability_grants: BTreeSet<CloudAutoScrubGrantKey>,
    run_grants: BTreeSet<CloudAutoScrubGrantKey>,
    attended_grants: BTreeSet<CloudAutoScrubGrantKey>,
}

impl Default for CloudAutoScrubConsentTier {
    fn default() -> Self {
        Self {
            explicit_scope_consents: BTreeSet::new(),
            security_review_gate: CloudAutoScrubSecurityReviewGate::Unmet,
            live_auth_epoch: None,
            high_sensitivity_acknowledgements: BTreeSet::new(),
            capability_grants: BTreeSet::new(),
            run_grants: BTreeSet::new(),
            attended_grants: BTreeSet::new(),
        }
    }
}

impl fmt::Debug for CloudAutoScrubConsentTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CloudAutoScrubConsentTier")
            .field(
                "explicit_scope_consent_count",
                &self.explicit_scope_consents.len(),
            )
            .field("security_review_gate", &self.security_review_gate)
            .field("live_auth_epoch", &self.live_auth_epoch)
            .field(
                "high_sensitivity_acknowledgement_count",
                &self.high_sensitivity_acknowledgements.len(),
            )
            .field("capability_grant_count", &self.capability_grants.len())
            .field("run_grant_count", &self.run_grants.len())
            .field("attended_grant_count", &self.attended_grants.len())
            .finish()
    }
}

impl CloudAutoScrubConsentTier {
    pub fn security_review_gate(&self) -> CloudAutoScrubSecurityReviewGate {
        self.security_review_gate
    }

    pub fn explicit_scope_consent_count(&self) -> usize {
        self.explicit_scope_consents.len()
    }

    pub fn high_sensitivity_acknowledgement_count(&self) -> usize {
        self.high_sensitivity_acknowledgements.len()
    }

    pub fn live_auth_epoch(&self) -> Option<u64> {
        self.live_auth_epoch
    }

    pub fn high_sensitivity_consent_prompt(
        &self,
        scope: CloudAutoScrubScope,
        finding_set: CloudAutoScrubFindingSet,
    ) -> CloudAutoScrubHighSensitivityConsentPrompt {
        CloudAutoScrubHighSensitivityConsentPrompt::new(scope, finding_set)
    }

    pub fn consent_for_scope(&self, scope: CloudAutoScrubScope) -> CloudAutoScrubScopeConsent {
        if self.explicit_scope_consents.contains(&scope) {
            CloudAutoScrubScopeConsent::Explicit
        } else {
            CloudAutoScrubScopeConsent::Refused
        }
    }

    pub fn record_explicit_scope_consent(
        &mut self,
        scope: CloudAutoScrubScope,
    ) -> Result<bool, CloudAutoScrubConsentError> {
        if !self.explicit_scope_consents.contains(&scope)
            && self.explicit_scope_consents.len() >= MAX_EXPLICIT_SCOPE_CONSENTS
        {
            return Err(CloudAutoScrubConsentError::TooManyScopes);
        }
        Ok(self.explicit_scope_consents.insert(scope))
    }

    pub fn revoke_scope_consent(&mut self, scope: CloudAutoScrubScope) -> bool {
        let removed_consent = self.explicit_scope_consents.remove(&scope);
        let removed_grants = self.revoke_scope(scope);
        removed_consent || removed_grants
    }

    pub fn record_high_sensitivity_acknowledgement(
        &mut self,
        scope: CloudAutoScrubScope,
        finding_set: CloudAutoScrubFindingSet,
        acknowledgement: CloudAutoScrubHighSensitivityAcknowledgement,
    ) -> Result<bool, CloudAutoScrubConsentError> {
        if !acknowledgement.all_required_acknowledged() {
            return Err(CloudAutoScrubConsentError::HighSensitivityAcknowledgementRequired);
        }
        self.record_explicit_scope_consent(scope)?;
        Ok(self
            .high_sensitivity_acknowledgements
            .insert(CloudAutoScrubGrantKey::new(
                scope,
                finding_set,
                CloudAutoScrubAuthority::Capability,
            )))
    }

    pub fn require_high_sensitivity_acknowledgement(
        &self,
        scope: CloudAutoScrubScope,
        finding_set: CloudAutoScrubFindingSet,
    ) -> Result<(), CloudAutoScrubConsentError> {
        if self.consent_for_scope(scope) == CloudAutoScrubScopeConsent::Explicit
            && self
                .high_sensitivity_acknowledgements
                .contains(&CloudAutoScrubGrantKey::new(
                    scope,
                    finding_set,
                    CloudAutoScrubAuthority::Capability,
                ))
        {
            Ok(())
        } else {
            Err(CloudAutoScrubConsentError::HighSensitivityAcknowledgementRequired)
        }
    }

    pub fn require_cloud_processing_scope(
        &self,
        scope: CloudAutoScrubScope,
    ) -> Result<(), CloudAutoScrubConsentError> {
        if self.consent_for_scope(scope) != CloudAutoScrubScopeConsent::Explicit {
            return Err(CloudAutoScrubConsentError::ExplicitScopeConsentRequired);
        }
        match self.security_review_gate {
            CloudAutoScrubSecurityReviewGate::Unmet => {
                Err(CloudAutoScrubConsentError::ExternalSecurityReviewRequired)
            }
        }
    }

    pub fn reauthenticate(&mut self, auth_epoch: u64) -> Result<(), CloudAutoScrubConsentError> {
        if auth_epoch == 0 || self.live_auth_epoch.is_some_and(|live| auth_epoch <= live) {
            return Err(CloudAutoScrubConsentError::LiveAuthEpochRequired);
        }
        self.live_auth_epoch = Some(auth_epoch);
        Ok(())
    }

    pub fn require_live_auth_epoch(
        &self,
        auth_epoch: u64,
    ) -> Result<(), CloudAutoScrubConsentError> {
        if auth_epoch != 0 && self.live_auth_epoch == Some(auth_epoch) {
            Ok(())
        } else {
            Err(CloudAutoScrubConsentError::LiveAuthEpochRequired)
        }
    }

    pub fn configure_cloud_autoscrub_capability(
        &mut self,
        scope: CloudAutoScrubScope,
        finding_set: CloudAutoScrubFindingSet,
    ) -> Result<CloudAutoScrubCapability, CloudAutoScrubConsentError> {
        let auth_epoch = self
            .live_auth_epoch
            .ok_or(CloudAutoScrubConsentError::LiveAuthEpochRequired)?;
        self.require_high_sensitivity_acknowledgement(scope, finding_set)?;
        let grant =
            CloudAutoScrubGrantKey::new(scope, finding_set, CloudAutoScrubAuthority::Capability);
        self.capability_grants.insert(grant);
        Ok(CloudAutoScrubCapability {
            grant_commitment: grant.commitment(),
            auth_epoch,
        })
    }

    pub fn require_cloud_autoscrub_capability(
        &self,
        capability: CloudAutoScrubCapability,
        scope: CloudAutoScrubScope,
        finding_set: CloudAutoScrubFindingSet,
        auth_epoch: u64,
    ) -> Result<(), CloudAutoScrubConsentError> {
        self.require_live_auth_epoch(auth_epoch)?;
        if capability.auth_epoch != auth_epoch {
            return Err(CloudAutoScrubConsentError::LiveAuthEpochRequired);
        }
        let grant =
            CloudAutoScrubGrantKey::new(scope, finding_set, CloudAutoScrubAuthority::Capability);
        if capability.grant_commitment != grant.commitment()
            || !self.capability_grants.contains(&grant)
            || self.consent_for_scope(scope) != CloudAutoScrubScopeConsent::Explicit
        {
            return Err(CloudAutoScrubConsentError::CapabilityRequired);
        }
        Ok(())
    }

    pub fn grant_run_authority(
        &mut self,
        capability: CloudAutoScrubCapability,
        scope: CloudAutoScrubScope,
        finding_set: CloudAutoScrubFindingSet,
        auth_epoch: u64,
    ) -> Result<(), CloudAutoScrubConsentError> {
        self.require_cloud_autoscrub_capability(capability, scope, finding_set, auth_epoch)?;
        self.run_grants.insert(CloudAutoScrubGrantKey::new(
            scope,
            finding_set,
            CloudAutoScrubAuthority::Run,
        ));
        Ok(())
    }

    pub fn grant_attended_authority(
        &mut self,
        capability: CloudAutoScrubCapability,
        scope: CloudAutoScrubScope,
        finding_set: CloudAutoScrubFindingSet,
        auth_epoch: u64,
    ) -> Result<(), CloudAutoScrubConsentError> {
        self.require_cloud_autoscrub_capability(capability, scope, finding_set, auth_epoch)?;
        self.attended_grants.insert(CloudAutoScrubGrantKey::new(
            scope,
            finding_set,
            CloudAutoScrubAuthority::Attended,
        ));
        Ok(())
    }

    pub fn require_run_authority(
        &self,
        scope: CloudAutoScrubScope,
        finding_set: CloudAutoScrubFindingSet,
        auth_epoch: u64,
    ) -> Result<(), CloudAutoScrubConsentError> {
        self.require_authority(scope, finding_set, auth_epoch, CloudAutoScrubAuthority::Run)
    }

    pub fn require_attended_authority(
        &self,
        scope: CloudAutoScrubScope,
        finding_set: CloudAutoScrubFindingSet,
        auth_epoch: u64,
    ) -> Result<(), CloudAutoScrubConsentError> {
        self.require_authority(
            scope,
            finding_set,
            auth_epoch,
            CloudAutoScrubAuthority::Attended,
        )
    }

    pub fn revoke_scope(&mut self, scope: CloudAutoScrubScope) -> bool {
        let before = (
            self.high_sensitivity_acknowledgements.len(),
            self.capability_grants.len(),
            self.run_grants.len(),
            self.attended_grants.len(),
        );
        self.high_sensitivity_acknowledgements
            .retain(|grant| grant.scope != scope);
        self.capability_grants.retain(|grant| grant.scope != scope);
        self.run_grants.retain(|grant| grant.scope != scope);
        self.attended_grants.retain(|grant| grant.scope != scope);
        before
            != (
                self.high_sensitivity_acknowledgements.len(),
                self.capability_grants.len(),
                self.run_grants.len(),
                self.attended_grants.len(),
            )
    }

    pub fn revoke_account(
        &mut self,
        account_binding: &[u8],
    ) -> Result<bool, CloudAutoScrubConsentError> {
        let account_commitment = binding_commitment(ACCOUNT_DOMAIN, account_binding)?;
        let mut removed = false;
        let consent_count = self.explicit_scope_consents.len();
        self.explicit_scope_consents
            .retain(|scope| scope.account_commitment != account_commitment);
        removed |= self.explicit_scope_consents.len() != consent_count;

        for grants in [
            &mut self.high_sensitivity_acknowledgements,
            &mut self.capability_grants,
            &mut self.run_grants,
            &mut self.attended_grants,
        ] {
            let count = grants.len();
            grants.retain(|grant| grant.scope.account_commitment != account_commitment);
            removed |= grants.len() != count;
        }
        Ok(removed)
    }

    pub fn revoke_all(&mut self) -> bool {
        let removed = !self.explicit_scope_consents.is_empty()
            || !self.high_sensitivity_acknowledgements.is_empty()
            || !self.capability_grants.is_empty()
            || !self.run_grants.is_empty()
            || !self.attended_grants.is_empty();
        self.explicit_scope_consents.clear();
        self.high_sensitivity_acknowledgements.clear();
        self.capability_grants.clear();
        self.run_grants.clear();
        self.attended_grants.clear();
        removed
    }

    pub fn revoke_run_authority(
        &mut self,
        scope: CloudAutoScrubScope,
        finding_set: CloudAutoScrubFindingSet,
    ) -> bool {
        let before = self.run_grants.len();
        self.run_grants.retain(|grant| {
            !(grant.scope == scope
                && grant.finding_set == finding_set
                && grant.authority == CloudAutoScrubAuthority::Run)
        });
        self.run_grants.len() != before
    }

    pub fn revoke_attended_authority(
        &mut self,
        scope: CloudAutoScrubScope,
        finding_set: CloudAutoScrubFindingSet,
    ) -> bool {
        let before = self.attended_grants.len();
        self.attended_grants.retain(|grant| {
            !(grant.scope == scope
                && grant.finding_set == finding_set
                && grant.authority == CloudAutoScrubAuthority::Attended)
        });
        self.attended_grants.len() != before
    }

    fn require_authority(
        &self,
        scope: CloudAutoScrubScope,
        finding_set: CloudAutoScrubFindingSet,
        auth_epoch: u64,
        authority: CloudAutoScrubAuthority,
    ) -> Result<(), CloudAutoScrubConsentError> {
        self.require_live_auth_epoch(auth_epoch)?;
        let grant = CloudAutoScrubGrantKey::new(scope, finding_set, authority);
        let grants = match authority {
            CloudAutoScrubAuthority::Capability => &self.capability_grants,
            CloudAutoScrubAuthority::Run => &self.run_grants,
            CloudAutoScrubAuthority::Attended => &self.attended_grants,
        };
        if grants.contains(&grant)
            && self.consent_for_scope(scope) == CloudAutoScrubScopeConsent::Explicit
        {
            Ok(())
        } else {
            Err(CloudAutoScrubConsentError::AuthorityRequired)
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CloudAutoScrubConsentError {
    InvalidScope,
    InvalidFindingSet,
    TooManyScopes,
    ExplicitScopeConsentRequired,
    HighSensitivityAcknowledgementRequired,
    ExternalSecurityReviewRequired,
    LiveAuthEpochRequired,
    CapabilityRequired,
    AuthorityRequired,
}

impl fmt::Display for CloudAutoScrubConsentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidScope => "cloud AutoScrub scope is invalid",
            Self::InvalidFindingSet => "cloud AutoScrub finding set is invalid",
            Self::TooManyScopes => "cloud AutoScrub has too many explicit scope consents",
            Self::ExplicitScopeConsentRequired => {
                "explicit per-scope cloud AutoScrub consent is required"
            }
            Self::HighSensitivityAcknowledgementRequired => {
                "explicit cloud AutoScrub high-sensitivity acknowledgement is required"
            }
            Self::ExternalSecurityReviewRequired => {
                "external cloud AutoScrub security review is required"
            }
            Self::LiveAuthEpochRequired => "a live account authorization epoch is required",
            Self::CapabilityRequired => "cloud AutoScrub capability is required",
            Self::AuthorityRequired => "cloud AutoScrub authority is required",
        };
        f.write_str(message)
    }
}

impl std::error::Error for CloudAutoScrubConsentError {}

fn validate_scope_binding(value: &[u8]) -> Result<(), CloudAutoScrubConsentError> {
    if value.is_empty() || value.len() > MAX_SCOPE_BINDING_BYTES {
        Err(CloudAutoScrubConsentError::InvalidScope)
    } else {
        Ok(())
    }
}

fn binding_commitment(domain: &[u8], value: &[u8]) -> Result<[u8; 32], CloudAutoScrubConsentError> {
    validate_scope_binding(value)?;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(domain);
    write_lp(&mut bytes, value)?;
    Ok(Sha256::digest(bytes).into())
}

fn write_lp(target: &mut Vec<u8>, value: &[u8]) -> Result<(), CloudAutoScrubConsentError> {
    let length =
        u32::try_from(value.len()).map_err(|_| CloudAutoScrubConsentError::InvalidScope)?;
    target.extend_from_slice(&length.to_be_bytes());
    target.extend_from_slice(value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(account: &[u8], data_scope: &[u8]) -> CloudAutoScrubScope {
        CloudAutoScrubScope::derive(b"owner-binding", b"discord", account, data_scope).unwrap()
    }

    fn findings(values: &[&str]) -> CloudAutoScrubFindingSet {
        let bindings = values
            .iter()
            .map(|value| value.as_bytes())
            .collect::<Vec<_>>();
        CloudAutoScrubFindingSet::derive(&bindings).unwrap()
    }

    fn acknowledge_high_sensitivity(
        tier: &mut CloudAutoScrubConsentTier,
        target: CloudAutoScrubScope,
        finding_set: CloudAutoScrubFindingSet,
    ) {
        assert_eq!(
            tier.record_high_sensitivity_acknowledgement(
                target,
                finding_set,
                CloudAutoScrubHighSensitivityAcknowledgement::all(),
            ),
            Ok(true)
        );
    }

    fn execution_scope(
        provider: &[u8],
        account: &[u8],
        scope: &[u8],
        findings: &[&[u8]],
    ) -> ExecutionScopeFingerprint {
        ExecutionScopeFingerprint::derive(provider, account, scope, findings).unwrap()
    }

    #[test]
    fn execution_consent_scope_fingerprint_contract() {
        let granted = execution_scope(
            b"provider-local",
            b"account-private",
            b"visible-data",
            &[b"credential".as_ref(), b"precise-location".as_ref()],
        );
        let reordered = execution_scope(
            b"provider-local",
            b"account-private",
            b"visible-data",
            &[b"precise-location".as_ref(), b"credential".as_ref()],
        );
        let other_account = execution_scope(
            b"provider-local",
            b"account-other",
            b"visible-data",
            &[b"credential".as_ref(), b"precise-location".as_ref()],
        );
        let other_scope = execution_scope(
            b"provider-local",
            b"account-private",
            b"all-data",
            &[b"credential".as_ref(), b"precise-location".as_ref()],
        );

        assert_eq!(granted, reordered);
        assert_ne!(granted, other_account);
        assert_ne!(granted, other_scope);

        let consent = ExecutionConsent::grant(granted, 42).unwrap();
        assert_eq!(consent.scope_fingerprint(), granted);
        assert_eq!(consent.auth_epoch(), 42);
        assert_eq!(consent.require(reordered, 42), Ok(()));
        assert_eq!(
            consent.require(other_account, 42),
            Err(CloudAutoScrubConsentError::ExplicitScopeConsentRequired)
        );
        assert_eq!(
            consent.require(granted, 41),
            Err(CloudAutoScrubConsentError::LiveAuthEpochRequired)
        );
        assert_eq!(
            ExecutionConsent::grant(granted, 0),
            Err(CloudAutoScrubConsentError::LiveAuthEpochRequired)
        );
        assert_eq!(
            ExecutionScopeFingerprint::derive(
                b"provider-local",
                b"account-private",
                b"visible-data",
                &[b"credential".as_ref(), b"credential".as_ref()],
            ),
            Err(CloudAutoScrubConsentError::InvalidFindingSet)
        );

        let debug = format!("{:?}", consent);
        assert!(!debug.contains("provider-local"));
        assert!(!debug.contains("account-private"));
        assert!(!debug.contains("visible-data"));
    }

    #[test]
    fn default_denies_absent_scope() {
        let tier = CloudAutoScrubConsentTier::default();
        let target = scope(b"account-one", b"osl-visible-data");

        assert_eq!(
            tier.security_review_gate(),
            CloudAutoScrubSecurityReviewGate::Unmet
        );
        assert_eq!(tier.explicit_scope_consent_count(), 0);
        assert_eq!(
            tier.consent_for_scope(target),
            CloudAutoScrubScopeConsent::Refused
        );
        assert_eq!(
            tier.require_cloud_processing_scope(target),
            Err(CloudAutoScrubConsentError::ExplicitScopeConsentRequired)
        );
    }

    #[test]
    fn consent_for_one_scope_does_not_cover_another() {
        let mut tier = CloudAutoScrubConsentTier::default();
        let granted = scope(b"account-one", b"osl-visible-data");
        let other_account = scope(b"account-two", b"osl-visible-data");
        let other_data_scope = scope(b"account-one", b"manual-export");

        assert_eq!(tier.record_explicit_scope_consent(granted), Ok(true));

        assert_eq!(
            tier.consent_for_scope(granted),
            CloudAutoScrubScopeConsent::Explicit
        );
        assert_eq!(
            tier.consent_for_scope(other_account),
            CloudAutoScrubScopeConsent::Refused
        );
        assert_eq!(
            tier.consent_for_scope(other_data_scope),
            CloudAutoScrubScopeConsent::Refused
        );
    }

    #[test]
    fn unmet_review_gate_blocks_enablement() {
        let mut tier = CloudAutoScrubConsentTier::default();
        let target = scope(b"account-one", b"osl-visible-data");

        tier.record_explicit_scope_consent(target).unwrap();

        assert_eq!(
            tier.require_cloud_processing_scope(target),
            Err(CloudAutoScrubConsentError::ExternalSecurityReviewRequired)
        );
    }

    #[test]
    fn debug_and_display_do_not_render_account_identifiers() {
        let mut tier = CloudAutoScrubConsentTier::default();
        let target = scope(b"secret-account-identifier", b"osl-visible-data");
        tier.record_explicit_scope_consent(target).unwrap();

        let scope_debug = format!("{:?}", target);
        let tier_debug = format!("{:?}", tier);
        let error_debug = format!(
            "{:?}",
            CloudAutoScrubConsentError::ExternalSecurityReviewRequired
        );
        let error_display = CloudAutoScrubConsentError::ExternalSecurityReviewRequired.to_string();

        for rendered in [scope_debug, tier_debug, error_debug, error_display] {
            assert!(!rendered.contains("secret-account-identifier"));
            assert!(!rendered.contains("discord"));
        }
    }

    #[test]
    fn high_sensitivity_consent_requires_explicit_acknowledgement() {
        let mut tier = CloudAutoScrubConsentTier::default();
        let target = scope(b"account-one", b"hosted-scan-only");
        let finding_set = findings(&["credential", "precise-location"]);
        let prompt = tier.high_sensitivity_consent_prompt(target, finding_set);

        assert_eq!(prompt.title(), CLOUD_AUTOSCRUB_HIGH_SENSITIVITY_TITLE);
        assert_eq!(prompt.required_acknowledgement_count(), 5);
        assert!(prompt
            .warnings()
            .contains(&CLOUD_AUTOSCRUB_HIGH_SENSITIVITY_WARNING));
        assert!(prompt
            .warnings()
            .contains(&CLOUD_AUTOSCRUB_HIGH_SENSITIVITY_NOT_PRIVATE_WARNING));
        assert!(prompt
            .warnings()
            .contains(&CLOUD_AUTOSCRUB_HIGH_SENSITIVITY_RETENTION_WARNING));
        assert!(prompt
            .warnings()
            .contains(&CLOUD_AUTOSCRUB_HIGH_SENSITIVITY_REVOCATION_WARNING));
        assert!(prompt
            .warnings()
            .contains(&CLOUD_AUTOSCRUB_HIGH_SENSITIVITY_SCOPE_WARNING));
        assert!(!format!("{:?}", prompt).contains("account-one"));

        assert_eq!(
            tier.record_high_sensitivity_acknowledgement(
                target,
                finding_set,
                CloudAutoScrubHighSensitivityAcknowledgement {
                    not_fully_end_to_end_private: true,
                    ..CloudAutoScrubHighSensitivityAcknowledgement::default()
                },
            ),
            Err(CloudAutoScrubConsentError::HighSensitivityAcknowledgementRequired)
        );
        assert_eq!(
            tier.require_high_sensitivity_acknowledgement(target, finding_set),
            Err(CloudAutoScrubConsentError::HighSensitivityAcknowledgementRequired)
        );

        tier.reauthenticate(42).unwrap();
        tier.record_explicit_scope_consent(target).unwrap();
        assert_eq!(
            tier.configure_cloud_autoscrub_capability(target, finding_set),
            Err(CloudAutoScrubConsentError::HighSensitivityAcknowledgementRequired)
        );

        assert_eq!(
            tier.record_high_sensitivity_acknowledgement(
                target,
                finding_set,
                CloudAutoScrubHighSensitivityAcknowledgement::all(),
            ),
            Ok(true)
        );
        let capability = tier
            .configure_cloud_autoscrub_capability(target, finding_set)
            .unwrap();
        assert_eq!(
            tier.require_cloud_autoscrub_capability(capability, target, finding_set, 42),
            Ok(())
        );
    }

    #[test]
    fn cloud_autoscrub_configure_capability_requires_live_auth_epoch_and_reauthenticates() {
        let mut tier = CloudAutoScrubConsentTier::default();
        let target = scope(b"account-one", b"osl-visible-data");
        let finding_set = findings(&["precise-location", "payment-card"]);

        assert_eq!(
            tier.configure_cloud_autoscrub_capability(target, finding_set),
            Err(CloudAutoScrubConsentError::LiveAuthEpochRequired)
        );

        tier.reauthenticate(1).unwrap();
        acknowledge_high_sensitivity(&mut tier, target, finding_set);
        let capability = tier
            .configure_cloud_autoscrub_capability(target, finding_set)
            .unwrap();

        assert_eq!(capability.auth_epoch(), 1);
        assert_eq!(
            tier.require_cloud_autoscrub_capability(capability, target, finding_set, 1),
            Ok(())
        );
        assert_eq!(
            tier.require_cloud_autoscrub_capability(capability, target, finding_set, 2),
            Err(CloudAutoScrubConsentError::LiveAuthEpochRequired)
        );

        tier.reauthenticate(2).unwrap();
        assert_eq!(
            tier.require_cloud_autoscrub_capability(capability, target, finding_set, 2),
            Err(CloudAutoScrubConsentError::LiveAuthEpochRequired)
        );

        assert_eq!(
            tier.record_high_sensitivity_acknowledgement(
                target,
                finding_set,
                CloudAutoScrubHighSensitivityAcknowledgement::all(),
            ),
            Ok(false)
        );
        let renewed = tier
            .configure_cloud_autoscrub_capability(target, finding_set)
            .unwrap();
        assert_eq!(renewed.auth_epoch(), 2);
        assert_eq!(
            tier.require_cloud_autoscrub_capability(renewed, target, finding_set, 2),
            Ok(())
        );
        assert_eq!(
            tier.reauthenticate(2),
            Err(CloudAutoScrubConsentError::LiveAuthEpochRequired)
        );
    }

    #[test]
    fn cloud_autoscrub_revoke_scope_account_all_run_and_attended_authority() {
        let mut tier = CloudAutoScrubConsentTier::default();
        tier.reauthenticate(1).unwrap();
        let target = scope(b"account-one", b"osl-visible-data");
        let same_account_other_scope = scope(b"account-one", b"manual-export");
        let other_account = scope(b"account-two", b"osl-visible-data");
        let finding_set = findings(&["work-secret"]);

        acknowledge_high_sensitivity(&mut tier, target, finding_set);
        let capability = tier
            .configure_cloud_autoscrub_capability(target, finding_set)
            .unwrap();
        tier.grant_run_authority(capability, target, finding_set, 1)
            .unwrap();
        tier.grant_attended_authority(capability, target, finding_set, 1)
            .unwrap();
        assert_eq!(tier.require_run_authority(target, finding_set, 1), Ok(()));
        assert_eq!(
            tier.require_attended_authority(target, finding_set, 1),
            Ok(())
        );

        assert!(tier.revoke_run_authority(target, finding_set));
        assert_eq!(
            tier.require_run_authority(target, finding_set, 1),
            Err(CloudAutoScrubConsentError::AuthorityRequired)
        );
        assert_eq!(
            tier.require_attended_authority(target, finding_set, 1),
            Ok(())
        );

        assert!(tier.revoke_attended_authority(target, finding_set));
        assert_eq!(
            tier.require_attended_authority(target, finding_set, 1),
            Err(CloudAutoScrubConsentError::AuthorityRequired)
        );

        tier.grant_run_authority(capability, target, finding_set, 1)
            .unwrap();
        tier.grant_attended_authority(capability, target, finding_set, 1)
            .unwrap();
        assert!(tier.revoke_scope(target));
        assert_eq!(
            tier.require_cloud_autoscrub_capability(capability, target, finding_set, 1),
            Err(CloudAutoScrubConsentError::CapabilityRequired)
        );

        acknowledge_high_sensitivity(&mut tier, same_account_other_scope, finding_set);
        let account_capability = tier
            .configure_cloud_autoscrub_capability(same_account_other_scope, finding_set)
            .unwrap();
        tier.grant_run_authority(account_capability, same_account_other_scope, finding_set, 1)
            .unwrap();
        assert!(tier.revoke_account(b"account-one").unwrap());
        assert_eq!(
            tier.consent_for_scope(same_account_other_scope),
            CloudAutoScrubScopeConsent::Refused
        );
        assert_eq!(
            tier.require_run_authority(same_account_other_scope, finding_set, 1),
            Err(CloudAutoScrubConsentError::AuthorityRequired)
        );

        acknowledge_high_sensitivity(&mut tier, other_account, finding_set);
        let other_capability = tier
            .configure_cloud_autoscrub_capability(other_account, finding_set)
            .unwrap();
        assert_eq!(
            tier.require_cloud_autoscrub_capability(
                other_capability,
                other_account,
                finding_set,
                1
            ),
            Ok(())
        );
        assert!(tier.revoke_all());
        assert_eq!(
            tier.require_cloud_autoscrub_capability(
                other_capability,
                other_account,
                finding_set,
                1
            ),
            Err(CloudAutoScrubConsentError::CapabilityRequired)
        );
        assert!(!tier.revoke_all());
    }

    #[test]
    fn cloud_autoscrub_consent_invalidates_on_scope_or_finding_change() {
        let mut tier = CloudAutoScrubConsentTier::default();
        tier.reauthenticate(7).unwrap();
        let granted_scope = scope(b"account-one", b"osl-visible-data");
        let changed_scope = scope(b"account-one", b"manual-export");
        let granted_findings = findings(&["precise-location", "credential"]);
        let changed_findings = findings(&["precise-location", "credential", "payment-card"]);

        acknowledge_high_sensitivity(&mut tier, granted_scope, granted_findings);
        let capability = tier
            .configure_cloud_autoscrub_capability(granted_scope, granted_findings)
            .unwrap();

        assert_eq!(
            tier.require_cloud_autoscrub_capability(capability, granted_scope, granted_findings, 7),
            Ok(())
        );
        assert_eq!(
            tier.require_cloud_autoscrub_capability(capability, changed_scope, granted_findings, 7),
            Err(CloudAutoScrubConsentError::CapabilityRequired)
        );
        assert_eq!(
            tier.require_cloud_autoscrub_capability(capability, granted_scope, changed_findings, 7),
            Err(CloudAutoScrubConsentError::CapabilityRequired)
        );
        assert_eq!(
            tier.consent_for_scope(changed_scope),
            CloudAutoScrubScopeConsent::Refused
        );
    }

    #[test]
    fn scan_only_and_generic_cloud_autoscrub_fixture() {
        let mut tier = CloudAutoScrubConsentTier::default();
        let scan_only_scope = scope(b"account-one", b"hosted-scan-only");
        let generic_scope = scope(b"account-one", b"generic-cloud-autoscrub");
        let scan_findings = findings(&["credential", "precise-location"]);
        let generic_findings = findings(&["payment-card"]);

        for target in [scan_only_scope, generic_scope] {
            assert_eq!(
                tier.require_cloud_processing_scope(target),
                Err(CloudAutoScrubConsentError::ExplicitScopeConsentRequired)
            );
            assert_eq!(
                tier.require_attended_authority(target, scan_findings, 9),
                Err(CloudAutoScrubConsentError::LiveAuthEpochRequired)
            );
            assert_eq!(
                tier.require_run_authority(target, generic_findings, 9),
                Err(CloudAutoScrubConsentError::LiveAuthEpochRequired)
            );
        }

        tier.reauthenticate(9).unwrap();
        acknowledge_high_sensitivity(&mut tier, scan_only_scope, scan_findings);
        let scan_capability = tier
            .configure_cloud_autoscrub_capability(scan_only_scope, scan_findings)
            .unwrap();
        tier.grant_attended_authority(scan_capability, scan_only_scope, scan_findings, 9)
            .unwrap();

        assert_eq!(
            tier.require_attended_authority(scan_only_scope, scan_findings, 9),
            Ok(())
        );
        assert_eq!(
            tier.require_run_authority(scan_only_scope, scan_findings, 9),
            Err(CloudAutoScrubConsentError::AuthorityRequired)
        );
        assert_eq!(
            tier.require_attended_authority(generic_scope, scan_findings, 9),
            Err(CloudAutoScrubConsentError::AuthorityRequired)
        );
        assert_eq!(
            tier.require_cloud_autoscrub_capability(
                scan_capability,
                generic_scope,
                scan_findings,
                9
            ),
            Err(CloudAutoScrubConsentError::CapabilityRequired)
        );

        acknowledge_high_sensitivity(&mut tier, generic_scope, generic_findings);
        let generic_capability = tier
            .configure_cloud_autoscrub_capability(generic_scope, generic_findings)
            .unwrap();
        tier.grant_run_authority(generic_capability, generic_scope, generic_findings, 9)
            .unwrap();
        assert_eq!(tier.explicit_scope_consent_count(), 2);

        assert_eq!(
            tier.require_run_authority(generic_scope, generic_findings, 9),
            Ok(())
        );
        assert_eq!(
            tier.require_cloud_autoscrub_capability(
                generic_capability,
                scan_only_scope,
                generic_findings,
                9
            ),
            Err(CloudAutoScrubConsentError::CapabilityRequired)
        );
        assert_eq!(
            tier.require_attended_authority(generic_scope, generic_findings, 9),
            Err(CloudAutoScrubConsentError::AuthorityRequired)
        );
        assert_eq!(
            tier.require_run_authority(scan_only_scope, generic_findings, 9),
            Err(CloudAutoScrubConsentError::AuthorityRequired)
        );
        assert_eq!(
            tier.require_cloud_processing_scope(scan_only_scope),
            Err(CloudAutoScrubConsentError::ExternalSecurityReviewRequired)
        );
        assert_eq!(
            tier.require_cloud_processing_scope(generic_scope),
            Err(CloudAutoScrubConsentError::ExternalSecurityReviewRequired)
        );

        assert!(tier.revoke_scope_consent(scan_only_scope));
        assert_eq!(
            tier.require_attended_authority(scan_only_scope, scan_findings, 9),
            Err(CloudAutoScrubConsentError::AuthorityRequired)
        );
        assert_eq!(
            tier.require_run_authority(generic_scope, generic_findings, 9),
            Ok(())
        );
    }
}
