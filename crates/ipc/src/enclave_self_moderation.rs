//! Enclave-scoped roles, the KEY/RELAY/TRUST permission catalogue, and the
//! signed instructions an Enclave's own people use to govern their own space.
//!
//! The 11 August 2026 owner ruling refines S5: an Enclave may govern itself,
//! and OSL may never govern anybody. This module is the whole of the allowed
//! half and none of the forbidden half.
//!
//! ## What ships here
//!
//! - **Custom roles**, persisted per Enclave. Authority travels with a role
//!   *id*; the name is a label the product renders and may change at any time.
//! - **A permission catalogue**, persisted per Enclave, where every permission
//!   carries the enforcement class that actually makes it true:
//!   [`EnforcementClass::Key`] (key possession decides), [`EnforcementClass::Relay`]
//!   (the relay refuses to carry it) and [`EnforcementClass::Trust`] (honest
//!   member clients honour it, exactly as burn works).
//! - **One resolver** — [`EnclaveGovernance::resolve`] — which answers every
//!   permission question for every member. There is no second decision path:
//!   the instruction admission code below calls this same function.
//! - **Signed instructions**: remove a member, mute them, restrict them to
//!   named channels, revoke their post or invite ability, and delete one
//!   message inside this Enclave. Every instruction binds the enclave, the
//!   current epoch, the actor's role/permission version, the actor key, the
//!   invoked role, the action, the target and a nonce, so a stale, replayed,
//!   forged, unauthorized or cross-enclave instruction cannot be honoured.
//!
//! ## What deliberately does not ship here
//!
//! Nothing in this module reports to OSL, bans or suspends an account,
//! collects evidence, opens a review or appeal, operates a moderator tool,
//! writes a global block list, scores reputation, or gives a server any path
//! to content. Removal, mute, restriction, revocation and deletion are scoped
//! to one Enclave and change nothing about the target's account, their direct
//! messages, or any other Enclave — [`IsolationOracle`] is how a check proves
//! that byte for byte. The forbidden half is enumerated, machine-readably, in
//! [`crate::central_moderation_needles`].
//!
//! ## Removal
//!
//! Removal here is the governance half: it refuses an unauthorized actor,
//! advances the Enclave to a fresh epoch, and drops the member from the
//! roster. The cryptographic half — the fresh epoch key wrapped independently
//! for every remaining member, with progress observed from those wraps rather
//! than from a counter — is [`crate::enclave_removal`], whose measured
//! progress this module's callers drive to completion.

use crate::enclave_layout::{member_id_for_key, ChannelId, EnclaveLayoutId, MessageId, RoleId};
use crate::space_roster::SpaceMemberId;
use crypto::aead::{self, Key, Nonce};
use crypto::ed25519::{self, PublicKey, SecretKey, Signature};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// One Enclave's identity. Governance and layout describe the same Enclave, so
/// they deliberately share one identifier rather than inventing a second.
pub type EnclaveId = EnclaveLayoutId;

/// Domain separator for the signing bytes of a governance instruction.
pub const INSTRUCTION_DOMAIN: &[u8] = b"osl.enclave.self-moderation.instruction.v1";

/// Domain separator for the relay envelope's additional authenticated data.
pub const RELAY_ENVELOPE_DOMAIN: &[u8] = b"osl.enclave.self-moderation.relay/v1";

/// On-disk schema version for persisted governance state.
pub const GOVERNANCE_STATE_VERSION: u8 = 1;

/// The number of independently bound fields in an instruction's signing bytes.
/// A check that does not exercise every one of them has not proved binding.
pub const BOUND_INSTRUCTION_FIELDS: [&str; 8] = [
    "enclave",
    "epoch",
    "authority_version",
    "actor",
    "actor_role",
    "action",
    "target",
    "nonce",
];

/// Everything the relay can read about an instruction it carries. None of
/// these is content, an actor, a target or an action.
pub const RELAY_READABLE_FIELDS: [&str; 3] = ["routing_tag", "nonce", "ciphertext_length"];

// ---------------------------------------------------------------------------
// Permission catalogue
// ---------------------------------------------------------------------------

/// How a permission is actually enforced.
///
/// This is not decoration. It is the honest answer to "what stops someone who
/// ignores the rule", and the product shows it beside every permission.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EnforcementClass {
    /// Key possession decides. Somebody without the key cannot read, whatever
    /// their client does.
    Key,
    /// The relay refuses to carry it. The relay still cannot read content.
    Relay,
    /// Honest member clients honour it, exactly as burn works. A dishonest
    /// client that already holds the plaintext is not stopped by anything, and
    /// the product says so rather than pretending otherwise.
    Trust,
}

impl EnforcementClass {
    /// Every class the product ships. A check must exercise all of them.
    pub const ALL: [Self; 3] = [Self::Key, Self::Relay, Self::Trust];

    pub const fn token(self) -> &'static str {
        match self {
            Self::Key => "KEY",
            Self::Relay => "RELAY",
            Self::Trust => "TRUST",
        }
    }
}

/// One permission in an Enclave's catalogue.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Permission {
    /// Read a channel in this Enclave.
    ReadChannel,
    /// Post a message in a channel of this Enclave.
    PostMessage,
    /// Mint an invite for this Enclave.
    CreateInvite,
    /// Remove a member from this Enclave. Removal re-keys the Enclave, which
    /// is why this is a KEY permission and not a request.
    RemoveMember,
    /// Mute a member in this Enclave.
    MuteMember,
    /// Restrict a member to a named set of this Enclave's channels.
    RestrictMemberChannels,
    /// Revoke a member's ability to post in this Enclave.
    RevokeMemberPost,
    /// Revoke a member's ability to invite to this Enclave.
    RevokeMemberInvite,
    /// Issue a signed instruction to delete one message in this Enclave.
    DeleteMessage,
}

impl Permission {
    /// The whole shipped catalogue. Every entry is enclave-scoped; none of
    /// them reaches an account, a direct message or another Enclave.
    pub const CATALOGUE: [Self; 9] = [
        Self::ReadChannel,
        Self::PostMessage,
        Self::CreateInvite,
        Self::RemoveMember,
        Self::MuteMember,
        Self::RestrictMemberChannels,
        Self::RevokeMemberPost,
        Self::RevokeMemberInvite,
        Self::DeleteMessage,
    ];

