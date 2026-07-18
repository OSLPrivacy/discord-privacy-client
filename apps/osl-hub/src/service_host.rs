use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;

use crate::models::EmailProvider;

const MAX_OPAQUE_ID_LEN: usize = 64;
static TOMBSTONE_SEQUENCE: AtomicU64 = AtomicU64::new(1);
#[cfg(feature = "desktop")]
const PROFILE_NAMESPACE: &str = "service-profiles-v2";
#[cfg(any(feature = "desktop", test))]
const DESKTOP_TITLE_HEIGHT: u32 = 44;
#[cfg(any(feature = "desktop", test))]
const TRUSTED_BAR_HEIGHT: u32 = 54;
#[cfg(any(feature = "desktop", test))]
const LOCAL_PROTECTED_SHEET_WIDTH: u32 = 420;
#[cfg(any(feature = "desktop", test))]
const MIN_SERVICE_HOST_WIDTH_WITH_SHEET: u32 = 320;
#[cfg(test)]
const MIN_TRUSTED_OVERLAY_WIDTH: u32 = 320;
#[cfg(test)]
const MIN_TRUSTED_OVERLAY_HEIGHT: u32 = 44;
const PROFILE_OWNER_DOMAIN: &[u8] = b"OSL-HUB/service-profile-owner/v1";

pub(crate) fn owner_profile_namespace(owner_osl_user_id: &str) -> Result<String, ServiceHostError> {
    if owner_osl_user_id.is_empty() || owner_osl_user_id.len() > 128 {
        return Err(ServiceHostError::InvalidOpaqueId);
    }
    let mut hash = Sha256::new();
    hash.update(PROFILE_OWNER_DOMAIN);
    hash.update(owner_osl_user_id.as_bytes());
    let digest = hash.finalize();
    let mut namespace = String::with_capacity(6 + 48);
    namespace.push_str("owner-");
    for byte in &digest[..24] {
        use std::fmt::Write as _;
        let _ = write!(namespace, "{byte:02x}");
    }
    Ok(namespace)
}

#[cfg(any(feature = "desktop", test))]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct HostRect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

#[cfg(any(feature = "desktop", test))]
fn host_rect(width: u32, height: u32) -> HostRect {
    let top_reserved = DESKTOP_TITLE_HEIGHT.saturating_add(TRUSTED_BAR_HEIGHT);
    HostRect {
        x: 0,
        y: top_reserved,
        width: width.max(1),
        height: height.saturating_sub(top_reserved).max(1),
    }
}

#[cfg(any(feature = "desktop", test))]
fn local_protected_sheet_rect(width: u32, height: u32, open: bool) -> HostRect {
    let mut rect = host_rect(width, height);
    if open {
        let reserve = LOCAL_PROTECTED_SHEET_WIDTH
            .min(rect.width.saturating_sub(MIN_SERVICE_HOST_WIDTH_WITH_SHEET));
        rect.width = rect.width.saturating_sub(reserve).max(1);
    }
    rect
}

/// Accepts only an explicit, wholly hosted-surface overlay bound. Geometry is
/// necessary but never sufficient evidence of a composer or conversation;
/// the future semantic adapter must establish that identity before calling
/// the layout API that consumes this validation.
#[cfg(test)]
fn validate_trusted_overlay_rect(width: u32, height: u32, candidate: HostRect) -> Option<HostRect> {
    if candidate.width < MIN_TRUSTED_OVERLAY_WIDTH || candidate.height < MIN_TRUSTED_OVERLAY_HEIGHT
    {
        return None;
    }
    let host = host_rect(width, height);
    let candidate_right = candidate.x.checked_add(candidate.width)?;
    let candidate_bottom = candidate.y.checked_add(candidate.height)?;
    let host_right = host.x.checked_add(host.width)?;
    let host_bottom = host.y.checked_add(host.height)?;
    if candidate.x < host.x
        || candidate.y < host.y
        || candidate_right > host_right
        || candidate_bottom > host_bottom
    {
        return None;
    }
    Some(candidate)
}

#[cfg(any(feature = "desktop", test))]
fn overlay_navigation_allowed(url: &Url) -> bool {
    let local_origin = (url.scheme() == "tauri" && url.host_str() == Some("localhost"))
        || (url.scheme() == "http"
            && url.host_str() == Some("tauri.localhost")
            && url.port().is_none());
    local_origin
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && matches!(url.path(), "/overlay.html" | "/overlay.html/")
}

/// A service may navigate only to these exact hosts over HTTPS. This is
/// deliberately an explicit list rather than a suffix or wildcard policy.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ServiceManifest {
    pub id: &'static str,
    pub display_name: &'static str,
    pub initial_url: &'static str,
    pub allowed_hosts: &'static [&'static str],
    pub launch_active: bool,
}

const SERVICES: &[ServiceManifest] = &[
    ServiceManifest {
        id: "discord",
        display_name: "Discord",
        initial_url: "https://discord.com/app",
        allowed_hosts: &["discord.com"],
        launch_active: true,
    },
    ServiceManifest {
        id: "telegram",
        display_name: "Telegram",
        initial_url: "https://web.telegram.org/a/",
        allowed_hosts: &["web.telegram.org"],
        launch_active: true,
    },
    ServiceManifest {
        id: "whatsapp",
        display_name: "WhatsApp",
        initial_url: "https://web.whatsapp.com/",
        allowed_hosts: &["web.whatsapp.com"],
        launch_active: true,
    },
    ServiceManifest {
        id: "instagram",
        display_name: "Instagram",
        initial_url: "https://www.instagram.com/",
        allowed_hosts: &["www.instagram.com"],
        launch_active: true,
    },
    ServiceManifest {
        id: "snapchat",
        display_name: "Snapchat",
        initial_url: "https://www.snapchat.com/web/",
        allowed_hosts: &["www.snapchat.com", "accounts.snapchat.com"],
        launch_active: true,
    },
    ServiceManifest {
        id: "email",
        display_name: "Email",
        initial_url: "https://mail.google.com/",
        allowed_hosts: &["mail.google.com", "accounts.google.com"],
        launch_active: true,
    },
    ServiceManifest {
        id: "x",
        display_name: "X",
        initial_url: "https://x.com/messages",
        allowed_hosts: &["x.com"],
        launch_active: true,
    },
    ServiceManifest {
        id: "signal",
        display_name: "Signal",
        // Signal does not offer a first-party web messenger. This URL is never
        // launched; it exists only as bounded metadata for the coming-soon row.
        initial_url: "https://signal.org/download/",
        allowed_hosts: &["signal.org"],
        launch_active: false,
    },
    ServiceManifest {
        id: "slack",
        display_name: "Slack",
        initial_url: "https://app.slack.com/",
        allowed_hosts: &["app.slack.com", "slack.com"],
        launch_active: false,
    },
    ServiceManifest {
        id: "teams",
        display_name: "Microsoft Teams",
        initial_url: "https://teams.microsoft.com/",
        allowed_hosts: &[
            "teams.microsoft.com",
            "teams.cloud.microsoft",
            "login.microsoftonline.com",
            "login.live.com",
        ],
        launch_active: false,
    },
    ServiceManifest {
        id: "messenger",
        display_name: "Facebook Messenger",
        initial_url: "https://www.messenger.com/",
        allowed_hosts: &["www.messenger.com", "www.facebook.com"],
        launch_active: true,
    },
];

