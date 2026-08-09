//! Local Space lifecycle state.
//!
//! A Space has members and roles.  It deliberately has no founder key: group
//! key distribution belongs to the sender-key lifecycle and is identical for
//! every active member. The founder receives the explicit moderation permission
//! represented by the roster role, not different cryptographic state.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use ipc::space_roster::SpaceChannelId;
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};

use crate::burn_authorize::{
    authorize_remote_friend_burn, BurnAuthorizationError, BurnScopeBindings,
};
use crate::burn_contract::{BurnSignatureVerifier, RemoteFriendBurnPlan, RemoteFriendBurnRequest};

/// A named server member permission stored in the local server roster.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ServerMemberRight {
    ReadOnly,
    ReadAndSend,
    RemovePeople,
}

impl ServerMemberRight {
    pub const fn name(self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::ReadAndSend => "read-and-send",
            Self::RemovePeople => "remove-people",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PermissionChangeRoute {
    Screens,
    DirectCommand,
}

impl PermissionChangeRoute {
    const fn name(self) -> &'static str {
        match self {
            Self::Screens => "screens",
            Self::DirectCommand => "direct command",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerMemberPermissionList {
    owner_name: String,
    members: BTreeMap<String, BTreeSet<ServerMemberRight>>,
    self_change_routes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServerMemberPermissionError {
    UnknownMember(String),
    OnlyOwnerMayChangePermissions {
        actor_name: String,
    },
    SelfChangeRefused {
        member_name: String,
        attempted_right: ServerMemberRight,
        route: PermissionChangeRoute,
    },
}

impl std::fmt::Display for ServerMemberPermissionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownMember(member_name) => write!(f, "{member_name} is not on the server"),
            Self::OnlyOwnerMayChangePermissions { actor_name } => {
                write!(f, "{actor_name} is not the server owner")
            }
            Self::SelfChangeRefused {
                member_name,
                attempted_right,
                route,
            } => write!(
                f,
                "{member_name} cannot add {} to themselves through {}",
                attempted_right.name(),
                route.name()
            ),
        }
    }
}

impl ServerMemberPermissionList {
    pub fn new(owner_name: impl Into<String>) -> Self {
        let owner_name = owner_name.into();
        Self {
            owner_name: owner_name.clone(),
            members: BTreeMap::from([(
                owner_name,
                BTreeSet::from([
                    ServerMemberRight::ReadOnly,
                    ServerMemberRight::ReadAndSend,
                    ServerMemberRight::RemovePeople,
                ]),
            )]),
            self_change_routes: 0,
        }
    }

    pub fn add_read_only_member(&mut self, member_name: impl Into<String>) {
        self.members.insert(
            member_name.into(),
            BTreeSet::from([ServerMemberRight::ReadOnly]),
        );
    }

    pub fn grant_right(
        &mut self,
        actor_name: &str,
        target_name: &str,
        right: ServerMemberRight,
        route: PermissionChangeRoute,
    ) -> Result<Vec<&'static str>, ServerMemberPermissionError> {
        if !self.members.contains_key(target_name) {
            return Err(ServerMemberPermissionError::UnknownMember(
                target_name.to_owned(),
            ));
        }
        if actor_name == target_name {
            return Err(ServerMemberPermissionError::SelfChangeRefused {
                member_name: target_name.to_owned(),
                attempted_right: right,
                route,
            });
        }
        if actor_name != self.owner_name {
            return Err(ServerMemberPermissionError::OnlyOwnerMayChangePermissions {
                actor_name: actor_name.to_owned(),
            });
        }

        let rights = self
            .members
            .get_mut(target_name)
            .expect("membership was checked before mutation");
        rights.insert(right);
        Ok(rights
            .iter()
            .copied()
            .map(ServerMemberRight::name)
            .collect())
    }

    pub fn saved_right_names(
        &self,
        member_name: &str,
    ) -> Result<Vec<&'static str>, ServerMemberPermissionError> {
        self.members
            .get(member_name)
            .map(|rights| {
                rights
                    .iter()
                    .copied()
                    .map(ServerMemberRight::name)
                    .collect()
            })
            .ok_or_else(|| ServerMemberPermissionError::UnknownMember(member_name.to_owned()))
    }

    pub const fn self_change_routes(&self) -> usize {
        self.self_change_routes
    }
}

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
    Moderator,
    Member,
}

