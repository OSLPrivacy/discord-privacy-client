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
pub const ONBOARDING_RESTART_FILE: &str = "onboarding_restart.json";
const STATUS_VERSION: u32 = 1;
const MAX_STATUS_BYTES: u64 = 4 * 1024;
const RESTART_VERSION: u32 = 1;
const MAX_RESTART_BYTES: u64 = 4 * 1024;

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

/// A setup page that is safe to paint before an unfinished account has been
/// unlocked. `Unlock` is deliberately not a member of this type, so callers
/// cannot persist it as an unfinished-account destination.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum SafeOnboardingRestartPage {
    #[serde(rename = "create")]
    CreateAccount,
    #[serde(rename = "recovery")]
    Recovery,
    #[serde(rename = "recovery-check")]
    RecoveryCheck,
    #[serde(rename = "pro")]
    Pro,
    #[serde(rename = "forward-secrecy")]
    ForwardSecrecy,
    #[serde(rename = "privacy")]
    Privacy,
    #[serde(rename = "defaults")]
    Defaults,
    #[serde(rename = "tor")]
    Tor,
    #[serde(rename = "sending")]
    Sending,
    #[serde(rename = "cover")]
    Cover,
    #[serde(rename = "silent-visible")]
    SilentVisible,
    #[serde(rename = "visibility")]
    Visibility,
    #[serde(rename = "passwords")]
    Passwords,
    #[serde(rename = "burnpass")]
    BurnPass,
    #[serde(rename = "mullvad")]
    Mullvad,
    #[serde(rename = "browser")]
    Browser,
    #[serde(rename = "detected")]
    Detected,
    #[serde(rename = "install")]
    Install,
    #[serde(rename = "apps")]
    Apps,
}

