//! Encrypted, identity-scoped local OSL profile metadata.
//!
//! The username is stored locally after the separately authenticated directory
//! claim succeeds. This module enforces the exact directory normalization.

use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use url::Url;
use zeroize::Zeroize;

const PROFILE_FILE: &str = "hub_profile.json";
const PROFILE_VERSION: u32 = 1;
const MAX_DISPLAY_NAME_CHARS: usize = 64;
const MAX_DISPLAY_NAME_BYTES: usize = 192;
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

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
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

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProfileDocument {
    version: u32,
    owner: String,
    profile: StoredProfile,
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
        assert_eq!(
            load_profile_with_key(&path, "osl-user-a", &TEST_KEY).unwrap(),
            Some(profile)
        );
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
}