/// One locally authoritative Space roster.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Space {
    id: SpaceId,
    members: BTreeMap<SpaceMemberId, SpaceRole>,
    channel_key_domains: BTreeMap<SpaceChannelId, ChannelKeyDomain>,
}

/// One member in a locally named group roster.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalGroupMember {
    id: SpaceMemberId,
    display_name: String,
}

impl LocalGroupMember {
    pub fn new(
        id: SpaceMemberId,
        display_name: impl Into<String>,
    ) -> Result<Self, LocalGroupError> {
        if id.0.iter().all(|byte| *byte == 0) {
            return Err(LocalGroupError::EmptyMemberIdentity);
        }
        let display_name = display_name.into();
        if display_name.trim().is_empty() {
            return Err(LocalGroupError::EmptyMemberName);
        }
        Ok(Self { id, display_name })
    }

    pub const fn id(&self) -> SpaceMemberId {
        self.id
    }

    pub fn display_name(&self) -> &str {
        &self.display_name
    }
}

/// A named local group with ordered membership as shown to the user.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalGroup {
    name: String,
    members: Vec<LocalGroupMember>,
}

impl LocalGroup {
    fn new(name: String, members: Vec<LocalGroupMember>) -> Self {
        Self { name, members }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn members(&self) -> &[LocalGroupMember] {
        &self.members
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LocalGroupDirectory {
    groups: BTreeMap<String, LocalGroup>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalGroupError {
    EmptyGroupName,
    EmptyMemberIdentity,
    EmptyMemberName,
    DuplicateGroupName,
    DuplicateMember,
    UnknownGroup,
    MemberIndexOutOfRange,
}

impl LocalGroupDirectory {
    pub fn group_count(&self) -> usize {
        self.groups.len()
    }

    pub fn group(&self, name: &str) -> Option<&LocalGroup> {
        self.groups.get(name)
    }

    pub fn create_group(
        &mut self,
        name: impl Into<String>,
        members: Vec<LocalGroupMember>,
    ) -> Result<(), LocalGroupError> {
        let name = validate_local_group_name(name.into())?;
        if self.groups.contains_key(&name) {
            return Err(LocalGroupError::DuplicateGroupName);
        }
        ensure_unique_local_group_members(&members)?;
        self.groups
            .insert(name.clone(), LocalGroup::new(name, members));
        Ok(())
    }

    pub fn replace_group_member(
        &mut self,
        group_name: &str,
        index: usize,
        member: LocalGroupMember,
    ) -> Result<(), LocalGroupError> {
        let group = self
            .groups
            .get_mut(group_name)
            .ok_or(LocalGroupError::UnknownGroup)?;
        let Some(candidate) = group.members.get(index).map(|_| {
            let mut candidate = group.members.clone();
            candidate[index] = member;
            candidate
        }) else {
            return Err(LocalGroupError::MemberIndexOutOfRange);
        };
        ensure_unique_local_group_members(&candidate)?;
        group.members = candidate;
        Ok(())
    }
}

fn validate_local_group_name(name: String) -> Result<String, LocalGroupError> {
    let name = name.trim().to_owned();
    if name.is_empty() {
        return Err(LocalGroupError::EmptyGroupName);
    }
    Ok(name)
}

fn ensure_unique_local_group_members(members: &[LocalGroupMember]) -> Result<(), LocalGroupError> {
    let mut seen = BTreeSet::new();
    if members.iter().any(|member| !seen.insert(member.id())) {
        return Err(LocalGroupError::DuplicateMember);
    }
    Ok(())
}

const CUSTOM_ROLE_STORE_LABEL: &str = "Space custom role store";
const MAX_CUSTOM_ROLE_STORE_BYTES: u64 = 256 * 1024;

/// The complete set of stored fields in a role record.
pub const CUSTOM_ROLE_RECORD_FIELDS: [&str; 15] = [
    "id",
    "name",
    "colour",
    "icon",
    "order",
    "hoist",
    "mention_policy",
    "slow_mode_seconds",
    "longest_mute_seconds",
    "actions_per_hour_budget",
    "self_assignable",
    "auto_grant_on_join",
    "expires_at_unix_seconds",
    "duplicate_source_role_id",
    "template_name",
];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum RoleMentionPolicy {
    OwnerOnly,
    OwnerAndModerators,
    Everyone,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CustomRoleProperties {
    pub name: String,
    pub colour: String,
    pub icon: String,
    pub order: i64,
    pub hoist: bool,
    pub mention_policy: RoleMentionPolicy,
    pub slow_mode_seconds: u64,
    pub longest_mute_seconds: u64,
    pub actions_per_hour_budget: u64,
    pub self_assignable: bool,
    pub auto_grant_on_join: bool,
    pub expires_at_unix_seconds: u64,
    pub duplicate_source_role_id: String,
    pub template_name: String,
}

impl CustomRoleProperties {
    pub const fn filled_property_count() -> usize {
        CUSTOM_ROLE_RECORD_FIELDS.len() - 1
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CustomRoleRecord {
    pub id: String,
    #[serde(flatten)]
    pub properties: CustomRoleProperties,
}

impl CustomRoleRecord {
    pub const fn stored_field_count() -> usize {
        CUSTOM_ROLE_RECORD_FIELDS.len()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
struct CustomRoleDocument {
    next_role_number: u64,
    roles: BTreeMap<String, CustomRoleRecord>,
    member_roles: BTreeMap<String, BTreeSet<String>>,
    templates: BTreeMap<String, CustomRoleProperties>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomRoleStore {
    path: PathBuf,
    document: CustomRoleDocument,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpiredRolePruneReport {
    pub expired_role_count: usize,
    pub member_role_removal_count: usize,
    pub template_count: usize,
}

impl CustomRoleStore {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref().to_path_buf();
        let Some(bytes) = crate::atomic_file::read_recoverable_bounded(
            &path,
            MAX_CUSTOM_ROLE_STORE_BYTES,
            CUSTOM_ROLE_STORE_LABEL,
        )?
        else {
            return Ok(Self {
                path,
                document: CustomRoleDocument::default(),
            });
        };
        let document = decode_custom_role_document(&bytes)?;
        Ok(Self { path, document })
    }

    pub fn save(&self) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(&self.document)
            .map_err(|_| format!("{CUSTOM_ROLE_STORE_LABEL} could not be encoded"))?;
        crate::atomic_file::write_recoverable(&self.path, &bytes, CUSTOM_ROLE_STORE_LABEL)
    }

    pub fn create_role(
        &mut self,
        properties: CustomRoleProperties,
    ) -> Result<CustomRoleRecord, String> {
        validate_custom_role_properties(&properties)?;
        self.document.next_role_number = self.document.next_role_number.saturating_add(1);
        let id = format!("role-{}", self.document.next_role_number);
        let role = CustomRoleRecord {
            id: id.clone(),
            properties,
        };
        self.document.roles.insert(id, role.clone());
        Ok(role)
    }

    pub fn duplicate_role(&mut self, source_role_id: &str) -> Result<CustomRoleRecord, String> {
        let source = self
            .document
            .roles
            .get(source_role_id)
            .ok_or_else(|| "Space custom role source role is unknown".to_owned())?;
        self.create_role(source.properties.clone())
    }

    pub fn role(&self, id: &str) -> Option<&CustomRoleRecord> {
        self.document.roles.get(id)
    }

    pub fn add_template(
        &mut self,
        name: impl Into<String>,
        properties: CustomRoleProperties,
    ) -> Result<(), String> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err("Space custom role template_name is missing".to_owned());
        }
        validate_custom_role_properties(&properties)?;
        self.document.templates.insert(name, properties);
        Ok(())
    }

    pub fn template(&self, name: &str) -> Option<&CustomRoleProperties> {
        self.document.templates.get(name)
    }

    pub fn grant_role_to_member(&mut self, member_id: impl Into<String>, role_id: &str) -> bool {
        if !self.document.roles.contains_key(role_id) {
            return false;
        }
        self.document
            .member_roles
            .entry(member_id.into())
            .or_default()
            .insert(role_id.to_owned())
    }

    pub fn role_ids_for_member(&self, member_id: &str) -> Vec<String> {
        self.document
            .member_roles
            .get(member_id)
            .map(|roles| roles.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn prune_expired_roles(&mut self, now_unix_seconds: u64) -> ExpiredRolePruneReport {
        let expired: BTreeSet<_> = self
            .document
            .roles
            .iter()
            .filter_map(|(id, role)| {
                (role.properties.expires_at_unix_seconds <= now_unix_seconds).then_some(id.clone())
            })
            .collect();
        for id in &expired {
            self.document.roles.remove(id);
        }

        let mut member_role_removal_count = 0;
        for roles in self.document.member_roles.values_mut() {
            let before = roles.len();
            roles.retain(|id| !expired.contains(id));
            member_role_removal_count += before.saturating_sub(roles.len());
        }

        ExpiredRolePruneReport {
            expired_role_count: expired.len(),
            member_role_removal_count,
            template_count: self.document.templates.len(),
        }
    }
}

fn decode_custom_role_document(bytes: &[u8]) -> Result<CustomRoleDocument, String> {
    let document: CustomRoleDocument = serde_json::from_slice(bytes).map_err(|error| {
        missing_custom_role_field_name(&error.to_string())
            .map(|field| format!("Space custom role property {field} is missing"))
            .unwrap_or_else(|| format!("{CUSTOM_ROLE_STORE_LABEL} is malformed"))
    })?;
    for role in document.roles.values() {
        validate_custom_role_record(role)?;
    }
    for template in document.templates.values() {
        validate_custom_role_properties(template)?;
    }
    Ok(document)
}

fn missing_custom_role_field_name(error: &str) -> Option<&'static str> {
    CUSTOM_ROLE_RECORD_FIELDS
        .iter()
        .copied()
        .find(|field| error.contains(&format!("missing field `{field}`")))
}

fn validate_custom_role_record(role: &CustomRoleRecord) -> Result<(), String> {
    if role.id.trim().is_empty() {
        return Err("Space custom role property id is missing".to_owned());
    }
    validate_custom_role_properties(&role.properties)
}

fn validate_custom_role_properties(properties: &CustomRoleProperties) -> Result<(), String> {
    for (field, value) in [
        ("name", properties.name.as_str()),
        ("colour", properties.colour.as_str()),
        ("icon", properties.icon.as_str()),
        (
            "duplicate_source_role_id",
            properties.duplicate_source_role_id.as_str(),
        ),
        ("template_name", properties.template_name.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(format!("Space custom role property {field} is missing"));
        }
    }
    Ok(())
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

/// The one server-side effect of a moderator deletion request.
///
/// This does not say anything about the copies held by Space members.  The
/// server can confirm its own blob delete; each member's local deletion is a
/// distinct, independently acknowledged request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServerBlobDeleteRequestState {
    Queued,
    Confirmed,
}

/// Honest, count-only status for a moderator's delete-for-everyone request.
///
/// The variants intentionally retain `Request`: even after the server blob is
/// confirmed gone, member copies remain independent instructions whose absent
/// acknowledgements are `Unconfirmed`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModeratorDeleteForEveryoneRequestStatus {
    RequestQueued { member_deletions_confirmed: usize },
    RequestServerBlobConfirmed { member_deletions_confirmed: usize },
}

impl ModeratorDeleteForEveryoneRequestStatus {
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

/// A planned moderator delete-for-everyone action. It is a request, not a claim
/// that any member copy has disappeared.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModeratorDeleteForEveryoneRequest {
    remote_burn_plan: RemoteFriendBurnPlan,
    pending_member_deletions: BTreeSet<SpaceMemberId>,
    confirmed_member_deletions: BTreeSet<SpaceMemberId>,
    server_blob_delete: ServerBlobDeleteRequestState,
}

impl ModeratorDeleteForEveryoneRequest {
    /// The outgoing authenticated instructions.  These are queued requests;
    /// their presence is not a member-deletion acknowledgement.
    pub fn peer_deletion_instruction_count(&self) -> usize {
        self.remote_burn_plan.notices.len()
    }

    pub const fn server_blob_delete_state(&self) -> ServerBlobDeleteRequestState {
        self.server_blob_delete
    }

    pub fn status(&self) -> ModeratorDeleteForEveryoneRequestStatus {
        let member_deletions_confirmed = self.confirmed_member_deletions.len();
        match self.server_blob_delete {
            ServerBlobDeleteRequestState::Queued => {
                ModeratorDeleteForEveryoneRequestStatus::RequestQueued {
                    member_deletions_confirmed,
                }
            }
            ServerBlobDeleteRequestState::Confirmed => {
                ModeratorDeleteForEveryoneRequestStatus::RequestServerBlobConfirmed {
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
pub enum ModeratorDeleteForEveryoneError {
    NotAModerator,
    NoOtherMembers,
    RequestTargetsDoNotMatchSpaceMembership,
    Authorization(BurnAuthorizationError),
}

/// Authorize a moderator's delete-for-everyone request and queue one authenticated
/// delete instruction for every other current Space member.
///
/// This deliberately delegates cryptographic scope, issuer, consent, and
/// signature checks to T2's [`authorize_remote_friend_burn`] boundary.  Space
/// moderation adds only the roster-role and current-membership checks; it does
/// not create another burn authorization system.
pub fn request_moderator_delete_for_everyone(
    space: &Space,
    requesting_moderator: SpaceMemberId,
    bindings: BurnScopeBindings<'_>,
    local_identity_commitment: [u8; 32],
    request: &RemoteFriendBurnRequest,
    revoked_grants: &BTreeMap<[u8; 16], u64>,
    verifier: &impl BurnSignatureVerifier,
) -> Result<ModeratorDeleteForEveryoneRequest, ModeratorDeleteForEveryoneError> {
    if space.role_of(requesting_moderator) != Some(SpaceRole::Moderator) {
        return Err(ModeratorDeleteForEveryoneError::NotAModerator);
    }

    let pending_member_deletions: BTreeSet<_> = space
        .members
        .keys()
        .copied()
        .filter(|member| *member != requesting_moderator)
        .collect();
    if pending_member_deletions.is_empty() {
        return Err(ModeratorDeleteForEveryoneError::NoOtherMembers);
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
        return Err(ModeratorDeleteForEveryoneError::RequestTargetsDoNotMatchSpaceMembership);
    }

    let remote_burn_plan = authorize_remote_friend_burn(
        bindings,
        local_identity_commitment,
        request,
        revoked_grants,
        verifier,
    )
    .map_err(ModeratorDeleteForEveryoneError::Authorization)?;

    Ok(ModeratorDeleteForEveryoneRequest {
        remote_burn_plan,
        pending_member_deletions,
        confirmed_member_deletions: BTreeSet::new(),
        server_blob_delete: ServerBlobDeleteRequestState::Queued,
    })
}

/// Creates a Space with its founder as a moderator.
///
/// The returned object contains no key material. The founder's moderation
/// permission is solely a roster role, so another member can hold the same
/// group sender-key epoch material when admitted by the membership lifecycle.
pub fn create_space(id: SpaceId, founder: SpaceMemberId) -> Result<Space, CreateSpaceError> {
    if founder.0.iter().all(|byte| *byte == 0) {
        return Err(CreateSpaceError::EmptyFounderIdentity);
    }

    Ok(Space {
        id,
        members: BTreeMap::from([(founder, SpaceRole::Moderator)]),
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
    /// Roles deliberately do not affect this list: moderators and ordinary members
    /// are equal cryptographic participants.  The caller supplies the same
    /// epoch material to each member through the established key lifecycle.
    pub fn key_recipients(&self) -> impl ExactSizeIterator<Item = SpaceMemberId> + '_ {
        self.members.keys().copied()
    }

    /// Whether no current member has the Moderator role.
    ///
    /// This is a normal, usable Space state. It occurs if the last moderator
    /// leaves; callers must not reject a membership event merely to prevent it.
    pub fn is_unmoderated(&self) -> bool {
        !self
            .members
            .values()
            .any(|role| *role == SpaceRole::Moderator)
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
        if recipients
            .iter()
            .any(|member| !self.members.contains_key(member))
        {
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

    fn named_member(value: u8, display_name: &str) -> LocalGroupMember {
        LocalGroupMember::new(member(value), display_name).unwrap()
    }

    fn local_group_member_names(group: &LocalGroup) -> String {
        group
            .members()
            .iter()
            .map(LocalGroupMember::display_name)
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn filled_role_properties() -> CustomRoleProperties {
        CustomRoleProperties {
            name: "Signal Watch Captain".to_owned(),
            colour: "#14b8a6".to_owned(),
            icon: "shield-check".to_owned(),
            order: 42,
            hoist: true,
            mention_policy: RoleMentionPolicy::OwnerAndModerators,
            slow_mode_seconds: 17,
            longest_mute_seconds: 3_600,
            actions_per_hour_budget: 24,
            self_assignable: true,
            auto_grant_on_join: true,
            expires_at_unix_seconds: 4_000_000_000,
            duplicate_source_role_id: "seed-role-template".to_owned(),
            template_name: "watch-captain-template".to_owned(),
        }
    }

    fn role_store_path(label: &str) -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "osl-space-role-{label}-{}-{nonce}.json",
            std::process::id()
        ))
    }

    fn missing_field_store_json(field: &str) -> Vec<u8> {
        let role = CustomRoleRecord {
            id: "role-missing-check".to_owned(),
            properties: filled_role_properties(),
        };
        let mut role_value = serde_json::to_value(role).expect("role serializes");
        role_value
            .as_object_mut()
            .expect("role is an object")
            .remove(field);
        let document = serde_json::json!({
            "next_role_number": 1,
            "roles": {
                "role-missing-check": role_value
            },
            "member_roles": {},
            "templates": {}
        });
        serde_json::to_vec_pretty(&document).expect("document serializes")
    }

    #[test]
    fn task_1314_duplicate_group_members_fail() {
        let mut groups = LocalGroupDirectory::default();
        let before_count = groups.group_count();

        groups
            .create_group(
                "Maple Group",
                vec![
                    named_member(1, "Ava"),
                    named_member(2, "Ben"),
                    named_member(3, "Cy"),
                ],
            )
            .unwrap();
        let after_create_count = groups.group_count();
        let after_create_members = local_group_member_names(groups.group("Maple Group").unwrap());

        let duplicate_result =
            groups.replace_group_member("Maple Group", 2, named_member(2, "Ben"));
        let duplicate_refused = matches!(duplicate_result, Err(LocalGroupError::DuplicateMember));
        let after_duplicate_count = groups.group_count();
        let after_duplicate_members =
            local_group_member_names(groups.group("Maple Group").unwrap());

        println!(
            "TASK1314 before_count={before_count} after_create_count={after_create_count} maple_members=\"{after_create_members}\" duplicate_result={} after_duplicate_count={after_duplicate_count} maple_after_duplicate=\"{after_duplicate_members}\"",
            if duplicate_refused {
                "refused duplicate member"
            } else {
                "not refused"
            }
        );

        assert_eq!(before_count, 0);
        assert_eq!(after_create_count, 1);
        assert_eq!(after_create_members, "Ava, Ben, Cy");
        assert!(duplicate_refused, "repeated Ben must be refused");
        assert_eq!(after_duplicate_count, 1);
        assert_eq!(after_duplicate_members, "Ava, Ben, Cy");
    }

    #[test]
    fn founder_is_a_moderator_role_and_not_a_unique_key_recipient() {
        let founder = member(1);
        let another_member = member(2);
        let mut space = create_space(SpaceId::from_bytes([9; 20]), founder).unwrap();

        // This represents a completed admission event.  Key recipients must
        // remain role-independent once the member is on the authoritative roster.
        space.members.insert(another_member, SpaceRole::Member);

        assert_eq!(space.role_of(founder), Some(SpaceRole::Moderator));
        assert_eq!(space.role_of(another_member), Some(SpaceRole::Member));
        assert_eq!(
            space.key_recipients().collect::<Vec<_>>(),
            vec![founder, another_member],
            "every active member receives the same epoch material; founder status is only a role"
        );
    }

    #[test]
    fn a_space_with_no_moderator_is_valid_after_the_last_moderator_leaves() {
        let founder = member(3);
        let mut space = create_space(SpaceId::from_bytes([4; 20]), founder).unwrap();

        // The leave event is authored by the C8 lifecycle.  This state model
        // intentionally accepts its resulting empty roster.
        space.members.remove(&founder);

        assert!(space.is_unmoderated());
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
            Err(JoinSpaceError::RotationFailed(
                "rotation unavailable".into()
            ))
        );
        assert_eq!(
            space.role_of(failed),
            None,
            "a failed rotation admits nobody"
        );
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

        space
            .create_channel_key_domain(public, [alice, bob, carol])
            .unwrap();
        space
            .create_channel_key_domain(private, [alice, bob])
            .unwrap();

        let public_domain = space.channel_key_domain(public).unwrap();
        let private_domain = space.channel_key_domain(private).unwrap();
        assert_ne!(public_domain.domain_id(), private_domain.domain_id());
        assert!(public_domain.admits(carol));
        assert!(
            !private_domain.admits(carol),
            "a non-member of #private receives no key domain"
        );
        assert_eq!(
            private_domain.recipients().collect::<Vec<_>>(),
            vec![alice, bob]
        );
    }

    #[test]
    fn t21_t37_moderator_delete_stays_a_request_until_each_member_acknowledges() {
        let bob = member(2);
        let carol = member(3);
        let mut request = ModeratorDeleteForEveryoneRequest {
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
            ModeratorDeleteForEveryoneRequestStatus::RequestQueued {
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
            ModeratorDeleteForEveryoneRequestStatus::RequestServerBlobConfirmed {
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

    #[test]
    fn task_1381_member_cannot_raise_their_own_remove_people_permission() {
        const OWNER: &str = "SERVER-OWNER-1381";
        const MEMBER: &str = "MEMBER-1381";

        let mut saved_list = ServerMemberPermissionList::new(OWNER);
        saved_list.add_read_only_member(MEMBER);

        let owner_first_save = saved_list
            .grant_right(
                OWNER,
                MEMBER,
                ServerMemberRight::ReadAndSend,
                PermissionChangeRoute::Screens,
            )
            .expect("server owner can make MEMBER-1381 read-and-send");
        println!(
            "owner first changes MEMBER-1381 saved rights: {}",
            owner_first_save.join(", ")
        );
        assert_eq!(owner_first_save, vec!["read-only", "read-and-send"]);

        let screen_refusal = saved_list
            .grant_right(
                MEMBER,
                MEMBER,
                ServerMemberRight::RemovePeople,
                PermissionChangeRoute::Screens,
            )
            .expect_err("screen self-escalation must be refused");
        println!("screen attempt refused: {screen_refusal}");
        assert_eq!(
            screen_refusal.to_string(),
            "MEMBER-1381 cannot add remove-people to themselves through screens"
        );
        assert_eq!(
            saved_list.saved_right_names(MEMBER).unwrap(),
            vec!["read-only", "read-and-send"]
        );

        let direct_refusal = saved_list
            .grant_right(
                MEMBER,
                MEMBER,
                ServerMemberRight::RemovePeople,
                PermissionChangeRoute::DirectCommand,
            )
            .expect_err("direct self-escalation must be refused");
        println!("direct attempt refused: {direct_refusal}");
        assert_eq!(
            direct_refusal.to_string(),
            "MEMBER-1381 cannot add remove-people to themselves through direct command"
        );
        let after_self_attempts = saved_list.saved_right_names(MEMBER).unwrap();
        println!(
            "after self attempts MEMBER-1381 saved rights: {}",
            after_self_attempts.join(", ")
        );
        assert_eq!(after_self_attempts, vec!["read-only", "read-and-send"]);

        let owner_final_save = saved_list
            .grant_right(
                OWNER,
                MEMBER,
                ServerMemberRight::RemovePeople,
                PermissionChangeRoute::Screens,
            )
            .expect("server owner can add remove-people");
        println!(
            "owner adds remove-people saved rights: {}",
            owner_final_save.join(", ")
        );
        assert_eq!(
            owner_final_save,
            vec!["read-only", "read-and-send", "remove-people"]
        );
        println!(
            "self-change routes remain {}",
            saved_list.self_change_routes()
        );
        assert_eq!(saved_list.self_change_routes(), 0);
    }

    #[test]
    fn task_4857_saves_all_role_properties_duplicates_and_prunes_expiry() {
        let path = role_store_path("task-4857-roundtrip");
        let mut store = CustomRoleStore::load(&path).expect("empty store loads");
        let properties = filled_role_properties();
        let original = store
            .create_role(properties.clone())
            .expect("15-field role is accepted");
        store.save().expect("custom role store saves");

        let restarted = CustomRoleStore::load(&path).expect("role store reloads after restart");
        let reloaded = restarted
            .role(&original.id)
            .expect("created role survives restart");
        let restart_matches = reloaded == &original;
        println!(
            "TASK4857_RESTART role_id={} stored_property_count={} restart_matches={} name={} colour={} icon={} order={} hoist={} mention_policy={:?} slow_mode_seconds={} longest_mute_seconds={} actions_per_hour_budget={} self_assignable={} auto_grant_on_join={} expires_at_unix_seconds={} duplicate_source_role_id={} template_name={}",
            reloaded.id,
            CustomRoleRecord::stored_field_count(),
            restart_matches,
            reloaded.properties.name,
            reloaded.properties.colour,
            reloaded.properties.icon,
            reloaded.properties.order,
            reloaded.properties.hoist,
            reloaded.properties.mention_policy,
            reloaded.properties.slow_mode_seconds,
            reloaded.properties.longest_mute_seconds,
            reloaded.properties.actions_per_hour_budget,
            reloaded.properties.self_assignable,
            reloaded.properties.auto_grant_on_join,
            reloaded.properties.expires_at_unix_seconds,
            reloaded.properties.duplicate_source_role_id,
            reloaded.properties.template_name
        );
        assert!(restart_matches);
        assert_eq!(CustomRoleRecord::stored_field_count(), 15);

        let duplicate_path = role_store_path("task-4857-duplicate");
        let mut duplicate_store = CustomRoleStore::load(&duplicate_path).expect("duplicate store");
        let duplicate_source = duplicate_store
            .create_role(properties.clone())
            .expect("source role");
        let duplicate = duplicate_store
            .duplicate_role(&duplicate_source.id)
            .expect("duplicate role");
        let copied_value_count = usize::from(duplicate.properties == duplicate_source.properties)
            * CustomRoleProperties::filled_property_count();
        println!(
            "TASK4857_DUPLICATE source_id={} duplicate_id={} ids_differ={} copied_value_count={}",
            duplicate_source.id,
            duplicate.id,
            duplicate_source.id != duplicate.id,
            copied_value_count
        );
        assert_ne!(duplicate_source.id, duplicate.id);
        assert_eq!(copied_value_count, 14);

        let expiry_path = role_store_path("task-4857-expiry");
        let mut expiry_store = CustomRoleStore::load(&expiry_path).expect("expiry store");
        let mut expiring_properties = properties.clone();
        expiring_properties.name = "Temporary watch".to_owned();
        expiring_properties.template_name = "temporary-watch-template".to_owned();
        expiring_properties.expires_at_unix_seconds = 99;
        expiry_store
            .add_template(
                expiring_properties.template_name.clone(),
                expiring_properties.clone(),
            )
            .expect("template saves");
        let template_before = expiry_store
            .template(&expiring_properties.template_name)
            .expect("template exists before prune")
            .clone();
        let expiring = expiry_store
            .create_role(expiring_properties.clone())
            .expect("expiring role");
        for member_id in ["member-a", "member-b", "member-c"] {
            assert!(expiry_store.grant_role_to_member(member_id, &expiring.id));
        }
        let report = expiry_store.prune_expired_roles(100);
        let template_after = expiry_store
            .template(&expiring_properties.template_name)
            .expect("template remains after prune")
            .clone();
        let remaining_member_grants: usize = ["member-a", "member-b", "member-c"]
            .into_iter()
            .map(|member_id| expiry_store.role_ids_for_member(member_id).len())
            .sum();
        println!(
            "TASK4857_EXPIRY expired_role_count={} member_role_removal_count={} remaining_member_grants={} template_untouched={}",
            report.expired_role_count,
            report.member_role_removal_count,
            remaining_member_grants,
            template_before == template_after
        );
        assert_eq!(report.expired_role_count, 1);
        assert_eq!(report.member_role_removal_count, 3);
        assert_eq!(remaining_member_grants, 0);
        assert_eq!(template_before, template_after);

        let missing_refusals: Vec<_> = CUSTOM_ROLE_RECORD_FIELDS
            .iter()
            .map(|field| {
                let error = decode_custom_role_document(&missing_field_store_json(field))
                    .expect_err("missing role property is refused");
                println!("TASK4857_MISSING_REFUSAL field={field} error=\"{error}\"");
                assert!(
                    error.contains(field),
                    "refusal must name missing field {field}, got {error}"
                );
                (*field).to_owned()
            })
            .collect();
        println!(
            "TASK4857_MISSING_REFUSAL_COUNT={} fields={}",
            missing_refusals.len(),
            missing_refusals.join(",")
        );
        assert_eq!(missing_refusals.len(), 15);

        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(duplicate_path);
        let _ = std::fs::remove_file(expiry_path);
    }
}
