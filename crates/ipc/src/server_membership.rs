use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServerMemberRecord {
    pub name: String,
    pub joined_at: String,
    pub owner: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServerMemberList {
    pub server_id: String,
    pub owner_name: String,
    members_by_name: BTreeMap<String, ServerMemberRecord>,
}

impl ServerMemberList {
    pub fn new(
        server_id: impl Into<String>,
        owner_name: impl Into<String>,
        owner_joined_at: impl Into<String>,
    ) -> Result<Self, ServerMembershipError> {
        let server_id = bounded_non_empty(server_id.into(), "server_id")?;
        let owner_name = bounded_non_empty(owner_name.into(), "owner_name")?;
        let owner_joined_at = bounded_non_empty(owner_joined_at.into(), "joined_at")?;
        let owner = ServerMemberRecord {
            name: owner_name.clone(),
            joined_at: owner_joined_at,
            owner: true,
        };
        Ok(Self {
            server_id,
            owner_name: owner_name.clone(),
            members_by_name: BTreeMap::from([(owner_name, owner)]),
        })
    }

    pub fn add_member(
        &mut self,
        name: impl Into<String>,
        joined_at: impl Into<String>,
    ) -> Result<bool, ServerMembershipError> {
        let name = bounded_non_empty(name.into(), "member_name")?;
        let joined_at = bounded_non_empty(joined_at.into(), "joined_at")?;
        if self.members_by_name.contains_key(&name) {
            return Ok(false);
        }
        self.members_by_name.insert(
            name.clone(),
            ServerMemberRecord {
                name,
                joined_at,
                owner: false,
            },
        );
        Ok(true)
    }

    pub fn remove_member_by_name(&mut self, name: &str) -> Result<bool, ServerMembershipError> {
        let name = bounded_non_empty(name.to_owned(), "member_name")?;
        if name == self.owner_name {
            return Err(ServerMembershipError::CannotRemoveOwner {
                owner_name: self.owner_name.clone(),
            });
        }
        Ok(self.members_by_name.remove(&name).is_some())
    }

    pub fn members(&self) -> Vec<ServerMemberRecord> {
        let mut members: Vec<_> = self.members_by_name.values().cloned().collect();
        members.sort_by(|left, right| {
            right
                .owner
                .cmp(&left.owner)
                .then_with(|| left.joined_at.cmp(&right.joined_at))
                .then_with(|| left.name.cmp(&right.name))
        });
        members
    }

    pub fn member_count(&self) -> usize {
        self.members_by_name.len()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerMembershipStore {
    servers: BTreeMap<String, ServerMemberList>,
}

impl ServerMembershipStore {
    pub fn upsert_server(
        &mut self,
        server_id: String,
        owner_name: String,
        owner_joined_at: String,
    ) -> Result<ServerMemberList, ServerMembershipError> {
        let list = ServerMemberList::new(server_id.clone(), owner_name, owner_joined_at)?;
        self.servers.insert(server_id, list.clone());
        Ok(list)
    }

    pub fn add_member(
        &mut self,
        server_id: &str,
        name: String,
        joined_at: String,
    ) -> Result<usize, ServerMembershipError> {
        let list = self.servers.get_mut(server_id).ok_or_else(|| {
            ServerMembershipError::ServerNotFound {
                server_id: server_id.to_owned(),
            }
        })?;
        let before = list.member_count();
        let _ = list.add_member(name, joined_at)?;
        Ok(list.member_count() - before)
    }

    pub fn remove_member_by_name(
        &mut self,
        server_id: &str,
        name: String,
    ) -> Result<usize, ServerMembershipError> {
        let list = self.servers.get_mut(server_id).ok_or_else(|| {
            ServerMembershipError::ServerNotFound {
                server_id: server_id.to_owned(),
            }
        })?;
        Ok(usize::from(list.remove_member_by_name(&name)?))
    }

    pub fn list(&self, server_id: &str) -> Result<ServerMemberList, ServerMembershipError> {
        self.servers
            .get(server_id)
            .cloned()
            .ok_or_else(|| ServerMembershipError::ServerNotFound {
                server_id: server_id.to_owned(),
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ServerMembershipError {
    #[error("OSL: {field} is empty")]
    EmptyField { field: &'static str },
    #[error("OSL: {field} is too long")]
    FieldTooLong { field: &'static str },
    #[error("OSL: server member list not found for {server_id}")]
    ServerNotFound { server_id: String },
    #[error("OSL: cannot remove server owner {owner_name}")]
    CannotRemoveOwner { owner_name: String },
}

fn bounded_non_empty(value: String, field: &'static str) -> Result<String, ServerMembershipError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ServerMembershipError::EmptyField { field });
    }
    if trimmed.len() > 128 {
        return Err(ServerMembershipError::FieldTooLong { field });
    }
    Ok(trimmed.to_owned())
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum ServerPermission {
    Read,
    Send,
    Invite,
    MakeChannels,
    RemoveMessages,
    RemovePeople,
    ChangeServer,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum RolePermissionOverrideRole {
    Owner,
    Moderator,
    Member,
}

impl RolePermissionOverrideRole {
    pub const ALL: [Self; 3] = [Self::Owner, Self::Moderator, Self::Member];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Moderator => "moderator",
            Self::Member => "member",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum RolePermissionOverridePermission {
    ReadMessages,
    SendMessages,
    AttachFiles,
    EmbedLinks,
    AddReactions,
    UseEmoji,
    CreateThreads,
    ReplyThreads,
    ManageThreads,
    PinMessages,
    DeleteOwnMessages,
    DeleteAnyMessages,
    EditOwnMessages,
    EditAnyMessages,
    MentionEveryone,
    MentionRoles,
    InvitePeople,
    RemovePeople,
    MutePeople,
    TimeoutPeople,
    BanPeople,
    KickPeople,
    ChangeChannelName,
    ChangeChannelTopic,
    ChangeSlowMode,
    ChangePermissions,
    ViewAuditLog,
    ManageRoles,
    ManageWebhooks,
    ManageIntegrations,
    StartVoice,
    JoinVoice,
    SpeakVoice,
    MuteVoice,
    DeafenVoice,
    StreamVoice,
    ShareScreen,
    StartPolls,
    VotePolls,
    ManagePolls,
}

impl RolePermissionOverridePermission {
    pub const ALL: [Self; 40] = [
        Self::ReadMessages,
        Self::SendMessages,
        Self::AttachFiles,
        Self::EmbedLinks,
        Self::AddReactions,
        Self::UseEmoji,
        Self::CreateThreads,
        Self::ReplyThreads,
        Self::ManageThreads,
        Self::PinMessages,
        Self::DeleteOwnMessages,
        Self::DeleteAnyMessages,
        Self::EditOwnMessages,
        Self::EditAnyMessages,
        Self::MentionEveryone,
        Self::MentionRoles,
        Self::InvitePeople,
        Self::RemovePeople,
        Self::MutePeople,
        Self::TimeoutPeople,
        Self::BanPeople,
        Self::KickPeople,
        Self::ChangeChannelName,
        Self::ChangeChannelTopic,
        Self::ChangeSlowMode,
        Self::ChangePermissions,
        Self::ViewAuditLog,
        Self::ManageRoles,
        Self::ManageWebhooks,
        Self::ManageIntegrations,
        Self::StartVoice,
        Self::JoinVoice,
        Self::SpeakVoice,
        Self::MuteVoice,
        Self::DeafenVoice,
        Self::StreamVoice,
        Self::ShareScreen,
        Self::StartPolls,
        Self::VotePolls,
        Self::ManagePolls,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::ReadMessages => "read messages",
            Self::SendMessages => "send messages",
            Self::AttachFiles => "attach files",
            Self::EmbedLinks => "embed links",
            Self::AddReactions => "add reactions",
            Self::UseEmoji => "use emoji",
            Self::CreateThreads => "create threads",
            Self::ReplyThreads => "reply threads",
            Self::ManageThreads => "manage threads",
            Self::PinMessages => "pin messages",
            Self::DeleteOwnMessages => "delete own messages",
            Self::DeleteAnyMessages => "delete any messages",
            Self::EditOwnMessages => "edit own messages",
            Self::EditAnyMessages => "edit any messages",
            Self::MentionEveryone => "mention everyone",
            Self::MentionRoles => "mention roles",
            Self::InvitePeople => "invite people",
            Self::RemovePeople => "remove people",
            Self::MutePeople => "mute people",
            Self::TimeoutPeople => "timeout people",
            Self::BanPeople => "ban people",
            Self::KickPeople => "kick people",
            Self::ChangeChannelName => "change channel name",
            Self::ChangeChannelTopic => "change channel topic",
            Self::ChangeSlowMode => "change slow mode",
            Self::ChangePermissions => "change permissions",
            Self::ViewAuditLog => "view audit log",
            Self::ManageRoles => "manage roles",
            Self::ManageWebhooks => "manage webhooks",
            Self::ManageIntegrations => "manage integrations",
            Self::StartVoice => "start voice",
            Self::JoinVoice => "join voice",
            Self::SpeakVoice => "speak voice",
            Self::MuteVoice => "mute voice",
            Self::DeafenVoice => "deafen voice",
            Self::StreamVoice => "stream voice",
            Self::ShareScreen => "share screen",
            Self::StartPolls => "start polls",
            Self::VotePolls => "vote polls",
            Self::ManagePolls => "manage polls",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RolePermissionOverrideState {
    Allow,
    Deny,
    Inherit,
}

impl RolePermissionOverrideState {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
            Self::Inherit => "inherit",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RolePermissionOverrideCell {
    pub role: RolePermissionOverrideRole,
    pub permission: RolePermissionOverridePermission,
    pub state: RolePermissionOverrideState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RolePermissionOverrideTable {
    pub server_id: String,
    pub channel_id: String,
    pub thread_id: Option<String>,
    pub cells: Vec<RolePermissionOverrideCell>,
    pub saved_cell_count: usize,
    pub explicit_grant_row_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct RolePermissionOverrideKey {
    role: RolePermissionOverrideRole,
    permission: RolePermissionOverridePermission,
}

impl RolePermissionOverrideKey {
    fn from_cell(cell: &RolePermissionOverrideCell) -> Self {
        Self {
            role: cell.role,
            permission: cell.permission,
        }
    }

    fn storage_key(self) -> String {
        format!("{}\n{}", self.role.name(), self.permission.name())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct RolePermissionOverrideScope {
    server_id: String,
    channel_id: String,
    thread_id: Option<String>,
    rows: BTreeMap<String, RolePermissionOverrideState>,
}

impl RolePermissionOverrideScope {
    fn new(server_id: String, channel_id: String, thread_id: Option<String>) -> Self {
        Self {
            server_id,
            channel_id,
            thread_id,
            rows: BTreeMap::new(),
        }
    }

    fn set_cell(&mut self, cell: RolePermissionOverrideCell) {
        let key = RolePermissionOverrideKey::from_cell(&cell).storage_key();
        match cell.state {
            RolePermissionOverrideState::Allow | RolePermissionOverrideState::Deny => {
                self.rows.insert(key, cell.state);
            }
            RolePermissionOverrideState::Inherit => {
                self.rows.remove(&key);
            }
        }
    }

    fn cell_state(
        &self,
        role: RolePermissionOverrideRole,
        permission: RolePermissionOverridePermission,
    ) -> RolePermissionOverrideState {
        self.rows
            .get(&RolePermissionOverrideKey { role, permission }.storage_key())
            .copied()
            .unwrap_or(RolePermissionOverrideState::Inherit)
    }

    fn table(&self) -> RolePermissionOverrideTable {
        let cells = RolePermissionOverrideRole::ALL
            .into_iter()
            .flat_map(|role| {
                RolePermissionOverridePermission::ALL
                    .into_iter()
                    .map(move |permission| RolePermissionOverrideCell {
                        role,
                        permission,
                        state: self.cell_state(role, permission),
                    })
            })
            .collect::<Vec<_>>();
        RolePermissionOverrideTable {
            server_id: self.server_id.clone(),
            channel_id: self.channel_id.clone(),
            thread_id: self.thread_id.clone(),
            saved_cell_count: cells.len(),
            explicit_grant_row_count: self.rows.len(),
            cells,
        }
    }
}

pub const CHANNEL_REACTION_SETTING_SENTENCE: &str =
    "This reaction setting runs in every honest app the same way automatic rules do.";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ChannelReactionPolicyMode {
    All,
    ChosenSet,
    None,
}

impl ChannelReactionPolicyMode {
    pub const fn name(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::ChosenSet => "chosen set",
            Self::None => "none",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChannelReactionPolicy {
    pub server_id: String,
    pub channel_id: String,
    pub mode: ChannelReactionPolicyMode,
    pub allowed_emoji: Vec<String>,
    pub setting_sentence: String,
}

impl ChannelReactionPolicy {
    fn all(server_id: String, channel_id: String) -> Self {
        Self {
            server_id,
            channel_id,
            mode: ChannelReactionPolicyMode::All,
            allowed_emoji: Vec::new(),
            setting_sentence: CHANNEL_REACTION_SETTING_SENTENCE.to_owned(),
        }
    }

    fn allows(&self, emoji: &str) -> bool {
        match self.mode {
            ChannelReactionPolicyMode::All => true,
            ChannelReactionPolicyMode::ChosenSet => {
                self.allowed_emoji.iter().any(|allowed| allowed == emoji)
            }
            ChannelReactionPolicyMode::None => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChannelReactionResult {
    pub server_id: String,
    pub channel_id: String,
    pub message_id: String,
    pub actor_name: String,
    pub emoji: String,
    pub landed: bool,
    pub reaction_count: usize,
    pub policy_mode: ChannelReactionPolicyMode,
    pub setting_sentence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ChannelReactionRecord {
    server_id: String,
    channel_id: String,
    message_id: String,
    actor_name: String,
    emoji: String,
}

impl ServerPermission {
    pub const ALL: [Self; 7] = [
        Self::Read,
        Self::Send,
        Self::Invite,
        Self::MakeChannels,
        Self::RemoveMessages,
        Self::RemovePeople,
        Self::ChangeServer,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Send => "send",
            Self::Invite => "invite",
            Self::MakeChannels => "make channels",
            Self::RemoveMessages => "remove messages",
            Self::RemovePeople => "remove people",
            Self::ChangeServer => "change server",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServerPermissionGrant {
    pub server_id: String,
    pub person_name: String,
    permissions: BTreeSet<ServerPermission>,
}

impl ServerPermissionGrant {
    fn new(
        server_id: impl Into<String>,
        person_name: impl Into<String>,
        permissions: impl IntoIterator<Item = ServerPermission>,
    ) -> Result<Self, ServerMembershipError> {
        let server_id = bounded_non_empty(server_id.into(), "server_id")?;
        let person_name = bounded_non_empty(person_name.into(), "person_name")?;
        Ok(Self {
            server_id,
            person_name,
            permissions: permissions.into_iter().collect(),
        })
    }

    pub fn permissions(&self) -> Vec<ServerPermission> {
        self.permissions.iter().copied().collect()
    }

    pub fn allows(&self, permission: ServerPermission) -> bool {
        self.permissions.contains(&permission)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerPermissionStore {
    grants_by_server_person: BTreeMap<String, ServerPermissionGrant>,
}

impl ServerPermissionStore {
    pub fn set_person_permissions(
        &mut self,
        server_id: String,
        person_name: String,
        permissions: Vec<ServerPermission>,
    ) -> Result<ServerPermissionGrant, ServerMembershipError> {
        let grant = ServerPermissionGrant::new(server_id, person_name, permissions)?;
        let key = grant_key(&grant.server_id, &grant.person_name);
        self.grants_by_server_person.insert(key, grant.clone());
        Ok(grant)
    }

    pub fn require_person_permission(
        &self,
        server_id: &str,
        person_name: &str,
        permission: ServerPermission,
    ) -> Result<(), ServerPermissionError> {
        let server_id = bounded_non_empty(server_id.to_owned(), "server_id")
            .map_err(ServerPermissionError::Membership)?;
        let person_name = bounded_non_empty(person_name.to_owned(), "person_name")
            .map_err(ServerPermissionError::Membership)?;
        let key = grant_key(&server_id, &person_name);
        let allowed = self
            .grants_by_server_person
            .get(&key)
            .is_some_and(|grant| grant.allows(permission));
        if allowed {
            Ok(())
        } else {
            Err(ServerPermissionError::Refused {
                person_name,
                permission,
            })
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ServerPermissionError {
    #[error("{0}")]
    Membership(ServerMembershipError),
    #[error("OSL: {person_name} is not allowed to {permission}", permission = permission.name())]
    Refused {
        person_name: String,
        permission: ServerPermission,
    },
}

fn grant_key(server_id: &str, person_name: &str) -> String {
    format!("{server_id}\n{person_name}")
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ServerChannelAccess {
    Open,
    Limited { person_names: BTreeSet<String> },
}

impl ServerChannelAccess {
    fn limited(
        person_names: impl IntoIterator<Item = String>,
    ) -> Result<Self, ServerMembershipError> {
        let mut bounded = BTreeSet::new();
        for person_name in person_names {
            bounded.insert(bounded_non_empty(person_name, "person_name")?);
        }
        Ok(Self::Limited {
            person_names: bounded,
        })
    }

    fn effective_readers(&self, members: &ServerMemberList) -> Vec<String> {
        match self {
            Self::Open => members
                .members()
                .into_iter()
                .map(|member| member.name)
                .collect(),
            Self::Limited { person_names } => person_names.iter().cloned().collect(),
        }
    }

    fn admits(&self, members: &ServerMemberList, person_name: &str) -> bool {
        match self {
            Self::Open => members.members_by_name.contains_key(person_name),
            Self::Limited { person_names } => person_names.contains(person_name),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServerChannelAccessRecord {
    pub server_id: String,
    pub channel_id: String,
    pub access: ServerChannelAccess,
}

impl ServerChannelAccessRecord {
    pub fn effective_readers(&self, members: &ServerMemberList) -> Vec<String> {
        self.access.effective_readers(members)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ThreadMessageRecord {
    pub message_id: String,
    pub body: String,
    pub marked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServerThreadRecord {
    pub server_id: String,
    pub channel_id: String,
    pub thread_id: String,
    messages: Vec<ThreadMessageRecord>,
}

impl ServerThreadRecord {
    pub fn marked_messages(&self) -> Vec<ThreadMessageRecord> {
        self.messages
            .iter()
            .filter(|message| message.marked)
            .cloned()
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServerThreadRead {
    pub server_id: String,
    pub channel_id: String,
    pub thread_id: String,
    pub reader_name: String,
    pub messages: Vec<ThreadMessageRecord>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerThreadPermissionStore {
    channels: BTreeMap<String, ServerChannelAccessRecord>,
    threads: BTreeMap<String, ServerThreadRecord>,
    #[serde(default)]
    role_overrides: BTreeMap<String, RolePermissionOverrideScope>,
    #[serde(default)]
    channel_reaction_policies: BTreeMap<String, ChannelReactionPolicy>,
    #[serde(default)]
    channel_reactions: BTreeMap<String, ChannelReactionRecord>,
}

impl ServerThreadPermissionStore {
    pub fn set_limited_channel(
        &mut self,
        members: &ServerMemberList,
        channel_id: String,
        person_names: Vec<String>,
    ) -> Result<ServerChannelAccessRecord, ServerThreadPermissionError> {
        let channel_id = bounded_non_empty(channel_id, "channel_id")
            .map_err(ServerThreadPermissionError::Membership)?;
        let access = ServerChannelAccess::limited(person_names)
            .map_err(ServerThreadPermissionError::Membership)?;
        if let ServerChannelAccess::Limited { person_names } = &access {
            for person_name in person_names {
                if !members.members_by_name.contains_key(person_name) {
                    return Err(ServerThreadPermissionError::UnknownChannelPerson {
                        person_name: person_name.clone(),
                    });
                }
            }
        }
        let record = ServerChannelAccessRecord {
            server_id: members.server_id.clone(),
            channel_id,
            access,
        };
        self.channels.insert(
            channel_key(&record.server_id, &record.channel_id),
            record.clone(),
        );
        Ok(record)
    }

    pub fn set_open_channel(
        &mut self,
        members: &ServerMemberList,
        channel_id: String,
    ) -> Result<ServerChannelAccessRecord, ServerThreadPermissionError> {
        let channel_id = bounded_non_empty(channel_id, "channel_id")
            .map_err(ServerThreadPermissionError::Membership)?;
        let record = ServerChannelAccessRecord {
            server_id: members.server_id.clone(),
            channel_id,
            access: ServerChannelAccess::Open,
        };
        self.channels.insert(
            channel_key(&record.server_id, &record.channel_id),
            record.clone(),
        );
        Ok(record)
    }

    pub fn create_thread(
        &mut self,
        server_id: String,
        channel_id: String,
        thread_id: String,
    ) -> Result<ServerThreadRecord, ServerThreadPermissionError> {
        let server_id = bounded_non_empty(server_id, "server_id")
            .map_err(ServerThreadPermissionError::Membership)?;
        let channel_id = bounded_non_empty(channel_id, "channel_id")
            .map_err(ServerThreadPermissionError::Membership)?;
        let thread_id = bounded_non_empty(thread_id, "thread_id")
            .map_err(ServerThreadPermissionError::Membership)?;
        self.channel(&server_id, &channel_id)?;
        let record = ServerThreadRecord {
            server_id,
            channel_id,
            thread_id,
            messages: Vec::new(),
        };
        self.threads.insert(
            thread_key(&record.server_id, &record.channel_id, &record.thread_id),
            record.clone(),
        );
        Ok(record)
    }

    pub fn add_thread_message(
        &mut self,
        server_id: String,
        channel_id: String,
        thread_id: String,
        message_id: String,
        body: String,
        marked: bool,
    ) -> Result<usize, ServerThreadPermissionError> {
        let server_id = bounded_non_empty(server_id, "server_id")
            .map_err(ServerThreadPermissionError::Membership)?;
        let channel_id = bounded_non_empty(channel_id, "channel_id")
            .map_err(ServerThreadPermissionError::Membership)?;
        let thread_id = bounded_non_empty(thread_id, "thread_id")
            .map_err(ServerThreadPermissionError::Membership)?;
        let message_id = bounded_non_empty(message_id, "message_id")
            .map_err(ServerThreadPermissionError::Membership)?;
        let body = bounded_non_empty(body, "message_body")
            .map_err(ServerThreadPermissionError::Membership)?;
        let key = thread_key(&server_id, &channel_id, &thread_id);
        let thread = self.threads.get_mut(&key).ok_or_else(|| {
            ServerThreadPermissionError::ThreadNotFound {
                thread_id: thread_id.clone(),
            }
        })?;
        thread.messages.push(ThreadMessageRecord {
            message_id,
            body,
            marked,
        });
        Ok(thread.messages.len())
    }

    pub fn read_thread(
        &self,
        members: &ServerMemberList,
        channel_id: &str,
        thread_id: &str,
        person_name: &str,
    ) -> Result<ServerThreadRead, ServerThreadPermissionError> {
        let channel = self.channel(&members.server_id, channel_id)?;
        let person_name = bounded_non_empty(person_name.to_owned(), "person_name")
            .map_err(ServerThreadPermissionError::Membership)?;
        if !channel.access.admits(members, &person_name) {
            return Err(ServerThreadPermissionError::ChannelReadRefused { person_name });
        }
        let thread = self.thread(&members.server_id, channel_id, thread_id)?;
        Ok(ServerThreadRead {
            server_id: members.server_id.clone(),
            channel_id: channel_id.to_owned(),
            thread_id: thread_id.to_owned(),
            reader_name: person_name,
            messages: thread.marked_messages(),
        })
    }

    pub fn effective_thread_readers(
        &self,
        members: &ServerMemberList,
        channel_id: &str,
        thread_id: &str,
    ) -> Result<Vec<String>, ServerThreadPermissionError> {
        let channel = self.channel(&members.server_id, channel_id)?;
        self.thread(&members.server_id, channel_id, thread_id)?;
        Ok(channel.effective_readers(members))
    }

    pub fn set_thread_permissions(
        &mut self,
        members: &ServerMemberList,
        channel_id: &str,
        thread_id: &str,
        requested_person_names: Vec<String>,
    ) -> Result<usize, ServerThreadPermissionError> {
        self.thread(&members.server_id, channel_id, thread_id)?;
        let channel = self.channel(&members.server_id, channel_id)?;
        let channel_readers: BTreeSet<_> = channel.effective_readers(members).into_iter().collect();
        let requested = ServerChannelAccess::limited(requested_person_names)
            .map_err(ServerThreadPermissionError::Membership)?;
        let ServerChannelAccess::Limited { person_names } = requested else {
            unreachable!("limited constructor always returns Limited");
        };
        if !person_names.is_subset(&channel_readers) {
            return Err(ServerThreadPermissionError::ThreadMoreOpenThanChannel {
                thread_id: thread_id.to_owned(),
                channel_id: channel_id.to_owned(),
            });
        }
        Ok(0)
    }

    pub fn set_channel_role_permission_overrides(
        &mut self,
        server_id: String,
        channel_id: String,
        cells: Vec<RolePermissionOverrideCell>,
    ) -> Result<RolePermissionOverrideTable, ServerThreadPermissionError> {
        let server_id = bounded_non_empty(server_id, "server_id")
            .map_err(ServerThreadPermissionError::Membership)?;
        let channel_id = bounded_non_empty(channel_id, "channel_id")
            .map_err(ServerThreadPermissionError::Membership)?;
        self.channel(&server_id, &channel_id)?;
        validate_override_table(&cells)?;

        let key = role_override_scope_key(&server_id, &channel_id, None);
        let scope = self
            .role_overrides
            .entry(key)
            .or_insert_with(|| RolePermissionOverrideScope::new(server_id, channel_id, None));
        for cell in cells {
            scope.set_cell(cell);
        }
        Ok(scope.table())
    }

    pub fn read_channel_role_permission_overrides(
        &self,
        server_id: String,
        channel_id: String,
    ) -> Result<RolePermissionOverrideTable, ServerThreadPermissionError> {
        let server_id = bounded_non_empty(server_id, "server_id")
            .map_err(ServerThreadPermissionError::Membership)?;
        let channel_id = bounded_non_empty(channel_id, "channel_id")
            .map_err(ServerThreadPermissionError::Membership)?;
        self.channel(&server_id, &channel_id)?;
        let key = role_override_scope_key(&server_id, &channel_id, None);
        Ok(self
            .role_overrides
            .get(&key)
            .cloned()
            .unwrap_or_else(|| RolePermissionOverrideScope::new(server_id, channel_id, None))
            .table())
    }

    pub fn set_thread_role_permission_overrides(
        &mut self,
        server_id: String,
        channel_id: String,
        thread_id: String,
        cells: Vec<RolePermissionOverrideCell>,
    ) -> Result<RolePermissionOverrideTable, ServerThreadPermissionError> {
        let server_id = bounded_non_empty(server_id, "server_id")
            .map_err(ServerThreadPermissionError::Membership)?;
        let channel_id = bounded_non_empty(channel_id, "channel_id")
            .map_err(ServerThreadPermissionError::Membership)?;
        let thread_id = bounded_non_empty(thread_id, "thread_id")
            .map_err(ServerThreadPermissionError::Membership)?;
        self.thread(&server_id, &channel_id, &thread_id)?;
        validate_override_table(&cells)?;

        let channel_key = role_override_scope_key(&server_id, &channel_id, None);
        let parent = self
            .role_overrides
            .get(&channel_key)
            .cloned()
            .unwrap_or_else(|| {
                RolePermissionOverrideScope::new(server_id.clone(), channel_id.clone(), None)
            });
        for cell in &cells {
            if cell.state == RolePermissionOverrideState::Allow
                && parent.cell_state(cell.role, cell.permission)
                    == RolePermissionOverrideState::Deny
            {
                return Err(ServerThreadPermissionError::ThreadOverrideMoreOpenThanChannel);
            }
        }

        let key = role_override_scope_key(&server_id, &channel_id, Some(&thread_id));
        let scope = self.role_overrides.entry(key).or_insert_with(|| {
            RolePermissionOverrideScope::new(server_id, channel_id, Some(thread_id))
        });
        for cell in cells {
            scope.set_cell(cell);
        }
        Ok(scope.table())
    }

    pub fn read_thread_role_permission_overrides(
        &self,
        server_id: String,
        channel_id: String,
        thread_id: String,
    ) -> Result<RolePermissionOverrideTable, ServerThreadPermissionError> {
        let server_id = bounded_non_empty(server_id, "server_id")
            .map_err(ServerThreadPermissionError::Membership)?;
        let channel_id = bounded_non_empty(channel_id, "channel_id")
            .map_err(ServerThreadPermissionError::Membership)?;
        let thread_id = bounded_non_empty(thread_id, "thread_id")
            .map_err(ServerThreadPermissionError::Membership)?;
        self.thread(&server_id, &channel_id, &thread_id)?;
        let key = role_override_scope_key(&server_id, &channel_id, Some(&thread_id));
        Ok(self
            .role_overrides
            .get(&key)
            .cloned()
            .unwrap_or_else(|| {
                RolePermissionOverrideScope::new(server_id, channel_id, Some(thread_id))
            })
            .table())
    }

    pub fn set_channel_reaction_policy(
        &mut self,
        server_id: String,
        channel_id: String,
        mode: ChannelReactionPolicyMode,
        allowed_emoji: Vec<String>,
    ) -> Result<ChannelReactionPolicy, ServerThreadPermissionError> {
        let server_id = bounded_non_empty(server_id, "server_id")
            .map_err(ServerThreadPermissionError::Membership)?;
        let channel_id = bounded_non_empty(channel_id, "channel_id")
            .map_err(ServerThreadPermissionError::Membership)?;
        self.channel(&server_id, &channel_id)?;
        let allowed_emoji = normalize_allowed_reaction_set(mode, allowed_emoji)?;
        let policy = ChannelReactionPolicy {
            server_id,
            channel_id,
            mode,
            allowed_emoji,
            setting_sentence: CHANNEL_REACTION_SETTING_SENTENCE.to_owned(),
        };
        self.channel_reaction_policies.insert(
            channel_key(&policy.server_id, &policy.channel_id),
            policy.clone(),
        );
        Ok(policy)
    }

    pub fn read_channel_reaction_policy(
        &self,
        server_id: String,
        channel_id: String,
    ) -> Result<ChannelReactionPolicy, ServerThreadPermissionError> {
        let server_id = bounded_non_empty(server_id, "server_id")
            .map_err(ServerThreadPermissionError::Membership)?;
        let channel_id = bounded_non_empty(channel_id, "channel_id")
            .map_err(ServerThreadPermissionError::Membership)?;
        self.channel(&server_id, &channel_id)?;
        Ok(self
            .channel_reaction_policies
            .get(&channel_key(&server_id, &channel_id))
            .cloned()
            .unwrap_or_else(|| ChannelReactionPolicy::all(server_id, channel_id)))
    }

    pub fn add_channel_reaction(
        &mut self,
        server_id: String,
        channel_id: String,
        message_id: String,
        actor_name: String,
        emoji: String,
    ) -> Result<ChannelReactionResult, ServerThreadPermissionError> {
        let server_id = bounded_non_empty(server_id, "server_id")
            .map_err(ServerThreadPermissionError::Membership)?;
        let channel_id = bounded_non_empty(channel_id, "channel_id")
            .map_err(ServerThreadPermissionError::Membership)?;
        let message_id = bounded_non_empty(message_id, "message_id")
            .map_err(ServerThreadPermissionError::Membership)?;
        let actor_name = bounded_non_empty(actor_name, "actor_name")
            .map_err(ServerThreadPermissionError::Membership)?;
        validate_reaction_emoji(&emoji)?;
        self.channel(&server_id, &channel_id)?;
        let policy = self.read_channel_reaction_policy(server_id.clone(), channel_id.clone())?;
        if !policy.allows(&emoji) {
            return Err(ServerThreadPermissionError::ReactionRefused { emoji });
        }
        let key = channel_reaction_key(&server_id, &channel_id, &message_id, &actor_name, &emoji);
        self.channel_reactions
            .entry(key)
            .or_insert_with(|| ChannelReactionRecord {
                server_id: server_id.clone(),
                channel_id: channel_id.clone(),
                message_id: message_id.clone(),
                actor_name: actor_name.clone(),
                emoji: emoji.clone(),
            });
        Ok(ChannelReactionResult {
            reaction_count: self.channel_reaction_count(&server_id, &channel_id, &message_id),
            server_id,
            channel_id,
            message_id,
            actor_name,
            emoji,
            landed: true,
            policy_mode: policy.mode,
            setting_sentence: policy.setting_sentence,
        })
    }

    pub fn channel_reaction_count(
        &self,
        server_id: &str,
        channel_id: &str,
        message_id: &str,
    ) -> usize {
        self.channel_reactions
            .values()
            .filter(|record| {
                record.server_id == server_id
                    && record.channel_id == channel_id
                    && record.message_id == message_id
            })
            .count()
    }

    fn channel(
        &self,
        server_id: &str,
        channel_id: &str,
    ) -> Result<&ServerChannelAccessRecord, ServerThreadPermissionError> {
        self.channels
            .get(&channel_key(server_id, channel_id))
            .ok_or_else(|| ServerThreadPermissionError::ChannelNotFound {
                channel_id: channel_id.to_owned(),
            })
    }

    fn thread(
        &self,
        server_id: &str,
        channel_id: &str,
        thread_id: &str,
    ) -> Result<&ServerThreadRecord, ServerThreadPermissionError> {
        self.threads
            .get(&thread_key(server_id, channel_id, thread_id))
            .ok_or_else(|| ServerThreadPermissionError::ThreadNotFound {
                thread_id: thread_id.to_owned(),
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ServerThreadPermissionError {
    #[error("{0}")]
    Membership(ServerMembershipError),
    #[error("OSL: channel permissions not found for {channel_id}")]
    ChannelNotFound { channel_id: String },
    #[error("OSL: thread not found for {thread_id}")]
    ThreadNotFound { thread_id: String },
    #[error("OSL: channel member not found for {person_name}")]
    UnknownChannelPerson { person_name: String },
    #[error("OSL: channel read refused for {person_name}")]
    ChannelReadRefused { person_name: String },
    #[error("OSL: thread {thread_id} cannot be more open than channel {channel_id}")]
    ThreadMoreOpenThanChannel {
        thread_id: String,
        channel_id: String,
    },
    #[error("thread cannot be more open than channel")]
    ThreadOverrideMoreOpenThanChannel,
    #[error("OSL: role override table must contain exactly 120 unique cells")]
    InvalidRoleOverrideTable,
    #[error("OSL: channel reaction emoji is invalid")]
    InvalidReactionEmoji,
    #[error("OSL: chosen reaction set must contain at least one emoji")]
    EmptyReactionSet,
    #[error("OSL: reaction {emoji} is not allowed in this channel")]
    ReactionRefused { emoji: String },
}

fn channel_key(server_id: &str, channel_id: &str) -> String {
    format!("{server_id}\n{channel_id}")
}

fn thread_key(server_id: &str, channel_id: &str, thread_id: &str) -> String {
    format!("{server_id}\n{channel_id}\n{thread_id}")
}

fn role_override_scope_key(server_id: &str, channel_id: &str, thread_id: Option<&str>) -> String {
    match thread_id {
        Some(thread_id) => format!("{server_id}\n{channel_id}\n{thread_id}"),
        None => format!("{server_id}\n{channel_id}"),
    }
}

fn channel_reaction_key(
    server_id: &str,
    channel_id: &str,
    message_id: &str,
    actor_name: &str,
    emoji: &str,
) -> String {
    format!("{server_id}\n{channel_id}\n{message_id}\n{actor_name}\n{emoji}")
}

fn validate_reaction_emoji(emoji: &str) -> Result<(), ServerThreadPermissionError> {
    if emoji.is_empty()
        || emoji.len() > 64
        || emoji.trim() != emoji
        || emoji.chars().any(char::is_control)
    {
        Err(ServerThreadPermissionError::InvalidReactionEmoji)
    } else {
        Ok(())
    }
}

fn normalize_allowed_reaction_set(
    mode: ChannelReactionPolicyMode,
    allowed_emoji: Vec<String>,
) -> Result<Vec<String>, ServerThreadPermissionError> {
    if mode != ChannelReactionPolicyMode::ChosenSet {
        return Ok(Vec::new());
    }
    let mut unique = BTreeSet::new();
    for emoji in allowed_emoji {
        validate_reaction_emoji(&emoji)?;
        unique.insert(emoji);
    }
    if unique.is_empty() {
        Err(ServerThreadPermissionError::EmptyReactionSet)
    } else {
        Ok(unique.into_iter().collect())
    }
}

fn validate_override_table(
    cells: &[RolePermissionOverrideCell],
) -> Result<(), ServerThreadPermissionError> {
    let expected =
        RolePermissionOverrideRole::ALL.len() * RolePermissionOverridePermission::ALL.len();
    let unique = cells
        .iter()
        .map(RolePermissionOverrideKey::from_cell)
        .collect::<BTreeSet<_>>();
    if cells.len() == expected && unique.len() == expected {
        Ok(())
    } else {
        Err(ServerThreadPermissionError::InvalidRoleOverrideTable)
    }
}