    pub const fn token(self) -> &'static str {
        match self {
            Self::ReadChannel => "read-channel",
            Self::PostMessage => "post-message",
            Self::CreateInvite => "create-invite",
            Self::RemoveMember => "remove-member",
            Self::MuteMember => "mute-member",
            Self::RestrictMemberChannels => "restrict-member-channels",
            Self::RevokeMemberPost => "revoke-member-post",
            Self::RevokeMemberInvite => "revoke-member-invite",
            Self::DeleteMessage => "delete-message",
        }
    }

    /// The class that actually enforces this permission.
    pub const fn class(self) -> EnforcementClass {
        match self {
            // Reading and posting need the channel's keys; removal rotates
            // them. All three are decided by key possession.
            Self::ReadChannel | Self::PostMessage | Self::RemoveMember => EnforcementClass::Key,
            // Invites are tokens the relay accepts or refuses; it never reads
            // what is behind them.
            Self::CreateInvite | Self::RevokeMemberInvite => EnforcementClass::Relay,
            // Mute, restriction, post revocation and deletion are honoured by
            // honest member clients, exactly as burn is.
            Self::MuteMember
            | Self::RestrictMemberChannels
            | Self::RevokeMemberPost
            | Self::DeleteMessage => EnforcementClass::Trust,
        }
    }

    pub fn from_token(token: &str) -> Option<Self> {
        Self::CATALOGUE
            .into_iter()
            .find(|permission| permission.token() == token)
    }
}

/// One catalogued permission as persisted for one Enclave.
///
/// The class is stored, not recomputed, so an Enclave whose stored catalogue
/// loses a class fails to load rather than silently resolving as if the
/// permission were enforced some other way.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CataloguedPermission {
    pub permission: Permission,
    pub class: EnforcementClass,
}

/// A custom role. Every field is required: a stored role that has lost its id,
/// its name or its grants is refused rather than loaded with a default.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CustomRole {
    pub id: RoleId,
    pub name: String,
    pub grants: BTreeSet<Permission>,
}

impl CustomRole {
    pub fn new(
        id: RoleId,
        name: impl Into<String>,
        grants: impl IntoIterator<Item = Permission>,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            grants: grants.into_iter().collect(),
        }
    }

    /// The enforcement classes this role can actually exercise.
    pub fn classes(&self) -> BTreeSet<EnforcementClass> {
        self.grants.iter().map(|grant| grant.class()).collect()
    }
}

/// Which roles one member holds. Stored as a sorted record rather than a map
/// so the persisted form is a plain, key-safe JSON array.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoleHolder {
    pub member: SpaceMemberId,
    pub roles: BTreeSet<RoleId>,
}

/// One member's channel restriction: the only channels of this Enclave they
/// may read or post in.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelRestriction {
    pub member: SpaceMemberId,
    pub allowed: BTreeSet<ChannelId>,
}

// ---------------------------------------------------------------------------
// The one resolver's vocabulary
// ---------------------------------------------------------------------------

/// What a permission question is being asked about.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PermissionScope {
    /// The Enclave as a whole.
    Enclave,
    /// One channel of this Enclave.
    Channel(ChannelId),
}

/// Why the resolver decided as it did. Every refusal names its cause so the
/// product can say what it declined instead of failing silently.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "reason")]
pub enum DecisionReason {
    NotAMember,
    NotInCatalogue,
    UnknownChannel,
    ChannelRestricted,
    Muted,
    PostRevoked,
    InviteRevoked,
    RoleGrant { role: RoleId },
    NoGrantingRole,
}

/// One resolved permission: the answer, the class that enforces it, the reason.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PermissionDecision {
    pub allowed: bool,
    pub class: EnforcementClass,
    pub reason: DecisionReason,
}

impl PermissionDecision {
    const fn new(allowed: bool, class: EnforcementClass, reason: DecisionReason) -> Self {
        Self {
            allowed,
            class,
            reason,
        }
    }
}

// ---------------------------------------------------------------------------
// Actions and signed instructions
// ---------------------------------------------------------------------------

/// One enclave-scoped governance action.
///
/// There is deliberately no variant that reaches an account, a direct message
/// or another Enclave, and no variant that asks OSL for anything.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "action")]
pub enum GovernanceAction {
    /// Remove the target from this Enclave. Honouring this advances the
    /// Enclave to a fresh epoch; the cryptographic re-key is
    /// [`crate::enclave_removal`].
    RemoveMember,
    /// Mute the target in this Enclave.
    MuteMember,
    /// Restrict the target to exactly these channels of this Enclave.
    RestrictToChannels { allowed: BTreeSet<ChannelId> },
    /// Revoke the target's ability to post in this Enclave.
    RevokePost,
    /// Revoke the target's ability to invite to this Enclave.
    RevokeInvite,
    /// Delete one message of this Enclave. Honest member clients honour this
    /// exactly as they honour burn: the body is destroyed, and a tombstone
    /// without a body is all that remains.
    DeleteMessage {
        channel: ChannelId,
        message: MessageId,
    },
}

impl GovernanceAction {
    /// The catalogue permission the actor must hold through the role they
    /// invoke. There is exactly one per action.
    pub const fn required_permission(&self) -> Permission {
        match self {
            Self::RemoveMember => Permission::RemoveMember,
            Self::MuteMember => Permission::MuteMember,
            Self::RestrictToChannels { .. } => Permission::RestrictMemberChannels,
            Self::RevokePost => Permission::RevokeMemberPost,
            Self::RevokeInvite => Permission::RevokeMemberInvite,
            Self::DeleteMessage { .. } => Permission::DeleteMessage,
        }
    }

    pub const fn token(&self) -> &'static str {
        match self {
            Self::RemoveMember => "remove-member",
            Self::MuteMember => "mute-member",
            Self::RestrictToChannels { .. } => "restrict-to-channels",
            Self::RevokePost => "revoke-post",
            Self::RevokeInvite => "revoke-invite",
            Self::DeleteMessage { .. } => "delete-message",
        }
    }

    /// Every action kind the product ships, for a check that must cover them.
    pub const KINDS: [&'static str; 6] = [
        "remove-member",
        "mute-member",
        "restrict-to-channels",
        "revoke-post",
        "revoke-invite",
        "delete-message",
    ];
}

/// One governance instruction, before it is signed.
///
/// Every field here is bound into the signing bytes. Nothing about an
/// instruction can be changed without invalidating its signature.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GovernanceInstruction {
    /// The Enclave this instruction is for. An instruction minted for one
    /// Enclave is refused by every other Enclave's clients.
    pub enclave: EnclaveId,
    /// The Enclave's epoch at the moment the instruction was minted.
    pub epoch: u64,
    /// The Enclave's role/permission version at the same moment.
    pub authority_version: u64,
    /// The actor's Ed25519 identity key.
    pub actor: [u8; 32],
    /// The role the actor invokes. The actor must hold it and it must grant
    /// the action's permission.
    pub actor_role: RoleId,
    pub action: GovernanceAction,
    /// The member the action is about.
    pub target: SpaceMemberId,
    /// Unique per instruction. A second delivery of the same nonce is a
    /// replay and changes nothing.
    pub nonce: [u8; 16],
}

