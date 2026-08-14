use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const SERVER_RECORDS_VERSION: u8 = 1;
const MAX_SERVER_RECORDS_BYTES: u64 = 256 * 1024;
const MAX_SERVERS_PER_OWNER: usize = 128;
const MAX_SERVER_NAME_BYTES: usize = 80;
const MAX_SERVER_NAME_CHARS: usize = 48;
const MAX_MEMBERS_PER_SERVER: usize = 512;

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedServerRecord {
    pub server_id: String,
    pub name: String,
    pub owner_osl_user_id: String,
    pub member_osl_user_ids: Vec<String>,
    pub chosen_for_launch: bool,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ServerRecordsDocument {
    version: u8,
    servers: Vec<NamedServerRecord>,
}

#[derive(Default)]
struct ServerRecordsCache {
    loaded: bool,
    servers: Vec<NamedServerRecord>,
}

pub struct NamedServerRegistryState {
    path: PathBuf,
    cache: Mutex<ServerRecordsCache>,
    next_id: AtomicU64,
}

impl NamedServerRegistryState {
    pub fn load(path: PathBuf) -> Self {
        Self {
            path,
            cache: Mutex::new(ServerRecordsCache::default()),
            next_id: AtomicU64::new(1),
        }
    }

    fn locked_cache(&self) -> Result<std::sync::MutexGuard<'_, ServerRecordsCache>, String> {
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| "server registry is unavailable".to_owned())?;
        if !cache.loaded {
            cache.servers = load_server_records(&self.path)?;
            cache.loaded = true;
        }
        Ok(cache)
    }

    pub fn list_for_owner(
        &self,
        owner_osl_user_id: &str,
    ) -> Result<Vec<NamedServerRecord>, String> {
        validate_osl_user_id(owner_osl_user_id, "server owner is invalid")?;
        let cache = self.locked_cache()?;
        Ok(cache
            .servers
            .iter()
            .filter(|server| server.owner_osl_user_id == owner_osl_user_id)
            .cloned()
            .collect())
    }

    pub fn create_launch_server_for_owner(
        &self,
        owner_osl_user_id: &str,
        name: String,
        member_osl_user_ids: Vec<String>,
    ) -> Result<NamedServerRecord, String> {
        validate_osl_user_id(owner_osl_user_id, "server owner is invalid")?;
        let name = normalize_server_name(&name)?;
        let members = normalize_members(owner_osl_user_id, member_osl_user_ids)?;

        let mut cache = self.locked_cache()?;
        if let Some(existing) = cache.servers.iter().find(|server| {
            server.owner_osl_user_id == owner_osl_user_id && server.name.eq_ignore_ascii_case(&name)
        }) {
            return Ok(existing.clone());
        }
        if cache
            .servers
            .iter()
            .filter(|server| server.owner_osl_user_id == owner_osl_user_id)
            .count()
            >= MAX_SERVERS_PER_OWNER
        {
            return Err("server registry owner limit reached".to_owned());
        }

        let record = NamedServerRecord {
            server_id: new_server_id(owner_osl_user_id, &name, &self.next_id),
            name,
            owner_osl_user_id: owner_osl_user_id.to_owned(),
            member_osl_user_ids: members,
            chosen_for_launch: true,
        };
        let mut updated = cache.servers.clone();
        updated.push(record.clone());
        write_server_records(&self.path, &updated)?;
        cache.servers = updated;
        Ok(record)
    }
}

fn normalize_server_name(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.len() > MAX_SERVER_NAME_BYTES
        || trimmed.chars().count() > MAX_SERVER_NAME_CHARS
        || trimmed.chars().any(|character| character.is_control())
    {
        return Err("server name must be 1-48 printable characters".to_owned());
    }
    Ok(trimmed.to_owned())
}

fn normalize_members(
    owner_osl_user_id: &str,
    member_osl_user_ids: Vec<String>,
) -> Result<Vec<String>, String> {
    if member_osl_user_ids.is_empty() || member_osl_user_ids.len() > MAX_MEMBERS_PER_SERVER {
        return Err("server must have at least one member".to_owned());
    }
    let mut seen = BTreeSet::new();
    let mut members = Vec::with_capacity(member_osl_user_ids.len());
    for member in member_osl_user_ids {
        validate_osl_user_id(&member, "server member is invalid")?;
        if member == owner_osl_user_id {
            return Err("server owner must not also be listed as a member".to_owned());
        }
        if !seen.insert(member.clone()) {
            return Err("server members must be unique".to_owned());
        }
        members.push(member);
    }
    Ok(members)
}

fn validate_osl_user_id(value: &str, message: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 128
        || value.trim() != value
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(message.to_owned());
    }
    Ok(())
}

fn new_server_id(owner_osl_user_id: &str, name: &str, counter: &AtomicU64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = counter.fetch_add(1, Ordering::Relaxed);
    let mut hash = Sha256::new();
    hash.update(owner_osl_user_id.as_bytes());
    hash.update([0]);
    hash.update(name.as_bytes());
    hash.update([0]);
    hash.update(now.to_le_bytes());
    hash.update(sequence.to_le_bytes());
    let digest = hash.finalize();
    let suffix: String = digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("srv-{now:x}-{sequence:x}-{suffix}")
}

