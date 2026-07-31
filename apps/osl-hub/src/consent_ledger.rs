//! Single-use consent grants bound to one owner/account pair.
//!
//! The ledger refuses missing or mismatched bindings, removes grants on
//! successful or expired consumption, and redacts owner/account values from
//! debug output.

use std::collections::BTreeMap;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ConsentGrantId([u8; 16]);

impl ConsentGrantId {
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct ConsentBinding {
    owner: String,
    account: String,
}

impl ConsentBinding {
    pub fn new(owner: impl Into<String>, account: impl Into<String>) -> Result<Self, ConsentError> {
        let binding = Self {
            owner: owner.into(),
            account: account.into(),
        };
        validate_binding(&binding.owner)?;
        validate_binding(&binding.account)?;
        Ok(binding)
    }

    pub fn is_bound_to(&self, owner: &str, account: &str) -> bool {
        self.owner == owner && self.account == account
    }
}

impl fmt::Debug for ConsentBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConsentBinding")
            .field("owner", &"<redacted>")
            .field("account", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct ConsentGrant {
    id: ConsentGrantId,
    binding: ConsentBinding,
    used: bool,
    expires_at_unix: Option<u64>,
}

impl ConsentGrant {
    pub fn id(&self) -> ConsentGrantId {
        self.id
    }

    pub fn is_bound_to(&self, owner: &str, account: &str) -> bool {
        self.binding.is_bound_to(owner, account)
    }

    pub fn is_unused(&self) -> bool {
        !self.used
    }

    pub fn expires_at_unix(&self) -> Option<u64> {
        self.expires_at_unix
    }
}

impl fmt::Debug for ConsentGrant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConsentGrant")
            .field("id", &self.id)
            .field("binding", &self.binding)
            .field("used", &self.used)
            .field("expires_at_unix", &self.expires_at_unix)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ConsentError {
    InvalidBinding,
    IdOverflow,
    GrantIdOverflow,
    MissingGrant,
    GrantMissing,
    BindingMismatch,
    AlreadyUsed,
    GrantExpired,
}

pub type ConsentLedgerError = ConsentError;

impl fmt::Display for ConsentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidBinding => "consent grant binding is invalid",
            Self::IdOverflow | Self::GrantIdOverflow => "consent grant id overflow",
            Self::MissingGrant | Self::GrantMissing => "consent grant is missing",
            Self::BindingMismatch => "consent grant binding mismatch",
            Self::AlreadyUsed => "consent grant was already used",
            Self::GrantExpired => "consent grant expired",
        };
        f.write_str(message)
    }
}

impl std::error::Error for ConsentError {}

#[derive(Debug, Default)]
pub struct ConsentLedger {
    next_id: u128,
    grants: BTreeMap<ConsentGrantId, ConsentGrant>,
}

impl ConsentLedger {
    pub fn issue(
        &mut self,
        owner: impl Into<String>,
        account: impl Into<String>,
    ) -> Result<ConsentGrant, ConsentLedgerError> {
        let binding = ConsentBinding::new(owner, account)?;
        self.issue_inner(binding, None, ConsentError::IdOverflow)
    }

    pub fn issue_with_expiry(
        &mut self,
        binding: ConsentBinding,
        now_unix: u64,
        ttl_secs: u64,
    ) -> Result<ConsentGrant, ConsentError> {
        self.issue_inner(
            binding,
            Some(now_unix.saturating_add(ttl_secs)),
            ConsentError::GrantIdOverflow,
        )
    }

    fn issue_inner(
        &mut self,
        binding: ConsentBinding,
        expires_at_unix: Option<u64>,
        overflow_error: ConsentError,
    ) -> Result<ConsentGrant, ConsentError> {
        self.next_id = self.next_id.checked_add(1).ok_or(overflow_error)?;
        let id = ConsentGrantId(self.next_id.to_be_bytes());
        let grant = ConsentGrant {
            id,
            binding,
            used: false,
            expires_at_unix,
        };
        self.grants.insert(id, grant.clone());
        Ok(grant)
    }

    pub fn consume(
        &mut self,
        id: ConsentGrantId,
        owner: &str,
        account: &str,
    ) -> Result<ConsentGrant, ConsentLedgerError> {
        let binding = ConsentBinding::new(owner, account)?;
        self.consume_inner(id, &binding, None, ConsentError::MissingGrant)
    }

