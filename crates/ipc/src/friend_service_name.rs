//! Which name a friend uses on each service (X, Instagram, or Messenger).

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

const BINDING_VERSION: u32 = 1;
const BINDING_STATE_FILE: &str = "friend_service_names.json";
const MAX_BINDINGS: usize = 1_000;

/// The services a friend can be bound to a name on.
///
/// Deliberately closed: a made-up service id must be refused by name rather
/// than silently accepted, so [`parse_friend_service_id`] is the only path
/// that produces one of these.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FriendServiceKind {
    X,
    Instagram,
    Messenger,
}

impl FriendServiceKind {
    pub const ALL: [Self; 3] = [Self::X, Self::Instagram, Self::Messenger];

    pub fn id(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Instagram => "instagram",
            Self::Messenger => "messenger",
        }
    }
}

pub fn parse_friend_service_id(input: &str) -> Result<FriendServiceKind, String> {
    FriendServiceKind::ALL
        .into_iter()
        .find(|kind| kind.id() == input)
        .ok_or_else(|| format!("OSL: unknown friend service '{input}'"))
}

/// One friend's name on one service.
///
/// `service_id` is kept as the raw string (rather than [`FriendServiceKind`])
/// so a corrupted or hand-edited document can be loaded and reported on
/// instead of failing to deserialize outright; every write path still
/// requires it to parse via [`parse_friend_service_id`] first.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FriendServiceNameRecord {
    pub friend_id: String,
    pub service_id: String,
    pub service_name: String,
}

impl FriendServiceNameRecord {
    pub fn service(&self) -> Result<FriendServiceKind, String> {
        parse_friend_service_id(&self.service_id)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FriendServiceNameFileState {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    records: Vec<FriendServiceNameRecord>,
}

impl FriendServiceNameFileState {
    pub fn new() -> Self {
        Self {
            version: BINDING_VERSION,
            records: Vec::new(),
        }
    }

    pub fn records(&self) -> &[FriendServiceNameRecord] {
        &self.records
    }

    fn validate(&self) -> Result<(), String> {
        if self.records.len() > MAX_BINDINGS {
            return Err("OSL friend service name list is full".to_owned());
        }
        Ok(())
    }

    /// Bind (or rebind) one friend to the name they use on one service.
    /// Refuses a service id that is not one of [`FriendServiceKind::ALL`],
    /// naming it in the error.
    pub fn bind(
        &mut self,
        friend_id: &str,
        service_id: &str,
        service_name: &str,
    ) -> Result<FriendServiceNameRecord, String> {
        parse_friend_service_id(service_id)?;
        let friend_id = valid_friend_id(friend_id)?;
        let service_name = valid_service_name(service_name)?;

        let record = FriendServiceNameRecord {
            friend_id,
            service_id: service_id.to_owned(),
            service_name,
        };

        if let Some(existing) = self
            .records
            .iter_mut()
            .find(|candidate| same_binding(candidate, &record))
        {
            *existing = record.clone();
        } else {
            if self.records.len() >= MAX_BINDINGS {
                return Err("OSL friend service name list is full".to_owned());
            }
            self.records.push(record.clone());
        }
        Ok(record)
    }

    pub fn read(&self, friend_id: &str, service_id: &str) -> Option<&FriendServiceNameRecord> {
        self.records
            .iter()
            .find(|record| record.friend_id == friend_id && record.service_id == service_id)
    }
}

fn same_binding(a: &FriendServiceNameRecord, b: &FriendServiceNameRecord) -> bool {
    a.friend_id == b.friend_id && a.service_id == b.service_id
}

/// Friends among `records` whose `service_id` is not one of the three known
/// services. Empty on any document produced only through
/// [`FriendServiceNameFileState::bind`].
pub fn friends_with_unknown_service(records: &[FriendServiceNameRecord]) -> Vec<String> {
    records
        .iter()
        .filter(|record| record.service().is_err())
        .map(|record| record.friend_id.clone())
        .collect()
}

fn valid_friend_id(value: &str) -> Result<String, String> {
    if value.is_empty()
        || value.len() > 80
        || value.trim() != value
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("OSL friend id is invalid".to_owned());
    }
    Ok(value.to_owned())
}

fn valid_service_name(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > 64 || trimmed != value {
        return Err("OSL friend service name is invalid".to_owned());
    }
    Ok(trimmed.to_owned())
}

pub fn save_friend_service_name_file_state(
    dir: &Path,
    state: &FriendServiceNameFileState,
) -> Result<(), String> {
    state.validate()?;
    fs::create_dir_all(dir)
        .map_err(|_| "OSL friend service name storage is unavailable".to_owned())?;
    let body = serde_json::to_vec(state)
        .map_err(|_| "OSL friend service name list could not be encoded".to_owned())?;
    let sealed = crate::main_password::maybe_encrypt(&body)?;
    let path = dir.join(BINDING_STATE_FILE);
    crate::recoverable_file::write_recoverable(&path, &sealed)
        .map_err(|_| "OSL friend service name list could not be persisted".to_owned())
}

pub fn load_friend_service_name_file_state(
    dir: &Path,
) -> Result<FriendServiceNameFileState, String> {
    let path = dir.join(BINDING_STATE_FILE);
    if !path.exists() {
        return Ok(FriendServiceNameFileState::new());
    }
    let blob =
        fs::read(&path).map_err(|_| "OSL friend service name list could not be read".to_owned())?;
    let plain = crate::main_password::maybe_decrypt_in_dir(dir, &blob)?;
    let state: FriendServiceNameFileState = serde_json::from_slice(&plain)
        .map_err(|_| "OSL friend service name list is malformed".to_owned())?;
    state.validate()?;
    Ok(state)
}
