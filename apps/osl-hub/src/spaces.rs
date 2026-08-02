//! Local Space lifecycle state.
//!
//! A Space has members and roles.  It deliberately has no founder key: group
//! key distribution belongs to the sender-key lifecycle and is identical for
//! every active member.  The founding member is an administrator because of a
//! roster role, not because their account holds different cryptographic state.

use std::collections::{BTreeMap, BTreeSet};

use ipc::space_roster::SpaceChannelId;
use rand::{rngs::OsRng, RngCore};

use crate::burn_authorize::{
    authorize_remote_friend_burn, BurnAuthorizationError, BurnScopeBindings,
};
use crate::burn_contract::{BurnSignatureVerifier, RemoteFriendBurnPlan, RemoteFriendBurnRequest};

/// A sender-side cooldown for one Space.
///
/// This is intentionally local advisory state, not a relay policy.  The relay
/// cannot inspect encrypted Space content or prove that another client obeyed
/// the cooldown.  Upload-grant issuance and invite gating may reduce abuse at
/// different boundaries, but this type only makes the local client wait.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClientSlowmode {
    interval_ms: u64,
    last_sent_at_ms: Option<u64>,
}

/// The local client's decision about its next Space send.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientSlowmodeDecision {
    MaySend,
    Wait { retry_after_ms: u64 },
}

impl ClientSlowmode {
    /// Starts a local cooldown tracker. `interval_ms == 0` disables it.
    pub const fn new(interval_ms: u64) -> Self {
        Self {
            interval_ms,
            last_sent_at_ms: None,
        }
    }

    /// Returns this client's advisory decision at a monotonic timestamp.
    ///
    /// The timestamp is supplied by the caller so this small model never
    /// claims to have a server clock or durable enforcement authority.
    pub fn decision_at(&self, now_ms: u64) -> ClientSlowmodeDecision {
        let Some(last_sent_at_ms) = self.last_sent_at_ms else {
            return ClientSlowmodeDecision::MaySend;
        };

        let elapsed_ms = now_ms.saturating_sub(last_sent_at_ms);
        let retry_after_ms = self.interval_ms.saturating_sub(elapsed_ms);
        if retry_after_ms == 0 {
            ClientSlowmodeDecision::MaySend
        } else {
            ClientSlowmodeDecision::Wait { retry_after_ms }
        }
    }

    /// Records a send that this client actually performed.
    ///
    /// Callers must check [`Self::decision_at`] before sending.  Recording a
    /// timestamp neither prevents a modified client from sending nor tells any
    /// other client to wait.
    pub fn record_local_send_at(&mut self, now_ms: u64) {
        self.last_sent_at_ms = Some(now_ms);
    }
}

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
    channel_key_domains: BTreeMap<SpaceChannelId, ChannelKeyDomain>,
}

/// The locally held key-domain boundary for one channel.
///
/// A domain is minted once per channel, never once per Space.  The key bytes
/// intentionally have no accessor: callers obtain the recipient set to drive
/// the established sender-key distribution path, and must not substitute a
/// Space-wide recipient list.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChannelKeyDomain {
    domain_id: [u8; 32],
    recipients: BTreeSet<SpaceMemberId>,
}

impl ChannelKeyDomain {
    fn mint(recipients: BTreeSet<SpaceMemberId>) -> Self {
        let mut domain_id = [0_u8; 32];
        OsRng.fill_bytes(&mut domain_id);
        Self {
            domain_id,
            recipients,
        }
    }

    /// A non-secret commitment useful for binding a sender-key state to this
    /// exact channel domain. It must differ for separately created channels.
    pub const fn domain_id(&self) -> &[u8; 32] {
        &self.domain_id
    }

    pub fn recipients(&self) -> impl ExactSizeIterator<Item = SpaceMemberId> + '_ {
        self.recipients.iter().copied()
    }

    pub fn admits(&self, member: SpaceMemberId) -> bool {
        self.recipients.contains(&member)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChannelKeyDomainError {
    UnknownSpaceMember,
    ChannelAlreadyHasKeyDomain,
}

/// Creation can fail only when the supplied founder identity is invalid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreateSpaceError {
    EmptyFounderIdentity,
}