fn load_server_records(path: &Path) -> Result<Vec<NamedServerRecord>, String> {
    let key = ipc::main_password::get_file_storage_key()
        .ok_or_else(|| "Unlock an OSL identity before accessing server records".to_owned())?;
    let backup = path.with_extension("bak");
    let primary = read_bounded(path)?;
    let fallback = read_bounded(&backup)?;

    for (bytes, recover) in [(primary.as_deref(), false), (fallback.as_deref(), true)] {
        let Some(bytes) = bytes else { continue };
        if !ipc::main_password::has_enc_magic(bytes) {
            continue;
        }
        if let Ok(document) = decode_server_records(bytes, &key) {
            if recover {
                fs::copy(&backup, path)
                    .map_err(|_| "server registry backup could not be recovered".to_owned())?;
            }
            if document.version != SERVER_RECORDS_VERSION {
                return Err("server registry version is unsupported".to_owned());
            }
            return Ok(sanitize_server_records(document.servers));
        }
    }

    let encrypted_present = primary
        .as_deref()
        .is_some_and(ipc::main_password::has_enc_magic)
        || fallback
            .as_deref()
            .is_some_and(ipc::main_password::has_enc_magic);
    if encrypted_present {
        return Err("server registry authentication failed".to_owned());
    }
    Ok(Vec::new())
}

fn sanitize_server_records(servers: Vec<NamedServerRecord>) -> Vec<NamedServerRecord> {
    let mut clean = Vec::new();
    for mut server in servers {
        if clean.len() >= MAX_SERVERS_PER_OWNER * 8
            || !valid_server_id(&server.server_id)
            || normalize_server_name(&server.name).is_err()
            || validate_osl_user_id(&server.owner_osl_user_id, "server owner is invalid").is_err()
            || normalize_members(
                &server.owner_osl_user_id,
                server.member_osl_user_ids.clone(),
            )
            .is_err()
            || clean.iter().any(|existing: &NamedServerRecord| {
                existing.server_id == server.server_id
                    || (existing.owner_osl_user_id == server.owner_osl_user_id
                        && existing.name.eq_ignore_ascii_case(&server.name))
            })
            || clean
                .iter()
                .filter(|existing| existing.owner_osl_user_id == server.owner_osl_user_id)
                .count()
                >= MAX_SERVERS_PER_OWNER
        {
            continue;
        }
        server.name = normalize_server_name(&server.name).unwrap();
        server.member_osl_user_ids =
            normalize_members(&server.owner_osl_user_id, server.member_osl_user_ids).unwrap();
        server.chosen_for_launch = true;
        clean.push(server);
    }
    clean
}

fn valid_server_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    value.starts_with("srv-")
        && bytes.len() <= 96
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

fn read_bounded(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() && metadata.len() <= MAX_SERVER_RECORDS_BYTES => {
            fs::read(path)
                .map(Some)
                .map_err(|_| "server registry could not be read".to_owned())
        }
        Ok(_) => Err("server registry is not a bounded regular file".to_owned()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("server registry metadata could not be read".to_owned()),
    }
}

fn decode_server_records(bytes: &[u8], key: &[u8; 32]) -> Result<ServerRecordsDocument, String> {
    let plain = ipc::main_password::decrypt_at_rest(bytes, key)
        .map_err(|_| "server registry decrypt failed".to_owned())?;
    serde_json::from_slice(&plain).map_err(|_| "server registry is malformed".to_owned())
}

fn write_server_records(path: &Path, servers: &[NamedServerRecord]) -> Result<(), String> {
    let key = ipc::main_password::get_file_storage_key()
        .ok_or_else(|| "Unlock an OSL identity before accessing server records".to_owned())?;
    let bytes = serde_json::to_vec(&ServerRecordsDocument {
        version: SERVER_RECORDS_VERSION,
        servers: servers.to_vec(),
    })
    .map_err(|_| "server registry could not be encoded".to_owned())?;
    if bytes.len() as u64 > MAX_SERVER_RECORDS_BYTES {
        return Err("server registry exceeds limit".to_owned());
    }
    let sealed = ipc::main_password::encrypt_at_rest(&bytes, &key)
        .map_err(|_| "server registry could not be encrypted".to_owned())?;
    crate::atomic_file::write_recoverable(path, &sealed, "server registry")
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNER: &str = "owner-1317";
    const MEMBER_A: &str = "member-1317-a";
    const MEMBER_B: &str = "member-1317-b";
    const TEST_KEY: [u8; 32] = [0x31; 32];

    fn temporary_registry() -> PathBuf {
        ipc::main_password::set_file_storage_key(Some(TEST_KEY));
        std::env::temp_dir().join(format!(
            "osl-hub-server-records-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn task1317_direct_command_creates_named_server_with_one_owner_and_two_members() {
        let _serial = crate::global_keystore_test_lock();
        let path = temporary_registry();
        let state = NamedServerRegistryState::load(path.clone());

        let server = state
            .create_launch_server_for_owner(
                OWNER,
                "Launch Server Alpha".to_owned(),
                vec![MEMBER_A.to_owned(), MEMBER_B.to_owned()],
            )
            .expect("direct command creates launch server record");
        let listed = state
            .list_for_owner(OWNER)
            .expect("direct command lists owner servers");
        println!(
            "TASK1317 direct command created_server={} name=\"{}\" owner={} owner_count={} member_count={} members={}",
            server.server_id,
            server.name,
            server.owner_osl_user_id,
            usize::from(server.owner_osl_user_id == OWNER),
            server.member_osl_user_ids.len(),
            server.member_osl_user_ids.join(",")
        );

        assert_eq!(listed, vec![server.clone()]);
        assert_eq!(server.name, "Launch Server Alpha");
        assert_eq!(server.owner_osl_user_id, OWNER);
        assert_eq!(usize::from(server.owner_osl_user_id == OWNER), 1);
        assert_eq!(server.member_osl_user_ids, [MEMBER_A, MEMBER_B]);
        assert!(server.chosen_for_launch);
        let _ = fs::remove_file(path);
    }
}
