use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::core_bridge::HubCoreState;
use crate::models::ServiceKind;
use crate::services::ServiceRegistryState;

const REACH_VERSION: u32 = 1;
const MAX_REACH_BYTES: u64 = 256 * 1024;
const MAX_REACH_RECORDS: usize = 1_000;

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FriendAccountReachRecord {
    pub owner_osl_user_id: String,
    pub friend_person_id: String,
    pub service_id: ServiceKind,
    pub account_id: String,
    pub allowed: bool,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReachDocument {
    version: u32,
    #[serde(default)]
    records: Vec<FriendAccountReachRecord>,
}

#[derive(Default)]
struct ReachCache {
    loaded: bool,
    records: Vec<FriendAccountReachRecord>,
}

/// Encrypted local choices for which owned service accounts one accepted
/// friend may reach.
pub struct FriendAccountReachState {
    path: PathBuf,
    cache: Mutex<ReachCache>,
}

impl FriendAccountReachState {
    pub fn load(path: PathBuf) -> Self {
        Self {
            path,
            cache: Mutex::new(ReachCache::default()),
        }
    }

    pub fn set_choice(
        &self,
        core: &HubCoreState,
        registry: &ServiceRegistryState,
        owner_osl_user_id: &str,
        friend_person_id: &str,
        service_id: ServiceKind,
        account_id: &str,
        allowed: bool,
    ) -> Result<FriendAccountReachRecord, String> {
        crate::security::require_accepted_friend(core, friend_person_id)?;
        registry.require_owned(owner_osl_user_id, service_id, account_id)?;

        let record = FriendAccountReachRecord {
            owner_osl_user_id: owner_osl_user_id.to_owned(),
            friend_person_id: friend_person_id.to_owned(),
            service_id,
            account_id: account_id.to_owned(),
            allowed,
        };

        let mut cache = self.locked_cache()?;
        let mut records = cache.records.clone();
        if let Some(existing) = records
            .iter_mut()
            .find(|candidate| same_choice(candidate, &record))
        {
            *existing = record.clone();
        } else {
            if records.len() >= MAX_REACH_RECORDS {
                return Err("OSL friend account reach list is full".to_owned());
            }
            records.push(record.clone());
        }
        write_reach_document(&self.path, &records)?;
        cache.records = records;
        Ok(record)
    }

    pub fn record(
        &self,
        core: &HubCoreState,
        registry: &ServiceRegistryState,
        owner_osl_user_id: &str,
        friend_person_id: &str,
        service_id: ServiceKind,
        account_id: &str,
    ) -> Result<Option<FriendAccountReachRecord>, String> {
        crate::security::require_accepted_friend(core, friend_person_id)?;
        registry.require_owned(owner_osl_user_id, service_id, account_id)?;
        let cache = self.locked_cache()?;
        Ok(cache
            .records
            .iter()
            .find(|record| {
                record.owner_osl_user_id == owner_osl_user_id
                    && record.friend_person_id == friend_person_id
                    && record.service_id == service_id
                    && record.account_id == account_id
            })
            .cloned())
    }

    pub fn is_allowed(
        &self,
        core: &HubCoreState,
        registry: &ServiceRegistryState,
        owner_osl_user_id: &str,
        friend_person_id: &str,
        service_id: ServiceKind,
        account_id: &str,
    ) -> Result<bool, String> {
        Ok(self
            .record(
                core,
                registry,
                owner_osl_user_id,
                friend_person_id,
                service_id,
                account_id,
            )?
            .is_some_and(|record| record.allowed))
    }

    fn locked_cache(&self) -> Result<std::sync::MutexGuard<'_, ReachCache>, String> {
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| "OSL friend account reach is unavailable".to_owned())?;
        if !cache.loaded {
            cache.records = load_reach_document(&self.path)?;
            cache.loaded = true;
        }
        Ok(cache)
    }
}

fn same_choice(a: &FriendAccountReachRecord, b: &FriendAccountReachRecord) -> bool {
    a.owner_osl_user_id == b.owner_osl_user_id
        && a.friend_person_id == b.friend_person_id
        && a.service_id == b.service_id
        && a.account_id == b.account_id
}

fn load_reach_document(path: &Path) -> Result<Vec<FriendAccountReachRecord>, String> {
    let key = ipc::main_password::get_file_storage_key()
        .ok_or_else(|| "OSL main password must be unlocked".to_owned())?;
    let Some(bytes) = crate::atomic_file::read_recoverable_bounded(
        path,
        MAX_REACH_BYTES,
        "OSL friend account reach",
    )?
    else {
        return Ok(Vec::new());
    };
    if !ipc::main_password::has_enc_magic(&bytes) {
        return Err("OSL friend account reach is not encrypted".to_owned());
    }
    let plain = ipc::main_password::decrypt_at_rest(&bytes, &key)
        .map_err(|_| "OSL friend account reach could not be opened".to_owned())?;
    let document: ReachDocument = serde_json::from_slice(&plain)
        .map_err(|_| "OSL friend account reach is malformed".to_owned())?;
    if document.version != REACH_VERSION
        || document.records.len() > MAX_REACH_RECORDS
        || !document.records.iter().all(valid_record)
    {
        return Err("OSL friend account reach is malformed".to_owned());
    }
    Ok(document.records)
}

fn write_reach_document(path: &Path, records: &[FriendAccountReachRecord]) -> Result<(), String> {
    let key = ipc::main_password::get_file_storage_key()
        .ok_or_else(|| "OSL main password must be unlocked".to_owned())?;
    let body = serde_json::to_vec(&ReachDocument {
        version: REACH_VERSION,
        records: records.to_vec(),
    })
    .map_err(|_| "OSL friend account reach could not be encoded".to_owned())?;
    let sealed = ipc::main_password::encrypt_at_rest(&body, &key)
        .map_err(|_| "OSL friend account reach could not be encrypted".to_owned())?;
    if sealed.len() as u64 > MAX_REACH_BYTES {
        return Err("OSL friend account reach exceeds its storage limit".to_owned());
    }
    crate::atomic_file::write_recoverable(path, &sealed, "OSL friend account reach")
        .map_err(|_| "OSL friend account reach could not be persisted".to_owned())
}

fn valid_record(record: &FriendAccountReachRecord) -> bool {
    valid_owner(&record.owner_osl_user_id)
        && valid_friend(&record.friend_person_id)
        && valid_account(&record.account_id)
}

fn valid_owner(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.trim() == value
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn valid_friend(value: &str) -> bool {
    value.starts_with("hub-person-")
        && value.len() <= 80
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn valid_account(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && (bytes[bytes.len() - 1].is_ascii_lowercase() || bytes[bytes.len() - 1].is_ascii_digit())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}
