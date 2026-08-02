//! Durable encrypted storage for the client-authoritative Space roster.
//!
//! The roster schema is deliberately owned by the Space lifecycle work. This
//! module owns only its at-rest boundary: a roster is never created or
//! overwritten without the unlocked file-storage key, and writes are atomic.

use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path};

/// Opaque, client-generated identity for a Space.
///
/// A Space ID is fresh CSPRNG output. It deliberately accepts no account or
/// founder input, so creating multiple Spaces cannot create an account-derived
/// identifier that a relay could use to group their memberships. This is local
/// roster state only: delivery routing uses independent rotating tags.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct SpaceId([u8; Self::LENGTH]);

impl SpaceId {
    /// The number of uniformly random bytes in a Space identity.
    pub const LENGTH: usize = 32;

    /// Creates a new unlinkable Space identity from the operating system CSPRNG.
    pub fn generate() -> Self {
        let mut bytes = [0_u8; Self::LENGTH];
        OsRng.fill_bytes(&mut bytes);
        Self(bytes)
    }

    /// Returns locally stored bytes for serialization in the encrypted roster.
    pub fn as_bytes(&self) -> &[u8; Self::LENGTH] {
        &self.0
    }
}

/// Monotonic membership version for a Space.
///
/// Membership events advance this value before key rotation binds to it. The
/// zero value represents a newly generated Space before its first event.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct SpaceEpoch(u64);

impl SpaceEpoch {
    /// Epoch of a Space before the create event establishes membership.
    pub const INITIAL: Self = Self(0);

    /// Returns the epoch as a value suitable for local roster persistence.
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Advances exactly once for one membership change.
    ///
    /// Overflow is rejected rather than wrapping, because wraparound could
    /// make a future membership event appear stale to the rotation layer.
    pub fn advance(self) -> Result<Self, SpaceEpochError> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or(SpaceEpochError::Exhausted)
    }
}

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum SpaceEpochError {
    #[error("space membership epoch is exhausted")]
    Exhausted,
}

/// Opaque local reference to a member's identity key.
///
/// A Space roster records members as identity-key digests, not account names
/// or delivery addresses. The value is meaningful only to the client that
/// holds this encrypted roster; routing is deliberately a separate concern.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct SpaceMemberId([u8; Self::LENGTH]);

impl SpaceMemberId {
    pub const LENGTH: usize = 32;

    /// Creates a local roster member reference from an identity-key digest.
    pub fn from_identity_key_digest(digest: [u8; Self::LENGTH]) -> Result<Self, SpaceRosterError> {
        if digest == [0_u8; Self::LENGTH] {
            return Err(SpaceRosterError::InvalidMemberId);
        }
        Ok(Self(digest))
    }

    pub fn as_bytes(&self) -> &[u8; Self::LENGTH] {
        &self.0
    }
}

/// Membership state for one Space, held only in the encrypted local roster.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LocalSpaceRoster {
    space_id: SpaceId,
    epoch: SpaceEpoch,
    members: BTreeSet<SpaceMemberId>,
}

impl LocalSpaceRoster {
    pub fn space_id(&self) -> SpaceId {
        self.space_id
    }

    pub fn epoch(&self) -> SpaceEpoch {
        self.epoch
    }

    pub fn members(&self) -> impl ExactSizeIterator<Item = SpaceMemberId> + '_ {
        self.members.iter().copied()
    }
}

/// Authoritative membership state for every locally known Space.
///
/// This type intentionally has no transport, keyserver, account, or delivery
/// address fields. The relay never receives this map: event distribution is a
/// later, encrypted client-to-client concern.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SpaceRoster {
    spaces: Vec<LocalSpaceRoster>,
}

impl SpaceRoster {
    /// Adds a newly learned local Space membership snapshot.
    ///
    /// Signed, ordered membership events are applied by T21-C4. This C3
    /// operation only establishes the locally held state container they will
    /// update; replacing an existing Space is refused.
    pub fn insert(
        &mut self,
        space_id: SpaceId,
        epoch: SpaceEpoch,
        members: impl IntoIterator<Item = SpaceMemberId>,
    ) -> Result<(), SpaceRosterError> {
        if self.get(space_id).is_some() {
            return Err(SpaceRosterError::SpaceAlreadyExists);
        }

        let members = members.into_iter().collect();
        self.spaces.push(LocalSpaceRoster {
            space_id,
            epoch,
            members,
        });
        Ok(())
    }

    /// Returns local membership state for one Space.
    pub fn get(&self, space_id: SpaceId) -> Option<&LocalSpaceRoster> {
        self.spaces
            .iter()
            .find(|local_roster| local_roster.space_id == space_id)
    }

    /// Returns the number of locally known Spaces.
    pub fn len(&self) -> usize {
        self.spaces.len()
    }

