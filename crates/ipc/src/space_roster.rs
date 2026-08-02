//! Durable encrypted storage for the client-authoritative Space roster.
//!
//! The roster schema is deliberately owned by the Space lifecycle work. This
//! module owns only its at-rest boundary: a roster is never created or
//! overwritten without the unlocked file-storage key, and writes are atomic.

use crypto::ed25519;
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path};

/// The fixed governance roles available in an Enclave in v1.
///
/// Roles deliberately grant governance capability only. They do not carry a
/// channel, key, or visibility grant: a member can read a channel only through
/// that channel's separate membership record and its corresponding keys.
/// Custom roles are intentionally not representable in this schema.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpaceRole {
    Member,
    Moderator,
    Admin,
    /// Compatibility role for signed membership events created before the
    /// governance vocabulary standardized on `Admin`.
    Owner,
}

impl SpaceRole {
    /// Every role that may be persisted or accepted from another client.
    pub const ALL: [Self; 3] = [Self::Member, Self::Moderator, Self::Admin];

    fn to_wire(self) -> u8 {
        match self {
            Self::Member => 1,
            Self::Moderator => 2,
            Self::Admin | Self::Owner => 3,
        }
    }
}

/// A governance action that a role may request.
///
/// This intentionally has no read or channel-visibility variant. Permission
/// evaluation is introduced by T21-F2; it must combine a role with a distinct
/// channel-membership/key-possession check rather than treat a role as access.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpaceGovernanceCapability {
    ModerateMembers,
    ManageRoles,
    ManageChannels,
}

impl SpaceRole {
    /// The governance capabilities associated with this fixed role.
    ///
    /// These are client-request capabilities, not cryptographic enforcement;
    /// key possession remains the only basis for channel visibility.
    pub const fn governance_capabilities(self) -> &'static [SpaceGovernanceCapability] {
        match self {
            Self::Member => &[],
            Self::Moderator => &[SpaceGovernanceCapability::ModerateMembers],
            Self::Admin | Self::Owner => &[
                SpaceGovernanceCapability::ModerateMembers,
                SpaceGovernanceCapability::ManageRoles,
                SpaceGovernanceCapability::ManageChannels,
            ],
        }
    }
}

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

const MEMBERSHIP_EVENT_DOMAIN: &[u8] = b"OSL/space-membership-event/v1";

/// The membership operation authorized by a roster event.
///
/// The expected actor identity is intentionally supplied by the local roster
/// when the event is accepted. A received event never gets to nominate the
/// public key that verifies it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MembershipEventKind {
    Create,
    Invite,
    Join,
    Leave,
    Remove,
    RoleChange(SpaceRole),
}

impl MembershipEventKind {
    fn wire_parts(self) -> (u8, u8) {
        match self {
            Self::Create => (1, 0),
            Self::Invite => (2, 0),
            Self::Join => (3, 0),
            Self::Leave => (4, 0),
            Self::Remove => (5, 0),
            Self::RoleChange(role) => (6, role.to_wire()),
        }
    }
}

/// One membership transition, signed by the authorized actor's identity key.
///
/// `subject` is the member affected by the transition. For create, join, and
/// leave it must be the signing actor; that invariant prevents a member from
/// making another identity appear to join or leave.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SignedMembershipEvent {
    pub space_id: SpaceId,
    pub epoch: SpaceEpoch,
    pub kind: MembershipEventKind,
    pub subject: ed25519::PublicKey,
    signature: [u8; ed25519::SIGNATURE_SIZE],
}

impl SignedMembershipEvent {
    /// Signs the exact membership transition with the actor's identity key.
    pub fn sign(
        space_id: SpaceId,
        epoch: SpaceEpoch,
        kind: MembershipEventKind,
        subject: ed25519::PublicKey,
        actor: &ed25519::SecretKey,
    ) -> Self {
        let mut event = Self {
            space_id,
            epoch,
            kind,
            subject,
            signature: [0; ed25519::SIGNATURE_SIZE],
        };
        event.signature = *ed25519::sign(actor, &event.signing_bytes()).as_bytes();
        event
    }

    /// Verifies the event against the actor selected from the local roster.
    ///
    /// The identity key is caller-owned roster state, not wire metadata. This
    /// keeps an attacker from pairing a valid signature from their own key
    /// with an event claiming it came from an authorized actor.
    pub fn verify(&self, actor: &ed25519::PublicKey) -> Result<(), MembershipEventError> {
        if matches!(
            self.kind,
            MembershipEventKind::Create | MembershipEventKind::Join | MembershipEventKind::Leave
        ) && self.subject != *actor
        {
            return Err(MembershipEventError::ActorSubjectMismatch);
        }
        let signature = ed25519::Signature::from_bytes(self.signature);
        match ed25519::verify(actor, &self.signing_bytes(), &signature) {
            Ok(true) => Ok(()),
            Ok(false) | Err(_) => Err(MembershipEventError::InvalidSignature),
        }
    }

    fn signing_bytes(&self) -> Vec<u8> {
        let (kind, role) = self.kind.wire_parts();
        let mut bytes = Vec::with_capacity(MEMBERSHIP_EVENT_DOMAIN.len() + 32 + 8 + 2 + 32);
        bytes.extend_from_slice(MEMBERSHIP_EVENT_DOMAIN);
        bytes.extend_from_slice(self.space_id.as_bytes());
        bytes.extend_from_slice(&self.epoch.get().to_be_bytes());
        bytes.push(kind);
        bytes.push(role);
        bytes.extend_from_slice(self.subject.as_bytes());
        bytes
    }
}