impl GovernanceInstruction {
    /// The exact bytes an actor signs.
    ///
    /// Every one of [`BOUND_INSTRUCTION_FIELDS`] contributes, each behind its
    /// own length prefix so no two different instructions can produce the same
    /// bytes by running two fields together.
    pub fn signing_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(256);
        bytes.extend_from_slice(INSTRUCTION_DOMAIN);
        push_frame(&mut bytes, self.enclave.as_bytes());
        bytes.extend_from_slice(&self.epoch.to_be_bytes());
        bytes.extend_from_slice(&self.authority_version.to_be_bytes());
        push_frame(&mut bytes, &self.actor);
        push_frame(&mut bytes, self.actor_role.as_bytes());
        push_frame(
            &mut bytes,
            &serde_json::to_vec(&self.action).expect("governance actions always serialize"),
        );
        push_frame(&mut bytes, self.target.as_bytes());
        push_frame(&mut bytes, &self.nonce);
        bytes
    }

    /// The member the actor key resolves to.
    pub fn actor_member(&self) -> SpaceMemberId {
        member_id_for_key(&self.actor)
    }

    /// Signs this instruction with the actor's identity key.
    pub fn sign(self, secret: &SecretKey) -> SignedGovernanceInstruction {
        let signature = ed25519::sign(secret, &self.signing_bytes());
        SignedGovernanceInstruction {
            instruction: self,
            signature: *signature.as_bytes(),
        }
    }
}

/// A governance instruction with its actor's signature.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SignedGovernanceInstruction {
    pub instruction: GovernanceInstruction,
    #[serde(with = "signature_bytes")]
    pub signature: [u8; ed25519::SIGNATURE_SIZE],
}

impl SignedGovernanceInstruction {
    /// Whether the signature is this actor's over exactly these bytes.
    pub fn verify(&self) -> bool {
        ed25519::verify(
            &PublicKey::from_bytes(self.instruction.actor),
            &self.instruction.signing_bytes(),
            &Signature::from_bytes(self.signature),
        )
        .unwrap_or(false)
    }

    /// Seals this instruction for the relay.
    ///
    /// The relay receives a rotating routing tag, a nonce and a ciphertext. It
    /// cannot read the Enclave, the actor, the action, the target or anything
    /// else, and it has no key with which it ever could.
    pub fn seal_for_relay(
        &self,
        transport_key: &[u8; aead::KEY_SIZE],
        routing_tag: [u8; 16],
    ) -> Result<RelayEnvelope, GovernanceError> {
        let nonce: [u8; aead::NONCE_SIZE] = random_array()?;
        let plaintext = serde_json::to_vec(self)
            .map_err(|error| GovernanceError::Malformed(error.to_string()))?;
        let ciphertext = aead::seal(
            &Key::from_bytes(*transport_key),
            &Nonce::from_bytes(nonce),
            &relay_aad(&routing_tag),
            &plaintext,
        )
        .map_err(|_| GovernanceError::RelayAuthentication)?;
        Ok(RelayEnvelope {
            routing_tag,
            nonce,
            ciphertext,
        })
    }

    /// Opens a relay envelope. Only a member holding the transport key can.
    pub fn open_from_relay(
        envelope: &RelayEnvelope,
        transport_key: &[u8; aead::KEY_SIZE],
    ) -> Result<Self, GovernanceError> {
        let plaintext = aead::open(
            &Key::from_bytes(*transport_key),
            &Nonce::from_bytes(envelope.nonce),
            &relay_aad(&envelope.routing_tag),
            &envelope.ciphertext,
        )
        .map_err(|_| GovernanceError::RelayAuthentication)?;
        serde_json::from_slice(&plaintext)
            .map_err(|error| GovernanceError::Malformed(error.to_string()))
    }
}

/// What the relay actually carries.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RelayEnvelope {
    pub routing_tag: [u8; 16],
    #[serde(with = "nonce_bytes")]
    pub nonce: [u8; aead::NONCE_SIZE],
    pub ciphertext: Vec<u8>,
}

impl RelayEnvelope {
    /// Exactly what a relay may read, and nothing else.
    pub fn server_readable(&self) -> BTreeMap<&'static str, String> {
        BTreeMap::from([
            ("routing_tag", hex::encode(self.routing_tag)),
            ("nonce", hex::encode(self.nonce)),
            ("ciphertext_length", self.ciphertext.len().to_string()),
        ])
    }
}

// ---------------------------------------------------------------------------
// Persisted governance state
// ---------------------------------------------------------------------------

/// One Enclave's persisted roles, catalogue and enforcement state.
///
/// Every collection is a sorted vector or set, so two clients that folded the
/// same instructions serialize byte-identically.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EnclaveGovernance {
    version: u8,
    enclave: EnclaveId,
    epoch: u64,
    authority_version: u64,
    owner: SpaceMemberId,
    catalogue: Vec<CataloguedPermission>,
    roles: Vec<CustomRole>,
    holders: Vec<RoleHolder>,
    members: BTreeSet<SpaceMemberId>,
    channels: BTreeSet<ChannelId>,
    muted: BTreeSet<SpaceMemberId>,
    restrictions: Vec<ChannelRestriction>,
    post_revoked: BTreeSet<SpaceMemberId>,
    invite_revoked: BTreeSet<SpaceMemberId>,
    deleted_messages: BTreeSet<MessageId>,
    honoured_nonces: BTreeSet<[u8; 16]>,
}

impl EnclaveGovernance {
    /// Founds an Enclave with the whole shipped catalogue and one owner.
    pub fn found(enclave: EnclaveId, owner_key: &[u8; 32], epoch: u64) -> Self {
        let owner = member_id_for_key(owner_key);
        Self {
            version: GOVERNANCE_STATE_VERSION,
            enclave,
            epoch,
            authority_version: 1,
            owner,
            catalogue: Permission::CATALOGUE
                .into_iter()
                .map(|permission| CataloguedPermission {
                    permission,
                    class: permission.class(),
                })
                .collect(),
            roles: Vec::new(),
            holders: Vec::new(),
            members: BTreeSet::from([owner]),
            channels: BTreeSet::new(),
            muted: BTreeSet::new(),
            restrictions: Vec::new(),
            post_revoked: BTreeSet::new(),
            invite_revoked: BTreeSet::new(),
            deleted_messages: BTreeSet::new(),
            honoured_nonces: BTreeSet::new(),
        }
    }