    pub fn consume_with_binding(
        &mut self,
        grant_id: ConsentGrantId,
        binding: &ConsentBinding,
        now_unix: u64,
    ) -> Result<ConsentGrant, ConsentError> {
        self.consume_inner(
            grant_id,
            binding,
            Some(now_unix),
            ConsentError::GrantMissing,
        )
    }

    pub fn pending_count(&self) -> usize {
        self.grants.len()
    }

    fn consume_inner(
        &mut self,
        id: ConsentGrantId,
        binding: &ConsentBinding,
        now_unix: Option<u64>,
        missing_error: ConsentError,
    ) -> Result<ConsentGrant, ConsentError> {
        let stored = self.grants.get(&id).ok_or(missing_error)?;
        if &stored.binding != binding {
            return Err(ConsentError::BindingMismatch);
        }
        if stored.used {
            return Err(ConsentError::AlreadyUsed);
        }

        let mut grant = self
            .grants
            .remove(&id)
            .expect("grant was checked present above");
        grant.used = true;
        if let (Some(now_unix), Some(expires_at_unix)) = (now_unix, grant.expires_at_unix) {
            if now_unix > expires_at_unix {
                return Err(ConsentError::GrantExpired);
            }
        }
        Ok(grant)
    }
}

fn validate_binding(value: &str) -> Result<(), ConsentError> {
    const MAX_BINDING_BYTES: usize = 256;
    if value.is_empty() || value.len() > MAX_BINDING_BYTES {
        Err(ConsentError::InvalidBinding)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consent_ledger_issue_mints_single_use_bound_grant() {
        let mut ledger = ConsentLedger::default();

        let grant = ledger.issue("owner-a", "account-a").unwrap();

        assert_eq!(ledger.pending_count(), 1);
        assert!(grant.is_unused());
        assert!(grant.is_bound_to("owner-a", "account-a"));
        assert!(!grant.is_bound_to("owner-b", "account-a"));
        assert!(!grant.is_bound_to("owner-a", "account-b"));
        assert_ne!(grant.id().as_bytes(), &[0; 16]);

        assert_eq!(
            ledger.consume(grant.id(), "owner-b", "account-a"),
            Err(ConsentLedgerError::BindingMismatch),
            "a grant bound to a different owner must refuse before spending"
        );
        assert_eq!(ledger.pending_count(), 1);

        let spent = ledger.consume(grant.id(), "owner-a", "account-a").unwrap();
        assert_eq!(spent.id(), grant.id());
        assert!(!spent.is_unused());
        assert_eq!(
            ledger.consume(grant.id(), "owner-a", "account-a"),
            Err(ConsentLedgerError::MissingGrant),
            "a successfully consumed grant must not be replayable"
        );

        let debug = format!("{grant:?}");
        assert!(debug.contains("ConsentGrant"));
        assert!(!debug.contains("owner-a"));
        assert!(!debug.contains("account-a"));
    }
}

#[cfg(test)]
mod expiry_tests {
    use super::*;

    #[test]
    fn consent_ledger_issue_mints_expiring_single_use_bound_grant() {
        let mut ledger = ConsentLedger::default();
        let binding = ConsentBinding::new("owner-a", "account-a").unwrap();
        let other_account = ConsentBinding::new("owner-a", "account-b").unwrap();

        let grant = ledger.issue_with_expiry(binding.clone(), 100, 30).unwrap();

        assert_eq!(
            grant.id().as_bytes(),
            &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]
        );
        assert_eq!(grant.expires_at_unix(), Some(130));
        assert_eq!(
            ledger.consume_with_binding(grant.id(), &other_account, 110),
            Err(ConsentError::BindingMismatch),
            "a grant bound to one account must refuse a different account"
        );
        let consumed = ledger
            .consume_with_binding(grant.id(), &binding, 110)
            .unwrap();
        assert_eq!(consumed.id(), grant.id());
        assert_eq!(
            ledger.consume_with_binding(grant.id(), &binding, 111),
            Err(ConsentError::GrantMissing),
            "a spent grant must not be usable twice"
        );
    }

    #[test]
    fn expired_grant_is_removed_when_refused() {
        let mut ledger = ConsentLedger::default();
        let binding = ConsentBinding::new("owner-a", "account-a").unwrap();
        let grant = ledger.issue_with_expiry(binding.clone(), 100, 10).unwrap();

        assert_eq!(
            ledger.consume_with_binding(grant.id(), &binding, 111),
            Err(ConsentError::GrantExpired)
        );
        assert_eq!(
            ledger.consume_with_binding(grant.id(), &binding, 111),
            Err(ConsentError::GrantMissing)
        );
    }
}