impl SafeOnboardingRestartPage {
    pub fn route(self) -> &'static str {
        match self {
            Self::CreateAccount => "create",
            Self::Recovery => "recovery",
            Self::RecoveryCheck => "recovery-check",
            Self::Pro => "pro",
            Self::ForwardSecrecy => "forward-secrecy",
            Self::Privacy => "privacy",
            Self::Defaults => "defaults",
            Self::Tor => "tor",
            Self::Sending => "sending",
            Self::Cover => "cover",
            Self::SilentVisible => "silent-visible",
            Self::Visibility => "visibility",
            Self::Passwords => "passwords",
            Self::BurnPass => "burnpass",
            Self::Mullvad => "mullvad",
            Self::Browser => "browser",
            Self::Detected => "detected",
            Self::Install => "install",
            Self::Apps => "apps",
        }
    }

    pub fn page_name(self) -> &'static str {
        match self {
            Self::CreateAccount => "Create account",
            Self::Recovery => "Recovery",
            Self::RecoveryCheck => "Recovery check",
            Self::Pro => "Pro",
            Self::ForwardSecrecy => "Forward secrecy",
            Self::Privacy => "Privacy",
            Self::Defaults => "Defaults",
            Self::Tor => "Tor",
            Self::Sending => "Sending",
            Self::Cover => "Cover",
            Self::SilentVisible => "Silent or visible",
            Self::Visibility => "Visibility",
            Self::Passwords => "Passwords",
            Self::BurnPass => "Burn password",
            Self::Mullvad => "Mullvad",
            Self::Browser => "Browser",
            Self::Detected => "Detected apps",
            Self::Install => "Install apps",
            Self::Apps => "Choose apps",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum OnboardingRestartPage {
    Safe(SafeOnboardingRestartPage),
    Unlock,
}

impl OnboardingRestartPage {
    pub fn route(self) -> &'static str {
        match self {
            Self::Safe(page) => page.route(),
            Self::Unlock => "unlock",
        }
    }

    pub fn page_name(self) -> &'static str {
        match self {
            Self::Safe(page) => page.page_name(),
            Self::Unlock => "Unlock",
        }
    }

    pub fn is_unlock(self) -> bool {
        matches!(self, Self::Unlock)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum OnboardingRestartState {
    Unfinished { page: SafeOnboardingRestartPage },
    Complete,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OnboardingRestartDocument {
    version: u32,
    state: OnboardingRestartKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    page: Option<SafeOnboardingRestartPage>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum OnboardingRestartKind {
    Unfinished,
    Complete,
}

/// Persist only the non-secret route name needed to resume an unfinished
/// setup. The type excludes `Unlock`, while the strict serialized allowlist
/// makes an injected or future unknown route fall back to Create account.
pub fn record_safe_onboarding_restart_page(
    directory: &Path,
    page: SafeOnboardingRestartPage,
) -> Result<(), String> {
    if page != SafeOnboardingRestartPage::CreateAccount && !account_files_exist(directory) {
        return Err("OSL onboarding restart page has unmet account prerequisites".to_owned());
    }
    write_onboarding_restart_document(directory, OnboardingRestartState::Unfinished { page })
}

/// Mark the setup spine complete. Selection still verifies both canonical
/// account files before it can return Unlock, so a stale completion marker
/// cannot paint an unlock screen over a partial account.
pub fn record_completed_onboarding(directory: &Path) -> Result<(), String> {
    if !account_files_exist(directory) {
        return Err("OSL completed onboarding marker has unmet account prerequisites".to_owned());
    }
    write_onboarding_restart_document(directory, OnboardingRestartState::Complete)
}

/// Choose the cold-start page without decrypting account data.
///
/// Missing, malformed, oversized, symlinked, unsupported, or internally
/// inconsistent unfinished state always returns Create account. The only
/// paths to Unlock are an explicit completed marker with both canonical files,
/// or a legacy account that predates both unfinished-state files.
pub fn choose_onboarding_restart_page(directory: &Path) -> OnboardingRestartPage {
    let create = OnboardingRestartPage::Safe(SafeOnboardingRestartPage::CreateAccount);
    let path = directory.join(ONBOARDING_RESTART_FILE);
    match read_onboarding_restart_document(&path) {
        RestartDocumentRead::Valid(OnboardingRestartState::Unfinished { page }) => {
            if page == SafeOnboardingRestartPage::CreateAccount || account_files_exist(directory) {
                OnboardingRestartPage::Safe(page)
            } else {
                create
            }
        }
        RestartDocumentRead::Valid(OnboardingRestartState::Complete)
            if account_files_exist(directory) =>
        {
            OnboardingRestartPage::Unlock
        }
        RestartDocumentRead::Missing
            if account_files_exist(directory)
                && !directory.join(RECOVERY_KIT_STATUS_FILE).exists() =>
        {
            // Compatibility for accounts completed before gate 3609. A 3609
            // status without the public restart marker is ambiguous while
            // locked, so it takes the safe Create-account fallback instead.
            OnboardingRestartPage::Unlock
        }
        RestartDocumentRead::Valid(_)
        | RestartDocumentRead::Missing
        | RestartDocumentRead::Invalid => create,
    }
}

fn account_files_exist(directory: &Path) -> bool {
    is_regular_nonsymlink(&directory.join("identity.json"))
        && is_regular_nonsymlink(&directory.join("password_marker.json"))
}

fn is_regular_nonsymlink(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .map(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
        .unwrap_or(false)
}

fn write_onboarding_restart_document(
    directory: &Path,
    state: OnboardingRestartState,
) -> Result<(), String> {
    std::fs::create_dir_all(directory)
        .map_err(|_| "OSL onboarding restart directory could not be created".to_owned())?;
    let (kind, page) = match state {
        OnboardingRestartState::Unfinished { page } => {
            (OnboardingRestartKind::Unfinished, Some(page))
        }
        OnboardingRestartState::Complete => (OnboardingRestartKind::Complete, None),
    };
    let encoded = serde_json::to_vec(&OnboardingRestartDocument {
        version: RESTART_VERSION,
        state: kind,
        page,
    })
    .map_err(|_| "OSL onboarding restart page could not be encoded".to_owned())?;
    if encoded.len() as u64 > MAX_RESTART_BYTES {
        return Err("OSL onboarding restart page exceeds its storage limit".to_owned());
    }
    crate::recoverable_file::write_recoverable(&directory.join(ONBOARDING_RESTART_FILE), &encoded)
        .map_err(|_| "OSL onboarding restart page could not be committed".to_owned())
}

enum RestartDocumentRead {
    Missing,
    Valid(OnboardingRestartState),
    Invalid,
}

fn read_onboarding_restart_document(path: &Path) -> RestartDocumentRead {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return RestartDocumentRead::Missing
        }
        Err(_) => return RestartDocumentRead::Invalid,
    };
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_RESTART_BYTES
    {
        return RestartDocumentRead::Invalid;
    }
    let encoded = match std::fs::read(path) {
        Ok(encoded) if encoded.len() as u64 <= MAX_RESTART_BYTES => encoded,
        _ => return RestartDocumentRead::Invalid,
    };
    match serde_json::from_slice::<OnboardingRestartDocument>(&encoded) {
        Ok(OnboardingRestartDocument {
            version: RESTART_VERSION,
            state: OnboardingRestartKind::Unfinished,
            page: Some(page),
        }) => RestartDocumentRead::Valid(OnboardingRestartState::Unfinished { page }),
        Ok(OnboardingRestartDocument {
            version: RESTART_VERSION,
            state: OnboardingRestartKind::Complete,
            page: None,
        }) => RestartDocumentRead::Valid(OnboardingRestartState::Complete),
        _ => RestartDocumentRead::Invalid,
    }
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
