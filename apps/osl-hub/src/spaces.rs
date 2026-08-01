//! Local Space lifecycle state.
//!
//! A Space has members and roles.  It deliberately has no founder key: group
//! key distribution belongs to the sender-key lifecycle and is identical for
//! every active member.  The founding member is an administrator because of a
//! roster role, not because their account holds different cryptographic state.

use std::collections::BTreeMap;

/// Opaque, client-generated Space identifier.  Construction is intentionally
/// separate from creation: the roster layer owns CSPRNG generation.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SpaceId([u8; 20]);

impl SpaceId {
    pub const fn from_bytes(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 20] {
        &self.0
    }
}

/// Opaque identity of one Space member.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SpaceMemberId([u8; 32]);

impl SpaceMemberId {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

/// Authority in the roster.  This changes moderation authority only; it is
/// never an input to group-key distribution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpaceRole {
    Admin,
    Member,
}

/// One locally authoritative Space roster.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Space {
    id: SpaceId,
    members: BTreeMap<SpaceMemberId, SpaceRole>,
}

/// Creation can fail only when the supplied founder identity is invalid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreateSpaceError {
    EmptyFounderIdentity,
}

/// Creates a Space with its founder as an administrator.
///
/// The returned object contains no key material.  In particular, the founder
/// is an admin solely by [`SpaceRole`], so another member can hold the same
/// group sender-key epoch material when admitted by the membership lifecycle.
pub fn create_space(id: SpaceId, founder: SpaceMemberId) -> Result<Space, CreateSpaceError> {
    if founder.0.iter().all(|byte| *byte == 0) {
        return Err(CreateSpaceError::EmptyFounderIdentity);
    }

    Ok(Space {
        id,
        members: BTreeMap::from([(founder, SpaceRole::Admin)]),
    })
}

impl Space {
    pub const fn id(&self) -> SpaceId {
        self.id
    }

    pub fn role_of(&self, member: SpaceMemberId) -> Option<SpaceRole> {
        self.members.get(&member).copied()
    }

    /// The identities that receive the current sender-key epoch material.
    ///
    /// Roles deliberately do not affect this list: admins and ordinary members
    /// are equal cryptographic participants.  The caller supplies the same
    /// epoch material to each member through the established key lifecycle.
    pub fn key_recipients(&self) -> impl ExactSizeIterator<Item = SpaceMemberId> + '_ {
        self.members.keys().copied()
    }

    /// Whether no current member has the Admin role.
    ///
    /// This is a normal, usable Space state.  It occurs if the last admin
    /// leaves; callers must not reject a membership event merely to prevent it.
    pub fn is_unowned(&self) -> bool {
        !self.members.values().any(|role| *role == SpaceRole::Admin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(value: u8) -> SpaceMemberId {
        SpaceMemberId::from_bytes([value; 32])
    }

    #[test]
    fn founder_is_an_admin_role_and_not_a_unique_key_recipient() {
        let founder = member(1);
        let another_member = member(2);
        let mut space = create_space(SpaceId::from_bytes([9; 20]), founder).unwrap();

        // This represents a completed admission event.  Key recipients must
        // remain role-independent once the member is on the authoritative roster.
        space.members.insert(another_member, SpaceRole::Member);

        assert_eq!(space.role_of(founder), Some(SpaceRole::Admin));
        assert_eq!(space.role_of(another_member), Some(SpaceRole::Member));
        assert_eq!(
            space.key_recipients().collect::<Vec<_>>(),
            vec![founder, another_member],
            "every active member receives the same epoch material; founder status is only a role"
        );
    }

    #[test]
    fn a_space_with_no_admin_is_valid_after_the_last_admin_leaves() {
        let founder = member(3);
        let mut space = create_space(SpaceId::from_bytes([4; 20]), founder).unwrap();

        // The leave event is authored by the C8 lifecycle.  This state model
        // intentionally accepts its resulting empty roster.
        space.members.remove(&founder);

        assert!(space.is_unowned());
        assert!(space.key_recipients().next().is_none());
    }

    #[test]
    fn creation_refuses_an_empty_founder_identity() {
        assert_eq!(
            create_space(SpaceId::from_bytes([1; 20]), member(0)),
            Err(CreateSpaceError::EmptyFounderIdentity)
        );
    }
}
