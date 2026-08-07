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
    MentionEveryone,
    Invite,
    MakeChannels,
    RemoveMessages,
    RemovePeople,
    ChangeServer,
}

impl ServerPermission {
    pub const ALL: [Self; 8] = [
        Self::Read,
        Self::Send,
        Self::MentionEveryone,
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
            Self::MentionEveryone => "mention-everyone",
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum ServerMentionKind {
    Everyone,
    Here,
    Role,
}

impl ServerMentionKind {
    pub const ALL_TRUST_ROWS: [Self; 3] = [Self::Everyone, Self::Here, Self::Role];

    pub const fn typed_text(self) -> &'static str {
        match self {
            Self::Everyone => "@everyone",
            Self::Here => "@here",
            Self::Role => "@role",
        }
    }

    pub const fn row_name(self) -> &'static str {
        match self {
            Self::Everyone => "everyone",
            Self::Here => "here",
            Self::Role => "role",
        }
    }

    pub const fn trust_row(self) -> MentionPermissionTrust {
        MentionPermissionTrust::Trust
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MentionPermissionTrust {
    Trust,
}

impl MentionPermissionTrust {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Trust => "TRUST",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServerMentionSendReceipt {
    pub accepted: bool,
    pub text: String,
}

pub fn accept_server_mention_text(
    text: impl Into<String>,
) -> Result<ServerMentionSendReceipt, ServerMembershipError> {
    let text = bounded_non_empty(text.into(), "message_text")?;
    Ok(ServerMentionSendReceipt {
        accepted: true,
        text,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HonestMentionRingDecision {
    pub mention: ServerMentionKind,
    pub trust: MentionPermissionTrust,
    pub honest_receiving_apps: usize,
    pub ringing_apps: usize,
    pub highlighted_apps: usize,
}

pub fn honest_mention_ring_decision(
    permissions: &ServerPermissionStore,
    server_id: &str,
    sender_name: &str,
    mention: ServerMentionKind,
    honest_receiving_apps: usize,
    sender_claimed_mention_everyone: bool,
) -> HonestMentionRingDecision {
    let _ = sender_claimed_mention_everyone;
    let can_ring = permissions
        .require_person_permission(server_id, sender_name, ServerPermission::MentionEveryone)
        .is_ok();
    let ringing_apps = if can_ring { honest_receiving_apps } else { 0 };
    HonestMentionRingDecision {
        mention,
        trust: mention.trust_row(),
        honest_receiving_apps,
        ringing_apps,
        highlighted_apps: ringing_apps,
    }
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
}

fn channel_key(server_id: &str, channel_id: &str) -> String {
    format!("{server_id}\n{channel_id}")
}

fn thread_key(server_id: &str, channel_id: &str, thread_id: &str) -> String {
    format!("{server_id}\n{channel_id}\n{thread_id}")
}
