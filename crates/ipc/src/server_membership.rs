use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