const EMAIL_GMAIL: ServiceManifest = ServiceManifest {
    id: "email",
    display_name: "Gmail",
    initial_url: "https://mail.google.com/",
    allowed_hosts: &["mail.google.com", "accounts.google.com"],
    launch_active: true,
};
const EMAIL_OUTLOOK: ServiceManifest = ServiceManifest {
    id: "email",
    display_name: "Outlook",
    initial_url: "https://outlook.live.com/mail/0/",
    allowed_hosts: &[
        "outlook.live.com",
        "login.live.com",
        "account.live.com",
        "login.microsoftonline.com",
    ],
    launch_active: true,
};
const EMAIL_PROTON: ServiceManifest = ServiceManifest {
    id: "email",
    display_name: "Proton Mail",
    initial_url: "https://mail.proton.me/",
    allowed_hosts: &["mail.proton.me", "account.proton.me"],
    launch_active: true,
};
const EMAIL_TUTA: ServiceManifest = ServiceManifest {
    id: "email",
    display_name: "Tuta Mail",
    initial_url: "https://app.tuta.com/",
    allowed_hosts: &["app.tuta.com"],
    launch_active: true,
};
const EMAIL_FASTMAIL: ServiceManifest = ServiceManifest {
    id: "email",
    display_name: "Fastmail",
    initial_url: "https://app.fastmail.com/login/",
    allowed_hosts: &["app.fastmail.com"],
    launch_active: true,
};
const EMAIL_YAHOO: ServiceManifest = ServiceManifest {
    id: "email",
    display_name: "Yahoo Mail",
    initial_url: "https://mail.yahoo.com/",
    allowed_hosts: &["mail.yahoo.com", "login.yahoo.com"],
    launch_active: true,
};
const EMAIL_ZOHO: ServiceManifest = ServiceManifest {
    id: "email",
    display_name: "Zoho Mail",
    initial_url: "https://mail.zoho.com/",
    allowed_hosts: &["mail.zoho.com", "accounts.zoho.com"],
    launch_active: true,
};
const EMAIL_AOL: ServiceManifest = ServiceManifest {
    id: "email",
    display_name: "AOL Mail",
    initial_url: "https://mail.aol.com/",
    allowed_hosts: &["mail.aol.com", "login.aol.com", "login.yahoo.com"],
    launch_active: true,
};
const EMAIL_GMX: ServiceManifest = ServiceManifest {
    id: "email",
    display_name: "GMX",
    initial_url: "https://www.gmx.com/",
    allowed_hosts: &["www.gmx.com", "login.gmx.com", "navigator.gmx.com"],
    launch_active: true,
};
const EMAIL_MAIL_COM: ServiceManifest = ServiceManifest {
    id: "email",
    display_name: "Mail.com",
    initial_url: "https://www.mail.com/",
    allowed_hosts: &["www.mail.com", "login.mail.com", "navigator-lxa.mail.com"],
    launch_active: true,
};

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ServiceHostError {
    InvalidOpaqueId,
    UnknownService,
    ServiceUnavailable,
    InvalidUrl,
    NavigationDenied,
    UnsafeProfilePath,
    Io(String),
    Runtime(String),
}

impl fmt::Display for ServiceHostError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidOpaqueId => formatter.write_str("invalid opaque identifier"),
            Self::UnknownService => formatter.write_str("unknown service"),
            Self::ServiceUnavailable => formatter.write_str("service is not launch-active"),
            Self::InvalidUrl => formatter.write_str("service URL is invalid"),
            Self::NavigationDenied => {
                formatter.write_str("navigation is outside the service policy")
            }
            Self::UnsafeProfilePath => formatter.write_str("service profile path is unsafe"),
            Self::Io(message) => write!(formatter, "service profile I/O failed: {message}"),
            Self::Runtime(message) => write!(formatter, "service host operation failed: {message}"),
        }
    }
}

impl std::error::Error for ServiceHostError {}

pub fn service_manifest(service_id: &str) -> Result<&'static ServiceManifest, ServiceHostError> {
    validate_opaque_id(service_id)?;
    SERVICES
        .iter()
        .find(|service| service.id == service_id)
        .ok_or(ServiceHostError::UnknownService)
}

pub fn service_manifest_for_provider(
    service_id: &str,
    provider: Option<EmailProvider>,
) -> Result<&'static ServiceManifest, ServiceHostError> {
    if service_id != "email" {
        return service_manifest(service_id);
    }
    Ok(match provider.unwrap_or_default() {
        EmailProvider::Gmail => &EMAIL_GMAIL,
        EmailProvider::Outlook => &EMAIL_OUTLOOK,
        EmailProvider::Proton => &EMAIL_PROTON,
        EmailProvider::Tuta => &EMAIL_TUTA,
        EmailProvider::Fastmail => &EMAIL_FASTMAIL,
        EmailProvider::Yahoo => &EMAIL_YAHOO,
        EmailProvider::Zoho => &EMAIL_ZOHO,
        EmailProvider::Aol => &EMAIL_AOL,
        EmailProvider::Gmx => &EMAIL_GMX,
        EmailProvider::Maildotcom => &EMAIL_MAIL_COM,
    })
}

