//! Encrypted, identity-scoped local OSL profile metadata.
//!
//! The username is stored locally after the separately authenticated directory
//! claim succeeds. This module enforces the exact directory normalization.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use url::Url;
use zeroize::Zeroize;

const PROFILE_FILE: &str = "hub_profile.json";
const PROFILE_PICTURE_FILE: &str = "owner_profile_picture.json";
const PROFILE_VERSION: u32 = 1;
const PROFILE_PICTURE_VERSION: u32 = 1;
const MAX_DISPLAY_NAME_CHARS: usize = 64;
const MAX_DISPLAY_NAME_BYTES: usize = 192;
const MAX_ABOUT_LINE_CHARS: usize = 120;
const MAX_ABOUT_LINE_BYTES: usize = 384;
const MIN_USERNAME_CHARS: usize = 3;
const MAX_USERNAME_CHARS: usize = 30;
const MAX_STATUS_CHARS: usize = 160;
const MAX_STATUS_BYTES: usize = 512;
const MAX_HTTPS_AVATAR_BYTES: usize = 2_048;
const MAX_AVATAR_DECODED_BYTES: usize = 2 * 1024 * 1024;
const MAX_AVATAR_DATA_URL_BYTES: usize = 2_800_000;
const MAX_PROFILE_PLAINTEXT_BYTES: usize = MAX_AVATAR_DATA_URL_BYTES + 8 * 1024;
const MAX_PROFILE_SEALED_BYTES: u64 = (MAX_PROFILE_PLAINTEXT_BYTES + 4 * 1024) as u64;
const MAX_OWNER_BYTES: usize = 160;
const MAX_PROFILE_SCOPE_ID_CHARS: usize = 128;
const MAX_PROFILE_SCOPE_ID_BYTES: usize = 384;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileFrame {
    None,
    Thin,
    Double,
    Glow,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileEffect {
    None,
    Gradient,
    Pulse,
    Shimmer,
}

#[derive(Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HubProfileInput {
    pub display_name: String,
    pub username_candidate: String,
    pub avatar: Option<String>,
    pub accent_color: String,
    pub banner_color: String,
    pub frame: ProfileFrame,
    pub effect: ProfileEffect,
    pub status: String,
}

