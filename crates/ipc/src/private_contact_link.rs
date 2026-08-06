//! One-use private contact links.
//!
//! These links are local, opaque bearer capabilities for adding one known
//! person without publishing or resolving a stable username. The file stores
//! only a SHA-256 digest of the bearer token, so reading the state file does
//! not yield a usable link.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

pub const PRIVATE_CONTACT_LINKS_FILE: &str = "private_contact_links.json";

const SCHEMA_VERSION: u32 = 1;
const LINK_PREFIX: &str = "OSLCL1.";
const TOKEN_BYTES: usize = 32;
const TOKEN_B64_LEN: usize = 43;
const ONE_USE_LIMIT: u32 = 1;
const MAX_PERSON_ID_BYTES: usize = 180;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivateContactLinkDto {
    pub person_id: String,
    pub link_value: String,
    pub uses_allowed: u32,
    pub uses_recorded: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivateContactLinkStatusDto {
    pub person_id: String,
    pub uses_allowed: u32,
    pub uses_recorded: u32,
    pub used: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivateContactLinkUseDto {
    pub person_id: String,
    pub uses_allowed: u32,
    pub uses_recorded: u32,
    pub accepted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PrivateContactLinkFile {
    schema_version: u32,
    links: Vec<StoredPrivateContactLink>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredPrivateContactLink {
    person_id: String,
    token_sha256_hex: String,
    created_at_ms: u64,
    uses_allowed: u32,
    uses_recorded: u32,
}

impl Default for PrivateContactLinkFile {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            links: Vec::new(),
        }
    }
}

pub fn create_private_contact_link(
    dir: &Path,
    person_id: &str,
    created_at_ms: u64,
) -> Result<PrivateContactLinkDto, String> {
    let person_id = validate_person_id(person_id)?;
    let token = URL_SAFE_NO_PAD.encode(crypto::random::random_bytes(TOKEN_BYTES));
    let link_value = format!("{LINK_PREFIX}{token}");
    let token_sha256_hex = token_digest_hex(&link_value)?;

    let mut file = load_file(dir)?;
    if file
        .links
        .iter()
        .any(|link| link.token_sha256_hex == token_sha256_hex)
    {
        return Err("OSL: private contact link collision".to_owned());
    }
    file.links.push(StoredPrivateContactLink {
        person_id: person_id.clone(),
        token_sha256_hex,
        created_at_ms,
        uses_allowed: ONE_USE_LIMIT,
        uses_recorded: 0,
    });
    save_file(dir, &file)?;

    Ok(PrivateContactLinkDto {
        person_id,
        link_value,
        uses_allowed: ONE_USE_LIMIT,
        uses_recorded: 0,
    })
}

pub fn private_contact_link_status(
    dir: &Path,
    link_value: &str,
) -> Result<PrivateContactLinkStatusDto, String> {
    let digest = token_digest_hex(link_value)?;
    let file = load_file(dir)?;
    let link = find_link(&file, &digest)?;
    Ok(link.status())
}

pub fn use_private_contact_link(
    dir: &Path,
    link_value: &str,
) -> Result<PrivateContactLinkUseDto, String> {
    let digest = token_digest_hex(link_value)?;
    let mut file = load_file(dir)?;
    let link = file
        .links
        .iter_mut()
        .find(|link| link.token_sha256_hex == digest)
        .ok_or_else(|| "OSL: private contact link is unavailable".to_owned())?;
    if link.uses_recorded >= link.uses_allowed {
        return Err("OSL: private contact link is already used".to_owned());
    }
    link.uses_recorded += 1;
    let used = PrivateContactLinkUseDto {
        person_id: link.person_id.clone(),
        uses_allowed: link.uses_allowed,
        uses_recorded: link.uses_recorded,
        accepted: true,
    };
    save_file(dir, &file)?;
    Ok(used)
}

impl StoredPrivateContactLink {
    fn status(&self) -> PrivateContactLinkStatusDto {
        PrivateContactLinkStatusDto {
            person_id: self.person_id.clone(),
            uses_allowed: self.uses_allowed,
            uses_recorded: self.uses_recorded,
            used: self.uses_recorded >= self.uses_allowed,
        }
    }
}

fn validate_person_id(person_id: &str) -> Result<String, String> {
    let person_id = person_id.trim();
    if person_id.is_empty()
        || person_id.len() > MAX_PERSON_ID_BYTES
        || person_id.chars().any(|character| character.is_control())
    {
        return Err("OSL: private contact link person is invalid".to_owned());
    }
    Ok(person_id.to_owned())
}

fn token_digest_hex(link_value: &str) -> Result<String, String> {
    let link_value = link_value.trim();
    let token = link_value
        .strip_prefix(LINK_PREFIX)
        .ok_or_else(|| "OSL: private contact link is invalid".to_owned())?;
    if token.len() != TOKEN_B64_LEN
        || !token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err("OSL: private contact link is invalid".to_owned());
    }
    let mut hash = Sha256::new();
    hash.update(link_value.as_bytes());
    Ok(hex_lower(&hash.finalize()))
}

fn find_link<'a>(
    file: &'a PrivateContactLinkFile,
    digest: &str,
) -> Result<&'a StoredPrivateContactLink, String> {
    file.links
        .iter()
        .find(|link| link.token_sha256_hex == digest)
        .ok_or_else(|| "OSL: private contact link is unavailable".to_owned())
}