/// Opaque IDs are intentionally much narrower than filenames. Prefixes are
/// added before they are used as directory names, avoiding reserved names.
pub fn validate_opaque_id(value: &str) -> Result<(), ServiceHostError> {
    if value.is_empty() || value.len() > MAX_OPAQUE_ID_LEN {
        return Err(ServiceHostError::InvalidOpaqueId);
    }

    let bytes = value.as_bytes();
    let edge_is_safe = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
    if !edge_is_safe(bytes[0]) || !edge_is_safe(bytes[bytes.len() - 1]) {
        return Err(ServiceHostError::InvalidOpaqueId);
    }

    if bytes
        .iter()
        .any(|byte| !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-'))
    {
        return Err(ServiceHostError::InvalidOpaqueId);
    }
    Ok(())
}

pub fn navigation_allowed(manifest: &ServiceManifest, url: &Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url
            .host_str()
            .is_some_and(|host| manifest.allowed_hosts.contains(&host))
}

pub fn validated_initial_url(manifest: &ServiceManifest) -> Result<Url, ServiceHostError> {
    if !manifest.launch_active {
        return Err(ServiceHostError::ServiceUnavailable);
    }
    let url = Url::parse(manifest.initial_url).map_err(|_| ServiceHostError::InvalidUrl)?;
    if navigation_allowed(manifest, &url) {
        Ok(url)
    } else {
        Err(ServiceHostError::NavigationDenied)
    }
}

pub fn profile_path(
    profile_root: &Path,
    service_id: &str,
    account_id: &str,
) -> Result<PathBuf, ServiceHostError> {
    service_manifest(service_id)?;
    validate_opaque_id(account_id)?;
    let path = profile_root
        .join(format!("service-{service_id}"))
        .join(format!("account-{account_id}"));
    if !path.starts_with(profile_root) {
        return Err(ServiceHostError::UnsafeProfilePath);
    }
    Ok(path)
}

/// Create the isolated profile without following a pre-planted symlink out of
/// the app-owned root. Existing non-directory components are rejected.
pub fn prepare_profile_directory(
    profile_root: &Path,
    service_id: &str,
    account_id: &str,
) -> Result<PathBuf, ServiceHostError> {
    fs::create_dir_all(profile_root).map_err(|error| ServiceHostError::Io(error.to_string()))?;
    let root_metadata = fs::symlink_metadata(profile_root)
        .map_err(|error| ServiceHostError::Io(error.to_string()))?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(ServiceHostError::UnsafeProfilePath);
    }
    let canonical_root =
        fs::canonicalize(profile_root).map_err(|error| ServiceHostError::Io(error.to_string()))?;
    let lexical_target = profile_path(&canonical_root, service_id, account_id)?;

    let relative = lexical_target
        .strip_prefix(&canonical_root)
        .map_err(|_| ServiceHostError::UnsafeProfilePath)?;
    let mut current = canonical_root.clone();
    for component in relative.components() {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(ServiceHostError::UnsafeProfilePath);
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current)
                    .map_err(|create_error| ServiceHostError::Io(create_error.to_string()))?;
            }
            Err(error) => return Err(ServiceHostError::Io(error.to_string())),
        }
    }

    let canonical_target =
        fs::canonicalize(&current).map_err(|error| ServiceHostError::Io(error.to_string()))?;
    if !canonical_target.starts_with(&canonical_root) {
        return Err(ServiceHostError::UnsafeProfilePath);
    }
    Ok(canonical_target)
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileResetOutcome {
    pub profile_existed: bool,
    pub cleanup_pending: bool,
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TombstoneScavengeReport {
    pub removed: u32,
    pub cleanup_pending: u32,
}

/// Retry cleanup of profiles that were already detached from their live
/// account names by an earlier reset. Only immediate, app-generated-looking
/// directories inside the canonical profile namespace are considered.
pub fn scavenge_profile_tombstones(
    profile_root: &Path,
) -> Result<TombstoneScavengeReport, ServiceHostError> {
    if !profile_root.exists() {
        return Ok(TombstoneScavengeReport::default());
    }
    let metadata = fs::symlink_metadata(profile_root)
        .map_err(|error| ServiceHostError::Io(error.to_string()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(ServiceHostError::UnsafeProfilePath);
    }
    let canonical_root =
        fs::canonicalize(profile_root).map_err(|error| ServiceHostError::Io(error.to_string()))?;
    let mut report = TombstoneScavengeReport::default();
    let entries =
        fs::read_dir(&canonical_root).map_err(|error| ServiceHostError::Io(error.to_string()))?;
    for entry in entries {
        let entry = entry.map_err(|error| ServiceHostError::Io(error.to_string()))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !looks_like_profile_tombstone(&name) {
            continue;
        }
        let metadata = match fs::symlink_metadata(entry.path()) {
            Ok(metadata) => metadata,
            Err(_) => {
                report.cleanup_pending = report.cleanup_pending.saturating_add(1);
                continue;
            }
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            report.cleanup_pending = report.cleanup_pending.saturating_add(1);
            continue;
        }
        let candidate = match fs::canonicalize(entry.path()) {
            Ok(candidate) if candidate.starts_with(&canonical_root) => candidate,
            _ => {
                report.cleanup_pending = report.cleanup_pending.saturating_add(1);
                continue;
            }
        };
        if fs::remove_dir_all(candidate).is_ok() {
            report.removed = report.removed.saturating_add(1);
        } else {
            report.cleanup_pending = report.cleanup_pending.saturating_add(1);
        }
    }
    Ok(report)
}

/// Scan only owner-hash namespaces. Pre-ownership flat service directories
/// are deliberately ignored and therefore quarantined rather than claimed by
/// whichever identity happens to launch first after upgrade.
#[cfg(any(feature = "desktop", test))]
fn scavenge_owned_profile_tombstones(
    profile_base: &Path,
) -> Result<TombstoneScavengeReport, ServiceHostError> {
    if !profile_base.exists() {
        return Ok(TombstoneScavengeReport::default());
    }
    let metadata = fs::symlink_metadata(profile_base)
        .map_err(|error| ServiceHostError::Io(error.to_string()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(ServiceHostError::UnsafeProfilePath);
    }
    let mut total = TombstoneScavengeReport::default();
    for entry in
        fs::read_dir(profile_base).map_err(|error| ServiceHostError::Io(error.to_string()))?
    {
        let entry = entry.map_err(|error| ServiceHostError::Io(error.to_string()))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(hash) = name.strip_prefix("owner-") else {
            continue;
        };
        if hash.len() != 48
            || !hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| ServiceHostError::Io(error.to_string()))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            continue;
        }
        let report = scavenge_profile_tombstones(&entry.path())?;
        total.removed = total.removed.saturating_add(report.removed);
        total.cleanup_pending = total.cleanup_pending.saturating_add(report.cleanup_pending);
    }
    Ok(total)
}

fn looks_like_profile_tombstone(name: &str) -> bool {
    let Some(rest) = name.strip_prefix(".deleted-") else {
        return false;
    };
    rest.len() >= 9
        && name.len() <= 240
        && rest.bytes().filter(|byte| *byte == b'-').count() >= 4
        && rest
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// Atomically detach one proven account profile from its live name, then
/// remove only that in-root tombstone on a best-effort basis. A failed cleanup
/// cannot cause the reset profile name to be reused with its old state.
pub fn tombstone_profile_directory(
    profile_root: &Path,
    service_id: &str,
    account_id: &str,
) -> Result<ProfileResetOutcome, ServiceHostError> {
    service_manifest(service_id)?;
    validate_opaque_id(account_id)?;
    if !profile_root.exists() {
        return Ok(ProfileResetOutcome::default());
    }
    let root_metadata = fs::symlink_metadata(profile_root)
        .map_err(|error| ServiceHostError::Io(error.to_string()))?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(ServiceHostError::UnsafeProfilePath);
    }

    let canonical_root =
        fs::canonicalize(profile_root).map_err(|error| ServiceHostError::Io(error.to_string()))?;
    let profile = profile_path(&canonical_root, service_id, account_id)?;
    let metadata = match fs::symlink_metadata(&profile) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ProfileResetOutcome::default());
        }
        Err(error) => return Err(ServiceHostError::Io(error.to_string())),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(ServiceHostError::UnsafeProfilePath);
    }
    let canonical_profile =
        fs::canonicalize(&profile).map_err(|error| ServiceHostError::Io(error.to_string()))?;
    if !canonical_profile.starts_with(&canonical_root) {
        return Err(ServiceHostError::UnsafeProfilePath);
    }

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = TOMBSTONE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let tombstone = canonical_root.join(format!(
        ".deleted-{service_id}-{account_id}-{}-{nonce:x}-{sequence:x}",
        std::process::id()
    ));
    if !tombstone.starts_with(&canonical_root) || tombstone.exists() {
        return Err(ServiceHostError::UnsafeProfilePath);
    }
    fs::rename(&canonical_profile, &tombstone)
        .map_err(|error| ServiceHostError::Io(error.to_string()))?;
    let cleanup_pending = fs::remove_dir_all(&tombstone).is_err();
    Ok(ProfileResetOutcome {
        profile_existed: true,
        cleanup_pending,
    })
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceAccountMutation {
    pub service_id: String,
    pub account_id: String,
    pub profile_existed: bool,
    pub cleanup_pending: bool,
    pub registry_removed: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveServiceHost {
    pub service_id: String,
    pub account_id: String,
    pub generation: u64,
    #[serde(skip_serializing)]
    pub(crate) owner_namespace: String,
}

#[cfg(any(feature = "desktop", test))]
fn active_profile_matches(
    active: Option<&ActiveServiceHost>,
    owner_namespace: &str,
    service_id: &str,
    account_id: &str,
) -> bool {
    active.is_some_and(|active| {
        active.owner_namespace == owner_namespace
            && active.service_id == service_id
            && active.account_id == account_id
    })
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ServiceHostPhase {
    #[default]
    Closed,
    Opening,
    Navigating,
    /// Tauri reported that a document finished loading. This deliberately
    /// does not claim that an HTTP response, login, or service feature works.
    DocumentReady,
    /// The exact account webview is retained but hidden. Reopening the same
    /// account resumes its current page without a new navigation or login.
    Suspended,
    NavigationBlocked,
    Failed,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ServiceHostFailureCode {
    ProfileUnavailable,
    ExistingHostCloseFailed,
    MainWindowUnavailable,
    WebviewCreationFailed,
    WebviewShowFailed,
    PageLoadFailed,
    NavigationBlocked,
    LayoutFailed,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceHostFailure {
    pub code: ServiceHostFailureCode,
    pub message: &'static str,
    pub retryable: bool,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceHostStatus {
    pub phase: ServiceHostPhase,
    pub active: Option<ActiveServiceHost>,
    /// Exact allowlisted host only. Paths, query strings, fragments, profile
    /// paths, cookies, and page content are never retained here.
    pub host: Option<String>,
    pub blocked_navigation_count: u64,
    pub last_error: Option<ServiceHostFailure>,
}

#[derive(Debug, Default)]
struct HostGeneration {
    generation: u64,
    active: Option<ActiveServiceHost>,
    retained: Option<ActiveServiceHost>,
    phase: ServiceHostPhase,
    resume_phase: ServiceHostPhase,
    host: Option<String>,
    blocked_navigation_count: u64,
    last_error: Option<ServiceHostFailure>,
}

/// Mutations are serialized for the entire close/create transition. The
/// monotonically increasing generation makes every replacement observable and
/// prevents a stale open from being recorded as current.
#[derive(Debug, Default)]
pub struct ServiceHostState {
    inner: Mutex<HostGeneration>,
    #[cfg(any(feature = "desktop", test))]
    transition: Mutex<()>,
}

impl ServiceHostState {
    pub fn current(&self) -> Result<Option<ActiveServiceHost>, ServiceHostError> {
        self.inner
            .lock()
            .map(|state| state.active.clone())
            .map_err(|_| ServiceHostError::Runtime("service host state is poisoned".to_owned()))
    }

    /// Return only the exact active host owned by the currently unlocked OSL
    /// identity. The owner namespace is derived internally and never exposed to
    /// the caller or serialized to a remote service page.
    pub fn require_current_owned(
        &self,
        owner_osl_user_id: &str,
        service_id: &str,
        account_id: &str,
    ) -> Result<ActiveServiceHost, ServiceHostError> {
        let owner_namespace = owner_profile_namespace(owner_osl_user_id)?;
        self.current()?
            .filter(|active| {
                active.owner_namespace == owner_namespace
                    && active.service_id == service_id
                    && active.account_id == account_id
                    && active.generation != 0
            })
            .ok_or_else(|| {
                ServiceHostError::Runtime(
                    "active service host does not belong to the unlocked OSL identity".to_owned(),
                )
            })
    }

    pub fn current_or_retained(&self) -> Result<Option<ActiveServiceHost>, ServiceHostError> {
        self.inner
            .lock()
            .map(|state| state.active.clone().or_else(|| state.retained.clone()))
            .map_err(|_| ServiceHostError::Runtime("service host state is poisoned".to_owned()))
    }

    pub fn next_generation(&self) -> Result<u64, ServiceHostError> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| ServiceHostError::Runtime("service host state is poisoned".to_owned()))?;
        state.generation = state.generation.saturating_add(1);
        state.active = None;
        state.retained = None;
        state.phase = ServiceHostPhase::Closed;
        state.resume_phase = ServiceHostPhase::Closed;
        state.host = None;
        state.last_error = None;
        Ok(state.generation)
    }

    pub fn status(&self) -> Result<ServiceHostStatus, ServiceHostError> {
        self.inner
            .lock()
            .map(|state| ServiceHostStatus {
                phase: state.phase,
                active: state.active.clone(),
                host: state.host.clone(),
                blocked_navigation_count: state.blocked_navigation_count,
                last_error: state.last_error.clone(),
            })
            .map_err(|_| ServiceHostError::Runtime("service host state is poisoned".to_owned()))
    }

    pub fn begin_open(
        &self,
        owner_namespace: &str,
        service_id: &str,
        account_id: &str,
        host: &str,
    ) -> Result<ActiveServiceHost, ServiceHostError> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| ServiceHostError::Runtime("service host state is poisoned".to_owned()))?;
        state.generation = state.generation.saturating_add(1);
        let active = ActiveServiceHost {
            service_id: service_id.to_owned(),
            account_id: account_id.to_owned(),
            generation: state.generation,
            owner_namespace: owner_namespace.to_owned(),
        };
        state.active = Some(active.clone());
        state.retained = None;
        state.phase = ServiceHostPhase::Opening;
        state.resume_phase = ServiceHostPhase::Opening;
        state.host = Some(host.to_owned());
        state.last_error = None;
        Ok(active)
    }

    pub fn page_load(
        &self,
        generation: u64,
        host: Option<&str>,
        finished: bool,
    ) -> Result<(), ServiceHostError> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| ServiceHostError::Runtime("service host state is poisoned".to_owned()))?;
        if generation != state.generation {
            return Ok(());
        }
        if let Some(host) = host {
            state.host = Some(host.to_owned());
        }
        let next_phase = if finished {
            ServiceHostPhase::DocumentReady
        } else {
            ServiceHostPhase::Navigating
        };
        if state.phase == ServiceHostPhase::Suspended {
            state.resume_phase = next_phase;
        } else {
            state.phase = next_phase;
            state.resume_phase = next_phase;
        }
        state.last_error = None;
        Ok(())
    }

    pub fn navigation_blocked(&self, generation: u64) -> Result<(), ServiceHostError> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| ServiceHostError::Runtime("service host state is poisoned".to_owned()))?;
        if generation != state.generation {
            return Ok(());
        }
        state.blocked_navigation_count = state.blocked_navigation_count.saturating_add(1);
        if state.phase == ServiceHostPhase::Suspended {
            state.resume_phase = ServiceHostPhase::NavigationBlocked;
        } else {
            state.phase = ServiceHostPhase::NavigationBlocked;
        }
        state.last_error = Some(ServiceHostFailure {
            code: ServiceHostFailureCode::NavigationBlocked,
            message: "The service tried to leave its explicit HTTPS host allowlist.",
            retryable: false,
        });
        Ok(())
    }

    pub fn fail(
        &self,
        generation: u64,
        code: ServiceHostFailureCode,
        message: &'static str,
        retryable: bool,
    ) -> Result<(), ServiceHostError> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| ServiceHostError::Runtime("service host state is poisoned".to_owned()))?;
        if generation != state.generation {
            return Ok(());
        }
        state.active = None;
        state.retained = None;
        state.phase = ServiceHostPhase::Failed;
        state.resume_phase = ServiceHostPhase::Failed;
        state.last_error = Some(ServiceHostFailure {
            code,
            message,
            retryable,
        });
        Ok(())
    }

    pub fn activate(
        &self,
        service_id: &str,
        account_id: &str,
        generation: u64,
    ) -> Result<ActiveServiceHost, ServiceHostError> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| ServiceHostError::Runtime("service host state is poisoned".to_owned()))?;
        if generation != state.generation {
            return Err(ServiceHostError::Runtime(
                "service host generation was superseded".to_owned(),
            ));
        }
        let active = state
            .active
            .as_ref()
            .filter(|active| {
                active.generation == generation
                    && active.service_id == service_id
                    && active.account_id == account_id
            })
            .cloned()
            .ok_or_else(|| {
                ServiceHostError::Runtime("service host identity changed while opening".to_owned())
            })?;
        state.active = Some(active.clone());
        state.retained = None;
        if state.phase == ServiceHostPhase::Opening {
            state.phase = ServiceHostPhase::Navigating;
        }
        Ok(active)
    }

    pub fn suspend(&self) -> Result<Option<ActiveServiceHost>, ServiceHostError> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| ServiceHostError::Runtime("service host state is poisoned".to_owned()))?;
        let Some(active) = state.active.take() else {
            return Ok(state.retained.clone());
        };
        state.resume_phase = match state.phase {
            ServiceHostPhase::Opening | ServiceHostPhase::Navigating => {
                ServiceHostPhase::Navigating
            }
            ServiceHostPhase::Suspended | ServiceHostPhase::Closed => {
                ServiceHostPhase::DocumentReady
            }
            phase => phase,
        };
        state.retained = Some(active.clone());
        state.phase = ServiceHostPhase::Suspended;
        Ok(Some(active))
    }

    pub fn resume(
        &self,
        owner_namespace: &str,
        service_id: &str,
        account_id: &str,
    ) -> Result<Option<ActiveServiceHost>, ServiceHostError> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| ServiceHostError::Runtime("service host state is poisoned".to_owned()))?;
        let Some(retained) = state.retained.as_ref() else {
            return Ok(None);
        };
        if retained.owner_namespace != owner_namespace
            || retained.service_id != service_id
            || retained.account_id != account_id
        {
            return Ok(None);
        }
        let retained = state.retained.take().expect("retained host was present");
        state.phase = match state.resume_phase {
            ServiceHostPhase::Opening | ServiceHostPhase::Closed | ServiceHostPhase::Suspended => {
                ServiceHostPhase::Navigating
            }
            phase => phase,
        };
        state.active = Some(retained.clone());
        Ok(Some(retained))
    }
}

