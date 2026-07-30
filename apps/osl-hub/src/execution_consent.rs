//! Explicit execution consent for hosted cleanup runs.
//!
//! Consent is scoped to the exact provider, account, and scope fingerprint the
//! run asks to execute. Missing consent, mismatched scope, or an expired grant is
//! a refusal.

use sha2::{Digest, Sha256};
use std::fmt;

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ScopeFingerprint {
    digest: [u8; 32],
}

impl ScopeFingerprint {
    pub fn from_digest(digest: [u8; 32]) -> Option<Self> {
        if digest.iter().all(|byte| *byte == 0) {
            None
        } else {
            Some(Self { digest })
        }
    }

    pub fn from_scope_parts(parts: &[&str]) -> Option<Self> {
        if parts.is_empty()
            || parts
                .iter()
                .any(|part| part.is_empty() || has_control(part))
        {
            return None;
        }
        let mut hasher = Sha256::new();
        hasher.update(b"OSL/execution-consent-scope/v1");
        for part in parts {
            let bytes = part.as_bytes();
            hasher.update((bytes.len() as u32).to_be_bytes());
            hasher.update(bytes);
        }
        Self::from_digest(hasher.finalize().into())
    }

    pub const fn digest(&self) -> [u8; 32] {
        self.digest
    }
}

impl fmt::Debug for ScopeFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ScopeFingerprint")
            .field(&"<redacted>")
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct ExecutionScope {
    provider_id: String,
    account_id: String,
    scope_fingerprint: ScopeFingerprint,
}

impl ExecutionScope {
    pub fn new(
        provider_id: impl Into<String>,
        account_id: impl Into<String>,
        scope_fingerprint: ScopeFingerprint,
    ) -> Result<Self, ExecutionConsentError> {
        let provider_id = provider_id.into();
        let account_id = account_id.into();
        if invalid_id(&provider_id) || invalid_id(&account_id) {
            return Err(ExecutionConsentError::InvalidScope);
        }
        Ok(Self {
            provider_id,
            account_id,
            scope_fingerprint,
        })
    }

    pub fn matches(&self, other: &Self) -> bool {
        self.provider_id == other.provider_id
            && self.account_id == other.account_id
            && self.scope_fingerprint == other.scope_fingerprint
    }
}

impl fmt::Debug for ExecutionScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExecutionScope")
            .field("provider_id", &"<redacted>")
            .field("account_id", &"<redacted>")
            .field("scope_fingerprint", &self.scope_fingerprint)
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct ExecutionConsent {
    scope: ExecutionScope,
    granted_at_unix_seconds: u64,
    expires_at_unix_seconds: u64,
}

impl ExecutionConsent {
    pub fn new(
        scope: ExecutionScope,
        granted_at_unix_seconds: u64,
        expires_at_unix_seconds: u64,
    ) -> Result<Self, ExecutionConsentError> {
        if expires_at_unix_seconds <= granted_at_unix_seconds {
            return Err(ExecutionConsentError::InvalidLifetime);
        }
        Ok(Self {
            scope,
            granted_at_unix_seconds,
            expires_at_unix_seconds,
        })
    }

    pub const fn scope(&self) -> &ExecutionScope {
        &self.scope
    }

    pub const fn expires_at_unix_seconds(&self) -> u64 {
        self.expires_at_unix_seconds
    }
}

impl fmt::Debug for ExecutionConsent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExecutionConsent")
            .field("scope", &self.scope)
            .field("granted_at_unix_seconds", &self.granted_at_unix_seconds)
            .field("expires_at_unix_seconds", &self.expires_at_unix_seconds)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ExecutionConsentError {
    MissingConsent,
    InvalidScope,
    InvalidLifetime,
    ScopeMismatch,
    Expired,
}

pub fn authorize_execution(
    consent: Option<&ExecutionConsent>,
    requested_scope: &ExecutionScope,
    now_unix_seconds: u64,
) -> Result<(), ExecutionConsentError> {
    let consent = consent.ok_or(ExecutionConsentError::MissingConsent)?;
    if !consent.scope.matches(requested_scope) {
        return Err(ExecutionConsentError::ScopeMismatch);
    }
    if now_unix_seconds >= consent.expires_at_unix_seconds {
        return Err(ExecutionConsentError::Expired);
    }
    Ok(())
}

fn invalid_id(value: &str) -> bool {
    value.is_empty() || value.len() > 128 || has_control(value)
}

fn has_control(value: &str) -> bool {
    value
        .chars()
        .any(|character| character <= '\u{1f}' || character == '\u{7f}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_consent_scope_fingerprint_contract() {
        let fingerprint =
            ScopeFingerprint::from_scope_parts(&["visible-row", "credential"]).unwrap();
        let changed_fingerprint =
            ScopeFingerprint::from_scope_parts(&["visible-row", "payment-card"]).unwrap();
        assert_ne!(fingerprint, changed_fingerprint);
        assert!(ScopeFingerprint::from_digest([0; 32]).is_none());

        let requested =
            ExecutionScope::new("discord", "account-123", fingerprint).expect("valid scope");
        let consent =
            ExecutionConsent::new(requested.clone(), 1_800_000_000, 1_800_000_300).unwrap();
        assert_eq!(
            authorize_execution(Some(&consent), &requested, 1_800_000_120),
            Ok(())
        );
        assert_eq!(
            authorize_execution(None, &requested, 1_800_000_120),
            Err(ExecutionConsentError::MissingConsent)
        );

        let wrong_provider =
            ExecutionScope::new("slack", "account-123", fingerprint).expect("valid scope");
        let wrong_account =
            ExecutionScope::new("discord", "account-456", fingerprint).expect("valid scope");
        let wrong_fingerprint = ExecutionScope::new("discord", "account-123", changed_fingerprint)
            .expect("valid scope");
        assert_eq!(
            authorize_execution(Some(&consent), &wrong_provider, 1_800_000_120),
            Err(ExecutionConsentError::ScopeMismatch)
        );
        assert_eq!(
            authorize_execution(Some(&consent), &wrong_account, 1_800_000_120),
            Err(ExecutionConsentError::ScopeMismatch)
        );
        assert_eq!(
            authorize_execution(Some(&consent), &wrong_fingerprint, 1_800_000_120),
            Err(ExecutionConsentError::ScopeMismatch)
        );
        assert_eq!(
            authorize_execution(Some(&consent), &requested, 1_800_000_300),
            Err(ExecutionConsentError::Expired)
        );

        let debug = format!("{consent:?}");
        assert!(!debug.contains("discord"));
        assert!(!debug.contains("account-123"));
        assert!(!debug.contains(&hex(fingerprint.digest())));
    }

    fn hex(bytes: [u8; 32]) -> String {
        const TABLE: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(64);
        for byte in bytes {
            out.push(TABLE[(byte >> 4) as usize] as char);
            out.push(TABLE[(byte & 0x0f) as usize] as char);
        }
        out
    }
}
