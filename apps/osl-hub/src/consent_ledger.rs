//! Single-use consent grants bound to one owner/account pair.
//!
//! This file is intentionally self-contained: the caller must present the exact
//! owner and account binding that minted a grant, and a successful or expired
//! consumption removes the grant so it cannot be replayed.

use std::collections::BTreeMap;
use std::fmt;

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
    id: u64,
    binding: ConsentBinding,
    expires_at_unix: u64,
}

impl ConsentGrant {
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn expires_at_unix(&self) -> u64 {
        self.expires_at_unix
    }
}

impl fmt::Debug for ConsentGrant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConsentGrant")
            .field("id", &self.id)
            .field("binding", &self.binding)
            .field("expires_at_unix", &self.expires_at_unix)
            .finish()
    }
}

#[derive(Debug, Default)]
pub struct ConsentLedger {
    next_id: u64,
    grants: BTreeMap<u64, ConsentGrant>,
}

impl ConsentLedger {
    pub fn issue(
        &mut self,
        binding: ConsentBinding,
        now_unix: u64,
        ttl_secs: u64,
    ) -> Result<ConsentGrant, ConsentError> {
        let id = self
            .next_id
            .checked_add(1)
            .ok_or(ConsentError::GrantIdOverflow)?;
        self.next_id = id;
        let grant = ConsentGrant {
            id,
            binding,
            expires_at_unix: now_unix.saturating_add(ttl_secs),
        };
        self.grants.insert(id, grant.clone());
        Ok(grant)
    }

    pub fn consume(
        &mut self,
        grant_id: u64,
        binding: &ConsentBinding,
        now_unix: u64,
    ) -> Result<ConsentGrant, ConsentError> {
        let stored = self
            .grants
            .get(&grant_id)
            .ok_or(ConsentError::GrantMissing)?;
        if &stored.binding != binding {
            return Err(ConsentError::BindingMismatch);
        }
        let grant = self
            .grants
            .remove(&grant_id)
            .expect("grant was checked present above");
        if now_unix > grant.expires_at_unix {
            return Err(ConsentError::GrantExpired);
        }
        Ok(grant)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ConsentError {
    InvalidBinding,
    GrantIdOverflow,
    GrantMissing,
    BindingMismatch,
    GrantExpired,
}

impl fmt::Display for ConsentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidBinding => "consent binding is invalid",
            Self::GrantIdOverflow => "consent grant id overflow",
            Self::GrantMissing => "consent grant is missing",
            Self::BindingMismatch => "consent binding mismatch",
            Self::GrantExpired => "consent grant expired",
        };
        f.write_str(message)
    }
}

impl std::error::Error for ConsentError {}

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
        let binding = ConsentBinding::new("owner-a", "account-a").unwrap();
        let other_account = ConsentBinding::new("owner-a", "account-b").unwrap();

        let grant = ledger.issue(binding.clone(), 100, 30).unwrap();

        assert_eq!(grant.id(), 1);
        assert_eq!(grant.expires_at_unix(), 130);
        assert_eq!(
            ledger.consume(grant.id(), &other_account, 110),
            Err(ConsentError::BindingMismatch),
            "a grant bound to one account must refuse a different account"
        );
        let consumed = ledger.consume(grant.id(), &binding, 110).unwrap();
        assert_eq!(consumed.id(), grant.id());
        assert_eq!(
            ledger.consume(grant.id(), &binding, 111),
            Err(ConsentError::GrantMissing),
            "a spent grant must not be usable twice"
        );
    }
}