    // ----- queries -------------------------------------------------------

    pub const fn enclave(&self) -> EnclaveId {
        self.enclave
    }

    pub const fn epoch(&self) -> u64 {
        self.epoch
    }

    pub const fn authority_version(&self) -> u64 {
        self.authority_version
    }

    pub const fn owner(&self) -> SpaceMemberId {
        self.owner
    }

    pub fn catalogue(&self) -> &[CataloguedPermission] {
        &self.catalogue
    }

    pub fn roles(&self) -> &[CustomRole] {
        &self.roles
    }

    pub fn role(&self, role: RoleId) -> Option<&CustomRole> {
        self.roles.iter().find(|existing| existing.id == role)
    }

    pub fn members(&self) -> &BTreeSet<SpaceMemberId> {
        &self.members
    }

    pub fn channels(&self) -> &BTreeSet<ChannelId> {
        &self.channels
    }

    pub fn is_member(&self, member: SpaceMemberId) -> bool {
        self.members.contains(&member)
    }

    pub fn is_muted(&self, member: SpaceMemberId) -> bool {
        self.muted.contains(&member)
    }

    pub fn restriction(&self, member: SpaceMemberId) -> Option<&BTreeSet<ChannelId>> {
        self.restrictions
            .iter()
            .find(|existing| existing.member == member)
            .map(|existing| &existing.allowed)
    }

    pub fn is_post_revoked(&self, member: SpaceMemberId) -> bool {
        self.post_revoked.contains(&member)
    }

    pub fn is_invite_revoked(&self, member: SpaceMemberId) -> bool {
        self.invite_revoked.contains(&member)
    }

    pub fn is_deleted(&self, message: MessageId) -> bool {
        self.deleted_messages.contains(&message)
    }

    pub fn deleted_messages(&self) -> &BTreeSet<MessageId> {
        &self.deleted_messages
    }

    pub fn roles_of(&self, member: SpaceMemberId) -> BTreeSet<RoleId> {
        self.holders
            .iter()
            .find(|holder| holder.member == member)
            .map(|holder| holder.roles.clone())
            .unwrap_or_default()
    }

    pub fn class_of(&self, permission: Permission) -> Option<EnforcementClass> {
        self.catalogue
            .iter()
            .find(|entry| entry.permission == permission)
            .map(|entry| entry.class)
    }

    /// The enforcement classes this Enclave's stored catalogue covers.
    pub fn catalogue_classes(&self) -> BTreeSet<EnforcementClass> {
        self.catalogue.iter().map(|entry| entry.class).collect()
    }

    // ----- role and catalogue administration -----------------------------

    /// Defines a custom role and advances the role/permission version.
    ///
    /// Advancing the version is what makes every already-minted instruction
    /// stale: an actor who was authorized under the old role set has to mint
    /// again under the new one.
    pub fn define_role(&mut self, role: CustomRole) -> Result<u64, GovernanceError> {
        if role.grants.is_empty() {
            return Err(GovernanceError::RoleGrantsEmpty);
        }
        if role.name.trim().is_empty() {
            return Err(GovernanceError::RoleNameEmpty);
        }
        if self.roles.iter().any(|existing| existing.id == role.id) {
            return Err(GovernanceError::DuplicateRole);
        }
        for grant in &role.grants {
            if self.class_of(*grant).is_none() {
                return Err(GovernanceError::PermissionNotCatalogued(grant.token()));
            }
        }
        self.roles.push(role);
        self.roles.sort();
        self.bump_authority_version()
    }

    /// Grants a role to a member and advances the role/permission version.
    pub fn grant_role(
        &mut self,
        member: SpaceMemberId,
        role: RoleId,
    ) -> Result<u64, GovernanceError> {
        if !self.members.contains(&member) {
            return Err(GovernanceError::NotAMember);
        }
        if self.role(role).is_none() {
            return Err(GovernanceError::UnknownRole);
        }
        match self
            .holders
            .iter_mut()
            .find(|holder| holder.member == member)
        {
            Some(holder) => {
                holder.roles.insert(role);
            }
            None => self.holders.push(RoleHolder {
                member,
                roles: BTreeSet::from([role]),
            }),
        }
        self.holders.sort();
        self.bump_authority_version()
    }

    /// Admits a member. Membership is not a role/permission change, so this
    /// deliberately does not advance the authority version.
    pub fn admit_member(&mut self, member: SpaceMemberId) -> Result<(), GovernanceError> {
        self.members.insert(member);
        Ok(())
    }

    /// Declares a channel of this Enclave. Restrictions may only name these.
    pub fn declare_channel(&mut self, channel: ChannelId) {
        self.channels.insert(channel);
    }

    fn bump_authority_version(&mut self) -> Result<u64, GovernanceError> {
        self.authority_version = self
            .authority_version
            .checked_add(1)
            .ok_or(GovernanceError::VersionExhausted)?;
        Ok(self.authority_version)
    }

    // ----- the one resolver ----------------------------------------------

    /// Resolves one member's permission, in one scope, for this Enclave.
    ///
    /// This is the only permission decision in the module. Instruction
    /// admission calls it too, so an actor cannot obtain through an
    /// instruction anything the resolver would refuse.
    ///
    /// Order, deterministic and independent of iteration order:
    /// 1. a non-member is refused outright;
    /// 2. a permission this Enclave does not catalogue is refused;
    /// 3. the target-side enforcement state — channel restriction, mute, post
    ///    revocation, invite revocation — refuses the member's own expression
    ///    permissions before any role is consulted;
    /// 4. otherwise the lowest role id the member holds that grants the
    ///    permission allows it;
    /// 5. otherwise it is refused for want of a granting role.
    pub fn resolve(
        &self,
        member: SpaceMemberId,
        permission: Permission,
        scope: PermissionScope,
    ) -> PermissionDecision {
        let fallback_class = permission.class();
        if !self.members.contains(&member) {
            return PermissionDecision::new(false, fallback_class, DecisionReason::NotAMember);
        }
        let Some(class) = self.class_of(permission) else {
            return PermissionDecision::new(false, fallback_class, DecisionReason::NotInCatalogue);
        };

        // Enforcement state applies to the permissions a member exercises for
        // themselves. It never silently widens an authority permission.
        match permission {
            Permission::ReadChannel | Permission::PostMessage => {
                if let PermissionScope::Channel(channel) = scope {
                    if !self.channels.contains(&channel) {
                        return PermissionDecision::new(
                            false,
                            class,
                            DecisionReason::UnknownChannel,
                        );
                    }
                    if let Some(allowed) = self.restriction(member) {
                        if !allowed.contains(&channel) {
                            return PermissionDecision::new(
                                false,
                                class,
                                DecisionReason::ChannelRestricted,
                            );
                        }
                    }
                }
                if permission == Permission::PostMessage {
                    if self.muted.contains(&member) {
                        return PermissionDecision::new(false, class, DecisionReason::Muted);
                    }
                    if self.post_revoked.contains(&member) {
                        return PermissionDecision::new(false, class, DecisionReason::PostRevoked);
                    }
                }
            }
            Permission::CreateInvite => {
                if self.invite_revoked.contains(&member) {
                    return PermissionDecision::new(false, class, DecisionReason::InviteRevoked);
                }
            }
            _ => {}
        }

        if let Some(role) = self.granting_role(member, permission) {
            return PermissionDecision::new(true, class, DecisionReason::RoleGrant { role });
        }
        PermissionDecision::new(false, class, DecisionReason::NoGrantingRole)
    }