impl std::fmt::Debug for HubProfileDto {
    /// Never derive Debug here. display_name, username_candidate and avatar are
    /// account identifiers, and username_candidate is the normalized value the
    /// authenticated username directory keys on. Presentation fields are safe to
    /// show; the identifying ones are not.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HubProfileDto")
            .field("display_name", &"[REDACTED]")
            .field("username_candidate", &"[REDACTED]")
            .field("avatar", &self.avatar.as_ref().map(|_| "[REDACTED]"))
            .field("accent_color", &self.accent_color)
            .field("banner_color", &self.banner_color)
            .field("frame", &self.frame)
            .field("effect", &self.effect)
            .field("status", &self.status)
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HubProfileDto {
    pub display_name: String,
    /// The normalized value used by the authenticated username directory.
    pub username_candidate: String,
    pub avatar: Option<String>,
    pub accent_color: String,
    pub banner_color: String,
    pub frame: ProfileFrame,
    pub effect: ProfileEffect,
    pub status: String,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OslProfileScope {
    Global,
    OslChats,
    Enclave { enclave_id: String },
}

impl OslProfileScope {
    pub fn storage_key(&self) -> String {
        match self {
            Self::Global => "global".to_owned(),
            Self::OslChats => "osl-chats".to_owned(),
            Self::Enclave { enclave_id } => format!("enclave:{enclave_id}"),
        }
    }

    fn label_prefix(&self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::OslChats => "chats",
            Self::Enclave { .. } => "enclave",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScopedProfileRecord {
    pub scope: OslProfileScope,
    pub use_separate_profile_here: bool,
    pub display_name: String,
    pub about_line: String,
    pub status: String,
    pub card_background: String,
    pub avatar: Option<String>,
    pub colour: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopedProfileFields {
    pub display_name: String,
    pub about_line: String,
    pub status: String,
    pub card_background: String,
    pub avatar: Option<String>,
    pub colour: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedScopedProfile {
    pub scope: OslProfileScope,
    pub source_scope: OslProfileScope,
    pub use_separate_profile_here: bool,
    pub profile: ScopedProfileFields,
}

pub fn resolve_scoped_profile_records(
    records: &[ScopedProfileRecord],
) -> Result<Vec<ResolvedScopedProfile>, String> {
    let mut seen = BTreeSet::new();
    let mut global = None;
    for record in records {
        validate_profile_scope(&record.scope)?;
        let storage_key = record.scope.storage_key();
        if !seen.insert(storage_key.clone()) {
            return Err(format!(
                "OSL profile scope record is duplicated: {storage_key}"
            ));
        }
        if record.scope == OslProfileScope::Global {
            if global.replace(record).is_some() {
                return Err("OSL profile has more than one global record".to_owned());
            }
        }
    }

    let global = global.ok_or_else(|| "OSL global profile record is missing".to_owned())?;
    let global_profile = validate_scoped_profile_fields(global, "global")?;
    let mut sorted: Vec<&ScopedProfileRecord> = records.iter().collect();
    sorted.sort_by_key(|record| profile_scope_sort_key(&record.scope));

    sorted
        .into_iter()
        .map(|record| {
            let use_override =
                record.scope != OslProfileScope::Global && record.use_separate_profile_here;
            let profile = if use_override {
                validate_scoped_profile_fields(record, record.scope.label_prefix())?
            } else {
                global_profile.clone()
            };
            let source_scope = if use_override {
                record.scope.clone()
            } else {
                OslProfileScope::Global
            };
            Ok(ResolvedScopedProfile {
                scope: record.scope.clone(),
                source_scope,
                use_separate_profile_here: use_override,
                profile,
            })
        })
        .collect()
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProfileDocument {
    version: u32,
    owner: String,
    profile: StoredProfile,
}

#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerProfilePictureDto {
    pub status: String,
    pub image: Option<String>,
}

impl OwnerProfilePictureDto {
    pub fn status(&self) -> &str {
        &self.status
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProfilePictureDocument {
    version: u32,
    owner: String,
    image: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredProfile {
    display_name: String,
    username_candidate: String,
    avatar: Option<String>,
    accent_color: String,
    banner_color: String,
    frame: ProfileFrame,
    effect: ProfileEffect,
    status: String,
}

impl From<HubProfileDto> for StoredProfile {
    fn from(profile: HubProfileDto) -> Self {
        Self {
            display_name: profile.display_name,
            username_candidate: profile.username_candidate,
            avatar: profile.avatar,
            accent_color: profile.accent_color,
            banner_color: profile.banner_color,
            frame: profile.frame,
            effect: profile.effect,
            status: profile.status,
        }
    }
}

impl From<StoredProfile> for HubProfileInput {
    fn from(profile: StoredProfile) -> Self {
        Self {
            display_name: profile.display_name,
            username_candidate: profile.username_candidate,
            avatar: profile.avatar,
            accent_color: profile.accent_color,
            banner_color: profile.banner_color,
            frame: profile.frame,
            effect: profile.effect,
            status: profile.status,
        }
    }
}

pub fn get_active_profile(owner: &str) -> Result<Option<HubProfileDto>, String> {
    let key = active_file_key()?;
    load_profile_with_key(&active_profile_path()?, owner, &key)
}

pub fn save_active_profile(owner: &str, input: HubProfileInput) -> Result<HubProfileDto, String> {
    let profile = validate_profile(input)?;
    let key = active_file_key()?;
    write_profile_with_key(&active_profile_path()?, owner, profile, &key)
}

pub fn set_active_profile_picture(
    owner: &str,
    image: String,
) -> Result<OwnerProfilePictureDto, String> {
    let key = active_file_key()?;
    set_profile_picture_with_key(&active_profile_picture_path()?, owner, image, &key)
}

pub fn read_active_profile_picture(owner: &str) -> Result<OwnerProfilePictureDto, String> {
    let key = active_file_key()?;
    read_profile_picture_with_key(&active_profile_picture_path()?, owner, &key)
}

pub fn read_active_profile_picture_for_reader(
    owner: &str,
    reader_id: &str,
    accepted_friend_ids: &[String],
) -> Result<OwnerProfilePictureDto, String> {
    let key = active_file_key()?;
    read_profile_picture_for_reader_with_key(
        &active_profile_picture_path()?,
        owner,
        reader_id,
        accepted_friend_ids,
        &key,
    )
}

pub fn clear_active_profile_picture(owner: &str) -> Result<OwnerProfilePictureDto, String> {
    clear_profile_picture_at_path(&active_profile_picture_path()?, owner)
}

/// Restores the exact logical profile state after a later username-directory
/// step fails. This remains identity-scoped and encrypted through the same
/// storage boundary as an ordinary save.
pub fn restore_active_profile(owner: &str, profile: Option<HubProfileDto>) -> Result<(), String> {
    let key = active_file_key()?;
    let path = active_profile_path()?;
    restore_profile_with_key(&path, owner, profile, &key)
}

fn restore_profile_with_key(
    path: &Path,
    owner: &str,
    profile: Option<HubProfileDto>,
    key: &[u8; 32],
) -> Result<(), String> {
    match profile {
        Some(profile) => write_profile_with_key(path, owner, profile, key).map(|_| ()),
        None => match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("OSL profile rollback could not remove the new profile".to_owned()),
        },
    }
}

fn active_file_key() -> Result<[u8; 32], String> {
    ipc::main_password::get_file_storage_key()
        .ok_or_else(|| "OSL main password must be unlocked".to_owned())
}

fn active_profile_path() -> Result<PathBuf, String> {
    keystore::active_account_dir()
        .map(|directory| directory.join(PROFILE_FILE))
        .ok_or_else(|| "OSL active identity storage is unavailable".to_owned())
}

fn active_profile_picture_path() -> Result<PathBuf, String> {
    keystore::active_account_dir()
        .map(|directory| directory.join(PROFILE_PICTURE_FILE))
        .ok_or_else(|| "OSL active identity storage is unavailable".to_owned())
}

fn validate_profile(input: HubProfileInput) -> Result<HubProfileDto, String> {
    Ok(HubProfileDto {
        display_name: bounded_trimmed_text(
            input.display_name,
            "OSL profile display name",
            MAX_DISPLAY_NAME_CHARS,
            MAX_DISPLAY_NAME_BYTES,
            false,
        )?,
        username_candidate: normalize_username_candidate(&input.username_candidate)?,
        avatar: input.avatar.map(validate_avatar).transpose()?,
        accent_color: normalize_color(&input.accent_color, "accent color")?,
        banner_color: normalize_color(&input.banner_color, "banner color")?,
        frame: input.frame,
        effect: input.effect,
        status: bounded_trimmed_text(
            input.status,
            "OSL profile status",
            MAX_STATUS_CHARS,
            MAX_STATUS_BYTES,
            true,
        )?,
    })
}

fn validate_scoped_profile_fields(
    record: &ScopedProfileRecord,
    label_prefix: &str,
) -> Result<ScopedProfileFields, String> {
    Ok(ScopedProfileFields {
        display_name: bounded_trimmed_text(
            record.display_name.clone(),
            &format!("{label_prefix} display name"),
            MAX_DISPLAY_NAME_CHARS,
            MAX_DISPLAY_NAME_BYTES,
            false,
        )?,
        about_line: bounded_trimmed_text(
            record.about_line.clone(),
            &format!("{label_prefix} about line"),
            MAX_ABOUT_LINE_CHARS,
            MAX_ABOUT_LINE_BYTES,
            true,
        )?,
        status: bounded_trimmed_text(
            record.status.clone(),
            &format!("{label_prefix} status"),
            MAX_STATUS_CHARS,
            MAX_STATUS_BYTES,
            true,
        )?,
        card_background: normalize_color(
            &record.card_background,
            &format!("{label_prefix} card background"),
        )?,
        avatar: record.avatar.clone().map(validate_avatar).transpose()?,
        colour: normalize_color(&record.colour, &format!("{label_prefix} colour"))?,
    })
}

fn validate_profile_scope(scope: &OslProfileScope) -> Result<(), String> {
    match scope {
        OslProfileScope::Global | OslProfileScope::OslChats => Ok(()),
        OslProfileScope::Enclave { enclave_id } => {
            let trimmed = bounded_trimmed_text(
                enclave_id.clone(),
                "OSL enclave profile scope id",
                MAX_PROFILE_SCOPE_ID_CHARS,
                MAX_PROFILE_SCOPE_ID_BYTES,
                false,
            )?;
            if trimmed != *enclave_id || trimmed.chars().any(char::is_whitespace) {
                return Err("OSL enclave profile scope id is invalid".to_owned());
            }
            Ok(())
        }
    }
}

fn profile_scope_sort_key(scope: &OslProfileScope) -> (u8, String) {
    match scope {
        OslProfileScope::Global => (0, String::new()),
        OslProfileScope::OslChats => (1, String::new()),
        OslProfileScope::Enclave { enclave_id } => (2, enclave_id.clone()),
    }
}

pub fn normalize_username_candidate(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    let candidate = trimmed.strip_prefix('@').unwrap_or(trimmed);
    if candidate.len() < MIN_USERNAME_CHARS || candidate.len() > MAX_USERNAME_CHARS {
        return Err(format!(
            "OSL username candidates must be {MIN_USERNAME_CHARS} to {MAX_USERNAME_CHARS} characters"
        ));
    }
    if !candidate.is_ascii() {
        return Err("OSL usernames use only lowercase ASCII letters, numbers, and '_'".to_owned());
    }
    let normalized = candidate.to_ascii_lowercase();
    let bytes = normalized.as_bytes();
    if !bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        || !bytes.last().is_some_and(u8::is_ascii_alphanumeric)
    {
        return Err(
            "OSL username candidates must start and end with a letter or number".to_owned(),
        );
    }
    for byte in bytes {
        if !byte.is_ascii_alphanumeric() && *byte != b'_' {
            return Err("OSL usernames use only lowercase letters, numbers, and '_'".to_owned());
        }
    }
    Ok(normalized)
}

fn bounded_trimmed_text(
    input: String,
    label: &str,
    max_chars: usize,
    max_bytes: usize,
    allow_empty: bool,
) -> Result<String, String> {
    let trimmed = input.trim();
    if (!allow_empty && trimmed.is_empty())
        || trimmed.len() > max_bytes
        || trimmed.chars().count() > max_chars
        || trimmed.chars().any(char::is_control)
    {
        return Err(format!("{label} is empty, malformed, or too long"));
    }
    Ok(trimmed.to_owned())
}

fn normalize_color(input: &str, label: &str) -> Result<String, String> {
    if input.len() != 7
        || !input.starts_with('#')
        || !input.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
    {
        return Err(format!("OSL profile {label} must be a six-digit hex color"));
    }
    Ok(input.to_ascii_lowercase())
}

fn validate_avatar(input: String) -> Result<String, String> {
    if input.starts_with("data:") {
        return validate_data_avatar(input);
    }
    if input.len() > MAX_HTTPS_AVATAR_BYTES || input.chars().any(char::is_control) {
        return Err("OSL profile avatar URL is malformed or too long".to_owned());
    }
    let parsed = Url::parse(&input).map_err(|_| "OSL profile avatar URL is invalid".to_owned())?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.fragment().is_some()
    {
        return Err(
            "OSL profile avatars require an HTTPS URL without credentials or fragments".to_owned(),
        );
    }
    Ok(parsed.to_string())
}

fn validate_data_avatar(input: String) -> Result<String, String> {
    if input.len() > MAX_AVATAR_DATA_URL_BYTES || input.chars().any(char::is_whitespace) {
        return Err("OSL profile avatar data is malformed or too large".to_owned());
    }
    let (header, body) = input
        .split_once(',')
        .ok_or_else(|| "OSL profile avatar data URL is malformed".to_owned())?;
    let mime = header
        .strip_prefix("data:")
        .and_then(|value| value.strip_suffix(";base64"))
        .ok_or_else(|| "OSL profile avatar must use a base64 image data URL".to_owned())?;
    if !matches!(
        mime,
        "image/png" | "image/jpeg" | "image/webp" | "image/gif"
    ) {
        return Err("OSL profile avatar image type is unsupported".to_owned());
    }
    let decoded = STANDARD
        .decode(body)
        .map_err(|_| "OSL profile avatar base64 is invalid".to_owned())?;
    if decoded.is_empty()
        || decoded.len() > MAX_AVATAR_DECODED_BYTES
        || !matches_mime(mime, &decoded)
    {
        return Err("OSL profile avatar bytes do not match the declared image type".to_owned());
    }
    Ok(input)
}

fn present_picture(image: String) -> OwnerProfilePictureDto {
    OwnerProfilePictureDto {
        status: "image-present".to_owned(),
        image: Some(image),
    }
}

fn absent_picture() -> OwnerProfilePictureDto {
    OwnerProfilePictureDto {
        status: "image-absent".to_owned(),
        image: None,
    }
}

fn matches_mime(mime: &str, bytes: &[u8]) -> bool {
    match mime {
        "image/png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "image/jpeg" => bytes.starts_with(&[0xff, 0xd8, 0xff]),
        "image/gif" => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
        "image/webp" => bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP",
        _ => false,
    }
}

fn validate_owner(owner: &str) -> Result<(), String> {
    if owner.is_empty()
        || owner.len() > MAX_OWNER_BYTES
        || owner
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err("OSL profile owner identity is invalid".to_owned());
    }
    Ok(())
}

fn load_profile_with_key(
    path: &Path,
    owner: &str,
    key: &[u8; 32],
) -> Result<Option<HubProfileDto>, String> {
    validate_owner(owner)?;
    let Some(sealed) = crate::atomic_file::read_recoverable_bounded(
        path,
        MAX_PROFILE_SEALED_BYTES,
        "OSL profile",
    )?
    else {
        return Ok(None);
    };
    if !ipc::main_password::has_enc_magic(&sealed) {
        return Err("OSL profile is not encrypted".to_owned());
    }
    let mut plaintext = ipc::main_password::decrypt_at_rest(&sealed, key)
        .map_err(|_| "OSL profile could not be decrypted".to_owned())?;
    if plaintext.len() > MAX_PROFILE_PLAINTEXT_BYTES {
        plaintext.zeroize();
        return Err("OSL profile exceeds its storage limit".to_owned());
    }
    let decoded = serde_json::from_slice::<ProfileDocument>(&plaintext);
    plaintext.zeroize();
    let document = decoded.map_err(|_| "OSL profile is malformed".to_owned())?;
    if document.version != PROFILE_VERSION || document.owner != owner {
        return Err("OSL profile does not belong to the active identity".to_owned());
    }
    validate_profile(document.profile.into()).map(Some)
}

fn write_profile_with_key(
    path: &Path,
    owner: &str,
    profile: HubProfileDto,
    key: &[u8; 32],
) -> Result<HubProfileDto, String> {
    validate_owner(owner)?;
    let document = ProfileDocument {
        version: PROFILE_VERSION,
        owner: owner.to_owned(),
        profile: profile.clone().into(),
    };
    let mut plaintext =
        serde_json::to_vec(&document).map_err(|_| "OSL profile could not be encoded".to_owned())?;
    if plaintext.len() > MAX_PROFILE_PLAINTEXT_BYTES {
        plaintext.zeroize();
        return Err("OSL profile exceeds its storage limit".to_owned());
    }
    let encrypted = ipc::main_password::encrypt_at_rest(&plaintext, key)
        .map_err(|_| "OSL profile encryption failed".to_owned());
    plaintext.zeroize();
    let sealed = encrypted?;
    if sealed.len() as u64 > MAX_PROFILE_SEALED_BYTES {
        return Err("OSL encrypted profile exceeds its storage limit".to_owned());
    }
    crate::atomic_file::write_recoverable(path, &sealed, "OSL profile")?;
    Ok(profile)
}

pub fn set_profile_picture_with_key(
    path: &Path,
    owner: &str,
    image: String,
    key: &[u8; 32],
) -> Result<OwnerProfilePictureDto, String> {
    validate_owner(owner)?;
    let image = validate_data_avatar(image)?;
    let document = ProfilePictureDocument {
        version: PROFILE_PICTURE_VERSION,
        owner: owner.to_owned(),
        image,
    };
    let mut plaintext = serde_json::to_vec(&document)
        .map_err(|_| "OSL profile picture could not be encoded".to_owned())?;
    if plaintext.len() > MAX_PROFILE_PLAINTEXT_BYTES {
        plaintext.zeroize();
        return Err("OSL profile picture exceeds its storage limit".to_owned());
    }
    let encrypted = ipc::main_password::encrypt_at_rest(&plaintext, key)
        .map_err(|_| "OSL profile picture encryption failed".to_owned());
    plaintext.zeroize();
    let sealed = encrypted?;
    if sealed.len() as u64 > MAX_PROFILE_SEALED_BYTES {
        return Err("OSL encrypted profile picture exceeds its storage limit".to_owned());
    }
    crate::atomic_file::write_recoverable(path, &sealed, "OSL profile picture")?;
    read_profile_picture_with_key(path, owner, key)
}

pub fn read_profile_picture_with_key(
    path: &Path,
    owner: &str,
    key: &[u8; 32],
) -> Result<OwnerProfilePictureDto, String> {
    validate_owner(owner)?;
    let Some(sealed) = crate::atomic_file::read_recoverable_bounded(
        path,
        MAX_PROFILE_SEALED_BYTES,
        "OSL profile picture",
    )?
    else {
        return Ok(absent_picture());
    };
    if !ipc::main_password::has_enc_magic(&sealed) {
        return Err("OSL profile picture is not encrypted".to_owned());
    }
    let mut plaintext = ipc::main_password::decrypt_at_rest(&sealed, key)
        .map_err(|_| "OSL profile picture could not be decrypted".to_owned())?;
    if plaintext.len() > MAX_PROFILE_PLAINTEXT_BYTES {
        plaintext.zeroize();
        return Err("OSL profile picture exceeds its storage limit".to_owned());
    }
    let decoded = serde_json::from_slice::<ProfilePictureDocument>(&plaintext);
    plaintext.zeroize();
    let document = decoded.map_err(|_| "OSL profile picture is malformed".to_owned())?;
    if document.version != PROFILE_PICTURE_VERSION || document.owner != owner {
        return Err("OSL profile picture does not belong to the active identity".to_owned());
    }
    let image = validate_data_avatar(document.image)?;
    Ok(present_picture(image))
}

pub fn read_profile_picture_for_reader_with_key(
    path: &Path,
    owner: &str,
    reader_id: &str,
    accepted_friend_ids: &[String],
    key: &[u8; 32],
) -> Result<OwnerProfilePictureDto, String> {
    validate_owner(owner)?;
    validate_owner(reader_id)?;
    if !accepted_friend_ids
        .iter()
        .any(|accepted_id| accepted_id == reader_id)
    {
        return Ok(absent_picture());
    }
    read_profile_picture_with_key(path, owner, key)
}

pub fn clear_profile_picture_at_path(
    path: &Path,
    owner: &str,
) -> Result<OwnerProfilePictureDto, String> {
    validate_owner(owner)?;
    match std::fs::remove_file(path) {
        Ok(()) => Ok(absent_picture()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(absent_picture()),
        Err(_) => Err("OSL profile picture could not be cleared".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_KEY: [u8; 32] = [0x51; 32];

    fn temporary_file(label: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir()
            .join(format!(
                "osl-profile-{label}-{}-{nonce}",
                std::process::id()
            ))
            .join(PROFILE_FILE)
    }

    fn valid_input() -> HubProfileInput {
        HubProfileInput {
            display_name: "  Liam Example  ".to_owned(),
            username_candidate: "@Liam_Example".to_owned(),
            avatar: Some("data:image/gif;base64,R0lGODlh".to_owned()),
            accent_color: "#22CCEE".to_owned(),
            banner_color: "#101820".to_owned(),
            frame: ProfileFrame::Glow,
            effect: ProfileEffect::Gradient,
            status: "  Available  ".to_owned(),
        }
    }

    fn scoped_record(
        scope: OslProfileScope,
        display_name: &str,
        about_line: &str,
        status: &str,
        card_background: &str,
        avatar: &str,
        colour: &str,
    ) -> ScopedProfileRecord {
        ScopedProfileRecord {
            scope,
            use_separate_profile_here: false,
            display_name: display_name.to_owned(),
            about_line: about_line.to_owned(),
            status: status.to_owned(),
            card_background: card_background.to_owned(),
            avatar: Some(avatar.to_owned()),
            colour: colour.to_owned(),
        }
    }

    fn seeded_scoped_records() -> Vec<ScopedProfileRecord> {
        vec![
            scoped_record(
                OslProfileScope::Global,
                "Global Quinn",
                "Global about line",
                "Global status",
                "#102030",
                "https://example.com/global-avatar.png",
                "#aabbcc",
            ),
            scoped_record(
                OslProfileScope::OslChats,
                "Chats Quinn",
                "Chats about line",
                "Chats status",
                "#203040",
                "https://example.com/chats-avatar.png",
                "#bbccdd",
            ),
            scoped_record(
                OslProfileScope::Enclave {
                    enclave_id: "maple".to_owned(),
                },
                "Maple Quinn",
                "Maple about line",
                "Maple status",
                "#304050",
                "https://example.com/maple-avatar.png",
                "#ccddee",
            ),
            scoped_record(
                OslProfileScope::Enclave {
                    enclave_id: "cedar".to_owned(),
                },
                "Cedar Quinn",
                "Cedar about line",
                "Cedar status",
                "#405060",
                "https://example.com/cedar-avatar.png",
                "#ddeeff",
            ),
        ]
    }

    fn global_scoped_fields() -> ScopedProfileFields {
        ScopedProfileFields {
            display_name: "Global Quinn".to_owned(),
            about_line: "Global about line".to_owned(),
            status: "Global status".to_owned(),
            card_background: "#102030".to_owned(),
            avatar: Some("https://example.com/global-avatar.png".to_owned()),
            colour: "#aabbcc".to_owned(),
        }
    }

    fn changed_scope_keys(resolved: &[ResolvedScopedProfile]) -> BTreeSet<String> {
        let global = global_scoped_fields();
        resolved
            .iter()
            .filter(|profile| profile.profile != global)
            .map(|profile| profile.scope.storage_key())
            .collect()
    }

    fn print_scoped_field_counts(label: &str, resolved: &[ResolvedScopedProfile]) {
        let global = global_scoped_fields();
        let display_names = resolved
            .iter()
            .filter(|profile| profile.profile.display_name == global.display_name)
            .count();
        let about_lines = resolved
            .iter()
            .filter(|profile| profile.profile.about_line == global.about_line)
            .count();
        let statuses = resolved
            .iter()
            .filter(|profile| profile.profile.status == global.status)
            .count();
        let card_backgrounds = resolved
            .iter()
            .filter(|profile| profile.profile.card_background == global.card_background)
            .count();
        let avatars = resolved
            .iter()
            .filter(|profile| profile.profile.avatar == global.avatar)
            .count();
        let colours = resolved
            .iter()
            .filter(|profile| profile.profile.colour == global.colour)
            .count();
        println!(
            "TASK4654 {label}_display_name_matches={display_names} value={}",
            global.display_name
        );
        println!(
            "TASK4654 {label}_about_line_matches={about_lines} value={}",
            global.about_line
        );
        println!(
            "TASK4654 {label}_status_matches={statuses} value={}",
            global.status
        );
        println!(
            "TASK4654 {label}_card_background_matches={card_backgrounds} value={}",
            global.card_background
        );
        println!(
            "TASK4654 {label}_avatar_matches={avatars} value={}",
            global.avatar.as_deref().unwrap_or("none")
        );
        println!(
            "TASK4654 {label}_colour_matches={colours} value={}",
            global.colour
        );
        assert_eq!(display_names, 4);
        assert_eq!(about_lines, 4);
        assert_eq!(statuses, 4);
        assert_eq!(card_backgrounds, 4);
        assert_eq!(avatars, 4);
        assert_eq!(colours, 4);
    }

    #[test]
    fn task_4654_per_scope_profile_records_resolve_global_until_overrides_are_enabled() {
        let records = seeded_scoped_records();
        let zero_overrides = resolve_scoped_profile_records(&records).unwrap();
        let scope_list = zero_overrides
            .iter()
            .map(|profile| profile.scope.storage_key())
            .collect::<Vec<_>>()
            .join(",");
        println!("TASK4654 scopes_count={}", zero_overrides.len());
        println!("TASK4654 scopes={scope_list}");
        assert_eq!(zero_overrides.len(), 4);
        assert_eq!(scope_list, "global,osl-chats,enclave:cedar,enclave:maple");
        print_scoped_field_counts("zero_overrides", &zero_overrides);
        let zero_changed = changed_scope_keys(&zero_overrides);
        println!(
            "TASK4654 zero_overrides_changed_scopes={}",
            zero_changed.len()
        );
        assert!(zero_changed.is_empty());

        let mut chats_override = records.clone();
        chats_override[1].use_separate_profile_here = true;
        let chats_resolved = resolve_scoped_profile_records(&chats_override).unwrap();
        let chats_changed = changed_scope_keys(&chats_resolved);
        println!(
            "TASK4654 chats_override_changed_scopes={}",
            chats_changed.len()
        );
        println!(
            "TASK4654 chats_override_changed_scope={}",
            chats_changed.iter().next().unwrap()
        );
        assert_eq!(chats_changed.len(), 1);
        assert!(chats_changed.contains("osl-chats"));

        let mut enclave_override = chats_override.clone();
        enclave_override[2].use_separate_profile_here = true;
        let enclave_resolved = resolve_scoped_profile_records(&enclave_override).unwrap();
        let enclave_changed = changed_scope_keys(&enclave_resolved);
        let newly_changed = enclave_changed
            .difference(&chats_changed)
            .cloned()
            .collect::<BTreeSet<_>>();
        println!(
            "TASK4654 enclave_override_changed_scopes={}",
            enclave_changed.len()
        );
        println!(
            "TASK4654 enclave_override_newly_changed_scopes={}",
            newly_changed.len()
        );
        println!(
            "TASK4654 enclave_override_newly_changed_scope={}",
            newly_changed.iter().next().unwrap()
        );
        assert_eq!(enclave_changed.len(), 2);
        assert_eq!(newly_changed.len(), 1);
        assert!(newly_changed.contains("enclave:maple"));

        let mut missing_global_name = records;
        missing_global_name[0].display_name = " ".to_owned();
        let refusal = resolve_scoped_profile_records(&missing_global_name).unwrap_err();
        println!("TASK4654 missing_global_display_name_refusal={refusal}");
        assert!(refusal.contains("global display name"));
    }

    #[test]
    fn profile_round_trip_is_encrypted_and_identity_scoped() {
        let path = temporary_file("roundtrip");
        let profile = validate_profile(valid_input()).unwrap();
        write_profile_with_key(&path, "osl-user-a", profile.clone(), &TEST_KEY).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert!(ipc::main_password::has_enc_magic(&bytes));
        let disk = String::from_utf8_lossy(&bytes);
        assert!(!disk.contains("Liam Example"));
        assert!(!disk.contains("liam_example"));
        assert!(!disk.contains("Available"));
        assert!(load_profile_with_key(&path, "osl-user-a", &TEST_KEY).unwrap() == Some(profile));
        assert!(load_profile_with_key(&path, "osl-user-b", &TEST_KEY).is_err());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn normalization_and_strict_field_validation_fail_closed() {
        assert_eq!(
            normalize_username_candidate(" @Mixed_Name_7 ").unwrap(),
            "mixed_name_7"
        );
        for invalid in ["ab", "_starts", "ends_", "two.dots", "space name", "námé"] {
            assert!(normalize_username_candidate(invalid).is_err(), "{invalid}");
        }
        let mut input = valid_input();
        input.display_name = "x".repeat(MAX_DISPLAY_NAME_BYTES + 1);
        assert!(validate_profile(input).is_err());
        let mut input = valid_input();
        input.accent_color = "cyan".to_owned();
        assert!(validate_profile(input).is_err());
        let mut input = valid_input();
        input.avatar = Some("http://example.com/avatar.png".to_owned());
        assert!(validate_profile(input).is_err());
    }

    #[test]
    fn malformed_oversized_and_plaintext_storage_fail_closed() {
        let path = temporary_file("invalid-storage");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, br#"{"displayName":"plaintext"}"#).unwrap();
        assert!(load_profile_with_key(&path, "osl-user-a", &TEST_KEY).is_err());
        std::fs::write(&path, vec![0_u8; MAX_PROFILE_SEALED_BYTES as usize + 1]).unwrap();
        assert!(load_profile_with_key(&path, "osl-user-a", &TEST_KEY).is_err());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn malformed_encrypted_documents_and_oversized_avatar_fail_closed() {
        let path = temporary_file("malformed-encrypted");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let malformed =
            ipc::main_password::encrypt_at_rest(b"{\"version\":1,\"extra\":true}", &TEST_KEY)
                .unwrap();
        std::fs::write(&path, malformed).unwrap();
        assert!(load_profile_with_key(&path, "osl-user-a", &TEST_KEY).is_err());
        let mut input = valid_input();
        input.avatar = Some(format!(
            "data:image/png;base64,{}",
            "A".repeat(MAX_AVATAR_DATA_URL_BYTES)
        ));
        assert!(validate_profile(input).is_err());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn transaction_rollback_restores_previous_or_absent_profile() {
        let path = temporary_file("rollback");
        let previous = validate_profile(valid_input()).unwrap();
        write_profile_with_key(&path, "osl-user-a", previous.clone(), &TEST_KEY).unwrap();
        let mut changed_input = valid_input();
        changed_input.username_candidate = "next_name".to_owned();
        let changed = validate_profile(changed_input).unwrap();
        write_profile_with_key(&path, "osl-user-a", changed, &TEST_KEY).unwrap();
        restore_profile_with_key(&path, "osl-user-a", Some(previous.clone()), &TEST_KEY).unwrap();
        assert_eq!(
            load_profile_with_key(&path, "osl-user-a", &TEST_KEY).unwrap(),
            Some(previous)
        );
        restore_profile_with_key(&path, "osl-user-a", None, &TEST_KEY).unwrap();
        assert_eq!(
            load_profile_with_key(&path, "osl-user-a", &TEST_KEY).unwrap(),
            None
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn owner_profile_picture_commands_set_read_and_clear_encrypted_image_state() {
        let path = temporary_file("picture").with_file_name(PROFILE_PICTURE_FILE);
        let image =
            "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///ywAAAAAAQABAAACAUwAOw==".to_owned();

        let set =
            set_profile_picture_with_key(&path, "osl-user-a", image.clone(), &TEST_KEY).unwrap();
        assert_eq!(set.status(), "image-present");
        assert_eq!(set.image.as_deref(), Some(image.as_str()));

        let bytes = std::fs::read(&path).unwrap();
        assert!(ipc::main_password::has_enc_magic(&bytes));
        assert!(!String::from_utf8_lossy(&bytes).contains("R0lGODlhAQAB"));

        let read = read_profile_picture_with_key(&path, "osl-user-a", &TEST_KEY).unwrap();
        assert_eq!(read.status(), "image-present");
        assert_eq!(read.image.as_deref(), Some(image.as_str()));
        assert!(read_profile_picture_with_key(&path, "osl-user-b", &TEST_KEY).is_err());

        let cleared = clear_profile_picture_at_path(&path, "osl-user-a").unwrap();
        assert_eq!(cleared.status(), "image-absent");
        let read_after_clear =
            read_profile_picture_with_key(&path, "osl-user-a", &TEST_KEY).unwrap();
        assert_eq!(read_after_clear.status(), "image-absent");

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn task_0232_profile_picture_requires_accepted_friendship() {
        let path = temporary_file("task-0232-picture").with_file_name(PROFILE_PICTURE_FILE);
        let image =
            "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///ywAAAAAAQABAAACAUwAOw==".to_owned();
        set_profile_picture_with_key(&path, "900000000000023200", image.clone(), &TEST_KEY)
            .expect("owner picture is stored before friend reads");

        let accepted_friend_ids = vec![
            "900000000000023201".to_owned(),
            "900000000000023200".to_owned(),
        ];
        let friend_read = read_profile_picture_for_reader_with_key(
            &path,
            "900000000000023200",
            "900000000000023201",
            &accepted_friend_ids,
            &TEST_KEY,
        )
        .expect("accepted friend read succeeds");
        println!("TASK0232 friend_read.status={}", friend_read.status());
        println!(
            "TASK0232 friend_read.image_matches={}",
            friend_read.image.as_deref() == Some(image.as_str())
        );
        assert_eq!(friend_read.status(), "image-present");
        assert_eq!(friend_read.image.as_deref(), Some(image.as_str()));

        let non_friend_read = read_profile_picture_for_reader_with_key(
            &path,
            "900000000000023200",
            "900000000000023299",
            &accepted_friend_ids,
            &TEST_KEY,
        )
        .expect("non-friend read is redacted, not disclosed as an error");
        println!(
            "TASK0232 non_friend_read.status={}",
            non_friend_read.status()
        );
        println!(
            "TASK0232 non_friend_read.image_present={}",
            non_friend_read.image.is_some()
        );
        assert_eq!(non_friend_read.status(), "image-absent");
        assert!(non_friend_read.image.is_none());

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