#[cfg(feature = "desktop")]
pub mod desktop {
    use super::*;
    use crate::services::{service_kind_from_id, ServiceRegistryState};
    use tauri::webview::{NewWindowResponse, PageLoadEvent, WebviewBuilder};
    use tauri::{
        AppHandle, LogicalPosition, LogicalSize, Manager, Position, Rect, Size, State, WebviewUrl,
    };

    const HOST_WEBVIEW_LABEL: &str = "service-host";
    const OVERLAY_WEBVIEW_LABEL: &str = "composer-overlay";
    const OVERLAY_ASSET: &str = "overlay.html";

    fn current_host_rect(
        main_window: &tauri::Window,
    ) -> Result<(LogicalPosition<u32>, LogicalSize<u32>), String> {
        let scale = main_window
            .scale_factor()
            .map_err(|error| error.to_string())?;
        let logical = main_window
            .inner_size()
            .map_err(|error| error.to_string())?
            .to_logical::<u32>(scale);
        let rect = host_rect(logical.width, logical.height);
        Ok((
            LogicalPosition::new(rect.x, rect.y),
            LogicalSize::new(rect.width, rect.height),
        ))
    }

    fn current_host_rect_with_local_sheet(
        main_window: &tauri::Window,
        open: bool,
    ) -> Result<(LogicalPosition<u32>, LogicalSize<u32>), String> {
        let scale = main_window
            .scale_factor()
            .map_err(|error| error.to_string())?;
        let logical = main_window
            .inner_size()
            .map_err(|error| error.to_string())?
            .to_logical::<u32>(scale);
        let rect = local_protected_sheet_rect(logical.width, logical.height, open);
        Ok((
            LogicalPosition::new(rect.x, rect.y),
            LogicalSize::new(rect.width, rect.height),
        ))
    }

    fn native_rect(position: LogicalPosition<u32>, size: LogicalSize<u32>) -> Rect {
        Rect {
            position: Position::Logical(LogicalPosition::new(
                f64::from(position.x),
                f64::from(position.y),
            )),
            size: Size::Logical(LogicalSize::new(
                f64::from(size.width),
                f64::from(size.height),
            )),
        }
    }

    fn close_overlay(app: &AppHandle) -> Result<(), String> {
        if let Some(overlay) = app.get_webview(OVERLAY_WEBVIEW_LABEL) {
            overlay.close().map_err(|_| {
                "The trusted composer overlay could not be closed safely.".to_owned()
            })?;
        }
        Ok(())
    }

    fn hide_overlay(app: &AppHandle) -> Result<(), String> {
        if let Some(overlay) = app.get_webview(OVERLAY_WEBVIEW_LABEL) {
            if overlay.hide().is_err() {
                overlay.close().map_err(|_| {
                    "The trusted composer overlay could not be hidden safely.".to_owned()
                })?;
            }
        }
        Ok(())
    }

    fn ensure_hidden_overlay(app: &AppHandle, main_window: &tauri::Window) -> Result<(), String> {
        if app.get_webview(OVERLAY_WEBVIEW_LABEL).is_some() {
            hide_overlay(app)?;
            return Ok(());
        }

        let builder = WebviewBuilder::new(
            OVERLAY_WEBVIEW_LABEL,
            WebviewUrl::App(PathBuf::from(OVERLAY_ASSET)),
        )
        .transparent(true)
        .focused(false)
        .devtools(false)
        .on_navigation(super::overlay_navigation_allowed)
        .on_new_window(|_, _| NewWindowResponse::Deny)
        .on_download(|_, _| false);
        let overlay = main_window
            .add_child(builder, LogicalPosition::new(0, 0), LogicalSize::new(1, 1))
            .map_err(|_| "The trusted composer overlay could not be created safely.".to_owned())?;
        overlay.hide().map_err(|_| {
            let _ = overlay.close();
            "The trusted composer overlay could not start hidden.".to_owned()
        })?;
        Ok(())
    }