    /// The lowest-id role the member holds that grants this permission.
    fn granting_role(&self, member: SpaceMemberId, permission: Permission) -> Option<RoleId> {
        self.roles_of(member).into_iter().find(|role| {
            self.role(*role)
                .is_some_and(|existing| existing.grants.contains(&permission))
        })
    }

    // ----- instruction minting and admission ------------------------------

    /// Mints an instruction bound to this Enclave's *current* epoch and
    /// role/permission version.
    ///
    /// This refuses before signing when the resolver would refuse, so a client
    /// cannot even produce something that looks locally authorized.
    pub fn mint(
        &self,
        actor_key: &[u8; 32],
        actor_role: RoleId,
        action: GovernanceAction,
        target: SpaceMemberId,
        nonce: [u8; 16],
    ) -> Result<GovernanceInstruction, GovernanceError> {
        let actor = member_id_for_key(actor_key);
        self.authorize(actor, actor_role, &action)?;
        self.check_scope(&action, target)?;
        Ok(GovernanceInstruction {
            enclave: self.enclave,
            epoch: self.epoch,
            authority_version: self.authority_version,
            actor: *actor_key,
            actor_role,
            action,
            target,
            nonce,
        })
    }

    /// The authority half of admission: the actor must be a member, must hold
    /// the role they invoke, that role must grant the action's permission, and
    /// the one resolver must agree.
    fn authorize(
        &self,
        actor: SpaceMemberId,
        actor_role: RoleId,
        action: &GovernanceAction,
    ) -> Result<Permission, GovernanceError> {
        let permission = action.required_permission();
        if !self.members.contains(&actor) {
            return Err(GovernanceError::Unauthorized {
                actor: hex::encode(actor.as_bytes()),
                permission: permission.token(),
            });
        }
        let holds_named_role = self.roles_of(actor).contains(&actor_role);
        let role_grants = self
            .role(actor_role)
            .is_some_and(|role| role.grants.contains(&permission));
        let decision = self.resolve(actor, permission, PermissionScope::Enclave);
        if !holds_named_role || !role_grants || !decision.allowed {
            return Err(GovernanceError::Unauthorized {
                actor: hex::encode(actor.as_bytes()),
                permission: permission.token(),
            });
        }
        Ok(permission)
    }