/// A join cannot add a recipient to any current sender-key epoch.  The
/// caller supplies T18-C4's key-rotation operation; this boundary only makes
/// its ordering non-optional for Space admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JoinSpaceError {
    EmptyJoinerIdentity,
    AlreadyMember,
    RotationFailed(String),
}

/// Admit a consumed, valid invite only after the group key has rotated.
///
/// `rotate` must distribute a fresh epoch to the roster that existed before
/// this call.  If it fails, the joiner is absent from the roster and receives
/// neither a historic nor a current key.  This is the Space call site for the
/// rotate-before-admit ordering owned by T18-C4.
pub fn admit_join_after_rotation<F>(
    space: &mut Space,
    joiner: SpaceMemberId,
    rotate: F,
) -> Result<(), JoinSpaceError>
where
    F: FnOnce(&Space) -> Result<(), String>,
{
    if joiner.0.iter().all(|byte| *byte == 0) {
        return Err(JoinSpaceError::EmptyJoinerIdentity);
    }
    if space.members.contains_key(&joiner) {
        return Err(JoinSpaceError::AlreadyMember);
    }
    rotate(space).map_err(JoinSpaceError::RotationFailed)?;
    space.members.insert(joiner, SpaceRole::Member);
    Ok(())
}

/// The one server-side effect of an administrative deletion request.
///
/// This does not say anything about the copies held by Space members.  The
/// server can confirm its own blob delete; each member's local deletion is a
/// distinct, independently acknowledged request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServerBlobDeleteRequestState {
    Queued,
    Confirmed,
}

/// Honest, count-only status for an admin's delete-for-everyone request.
///
/// The variants intentionally retain `Request`: even after the server blob is
/// confirmed gone, member copies remain independent instructions whose absent
/// acknowledgements are `Unconfirmed`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminDeleteForEveryoneRequestStatus {
    RequestQueued { member_deletions_confirmed: usize },
    RequestServerBlobConfirmed { member_deletions_confirmed: usize },
}

impl AdminDeleteForEveryoneRequestStatus {
    /// Display copy for a UI that must never turn a queued request into a
    /// completed deletion.  The member result is a count, never a boolean or
    /// a denominator, so it does not expose member device totals.
    pub fn display_text(self) -> String {
        let confirmed = match self {
            Self::RequestQueued {
                member_deletions_confirmed,
            }
            | Self::RequestServerBlobConfirmed {
                member_deletions_confirmed,
            } => member_deletions_confirmed,
        };
        let member_word = if confirmed == 1 { "member" } else { "members" };
        let server_text = match self {
            Self::RequestQueued { .. } => {
                "Delete request queued; server blob deletion is unconfirmed"
            }
            Self::RequestServerBlobConfirmed { .. } => {
                "Delete request; server blob deletion confirmed"
            }
        };
        format!(
            "{server_text}. {confirmed} {member_word} confirmed deletion; all other member deletions are Unconfirmed."
        )
    }
}

/// A planned admin delete-for-everyone action.  It is a request, not a claim
/// that any member copy has disappeared.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminDeleteForEveryoneRequest {
    remote_burn_plan: RemoteFriendBurnPlan,
    pending_member_deletions: BTreeSet<SpaceMemberId>,
    confirmed_member_deletions: BTreeSet<SpaceMemberId>,
    server_blob_delete: ServerBlobDeleteRequestState,
}

impl AdminDeleteForEveryoneRequest {
    /// The outgoing authenticated instructions.  These are queued requests;
    /// their presence is not a member-deletion acknowledgement.
    pub fn peer_deletion_instruction_count(&self) -> usize {
        self.remote_burn_plan.notices.len()
    }

    pub const fn server_blob_delete_state(&self) -> ServerBlobDeleteRequestState {
        self.server_blob_delete
    }

    pub fn status(&self) -> AdminDeleteForEveryoneRequestStatus {
        let member_deletions_confirmed = self.confirmed_member_deletions.len();
        match self.server_blob_delete {
            ServerBlobDeleteRequestState::Queued => {
                AdminDeleteForEveryoneRequestStatus::RequestQueued {
                    member_deletions_confirmed,
                }
            }
            ServerBlobDeleteRequestState::Confirmed => {
                AdminDeleteForEveryoneRequestStatus::RequestServerBlobConfirmed {
                    member_deletions_confirmed,
                }
            }
        }
    }

