//! Fail-closed consent contract for optional cloud AutoScrub processing.
//!
//! This module only models consent and the external security-review gate. It
//! does not send data, select providers, or offer a path to enable cloud work.

use std::collections::BTreeSet;
use std::fmt;

use sha2::{Digest, Sha256};

const SCOPE_DOMAIN: &[u8] = b"OSL/cloud-autoscrub-scope/v1";
const MAX_SCOPE_BINDING_BYTES: usize = 256;
const MAX_EXPLICIT_SCOPE_CONSENTS: usize = 128;

#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CloudAutoScrubScope {
    commitment: [u8; 32],
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
        })
    }

    pub fn commitment(&self) -> [u8; 32] {
        self.commitment
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

#[derive(Clone, Eq, PartialEq)]
pub struct CloudAutoScrubConsentTier {
    explicit_scope_consents: BTreeSet<CloudAutoScrubScope>,
    security_review_gate: CloudAutoScrubSecurityReviewGate,
}

impl Default for CloudAutoScrubConsentTier {
    fn default() -> Self {
        Self {
            explicit_scope_consents: BTreeSet::new(),
            security_review_gate: CloudAutoScrubSecurityReviewGate::Unmet,
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
        self.explicit_scope_consents.remove(&scope)
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
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CloudAutoScrubConsentError {
    InvalidScope,
    TooManyScopes,
    ExplicitScopeConsentRequired,
    ExternalSecurityReviewRequired,
}

impl fmt::Display for CloudAutoScrubConsentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidScope => "cloud AutoScrub scope is invalid",
            Self::TooManyScopes => "cloud AutoScrub has too many explicit scope consents",
            Self::ExplicitScopeConsentRequired => {
                "explicit per-scope cloud AutoScrub consent is required"
            }
            Self::ExternalSecurityReviewRequired => {
                "external cloud AutoScrub security review is required"
            }
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
        CloudAutoScrubScope::derive(b"owner-binding", b"discord", account, data_scope)
            .unwrap()
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
        let error_display =
            CloudAutoScrubConsentError::ExternalSecurityReviewRequired.to_string();

        for rendered in [scope_debug, tier_debug, error_debug, error_display] {
            assert!(!rendered.contains("secret-account-identifier"));
            assert!(!rendered.contains("discord"));
        }
    }
}