fn validate_file(file: &PrivateContactLinkFile) -> Result<(), String> {
    if file.schema_version != SCHEMA_VERSION {
        return Err("OSL: private contact link storage is unreadable".to_owned());
    }
    for link in &file.links {
        validate_person_id(&link.person_id)?;
        if link.token_sha256_hex.len() != 64
            || !link
                .token_sha256_hex
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || link.uses_allowed != ONE_USE_LIMIT
            || link.uses_recorded > link.uses_allowed
        {
            return Err("OSL: private contact link storage is unreadable".to_owned());
        }
    }
    Ok(())
}

fn load_file(dir: &Path) -> Result<PrivateContactLinkFile, String> {
    let path = dir.join(PRIVATE_CONTACT_LINKS_FILE);
    if !path.exists() {
        return Ok(PrivateContactLinkFile::default());
    }
    let blob = fs::read(&path)
        .map_err(|_| "OSL: private contact link storage is unavailable".to_owned())?;
    let plain = crate::main_password::maybe_decrypt_in_dir(dir, &blob)
        .map_err(|_| "OSL: private contact link storage is unavailable".to_owned())?;
    let file: PrivateContactLinkFile = serde_json::from_slice(&plain)
        .map_err(|_| "OSL: private contact link storage is unreadable".to_owned())?;
    validate_file(&file)?;
    Ok(file)
}

fn save_file(dir: &Path, file: &PrivateContactLinkFile) -> Result<(), String> {
    validate_file(file)?;
    fs::create_dir_all(dir)
        .map_err(|_| "OSL: private contact link storage is unavailable".to_owned())?;
    let body = serde_json::to_vec(file)
        .map_err(|_| "OSL: private contact link storage is unavailable".to_owned())?;
    let sealed = crate::main_password::maybe_encrypt(&body)
        .map_err(|_| "OSL: private contact link storage is unavailable".to_owned())?;
    let path = dir.join(PRIVATE_CONTACT_LINKS_FILE);
    let tmp = dir.join(format!("{PRIVATE_CONTACT_LINKS_FILE}.tmp"));
    fs::write(&tmp, sealed)
        .map_err(|_| "OSL: private contact link storage is unavailable".to_owned())?;
    fs::rename(&tmp, &path)
        .map_err(|_| "OSL: private contact link storage is unavailable".to_owned())
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}