    /// Records only the relay's confirmation of its own blob deletion.
    pub fn confirm_server_blob_deletion(&mut self) {
        self.server_blob_delete = ServerBlobDeleteRequestState::Confirmed;
    }

    /// Records an individual member acknowledgement.  An absent acknowledgement
    /// remains `Unconfirmed`; it is neither compliance nor refusal.
    pub fn record_member_deletion_acknowledgement(&mut self, member: SpaceMemberId) -> bool {
        if !self.pending_member_deletions.contains(&member) {
            return false;
        }
        self.confirmed_member_deletions.insert(member)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum AdminDeleteForEveryoneError {
    NotAnAdmin,
    NoOtherMembers,
    RequestTargetsDoNotMatchSpaceMembership,
    Authorization(BurnAuthorizationError),
}

/// Authorize an admin's delete-for-everyone request and queue one authenticated
/// delete instruction for every other current Space member.
///
/// This deliberately delegates cryptographic scope, issuer, consent, and
/// signature checks to T2's [`authorize_remote_friend_burn`] boundary.  Space
/// moderation adds only the roster-role and current-membership checks; it does
/// not create another burn authorization system.
pub fn request_admin_delete_for_everyone(
    space: &Space,
    requesting_admin: SpaceMemberId,
    bindings: BurnScopeBindings<'_>,
    local_identity_commitment: [u8; 32],
    request: &RemoteFriendBurnRequest,
    revoked_grants: &BTreeMap<[u8; 16], u64>,
    verifier: &impl BurnSignatureVerifier,
) -> Result<AdminDeleteForEveryoneRequest, AdminDeleteForEveryoneError> {
    if space.role_of(requesting_admin) != Some(SpaceRole::Admin) {
        return Err(AdminDeleteForEveryoneError::NotAnAdmin);
    }

    let pending_member_deletions: BTreeSet<_> = space
        .members
        .keys()
        .copied()
        .filter(|member| *member != requesting_admin)
        .collect();
    if pending_member_deletions.is_empty() {
        return Err(AdminDeleteForEveryoneError::NoOtherMembers);
    }
    let request_targets: BTreeSet<_> = request
        .affected_identity_commitments
        .iter()
        .copied()
        .map(SpaceMemberId::from_bytes)
        .collect();
    if request_targets != pending_member_deletions
        || request.affected_identity_commitments.len() != request_targets.len()
    {
        return Err(AdminDeleteForEveryoneError::RequestTargetsDoNotMatchSpaceMembership);
    }

    let remote_burn_plan = authorize_remote_friend_burn(
        bindings,
        local_identity_commitment,
        request,
        revoked_grants,
        verifier,
    )
    .map_err(AdminDeleteForEveryoneError::Authorization)?;

    Ok(AdminDeleteForEveryoneRequest {
        remote_burn_plan,
        pending_member_deletions,
        confirmed_member_deletions: BTreeSet::new(),
        server_blob_delete: ServerBlobDeleteRequestState::Queued,
    })
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
        channel_key_domains: BTreeMap::new(),
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

    /// Mints an independent key domain for a channel and scopes it to the
    /// supplied current members. A member absent from this list must not be
    /// given that channel's sender-key material.
    pub fn create_channel_key_domain(
        &mut self,
        channel: SpaceChannelId,
        recipients: impl IntoIterator<Item = SpaceMemberId>,
    ) -> Result<(), ChannelKeyDomainError> {
        if self.channel_key_domains.contains_key(&channel) {
            return Err(ChannelKeyDomainError::ChannelAlreadyHasKeyDomain);
        }
        let recipients: BTreeSet<_> = recipients.into_iter().collect();
        if recipients.iter().any(|member| !self.members.contains_key(member)) {
            return Err(ChannelKeyDomainError::UnknownSpaceMember);
        }
        self.channel_key_domains
            .insert(channel, ChannelKeyDomain::mint(recipients));
        Ok(())
    }

    pub fn channel_key_domain(&self, channel: SpaceChannelId) -> Option<&ChannelKeyDomain> {
        self.channel_key_domains.get(&channel)
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

    #[test]
    fn t21_t13_rotation_completes_before_joiner_is_admitted() {
        let founder = member(1);
        let joiner = member(2);
        let mut space = create_space(SpaceId::from_bytes([7; 20]), founder).unwrap();
        let mut rotation_saw_joiner = false;

        admit_join_after_rotation(&mut space, joiner, |before_admission| {
            rotation_saw_joiner = before_admission.role_of(joiner).is_some();
            Ok(())
        })
        .unwrap();
        assert!(!rotation_saw_joiner, "rotation must exclude the joiner");
        assert_eq!(space.role_of(joiner), Some(SpaceRole::Member));

        let failed = member(3);
        assert_eq!(
            admit_join_after_rotation(&mut space, failed, |_| Err("rotation unavailable".into())),
            Err(JoinSpaceError::RotationFailed("rotation unavailable".into()))
        );
        assert_eq!(space.role_of(failed), None, "a failed rotation admits nobody");
    }

    #[test]
    fn t21_t26_each_channel_has_its_own_key_domain_and_recipient_boundary() {
        let alice = member(1);
        let bob = member(2);
        let carol = member(3);
        let mut space = create_space(SpaceId::from_bytes([8; 20]), alice).unwrap();
        space.members.insert(bob, SpaceRole::Member);
        space.members.insert(carol, SpaceRole::Member);
        let public = SpaceChannelId::from_bytes([1; SpaceChannelId::LENGTH]);
        let private = SpaceChannelId::from_bytes([2; SpaceChannelId::LENGTH]);

        space.create_channel_key_domain(public, [alice, bob, carol]).unwrap();
        space.create_channel_key_domain(private, [alice, bob]).unwrap();

        let public_domain = space.channel_key_domain(public).unwrap();
        let private_domain = space.channel_key_domain(private).unwrap();
        assert_ne!(public_domain.domain_id(), private_domain.domain_id());
        assert!(public_domain.admits(carol));
        assert!(!private_domain.admits(carol), "a non-member of #private receives no key domain");
        assert_eq!(private_domain.recipients().collect::<Vec<_>>(), vec![alice, bob]);
    }

    #[test]
    fn t21_t37_admin_delete_stays_a_request_until_each_member_acknowledges() {
        let bob = member(2);
        let carol = member(3);
        let mut request = AdminDeleteForEveryoneRequest {
            remote_burn_plan: RemoteFriendBurnPlan {
                burn_id: [9; 32],
                notices: vec![],
            },
            pending_member_deletions: BTreeSet::from([bob, carol]),
            confirmed_member_deletions: BTreeSet::new(),
            server_blob_delete: ServerBlobDeleteRequestState::Queued,
        };

        assert_eq!(request.peer_deletion_instruction_count(), 0);
        assert_eq!(
            request.status(),
            AdminDeleteForEveryoneRequestStatus::RequestQueued {
                member_deletions_confirmed: 0
            }
        );
        assert_eq!(
            request.status().display_text(),
            "Delete request queued; server blob deletion is unconfirmed. 0 members confirmed deletion; all other member deletions are Unconfirmed."
        );

        request.confirm_server_blob_deletion();
        assert!(request.record_member_deletion_acknowledgement(bob));
        assert!(!request.record_member_deletion_acknowledgement(member(4)));
        assert_eq!(
            request.status(),
            AdminDeleteForEveryoneRequestStatus::RequestServerBlobConfirmed {
                member_deletions_confirmed: 1
            },
            "a server confirmation and one acknowledgement cannot complete the other member request"
        );
        assert_eq!(
            request.status().display_text(),
            "Delete request; server blob deletion confirmed. 1 member confirmed deletion; all other member deletions are Unconfirmed."
        );
    }

    #[test]
    fn t21_t39_slowmode_is_a_local_advisory_not_relay_enforcement() {
        let mut alice_client = ClientSlowmode::new(1_000);

        assert_eq!(
            alice_client.decision_at(10_000),
            ClientSlowmodeDecision::MaySend
        );
        alice_client.record_local_send_at(10_000);
        assert_eq!(
            alice_client.decision_at(10_250),
            ClientSlowmodeDecision::Wait {
                retry_after_ms: 750
            }
        );
        assert_eq!(
            alice_client.decision_at(11_000),
            ClientSlowmodeDecision::MaySend
        );

        // A fresh or modified client has no shared relay-side cooldown state.
        // That limitation is why callers must label this as client-side advice.
        let modified_client = ClientSlowmode::new(1_000);
        assert_eq!(
            modified_client.decision_at(10_250),
            ClientSlowmodeDecision::MaySend
        );
    }
}
