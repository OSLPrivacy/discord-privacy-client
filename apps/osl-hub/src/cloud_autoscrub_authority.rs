//! Single-use local authority for future cloud AutoScrub actions.
//!
//! This module does not enable cloud processing. It models the local proof that
//! a specific attended action was both locked to a target and, for the reviewed
//! lane, reviewed by the owner. Missing or already-spent authority refuses.

use std::collections::BTreeSet;
use std::fmt;

use sha2::{Digest, Sha256};

const GRANT_DOMAIN: &[u8] = b"OSL/cloud-autoscrub-authority/v1";
const MAX_BINDING_BYTES: usize = 256;

#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CloudAutoScrubGrant {
    commitment: [u8; 32],
}

impl CloudAutoScrubGrant {
    pub fn derive(
        owner_binding: &[u8],
        target_binding: &[u8],
        action_binding: &[u8],
        nonce: &[u8],
    ) -> Result<Self, CloudAutoScrubAuthorityError> {
        validate_binding(owner_binding)?;
        validate_binding(target_binding)?;
        validate_binding(action_binding)?;
        validate_binding(nonce)?;

        let mut bytes = Vec::new();
        bytes.extend_from_slice(GRANT_DOMAIN);
        write_lp(&mut bytes, owner_binding)?;
        write_lp(&mut bytes, target_binding)?;
        write_lp(&mut bytes, action_binding)?;
        write_lp(&mut bytes, nonce)?;
        Ok(Self {
            commitment: Sha256::digest(bytes).into(),
        })
    }
}

impl fmt::Debug for CloudAutoScrubGrant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CloudAutoScrubGrant")
            .field("commitment", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CloudAutoScrubAuthorityError {
    InvalidBinding,
    MissingAuthority,
}

impl fmt::Display for CloudAutoScrubAuthorityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidBinding => "cloud AutoScrub authority binding is invalid",
            Self::MissingAuthority => "cloud AutoScrub authority is missing or already spent",
        };
        f.write_str(message)
    }
}

impl std::error::Error for CloudAutoScrubAuthorityError {}

#[derive(Default)]
pub struct CloudAutoScrubAuthority {
    attended_locked: BTreeSet<CloudAutoScrubGrant>,
    reviewed_attended_locked: BTreeSet<CloudAutoScrubGrant>,
}

impl fmt::Debug for CloudAutoScrubAuthority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CloudAutoScrubAuthority")
            .field("attended_locked_count", &self.attended_locked.len())
            .field(
                "reviewed_attended_locked_count",
                &self.reviewed_attended_locked.len(),
            )
            .finish()
    }
}

impl CloudAutoScrubAuthority {
    pub fn issue_attended_locked(&mut self, grant: CloudAutoScrubGrant) -> bool {
        self.attended_locked.insert(grant)
    }

    pub fn issue_reviewed_attended_locked(&mut self, grant: CloudAutoScrubGrant) -> bool {
        self.reviewed_attended_locked.insert(grant)
    }

    pub fn spend_attended_locked(
        &mut self,
        grant: CloudAutoScrubGrant,
    ) -> Result<(), CloudAutoScrubAuthorityError> {
        if self.attended_locked.remove(&grant) {
            Ok(())
        } else {
            Err(CloudAutoScrubAuthorityError::MissingAuthority)
        }
    }

    pub fn spend_reviewed_attended_locked(
        &mut self,
        grant: CloudAutoScrubGrant,
    ) -> Result<(), CloudAutoScrubAuthorityError> {
        if self.reviewed_attended_locked.remove(&grant) {
            Ok(())
        } else {
            Err(CloudAutoScrubAuthorityError::MissingAuthority)
        }
    }
}

fn validate_binding(value: &[u8]) -> Result<(), CloudAutoScrubAuthorityError> {
    if value.is_empty() || value.len() > MAX_BINDING_BYTES {
        Err(CloudAutoScrubAuthorityError::InvalidBinding)
    } else {
        Ok(())
    }
}

fn write_lp(target: &mut Vec<u8>, value: &[u8]) -> Result<(), CloudAutoScrubAuthorityError> {
    let length =
        u32::try_from(value.len()).map_err(|_| CloudAutoScrubAuthorityError::InvalidBinding)?;
    target.extend_from_slice(&length.to_be_bytes());
    target.extend_from_slice(value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grant(nonce: &[u8]) -> CloudAutoScrubGrant {
        CloudAutoScrubGrant::derive(
            b"owner-binding",
            b"target-account-binding",
            b"delete-selected-posts",
            nonce,
        )
        .unwrap()
    }

    #[test]
    fn spend_attended_locked_and_reviewed_attended_locked_are_single_use() {
        let mut authority = CloudAutoScrubAuthority::default();
        let attended = grant(b"attended-nonce");
        let reviewed = grant(b"reviewed-nonce");

        assert_eq!(
            authority.spend_attended_locked(attended),
            Err(CloudAutoScrubAuthorityError::MissingAuthority)
        );
        assert!(authority.issue_attended_locked(attended));
        assert!(authority.issue_reviewed_attended_locked(reviewed));

        assert_eq!(
            authority.spend_reviewed_attended_locked(attended),
            Err(CloudAutoScrubAuthorityError::MissingAuthority),
            "an attended grant must not satisfy the reviewed lane"
        );
        assert_eq!(authority.spend_attended_locked(attended), Ok(()));
        assert_eq!(
            authority.spend_attended_locked(attended),
            Err(CloudAutoScrubAuthorityError::MissingAuthority),
            "attended authority must be consumed on first spend"
        );

        assert_eq!(authority.spend_reviewed_attended_locked(reviewed), Ok(()));
        assert_eq!(
            authority.spend_reviewed_attended_locked(reviewed),
            Err(CloudAutoScrubAuthorityError::MissingAuthority),
            "reviewed attended authority must be consumed on first spend"
        );
    }
}
