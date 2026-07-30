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
pub struct ConsentGrant {
    id: ConsentGrantId,
    owner: String,
    account: String,
    used: bool,
}

impl ConsentGrant {
    pub fn id(&self) -> ConsentGrantId {
        self.id
    }

    pub fn is_bound_to(&self, owner: &str, account: &str) -> bool {
        self.owner == owner && self.account == account
    }

    pub fn is_unused(&self) -> bool {
        !self.used
    }
}

impl fmt::Debug for ConsentGrant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConsentGrant")
            .field("id", &self.id)
            .field("owner", &"<redacted>")
            .field("account", &"<redacted>")
            .field("used", &self.used)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ConsentLedgerError {
    InvalidBinding,
    IdOverflow,
    MissingGrant,
    BindingMismatch,
    AlreadyUsed,
}

impl fmt::Display for ConsentLedgerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidBinding => "consent grant binding is invalid",
            Self::IdOverflow => "consent grant id overflow",
            Self::MissingGrant => "consent grant is missing",
            Self::BindingMismatch => "consent grant binding mismatch",
            Self::AlreadyUsed => "consent grant was already used",
        };
        f.write_str(message)
    }
}

impl std::error::Error for ConsentLedgerError {}

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
        let owner = owner.into();
        let account = account.into();
        validate_binding(&owner)?;
        validate_binding(&account)?;

        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or(ConsentLedgerError::IdOverflow)?;
        let id = ConsentGrantId(self.next_id.to_be_bytes());
        let grant = ConsentGrant {
            id,
            owner,
            account,
            used: false,
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
        let grant = self
            .grants
            .get_mut(&id)
            .ok_or(ConsentLedgerError::MissingGrant)?;
        if !grant.is_bound_to(owner, account) {
            return Err(ConsentLedgerError::BindingMismatch);
        }
        if grant.used {
            return Err(ConsentLedgerError::AlreadyUsed);
        }
        grant.used = true;
        let spent = grant.clone();
        self.grants.remove(&id);
        Ok(spent)
    }

    pub fn pending_count(&self) -> usize {
        self.grants.len()
    }
}

fn validate_binding(value: &str) -> Result<(), ConsentLedgerError> {
    if value.is_empty() || value.len() > 256 {
        Err(ConsentLedgerError::InvalidBinding)
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