    /// The scope half of admission: nothing an action names may live outside
    /// this Enclave. This is what keeps a governance power from reaching an
    /// account, a direct message or a second Enclave.
    fn check_scope(
        &self,
        action: &GovernanceAction,
        target: SpaceMemberId,
    ) -> Result<(), GovernanceError> {
        if !self.members.contains(&target) {
            return Err(GovernanceError::TargetOutsideEnclave {
                enclave: hex::encode(self.enclave.as_bytes()),
                target: hex::encode(target.as_bytes()),
            });
        }
        match action {
            GovernanceAction::RestrictToChannels { allowed } => {
                if allowed.is_empty() {
                    return Err(GovernanceError::OverBroadAction {
                        action: action.token(),
                        detail: "a restriction must name at least one channel of this Enclave",
                    });
                }
                for channel in allowed {
                    if !self.channels.contains(channel) {
                        return Err(GovernanceError::OverBroadAction {
                            action: action.token(),
                            detail: "restriction names a channel outside this Enclave",
                        });
                    }
                }
            }
            GovernanceAction::DeleteMessage { channel, .. } => {
                if !self.channels.contains(channel) {
                    return Err(GovernanceError::OverBroadAction {
                        action: action.token(),
                        detail: "deletion names a channel outside this Enclave",
                    });
                }
            }
            GovernanceAction::RemoveMember => {
                if target == self.owner {
                    return Err(GovernanceError::OverBroadAction {
                        action: action.token(),
                        detail: "the Enclave owner cannot be removed from their own Enclave",
                    });
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Admits one signed instruction, or says exactly why it refused.
    ///
    /// Every fallible check precedes the first mutation, so a refusal leaves
    /// this state byte-for-byte as it was.
    fn admit(
        &mut self,
        signed: &SignedGovernanceInstruction,
    ) -> Result<Permission, InstructionRefusal> {
        let instruction = &signed.instruction;
        if !signed.verify() {
            return Err(InstructionRefusal::Forged {
                actor: hex::encode(instruction.actor),
            });
        }
        if instruction.enclave != self.enclave {
            return Err(InstructionRefusal::WrongEnclave {
                expected: hex::encode(self.enclave.as_bytes()),
                received: hex::encode(instruction.enclave.as_bytes()),
            });
        }
        // Replay is checked before freshness on purpose. An instruction this
        // client already honoured is a replay whatever the Enclave's epoch has
        // done since, and calling it "stale" would hide the fact that it was
        // delivered twice. A *fresh* nonce still has to be current.
        if self.honoured_nonces.contains(&instruction.nonce) {
            return Err(InstructionRefusal::Replayed {
                nonce: hex::encode(instruction.nonce),
            });
        }
        if instruction.epoch != self.epoch {
            return Err(InstructionRefusal::StaleEpoch {
                current: self.epoch,
                received: instruction.epoch,
            });
        }
        if instruction.authority_version != self.authority_version {
            return Err(InstructionRefusal::StaleAuthorityVersion {
                current: self.authority_version,
                received: instruction.authority_version,
            });
        }
        let permission = self
            .authorize(
                instruction.actor_member(),
                instruction.actor_role,
                &instruction.action,
            )
            .map_err(|error| match error {
                GovernanceError::Unauthorized { actor, permission } => {
                    InstructionRefusal::Unauthorized { actor, permission }
                }
                other => InstructionRefusal::Malformed {
                    detail: other.to_string(),
                },
            })?;
        self.check_scope(&instruction.action, instruction.target)
            .map_err(|error| match error {
                GovernanceError::TargetOutsideEnclave { enclave, target } => {
                    InstructionRefusal::TargetOutsideEnclave { enclave, target }
                }
                GovernanceError::OverBroadAction { action, detail } => {
                    InstructionRefusal::OverBroadAction {
                        action: action.to_owned(),
                        detail: detail.to_owned(),
                    }
                }
                other => InstructionRefusal::Malformed {
                    detail: other.to_string(),
                },
            })?;
        Ok(permission)
    }

    /// Applies an admitted instruction. Only [`HonestMemberClient::honour`]
    /// calls this, and only after [`EnclaveGovernance::admit`] returned.
    fn apply(&mut self, instruction: &GovernanceInstruction) -> Result<u64, GovernanceError> {
        let target = instruction.target;
        match &instruction.action {
            GovernanceAction::RemoveMember => {
                self.members.remove(&target);
                self.holders.retain(|holder| holder.member != target);
                self.restrictions
                    .retain(|restriction| restriction.member != target);
                self.muted.remove(&target);
                self.post_revoked.remove(&target);
                self.invite_revoked.remove(&target);
                // A removal is a key transition. The Enclave leaves the epoch
                // the removed device holds keys for and never returns to it.
                self.epoch = self
                    .epoch
                    .checked_add(1)
                    .ok_or(GovernanceError::EpochExhausted)?;
            }
            GovernanceAction::MuteMember => {
                self.muted.insert(target);
            }
            GovernanceAction::RestrictToChannels { allowed } => {
                self.restrictions
                    .retain(|restriction| restriction.member != target);
                self.restrictions.push(ChannelRestriction {
                    member: target,
                    allowed: allowed.clone(),
                });
                self.restrictions.sort();
            }
            GovernanceAction::RevokePost => {
                self.post_revoked.insert(target);
            }
            GovernanceAction::RevokeInvite => {
                self.invite_revoked.insert(target);
            }
            GovernanceAction::DeleteMessage { message, .. } => {
                self.deleted_messages.insert(*message);
            }
        }
        self.honoured_nonces.insert(instruction.nonce);
        Ok(self.epoch)
    }

    // ----- persistence ---------------------------------------------------

    /// The per-Enclave, per-member governance file.
    pub fn state_path(dir: &Path, enclave: EnclaveId, member: SpaceMemberId) -> PathBuf {
        dir.join(format!(
            "enclave_governance_{}_{}.json",
            hex::encode(enclave.as_bytes()),
            hex::encode(member.as_bytes())
        ))
    }

    fn validate(&self) -> Result<(), GovernanceError> {
        if self.version != GOVERNANCE_STATE_VERSION {
            return Err(GovernanceError::Malformed(
                "unknown state version".to_owned(),
            ));
        }
        if self.catalogue.is_empty() {
            return Err(GovernanceError::Malformed(
                "an Enclave with no permission catalogue cannot resolve anything".to_owned(),
            ));
        }
        let classes = self.catalogue_classes();
        for class in EnforcementClass::ALL {
            if !classes.contains(&class) {
                return Err(GovernanceError::CatalogueClassMissing(class.token()));
            }
        }
        for role in &self.roles {
            if role.grants.is_empty() {
                return Err(GovernanceError::RoleGrantsEmpty);
            }
            if role.name.trim().is_empty() {
                return Err(GovernanceError::RoleNameEmpty);
            }
            for grant in &role.grants {
                if self.class_of(*grant).is_none() {
                    return Err(GovernanceError::PermissionNotCatalogued(grant.token()));
                }
            }
        }
        for holder in &self.holders {
            for role in &holder.roles {
                if self.role(*role).is_none() {
                    return Err(GovernanceError::UnknownRole);
                }
            }
        }
        for restriction in &self.restrictions {
            for channel in &restriction.allowed {
                if !self.channels.contains(channel) {
                    return Err(GovernanceError::OverBroadAction {
                        action: "restrict-to-channels",
                        detail: "stored restriction names a channel outside this Enclave",
                    });
                }
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Honest member client
// ---------------------------------------------------------------------------

/// One message as an honest member client holds it.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClientMessage {
    pub id: MessageId,
    pub channel: ChannelId,
    pub author: SpaceMemberId,
    pub body: String,
}

/// Why an honest client refused an instruction. Every variant names the thing
/// that was wrong, so a client can say what it declined.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, thiserror::Error)]
#[serde(rename_all = "snake_case", tag = "refusal")]
pub enum InstructionRefusal {
    #[error("instruction signature does not belong to actor {actor}")]
    Forged { actor: String },
    #[error("instruction is for enclave {received}, not {expected}")]
    WrongEnclave { expected: String, received: String },
    #[error("instruction epoch {received} is stale; this Enclave is at {current}")]
    StaleEpoch { current: u64, received: u64 },
    #[error(
        "instruction role/permission version {received} is stale; this Enclave is at {current}"
    )]
    StaleAuthorityVersion { current: u64, received: u64 },
    #[error("instruction nonce {nonce} was already honoured")]
    Replayed { nonce: String },
    #[error("actor {actor} is not authorized for {permission} in this Enclave")]
    Unauthorized {
        actor: String,
        permission: &'static str,
    },
    #[error("target {target} is not a member of enclave {enclave}")]
    TargetOutsideEnclave { enclave: String, target: String },
    #[error("action {action} is over-broad: {detail}")]
    OverBroadAction { action: String, detail: String },
    #[error("instruction is malformed: {detail}")]
    Malformed { detail: String },
}

impl InstructionRefusal {
    /// The stable token a check counts refusals by.
    pub const fn token(&self) -> &'static str {
        match self {
            Self::Forged { .. } => "forged",
            Self::WrongEnclave { .. } => "cross-enclave",
            Self::StaleEpoch { .. } => "stale-epoch",
            Self::StaleAuthorityVersion { .. } => "stale-authority-version",
            Self::Replayed { .. } => "replayed",
            Self::Unauthorized { .. } => "unauthorized",
            Self::TargetOutsideEnclave { .. } => "target-outside-enclave",
            Self::OverBroadAction { .. } => "over-broad-action",
            Self::Malformed { .. } => "malformed",
        }
    }
}

/// What honouring one instruction did.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Honoured {
    pub action: &'static str,
    pub permission: Permission,
    pub class: EnforcementClass,
    pub epoch: u64,
    /// Set when the honoured action destroyed a message body on this client.
    pub destroyed_body_bytes: usize,
}

/// One honest member client: its own replica of the Enclave's governance
/// state, its own copy of the Enclave's messages, and its own replay ledger.
///
/// "Honest" is the whole claim. A TRUST-class instruction is honoured because
/// this client chooses to; nothing here pretends a dishonest client that
/// already holds a plaintext could be stopped.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HonestMemberClient {
    label: String,
    member: SpaceMemberId,
    governance: EnclaveGovernance,
    messages: Vec<ClientMessage>,
    honoured: Vec<[u8; 16]>,
    #[serde(skip)]
    refusals: Vec<InstructionRefusal>,
}

impl HonestMemberClient {
    pub fn new(
        label: impl Into<String>,
        member: SpaceMemberId,
        governance: EnclaveGovernance,
    ) -> Self {
        Self {
            label: label.into(),
            member,
            governance,
            messages: Vec::new(),
            honoured: Vec::new(),
            refusals: Vec::new(),
        }
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub const fn member(&self) -> SpaceMemberId {
        self.member
    }

    pub const fn governance(&self) -> &EnclaveGovernance {
        &self.governance
    }

    pub fn governance_mut(&mut self) -> &mut EnclaveGovernance {
        &mut self.governance
    }

    pub fn messages(&self) -> &[ClientMessage] {
        &self.messages
    }

    pub fn message(&self, id: MessageId) -> Option<&ClientMessage> {
        self.messages.iter().find(|message| message.id == id)
    }

    pub fn honoured_nonces(&self) -> &[[u8; 16]] {
        &self.honoured
    }

    pub fn refusals(&self) -> &[InstructionRefusal] {
        &self.refusals
    }

    /// Delivers a message to this client's copy of the Enclave.
    pub fn receive(&mut self, message: ClientMessage) {
        self.messages.retain(|held| held.id != message.id);
        self.messages.push(message);
        self.messages.sort();
    }

    /// Honours one signed instruction, or refuses it and records why.
    ///
    /// Honouring is idempotent by nonce: a second delivery of the same
    /// instruction is refused as a replay and changes nothing at all, so every
    /// honest client applies an authorized unique instruction exactly once.
    pub fn honour(
        &mut self,
        signed: &SignedGovernanceInstruction,
    ) -> Result<Honoured, InstructionRefusal> {
        let permission = match self.governance.admit(signed) {
            Ok(permission) => permission,
            Err(refusal) => {
                self.refusals.push(refusal.clone());
                return Err(refusal);
            }
        };
        let class = self
            .governance
            .class_of(permission)
            .unwrap_or_else(|| permission.class());
        let instruction = &signed.instruction;

        // Deletion works exactly as burn does: the body is destroyed on this
        // client, and what remains is a tombstone with nothing in it.
        let mut destroyed_body_bytes = 0;
        if let GovernanceAction::DeleteMessage { message, .. } = &instruction.action {
            if let Some(position) = self.messages.iter().position(|held| held.id == *message) {
                destroyed_body_bytes = self.messages[position].body.len();
                self.messages.remove(position);
            }
        }

        let epoch = match self.governance.apply(instruction) {
            Ok(epoch) => epoch,
            Err(error) => {
                let refusal = InstructionRefusal::Malformed {
                    detail: error.to_string(),
                };
                self.refusals.push(refusal.clone());
                return Err(refusal);
            }
        };
        self.honoured.push(instruction.nonce);
        Ok(Honoured {
            action: instruction.action.token(),
            permission,
            class,
            epoch,
            destroyed_body_bytes,
        })
    }

    /// Asks the one resolver on this client's own replica.
    pub fn resolve(&self, permission: Permission, scope: PermissionScope) -> PermissionDecision {
        self.governance.resolve(self.member, permission, scope)
    }

    /// Asks the one resolver about another member of this Enclave.
    pub fn resolve_for(
        &self,
        member: SpaceMemberId,
        permission: Permission,
        scope: PermissionScope,
    ) -> PermissionDecision {
        self.governance.resolve(member, permission, scope)
    }

    /// The per-client file.
    pub fn state_path(&self, dir: &Path) -> PathBuf {
        EnclaveGovernance::state_path(dir, self.governance.enclave, self.member)
    }

    /// Writes this client's state, encrypted at rest and atomically.
    ///
    /// A client without the unlocked file-storage key refuses rather than
    /// leaving governance state in plaintext.
    pub fn save(&self, dir: &Path) -> Result<PathBuf, GovernanceError> {
        self.governance.validate()?;
        let path = self.state_path(dir);
        let plaintext = serde_json::to_vec(self)
            .map_err(|error| GovernanceError::Malformed(error.to_string()))?;
        let key = crate::main_password::get_file_storage_key()
            .ok_or(GovernanceError::StorageKeyUnavailable)?;
        let sealed = crate::main_password::encrypt_at_rest(&plaintext, &key)
            .map_err(GovernanceError::Persistence)?;
        crate::recoverable_file::write_recoverable(&path, &sealed)
            .map_err(|error| GovernanceError::Persistence(error.to_string()))?;
        Ok(path)
    }

    /// Reads a client back after a restart. Nothing carries over in memory.
    pub fn reopen(
        dir: &Path,
        enclave: EnclaveId,
        member: SpaceMemberId,
    ) -> Result<Self, GovernanceError> {
        let path = EnclaveGovernance::state_path(dir, enclave, member);
        let key = crate::main_password::get_file_storage_key()
            .ok_or(GovernanceError::StorageKeyUnavailable)?;
        let sealed = std::fs::read(&path)
            .map_err(|error| GovernanceError::Persistence(error.to_string()))?;
        if !crate::main_password::has_enc_magic(&sealed) {
            return Err(GovernanceError::Persistence(
                "governance state on disk is not encrypted".to_owned(),
            ));
        }
        let plaintext = crate::main_password::decrypt_at_rest(&sealed, &key)
            .map_err(GovernanceError::Persistence)?;
        let client: Self = serde_json::from_slice(&plaintext)
            .map_err(|error| GovernanceError::Malformed(error.to_string()))?;
        client.governance.validate()?;
        if client.governance.enclave != enclave || client.member != member {
            return Err(GovernanceError::Malformed(
                "stored client belongs to a different Enclave or member".to_owned(),
            ));
        }
        Ok(client)
    }
}

// ---------------------------------------------------------------------------
// Isolation oracle
// ---------------------------------------------------------------------------

/// A byte-for-byte "nothing else moved" oracle.
///
/// Enclave self-moderation is only self-moderation if it changes nothing
/// outside the Enclave that decided it. This captures the exact bytes of every
/// subject that must not move — the control Enclave, the target's account
/// record, the target's direct messages — and reports the first one that did.
#[derive(Clone, Debug, Default)]
pub struct IsolationOracle {
    subjects: BTreeMap<String, Vec<u8>>,
}

impl IsolationOracle {
    pub fn new() -> Self {
        Self::default()
    }

    /// Captures a subject's bytes. Re-capturing a subject replaces it.
    pub fn capture(&mut self, subject: impl Into<String>, bytes: Vec<u8>) {
        self.subjects.insert(subject.into(), bytes);
    }

    /// Captures a subject by reading a file.
    pub fn capture_file(
        &mut self,
        subject: impl Into<String>,
        path: &Path,
    ) -> Result<(), GovernanceError> {
        let bytes =
            std::fs::read(path).map_err(|error| GovernanceError::Persistence(error.to_string()))?;
        self.capture(subject, bytes);
        Ok(())
    }

    pub fn subjects(&self) -> Vec<&str> {
        self.subjects.keys().map(String::as_str).collect()
    }

    pub fn len(&self) -> usize {
        self.subjects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.subjects.is_empty()
    }

    /// Verifies one subject is byte-for-byte what it was.
    pub fn verify(&self, subject: &str, bytes: &[u8]) -> Result<(), IsolationBreach> {
        let Some(captured) = self.subjects.get(subject) else {
            return Err(IsolationBreach::SubjectNotCaptured {
                subject: subject.to_owned(),
            });
        };
        if captured.as_slice() != bytes {
            return Err(IsolationBreach::Changed {
                subject: subject.to_owned(),
                captured_bytes: captured.len(),
                observed_bytes: bytes.len(),
            });
        }
        Ok(())
    }

    /// Verifies one subject that lives in a file.
    pub fn verify_file(&self, subject: &str, path: &Path) -> Result<(), IsolationBreach> {
        let bytes = std::fs::read(path).map_err(|error| IsolationBreach::Unreadable {
            subject: subject.to_owned(),
            detail: error.to_string(),
        })?;
        self.verify(subject, &bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum IsolationBreach {
    #[error("isolation subject {subject} was never captured; the oracle is starved")]
    SubjectNotCaptured { subject: String },
    #[error("isolation subject {subject} changed: {captured_bytes} bytes became {observed_bytes}")]
    Changed {
        subject: String,
        captured_bytes: usize,
        observed_bytes: usize,
    },
    #[error("isolation subject {subject} could not be read: {detail}")]
    Unreadable { subject: String, detail: String },
}

// ---------------------------------------------------------------------------
// Support
// ---------------------------------------------------------------------------

fn push_frame(bytes: &mut Vec<u8>, field: &[u8]) {
    bytes.extend_from_slice(&(field.len() as u64).to_be_bytes());
    bytes.extend_from_slice(field);
}

fn relay_aad(routing_tag: &[u8; 16]) -> Vec<u8> {
    let mut aad = Vec::with_capacity(RELAY_ENVELOPE_DOMAIN.len() + routing_tag.len());
    aad.extend_from_slice(RELAY_ENVELOPE_DOMAIN);
    aad.extend_from_slice(routing_tag);
    aad
}

fn random_array<const N: usize>() -> Result<[u8; N], GovernanceError> {
    crypto::random::random_bytes(N)
        .try_into()
        .map_err(|_| GovernanceError::Randomness)
}

/// Fresh nonce for one instruction.
pub fn fresh_nonce() -> Result<[u8; 16], GovernanceError> {
    random_array()
}

mod signature_bytes {
    use crypto::ed25519::SIGNATURE_SIZE;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(
        value: &[u8; SIGNATURE_SIZE],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&hex::encode(value))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<[u8; SIGNATURE_SIZE], D::Error> {
        let encoded = String::deserialize(deserializer)?;
        let bytes = hex::decode(&encoded).map_err(serde::de::Error::custom)?;
        <[u8; SIGNATURE_SIZE]>::try_from(bytes.as_slice())
            .map_err(|_| serde::de::Error::custom("signature must be 64 bytes"))
    }
}

mod nonce_bytes {
    use crypto::aead::NONCE_SIZE;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(
        value: &[u8; NONCE_SIZE],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&hex::encode(value))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<[u8; NONCE_SIZE], D::Error> {
        let encoded = String::deserialize(deserializer)?;
        let bytes = hex::decode(&encoded).map_err(serde::de::Error::custom)?;
        <[u8; NONCE_SIZE]>::try_from(bytes.as_slice())
            .map_err(|_| serde::de::Error::custom("nonce must be 24 bytes"))
    }
}

#[derive(Clone, Debug, thiserror::Error, Eq, PartialEq)]
pub enum GovernanceError {
    #[error("actor {actor} is not authorized for {permission} in this Enclave")]
    Unauthorized {
        actor: String,
        permission: &'static str,
    },
    #[error("target {target} is not a member of enclave {enclave}")]
    TargetOutsideEnclave { enclave: String, target: String },
    #[error("action {action} is over-broad: {detail}")]
    OverBroadAction {
        action: &'static str,
        detail: &'static str,
    },
    #[error("a custom role must grant at least one catalogued permission")]
    RoleGrantsEmpty,
    #[error("a custom role must have a name")]
    RoleNameEmpty,
    #[error("this Enclave already has a role with that id")]
    DuplicateRole,
    #[error("no role with that id is in this Enclave")]
    UnknownRole,
    #[error("member is not in this Enclave")]
    NotAMember,
    #[error("permission {0} is not in this Enclave's catalogue")]
    PermissionNotCatalogued(&'static str),
    #[error("this Enclave's stored catalogue has no {0}-class permission")]
    CatalogueClassMissing(&'static str),
    #[error("role/permission version is exhausted")]
    VersionExhausted,
    #[error("enclave epoch is exhausted")]
    EpochExhausted,
    #[error("relay envelope authentication failed")]
    RelayAuthentication,
    #[error("file storage key is unavailable")]
    StorageKeyUnavailable,
    #[error("governance state is malformed: {0}")]
    Malformed(String),
    #[error("governance persistence failed: {0}")]
    Persistence(String),
    #[error("secure randomness failed")]
    Randomness,
}