/// The replay-safe, ordered event history for one locally-held Space roster.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MembershipEventLog {
    space_id: SpaceId,
    epoch: SpaceEpoch,
    events: Vec<SignedMembershipEvent>,
}

impl MembershipEventLog {
    pub fn new(space_id: SpaceId) -> Self {
        Self {
            space_id,
            epoch: SpaceEpoch::INITIAL,
            events: Vec::new(),
        }
    }

    pub fn epoch(&self) -> SpaceEpoch {
        self.epoch
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

impl MembershipEventLog {
    pub fn events(&self) -> &[SignedMembershipEvent] {
        &self.events
    }

    /// Verifies and records one transition. The only accepted new epoch is
    /// exactly the successor of the local high-water mark; a gap is unsafe
    /// because it could hide an intervening removal or key rotation.
    pub fn apply(
        &mut self,
        event: SignedMembershipEvent,
        actor: &ed25519::PublicKey,
    ) -> Result<MembershipEventApply, MembershipEventError> {
        if event.space_id != self.space_id {
            return Err(MembershipEventError::WrongSpace);
        }
        event.verify(actor)?;

        if event.epoch <= self.epoch {
            return if self.events.iter().any(|known| known == &event) {
                Ok(MembershipEventApply::Duplicate)
            } else {
                Err(MembershipEventError::StaleOrConflictingEpoch {
                    current: self.epoch,
                    received: event.epoch,
                })
            };
        }

        let expected = self
            .epoch
            .advance()
            .map_err(|_| MembershipEventError::EpochExhausted)?;
        if event.epoch != expected {
            return Err(MembershipEventError::EpochGap {
                expected,
                received: event.epoch,
            });
        }
        self.epoch = event.epoch;
        self.events.push(event);
        Ok(MembershipEventApply::Applied)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MembershipEventApply {
    Applied,
    Duplicate,
}

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum MembershipEventError {
    #[error("membership event signature is invalid")]
    InvalidSignature,
    #[error("self-membership event subject does not match its actor")]
    ActorSubjectMismatch,
    #[error("membership event belongs to a different space")]
    WrongSpace,
    #[error(
        "membership event epoch {received:?} is stale or conflicts with local epoch {current:?}"
    )]
    StaleOrConflictingEpoch {
        current: SpaceEpoch,
        received: SpaceEpoch,
    },
    #[error("membership event epoch gap: expected {expected:?}, received {received:?}")]
    EpochGap {
        expected: SpaceEpoch,
        received: SpaceEpoch,
    },
    #[error("membership event epoch is exhausted")]
    EpochExhausted,
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

    #[test]
    fn membership_events_are_signed_ordered_idempotent_and_replay_proof() {
        let space = SpaceId::generate();
        let (owner_secret, owner_public) = ed25519::generate_keypair();
        let (_member_secret, member_public) = ed25519::generate_keypair();
        let mut log = MembershipEventLog::new(space);

        let create = SignedMembershipEvent::sign(
            space,
            SpaceEpoch(1),
            MembershipEventKind::Create,
            owner_public,
            &owner_secret,
        );
        assert_eq!(
            log.apply(create, &owner_public),
            Ok(MembershipEventApply::Applied)
        );
        assert_eq!(
            log.apply(create, &owner_public),
            Ok(MembershipEventApply::Duplicate)
        );

        let invite = SignedMembershipEvent::sign(
            space,
            SpaceEpoch(2),
            MembershipEventKind::Invite,
            member_public,
            &owner_secret,
        );
        assert_eq!(
            log.apply(invite, &owner_public),
            Ok(MembershipEventApply::Applied)
        );

        let removal = SignedMembershipEvent::sign(
            space,
            SpaceEpoch(3),
            MembershipEventKind::Remove,
            member_public,
            &owner_secret,
        );
        assert_eq!(
            log.apply(removal, &owner_public),
            Ok(MembershipEventApply::Applied)
        );

        // A delayed, correctly signed invite cannot overwrite the later
        // removal. Treating all lower epochs as idempotent would re-admit the
        // member at a roster-state layer built on this log.
        assert_eq!(
            log.apply(invite, &owner_public),
            Err(MembershipEventError::StaleOrConflictingEpoch {
                current: SpaceEpoch(3),
                received: SpaceEpoch(2),
            })
        );
        assert_eq!(log.epoch(), SpaceEpoch(3));
        assert_eq!(log.events(), &[create, invite, removal]);
    }

    #[test]
    fn membership_events_fail_closed_on_a_gap_or_wrong_actor_signature() {
        let space = SpaceId::generate();
        let (owner_secret, owner_public) = ed25519::generate_keypair();
        let (_other_secret, other_public) = ed25519::generate_keypair();
        let mut log = MembershipEventLog::new(space);

        let gap = SignedMembershipEvent::sign(
            space,
            SpaceEpoch(2),
            MembershipEventKind::Create,
            owner_public,
            &owner_secret,
        );
        assert_eq!(
            log.apply(gap, &owner_public),
            Err(MembershipEventError::EpochGap {
                expected: SpaceEpoch(1),
                received: SpaceEpoch(2),
            })
        );

        let wrong_actor = SignedMembershipEvent::sign(
            space,
            SpaceEpoch(1),
            MembershipEventKind::Invite,
            owner_public,
            &owner_secret,
        );
        assert_eq!(
            log.apply(wrong_actor, &other_public),
            Err(MembershipEventError::InvalidSignature)
        );
        assert!(log.events().is_empty());
    }
}