    fn profile_base(app: &AppHandle) -> Result<PathBuf, String> {
        app.path()
            .app_local_data_dir()
            .map(|path| path.join(PROFILE_NAMESPACE))
            .map_err(|error| format!("could not resolve local app data directory: {error}"))
    }

    fn profile_root(app: &AppHandle, owner_osl_user_id: &str) -> Result<PathBuf, String> {
        let namespace = owner_profile_namespace(owner_osl_user_id).map_err(|e| e.to_string())?;
        Ok(profile_base(app)?.join(namespace))
    }

    pub fn scavenge_profile_tombstones_on_startup(
        app: &AppHandle,
    ) -> Result<TombstoneScavengeReport, String> {
        let base = profile_base(app)?;
        super::scavenge_owned_profile_tombstones(&base).map_err(|error| error.to_string())
    }

    fn record_failure(
        state: &ServiceHostState,
        generation: u64,
        code: ServiceHostFailureCode,
        message: &'static str,
        retryable: bool,
    ) -> String {
        let _ = state.fail(generation, code, message, retryable);
        message.to_owned()
    }

    pub async fn mutate_account_profile(
        app: AppHandle,
        state: State<'_, ServiceHostState>,
        registry: State<'_, ServiceRegistryState>,
        owner_osl_user_id: String,
        service_id: String,
        account_id: String,
        remove_registry: bool,
    ) -> Result<ServiceAccountMutation, String> {
        let _transition = state
            .transition
            .lock()
            .map_err(|_| "service host transition lock is poisoned".to_owned())?;
        service_manifest(&service_id).map_err(|error| error.to_string())?;
        validate_opaque_id(&account_id).map_err(|error| error.to_string())?;
        let service_kind =
            service_kind_from_id(&service_id).ok_or_else(|| "unknown service".to_owned())?;
        registry.require_owned(&owner_osl_user_id, service_kind, &account_id)?;

        let owner_namespace =
            owner_profile_namespace(&owner_osl_user_id).map_err(|error| error.to_string())?;
        let active = state
            .current_or_retained()
            .map_err(|error| error.to_string())?;
        let matching_active =
            active_profile_matches(active.as_ref(), &owner_namespace, &service_id, &account_id);
        if app.get_webview(HOST_WEBVIEW_LABEL).is_some() && active.is_none() {
            return Err(
                "The active service view identity cannot be proven; close it and retry.".to_owned(),
            );
        }
        if matching_active {
            close_overlay(&app)?;
            if let Some(webview) = app.get_webview(HOST_WEBVIEW_LABEL) {
                webview.close().map_err(|_| {
                    "The matching service view could not be closed safely.".to_owned()
                })?;
            }
            state.next_generation().map_err(|error| error.to_string())?;
        }

        let root = profile_root(&app, &owner_osl_user_id)?;
        let outcome = tombstone_profile_directory(&root, &service_id, &account_id)
            .map_err(|_| "The isolated account profile could not be reset safely.".to_owned())?;
        if remove_registry
            && !registry.remove_for_owner(&owner_osl_user_id, service_kind, &account_id)?
        {
            return Err("service account disappeared before removal completed".to_owned());
        }
        Ok(ServiceAccountMutation {
            service_id,
            account_id,
            profile_existed: outcome.profile_existed,
            cleanup_pending: outcome.cleanup_pending,
            registry_removed: remove_registry,
        })
    }

    pub async fn open(
        app: AppHandle,
        state: State<'_, ServiceHostState>,
        registry: State<'_, ServiceRegistryState>,
        owner_osl_user_id: String,
        service_id: String,
        account_id: String,
    ) -> Result<ActiveServiceHost, String> {
        let _transition = state
            .transition
            .lock()
            .map_err(|_| "service host transition lock is poisoned".to_owned())?;
        validate_opaque_id(&account_id).map_err(|error| error.to_string())?;
        let service_kind =
            service_kind_from_id(&service_id).ok_or_else(|| "unknown service".to_owned())?;
        registry.require_owned(&owner_osl_user_id, service_kind, &account_id)?;
        let owner_namespace =
            owner_profile_namespace(&owner_osl_user_id).map_err(|error| error.to_string())?;
        let provider =
            registry.email_provider_for_owner(&owner_osl_user_id, service_kind, &account_id)?;
        let manifest = service_manifest_for_provider(&service_id, provider)
            .map_err(|error| error.to_string())?;
        let initial_url = validated_initial_url(manifest).map_err(|error| error.to_string())?;
        let initial_host = initial_url
            .host_str()
            .ok_or_else(|| "service URL has no host".to_owned())?;

        if let Some(existing) = app.get_webview(HOST_WEBVIEW_LABEL) {
            let hosted = state
                .current_or_retained()
                .map_err(|error| error.to_string())?;
            if active_profile_matches(hosted.as_ref(), &owner_namespace, &service_id, &account_id) {
                let main_window = app
                    .get_window("main")
                    .ok_or_else(|| "The trusted OSL Privacy window is unavailable.".to_owned())?;
                let (position, size) = current_host_rect(&main_window)?;
                let resumed = state
                    .current()
                    .map_err(|error| error.to_string())?
                    .or(state
                        .resume(&owner_namespace, &service_id, &account_id)
                        .map_err(|error| error.to_string())?)
                    .ok_or_else(|| "The retained service view changed while opening.".to_owned())?;
                let reveal = existing
                    .set_bounds(native_rect(position, size))
                    .map_err(|_| {
                        "The retained service view could not be bounded safely.".to_owned()
                    })
                    .and_then(|_| {
                        existing.show().map_err(|_| {
                            "The retained service view could not be shown safely.".to_owned()
                        })
                    });
                if let Err(error) = reveal {
                    let _ = existing.hide();
                    let _ = state.suspend();
                    return Err(error);
                }
                let _ = existing.set_focus();
                return Ok(resumed);
            }

            if hosted.is_none() {
                return Err(
                    "The existing service view identity cannot be proven; close it and retry."
                        .to_owned(),
                );
            }
            close_overlay(&app)?;
            existing
                .close()
                .map_err(|_| "The previous service view could not be closed safely.".to_owned())?;
            state.next_generation().map_err(|error| error.to_string())?;
        }

        let pending = state
            .begin_open(&owner_namespace, &service_id, &account_id, initial_host)
            .map_err(|error| error.to_string())?;
        let generation = pending.generation;
        let root = profile_root(&app, &owner_osl_user_id).map_err(|_| {
            record_failure(
                &state,
                generation,
                ServiceHostFailureCode::ProfileUnavailable,
                "The local service profile directory is unavailable.",
                true,
            )
        })?;
        let profile = prepare_profile_directory(&root, &service_id, &account_id).map_err(|_| {
            record_failure(
                &state,
                generation,
                ServiceHostFailureCode::ProfileUnavailable,
                "The isolated account profile could not be prepared.",
                true,
            )
        })?;

        let main_window = app.get_window("main").ok_or_else(|| {
            record_failure(
                &state,
                generation,
                ServiceHostFailureCode::MainWindowUnavailable,
                "The trusted OSL Privacy window is unavailable.",
                true,
            )
        })?;
        let (position, size) = current_host_rect(&main_window).map_err(|_| {
            record_failure(
                &state,
                generation,
                ServiceHostFailureCode::LayoutFailed,
                "The service view bounds could not be calculated.",
                true,
            )
        })?;
        let navigation_app = app.clone();
        let page_load_app = app.clone();
        let builder = WebviewBuilder::new(HOST_WEBVIEW_LABEL, WebviewUrl::External(initial_url))
            .data_directory(profile)
            .devtools(false)
            .on_navigation(move |url| {
                let allowed = navigation_allowed(manifest, url);
                if !allowed {
                    let host_state = navigation_app.state::<ServiceHostState>();
                    let _ = host_state.navigation_blocked(generation);
                }
                allowed
            })
            .on_page_load(move |_, payload| {
                let host_state = page_load_app.state::<ServiceHostState>();
                if navigation_allowed(manifest, payload.url()) {
                    let finished = payload.event() == PageLoadEvent::Finished;
                    let _ = host_state.page_load(generation, payload.url().host_str(), finished);
                } else {
                    let _ = host_state.fail(
                        generation,
                        ServiceHostFailureCode::PageLoadFailed,
                        "The service page did not finish on an allowed HTTPS host.",
                        true,
                    );
                }
            })
            .on_new_window(|_, _| NewWindowResponse::Deny)
            .on_download(|_, _| false);

        let webview = main_window
            .add_child(builder, position, size)
            .map_err(|_| {
                record_failure(
                    &state,
                    generation,
                    ServiceHostFailureCode::WebviewCreationFailed,
                    "WebView2 could not create the isolated service view.",
                    true,
                )
            })?;
        webview.show().map_err(|_| {
            let _ = webview.close();
            record_failure(
                &state,
                generation,
                ServiceHostFailureCode::WebviewShowFailed,
                "The isolated service view was created but could not be shown.",
                true,
            )
        })?;
        // Focus is best-effort: failure must not destroy a visible, usable
        // service view. The user can focus it with an ordinary click.
        let _ = webview.set_focus();

        state
            .activate(&service_id, &account_id, generation)
            .map_err(|error| {
                if let Some(webview) = app.get_webview(HOST_WEBVIEW_LABEL) {
                    let _ = webview.close();
                }
                error.to_string()
            })
    }