    pub fn is_empty(&self) -> bool {
        self.spaces.is_empty()
    }
}

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum SpaceRosterError {
    #[error("a space member identity digest must not be all zeroes")]
    InvalidMemberId,
    #[error("space roster already contains this Space")]
    SpaceAlreadyExists,
}

/// The single account-relative path for the Space roster.
///
/// Keep this constant as the source of truth for every lifecycle sweep. A
/// Space roster contains membership state and must move with an identity,
/// survive password changes, and be present in an encrypted data export.
pub const SPACE_ROSTER_FILE: &str = "space_roster.json";

#[derive(Debug, thiserror::Error)]
pub enum SpaceRosterFileError {
    #[error("space roster not found at {0}")]
    NotFound(String),
    #[error("space roster read failed at {path}: {source}")]
    ReadFailed {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("space roster decrypt failed at {path}: {reason}")]
    DecryptFailed { path: String, reason: String },
    #[error("space roster write failed at {path}: {source}")]
    WriteFailed {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

/// Loads the decrypted serialized roster. Interpretation belongs to the
/// roster-state owner so persistence does not freeze a schema ahead of it.
pub fn load_space_roster(path: &Path) -> Result<Vec<u8>, SpaceRosterFileError> {
    let encrypted = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            return Err(SpaceRosterFileError::NotFound(path.display().to_string()));
        }
        Err(source) => {
            return Err(SpaceRosterFileError::ReadFailed {
                path: path.display().to_string(),
                source,
            });
        }
    };

    crate::main_password::maybe_decrypt_file(path, &encrypted).map_err(|reason| {
        SpaceRosterFileError::DecryptFailed {
            path: path.display().to_string(),
            reason,
        }
    })
}

/// Encrypts and atomically replaces the serialized roster.
///
/// A locked client must refuse rather than create plaintext state or overwrite
/// the encrypted roster with data it cannot protect.
pub fn write_space_roster(
    path: &Path,
    serialized_roster: &[u8],
) -> Result<(), SpaceRosterFileError> {
    let key = crate::main_password::get_file_storage_key().ok_or_else(|| {
        SpaceRosterFileError::WriteFailed {
            path: path.display().to_string(),
            source: std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "OSL: refusing to write plaintext space roster without file_storage_key",
            ),
        }
    })?;
    let encrypted =
        crate::main_password::encrypt_at_rest(serialized_roster, &key).map_err(|source| {
            SpaceRosterFileError::WriteFailed {
                path: path.display().to_string(),
                source: std::io::Error::other(source),
            }
        })?;
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, encrypted).map_err(|source| SpaceRosterFileError::WriteFailed {
        path: temporary.display().to_string(),
        source,
    })?;
    std::fs::rename(&temporary, path).map_err(|source| SpaceRosterFileError::WriteFailed {
        path: path.display().to_string(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::main_password::{has_enc_magic, set_file_storage_key};
    use tempfile::tempdir;

    #[test]
    fn roster_persistence_is_encrypted_atomic_and_refuses_a_locked_write() {
        let _guard = crate::test_process_globals::serialize();
        let directory = tempdir().unwrap();
        let path = directory.path().join(SPACE_ROSTER_FILE);
        let roster = br#"{"version":1,"spaces":[]}"#;

        set_file_storage_key(None);
        let refused = write_space_roster(&path, roster).unwrap_err();
        assert!(matches!(
            refused,
            SpaceRosterFileError::WriteFailed { source, .. }
                if source.kind() == std::io::ErrorKind::PermissionDenied
        ));
        assert!(
            !path.exists(),
            "a locked write must not create plaintext state"
        );

        set_file_storage_key(Some([0x6d; 32]));
        write_space_roster(&path, roster).unwrap();
        assert!(has_enc_magic(&std::fs::read(&path).unwrap()));
        assert_eq!(load_space_roster(&path).unwrap(), roster);

        set_file_storage_key(None);
    }

    #[test]
    fn generated_space_ids_are_fresh_and_epochs_advance_monotonically() {
        let first = SpaceId::generate();
        let second = SpaceId::generate();
        assert_ne!(first, second, "independently created Spaces need fresh IDs");
        assert_ne!(first.as_bytes(), &[0_u8; SpaceId::LENGTH]);

        let first_membership = SpaceEpoch::INITIAL.advance().unwrap();
        let second_membership = first_membership.advance().unwrap();
        assert_eq!(first_membership.get(), 1);
        assert_eq!(second_membership.get(), 2);
        assert!(second_membership > first_membership);
    }

    #[test]
    fn an_exhausted_epoch_never_wraps_to_a_stale_value() {
        let exhausted = SpaceEpoch(u64::MAX);
        assert_eq!(exhausted.advance(), Err(SpaceEpochError::Exhausted));
    }
}
