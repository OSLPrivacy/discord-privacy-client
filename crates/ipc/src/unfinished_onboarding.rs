//! Durable authority for accounts interrupted before recovery confirmation.
//!
//! A canonical `identity.json` can exist before onboarding is complete. This
//! module prevents that provisional key from being counted or reused as a
//! usable account, and owns the bounded key cleanup performed when the owner
//! starts that unfinished account again.

use std::io::Write as _;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::AppState;

pub const RECOVERY_KIT_STATUS_FILE: &str = "recovery_kit_status.json";
const STATUS_VERSION: u32 = 1;
const MAX_STATUS_BYTES: u64 = 4 * 1024;

const UNFINISHED_KEY_ARTIFACTS: &[&str] = &[
    "identity.json",
    "prekeys.json",
    "peer_map.json",
    "whitelist_state.json",
    "sender_key_state.json",
    "channels.json",
    "burned_scopes.json",
    "membership.json",
    "scope_ttl.json",
    "scope_blobs.json",
    crate::space_roster::SPACE_ROSTER_FILE,
    crate::tombstone_file::TOMBSTONE_FILE,
    "pending_rotation.json",
    "pending_invitations.json",
    "store",
];

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountSetupStatus {
    pub version: u32,
    pub kit_unsaved: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_confirmed_at_unix_seconds: Option<i64>,
}

impl AccountSetupStatus {
    pub fn unfinished() -> Self {
        Self {
            version: STATUS_VERSION,
            kit_unsaved: true,
            recovery_confirmed_at_unix_seconds: None,
        }
    }

    pub fn confirmed(at_unix_seconds: i64) -> Self {
        Self {
            version: STATUS_VERSION,
            kit_unsaved: false,
            recovery_confirmed_at_unix_seconds: Some(at_unix_seconds),
        }
    }

    pub fn is_unfinished(&self) -> bool {
        self.recovery_confirmed_at_unix_seconds.is_none()
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountUsabilitySnapshot {
    pub account_name: String,
    pub usable_account_count: usize,
    pub usable_key_count: usize,
}

pub fn read_setup_status(
    directory: &Path,
    key: &[u8; 32],
) -> Result<Option<AccountSetupStatus>, String> {
    let path = directory.join(RECOVERY_KIT_STATUS_FILE);
    let sealed = match std::fs::read(&path) {
        Ok(sealed) => sealed,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("OSL unfinished account status could not be read".to_owned()),
    };
    if sealed.len() as u64 > MAX_STATUS_BYTES || !crate::main_password::has_enc_magic(&sealed) {
        return Err("OSL unfinished account status is invalid".to_owned());
    }
    let plaintext = crate::main_password::decrypt_at_rest(&sealed, key)
        .map_err(|_| "OSL unfinished account status could not be decrypted".to_owned())?;
    let status = serde_json::from_slice::<AccountSetupStatus>(&plaintext)
        .map_err(|_| "OSL unfinished account status is malformed".to_owned())?;
    if status.version != STATUS_VERSION {
        return Err("OSL unfinished account status version is unsupported".to_owned());
    }
    Ok(Some(status))
}

pub fn write_setup_status(
    directory: &Path,
    status: &AccountSetupStatus,
    key: &[u8; 32],
) -> Result<(), String> {
    if status.version != STATUS_VERSION {
        return Err("OSL unfinished account status version is unsupported".to_owned());
    }
    std::fs::create_dir_all(directory)
        .map_err(|_| "OSL unfinished account status directory could not be created".to_owned())?;
    let plaintext = serde_json::to_vec(status)
        .map_err(|_| "OSL unfinished account status could not be encoded".to_owned())?;
    let sealed = crate::main_password::encrypt_at_rest(&plaintext, key)
        .map_err(|_| "OSL unfinished account status could not be encrypted".to_owned())?;
    if sealed.len() as u64 > MAX_STATUS_BYTES {
        return Err("OSL unfinished account status exceeds its storage limit".to_owned());
    }
    let path = directory.join(RECOVERY_KIT_STATUS_FILE);
    let temporary = directory.join(format!(
        ".recovery-kit-status-{}-{}.tmp",
        std::process::id(),
        crate::main_password::now_unix_secs_pub()
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|_| "OSL unfinished account status could not be staged".to_owned())?;
    if file
        .write_all(&sealed)
        .and_then(|_| file.sync_all())
        .is_err()
    {
        drop(file);
        let _ = std::fs::remove_file(&temporary);
        return Err("OSL unfinished account status could not be synchronized".to_owned());
    }
    drop(file);
    if std::fs::rename(&temporary, &path).is_err() {
        let _ = std::fs::remove_file(&temporary);
        return Err("OSL unfinished account status could not be committed".to_owned());
    }
    Ok(())
}

pub fn account_usability_snapshot(
    directory: &Path,
    key: &[u8; 32],
) -> Result<AccountUsabilitySnapshot, String> {
    let confirmed = read_setup_status(directory, key)?
        .map(|status| !status.is_unfinished())
        // Compatibility for accounts completed before this status existed,
        // while still recognizing an identity interrupted before its password.
        .unwrap_or_else(|| {
            !directory.join("identity.json").is_file()
                || directory.join("password_marker.json").is_file()
        });
    let usable = confirmed && directory.join("identity.json").is_file();
    Ok(AccountUsabilitySnapshot {
        account_name: if confirmed { "finished" } else { "unfinished" }.to_owned(),
        usable_account_count: usize::from(usable),
        usable_key_count: usize::from(usable),
    })
}

/// Remove prior key material when an authenticated owner starts an unfinished
/// account again. Confirmed and legacy accounts are never touched.
pub fn restart_unfinished_account(
    state: &AppState,
    directory: &Path,
    key: &[u8; 32],
) -> Result<bool, String> {
    let Some(status) = read_setup_status(directory, key)? else {
        return Ok(false);
    };
    if !status.is_unfinished() {
        return Ok(false);
    }

    remove_unfinished_account_keys(state, directory)?;
    Ok(true)
}

/// Retry the earlier interruption point: identity creation completed but the
/// password (and therefore encrypted recovery status) was never created.
pub fn restart_pre_password_unfinished_account(
    state: &AppState,
    directory: &Path,
) -> Result<bool, String> {
    if directory.join(RECOVERY_KIT_STATUS_FILE).exists()
        || directory.join("password_marker.json").exists()
        || !directory.join("identity.json").is_file()
    {
        return Ok(false);
    }
    remove_unfinished_account_keys(state, directory)?;
    Ok(true)
}

fn remove_unfinished_account_keys(state: &AppState, directory: &Path) -> Result<(), String> {
    state.clear_identity();
    for relative in UNFINISHED_KEY_ARTIFACTS {
        let path = directory.join(relative);
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err("OSL unfinished account keys could not be inspected".to_owned()),
        };
        let removed = if metadata.file_type().is_symlink() || metadata.is_file() {
            std::fs::remove_file(&path)
        } else if metadata.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            return Err("OSL unfinished account keys have an unsupported file type".to_owned());
        };
        removed.map_err(|_| "OSL unfinished account keys could not be removed".to_owned())?;
    }
    Ok(())
}