    pub async fn close(app: AppHandle, state: State<'_, ServiceHostState>) -> Result<(), String> {
        let _transition = state
            .transition
            .lock()
            .map_err(|_| "service host transition lock is poisoned".to_owned())?;
        hide_overlay(&app)?;
        if let Some(webview) = app.get_webview(HOST_WEBVIEW_LABEL) {
            if state
                .current_or_retained()
                .map_err(|error| error.to_string())?
                .is_some()
            {
                webview
                    .hide()
                    .map_err(|_| "The service view could not be hidden safely.".to_owned())?;
                state.suspend().map_err(|error| error.to_string())?;
                return Ok(());
            }
            // Fail closed if a native child exists without a proven identity.
            webview
                .close()
                .map_err(|_| "The untracked service view could not be closed safely.".to_owned())?;
        }
        state.next_generation().map_err(|error| error.to_string())?;
        Ok(())
    }

    /// Permanently close any retained service surface and invalidate its
    /// generation. Identity switches, account burns, and full OSL Privacy cleanup use
    /// this stronger boundary instead of the ordinary fast suspend path.
    pub async fn shutdown(app: &AppHandle, state: &ServiceHostState) -> Result<(), String> {
        let _transition = state
            .transition
            .lock()
            .map_err(|_| "service host transition lock is poisoned".to_owned())?;
        let overlay_result = close_overlay(app);
        let host_result = if let Some(webview) = app.get_webview(HOST_WEBVIEW_LABEL) {
            webview
                .close()
                .map_err(|_| "The service view could not be closed safely".to_owned())
        } else {
            Ok(())
        };
        overlay_result?;
        host_result?;
        state.next_generation().map_err(|error| error.to_string())?;
        Ok(())
    }

    /// Focus only the exact retained/active hosted service surface. This is
    /// used immediately before an explicit trusted-UI placement action and
    /// grants no page script or native command capability.
    pub fn focus_active_host(
        app: &AppHandle,
        state: &ServiceHostState,
        expected: &ActiveServiceHost,
    ) -> Result<(), String> {
        let current = state
            .current()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "The service view is not active".to_owned())?;
        if &current != expected {
            return Err("The active service view changed before placement".to_owned());
        }
        app.get_webview(HOST_WEBVIEW_LABEL)
            .ok_or_else(|| "The hosted service surface is unavailable".to_owned())?
            .set_focus()
            .map_err(|_| "The hosted service surface could not be focused".to_owned())
    }

    pub async fn set_layout(
        app: AppHandle,
        state: State<'_, ServiceHostState>,
        protected: bool,
    ) -> Result<bool, String> {
        let _transition = state
            .transition
            .lock()
            .map_err(|_| "service host transition lock is poisoned".to_owned())?;
        let Some(webview) = app.get_webview(HOST_WEBVIEW_LABEL) else {
            return Ok(false);
        };
        let generation = state
            .current()
            .ok()
            .flatten()
            .map(|active| active.generation);
        let layout_failure = || {
            if let Some(generation) = generation {
                return record_failure(
                    &state,
                    generation,
                    ServiceHostFailureCode::LayoutFailed,
                    "The isolated service view could not be kept inside its trusted bounds.",
                    true,
                );
            }
            "The isolated service view could not be laid out.".to_owned()
        };
        let main_window = app.get_window("main").ok_or_else(layout_failure)?;
        let (position, size) = current_host_rect(&main_window).map_err(|_| layout_failure())?;
        webview
            .set_bounds(native_rect(position, size))
            .map_err(|_| layout_failure())?;
        if protected {
            // Protected mode alone is not semantic evidence of the native
            // composer. Instantiate the isolated surface above `service-host`
            // but keep it hidden and non-interactive until a future trusted-
            // bounds API supplies an adapter-verified composer rectangle.
            ensure_hidden_overlay(&app, &main_window).map_err(|_| layout_failure())?;
        } else {
            hide_overlay(&app).map_err(|_| layout_failure())?;
        }
        Ok(true)
    }

    /// Reveal or reclaim the fixed trusted-main-UI sheet area without allowing
    /// the caller to supply geometry. Only the capability-free `service-host`
    /// child is resized, and the exact active host generation is checked while
    /// the service-host transition lock is held.
    pub async fn set_local_protected_sheet_open(
        app: AppHandle,
        state: State<'_, ServiceHostState>,
        expected: ActiveServiceHost,
        open: bool,
    ) -> Result<bool, String> {
        let _transition = state
            .transition
            .lock()
            .map_err(|_| "service host transition lock is poisoned".to_owned())?;
        let current = state
            .current()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "The service view is not active".to_owned())?;
        if current != expected {
            return Err("The active service view changed before layout".to_owned());
        }
        let webview = app
            .get_webview(HOST_WEBVIEW_LABEL)
            .ok_or_else(|| "The hosted service surface is unavailable".to_owned())?;
        let main_window = app
            .get_window("main")
            .ok_or_else(|| "The trusted OSL Privacy window is unavailable".to_owned())?;
        let (position, size) = current_host_rect_with_local_sheet(&main_window, open)?;
        webview
            .set_bounds(native_rect(position, size))
            .map_err(|_| {
                "The service view could not make room for the protected sheet".to_owned()
            })?;
        if state.current().map_err(|error| error.to_string())?.as_ref() != Some(&expected) {
            return Err("The active service view changed during layout".to_owned());
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{mpsc, Arc, TryLockError};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn discord() -> &'static ServiceManifest {
        service_manifest("discord").unwrap()
    }

    #[test]
    fn opaque_ids_reject_path_syntax_and_ambiguous_forms() {
        for invalid in [
            "",
            ".",
            "..",
            "../rose",
            "rose/alt",
            "rose\\alt",
            "Rose",
            "rose_2",
            "rose.",
            "-rose",
            "rose-",
            "rose%2falt",
            "röse",
        ] {
            assert_eq!(
                validate_opaque_id(invalid),
                Err(ServiceHostError::InvalidOpaqueId),
                "accepted {invalid:?}"
            );
        }
        assert!(validate_opaque_id("rose-2").is_ok());
    }

    #[test]
    fn protected_layout_keeps_the_service_full_height_behind_the_overlay() {
        let rect = host_rect(1180, 780);
        let top_reserved = DESKTOP_TITLE_HEIGHT + TRUSTED_BAR_HEIGHT;
        assert_eq!(rect.x, 0);
        assert_eq!(rect.y, top_reserved);
        assert_eq!(rect.width, 1180);
        assert_eq!(rect.height, 780 - top_reserved);
    }

    #[test]
    fn local_protected_sheet_reserves_only_a_fixed_clamped_right_region() {
        let closed = local_protected_sheet_rect(1_180, 780, false);
        let open = local_protected_sheet_rect(1_180, 780, true);
        assert_eq!(closed, host_rect(1_180, 780));
        assert_eq!(open.x, closed.x);
        assert_eq!(open.y, closed.y);
        assert_eq!(open.height, closed.height);
        assert_eq!(open.width, closed.width - LOCAL_PROTECTED_SHEET_WIDTH);

        let compact = local_protected_sheet_rect(500, 700, true);
        assert_eq!(compact.width, MIN_SERVICE_HOST_WIDTH_WITH_SHEET);
        assert_eq!(compact.height, host_rect(500, 700).height);
    }

    #[test]
    fn active_host_owner_proof_rejects_other_owner_account_and_generation() {
        let state = ServiceHostState::default();
        let owner = "osl_owner_aaaaaaaaaaaaaaaa";
        let namespace = owner_profile_namespace(owner).unwrap();
        let active = state
            .begin_open(&namespace, "instagram", "account-one", "www.instagram.com")
            .unwrap();
        assert_eq!(
            state
                .require_current_owned(owner, "instagram", "account-one")
                .unwrap(),
            active
        );
        assert!(state
            .require_current_owned("osl_owner_bbbbbbbbbbbbbbbb", "instagram", "account-one")
            .is_err());
        assert!(state
            .require_current_owned(owner, "discord", "account-one")
            .is_err());
        assert!(state
            .require_current_owned(owner, "instagram", "account-two")
            .is_err());
        state.next_generation().unwrap();
        assert!(state
            .require_current_owned(owner, "instagram", "account-one")
            .is_err());
    }

    #[test]
    fn native_layout_uses_the_area_below_and_right_of_trusted_chrome() {
        let bundled_styles = include_str!("../../osl-hub-ui/src/styles.css");
        assert!(bundled_styles.contains("grid-template-rows: 44px minmax(0, 1fr);"));
        assert!(bundled_styles.contains("height: 54px;"));
        assert_eq!(DESKTOP_TITLE_HEIGHT, 44);
        assert_eq!(TRUSTED_BAR_HEIGHT, 54);
        let rect = host_rect(1180, 780);
        let top_reserved = DESKTOP_TITLE_HEIGHT + TRUSTED_BAR_HEIGHT;
        assert_eq!(rect.x, 0);
        assert_eq!(rect.y, top_reserved);
        assert_eq!(rect.width, 1180);
        assert_eq!(rect.height, 780 - top_reserved);
    }

    #[test]
    fn tiny_windows_never_underflow_trusted_reserves() {
        let rect = host_rect(40, 40);
        assert_eq!(rect.x, 0);
        assert_eq!(rect.y, DESKTOP_TITLE_HEIGHT + TRUSTED_BAR_HEIGHT);
        assert_eq!(rect.width, 40);
        assert_eq!(rect.height, 1);
    }

    #[test]
    fn overlay_bounds_require_an_explicit_safe_drawing_region() {
        let valid = HostRect {
            x: 100,
            y: 580,
            width: 760,
            height: 64,
        };
        assert_eq!(validate_trusted_overlay_rect(960, 680, valid), Some(valid));
        assert_eq!(
            validate_trusted_overlay_rect(960, 680, HostRect { y: 40, ..valid }),
            None
        );
        assert_eq!(
            validate_trusted_overlay_rect(960, 680, HostRect { x: 900, ..valid }),
            None
        );
        assert_eq!(
            validate_trusted_overlay_rect(
                960,
                680,
                HostRect {
                    width: MIN_TRUSTED_OVERLAY_WIDTH - 1,
                    ..valid
                }
            ),
            None
        );
    }

    #[test]
    fn exact_https_navigation_rejects_lookalikes_schemes_credentials_and_ports() {
        let allowed = [
            "https://discord.com/app",
            "https://discord.com/channels/@me",
        ];
        for candidate in allowed {
            assert!(navigation_allowed(
                discord(),
                &Url::parse(candidate).unwrap()
            ));
        }

        let denied = [
            "http://discord.com/app",
            "https://evil.example/discord.com",
            "https://discord.com.evil.example/app",
            "https://sub.discord.com/app",
            "https://discоrd.com/app",
            "https://discord.com:444/app",
            "https://user@discord.com/app",
            "file:///discord.com/app",
            "javascript:alert(1)",
        ];
        for candidate in denied {
            assert!(
                !navigation_allowed(discord(), &Url::parse(candidate).unwrap()),
                "allowed {candidate:?}"
            );
        }
    }

    #[test]
    fn trusted_overlay_navigation_accepts_only_its_bundled_local_page() {
        for allowed in [
            "tauri://localhost/overlay.html",
            "http://tauri.localhost/overlay.html",
        ] {
            assert!(overlay_navigation_allowed(&Url::parse(allowed).unwrap()));
        }
        for denied in [
            "https://tauri.localhost/overlay.html",
            "http://tauri.localhost/",
            "http://tauri.localhost/overlay.html?remote=1",
            "http://evil.example/overlay.html",
            "data:text/html,overlay",
        ] {
            assert!(!overlay_navigation_allowed(&Url::parse(denied).unwrap()));
        }
    }

    #[test]
    fn every_active_initial_url_is_allowed_and_unavailable_services_never_launch() {
        for manifest in SERVICES {
            if manifest.launch_active {
                assert!(validated_initial_url(manifest).is_ok(), "{}", manifest.id);
            } else {
                assert_eq!(
                    validated_initial_url(manifest),
                    Err(ServiceHostError::ServiceUnavailable),
                    "{}",
                    manifest.id
                );
            }
        }
        let snapchat = service_manifest("snapchat").unwrap();
        assert_eq!(snapchat.initial_url, "https://www.snapchat.com/web/");
        assert!(snapchat.allowed_hosts.contains(&"accounts.snapchat.com"));
    }

    #[test]
    fn email_providers_use_only_fixed_exact_https_origins() {
        let cases = [
            (EmailProvider::Gmail, "mail.google.com"),
            (EmailProvider::Outlook, "outlook.live.com"),
            (EmailProvider::Proton, "mail.proton.me"),
            (EmailProvider::Tuta, "app.tuta.com"),
            (EmailProvider::Fastmail, "app.fastmail.com"),
            (EmailProvider::Yahoo, "mail.yahoo.com"),
            (EmailProvider::Zoho, "mail.zoho.com"),
            (EmailProvider::Aol, "mail.aol.com"),
            (EmailProvider::Gmx, "www.gmx.com"),
            (EmailProvider::Maildotcom, "www.mail.com"),
        ];
        for (provider, expected_host) in cases {
            let manifest = service_manifest_for_provider("email", Some(provider)).unwrap();
            let initial = validated_initial_url(manifest).unwrap();
            assert_eq!(initial.scheme(), "https");
            assert_eq!(initial.host_str(), Some(expected_host));
            assert!(navigation_allowed(manifest, &initial));
            assert!(!navigation_allowed(
                manifest,
                &Url::parse(&format!("https://{expected_host}.evil.example/")).unwrap()
            ));
        }
    }

    #[test]
    fn status_is_bounded_and_stale_page_callbacks_cannot_replace_current_state() {
        let state = ServiceHostState::default();
        let first = state
            .begin_open("owner-a", "discord", "rose", "discord.com")
            .unwrap();
        assert_eq!(state.status().unwrap().phase, ServiceHostPhase::Opening);
        state
            .page_load(first.generation, Some("discord.com"), false)
            .unwrap();
        assert_eq!(state.status().unwrap().phase, ServiceHostPhase::Navigating);
        state
            .page_load(first.generation, Some("discord.com"), true)
            .unwrap();
        assert_eq!(
            state.status().unwrap().phase,
            ServiceHostPhase::DocumentReady
        );

        let second = state
            .begin_open("owner-a", "instagram", "osl", "www.instagram.com")
            .unwrap();
        state
            .page_load(first.generation, Some("discord.com"), true)
            .unwrap();
        let current = state.status().unwrap();
        assert_eq!(current.phase, ServiceHostPhase::Opening);
        assert_eq!(current.active.unwrap().generation, second.generation);
        assert_eq!(current.host.as_deref(), Some("www.instagram.com"));

        state.navigation_blocked(second.generation).unwrap();
        let blocked = state.status().unwrap();
        assert_eq!(blocked.phase, ServiceHostPhase::NavigationBlocked);
        let error = blocked.last_error.unwrap();
        assert_eq!(error.code, ServiceHostFailureCode::NavigationBlocked);
        assert!(error.message.len() < 160);
        assert_eq!(blocked.blocked_navigation_count, 1);
    }

    #[test]
    fn suspended_account_resumes_same_generation_and_current_document() {
        let state = ServiceHostState::default();
        let opened = state
            .begin_open("owner-a", "telegram", "acct-rose", "web.telegram.org")
            .unwrap();
        state
            .page_load(opened.generation, Some("web.telegram.org"), true)
            .unwrap();
        state
            .activate("telegram", "acct-rose", opened.generation)
            .unwrap();
        assert_eq!(state.suspend().unwrap(), Some(opened.clone()));
        assert_eq!(state.status().unwrap().phase, ServiceHostPhase::Suspended);
        assert!(state.current().unwrap().is_none());
        assert_eq!(state.current_or_retained().unwrap(), Some(opened.clone()));

        let resumed = state
            .resume("owner-a", "telegram", "acct-rose")
            .unwrap()
            .unwrap();
        assert_eq!(resumed, opened);
        assert_eq!(
            state.status().unwrap().phase,
            ServiceHostPhase::DocumentReady
        );
        assert!(state
            .resume("owner-a", "telegram", "another-account")
            .unwrap()
            .is_none());
    }

    #[test]
    fn profile_paths_are_opaque_and_contained() {
        let root = Path::new("/tmp/osl-profiles");
        let path = profile_path(root, "instagram", "rose-2").unwrap();
        assert!(path.starts_with(root));
        assert_eq!(path, root.join("service-instagram").join("account-rose-2"));
        assert!(profile_path(root, "instagram", "../../escape").is_err());
        assert!(profile_path(root, "unknown", "rose").is_err());
    }

    #[test]
    fn owner_namespaces_are_stable_collision_resistant_and_hide_flat_profiles() {
        let base = std::env::temp_dir().join(format!(
            "osl-host-owner-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let owner_a = owner_profile_namespace("osl_owner_a").unwrap();
        let owner_b = owner_profile_namespace("osl_owner_b").unwrap();
        assert_eq!(owner_a, owner_profile_namespace("osl_owner_a").unwrap());
        assert_ne!(owner_a, owner_b);
        assert!(!owner_a.contains("osl_owner_a"));

        let profile_a =
            prepare_profile_directory(&base.join(&owner_a), "discord", "acct-shared").unwrap();
        let profile_b =
            prepare_profile_directory(&base.join(&owner_b), "discord", "acct-shared").unwrap();
        assert_ne!(profile_a, profile_b);

        // A pre-ownership flat directory is not scanned, moved, or claimed.
        let flat = base.join("discord").join("acct-shared");
        fs::create_dir_all(&flat).unwrap();
        assert_eq!(
            scavenge_owned_profile_tombstones(&base).unwrap(),
            TombstoneScavengeReport::default()
        );
        assert!(flat.exists());
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn prepared_profile_is_canonically_inside_root() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("osl-host-test-{nonce}"));
        let profile = prepare_profile_directory(&root, "telegram", "rose").unwrap();
        assert!(profile.starts_with(fs::canonicalize(&root).unwrap()));
        assert!(profile.is_dir());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn preparing_existing_profiles_preserves_session_state_and_account_isolation() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("osl-host-persist-test-{nonce}"));
        let rose = prepare_profile_directory(&root, "telegram", "acct-rose").unwrap();
        let osl = prepare_profile_directory(&root, "telegram", "acct-osl").unwrap();
        assert_ne!(rose, osl);
        fs::write(
            rose.join("webview-cookie-store.fixture"),
            b"persistent state",
        )
        .unwrap();

        let reopened = prepare_profile_directory(&root, "telegram", "acct-rose").unwrap();
        assert_eq!(reopened, rose);
        assert_eq!(
            fs::read(reopened.join("webview-cookie-store.fixture")).unwrap(),
            b"persistent state"
        );
        assert!(!osl.join("webview-cookie-store.fixture").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reset_tombstones_exact_profile_then_cleans_inside_root() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("osl-host-reset-test-{nonce}"));
        let profile = prepare_profile_directory(&root, "discord", "acct-rose").unwrap();
        fs::write(profile.join("webview-state.bin"), b"local test state").unwrap();

        let outcome = tombstone_profile_directory(&root, "discord", "acct-rose").unwrap();
        assert_eq!(
            outcome,
            ProfileResetOutcome {
                profile_existed: true,
                cleanup_pending: false,
            }
        );
        assert!(!profile.exists());
        assert!(fs::read_dir(&root).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".deleted-")));

        let second = tombstone_profile_directory(&root, "discord", "acct-rose").unwrap();
        assert_eq!(second, ProfileResetOutcome::default());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_scavenger_removes_only_bounded_in_root_tombstones() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("osl-host-scavenge-test-{nonce}"));
        fs::create_dir_all(&root).unwrap();
        let tombstone = root.join(".deleted-discord-acct-rose-1-a-1");
        let live = root.join("service-discord");
        let unrelated = root.join("keep-me");
        fs::create_dir_all(&tombstone).unwrap();
        fs::create_dir_all(&live).unwrap();
        fs::write(&unrelated, b"not profile state").unwrap();

        assert_eq!(
            scavenge_profile_tombstones(&root).unwrap(),
            TombstoneScavengeReport {
                removed: 1,
                cleanup_pending: 0,
            }
        );
        assert!(!tombstone.exists());
        assert!(live.exists());
        assert!(unrelated.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reset_rejects_unknown_services_and_traversal_ids() {
        let root = std::env::temp_dir().join("osl-host-reset-invalid-test");
        assert_eq!(
            tombstone_profile_directory(&root, "unknown", "acct-rose"),
            Err(ServiceHostError::UnknownService)
        );
        assert_eq!(
            tombstone_profile_directory(&root, "discord", "../rose"),
            Err(ServiceHostError::InvalidOpaqueId)
        );
    }

    #[test]
    fn matching_host_close_scope_requires_exact_service_and_account() {
        let active = ActiveServiceHost {
            service_id: "discord".to_owned(),
            account_id: "acct-rose".to_owned(),
            generation: 7,
            owner_namespace: "owner-a".to_owned(),
        };
        assert!(active_profile_matches(
            Some(&active),
            "owner-a",
            "discord",
            "acct-rose"
        ));
        assert!(!active_profile_matches(
            Some(&active),
            "owner-a",
            "instagram",
            "acct-rose"
        ));
        assert!(!active_profile_matches(
            Some(&active),
            "owner-a",
            "discord",
            "acct-osl"
        ));
        assert!(!active_profile_matches(
            Some(&active),
            "owner-b",
            "discord",
            "acct-rose"
        ));
        assert!(!active_profile_matches(
            None,
            "owner-a",
            "discord",
            "acct-rose"
        ));
    }

    #[cfg(unix)]
    #[test]
    fn prepared_profile_rejects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("osl-host-link-test-{nonce}"));
        let outside = std::env::temp_dir().join(format!("osl-host-outside-{nonce}"));
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        symlink(&outside, root.join("service-discord")).unwrap();

        assert_eq!(
            prepare_profile_directory(&root, "discord", "rose"),
            Err(ServiceHostError::UnsafeProfilePath)
        );
        fs::remove_file(root.join("service-discord")).unwrap();
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn reset_never_follows_a_profile_symlink() {
        use std::os::unix::fs::symlink;

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("osl-host-reset-link-{nonce}"));
        let outside = std::env::temp_dir().join(format!("osl-host-reset-outside-{nonce}"));
        fs::create_dir_all(root.join("service-discord")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        symlink(&outside, root.join("service-discord/account-acct-rose")).unwrap();

        assert_eq!(
            tombstone_profile_directory(&root, "discord", "acct-rose"),
            Err(ServiceHostError::UnsafeProfilePath)
        );
        assert!(outside.exists());
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn reset_never_accepts_a_symlinked_profile_root() {
        use std::os::unix::fs::symlink;

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("osl-host-root-link-{nonce}"));
        let outside = std::env::temp_dir().join(format!("osl-host-root-outside-{nonce}"));
        fs::create_dir_all(&outside).unwrap();
        symlink(&outside, &root).unwrap();

        assert_eq!(
            tombstone_profile_directory(&root, "discord", "acct-rose"),
            Err(ServiceHostError::UnsafeProfilePath)
        );
        assert_eq!(
            prepare_profile_directory(&root, "discord", "acct-rose"),
            Err(ServiceHostError::UnsafeProfilePath)
        );
        fs::remove_file(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }

    #[test]
    fn generations_replace_and_close_without_stale_activation() {
        let state = ServiceHostState::default();
        let first = state
            .begin_open("owner-a", "discord", "rose", "discord.com")
            .unwrap()
            .generation;
        state.activate("discord", "rose", first).unwrap();
        assert_eq!(state.current().unwrap().unwrap().account_id, "rose");

        let second = state
            .begin_open("owner-a", "discord", "osl", "discord.com")
            .unwrap()
            .generation;
        assert!(state.activate("discord", "rose", first).is_err());
        state.activate("discord", "osl", second).unwrap();
        assert_eq!(state.current().unwrap().unwrap().account_id, "osl");

        let close_generation = state.next_generation().unwrap();
        assert!(close_generation > second);
        assert!(state.current().unwrap().is_none());
    }

    #[test]
    fn transition_lock_serializes_concurrent_profile_switches() {
        let state = Arc::new(ServiceHostState::default());
        let worker_state = Arc::clone(&state);
        let (locked_tx, locked_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();

        let worker = std::thread::spawn(move || {
            let _transition = worker_state.transition.lock().unwrap();
            let generation = worker_state
                .begin_open("owner-a", "discord", "rose", "discord.com")
                .unwrap()
                .generation;
            locked_tx.send(generation).unwrap();
            release_rx.recv().unwrap();
            worker_state
                .activate("discord", "rose", generation)
                .unwrap();
        });

        let first = locked_rx.recv().unwrap();
        assert!(matches!(
            state.transition.try_lock(),
            Err(TryLockError::WouldBlock)
        ));
        release_tx.send(()).unwrap();
        worker.join().unwrap();

        let _transition = state.transition.lock().unwrap();
        let second = state
            .begin_open("owner-a", "discord", "osl", "discord.com")
            .unwrap()
            .generation;
        assert!(second > first);
        state.activate("discord", "osl", second).unwrap();
        assert_eq!(state.current().unwrap().unwrap().account_id, "osl");
    }
}
