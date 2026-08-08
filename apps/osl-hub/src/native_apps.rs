//! Fixed Windows-native service launch boundary.
//!
//! The trusted UI can select only one of the enum variants below. It cannot
//! supply an executable, path, URI, command-line option, or package source.
//! This keeps the launcher useful without turning a Tauri command into a
//! general process-execution primitive.

use adapter_profile::{AdapterService, AdapterSurface, SupportLevel};
use serde::{Deserialize, Serialize};

use crate::windows_executable_trust::ExecutablePublisher;
#[cfg(target_os = "windows")]
use crate::windows_executable_trust::{verify_executable, TrustedExecutable};

#[cfg(target_os = "windows")]
use std::os::windows::ffi::OsStringExt;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
#[cfg(any(target_os = "windows", test))]
use std::path::{Path, PathBuf};
#[cfg(target_os = "windows")]
use std::process::{Child, Command, Output, Stdio};
#[cfg(any(target_os = "windows", test))]
use std::sync::Mutex;
#[cfg(target_os = "windows")]
use std::sync::OnceLock;
#[cfg(target_os = "windows")]
use std::thread;
#[cfg(any(target_os = "windows", test))]
use std::time::{Duration, Instant};
#[cfg(target_os = "windows")]
use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NativeAppId {
    Discord,
    Telegram,
    Signal,
    Whatsapp,
    Outlook,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BrowserImportId {
    Chrome,
    Edge,
    Firefox,
    Brave,
    Opera,
    DuckDuckGo,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserImportStatus {
    pub id: BrowserImportId,
    pub display_name: &'static str,
    pub installed: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserImportResult {
    pub id: BrowserImportId,
    pub opened: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserAccountImportResult {
    pub preferred_source: BrowserImportId,
    pub detected_sources: Vec<BrowserImportId>,
    pub opened: bool,
    pub mode: &'static str,
    pub manual_export_required: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtectedBrowserImportResult {
    pub selected_sources: Vec<BrowserImportId>,
    pub started: bool,
    pub mode: &'static str,
    pub source_selected: bool,
    pub manual_fallback: Option<String>,
}

#[cfg(target_os = "windows")]
fn protected_browser_import_process() -> &'static Mutex<Option<Child>> {
    static PROCESS: OnceLock<Mutex<Option<Child>>> = OnceLock::new();
    PROCESS.get_or_init(|| Mutex::new(None))
}

#[cfg(target_os = "windows")]
fn close_protected_browser_import_process() -> Result<(), String> {
    let mut process = protected_browser_import_process()
        .lock()
        .map_err(|_| "The OSL Firefox import process state is unavailable".to_owned())?;
    let Some(mut child) = process.take() else {
        return Ok(());
    };
    match child.try_wait() {
        Ok(Some(_)) => Ok(()),
        Ok(None) => {
            crate::firefox_migration_coordinator::close(child.id())?;
            let root_process_id = child.id();
            let deadline = Instant::now() + Duration::from_secs(1);
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => return Ok(()),
                    Ok(None) if Instant::now() < deadline => {
                        thread::sleep(Duration::from_millis(50));
                    }
                    Ok(None) => {
                        child.kill().map_err(|_| {
                            "The OSL Firefox import window could not be closed".to_owned()
                        })?;
                        child.wait().map_err(|_| {
                            "The OSL Firefox import process could not be reaped".to_owned()
                        })?;
                        thread::sleep(Duration::from_millis(200));
                        return crate::firefox_migration_coordinator::is_closed(root_process_id);
                    }
                    Err(_) => {
                        return Err(
                            "The OSL Firefox import process could not be verified".to_owned()
                        )
                    }
                }
            }
        }
        Err(_) => Err("The OSL Firefox import process could not be verified".to_owned()),
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeAppStatus {
    pub id: NativeAppId,
    pub display_name: &'static str,
    /// Installed means the fixed, reviewed native client can be launched. It is
    /// not evidence that OSL Protected mode is available for that service.
    pub availability: NativeAppAvailability,
    /// Public claim state for OSL's support of this native service. This is
    /// deliberately separate from `availability` so a detected app does not
    /// become a product support claim.
    ///
    /// Derived from [`crate::claim_state`]; it cannot be set directly.
    pub support_status: NativeAppSupportStatus,
    /// **Why the label is what it is.** What has been measured about this
    /// service's carrier path — built and never proven, measured and refused,
    /// never built at all. Two services can share a badge and be in different
    /// states, and shipping only the badge is how those two states collapse.
    pub carrier_evidence: &'static str,
    /// Whether a message can actually reach a person on this surface.
    pub delivery_evidence: &'static str,
    /// Governance conditions standing on the row, if any.
    pub claim_blockers: Vec<&'static str>,
    /// **The sentence shown to the user.** One line, no promise. `PLAN.md` r4-6:
    /// every capability the app does not have says so plainly, in a label the
    /// allowlist permits.
    pub claim_note: &'static str,
    /// Direct status-page data, derived from the same claim-state row that
    /// creates `support_status`. The page must not invent a second label or a
    /// second capability explanation.
    pub status_page: NativeAppStatusPageData,
    /// The strongest protected-mode handoff the public UI may offer today.
    pub protected_mode: NativeAppProtectedMode,
    /// True only when the current integration has a verified secondary-instance
    /// switch that keeps writable state inside an OSL-owned profile.
    pub isolated_profile_available: bool,
    /// Remains false until a service-specific Windows accessibility adapter
    /// can prove the exact account, conversation, recipients, and composer.
    pub supports_overlay: bool,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeAppStatusPageData {
    pub capability: &'static str,
    pub generated_label: &'static str,
    pub explanation: &'static str,
}

impl From<crate::claim_state::StatusPageData> for NativeAppStatusPageData {
    fn from(data: crate::claim_state::StatusPageData) -> Self {
        Self {
            capability: data.capability,
            generated_label: data.generated_label,
            explanation: data.explanation,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NativeAppAvailability {
    Installed,
    Installable,
    Unavailable,
}

/// The public claim OSL makes about a native service, and nothing else.
///
/// **This enum is no longer written anywhere.** It is the wire form of
/// [`crate::claim_state::PublicClaim`], which is *derived* from what was
/// measured — see `claim_state`'s module docs for the gate this closes
/// (`PLAN.md` r4-5, "the claim-state gap").
///
/// It gained two variants because two real states had no label:
///
/// * `Experimental` — an adapter is wired and has **never been proven against a
///   live provider**. D-206 held Telegram at `ComingSoon` for want of exactly
///   this: *"Held until a status that renders as `Experimental` exists."*
///   Permitted for these services by `osl-public-claim-allowlist.md` §B
///   ("Must carry `Coming soon`, `Experimental`, or `Externally blocked`") and
///   absent from `check-app-claims.mjs` `FORBIDDEN_PROMOTION_STATUSES`.
/// * `NoClaim` — allowlist §E maps `open-security-finding` and
///   `unknown-recheck-required` to **no badge, no claim**. Both stand on Discord
///   at once (D-203), which is why every one of the old three labels was false.
///
/// `Available` exists so the mapping from `PublicClaim` is total. Nothing in
/// this product reaches it, and `claim_state::tests::nothing_reaches_available`
/// holds that open rather than deleting the variant.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NativeAppSupportStatus {
    Available,
    Beta,
    Experimental,
    ComingSoon,
    ExternallyBlocked,
    NoClaim,
}

impl From<crate::claim_state::PublicClaim> for NativeAppSupportStatus {
    fn from(claim: crate::claim_state::PublicClaim) -> Self {
        use crate::claim_state::PublicClaim;
        match claim {
            PublicClaim::Available => Self::Available,
            PublicClaim::Beta => Self::Beta,
            PublicClaim::Experimental => Self::Experimental,
            PublicClaim::Planned => Self::ComingSoon,
            PublicClaim::ExternallyBlocked => Self::ExternallyBlocked,
            PublicClaim::NoClaim => Self::NoClaim,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NativeAppProtectedMode {
    AssistOnly,
    Unavailable,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeInstallResult {
    pub id: NativeAppId,
    pub started: bool,
    pub package_id: &'static str,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MullvadStatus {
    pub availability: NativeAppAvailability,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MullvadActionResult {
    pub started: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FirefoxServiceId {
    Messenger,
    Gmail,
    Outlook,
    Proton,
    Tuta,
    Yahoo,
    Aol,
    Gmx,
    Maildotcom,
    Icloud,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirefoxStatus {
    pub availability: NativeAppAvailability,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirefoxLaunchResult {
    pub service_id: FirefoxServiceId,
    pub started: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirefoxInstallResult {
    pub started: bool,
    pub package_id: &'static str,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum KnownFolder {
    Local,
    Roaming,
    ProgramFiles,
    ProgramFilesX86,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct ExecutableCandidate {
    folder: KnownFolder,
    relative_path: &'static str,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct NativeAppManifest {
    id: NativeAppId,
    display_name: &'static str,
    adapter_service: AdapterService,
    adapter_surface: AdapterSurface,
    adapter_support: SupportLevel,
    package_id: &'static str,
    package_source: &'static str,
    candidates: &'static [ExecutableCandidate],
    publisher: Option<ExecutablePublisher>,
    store_package_family_name: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg(any(target_os = "windows", test))]
struct BrowserImportManifest {
    id: BrowserImportId,
    display_name: &'static str,
    candidates: &'static [ExecutableCandidate],
    import_arguments: &'static [&'static str],
    publisher_attestation: BrowserPublisherAttestation,
}

/// A browser becomes launchable only after its installed executable's exact
/// Authenticode leaf organization has been observed and reviewed.  Keeping the
/// pending package identity here makes the remaining attestation bounded
/// without guessing a certificate subject from winget's display metadata.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg(any(target_os = "windows", test))]
enum BrowserPublisherAttestation {
    Verified(ExecutablePublisher),
}

const DISCORD_CANDIDATES: &[ExecutableCandidate] = &[
    ExecutableCandidate {
        folder: KnownFolder::Local,
        relative_path: r"Discord\Update.exe",
    },
    ExecutableCandidate {
        folder: KnownFolder::Local,
        relative_path: r"DiscordPTB\Update.exe",
    },
    ExecutableCandidate {
        folder: KnownFolder::Local,
        relative_path: r"DiscordCanary\Update.exe",
    },
];

const TELEGRAM_CANDIDATES: &[ExecutableCandidate] = &[
    ExecutableCandidate {
        folder: KnownFolder::Roaming,
        relative_path: r"Telegram Desktop\Telegram.exe",
    },
    ExecutableCandidate {
        folder: KnownFolder::Local,
        relative_path: r"Programs\Telegram Desktop\Telegram.exe",
    },
];

const SIGNAL_CANDIDATES: &[ExecutableCandidate] = &[ExecutableCandidate {
    folder: KnownFolder::Local,
    relative_path: r"Programs\signal-desktop\Signal.exe",
}];

const WHATSAPP_CANDIDATES: &[ExecutableCandidate] = &[ExecutableCandidate {
    folder: KnownFolder::Local,
    relative_path: r"WhatsApp\WhatsApp.exe",
}];

// Classic Outlook is a signed Win32 desktop application. Restrict discovery
// to Microsoft's documented Click-to-Run Office16 layout; never use the
// user-writable App Paths registry or an executable-name search.
const OUTLOOK_CLASSIC_CANDIDATES: &[ExecutableCandidate] = &[
    ExecutableCandidate {
        folder: KnownFolder::ProgramFiles,
        relative_path: r"Microsoft Office\root\Office16\OUTLOOK.EXE",
    },
    ExecutableCandidate {
        folder: KnownFolder::ProgramFilesX86,
        relative_path: r"Microsoft Office\root\Office16\OUTLOOK.EXE",
    },
];

#[cfg(any(target_os = "windows", test))]
const OUTLOOK_PACKAGE_NAME: &str = "Microsoft.OutlookForWindows";
#[cfg(any(target_os = "windows", test))]
const OUTLOOK_PACKAGE_PUBLISHER_ID: &str = "8wekyb3d8bbwe";
#[cfg(any(target_os = "windows", test))]
const OUTLOOK_PACKAGE_FAMILY_NAME: &str = "Microsoft.OutlookForWindows_8wekyb3d8bbwe";
#[cfg(any(target_os = "windows", test))]
pub(crate) const OUTLOOK_PACKAGE_AUMID: &str = concat!(
    "shell:AppsFolder\\Microsoft.OutlookForWindows_8wekyb3d8bbwe!",
    "Microsoft.OutlookforWindows"
);

#[cfg(any(target_os = "windows", test))]
const WHATSAPP_PACKAGE_NAME: &str = "5319275A.WhatsAppDesktop";

#[cfg(any(target_os = "windows", test))]
const WHATSAPP_PACKAGE_PUBLISHER_ID: &str = "cv1g1gvanyjgm";

pub(crate) const WHATSAPP_PACKAGE_FAMILY_NAME: &str = "5319275A.WhatsAppDesktop_cv1g1gvanyjgm";

#[cfg(target_os = "windows")]
const MAX_WHATSAPP_PACKAGE_COUNT: u32 = 32;

#[cfg(target_os = "windows")]
const MAX_WHATSAPP_PACKAGE_BUFFER_UNITS: u32 = 32_768;

#[cfg(target_os = "windows")]
const MAX_WHATSAPP_PACKAGE_ID_BYTES: u32 = 16_384;

#[cfg(any(target_os = "windows", test))]
const WINDOWS_APPS_DIRECTORY: &str = "WindowsApps";

#[cfg(any(target_os = "windows", test))]
const DESKTOP_APP_INSTALLER_PREFIX: &str = "microsoft.desktopappinstaller_";

#[cfg(any(target_os = "windows", test))]
const DESKTOP_APP_INSTALLER_PUBLISHER_ID: &str = "_8wekyb3d8bbwe";

#[cfg(any(target_os = "windows", test))]
const DESKTOP_APP_INSTALLER_FAMILY_NAME: &str = "Microsoft.DesktopAppInstaller_8wekyb3d8bbwe";

#[cfg(any(target_os = "windows", test))]
const INSTALLER_FAILURE_RETRY_DELAY: Duration = Duration::from_millis(250);

#[cfg(target_os = "windows")]
// A cold Windows PowerShell/AppX query can exceed three seconds on small VMs
// or immediately after sign-in. Keep this below the setup UI's eight-second
// decision budget so native availability remains truthful without blocking
// startup.
const APP_INSTALLER_PROBE_TIMEOUT: Duration = Duration::from_secs(6);

/// Windows PowerShell's redirected text encoding varies by host/version. Write
/// the AppX location as explicit UTF-8 bytes so Rust never has to guess.
#[cfg(any(target_os = "windows", test))]
const APP_INSTALLER_LOCATION_SCRIPT: &str = concat!(
    "$location = Get-AppxPackage -Name Microsoft.DesktopAppInstaller ",
    "-ErrorAction SilentlyContinue | Select-Object -First 1 ",
    "-ExpandProperty InstallLocation; ",
    "if (-not [string]::IsNullOrWhiteSpace([string]$location)) { ",
    "$utf8 = [System.Text.UTF8Encoding]::new($false); ",
    "$bytes = $utf8.GetBytes([string]$location); ",
    "$stdout = [Console]::OpenStandardOutput(); ",
    "$stdout.Write($bytes, 0, $bytes.Length); $stdout.Flush() }"
);

#[cfg(any(target_os = "windows", test))]
#[derive(Debug, Default)]
struct InstallerAvailabilityCache {
    verified: bool,
    retry_after_failure: Option<Instant>,
}

#[cfg(target_os = "windows")]
static VERIFIED_INSTALLER_AVAILABLE: Mutex<InstallerAvailabilityCache> =
    Mutex::new(InstallerAvailabilityCache {
        verified: false,
        retry_after_failure: None,
    });

#[cfg(any(target_os = "windows", test))]
const MULLVAD_PACKAGE_ID: &str = "MullvadVPN.MullvadVPN";
#[cfg(any(target_os = "windows", test))]
const DISCORD_DEDICATED_PACKAGE_ID: &str = "Discord.Discord.PTB";

#[cfg(any(target_os = "windows", test))]
const MULLVAD_CANDIDATES: &[ExecutableCandidate] = &[
    ExecutableCandidate {
        folder: KnownFolder::ProgramFiles,
        relative_path: r"Mullvad VPN\Mullvad VPN.exe",
    },
    ExecutableCandidate {
        folder: KnownFolder::Local,
        relative_path: r"Programs\Mullvad VPN\Mullvad VPN.exe",
    },
];

const NATIVE_APPS: &[NativeAppManifest] = &[
    NativeAppManifest {
        id: NativeAppId::Discord,
        display_name: "Discord",
        adapter_service: AdapterService::Discord,
        adapter_surface: AdapterSurface::InstalledNativeClient,
        // D-203, ESCALATED TO THE OWNER 2026-08-04 — VALUE UNCHANGED ON PURPOSE.
        //
        // This value is WRONG and there is no right one to move it to. It is an
        // overclaim: `native_app_support_status` (below) maps `Experimental` to
        // `NativeAppSupportStatus::Beta`, and `osl-public-claim-allowlist.md:271-272`
        // reserves `Beta` for `runtime-proven` / `test-proven-only` rows. Discord is
        // neither. Master §9 (`osl-master-decision-2026-07-26.md:793`) records Discord
        // protected send as "`verified-live` on dated QA builds; current tree recheck
        // required", with the C4 harness `blocked`/inadmissible — which allowlist rule 5
        // (`:24-26`) makes `unknown-recheck-required`, and §E (`:277`) maps to
        // **no badge, no claim**. Independently, `support-matrix.json` carries D6 as
        // `open-security-finding`, and §E (`:282-283`) says that outranks everything
        // else on the row however well built the feature is.
        //
        // `support-matrix.json` (`unavailable` / `not-qualified`) is therefore RIGHT,
        // not stale, and must not be aligned upward to this value: `beta` is in
        // `check-app-claims.mjs:340-347` `FORBIDDEN_PROMOTION_STATUSES`, so writing it
        // into the matrix is a claim-gate floor failure, measured.
        //
        // But the other two values are false, not merely conservative:
        //   `ComingSoon`    -> master §8.2 `Planned`. Discord is not planned; it is the
        //                      only carrier `main.ts:735` enables today, and this same
        //                      manifest gives it `NativeAppProtectedMode::AssistOnly`.
        //   `ExternallyBlocked` -> "depends on an unavailable third-party surface"
        //                      (master §0.3). Nothing external blocks Discord; D-205
        //                      resolved its live composer at 558 elements.
        //
        // So the enum cannot express Discord's actual state, and even the claim the
        // allowlist DOES permit for the send path (`:218`, `runtime-proven` -> `Beta`)
        // is inexpressible here, because allowlist rule 3 (`:20-21`) requires the
        // limitation to ship with the claim and this is a bare three-valued enum with
        // nowhere to carry "Verified on QA builds, not yet on the release build."
        //
        // Same product gap as Telegram's, from the opposite direction. Owner decision.
        // Do not resolve this by editing this line or the matrix row.
        adapter_support: SupportLevel::Experimental,
        package_id: "Discord.Discord",
        package_source: "winget",
        candidates: DISCORD_CANDIDATES,
        publisher: Some(ExecutablePublisher::Discord),
        store_package_family_name: None,
    },
    NativeAppManifest {
        id: NativeAppId::Telegram,
        display_name: "Telegram",
        adapter_service: AdapterService::Telegram,
        adapter_surface: AdapterSurface::InstalledNativeClient,
        // HELD at ComingSoon, D-206. The evidence for Telegram is real -- placement
        // driven live, cover text carried byte-exact and decoded back out, and the
        // signed-client row probe returning `supported` on the conversation pane --
        // but `native_app_support_status` maps BOTH `Supported` and `Experimental`
        // to `NativeAppSupportStatus::Beta`, and `beta` is a label
        // `osl-public-claim-allowlist.md:191` forbids for Telegram (it must carry
        // `Coming soon`, `Experimental` or `Externally blocked`) and :271-272
        // reserves for `runtime-proven` / `test-proven-only` rows.
        //
        // So the product cannot currently express the label the evidence supports.
        // `ExternallyBlocked` would agree with `support-matrix.json`, but it is a
        // STRONGER negative claim than the measurement now supports -- the rows are
        // demonstrably exposed -- so moving there would trade one wrong label for
        // another. Held until a status that renders as `Experimental` exists.
        adapter_support: SupportLevel::ComingSoon,
        package_id: "Telegram.TelegramDesktop",
        package_source: "winget",
        candidates: TELEGRAM_CANDIDATES,
        publisher: Some(ExecutablePublisher::Telegram),
        store_package_family_name: None,
    },
    NativeAppManifest {
        id: NativeAppId::Signal,
        display_name: "Signal",
        adapter_service: AdapterService::Signal,
        adapter_surface: AdapterSurface::InstalledNativeClient,
        adapter_support: SupportLevel::ComingSoon,
        package_id: "OpenWhisperSystems.Signal",
        package_source: "winget",
        candidates: SIGNAL_CANDIDATES,
        publisher: Some(ExecutablePublisher::Signal),
        store_package_family_name: None,
    },
    NativeAppManifest {
        id: NativeAppId::Whatsapp,
        display_name: "WhatsApp",
        adapter_service: AdapterService::Whatsapp,
        adapter_surface: AdapterSurface::InstalledNativeClient,
        adapter_support: SupportLevel::ComingSoon,
        package_id: "9NKSQGP7F2NH",
        package_source: "msstore",
        candidates: WHATSAPP_CANDIDATES,
        publisher: None,
        store_package_family_name: Some(WHATSAPP_PACKAGE_FAMILY_NAME),
    },
    NativeAppManifest {
        id: NativeAppId::Outlook,
        display_name: "Outlook",
        adapter_service: AdapterService::Outlook,
        adapter_surface: AdapterSurface::InstalledNativeClient,
        adapter_support: SupportLevel::ComingSoon,
        // Outlook is commonly provisioned with Microsoft 365 rather than as
        // an independently safe winget action. Missing Outlook remains
        // unavailable instead of exposing a guessed installer command.
        package_id: "",
        package_source: "unavailable",
        candidates: OUTLOOK_CLASSIC_CANDIDATES,
        publisher: Some(ExecutablePublisher::Microsoft),
        store_package_family_name: None,
    },
];

#[cfg(any(target_os = "windows", test))]
const CHROME_IMPORT_CANDIDATES: &[ExecutableCandidate] = &[
    ExecutableCandidate {
        folder: KnownFolder::Local,
        relative_path: r"Google\Chrome\Application\chrome.exe",
    },
    ExecutableCandidate {
        folder: KnownFolder::ProgramFiles,
        relative_path: r"Google\Chrome\Application\chrome.exe",
    },
    ExecutableCandidate {
        folder: KnownFolder::ProgramFilesX86,
        relative_path: r"Google\Chrome\Application\chrome.exe",
    },
];

#[cfg(any(target_os = "windows", test))]
const EDGE_IMPORT_CANDIDATES: &[ExecutableCandidate] = &[
    ExecutableCandidate {
        folder: KnownFolder::ProgramFilesX86,
        relative_path: r"Microsoft\Edge\Application\msedge.exe",
    },
    ExecutableCandidate {
        folder: KnownFolder::ProgramFiles,
        relative_path: r"Microsoft\Edge\Application\msedge.exe",
    },
    ExecutableCandidate {
        folder: KnownFolder::Local,
        relative_path: r"Microsoft\Edge\Application\msedge.exe",
    },
];

#[cfg(any(target_os = "windows", test))]
const BRAVE_IMPORT_CANDIDATES: &[ExecutableCandidate] = &[
    ExecutableCandidate {
        folder: KnownFolder::Local,
        relative_path: r"BraveSoftware\Brave-Browser\Application\brave.exe",
    },
    ExecutableCandidate {
        folder: KnownFolder::ProgramFiles,
        relative_path: r"BraveSoftware\Brave-Browser\Application\brave.exe",
    },
    ExecutableCandidate {
        folder: KnownFolder::ProgramFilesX86,
        relative_path: r"BraveSoftware\Brave-Browser\Application\brave.exe",
    },
];

#[cfg(any(target_os = "windows", test))]
const OPERA_IMPORT_CANDIDATES: &[ExecutableCandidate] = &[ExecutableCandidate {
    folder: KnownFolder::Local,
    relative_path: r"Programs\Opera\opera.exe",
}];

#[cfg(any(target_os = "windows", test))]
const DUCKDUCKGO_IMPORT_CANDIDATES: &[ExecutableCandidate] = &[];
#[cfg(any(target_os = "windows", test))]
const DUCKDUCKGO_PACKAGE_NAME: &str = "DuckDuckGo.DesktopBrowser";
#[cfg(any(target_os = "windows", test))]
const DUCKDUCKGO_EXECUTABLE_RELATIVE_PATH: &str = r"WindowsBrowser\DuckDuckGo.exe";
#[cfg(any(target_os = "windows", test))]
const DUCKDUCKGO_LOCATION_SCRIPT: &str = concat!(
    "$location = Get-AppxPackage -Name DuckDuckGo.DesktopBrowser ",
    "-ErrorAction SilentlyContinue | Select-Object -First 1 ",
    "-ExpandProperty InstallLocation; ",
    "if (-not [string]::IsNullOrWhiteSpace([string]$location)) { ",
    "$utf8 = [System.Text.UTF8Encoding]::new($false); ",
    "$bytes = $utf8.GetBytes([string]$location); ",
    "$stdout = [Console]::OpenStandardOutput(); ",
    "$stdout.Write($bytes, 0, $bytes.Length); $stdout.Flush() }"
);

#[cfg(any(target_os = "windows", test))]
const BROWSER_IMPORTS: &[BrowserImportManifest] = &[
    BrowserImportManifest {
        id: BrowserImportId::Chrome,
        display_name: "Chrome",
        candidates: CHROME_IMPORT_CANDIDATES,
        import_arguments: &["--new-window", "chrome://password-manager/settings"],
        publisher_attestation: BrowserPublisherAttestation::Verified(ExecutablePublisher::Chrome),
    },
    BrowserImportManifest {
        id: BrowserImportId::Edge,
        display_name: "Edge",
        candidates: EDGE_IMPORT_CANDIDATES,
        import_arguments: &["--new-window", "edge://settings/passwords"],
        publisher_attestation: BrowserPublisherAttestation::Verified(ExecutablePublisher::Edge),
    },
    BrowserImportManifest {
        id: BrowserImportId::Firefox,
        display_name: "Firefox",
        candidates: FIREFOX_CANDIDATES,
        import_arguments: &["--new-window", "about:logins"],
        publisher_attestation: BrowserPublisherAttestation::Verified(ExecutablePublisher::Firefox),
    },
    BrowserImportManifest {
        id: BrowserImportId::Brave,
        display_name: "Brave",
        candidates: BRAVE_IMPORT_CANDIDATES,
        import_arguments: &["--new-window", "brave://password-manager/settings"],
        publisher_attestation: BrowserPublisherAttestation::Verified(ExecutablePublisher::Brave),
    },
    BrowserImportManifest {
        id: BrowserImportId::Opera,
        display_name: "Opera",
        candidates: OPERA_IMPORT_CANDIDATES,
        import_arguments: &["--new-window", "opera://password-manager/settings"],
        publisher_attestation: BrowserPublisherAttestation::Verified(ExecutablePublisher::Opera),
    },
    BrowserImportManifest {
        id: BrowserImportId::DuckDuckGo,
        display_name: "DuckDuckGo",
        candidates: DUCKDUCKGO_IMPORT_CANDIDATES,
        // DuckDuckGo documents this workflow through its native menu rather
        // than a stable external settings URI. Launch only the verified app;
        // never guess a private URI or pass a profile path.
        import_arguments: &[],
        publisher_attestation: BrowserPublisherAttestation::Verified(
            ExecutablePublisher::DuckDuckGo,
        ),
    },
];

#[cfg(any(target_os = "windows", test))]
const FIREFOX_PACKAGE_ID: &str = "Mozilla.Firefox";
const FIREFOX_MIGRATION_SWITCH: &str = "--migration";
const FIREFOX_WAIT_FOR_BROWSER_SWITCH: &str = "-wait-for-browser";

/// Firefox data shares the same owner-hashed namespace as every embedded
/// service profile. The browser itself receives only the resulting local path;
/// neither the OSL user id nor a caller-controlled component reaches it.
#[cfg(any(target_os = "windows", test))]
const FIREFOX_PROFILE_BASE_COMPONENT: &str = "service-profiles-v2";

#[cfg(any(target_os = "windows", test))]
const FIREFOX_PROFILE_COMPONENT: &str = "firefox-browser";
#[cfg(any(target_os = "windows", test))]
const FIREFOX_UIA_USER_PREF: &str = "user_pref(\"accessibility.uia.enable\", 1);\n";

#[cfg(any(target_os = "windows", test))]
const FIREFOX_CANDIDATES: &[ExecutableCandidate] = &[
    ExecutableCandidate {
        folder: KnownFolder::ProgramFiles,
        relative_path: r"Mozilla Firefox\firefox.exe",
    },
    ExecutableCandidate {
        folder: KnownFolder::ProgramFilesX86,
        relative_path: r"Mozilla Firefox\firefox.exe",
    },
    ExecutableCandidate {
        folder: KnownFolder::Local,
        relative_path: r"Mozilla Firefox\firefox.exe",
    },
    ExecutableCandidate {
        folder: KnownFolder::Local,
        relative_path: r"Programs\Mozilla Firefox\firefox.exe",
    },
];

#[cfg(any(target_os = "windows", test))]
const FIREFOX_SERVICES: &[(FirefoxServiceId, &str)] = &[
    (
        FirefoxServiceId::Messenger,
        "https://www.facebook.com/messages/",
    ),
    (FirefoxServiceId::Gmail, "https://mail.google.com/"),
    (FirefoxServiceId::Outlook, "https://outlook.live.com/mail/"),
    (FirefoxServiceId::Proton, "https://mail.proton.me/"),
    (FirefoxServiceId::Tuta, "https://app.tuta.com/"),
    (FirefoxServiceId::Yahoo, "https://mail.yahoo.com/"),
    (FirefoxServiceId::Aol, "https://mail.aol.com/"),
    (FirefoxServiceId::Gmx, "https://www.gmx.com/"),
    (FirefoxServiceId::Maildotcom, "https://www.mail.com/"),
    (FirefoxServiceId::Icloud, "https://www.icloud.com/mail/"),
];

fn manifest(id: NativeAppId) -> &'static NativeAppManifest {
    match id {
        NativeAppId::Discord => &NATIVE_APPS[0],
        NativeAppId::Telegram => &NATIVE_APPS[1],
        NativeAppId::Signal => &NATIVE_APPS[2],
        NativeAppId::Whatsapp => &NATIVE_APPS[3],
        NativeAppId::Outlook => &NATIVE_APPS[4],
    }
}

pub(crate) fn native_app_display_name(id: NativeAppId) -> &'static str {
    manifest(id).display_name
}

pub(crate) fn whatsapp_store_package_family_name() -> &'static str {
    manifest(NativeAppId::Whatsapp)
        .store_package_family_name
        .expect("WhatsApp manifest must bind a Store package family")
}

#[cfg(any(target_os = "windows", test))]
pub(crate) fn native_app_publisher(id: NativeAppId) -> Option<ExecutablePublisher> {
    manifest(id).publisher
}

fn isolated_native_profile_available(id: NativeAppId) -> bool {
    matches!(id, NativeAppId::Discord | NativeAppId::Telegram)
}

/// The ruled surface a native app is.
///
/// `NativeAppId::Outlook` is the **desktop** Outlook surface. `PLAN.md` r5-2a
/// treats desktop and web Outlook as two surfaces "unless measurement shows one
/// composer shape serves both — that is a measurement, not a decision", and no
/// such measurement exists.
pub(crate) const fn claim_surface(id: NativeAppId) -> crate::claim_state::Surface {
    use crate::claim_state::Surface;
    match id {
        NativeAppId::Discord => Surface::Discord,
        NativeAppId::Telegram => Surface::Telegram,
        NativeAppId::Signal => Surface::Signal,
        NativeAppId::Whatsapp => Surface::Whatsapp,
        NativeAppId::Outlook => Surface::OutlookDesktop,
    }
}

/// The public claim for a native app.
///
/// **Derived, never written.** It used to read `manifest(id).adapter_support`
/// and collapse `Supported` and `Experimental` onto one label, which is the
/// collapse the owner gate names. `adapter_support` remains what it always was —
/// the *adapter profile's* internal support level, consumed by
/// `adapter_profile` and by the publication gate — and it is no longer a public
/// claim about anything.
fn native_app_support_status(id: NativeAppId) -> NativeAppSupportStatus {
    crate::claim_state::public_claim(claim_surface(id)).into()
}

fn native_app_support_status_for_claim(
    claim: &crate::claim_state::SurfaceClaim,
) -> NativeAppSupportStatus {
    crate::claim_state::claim_for(claim).into()
}

fn native_app_protected_mode(id: NativeAppId) -> NativeAppProtectedMode {
    match id {
        NativeAppId::Discord => NativeAppProtectedMode::AssistOnly,
        NativeAppId::Telegram
        | NativeAppId::Signal
        | NativeAppId::Whatsapp
        | NativeAppId::Outlook => NativeAppProtectedMode::Unavailable,
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn outlook_native_executable_paths() -> Vec<std::path::PathBuf> {
    let mut paths = Vec::with_capacity(2);
    if let Some(path) = outlook_store_executable_path() {
        paths.push(path);
    }
    if let Some(path) = installed_executable(manifest(NativeAppId::Outlook))
        .map(|trusted| trusted.path().to_owned())
    {
        paths.push(path);
    }
    paths
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn outlook_native_executable_paths() -> Vec<std::path::PathBuf> {
    Vec::new()
}

pub fn list_native_apps() -> Vec<NativeAppStatus> {
    list_native_apps_with_installer_probe(installer_available_for_listing)
}

fn list_native_apps_with_installer_probe(
    mut installer_available: impl FnMut() -> bool,
) -> Vec<NativeAppStatus> {
    list_native_apps_with_claims_and_installer_probe(
        |surface| *crate::claim_state::claim_of(surface),
        || installer_available(),
    )
}

fn list_native_apps_with_claims_and_installer_probe(
    claim_of: impl Fn(crate::claim_state::Surface) -> crate::claim_state::SurfaceClaim,
    mut installer_available: impl FnMut() -> bool,
) -> Vec<NativeAppStatus> {
    let mut statuses: Vec<_> = NATIVE_APPS
        .iter()
        .map(|app| {
            let installed = match app.id {
                NativeAppId::Whatsapp => whatsapp_store_package_installed(),
                NativeAppId::Outlook => !outlook_native_executable_paths().is_empty(),
                _ => installed_executable(app).is_some(),
            };
            let availability = if installed {
                NativeAppAvailability::Installed
            } else {
                NativeAppAvailability::Unavailable
            };
            let claim = claim_of(claim_surface(app.id));
            NativeAppStatus {
                id: app.id,
                display_name: app.display_name,
                availability,
                support_status: native_app_support_status_for_claim(&claim),
                carrier_evidence: claim.carrier.slug(),
                delivery_evidence: claim.delivery.slug(),
                claim_blockers: claim
                    .blockers
                    .iter()
                    .map(|blocker| blocker.slug())
                    .collect(),
                claim_note: claim.reason,
                status_page: crate::claim_state::status_page_data_for(claim).into(),
                protected_mode: native_app_protected_mode(app.id),
                isolated_profile_available: isolated_native_profile_available(app.id),
                supports_overlay: false,
            }
        })
        .collect();

    // Resolving the signed App Installer package launches a bounded PowerShell
    // query on Windows. Skip it when every app is present; otherwise do it once
    // per refresh and reuse the result across all missing app tiles.
    if statuses
        .iter()
        .any(|status| status.availability == NativeAppAvailability::Unavailable)
        && installer_available()
    {
        for status in &mut statuses {
            if status.availability == NativeAppAvailability::Unavailable
                && status.id != NativeAppId::Outlook
            {
                status.availability = NativeAppAvailability::Installable;
            }
        }
    }
    statuses
}

/// Reports only whether one of six fixed browsers exists in a standard
/// Windows install location. OSL never opens a profile or reads browser data.
pub fn list_browser_imports() -> Vec<BrowserImportStatus> {
    #[cfg(any(target_os = "windows", test))]
    {
        BROWSER_IMPORTS
            .iter()
            .map(|browser| BrowserImportStatus {
                id: browser.id,
                display_name: browser.display_name,
                // Listing is only a fast presence hint for the setup UI. The
                // action path below still performs the full publisher and
                // file-identity verification before it launches anything.
                installed: browser_import_present(browser),
            })
            .collect()
    }
    #[cfg(not(any(target_os = "windows", test)))]
    {
        Vec::new()
    }
}

#[cfg(target_os = "windows")]
fn browser_import_present(browser: &BrowserImportManifest) -> bool {
    if browser.id == BrowserImportId::DuckDuckGo {
        return duckduckgo_store_executable().is_some();
    }
    browser.candidates.iter().any(|candidate| {
        known_folder(candidate.folder)
            .map(|folder| folder.join(candidate.relative_path).is_file())
            .unwrap_or(false)
    })
}

#[cfg(all(test, not(target_os = "windows")))]
fn browser_import_present(_browser: &BrowserImportManifest) -> bool {
    false
}

/// Opens one browser-owned password-manager/export surface after an explicit
/// click. The browser retains export confirmation, OS or primary-password
/// authentication, the destination chooser, and the resulting plaintext CSV.
/// The caller supplies only an enum; executable paths and arguments are fixed.
/// OSL does not receive a result from, observe, or perform the export.
pub fn open_browser_import(id: BrowserImportId) -> Result<BrowserImportResult, String> {
    #[cfg(target_os = "windows")]
    {
        let browser = browser_import_manifest(id);
        let executable = browser_import_executable(browser).ok_or_else(|| {
            format!(
                "{} is not installed in a supported Windows location",
                browser.display_name
            )
        })?;
        spawn_trusted_detached(&executable, browser.import_arguments).map_err(|_| {
            format!(
                "{} could not open its password manager",
                browser.display_name
            )
        })?;
        Ok(BrowserImportResult { id, opened: true })
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = id;
        Err("Browser-owned password handoff is available only on Windows".to_owned())
    }
}

/// Opens Firefox's own migration wizard inside OSL's isolated Firefox profile.
/// Detection, source choice, profile path, executable, and arguments are fixed
/// natively. Firefox—not OSL—reads supported browser data, requests OS/browser
/// authorization, and owns any CSV file picker.
pub fn begin_browser_account_import(
    app_local_data_dir: &std::path::Path,
    owner_osl_user_id: &str,
) -> Result<BrowserAccountImportResult, String> {
    #[cfg(target_os = "windows")]
    {
        let detected_sources = BROWSER_IMPORTS
            .iter()
            .filter(|browser| browser_import_executable(browser).is_some())
            .map(|browser| browser.id)
            .collect::<Vec<_>>();
        let preferred_source = preferred_browser_import_source(&detected_sources)
            .ok_or_else(|| "No supported browser account source was found".to_owned())?;
        let firefox = firefox_executable().ok_or_else(|| {
            "Firefox is required for OSL's browser-owned account migration".to_owned()
        })?;
        let profile = ensure_firefox_profile(app_local_data_dir, owner_osl_user_id)?;
        let mut firefox_process = spawn_firefox_migration_wizard(&firefox, &profile)
            .map_err(|_| "The OSL Firefox import settings could not be opened".to_owned())?;
        thread::sleep(Duration::from_millis(900));
        if firefox_process
            .try_wait()
            .map_err(|_| "The OSL Firefox import process could not be verified".to_owned())?
            .is_some()
        {
            return Err("The OSL Firefox import settings closed before opening".to_owned());
        }
        Ok(BrowserAccountImportResult {
            preferred_source,
            detected_sources,
            opened: true,
            mode: "firefoxMigrationWizard",
            manual_export_required: matches!(
                preferred_source,
                BrowserImportId::Chrome | BrowserImportId::Firefox
            ),
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app_local_data_dir, owner_osl_user_id);
        Err("Browser account migration is available only on Windows".to_owned())
    }
}

/// Starts the browser-owned migration flow for a bounded set chosen in OSL.
/// OSL never accepts paths, profiles, URLs, credentials, or browser arguments.
pub fn begin_protected_browser_import(
    app_local_data_dir: &std::path::Path,
    owner_osl_user_id: &str,
    selected_sources: Vec<BrowserImportId>,
) -> Result<ProtectedBrowserImportResult, String> {
    #[cfg(target_os = "windows")]
    {
        close_protected_browser_import_process()?;
        if selected_sources.len() != 1 {
            return Err("Open exactly one queued browser import at a time".to_owned());
        }
        let mut unique = Vec::with_capacity(selected_sources.len());
        for id in selected_sources {
            if unique.contains(&id)
                || browser_import_executable(browser_import_manifest(id)).is_none()
            {
                return Err("A selected browser is unavailable".to_owned());
            }
            unique.push(id);
        }
        let firefox = firefox_executable().ok_or_else(|| {
            "Firefox is required for OSL's browser-owned account migration".to_owned()
        })?;
        let profile = ensure_firefox_profile(app_local_data_dir, owner_osl_user_id)?;
        let mut firefox_process = spawn_firefox_migration_wizard(&firefox, &profile)
            .map_err(|_| "The OSL Firefox migration wizard could not be opened".to_owned())?;
        thread::sleep(Duration::from_millis(900));
        if firefox_process
            .try_wait()
            .map_err(|_| "The OSL Firefox import process could not be verified".to_owned())?
            .is_some()
        {
            return Err("The OSL Firefox migration wizard closed before opening".to_owned());
        }
        let process_id = firefox_process.id();
        let coordination = crate::firefox_migration_coordinator::coordinate(process_id, unique[0]);
        *protected_browser_import_process()
            .lock()
            .map_err(|_| "The OSL Firefox import process state is unavailable".to_owned())? =
            Some(firefox_process);
        if coordination.is_ok() {
            // Firefox closes the migration wizard after its own Import action
            // finishes. Wait for that exact OSL-owned window so the renderer
            // can advance a multi-browser queue without asking for a second
            // confirmation. The retained process is still the only process
            // finish_protected_browser_import may close or terminate.
            let deadline = Instant::now() + Duration::from_secs(300);
            loop {
                if crate::firefox_migration_coordinator::is_closed(process_id).is_ok() {
                    close_protected_browser_import_process()?;
                    break;
                }
                if Instant::now() >= deadline {
                    return Err("Firefox import did not finish within five minutes".to_owned());
                }
                thread::sleep(Duration::from_millis(100));
            }
        }
        Ok(ProtectedBrowserImportResult {
            selected_sources: unique,
            started: true,
            mode: "firefoxMigrationWizard",
            source_selected: coordination.is_ok(),
            manual_fallback: coordination.err(),
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app_local_data_dir, owner_osl_user_id, selected_sources);
        Err("Browser account migration is available only on Windows".to_owned())
    }
}

/// Closes only the exact isolated Firefox migration process retained above.
/// The user's normal Firefox process and profiles are never enumerated or touched.
pub fn finish_protected_browser_import() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        close_protected_browser_import_process()
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("Browser account migration is available only on Windows".to_owned())
    }
}

#[cfg(any(target_os = "windows", test))]
fn preferred_browser_import_source(sources: &[BrowserImportId]) -> Option<BrowserImportId> {
    // Prefer Firefox's direct Windows migrators before Chrome's specialized
    // manual CSV flow. The wizard remains visible and lets the user change the
    // detected source before confirming anything.
    [
        BrowserImportId::Edge,
        BrowserImportId::Brave,
        BrowserImportId::Opera,
        BrowserImportId::DuckDuckGo,
        BrowserImportId::Chrome,
        BrowserImportId::Firefox,
    ]
    .into_iter()
    .find(|candidate| sources.contains(candidate))
}

#[cfg(any(target_os = "windows", test))]
fn browser_import_manifest(id: BrowserImportId) -> &'static BrowserImportManifest {
    BROWSER_IMPORTS
        .iter()
        .find(|browser| browser.id == id)
        .expect("every browser import enum has a fixed manifest")
}

#[cfg(any(target_os = "windows", test))]
fn attested_browser_publisher(browser: &BrowserImportManifest) -> Option<ExecutablePublisher> {
    match browser.publisher_attestation {
        BrowserPublisherAttestation::Verified(publisher) => Some(publisher),
    }
}

#[cfg(target_os = "windows")]
fn browser_import_executable(browser: &BrowserImportManifest) -> Option<TrustedExecutable> {
    if browser.id == BrowserImportId::DuckDuckGo {
        return duckduckgo_store_executable();
    }
    let publisher = attested_browser_publisher(browser)?;
    browser.candidates.iter().find_map(|candidate| {
        let executable = known_folder(candidate.folder)?.join(candidate.relative_path);
        verify_executable(&executable, publisher).ok()
    })
}

/// Resolve one renderer-selected browser enum through the fixed manifest and
/// re-verify its installed executable. No path, argument, or profile selector
/// crosses the IPC boundary.
#[cfg(target_os = "windows")]
pub(crate) fn trusted_browser_executable(id: BrowserImportId) -> Option<TrustedExecutable> {
    browser_import_executable(browser_import_manifest(id))
}

#[cfg(target_os = "windows")]
fn duckduckgo_store_executable() -> Option<TrustedExecutable> {
    let system_directory = system_directory()?;
    let powershell = system_directory
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    if !powershell.is_file() {
        return None;
    }
    let mut command = Command::new(powershell);
    command
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            DUCKDUCKGO_LOCATION_SCRIPT,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let output = command_output_with_timeout(command, APP_INSTALLER_PROBE_TIMEOUT).ok()?;
    if !output.status.success() {
        return None;
    }
    let root = decode_duckduckgo_location(&output.stdout)?;
    let program_files = known_folder(KnownFolder::ProgramFiles)?;
    if !is_trusted_duckduckgo_package_path(&root, &program_files) {
        return None;
    }
    verify_executable(
        &root.join(DUCKDUCKGO_EXECUTABLE_RELATIVE_PATH),
        ExecutablePublisher::DuckDuckGo,
    )
    .ok()
}

#[cfg(any(target_os = "windows", test))]
fn decode_duckduckgo_location(stdout: &[u8]) -> Option<PathBuf> {
    let location = std::str::from_utf8(stdout).ok()?;
    if location.is_empty() || location.contains(['\r', '\n', '\0']) || location.trim() != location {
        return None;
    }
    Some(PathBuf::from(location))
}

#[cfg(any(target_os = "windows", test))]
fn duckduckgo_package_full_name_matches(full_name: &str) -> bool {
    let parts = full_name.split('_').collect::<Vec<_>>();
    if parts.len() != 5
        || parts[0] != DUCKDUCKGO_PACKAGE_NAME
        || !matches!(parts[2], "x64" | "x86" | "arm64" | "neutral")
        || !parts[3].is_empty()
        || !(8..=20).contains(&parts[4].len())
        || !parts[4]
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    {
        return false;
    }
    let version = parts[1].split('.').collect::<Vec<_>>();
    version.len() == 4
        && version.iter().all(|component| {
            !component.is_empty() && component.bytes().all(|byte| byte.is_ascii_digit())
        })
}

#[cfg(any(target_os = "windows", test))]
fn is_trusted_duckduckgo_package_path(path: &Path, program_files: &Path) -> bool {
    if path_has_parent_component(path) {
        return false;
    }
    let Some(package_directory_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    path.parent().is_some_and(|parent| {
        path_file_name_eq(parent, WINDOWS_APPS_DIRECTORY)
            && path_eq_ignore_ascii_case(parent.parent().unwrap_or(Path::new("")), program_files)
    }) && duckduckgo_package_full_name_matches(package_directory_name)
}

#[cfg(all(test, not(target_os = "windows")))]
fn browser_import_executable(
    _browser: &BrowserImportManifest,
) -> Option<crate::windows_executable_trust::TrustedExecutable> {
    None
}

/// Starts a fixed package-manager action after a trusted UI click. This does
/// not request elevation, invoke a shell, or accept package/options from the
/// caller. Winget and the package installer remain responsible for any user
/// confirmation their package requires.
pub fn install_native_app(id: NativeAppId) -> Result<NativeInstallResult, String> {
    #[cfg(target_os = "windows")]
    {
        let app = manifest(id);
        if id == NativeAppId::Outlook {
            return Err(
                "Outlook installation is managed by Microsoft 365 or the Microsoft Store"
                    .to_owned(),
            );
        }
        let winget = installer_executable()
            .ok_or_else(|| "Windows App Installer (winget) is unavailable".to_owned())?;
        let arguments = [
            "install",
            "--id",
            app.package_id,
            "--exact",
            "--source",
            app.package_source,
            "--accept-source-agreements",
            "--accept-package-agreements",
            "--silent",
        ];
        spawn_detached(&winget, &arguments)
            .map_err(|_| format!("The {} installer could not be started", app.display_name))?;
        Ok(NativeInstallResult {
            id,
            started: true,
            package_id: app.package_id,
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = id;
        Err("Native app installation is available only on Windows".to_owned())
    }
}

/// Installs the fixed official Discord PTB channel only when the normal
/// Stable channel is already populated outside OSL. PTB gives OSL a separate
/// signed native profile without reading, moving, or closing Stable Discord.
/// The exact winget argument vector used to install the dedicated Discord
/// channel.
///
/// Split out so the "one fixed official channel" property can be asserted
/// without running an installer. The test used to assert that
/// `install_discord_dedicated_channel()` returned `Err`, which only held on
/// machines that have no winget; on a CI runner that does have it, the test
/// was launching a real Discord PTB install as a side effect.
#[cfg(any(target_os = "windows", test))]
pub(crate) fn discord_dedicated_install_arguments() -> [&'static str; 9] {
    [
        "install",
        "--id",
        DISCORD_DEDICATED_PACKAGE_ID,
        "--exact",
        "--source",
        "winget",
        "--accept-source-agreements",
        "--accept-package-agreements",
        "--silent",
    ]
}

pub fn install_discord_dedicated_channel() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let winget = installer_executable()
            .ok_or_else(|| "Windows App Installer (winget) is unavailable".to_owned())?;
        let arguments = discord_dedicated_install_arguments();
        spawn_detached(&winget, &arguments)
            .map_err(|_| "The dedicated Discord installer could not be started".to_owned())?;
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("Dedicated Discord installation is available only on Windows".to_owned())
    }
}

/// Reports only whether the fixed Mullvad desktop executable exists in one of
/// its reviewed standard Windows locations. No account, tunnel, traffic, or
/// configuration state is read.
pub fn get_mullvad_status() -> MullvadStatus {
    let availability = if mullvad_executable().is_some() {
        NativeAppAvailability::Installed
    } else if installer_available_for_listing() {
        NativeAppAvailability::Installable
    } else {
        NativeAppAvailability::Unavailable
    };
    MullvadStatus { availability }
}

/// Starts only the exact current Mullvad package from the public winget
/// repository. Winget verifies the selected manifest and installer; OSL does
/// not accept a package id, source, executable path, or argument from the UI.
pub fn install_mullvad() -> Result<MullvadActionResult, String> {
    #[cfg(target_os = "windows")]
    {
        let winget = installer_executable()
            .ok_or_else(|| "Windows App Installer (winget) is unavailable".to_owned())?;
        let arguments = mullvad_install_arguments();
        spawn_detached(&winget, &arguments)
            .map_err(|_| "The Mullvad installer could not be started".to_owned())?;
        Ok(MullvadActionResult { started: true })
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("Mullvad installation is available only on Windows".to_owned())
    }
}

#[cfg(any(target_os = "windows", test))]
fn mullvad_install_arguments() -> [&'static str; 9] {
    [
        "install",
        "--id",
        MULLVAD_PACKAGE_ID,
        "--exact",
        "--source",
        "winget",
        "--accept-source-agreements",
        "--accept-package-agreements",
        "--silent",
    ]
}

#[cfg(any(target_os = "windows", test))]
fn mullvad_open_candidates() -> &'static [ExecutableCandidate] {
    MULLVAD_CANDIDATES
}

/// Opens only Mullvad's reviewed desktop executable with no arguments. OSL
/// never reads or changes VPN account, tunnel, DNS, or Lockdown Mode state.
pub fn open_mullvad() -> Result<MullvadActionResult, String> {
    #[cfg(target_os = "windows")]
    {
        let executable = mullvad_executable()
            .ok_or_else(|| "Mullvad is not installed in a supported Windows location".to_owned())?;
        spawn_detached(&executable, &[]).map_err(|_| "Mullvad could not be opened".to_owned())?;
        Ok(MullvadActionResult { started: true })
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("Mullvad can be opened by OSL only on Windows".to_owned())
    }
}

#[cfg(target_os = "windows")]
fn mullvad_executable() -> Option<PathBuf> {
    mullvad_open_candidates().iter().find_map(|candidate| {
        let executable = known_folder(candidate.folder)?.join(candidate.relative_path);
        executable.is_file().then_some(executable)
    })
}

#[cfg(all(test, not(target_os = "windows")))]
fn mullvad_executable() -> Option<std::path::PathBuf> {
    None
}

#[cfg(all(not(target_os = "windows"), not(test)))]
fn mullvad_executable() -> Option<std::path::PathBuf> {
    None
}

pub fn get_firefox_status() -> FirefoxStatus {
    let availability = if firefox_executable().is_some() {
        NativeAppAvailability::Installed
    } else if installer_available_for_listing() {
        NativeAppAvailability::Installable
    } else {
        NativeAppAvailability::Unavailable
    };
    FirefoxStatus { availability }
}

/// Opens one exact reviewed service origin in an independently installed
/// Firefox process. Firefox receives one fixed OSL-owned local profile and
/// `--new-tab`, allowing its existing profile process to handle later clicks
/// instead of creating another service window. The caller selects only an enum
/// and can supply neither a URL, profile path, nor browser argument. OSL never
/// embeds or controls the resulting page.
pub fn launch_firefox_service(
    app_local_data_dir: &std::path::Path,
    owner_osl_user_id: &str,
    service_id: FirefoxServiceId,
) -> Result<FirefoxLaunchResult, String> {
    #[cfg(target_os = "windows")]
    {
        let firefox = firefox_executable()
            .ok_or_else(|| "Firefox is not installed in a supported Windows location".to_owned())?;
        let profile = ensure_firefox_profile(app_local_data_dir, owner_osl_user_id)?;
        let url = firefox_service_url(service_id);
        spawn_firefox_tab(&firefox, &profile, url)
            .map_err(|_| "Firefox could not be launched".to_owned())?;
        Ok(FirefoxLaunchResult {
            service_id,
            started: true,
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app_local_data_dir, owner_osl_user_id);
        let _ = service_id;
        Err("Firefox service launching is available only on Windows".to_owned())
    }
}

/// Starts the one fixed Mozilla Firefox winget install action. It is called
/// only after an explicit trusted-UI click and never asks Windows to elevate.
pub fn install_firefox() -> Result<FirefoxInstallResult, String> {
    #[cfg(target_os = "windows")]
    {
        let winget = installer_executable()
            .ok_or_else(|| "Windows App Installer (winget) is unavailable".to_owned())?;
        let arguments = [
            "install",
            "--id",
            FIREFOX_PACKAGE_ID,
            "--exact",
            "--source",
            "winget",
            "--accept-source-agreements",
            "--accept-package-agreements",
        ];
        spawn_detached(&winget, &arguments)
            .map_err(|_| "The Firefox installer could not be started".to_owned())?;
        Ok(FirefoxInstallResult {
            started: true,
            package_id: FIREFOX_PACKAGE_ID,
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("Firefox installation is available only on Windows".to_owned())
    }
}

#[cfg(any(target_os = "windows", test))]
pub(crate) fn firefox_service_url(service_id: FirefoxServiceId) -> &'static str {
    match service_id {
        FirefoxServiceId::Messenger => "https://www.facebook.com/messages/",
        FirefoxServiceId::Gmail => "https://mail.google.com/",
        FirefoxServiceId::Outlook => "https://outlook.live.com/mail/",
        FirefoxServiceId::Proton => "https://mail.proton.me/",
        FirefoxServiceId::Tuta => "https://app.tuta.com/",
        FirefoxServiceId::Yahoo => "https://mail.yahoo.com/",
        FirefoxServiceId::Aol => "https://mail.aol.com/",
        FirefoxServiceId::Gmx => "https://www.gmx.com/",
        FirefoxServiceId::Maildotcom => "https://www.mail.com/",
        FirefoxServiceId::Icloud => "https://www.icloud.com/mail/",
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn trusted_browser_executable_at(
    path: &Path,
) -> Option<(BrowserImportId, TrustedExecutable)> {
    let canonical = path.canonicalize().ok()?;
    if let Some(trusted) = duckduckgo_store_executable() {
        if trusted.path() == canonical {
            return Some((BrowserImportId::DuckDuckGo, trusted));
        }
    }
    for browser in BROWSER_IMPORTS {
        for candidate in browser.candidates {
            let Some(expected) = known_folder(candidate.folder)
                .and_then(|folder| folder.join(candidate.relative_path).canonicalize().ok())
            else {
                continue;
            };
            if expected == canonical {
                let publisher = attested_browser_publisher(browser)?;
                return verify_executable(&canonical, publisher)
                    .ok()
                    .map(|trusted| (browser.id, trusted));
            }
        }
    }
    None
}

#[cfg(any(target_os = "windows", test))]
pub(crate) fn browser_display_name(id: BrowserImportId) -> &'static str {
    browser_import_manifest(id).display_name
}

#[cfg(any(target_os = "windows", test))]
pub(crate) fn firefox_service_display_name(id: FirefoxServiceId) -> &'static str {
    match id {
        FirefoxServiceId::Gmail => "Gmail",
        FirefoxServiceId::Outlook => "Outlook",
        FirefoxServiceId::Proton => "Proton Mail",
        FirefoxServiceId::Tuta => "Tuta Mail",
        FirefoxServiceId::Yahoo => "Yahoo Mail",
        FirefoxServiceId::Aol => "AOL Mail",
        FirefoxServiceId::Gmx => "GMX Mail",
        FirefoxServiceId::Maildotcom => "mail.com",
        FirefoxServiceId::Icloud => "iCloud Mail",
    }
}

#[cfg(target_os = "windows")]
fn version_from_package_full_name(full_name: &str) -> Option<String> {
    let version = full_name.split('_').nth(1)?;
    let components = version.split('.').collect::<Vec<_>>();
    (components.len() == 4
        && components.iter().all(|component| {
            !component.is_empty() && component.bytes().all(|byte| byte.is_ascii_digit())
        }))
    .then(|| version.to_owned())
}

#[cfg(target_os = "windows")]
fn file_version(path: &Path) -> Option<String> {
    let system_directory = system_directory()?;
    let powershell = system_directory
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    if !powershell.is_file() {
        return None;
    }
    let mut command = Command::new(powershell);
    command
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[Diagnostics.FileVersionInfo]::GetVersionInfo($args[0]).FileVersion",
        ])
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let output = command_output_with_timeout(command, APP_INSTALLER_PROBE_TIMEOUT).ok()?;
    if !output.status.success() {
        return None;
    }
    let version = std::str::from_utf8(&output.stdout).ok()?.trim();
    (!version.is_empty()
        && !version.contains('\0')
        && version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b' ')))
    .then(|| version.to_owned())
}

#[cfg(target_os = "windows")]
fn trusted_executable_version(executable: &TrustedExecutable) -> Option<String> {
    if let Some(parent) = executable.path().parent() {
        if let Some(version) = parent
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_prefix("app-"))
            .filter(|version| discord_version_key(&format!("app-{version}")).is_some())
        {
            return Some(version.to_owned());
        }
    }
    file_version(executable.path())
}

#[cfg(target_os = "windows")]
pub(crate) fn native_app_exact_version(id: NativeAppId) -> Option<String> {
    match id {
        NativeAppId::Whatsapp => whatsapp_store_package_version(),
        NativeAppId::Outlook => outlook_store_package_version().or_else(|| {
            installed_executable(manifest(id))
                .and_then(|executable| trusted_executable_version(&executable))
        }),
        _ => installed_executable(manifest(id))
            .and_then(|executable| trusted_executable_version(&executable)),
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn native_app_exact_version(_id: NativeAppId) -> Option<String> {
    None
}

#[cfg(target_os = "windows")]
pub(crate) fn browser_import_exact_version(id: BrowserImportId) -> Option<String> {
    browser_import_executable(browser_import_manifest(id))
        .and_then(|executable| trusted_executable_version(&executable))
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn browser_import_exact_version(_id: BrowserImportId) -> Option<String> {
    None
}

#[cfg(target_os = "windows")]
pub(crate) fn firefox_service_browser_exact_version(_id: FirefoxServiceId) -> Option<String> {
    firefox_executable().and_then(|executable| trusted_executable_version(&executable))
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn firefox_service_browser_exact_version(_id: FirefoxServiceId) -> Option<String> {
    None
}

#[cfg(any(target_os = "windows", test))]
pub(crate) fn browser_uses_chromium_app_mode(id: BrowserImportId) -> bool {
    matches!(
        id,
        BrowserImportId::Chrome
            | BrowserImportId::Edge
            | BrowserImportId::Brave
            | BrowserImportId::Opera
    )
}

#[cfg(target_os = "windows")]
fn firefox_executable() -> Option<TrustedExecutable> {
    FIREFOX_CANDIDATES.iter().find_map(|candidate| {
        let executable = known_folder(candidate.folder)?.join(candidate.relative_path);
        verify_executable(&executable, ExecutablePublisher::Firefox).ok()
    })
}

#[cfg(target_os = "windows")]
fn ensure_firefox_profile(
    app_local_data_dir: &Path,
    owner_osl_user_id: &str,
) -> Result<PathBuf, String> {
    if !app_local_data_dir.is_absolute() || app_local_data_dir.parent().is_none() {
        return Err("The OSL app-local-data directory is invalid".to_owned());
    }
    let base = app_local_data_dir.to_owned();
    ensure_plain_directory(&base)?;
    let canonical_base = base
        .canonicalize()
        .map_err(|_| "The OSL app-local-data directory could not be verified".to_owned())?;
    let owner_namespace = crate::service_host::owner_profile_namespace(owner_osl_user_id)
        .map_err(|_| "The active OSL identity is invalid".to_owned())?;
    let mut profile = base;
    for component in [
        FIREFOX_PROFILE_BASE_COMPONENT,
        owner_namespace.as_str(),
        FIREFOX_PROFILE_COMPONENT,
    ] {
        profile.push(component);
        ensure_plain_directory(&profile)?;
    }
    let canonical_profile = profile
        .canonicalize()
        .map_err(|_| "The Firefox profile directory could not be verified".to_owned())?;
    if !canonical_profile.starts_with(&canonical_base) {
        return Err("The Firefox profile directory escaped local OSL storage".to_owned());
    }
    ensure_firefox_migration_uia(&canonical_profile)?;
    Ok(canonical_profile)
}

/// Firefox can otherwise expose only an empty document shell to Windows UI
/// Automation until an assistive-technology client has already activated it.
/// This preference is written solely inside the OSL-owned migration profile;
/// it never changes the user's ordinary Firefox profile.
#[cfg(target_os = "windows")]
fn ensure_firefox_migration_uia(profile: &Path) -> Result<(), String> {
    use std::io::{Read, Write};
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};

    const MAX_USER_JS_BYTES: u64 = 64 * 1024;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

    let path = profile.join("user.js");
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) => {
            if !metadata.is_file()
                || metadata.file_type().is_symlink()
                || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
                || metadata.len() > MAX_USER_JS_BYTES
            {
                return Err("The OSL Firefox accessibility preference file is invalid".to_owned());
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("The OSL Firefox accessibility preference is unavailable".to_owned()),
    }

    let mut current = String::new();
    if path.exists() {
        let mut source = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&path)
            .map_err(|_| "The OSL Firefox accessibility preference could not be read".to_owned())?;
        let opened = source.metadata().map_err(|_| {
            "The OSL Firefox accessibility preference could not be verified".to_owned()
        })?;
        if !opened.is_file()
            || opened.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || opened.len() > MAX_USER_JS_BYTES
        {
            return Err("The OSL Firefox accessibility preference file is invalid".to_owned());
        }
        source
            .read_to_string(&mut current)
            .map_err(|_| "The OSL Firefox accessibility preference could not be read".to_owned())?;
    }
    if current
        .lines()
        .any(|line| line.trim() == FIREFOX_UIA_USER_PREF.trim())
    {
        return Ok(());
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&path)
        .map_err(|_| "The OSL Firefox accessibility preference could not be written".to_owned())?;
    let opened = file
        .metadata()
        .map_err(|_| "The OSL Firefox accessibility preference could not be verified".to_owned())?;
    if !opened.is_file()
        || opened.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || opened.len() > MAX_USER_JS_BYTES
    {
        return Err("The OSL Firefox accessibility preference file is invalid".to_owned());
    }
    if !current.is_empty() && !current.ends_with('\n') {
        file.write_all(b"\n").map_err(|_| {
            "The OSL Firefox accessibility preference could not be written".to_owned()
        })?;
    }
    file.write_all(FIREFOX_UIA_USER_PREF.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| "The OSL Firefox accessibility preference could not be committed".to_owned())
}

#[cfg(test)]
pub(crate) fn firefox_profile_relative_path(owner_osl_user_id: &str) -> std::path::PathBuf {
    let owner_namespace = crate::service_host::owner_profile_namespace(owner_osl_user_id)
        .expect("test owner must be valid");
    [
        FIREFOX_PROFILE_BASE_COMPONENT,
        owner_namespace.as_str(),
        FIREFOX_PROFILE_COMPONENT,
    ]
    .iter()
    .collect()
}

#[cfg(target_os = "windows")]
fn ensure_plain_directory(path: &Path) -> Result<(), String> {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.is_dir()
                || metadata.file_type().is_symlink()
                || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
            {
                return Err("The OSL Firefox profile path is not a plain directory".to_owned());
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir(path)
                .map_err(|_| "The OSL Firefox profile directory could not be created".to_owned())?;
            // Re-read after creation. This detects a junction/reparse point
            // substituted before Firefox receives the path.
            let metadata = std::fs::symlink_metadata(path).map_err(|_| {
                "The OSL Firefox profile directory could not be verified".to_owned()
            })?;
            if !metadata.is_dir()
                || metadata.file_type().is_symlink()
                || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
            {
                return Err("The OSL Firefox profile path is not a plain directory".to_owned());
            }
        }
        Err(_) => {
            return Err("The OSL Firefox profile directory is unavailable".to_owned());
        }
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn firefox_executable() -> Option<crate::windows_executable_trust::TrustedExecutable> {
    None
}

#[cfg(target_os = "windows")]
fn installed_executable(app: &NativeAppManifest) -> Option<TrustedExecutable> {
    let publisher = app.publisher?;
    if app.id == NativeAppId::Discord {
        return app.candidates.iter().find_map(|candidate| {
            let local = known_folder(candidate.folder)?;
            let updater = local.join(candidate.relative_path);
            let install_root = updater.parent()?;
            let executable_name = discord_executable_name(candidate.relative_path)?;
            let executable =
                newest_discord_channel_executable_under(install_root, executable_name)?;
            verify_executable(&executable, publisher).ok()
        });
    }
    app.candidates.iter().find_map(|candidate| {
        let root = known_folder(candidate.folder)?;
        let executable = root.join(candidate.relative_path);
        verify_executable(&executable, publisher).ok()
    })
}

#[cfg(test)]
pub(crate) fn newest_discord_executable_under(install_root: &Path) -> Option<PathBuf> {
    newest_discord_channel_executable_under(install_root, "Discord.exe")
}

#[cfg(any(target_os = "windows", test))]
fn newest_discord_channel_executable_under(
    install_root: &Path,
    executable_name: &str,
) -> Option<PathBuf> {
    let mut candidates = std::fs::read_dir(install_root)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_str()?;
            let version = discord_version_key(name)?;
            let executable = entry.path().join(executable_name);
            executable.is_file().then_some((version, executable))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.0.cmp(&right.0));
    candidates.pop().map(|(_, executable)| executable)
}

#[cfg(any(target_os = "windows", test))]
fn discord_executable_name(update_relative_path: &str) -> Option<&'static str> {
    match update_relative_path {
        r"Discord\Update.exe" => Some("Discord.exe"),
        r"DiscordPTB\Update.exe" => Some("DiscordPTB.exe"),
        r"DiscordCanary\Update.exe" => Some("DiscordCanary.exe"),
        _ => None,
    }
}

#[cfg(any(target_os = "windows", test))]
fn discord_version_key(directory_name: &str) -> Option<Vec<u64>> {
    let version = directory_name.strip_prefix("app-")?;
    let components = version
        .split('.')
        .map(|component| {
            (!component.is_empty() && component.bytes().all(|byte| byte.is_ascii_digit()))
                .then(|| component.parse::<u64>().ok())
                .flatten()
        })
        .collect::<Option<Vec<_>>>()?;
    (components.len() >= 2).then_some(components)
}

#[cfg(not(target_os = "windows"))]
fn installed_executable(
    _app: &NativeAppManifest,
) -> Option<crate::windows_executable_trust::TrustedExecutable> {
    None
}

/// Uses the current user's protected AppModel registration rather than a
/// user-writable executable alias. Dedicated secondary WhatsApp profiles stay
/// unsupported; this registration also anchors the consented existing-session
/// companion path.
#[cfg(target_os = "windows")]
fn whatsapp_store_package_installed() -> bool {
    whatsapp_store_package_root().is_some()
}

#[cfg(target_os = "windows")]
fn whatsapp_store_package_version() -> Option<String> {
    whatsapp_store_package_full_name()
        .and_then(|full_name| version_from_package_full_name(&full_name))
}

/// Resolve the executable only through the current user's exact AppX
/// registration. WhatsApp's packaged executable is not independently
/// Authenticode signed, so callers must never fall back to a user-writable
/// alias or an executable-name search.
#[cfg(target_os = "windows")]
pub(crate) fn whatsapp_store_executable_path() -> Option<PathBuf> {
    let root = whatsapp_store_package_root()?;
    // A normal unpackaged desktop process can be denied redundant metadata
    // reads inside WindowsApps even though the exact registered app can run.
    // The package API and protected-root checks below are the trust boundary;
    // process discovery later requires this exact canonical image path.
    Some(root.join("WhatsApp.Root.exe"))
}

#[cfg(target_os = "windows")]
fn whatsapp_store_package_root() -> Option<PathBuf> {
    whatsapp_store_package_full_name().and_then(|full_name| package_path_from_full_name(&full_name))
}

#[cfg(target_os = "windows")]
fn whatsapp_store_package_full_name() -> Option<String> {
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS};
    use windows_sys::Win32::Storage::Packaging::Appx::GetPackagesByPackageFamily;

    let family = std::ffi::OsStr::new(whatsapp_store_package_family_name())
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mut count = 0u32;
    let mut buffer_length = 0u32;
    let first = unsafe {
        GetPackagesByPackageFamily(
            family.as_ptr(),
            &mut count,
            std::ptr::null_mut(),
            &mut buffer_length,
            std::ptr::null_mut(),
        )
    };
    if first != ERROR_INSUFFICIENT_BUFFER
        || count == 0
        || count > MAX_WHATSAPP_PACKAGE_COUNT
        || !(1..=MAX_WHATSAPP_PACKAGE_BUFFER_UNITS).contains(&buffer_length)
    {
        return None;
    }

    let mut package_names = vec![std::ptr::null_mut(); count as usize];
    let mut buffer = vec![0u16; buffer_length as usize];
    let second = unsafe {
        GetPackagesByPackageFamily(
            family.as_ptr(),
            &mut count,
            package_names.as_mut_ptr(),
            &mut buffer_length,
            buffer.as_mut_ptr(),
        )
    };
    if second != ERROR_SUCCESS
        || count == 0
        || count as usize > package_names.len()
        || buffer_length as usize > buffer.len()
    {
        return None;
    }

    let program_files = known_folder(KnownFolder::ProgramFiles)?;
    package_names[..count as usize]
        .iter()
        .find_map(|package_name| {
            utf16_string_from_api_buffer(*package_name, &buffer[..buffer_length as usize])
                .is_some_and(|full_name| {
                    whatsapp_package_registration_is_valid(&full_name, &program_files)
                })
                .then(|| {
                    utf16_string_from_api_buffer(*package_name, &buffer[..buffer_length as usize])
                })
                .flatten()
        })
}

/// Resolve new Outlook only through the current user's exact Microsoft Store
/// registration. The fixed package family and Windows package APIs prevent a
/// user-writable alias or same-named executable from entering the host path.
#[cfg(target_os = "windows")]
fn outlook_store_executable_path() -> Option<PathBuf> {
    let root = outlook_store_package_root()?;
    let executable = root.join("olk.exe");
    verify_executable(&executable, ExecutablePublisher::Microsoft)
        .ok()
        .map(|trusted| trusted.path().to_owned())
}

#[cfg(target_os = "windows")]
fn outlook_store_package_version() -> Option<String> {
    outlook_store_package_full_name()
        .and_then(|full_name| version_from_package_full_name(&full_name))
}

#[cfg(target_os = "windows")]
fn outlook_store_package_root() -> Option<PathBuf> {
    outlook_store_package_full_name().and_then(|full_name| package_path_from_full_name(&full_name))
}

#[cfg(target_os = "windows")]
fn outlook_store_package_full_name() -> Option<String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS};
    use windows_sys::Win32::Storage::Packaging::Appx::GetPackagesByPackageFamily;

    let family = std::ffi::OsStr::new(OUTLOOK_PACKAGE_FAMILY_NAME)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mut count = 0u32;
    let mut buffer_length = 0u32;
    if unsafe {
        GetPackagesByPackageFamily(
            family.as_ptr(),
            &mut count,
            std::ptr::null_mut(),
            &mut buffer_length,
            std::ptr::null_mut(),
        )
    } != ERROR_INSUFFICIENT_BUFFER
        || count == 0
        || count > MAX_WHATSAPP_PACKAGE_COUNT
        || !(1..=MAX_WHATSAPP_PACKAGE_BUFFER_UNITS).contains(&buffer_length)
    {
        return None;
    }
    let mut package_names = vec![std::ptr::null_mut(); count as usize];
    let mut buffer = vec![0u16; buffer_length as usize];
    if unsafe {
        GetPackagesByPackageFamily(
            family.as_ptr(),
            &mut count,
            package_names.as_mut_ptr(),
            &mut buffer_length,
            buffer.as_mut_ptr(),
        )
    } != ERROR_SUCCESS
        || count == 0
        || count as usize > package_names.len()
        || buffer_length as usize > buffer.len()
    {
        return None;
    }
    let program_files = known_folder(KnownFolder::ProgramFiles)?;
    package_names[..count as usize]
        .iter()
        .filter_map(|package_name| {
            utf16_string_from_api_buffer(*package_name, &buffer[..buffer_length as usize])
        })
        .find_map(|full_name| {
            let (name, publisher_id, resource_id) = package_identity_from_full_name(&full_name)?;
            if name != OUTLOOK_PACKAGE_NAME
                || publisher_id != OUTLOOK_PACKAGE_PUBLISHER_ID
                || !resource_id.is_empty()
            {
                return None;
            }
            let path = package_path_from_full_name(&full_name)?;
            is_trusted_outlook_package_path(&path, &program_files).then_some(full_name)
        })
}

#[cfg(any(target_os = "windows", test))]
fn is_trusted_outlook_package_path(path: &Path, program_files: &Path) -> bool {
    if path_has_parent_component(path) {
        return false;
    }
    let Some(package_directory_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    path.parent().is_some_and(|parent| {
        path_file_name_eq(parent, WINDOWS_APPS_DIRECTORY)
            && path_eq_ignore_ascii_case(parent.parent().unwrap_or(Path::new("")), program_files)
    }) && package_directory_name.starts_with(&format!("{OUTLOOK_PACKAGE_NAME}_"))
        && package_directory_name.ends_with(&format!("__{OUTLOOK_PACKAGE_PUBLISHER_ID}"))
}

#[cfg(not(target_os = "windows"))]
fn whatsapp_store_package_installed() -> bool {
    false
}

#[cfg(target_os = "windows")]
fn whatsapp_package_registration_is_valid(full_name: &str, program_files: &Path) -> bool {
    if !whatsapp_package_full_name_matches(full_name) {
        return false;
    }
    let Some(path) = package_path_from_full_name(full_name) else {
        return false;
    };
    is_trusted_whatsapp_package_path(&path, program_files)
}

#[cfg(any(target_os = "windows", test))]
fn whatsapp_package_full_name_matches(full_name: &str) -> bool {
    let parts = full_name.split('_').collect::<Vec<_>>();
    if parts.len() != 5
        || !whatsapp_package_identity_matches(parts[0], parts[4], parts[3])
        || !matches!(parts[2], "x64" | "x86" | "arm64" | "neutral")
    {
        return false;
    }
    let version = parts[1].split('.').collect::<Vec<_>>();
    version.len() == 4
        && version.iter().all(|component| {
            !component.is_empty() && component.bytes().all(|byte| byte.is_ascii_digit())
        })
}

#[cfg(target_os = "windows")]
fn package_identity_from_full_name(full_name: &str) -> Option<(String, String, String)> {
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER;
    use windows_sys::Win32::Storage::Packaging::Appx::{
        PackageIdFromFullName, PACKAGE_ID, PACKAGE_INFORMATION_BASIC,
    };

    let full_name = std::ffi::OsStr::new(full_name)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mut required = 0u32;
    if unsafe {
        PackageIdFromFullName(
            full_name.as_ptr(),
            PACKAGE_INFORMATION_BASIC,
            &mut required,
            std::ptr::null_mut(),
        )
    } != ERROR_INSUFFICIENT_BUFFER
        || !(std::mem::size_of::<PACKAGE_ID>() as u32..=MAX_WHATSAPP_PACKAGE_ID_BYTES)
            .contains(&required)
    {
        return None;
    }
    let words = (required as usize).div_ceil(std::mem::size_of::<usize>());
    let mut buffer = vec![0usize; words];
    let mut capacity = (words * std::mem::size_of::<usize>()) as u32;
    if unsafe {
        PackageIdFromFullName(
            full_name.as_ptr(),
            PACKAGE_INFORMATION_BASIC,
            &mut capacity,
            buffer.as_mut_ptr().cast::<u8>(),
        )
    } != 0
        || capacity as usize > words * std::mem::size_of::<usize>()
    {
        return None;
    }
    let id = unsafe { std::ptr::read_unaligned(buffer.as_ptr().cast::<PACKAGE_ID>()) };
    let utf16_buffer = unsafe {
        std::slice::from_raw_parts(
            buffer.as_ptr().cast::<u16>(),
            words * std::mem::size_of::<usize>() / std::mem::size_of::<u16>(),
        )
    };
    Some((
        utf16_string_from_api_buffer(id.name, utf16_buffer)?,
        utf16_string_from_api_buffer(id.publisherId, utf16_buffer)?,
        if id.resourceId.is_null() {
            String::new()
        } else {
            utf16_string_from_api_buffer(id.resourceId, utf16_buffer)?
        },
    ))
}

#[cfg(target_os = "windows")]
fn package_path_from_full_name(full_name: &str) -> Option<PathBuf> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    use windows_sys::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER;
    use windows_sys::Win32::Storage::Packaging::Appx::GetPackagePathByFullName;

    let full_name = std::ffi::OsStr::new(full_name)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mut required = 0u32;
    if unsafe { GetPackagePathByFullName(full_name.as_ptr(), &mut required, std::ptr::null_mut()) }
        != ERROR_INSUFFICIENT_BUFFER
        || !(2..=MAX_WHATSAPP_PACKAGE_BUFFER_UNITS).contains(&required)
    {
        return None;
    }
    let mut path = vec![0u16; required as usize];
    if unsafe { GetPackagePathByFullName(full_name.as_ptr(), &mut required, path.as_mut_ptr()) }
        != 0
        || required < 2
        || required as usize > path.len()
    {
        return None;
    }
    let end = path.iter().position(|unit| *unit == 0)?;
    (end > 0).then(|| PathBuf::from(std::ffi::OsString::from_wide(&path[..end])))
}

#[cfg(any(target_os = "windows", test))]
fn utf16_string_from_api_buffer(pointer: *const u16, buffer: &[u16]) -> Option<String> {
    if pointer.is_null() || buffer.is_empty() {
        return None;
    }
    let start = buffer.as_ptr() as usize;
    let byte_length = buffer.len().checked_mul(std::mem::size_of::<u16>())?;
    let end = start.checked_add(byte_length)?;
    let address = pointer as usize;
    if address < start
        || address >= end
        || !(address - start).is_multiple_of(std::mem::align_of::<u16>())
    {
        return None;
    }
    let offset = (address - start) / std::mem::size_of::<u16>();
    let remaining = &buffer[offset..];
    let nul = remaining.iter().position(|unit| *unit == 0)?;
    String::from_utf16(&remaining[..nul]).ok()
}

#[cfg(any(target_os = "windows", test))]
fn whatsapp_package_identity_matches(name: &str, publisher_id: &str, resource_id: &str) -> bool {
    name == WHATSAPP_PACKAGE_NAME
        && publisher_id == WHATSAPP_PACKAGE_PUBLISHER_ID
        && resource_id.is_empty()
}

#[cfg(any(target_os = "windows", test))]
fn is_trusted_whatsapp_package_path(path: &Path, program_files: &Path) -> bool {
    if path_has_parent_component(path) {
        return false;
    }
    let Some(package_directory_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    path.parent().is_some_and(|parent| {
        path_file_name_eq(parent, WINDOWS_APPS_DIRECTORY)
            && path_eq_ignore_ascii_case(parent.parent().unwrap_or(Path::new("")), program_files)
    }) && package_directory_name.starts_with(&format!("{WHATSAPP_PACKAGE_NAME}_"))
        && package_directory_name.ends_with(&format!("__{WHATSAPP_PACKAGE_PUBLISHER_ID}"))
}

#[cfg(target_os = "windows")]
fn installer_executable() -> Option<PathBuf> {
    let program_files = known_folder(KnownFolder::ProgramFiles)?;
    // The package APIs already bind this path to the current user's exact
    // Desktop App Installer registration. An unpackaged process can be denied
    // metadata reads inside WindowsApps even when CreateProcess is allowed, so
    // do not turn that redundant stat into a false "not installed" result.
    // A missing or damaged registration still fails closed when spawn runs.
    resolve_winget_executable(&app_installer_winget_candidates(), &program_files)
}

#[cfg(any(target_os = "windows", test))]
fn cached_installer_availability(
    cache: &Mutex<InstallerAvailabilityCache>,
    mut now: impl FnMut() -> Instant,
    probe: impl FnOnce() -> bool,
) -> bool {
    let mut state = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if state.verified {
        return true;
    }
    if state
        .retry_after_failure
        .is_some_and(|retry_after| now() < retry_after)
    {
        return false;
    }
    if !probe() {
        state.retry_after_failure = Some(now() + INSTALLER_FAILURE_RETRY_DELAY);
        return false;
    }
    state.verified = true;
    state.retry_after_failure = None;
    true
}

#[cfg(target_os = "windows")]
fn installer_available_for_listing() -> bool {
    cached_installer_availability(&VERIFIED_INSTALLER_AVAILABLE, Instant::now, || {
        installer_executable().is_some()
    })
}

#[cfg(not(target_os = "windows"))]
fn installer_available_for_listing() -> bool {
    false
}

/// Asks Windows for the signed App Installer package location. OSL never
/// executes the user-writable `winget.exe` App Execution Alias. The returned
/// path is data only and must pass `is_trusted_winget_path` before launch.
#[cfg(target_os = "windows")]
fn app_installer_winget_candidates() -> Vec<PathBuf> {
    let registered = registered_app_installer_winget_candidates();
    if known_folder(KnownFolder::ProgramFiles)
        .and_then(|program_files| resolve_winget_executable(&registered, &program_files))
        .is_some()
    {
        return registered;
    }
    let Some(system_directory) = system_directory() else {
        return Vec::new();
    };
    let powershell = system_directory
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    if !powershell.is_file() {
        return Vec::new();
    }
    let mut command = Command::new(powershell);
    command
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            APP_INSTALLER_LOCATION_SCRIPT,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    command_output_with_timeout(command, APP_INSTALLER_PROBE_TIMEOUT)
        .ok()
        .filter(|output| output.status.success())
        .map(|output| decode_app_installer_locations(&output.stdout))
        .unwrap_or_default()
}

/// Reads the current user's exact App Installer package registration through
/// the Windows package API. This avoids a cold PowerShell process on startup
/// while retaining the same protected WindowsApps path checks downstream.
#[cfg(target_os = "windows")]
fn registered_app_installer_winget_candidates() -> Vec<PathBuf> {
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS};
    use windows_sys::Win32::Storage::Packaging::Appx::GetPackagesByPackageFamily;

    let family = std::ffi::OsStr::new(DESKTOP_APP_INSTALLER_FAMILY_NAME)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mut count = 0u32;
    let mut buffer_length = 0u32;
    let first = unsafe {
        GetPackagesByPackageFamily(
            family.as_ptr(),
            &mut count,
            std::ptr::null_mut(),
            &mut buffer_length,
            std::ptr::null_mut(),
        )
    };
    if first != ERROR_INSUFFICIENT_BUFFER
        || count == 0
        || count > MAX_WHATSAPP_PACKAGE_COUNT
        || !(1..=MAX_WHATSAPP_PACKAGE_BUFFER_UNITS).contains(&buffer_length)
    {
        return Vec::new();
    }
    let mut package_names = vec![std::ptr::null_mut(); count as usize];
    let mut buffer = vec![0u16; buffer_length as usize];
    let second = unsafe {
        GetPackagesByPackageFamily(
            family.as_ptr(),
            &mut count,
            package_names.as_mut_ptr(),
            &mut buffer_length,
            buffer.as_mut_ptr(),
        )
    };
    if second != ERROR_SUCCESS
        || count == 0
        || count as usize > package_names.len()
        || buffer_length as usize > buffer.len()
    {
        return Vec::new();
    }
    let Some(windows_apps) = known_folder(KnownFolder::ProgramFiles)
        .map(|program_files| program_files.join(WINDOWS_APPS_DIRECTORY))
    else {
        return Vec::new();
    };
    package_names[..count as usize]
        .iter()
        .filter_map(|package_name| {
            let full_name =
                utf16_string_from_api_buffer(*package_name, &buffer[..buffer_length as usize])?;
            let (name, publisher_id, resource_id) = package_identity_from_full_name(&full_name)?;
            if name != "Microsoft.DesktopAppInstaller"
                || publisher_id != "8wekyb3d8bbwe"
                || !resource_id.is_empty()
            {
                return None;
            }
            // Package full names come from Windows itself and are parsed above
            // before being joined beneath the protected WindowsApps root. The
            // final candidate still passes `is_trusted_winget_path` before use.
            Some(windows_apps.join(full_name).join("winget.exe"))
        })
        .collect()
}

#[cfg(any(target_os = "windows", test))]
fn decode_app_installer_locations(stdout: &[u8]) -> Vec<PathBuf> {
    let Ok(stdout) = std::str::from_utf8(stdout) else {
        return Vec::new();
    };
    if stdout.contains('\0') {
        return Vec::new();
    }
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .map(|directory| directory.join("winget.exe"))
        .collect()
}

#[cfg(target_os = "windows")]
fn command_output_with_timeout(mut command: Command, timeout: Duration) -> std::io::Result<Output> {
    let mut child = command.spawn()?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output(),
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "App Installer probe timed out",
            ));
        }
        thread::sleep(Duration::from_millis(25));
    }
}

#[cfg(target_os = "windows")]
fn system_directory() -> Option<PathBuf> {
    let mut buffer = vec![0u16; 32_768];
    let length = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    (length > 0 && length < buffer.len())
        .then(|| PathBuf::from(std::ffi::OsString::from_wide(&buffer[..length])))
        .filter(|path| path.is_absolute())
}

/// Selects only App Installer's package executable from Windows' protected
/// application directory. User-profile aliases and PATH results are rejected.
#[cfg(any(target_os = "windows", test))]
fn resolve_winget_executable(candidates: &[PathBuf], program_files: &Path) -> Option<PathBuf> {
    candidates
        .iter()
        .find(|candidate| is_trusted_winget_path(candidate, program_files))
        .cloned()
}

#[cfg(any(target_os = "windows", test))]
fn is_trusted_winget_path(path: &Path, program_files: &Path) -> bool {
    if path_has_parent_component(path) {
        return false;
    }

    let Some(package_directory) = path.parent() else {
        return false;
    };
    let Some(windows_apps) = package_directory.parent() else {
        return false;
    };
    path_file_name_eq(path, "winget.exe")
        && path_file_name_eq(windows_apps, WINDOWS_APPS_DIRECTORY)
        && path_eq_ignore_ascii_case(
            windows_apps.parent().unwrap_or(Path::new("")),
            program_files,
        )
        && package_directory
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(is_desktop_app_installer_directory)
}

#[cfg(any(target_os = "windows", test))]
fn path_has_parent_component(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
}

#[cfg(any(target_os = "windows", test))]
fn path_eq_ignore_ascii_case(left: &Path, right: &Path) -> bool {
    left.as_os_str()
        .to_string_lossy()
        .eq_ignore_ascii_case(&right.as_os_str().to_string_lossy())
}

#[cfg(any(target_os = "windows", test))]
fn path_file_name_eq(path: &Path, expected: &str) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(expected))
}

#[cfg(any(target_os = "windows", test))]
fn is_desktop_app_installer_directory(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.starts_with(DESKTOP_APP_INSTALLER_PREFIX)
        && name.ends_with(DESKTOP_APP_INSTALLER_PUBLISHER_ID)
}

#[cfg(target_os = "windows")]
fn known_folder(folder: KnownFolder) -> Option<PathBuf> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{
        FOLDERID_LocalAppData, FOLDERID_ProgramFiles, FOLDERID_ProgramFilesX86,
        FOLDERID_RoamingAppData, SHGetKnownFolderPath, KF_FLAG_DEFAULT,
    };

    let folder_id = match folder {
        KnownFolder::Local => &FOLDERID_LocalAppData,
        KnownFolder::Roaming => &FOLDERID_RoamingAppData,
        KnownFolder::ProgramFiles => &FOLDERID_ProgramFiles,
        KnownFolder::ProgramFilesX86 => &FOLDERID_ProgramFilesX86,
    };
    let mut raw = std::ptr::null_mut();
    // SAFETY: SHGetKnownFolderPath initializes `raw` on success with a
    // NUL-terminated allocation owned by the COM task allocator. We scan only
    // to its first NUL and free that exact allocation once.
    let result = unsafe {
        SHGetKnownFolderPath(
            folder_id,
            KF_FLAG_DEFAULT as u32,
            std::ptr::null_mut(),
            &mut raw,
        )
    };
    if result < 0 || raw.is_null() {
        return None;
    }
    let mut length = 0usize;
    // SAFETY: successful SHGetKnownFolderPath returns a valid NUL-terminated
    // UTF-16 buffer.
    unsafe {
        while *raw.add(length) != 0 {
            length += 1;
        }
    }
    // SAFETY: the slice is confined to the measured allocation contents.
    let value = unsafe { std::slice::from_raw_parts(raw, length) };
    let path = PathBuf::from(OsString::from_wide(value));
    // SAFETY: `raw` came from SHGetKnownFolderPath and has not been freed yet.
    unsafe { CoTaskMemFree(raw.cast()) };
    Some(path)
}

#[cfg(target_os = "windows")]
fn spawn_detached(executable: &Path, arguments: &[&str]) -> std::io::Result<()> {
    Command::new(executable)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000)
        .spawn()
        .map(|_| ())
}

#[cfg(target_os = "windows")]
fn spawn_trusted_detached(
    executable: &TrustedExecutable,
    arguments: &[&str],
) -> std::io::Result<()> {
    Command::new(executable.path())
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000)
        .spawn()
        .map(|_| ())
}

#[cfg(target_os = "windows")]
fn spawn_firefox_tab(
    executable: &TrustedExecutable,
    profile: &Path,
    url: &str,
) -> std::io::Result<()> {
    Command::new(executable.path())
        .arg("--profile")
        .arg(profile)
        .arg("--new-tab")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

#[cfg(target_os = "windows")]
fn spawn_firefox_migration_wizard(
    executable: &TrustedExecutable,
    profile: &Path,
) -> std::io::Result<Child> {
    Command::new(executable.path())
        .arg("--no-remote")
        .arg("--profile")
        .arg(profile)
        .arg(FIREFOX_WAIT_FOR_BROWSER_SWITCH)
        .arg(FIREFOX_MIGRATION_SWITCH)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000)
        .spawn()
}

#[cfg(test)]
pub(crate) mod tests {
    use std::cell::Cell;
    use std::collections::BTreeSet;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    fn unique_discord_test_root(label: &str) -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "osl-native-apps-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn manifest_is_exhaustive_unique_and_uses_fixed_packages() {
        assert_eq!(NATIVE_APPS.len(), 5);
        for (index, app) in NATIVE_APPS.iter().enumerate() {
            assert!(!app.display_name.is_empty());
            assert_eq!(
                app.adapter_surface,
                AdapterSurface::InstalledNativeClient,
                "{:?} native inventory must bind to installed native adapter surface",
                app.id
            );
            // The public status is no longer a second reading of
            // `adapter_support`. The mapping that used to sit here sent BOTH
            // `Supported` and `Experimental` to `Beta` -- the collapse the owner
            // gate names (`PLAN.md` r4-5) -- and it made the label a function of
            // the adapter profile's internal support level, which is not a
            // public claim about anything. `decoupling` below proves the two are
            // now independent; asserting the map here would only restate it.
            assert!(
                !matches!(
                    (app.adapter_support, native_app_support_status(app.id)),
                    (SupportLevel::Experimental, NativeAppSupportStatus::Beta)
                        | (SupportLevel::Supported, NativeAppSupportStatus::Beta)
                ),
                "{:?} reproduces D-203: an adapter-profile support level promoted straight to a \
                 `Beta` public claim. `Beta` is reserved for runtime-proven / test-proven-only \
                 rows, and no carrier surface has earned one.",
                app.id
            );
            if app.id == NativeAppId::Outlook {
                assert!(app.package_id.is_empty());
                assert_eq!(app.package_source, "unavailable");
            } else {
                assert!(!app.package_id.is_empty());
                assert!(!app.package_id.starts_with('-'));
                assert!(app
                    .package_id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'.'));
                assert!(matches!(app.package_source, "winget" | "msstore"));
            }
            assert!(!app.candidates.is_empty());
            assert!(NATIVE_APPS[..index]
                .iter()
                .all(|previous| previous.id != app.id));
            for candidate in app.candidates {
                assert!(!candidate.relative_path.is_empty());
                assert!(!candidate.relative_path.starts_with(['/', '\\']));
                assert!(!candidate.relative_path.contains(".."));
                assert!(!candidate.relative_path.contains(':'));
                assert!(candidate
                    .relative_path
                    .to_ascii_lowercase()
                    .ends_with(".exe"));
            }
        }
        assert_eq!(manifest(NativeAppId::Discord).package_id, "Discord.Discord");
        assert_eq!(
            manifest(NativeAppId::Discord)
                .candidates
                .iter()
                .map(|candidate| candidate.relative_path)
                .collect::<Vec<_>>(),
            vec![
                r"Discord\Update.exe",
                r"DiscordPTB\Update.exe",
                r"DiscordCanary\Update.exe",
            ]
        );
        assert_eq!(
            &manifest(NativeAppId::Discord).adapter_service,
            &AdapterService::Discord
        );
        assert_eq!(
            &manifest(NativeAppId::Telegram).adapter_service,
            &AdapterService::Telegram
        );
        assert_eq!(
            &manifest(NativeAppId::Signal).adapter_service,
            &AdapterService::Signal
        );
        assert_eq!(
            &manifest(NativeAppId::Whatsapp).adapter_service,
            &AdapterService::Whatsapp
        );
        assert_eq!(
            &manifest(NativeAppId::Outlook).adapter_service,
            &AdapterService::Outlook
        );
        assert_eq!(manifest(NativeAppId::Whatsapp).package_source, "msstore");
        assert_eq!(
            native_app_publisher(NativeAppId::Discord),
            Some(ExecutablePublisher::Discord)
        );
        assert_eq!(
            native_app_publisher(NativeAppId::Telegram),
            Some(ExecutablePublisher::Telegram)
        );
        assert_eq!(
            native_app_publisher(NativeAppId::Signal),
            Some(ExecutablePublisher::Signal)
        );
        assert_eq!(native_app_publisher(NativeAppId::Whatsapp), None);
        assert_eq!(
            native_app_publisher(NativeAppId::Outlook),
            Some(ExecutablePublisher::Microsoft)
        );
        assert!(isolated_native_profile_available(NativeAppId::Discord));
        assert!(isolated_native_profile_available(NativeAppId::Telegram));
        assert!(!isolated_native_profile_available(NativeAppId::Signal));
        assert!(!isolated_native_profile_available(NativeAppId::Whatsapp));
        assert!(!isolated_native_profile_available(NativeAppId::Outlook));
    }

    #[test]
    fn telegram() {
        let telegram = manifest(NativeAppId::Telegram);

        assert_eq!(telegram.id, NativeAppId::Telegram);
        assert_eq!(telegram.display_name, "Telegram");
        assert_eq!(&telegram.adapter_service, &AdapterService::Telegram);
        assert_eq!(
            telegram.adapter_surface,
            AdapterSurface::InstalledNativeClient
        );
        assert_eq!(telegram.adapter_support, SupportLevel::ComingSoon);
        assert_eq!(telegram.package_id, "Telegram.TelegramDesktop");
        assert_eq!(telegram.package_source, "winget");
        assert_eq!(telegram.publisher, Some(ExecutablePublisher::Telegram));
        assert_eq!(
            telegram.candidates,
            &[
                ExecutableCandidate {
                    folder: KnownFolder::Roaming,
                    relative_path: r"Telegram Desktop\Telegram.exe",
                },
                ExecutableCandidate {
                    folder: KnownFolder::Local,
                    relative_path: r"Programs\Telegram Desktop\Telegram.exe",
                },
            ]
        );
        // D-206: the evidence-only derivation is `Experimental` -- the label the
        // product could not express -- and the support matrix still says
        // `externally blocked`. Conflicting authorities earn no claim.
        assert_eq!(
            crate::claim_state::derived_claim(
                crate::claim_state::claim_of(crate::claim_state::Surface::Telegram).carrier,
                crate::claim_state::claim_of(crate::claim_state::Surface::Telegram).delivery,
            ),
            crate::claim_state::PublicClaim::Experimental
        );
        assert_eq!(
            native_app_support_status(NativeAppId::Telegram),
            NativeAppSupportStatus::NoClaim
        );
        assert_eq!(
            native_app_protected_mode(NativeAppId::Telegram),
            NativeAppProtectedMode::Unavailable
        );
        assert!(isolated_native_profile_available(NativeAppId::Telegram));
    }

    #[test]
    fn outlook_native_identities_are_fixed_and_store_path_is_protected() {
        assert_eq!(
            OUTLOOK_PACKAGE_FAMILY_NAME,
            "Microsoft.OutlookForWindows_8wekyb3d8bbwe"
        );
        assert_eq!(
            OUTLOOK_PACKAGE_AUMID,
            "shell:AppsFolder\\Microsoft.OutlookForWindows_8wekyb3d8bbwe!Microsoft.OutlookforWindows"
        );
        assert_eq!(
            manifest(NativeAppId::Outlook)
                .candidates
                .iter()
                .map(|candidate| candidate.relative_path)
                .collect::<Vec<_>>(),
            vec![
                r"Microsoft Office\root\Office16\OUTLOOK.EXE",
                r"Microsoft Office\root\Office16\OUTLOOK.EXE",
            ]
        );
        let program_files = Path::new("C:/Program Files");
        let package = program_files
            .join("WindowsApps/Microsoft.OutlookForWindows_1.2026.707.300_x64__8wekyb3d8bbwe");
        assert!(is_trusted_outlook_package_path(&package, program_files));
        for rejected in [
            PathBuf::from(
                "C:/Users/alice/Microsoft.OutlookForWindows_1.2026.707.300_x64__8wekyb3d8bbwe",
            ),
            program_files.join(
                "WindowsApps/nested/Microsoft.OutlookForWindows_1.2026.707.300_x64__8wekyb3d8bbwe",
            ),
            program_files
                .join("WindowsApps/Microsoft.OutlookForWindows_1.2026.707.300_x64__attacker"),
        ] {
            assert!(!is_trusted_outlook_package_path(&rejected, program_files));
        }
    }

    #[test]
    fn task_4505_outlook_web_has_a_firefox_service_address() {
        assert_eq!(
            firefox_service_url(FirefoxServiceId::Outlook),
            "https://outlook.live.com/mail/"
        );
        println!(
            "TASK4505_OUTLOOK_WEB_FIREFOX_URL={}",
            firefox_service_url(FirefoxServiceId::Outlook)
        );
    }

    #[test]
    fn mullvad_actions_use_fixed_package_and_path() {
        assert_eq!(MULLVAD_PACKAGE_ID, "MullvadVPN.MullvadVPN");
        assert_eq!(
            mullvad_install_arguments(),
            [
                "install",
                "--id",
                "MullvadVPN.MullvadVPN",
                "--exact",
                "--source",
                "winget",
                "--accept-source-agreements",
                "--accept-package-agreements",
                "--silent",
            ]
        );
        assert_eq!(
            mullvad_open_candidates(),
            &[
                ExecutableCandidate {
                    folder: KnownFolder::ProgramFiles,
                    relative_path: r"Mullvad VPN\Mullvad VPN.exe",
                },
                ExecutableCandidate {
                    folder: KnownFolder::Local,
                    relative_path: r"Programs\Mullvad VPN\Mullvad VPN.exe",
                },
            ]
        );
        for candidate in mullvad_open_candidates() {
            assert!(!candidate.relative_path.starts_with(['/', '\\']));
            assert!(!candidate.relative_path.contains(".."));
            assert!(!candidate.relative_path.contains(':'));
            assert!(candidate.relative_path.ends_with(r"Mullvad VPN.exe"));
        }
        assert!(
            serde_json::to_value(MullvadActionResult { started: true }).unwrap()["started"]
                .as_bool()
                .unwrap()
        );
    }

    #[test]
    fn native_apps() {
        let status = NativeAppStatus {
            id: NativeAppId::Discord,
            display_name: "Discord",
            availability: NativeAppAvailability::Installed,
            support_status: NativeAppSupportStatus::NoClaim,
            carrier_evidence: "builtNeverProvenLive",
            delivery_evidence: "neverProvenLive",
            claim_blockers: vec!["open-security-finding", "unknown-recheck-required"],
            claim_note: "OSL has never carried a message through Discord and back in a recorded \
                         two-party run.",
            status_page: NativeAppStatusPageData {
                capability: "carrier capability is wired but not live-proven",
                generated_label: "Not claimed",
                explanation: "OSL has never carried a message through Discord and back in a \
                              recorded two-party run.",
            },
            protected_mode: NativeAppProtectedMode::AssistOnly,
            isolated_profile_available: true,
            supports_overlay: false,
        };

        let json = serde_json::to_value(status).unwrap();
        assert_eq!(json["id"], "discord");
        assert_eq!(json["availability"], "installed");
        assert_eq!(json["supportStatus"], "noClaim");
        assert_eq!(json["protectedMode"], "assistOnly");
        assert_eq!(json["isolatedProfileAvailable"], true);
        assert_eq!(
            json["supportsOverlay"], false,
            "native app support must not imply overlay support"
        );
    }

    #[test]
    fn dedicated_discord_fallback_is_one_fixed_official_channel() {
        assert_eq!(DISCORD_DEDICATED_PACKAGE_ID, "Discord.Discord.PTB");

        // This used to assert `install_discord_dedicated_channel().is_err()`,
        // which is not a property anyone wants: it held only because the dev
        // box has no winget. On a runner that has winget the call succeeds --
        // and really starts installing Discord PTB. Assert the invariant in
        // the name instead, on the command we would issue, with no installer
        // executed and nothing about the local machine involved.
        let arguments = discord_dedicated_install_arguments();
        assert_eq!(
            arguments,
            [
                "install",
                "--id",
                "Discord.Discord.PTB",
                "--exact",
                "--source",
                "winget",
                "--accept-source-agreements",
                "--accept-package-agreements",
                "--silent",
            ]
        );
        // Exactly one channel, pinned by exact id against the official source.
        assert_eq!(
            arguments
                .iter()
                .filter(|argument| argument.starts_with("Discord."))
                .count(),
            1,
            "never offer a second Discord channel"
        );
        assert!(
            arguments.contains(&"--exact"),
            "an inexact id could resolve to a different package"
        );
        assert_eq!(
            arguments
                .iter()
                .position(|argument| *argument == "--source"),
            Some(4)
        );
        assert_eq!(arguments[5], "winget", "only the official winget source");
    }

    #[test]
    fn native_app_status_serializes_the_public_support_contract_without_implying_overlay() {
        let status = NativeAppStatus {
            id: NativeAppId::Discord,
            display_name: "Discord",
            availability: NativeAppAvailability::Installed,
            support_status: NativeAppSupportStatus::NoClaim,
            carrier_evidence: "builtNeverProvenLive",
            delivery_evidence: "neverProvenLive",
            claim_blockers: vec!["open-security-finding", "unknown-recheck-required"],
            claim_note: "OSL has never carried a message through Discord and back in a recorded \
                         two-party run.",
            status_page: NativeAppStatusPageData {
                capability: "carrier capability is wired but not live-proven",
                generated_label: "Not claimed",
                explanation: "OSL has never carried a message through Discord and back in a \
                              recorded two-party run.",
            },
            protected_mode: NativeAppProtectedMode::AssistOnly,
            isolated_profile_available: true,
            supports_overlay: false,
        };

        let json = serde_json::to_value(status).unwrap();
        let object = json.as_object().unwrap();
        let keys = object.keys().map(String::as_str).collect::<BTreeSet<_>>();
        assert_eq!(
            keys,
            BTreeSet::from([
                "availability",
                "carrierEvidence",
                "claimBlockers",
                "claimNote",
                "deliveryEvidence",
                "displayName",
                "id",
                "isolatedProfileAvailable",
                "protectedMode",
                "statusPage",
                "supportStatus",
                "supportsOverlay"
            ]),
            "the public contract must ship the REASON with the label. A badge alone cannot \
             distinguish `measured and refused` from `never built`, and shipping only the badge \
             is how those two states collapse."
        );
        assert_eq!(json["id"], "discord");
        assert_eq!(json["availability"], "installed");
        assert_eq!(json["supportStatus"], "noClaim");
        assert_eq!(
            json["statusPage"]["capability"],
            "carrier capability is wired but not live-proven"
        );
        assert_eq!(json["statusPage"]["generatedLabel"], "Not claimed");
        assert_eq!(json["statusPage"]["explanation"], json["claimNote"]);
        assert_eq!(json["protectedMode"], "assistOnly");
        assert_eq!(json["isolatedProfileAvailable"], true);
        assert_eq!(json["supportsOverlay"], false);
    }

    #[test]
    fn native_app_support_status_does_not_follow_install_availability() {
        let statuses = list_native_apps_with_installer_probe(|| true);
        assert_eq!(statuses.len(), NATIVE_APPS.len());

        for status in statuses {
            match status.id {
                // D-203 is DISCHARGED as a label. Two allowlist extinguishers
                // stand on Discord at once -- `support-matrix.json` carries D6
                // as `open-security-finding`, and master §9's "current tree
                // recheck required" is allowlist rule 5's
                // `unknown-recheck-required` -- and §E maps both to **no badge,
                // no claim**. `Beta` was an overclaim, `ComingSoon` said Discord
                // was planned when it is the one carrier the app enables, and
                // `ExternallyBlocked` said a third party blocks us. None of the
                // three was true; `NoClaim` is.
                NativeAppId::Discord => {
                    assert_eq!(status.support_status, NativeAppSupportStatus::NoClaim);
                    assert_eq!(status.protected_mode, NativeAppProtectedMode::AssistOnly);
                    assert_eq!(status.carrier_evidence, "builtNeverProvenLive");
                }
                // D-206's held label. The evidence supports `Experimental` and
                // `support-matrix.json` still records Telegram as
                // `externally_blocked`; those are different assertions, not
                // different strengths, so allowlist rule 5's "conflicting"
                // clause applies and the answer is no claim.
                NativeAppId::Telegram => {
                    assert_eq!(status.support_status, NativeAppSupportStatus::NoClaim);
                    assert_eq!(status.protected_mode, NativeAppProtectedMode::Unavailable);
                }
                NativeAppId::Signal | NativeAppId::Whatsapp | NativeAppId::Outlook => {
                    assert_eq!(status.support_status, NativeAppSupportStatus::ComingSoon);
                    assert_eq!(status.protected_mode, NativeAppProtectedMode::Unavailable);
                }
            }

            // Whatever the label, every row ships the sentence that says what
            // OSL has and has not proven there. `PLAN.md` r4-6.
            assert!(
                !status.claim_note.is_empty(),
                "{:?} ships a label with no reason line",
                status.id
            );
        }
    }

    #[test]
    fn task_0852_direct_status_data_names_real_capability_and_matching_generated_label() {
        let statuses = list_native_apps_with_installer_probe(|| true);
        assert_eq!(statuses.len(), NATIVE_APPS.len());
        println!("task_0852_direct_status_data_count={}", statuses.len());

        for status in statuses {
            let surface = claim_surface(status.id);
            let claim = crate::claim_state::claim_of(surface);
            let direct = &status.status_page;
            let expected_capability =
                crate::claim_state::capability_name(claim.carrier, claim.delivery);
            let expected_label = crate::claim_state::public_claim(surface).label();

            assert_eq!(
                direct.capability, expected_capability,
                "{:?} status-page capability must be derived from the same carrier/delivery facts \
                 as its tile label",
                status.id
            );
            assert_eq!(
                direct.generated_label, expected_label,
                "{:?} status-page generated label must match the generated tile label",
                status.id
            );
            assert_eq!(
                direct.explanation, status.claim_note,
                "{:?} status-page explanation must not be a second source of truth",
                status.id
            );

            println!(
                "task_0852_status_page {} capability=\"{}\" generated_label=\"{}\" support_status={}",
                claim.surface.ruling_slug(),
                direct.capability,
                direct.generated_label,
                crate::claim_state::public_claim(surface).slug()
            );
        }
    }

    /// **The public claim and the adapter profile's support level are
    /// independent.** This is the property the old hand-written map destroyed:
    /// it made the label a function of `adapter_support`, so a provider could
    /// not be described except by moving a value that means something else.
    ///
    /// Falsifiable by construction — it fails the moment the mapping is
    /// reinstated, because a reinstated mapping makes equal support levels
    /// produce equal labels.
    #[test]
    fn the_public_claim_is_not_a_function_of_the_adapter_support_level() {
        let mut by_level: std::collections::BTreeMap<String, BTreeSet<String>> = Default::default();
        for app in NATIVE_APPS {
            by_level
                .entry(format!("{:?}", app.adapter_support))
                .or_default()
                .insert(format!("{:?}", native_app_support_status(app.id)));
        }
        assert!(
            by_level.values().any(|labels| labels.len() > 1),
            "every adapter support level maps to exactly one public label, so the label is still \
             a function of the adapter profile rather than of the evidence: {by_level:?}"
        );

        // And the reverse: distinct evidence must not be flattened into one
        // label across the whole native inventory.
        let labels: BTreeSet<String> = NATIVE_APPS
            .iter()
            .map(|app| format!("{:?}", native_app_support_status(app.id)))
            .collect();
        assert!(
            labels.len() > 1,
            "every native app carries the same public label: {labels:?}"
        );
    }

    // ---------------------------------------------------------------------
    // The publication gate. Rewritten after D-206 refuted its first version.
    //
    // The first version registered a proof: a fn pointer that drove `RecordedHost`,
    // an in-repo fake whose `value_of` returns exactly what `set_value` stored. The
    // Adversary published WhatsApp in 27 lines by wrapping assertions that already
    // passed, and both gate tests went green. That is this project's founding
    // failure -- a test that supplies its own composer -- reproduced inside the
    // guard built to prevent it.
    //
    // So there is now NO HOST AT GATE TIME, fake or otherwise, and nothing to
    // register. Leaving `ComingSoon` requires a receipt that only a live run
    // against the real client can produce, and the receipt is bound by content
    // hash to the adapter and substrate sources that produced it, so it goes stale
    // the moment either changes.
    //
    // What this still cannot do, stated rather than papered over: it cannot prove a
    // human did not hand-write the file. What it can do is make that a fabricated
    // artifact committed to the repository -- visible in review and in history,
    // and needing re-fabrication on every adapter edit -- instead of a test wrapper
    // that reuses green assertions.
    // ---------------------------------------------------------------------

    /// How a native app's carrier actually reaches the provider.
    ///
    /// D-206 found the previous scope derivation was **spelling**:
    /// `source.contains("host: &dyn Uia2Syscalls")`, which
    /// `native_discord_adapter.rs:99` already escapes by writing the path-qualified
    /// `&dyn crate::native_a11y::Uia2Syscalls`. A zero-behaviour import cleanup
    /// would have flipped the scan and panicked the gate.
    ///
    /// This map is instead **exhaustive over `NativeAppId`**, so a new native app
    /// cannot compile without a seam decision, and each `Uia2Substrate` claim is
    /// bound by [`seam_bindings`] to a real function of the right signature, so a
    /// declaration that corresponds to no code does not compile either. Formatting,
    /// import style, `&impl`, generic bounds and parameter names are all now
    /// irrelevant to it.
    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    pub(crate) enum CarrySeam {
        /// `native_a11y`'s `Uia2Syscalls`: resolve a window, list editables, write
        /// one value, read it back.
        Uia2Substrate,
        /// Discord's `NativeWindowHostState` accessibility target.
        NativeWindowHost,
        /// A placement backend the provider's own adapter defines.
        ProviderOwnedBackend,
        /// No carrier path is wired at all.
        NoCarryPath,
    }

    pub(crate) const fn carry_seam(id: NativeAppId) -> CarrySeam {
        match id {
            NativeAppId::Discord => CarrySeam::NativeWindowHost,
            NativeAppId::Telegram => CarrySeam::Uia2Substrate,
            NativeAppId::Signal => CarrySeam::ProviderOwnedBackend,
            NativeAppId::Whatsapp => CarrySeam::Uia2Substrate,
            NativeAppId::Outlook => CarrySeam::NoCarryPath,
        }
    }

    /// Compile-time evidence for every seam declared above. These are `const`
    /// coercions, not calls: if the named entry point does not exist with that
    /// exact shape, this module does not build, and the seam map cannot claim
    /// something the code does not have.
    mod seam_bindings {
        use crate::native_a11y::Uia2Syscalls;

        /// `NativeAppId::Telegram => Uia2Substrate`
        const _TELEGRAM_TAKES_THE_SUBSTRATE:
            for<'a, 'b> fn(
                &'a dyn Uia2Syscalls,
                crate::native_telegram_adapter::TelegramLivePlacementRequest<'b>,
            )
                -> crate::native_telegram_adapter::TelegramLivePlacementReceipt =
            crate::native_telegram_adapter::drive_telegram_composer_placement;

        /// `NativeAppId::Whatsapp => Uia2Substrate`
        const _WHATSAPP_TAKES_THE_SUBSTRATE:
            for<'a, 'b> fn(
                &'a dyn Uia2Syscalls,
                &'b str,
                bool,
            )
                -> crate::native_whatsapp_adapter::WhatsAppLivePlacementReceipt =
            crate::native_whatsapp_adapter::drive_whatsapp_composer_placement;
    }

    /// Native apps published above `ComingSoon` with **no live carry receipt**.
    ///
    /// This is a recorded debt, not an exemption, and it is size-locked: the gate
    /// fails if it grows, and fails if a member quietly earns a receipt without
    /// being removed from it. Discord is here because `support-matrix.json` carries
    /// it as `not-qualified` / `unavailable` while `native_apps.rs` ships it as
    /// `Experimental`, which is **D-203** — and Discord is the one carrier a friend
    /// uses today, so the contradiction is not academic.
    const PUBLISHED_WITHOUT_A_LIVE_RECEIPT: &[(NativeAppId, &str)] =
        &[(NativeAppId::Discord, "D-203")];

    /// **The shrink obligation.** D-203's own correction: *"That exemption must
    /// shrink, not persist, and it must not become the place future
    /// contradictions are parked."*
    ///
    /// The debt is size-locked in **three** places that must agree --  this
    /// constant, the list above, and `carry-receipts/debt-baseline.json` --  so
    /// adding a name is a three-file edit that no diff can hide, and each entry
    /// must carry a defect id in both the code and the baseline. Shrinking is a
    /// three-file edit too, and that is deliberate: this number may only ever be
    /// **lowered**, and lowering it is the act that records the debt as paid.
    ///
    /// Stated plainly rather than overclaimed: nothing in the compiler can stop
    /// someone editing all three. What the lock buys is that growth cannot happen
    /// *quietly*, and the current list is printed in every failure this gate can
    /// produce.
    const RECEIPT_DEBT_CEILING: usize = 1;

    /// The one command an operator runs when the seam changes.
    const FLEET_RERUN_COMMAND: &str =
        "cargo test --manifest-path apps/osl-hub/Cargo.toml --lib -- \
                                       --exact --nocapture \
                                       native_apps::tests::carry_receipt_fleet_rerun_report";

    /// The recorded debt, rendered for a human. Appended to every failure this
    /// gate can produce, so the by-name exemption list is never invisible in the
    /// output that matters.
    fn receipt_debt_summary() -> String {
        let mut out = format!(
            "PUBLISHED_WITHOUT_A_LIVE_RECEIPT ({} of a ceiling of {RECEIPT_DEBT_CEILING}):",
            PUBLISHED_WITHOUT_A_LIVE_RECEIPT.len()
        );
        if PUBLISHED_WITHOUT_A_LIVE_RECEIPT.is_empty() {
            out.push_str(" empty -- every published provider has earned a live receipt.");
            return out;
        }
        for (id, defect) in PUBLISHED_WITHOUT_A_LIVE_RECEIPT {
            out.push_str(&format!(
                "\n  - {id:?} published on no live receipt, against {defect}"
            ));
        }
        out
    }

    /// **The publication gate.**
    ///
    /// A native app may not be published above `ComingSoon` without a live carry
    /// receipt earned against the real client. Stated as an implication over the
    /// manifest, so it says nothing about which providers *should* be published and
    /// cannot be satisfied by editing it to agree with them.
    #[test]
    fn no_native_app_is_published_above_coming_soon_without_an_earned_live_receipt() {
        use carry_receipt::{receipt_path, verify_receipt, ReceiptVerdict};

        let mut debts_seen = Vec::new();
        for manifest in NATIVE_APPS {
            let seam = carry_seam(manifest.id);
            let debt = PUBLISHED_WITHOUT_A_LIVE_RECEIPT
                .iter()
                .find(|(id, _)| *id == manifest.id);
            let verdict = verify_receipt(manifest.id, seam);

            if manifest.adapter_support == SupportLevel::ComingSoon {
                // Not published: a receipt is optional, but a receipt that is
                // present must still be valid, or a stale one would sit here
                // rotting until the day it is relied on.
                if let ReceiptVerdict::Invalid(why) = verdict {
                    panic!(
                        "{:?} is still ComingSoon, but the receipt at {} was never sound: {why}. \
                         Delete it or re-earn it -- do not leave a broken proof in the tree.\n{}",
                        manifest.id,
                        receipt_path(manifest.id).display(),
                        receipt_debt_summary()
                    );
                }
                continue;
            }

            if let Some((_, defect)) = debt {
                debts_seen.push(manifest.id);
                assert!(
                    matches!(verdict, ReceiptVerdict::Absent),
                    "{:?} is recorded in PUBLISHED_WITHOUT_A_LIVE_RECEIPT against {defect}, but a \
                     receipt now exists. Remove it from that list AND lower \
                     RECEIPT_DEBT_CEILING and debt-baseline.json -- a recorded debt that has been \
                     paid must not stay recorded, or the list stops meaning anything.\n{}",
                    manifest.id,
                    receipt_debt_summary()
                );
                continue;
            }

            match verdict {
                ReceiptVerdict::Earned => {}
                ReceiptVerdict::EarnedWithSubstrateDrift(note) => eprintln!(
                    "carry-receipt: {:?} is earned, with substrate drift -- {note}",
                    manifest.id
                ),
                ReceiptVerdict::Absent => panic!(
                    "{:?} is published as {:?}, not ComingSoon, and there is no live carry receipt \
                     at {}. A provider may not be shown to a user as supported until its adapter \
                     has been driven against the real client. Earn one:\n  \
                     cargo test --manifest-path apps/osl-hub/Cargo.toml --lib \
                     --target x86_64-pc-windows-gnu --no-run\n  \
                     <exe> --ignored --test-threads=1 --nocapture \
                     native_telegram_adapter::tests::carry_real_cover_text_through_live_telegram\n{}",
                    manifest.id,
                    manifest.adapter_support,
                    receipt_path(manifest.id).display(),
                    receipt_debt_summary()
                ),
                ReceiptVerdict::Stale(why) => panic!(
                    "{:?} is published as {:?} but its live carry receipt is stale: {why}\n{}",
                    manifest.id,
                    manifest.adapter_support,
                    receipt_debt_summary()
                ),
                ReceiptVerdict::Invalid(why) => panic!(
                    "{:?} is published as {:?} but its live carry receipt is not usable: {why}\n{}",
                    manifest.id,
                    manifest.adapter_support,
                    receipt_debt_summary()
                ),
            }
        }

        let recorded: Vec<NativeAppId> = PUBLISHED_WITHOUT_A_LIVE_RECEIPT
            .iter()
            .map(|(id, _)| *id)
            .collect();
        assert_eq!(
            debts_seen,
            recorded,
            "PUBLISHED_WITHOUT_A_LIVE_RECEIPT must name exactly the published apps that have no \
             receipt. An entry that no longer applies is a stale exemption; one that has been \
             added is a provider published on nothing.\n{}",
            receipt_debt_summary()
        );
    }

    /// **The shrink obligation, mechanically.** The by-name debt list may lose
    /// members freely; it may not gain one without three files agreeing and a
    /// defect id in two of them.
    #[test]
    fn the_receipt_debt_is_size_locked_and_every_entry_names_a_defect() {
        let baseline: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("carry-receipts/debt-baseline.json"),
            )
            .expect("carry-receipts/debt-baseline.json is readable"),
        )
        .expect("the debt baseline parses");

        assert_eq!(
            baseline["schema"].as_str(),
            Some("osl-receipt-debt-baseline-v1"),
            "the debt baseline schema changed"
        );

        // The baseline's own shape, per the plan's one-schema-for-every-ratchet
        // rule: `ids` is required and `count` must equal its length, so a count
        // can never be pinned while the set moves underneath it.
        let ids: Vec<String> = baseline["ids"]
            .as_array()
            .expect("the baseline names its ids")
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .expect("every baseline id is a string")
                    .to_owned()
            })
            .collect();
        assert_eq!(
            baseline["count"].as_u64(),
            Some(ids.len() as u64),
            "the debt baseline pins a count that does not match its own id list"
        );

        let mut code_ids: Vec<String> = PUBLISHED_WITHOUT_A_LIVE_RECEIPT
            .iter()
            .map(|(id, _)| carry_receipt::provider_slug(*id).to_owned())
            .collect();
        let mut sorted_ids = ids.clone();
        code_ids.sort();
        sorted_ids.sort();
        assert_eq!(
            code_ids,
            sorted_ids,
            "PUBLISHED_WITHOUT_A_LIVE_RECEIPT and carry-receipts/debt-baseline.json disagree about \
             who is published on nothing.\n{}",
            receipt_debt_summary()
        );
        assert_eq!(
            code_ids.len(),
            {
                let mut deduped = code_ids.clone();
                deduped.dedup();
                deduped.len()
            },
            "a provider is recorded in the debt list twice"
        );

        for (id, defect) in PUBLISHED_WITHOUT_A_LIVE_RECEIPT {
            let slug = carry_receipt::provider_slug(*id);
            assert!(
                defect.starts_with("D-") && defect[2..].chars().all(|c| c.is_ascii_digit()),
                "{id:?} is exempted against {defect:?}, which is not a recorded defect id. An \
                 entry may only be added with the defect that justifies it."
            );
            assert_eq!(
                baseline["defects"][slug].as_str(),
                Some(*defect),
                "{id:?} is exempted against {defect:?} in code, and the baseline disagrees. A debt \
                 must be recorded in both places or in neither."
            );
        }

        assert_eq!(
            baseline["ceiling"].as_u64(),
            Some(RECEIPT_DEBT_CEILING as u64),
            "the debt ceiling in code and in the baseline disagree"
        );
        assert_eq!(
            PUBLISHED_WITHOUT_A_LIVE_RECEIPT.len(),
            RECEIPT_DEBT_CEILING,
            "the debt list and its ceiling must track exactly, so growth needs a deliberate bump \
             and a shrink forces the ceiling DOWN. This number may only ever be lowered.\n{}",
            receipt_debt_summary()
        );
    }

    /// The gate above is only as good as its ability to reject. Drive every
    /// rejection path against a receipt built to fail it.
    #[test]
    fn the_receipt_verifier_rejects_every_way_a_receipt_can_be_wrong() {
        use carry_receipt::{verify_receipt_bytes, ReceiptVerdict};

        let sound = carry_receipt::sample_sound_receipt();
        assert!(
            matches!(
                verify_receipt_bytes(NativeAppId::Telegram, &sound.to_json()),
                ReceiptVerdict::Earned
            ),
            "the sample receipt must be accepted, or every rejection below is vacuous"
        );

        for (mutate, expect) in carry_receipt::rejection_cases() {
            let mut broken = carry_receipt::sample_sound_receipt();
            mutate(&mut broken);
            match verify_receipt_bytes(NativeAppId::Telegram, &broken.to_json()) {
                ReceiptVerdict::Invalid(why) | ReceiptVerdict::Stale(why) => assert!(
                    why.contains(expect),
                    "expected a rejection mentioning {expect:?}, got {why:?}"
                ),
                other => panic!("a receipt mutated to break {expect:?} was accepted: {other:?}"),
            }
        }
    }

    /// The v1 spec that changed meaning instead of disappearing: a substrate file
    /// that has moved while every declaration this adapter consumes has not is
    /// **drift** -- earned, reported, not fatal. Asserted exactly, so it can be
    /// neither a silent `Earned` nor a `Stale`.
    #[test]
    fn a_moved_substrate_with_an_unmoved_seam_is_drift_and_says_so() {
        use carry_receipt::{verify_receipt_bytes, ReceiptVerdict};

        for (mutate, expect) in carry_receipt::substrate_drift_cases() {
            let mut drifted = carry_receipt::sample_sound_receipt();
            mutate(&mut drifted);
            match verify_receipt_bytes(NativeAppId::Telegram, &drifted.to_json()) {
                ReceiptVerdict::EarnedWithSubstrateDrift(note) => assert!(
                    note.contains(expect),
                    "expected a drift note mentioning {expect:?}, got {note:?}"
                ),
                other => panic!(
                    "a substrate file hash that moved while the seam did not must be reported as \
                     drift, not as {other:?}"
                ),
            }
        }
    }

    /// A v1 receipt is `Stale`, not `Invalid` and never `Earned`: the run was
    /// sound, the binding rule changed under it. Asserted exactly in all three
    /// directions so the superseded schema cannot become a quiet pass.
    #[test]
    fn a_receipt_earned_under_the_superseded_v1_binding_is_stale() {
        use carry_receipt::{verify_receipt_bytes, ReceiptVerdict};

        let mut v1 = carry_receipt::sample_sound_receipt();
        v1.schema = "osl-live-carry-receipt-v1".to_owned();
        match verify_receipt_bytes(NativeAppId::Telegram, &v1.to_json()) {
            ReceiptVerdict::Stale(why) => assert!(
                why.contains("superseded") && why.contains("seam-contract binding"),
                "{why}"
            ),
            other => panic!("a v1 receipt must be stale, not {other:?}"),
        }
    }

    /// **The two directions, on the real sources.**
    ///
    /// The fixtures in `carry_seam_contract` prove the extraction rules; this
    /// proves the property that matters on `native_a11y.rs` and
    /// `native_telegram_adapter.rs` as they actually stand, so an extractor that
    /// happens to work on a 50-line fixture and not on a 4,500-line substrate is
    /// a failure here.
    ///
    /// Each mutant is required to change the source it is applied to, so a
    /// `replace` that stopped matching is a failure and not a silent pass.
    #[test]
    fn the_real_substrate_survives_a_refactor_and_not_a_seam_change() {
        use crate::carry_seam_contract::seam_contract_from_sources;

        let adapter_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/native_telegram_adapter.rs");
        let substrate_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(carry_receipt::SUBSTRATE_SOURCE);
        let adapter = std::fs::read_to_string(adapter_path).expect("the adapter is readable");
        let substrate = std::fs::read_to_string(substrate_path).expect("the substrate is readable");

        let base = seam_contract_from_sources(&adapter, &substrate)
            .expect("the real Telegram seam contract computes");

        // (1) BEHAVIOUR-PRESERVING. A comment, a renamed local, a reflowed
        // signature. `uia2_settle_plan`'s body is the substrate's own poll ladder
        // walk -- exactly the kind of code the placement-primitive work rewrites.
        let refactored = substrate
            .replace(
                "    let mut plan = Vec::new();\n    let mut spent = 0u64;\n    let mut rung = 0usize;",
                "    // A comment added by a refactor that changes nothing observable.\n    \
                 let mut waits = Vec::new();\n    let mut elapsed = 0u64;\n    let mut step_index = 0usize;",
            )
            .replace("while spent < budget_ms {", "while elapsed < budget_ms {")
            .replace(
                "UIA2_SETTLE_LADDER_MS[rung.min(UIA2_SETTLE_LADDER_MS.len() - 1)]",
                "UIA2_SETTLE_LADDER_MS[step_index.min(UIA2_SETTLE_LADDER_MS.len() - 1)]",
            )
            .replace("let step = step.min(budget_ms - spent);", "let step = step.min(budget_ms - elapsed);")
            .replace("        spent += step;\n        plan.push(step);\n        rung += 1;", "        elapsed += step;\n        waits.push(step);\n        step_index += 1;")
            .replace("    plan\n}", "    waits\n}")
            .replace(
                "pub fn uia2_settle_plan(budget_ms: u64) -> Vec<u64> {",
                "pub fn uia2_settle_plan(\n    budget_ms: u64,\n) -> Vec<u64> {",
            );
        assert_ne!(
            refactored, substrate,
            "the refactor mutant matched nothing; it is measuring nothing"
        );
        let after_refactor = seam_contract_from_sources(&adapter, &refactored)
            .expect("the contract still computes after a refactor");
        assert_eq!(
            base.sha256,
            after_refactor.sha256,
            "a behaviour-preserving refactor of {} invalidated the seam contract. At 12-14 \
             published providers that is 12-14 live signed-in Windows runs for a renamed local.",
            carry_receipt::SUBSTRATE_SOURCE
        );

        // (2) THE SEAM ITSELF. Each of these is something a provider can observe.
        for (label, mutated) in [
            (
                "a parameter of Uia2Syscalls::set_value renamed",
                substrate.replace(
                    "        element: &Uia2Editable,\n        value: &str,\n        deadline: Uia2Deadline,\n    ) -> Result<bool, Uia2CallTimeout>;",
                    "        element: &Uia2Editable,\n        carrier: &str,\n        deadline: Uia2Deadline,\n    ) -> Result<bool, Uia2CallTimeout>;",
                ),
            ),
            (
                "the return type of Uia2Syscalls::value_of changed",
                substrate.replace(
                    "    ) -> Result<Option<String>, Uia2CallTimeout>;",
                    "    ) -> Result<Option<Box<str>>, Uia2CallTimeout>;",
                ),
            ),
            (
                "a field of Uia2Editable removed",
                substrate.replace("    pub keyboard_focusable: bool,\n", ""),
            ),
        ] {
            assert_ne!(
                mutated, substrate,
                "{label}: the mutant matched nothing, so it proves nothing"
            );
            let after = seam_contract_from_sources(&adapter, &mutated)
                .expect("the contract computes after a seam change");
            assert_ne!(
                base.sha256, after.sha256,
                "{label} did not move the seam contract. A receipt that never invalidates is worse \
                 than one that over-invalidates."
            );
        }
    }

    /// **The blind spot, measured rather than claimed.**
    ///
    /// The seam binding sees declarations. A substrate edit that changes only the
    /// *inside* of a function this adapter calls is invisible to it -- and the
    /// settle ladder is the sharpest example available: `UIA2_SETTLE_LADDER_MS` is
    /// walked by `uia2_settle_plan` inside `acquire_uia2_window`, which Telegram
    /// calls, but Telegram never names the constant, so it is not in Telegram's
    /// contract.
    ///
    /// This test exists so that boundary is a **recorded measurement with a
    /// mutant behind it**, not a sentence in a doc comment. It asserts both
    /// halves: the contract does not move, **and** the whole-file hash does, which
    /// is what makes the receipt read `EarnedWithSubstrateDrift` and puts the
    /// change in front of an operator instead of dropping it.
    ///
    /// If a future lane wants this class caught by the gate rather than reported
    /// by it, the honest move is a behavioural pin on the ladder in
    /// `native_a11y`'s own tests -- not widening the contract to the transitive
    /// closure of every body, which would make extracting a helper function
    /// invalidate 12-14 live proofs and reintroduce the trap this replaced.
    #[test]
    fn what_the_seam_binding_cannot_see_is_reported_as_drift_and_named_here() {
        use crate::carry_seam_contract::{seam_contract_from_sources, sha256_hex};

        let adapter = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/native_telegram_adapter.rs"),
        )
        .expect("the adapter is readable");
        let substrate = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(carry_receipt::SUBSTRATE_SOURCE),
        )
        .expect("the substrate is readable");
        let base = seam_contract_from_sources(&adapter, &substrate).expect("the contract computes");

        for (label, mutated) in [(
            "the settle ladder walked inside acquire_uia2_window, which Telegram never names",
            substrate.replace(
                "&[150, 300, 500, 1_000, 2_000, 5_000, 10_000]",
                "&[150, 300, 500, 1_000, 2_000, 5_000, 20_000]",
            ),
        )] {
            assert_ne!(
                mutated, substrate,
                "{label}: the mutant matched nothing, so it measures nothing"
            );
            let after =
                seam_contract_from_sources(&adapter, &mutated).expect("the contract computes");
            assert_eq!(
                base.sha256, after.sha256,
                "{label} DOES move the seam contract; this blind spot has closed and this test \
                 should be promoted into the directional one above"
            );
            assert_ne!(
                sha256_hex(substrate.as_bytes()),
                sha256_hex(mutated.as_bytes()),
                "{label} must at least move the whole-file hash, or the drift signal would not \
                 report it either and the change really would be invisible"
            );
        }
    }

    /// **The seam contract is not vacuous.** Computed against the real tree, for
    /// the one provider that has earned a live receipt.
    ///
    /// The anchors below are declaration names, not behaviour claims: they are a
    /// floor that fails loudly if the extractor stops resolving the substrate,
    /// which would otherwise show up as "every receipt suddenly agrees".
    #[test]
    fn the_telegram_seam_contract_binds_the_substrate_the_adapter_actually_uses() {
        let contract = carry_receipt::seam_contract(NativeAppId::Telegram)
            .expect("the Telegram seam contract computes");
        assert!(
            contract.items.len() >= crate::carry_seam_contract::SEAM_CONTRACT_MIN_ITEMS,
            "the Telegram seam resolved only {} declarations",
            contract.items.len()
        );
        for anchor in [
            "trait Uia2Syscalls",
            "fn set_value",
            "fn value_of",
            "fn place_uia2_carrier",
            "fn clear_uia2_composer",
            "struct Uia2Editable",
        ] {
            assert!(
                contract.rendered.contains(anchor),
                "the Telegram seam contract does not contain {anchor:?}; the extractor is \
                 measuring less than the adapter consumes"
            );
        }
        assert!(
            !contract.rendered.contains("let mut plan = Vec::new();"),
            "a function body reached the seam contract, so a renamed local would invalidate every \
             receipt -- which is the trap this rebinding exists to remove"
        );
    }

    /// **The fleet re-run job.** One command that lists exactly which providers
    /// must be re-proven, instead of discovering it provider by provider:
    ///
    /// ```text
    /// cargo test --manifest-path apps/osl-hub/Cargo.toml --lib -- \
    ///   --exact --nocapture native_apps::tests::carry_receipt_fleet_rerun_report
    /// ```
    ///
    /// It is not a report that cannot fail. It covers every declared app, and
    /// every row's action is re-derived here from `verify_receipt` over the real
    /// tree, so a report that stopped looking is a failure and not a quiet blank
    /// line.
    ///
    /// **What this check is and is not, stated honestly (D-240).** Since the
    /// report's verdict is now *derived* from the verifier rather than computed
    /// beside it, this loop is a WIRING check: it proves the derivation is still
    /// wired and that the verdict-to-action map is faithful, not that two
    /// independent readings agree. It was the "independent" version that lied --
    /// the second route asked a weaker question. The teeth that actually refuse
    /// a lying report are in
    /// [`the_fleet_report_cannot_call_a_receipt_ok_that_the_verifier_refuses`],
    /// which drives receipt bytes the tree does not contain.
    #[test]
    fn carry_receipt_fleet_rerun_report() {
        use carry_receipt::ReceiptVerdict;

        let rows = fleet_report();
        eprintln!("{}", render_fleet_report(&rows));

        assert_eq!(
            rows.len(),
            NATIVE_APPS.len(),
            "the fleet report does not cover every native app"
        );

        for row in &rows {
            let verdict = carry_receipt::verify_receipt(row.id, carry_seam(row.id));
            let agrees = match row.action {
                FleetAction::Ok => matches!(verdict, ReceiptVerdict::Earned),
                FleetAction::SubstrateDrift => {
                    matches!(verdict, ReceiptVerdict::EarnedWithSubstrateDrift(_))
                }
                FleetAction::MustRerun | FleetAction::RerunBeforePublishing => {
                    matches!(verdict, ReceiptVerdict::Stale(_))
                }
                FleetAction::ContractUnavailable | FleetAction::Unusable => {
                    matches!(verdict, ReceiptVerdict::Invalid(_))
                }
                FleetAction::MustEarn
                | FleetAction::DebtRecorded
                | FleetAction::NoReceiptNotPublished => {
                    matches!(verdict, ReceiptVerdict::Absent)
                }
            };
            assert!(
                agrees,
                "the fleet report calls {:?} {:?}, but the gate's own verifier says {verdict:?}. \
                 The report and the gate must not be able to disagree about who has to re-run.",
                row.id, row.action
            );
        }
    }

    /// One row's rendered line from the real operator-facing report, so the
    /// assertions below read the SENTENCE A HUMAN READS rather than the enum
    /// behind it. D-240 was a defect in the sentence.
    fn rendered_row(id: NativeAppId, seam: CarrySeam, action: FleetAction, detail: &str) -> String {
        render_fleet_report(&[FleetRow {
            id,
            published: false,
            support: SupportLevel::ComingSoon,
            seam,
            action,
            detail: detail.to_owned(),
        }])
    }

    /// The action column of a rendered fleet-report line, by position rather
    /// than by substring, so "does the table say Ok" cannot be answered by a
    /// word that happened to appear in the detail text.
    fn rendered_action_word(rendered: &str) -> String {
        rendered
            .lines()
            .find(|line| line.starts_with("  telegram"))
            .unwrap_or_else(|| panic!("no provider row in the rendered report:\n{rendered}"))
            .split_whitespace()
            .nth(3)
            .expect("the rendered row has an action column")
            .to_owned()
    }

    /// **D-240, THE REGRESSION.** The fleet report may not print `Ok` for a
    /// receipt the verifier refuses -- and may not buy that by refusing
    /// everything.
    ///
    /// Driven over receipt BYTES, not over the tree: every rejection the
    /// verifier knows is pushed through the real classifier and the real
    /// renderer, and the assertion is on the printed action column. Before the
    /// fix, `byte_exact: false` alone printed
    /// `telegram ... Ok  seam 06a86bda0d0b over 43 declarations` while
    /// `verify_receipt` on the same bytes said `Invalid`.
    ///
    /// The three positive cases are what stops the fix being "make everything
    /// fail": a sound receipt still prints `Ok`, a substrate-drifted one still
    /// prints `SubstrateDrift` and still counts as sound, and a superseded v1
    /// receipt still prints as a re-run rather than as never-sound.
    #[test]
    fn the_fleet_report_cannot_call_a_receipt_ok_that_the_verifier_refuses() {
        let id = NativeAppId::Telegram;
        let seam = carry_seam(id);

        // POSITIVE 1: a receipt that is sound for the tree as it stands prints Ok.
        let sound_json = carry_receipt::sample_sound_receipt().to_json();
        let (action, detail) = classify_fleet_row(id, seam, false, None, Some(sound_json.as_str()));
        assert_eq!(
            action,
            FleetAction::Ok,
            "a sound receipt must still print Ok, or the report was fixed by making it useless: \
             {detail}"
        );
        let rendered = rendered_row(id, seam, action, &detail);
        assert_eq!(rendered_action_word(&rendered), "Ok", "\n{rendered}");
        assert!(
            crate::seam_ledger::classify(action, false, seam).1,
            "ledger 9 must still read a sound receipt as sound"
        );

        // POSITIVE 2: substrate drift is earned, reported, and NOT fatal.
        for (mutate, _) in carry_receipt::substrate_drift_cases() {
            let mut drifted = carry_receipt::sample_sound_receipt();
            mutate(&mut drifted);
            let drifted_json = drifted.to_json();
            let (action, detail) =
                classify_fleet_row(id, seam, false, None, Some(drifted_json.as_str()));
            assert_eq!(action, FleetAction::SubstrateDrift, "{detail}");
            assert!(crate::seam_ledger::classify(action, false, seam).1);
        }

        // POSITIVE 3: a superseded v1 receipt is a re-run, not a never-sound
        // receipt, and whether it is urgent depends on publication.
        let mut v1 = carry_receipt::sample_sound_receipt();
        v1.schema = "osl-live-carry-receipt-v1".to_owned();
        let v1 = v1.to_json();
        assert_eq!(
            classify_fleet_row(id, seam, true, None, Some(v1.as_str())).0,
            FleetAction::MustRerun
        );
        assert_eq!(
            classify_fleet_row(id, seam, false, None, Some(v1.as_str())).0,
            FleetAction::RerunBeforePublishing
        );

        // THE DEFECT ITSELF: every way a receipt can be wrong, read off the
        // table a human reads.
        let mut checked = 0usize;
        for (mutate, phrase) in carry_receipt::rejection_cases() {
            let mut broken = carry_receipt::sample_sound_receipt();
            mutate(&mut broken);
            let json = broken.to_json();

            let verdict = carry_receipt::verify_receipt_text(id, seam, &json);
            assert!(
                !verdict.is_earned(),
                "the mutation for {phrase:?} did not change anything the verifier refuses; a \
                 mutant that mutates nothing proves nothing"
            );

            let (action, detail) = classify_fleet_row(id, seam, false, None, Some(json.as_str()));
            assert!(
                !matches!(action, FleetAction::Ok | FleetAction::SubstrateDrift),
                "the fleet report calls a receipt the verifier rates {verdict:?} {action:?}. This \
                 is D-240: the gate refuses and the report says it passed. ({phrase})"
            );

            let rendered = rendered_row(id, seam, action, &detail);
            let word = rendered_action_word(&rendered);
            assert_ne!(
                word, "Ok",
                "the rendered fleet report prints Ok for a receipt rated {verdict:?} \
                 ({phrase}):\n{rendered}"
            );
            assert_eq!(
                word,
                format!("{action:?}"),
                "the report printed a different action from the one it decided:\n{rendered}"
            );

            let (present, sound, _, violation) = crate::seam_ledger::classify(action, false, seam);
            assert!(
                present,
                "the receipt is on disk in this scenario ({phrase})"
            );
            assert!(
                !sound,
                "ledger 9 prints `sound yes` for a receipt rated {verdict:?} ({phrase})"
            );
            assert!(
                violation.is_some(),
                "ledger 9 records no violation for a receipt rated {verdict:?} ({phrase})"
            );
            checked += 1;
        }
        assert!(
            checked >= 20,
            "only {checked} rejection case(s) reached the report; the case table shrank"
        );

        // And the absent-receipt branches still say the three different things
        // they mean, so "no receipt" cannot be printed as a proof either.
        assert!(matches!(
            classify_fleet_row(id, seam, false, None, None).0,
            FleetAction::NoReceiptNotPublished
        ));
        assert!(matches!(
            classify_fleet_row(id, seam, true, None, None).0,
            FleetAction::MustEarn
        ));
        assert!(matches!(
            classify_fleet_row(id, seam, true, Some("D-203"), None).0,
            FleetAction::DebtRecorded
        ));

        // Non-JSON on disk is the fatal class, never a blank row.
        let (action, _) = classify_fleet_row(id, seam, false, None, Some("not json"));
        assert!(matches!(
            action,
            FleetAction::Unusable | FleetAction::ContractUnavailable
        ));
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum FleetAction {
        /// Earned, seam unchanged, substrate file unchanged.
        Ok,
        /// Earned, seam unchanged, `native_a11y.rs` edited. Not fatal; read the
        /// note before assuming the behaviour behind the declarations held.
        SubstrateDrift,
        /// Published, and the seam it was proven against has changed. **This is
        /// the re-run list.**
        MustRerun,
        /// Same, but the provider is still `ComingSoon`, so it costs nothing
        /// today and everything on the day it is published.
        RerunBeforePublishing,
        /// A receipt exists and its contract cannot be computed at all.
        ContractUnavailable,
        /// A receipt exists and was never sound -- wrong schema, unparseable, or
        /// otherwise rejected outright. Fatal wherever it is found, including at
        /// `ComingSoon`.
        Unusable,
        /// Published with no receipt and no recorded debt.
        MustEarn,
        /// Published with no receipt, against a recorded defect.
        DebtRecorded,
        NoReceiptNotPublished,
    }

    pub(crate) struct FleetRow {
        pub id: NativeAppId,
        pub published: bool,
        pub support: SupportLevel,
        pub seam: CarrySeam,
        pub action: FleetAction,
        pub detail: String,
    }

    /// Every native app the manifest declares, in manifest order.
    ///
    /// `NATIVE_APPS` is private to this module and stays that way. Binding
    /// Ledger 9 (`crate::seam_ledger`) needs the declared POPULATION to prove
    /// its own coverage -- a ledger that silently stopped seeing a provider
    /// would report a clean set difference over an empty set, which is the
    /// failure `scripts/ledger/lib/io.mjs:124-135` already names.
    pub(crate) fn declared_native_apps() -> Vec<NativeAppId> {
        NATIVE_APPS.iter().map(|manifest| manifest.id).collect()
    }

    /// **D-240. The report's verdict is DERIVED from the verifier -- it is not a
    /// second opinion about the same bytes.**
    ///
    /// This function used to classify every provider *without* going through
    /// `verify_receipt`, on the theory that two independent routes make the
    /// cross-check meaningful. They do -- but only if the two routes ask the
    /// same question. They did not. This one read three fields (`schema`,
    /// `seam_contract_sha256`, `substrate_source_sha256`); the verifier reads
    /// eighteen. So the report was a strictly WEAKER predicate wearing the
    /// verifier's name, and a receipt with `byte_exact: false` on disk printed
    ///
    /// ```text
    ///   telegram   ComingSoon   uia2_substrate   Ok   seam 06a86bda0d0b over 43 declarations
    /// ```
    ///
    /// while `verify_receipt` rated the same bytes `Invalid`. The gate was
    /// right and the sentence a human reads was wrong, which is worse than a
    /// wrong gate: the sentence is what gets quoted into a tasklog. The lane
    /// that found it nearly mis-reported its own mutation proof.
    ///
    /// So the classification is now a **total map from `ReceiptVerdict`**, and
    /// this function decides only the three things the verdict genuinely does
    /// not know: whether the provider is published, whether a missing receipt is
    /// recorded as debt, and which words to print. `detail` now carries the
    /// verifier's own reason, so the report says *why* instead of only *what*.
    ///
    /// The teeth moved with it, they were not removed: see
    /// [`the_fleet_report_cannot_call_a_receipt_ok_that_the_verifier_refuses`],
    /// which drives real receipt bytes through this classifier and reads the
    /// rendered table.
    pub(crate) fn fleet_report() -> Vec<FleetRow> {
        NATIVE_APPS
            .iter()
            .map(|manifest| {
                let id = manifest.id;
                let seam = carry_seam(id);
                let published = manifest.adapter_support != SupportLevel::ComingSoon;
                let debt = PUBLISHED_WITHOUT_A_LIVE_RECEIPT
                    .iter()
                    .find(|(other, _)| *other == id)
                    .map(|(_, defect)| *defect);
                let raw = std::fs::read_to_string(carry_receipt::receipt_path(id)).ok();

                let (action, detail) =
                    classify_fleet_row(id, seam, published, debt, raw.as_deref());

                FleetRow {
                    id,
                    published,
                    support: manifest.adapter_support,
                    seam,
                    action,
                    detail,
                }
            })
            .collect()
    }

    /// **The whole classification, over bytes rather than over the tree.**
    ///
    /// Pure with respect to the receipt: hand it the receipt text and it answers
    /// exactly what the fleet report and Ledger 9 will print. That is what lets
    /// the D-240 regression test drive a tampered receipt through the *real*
    /// report without writing a tampered file into the tree -- and what makes
    /// "the report agrees with the verifier" a property of the code rather than
    /// a hope checked once per provider that happens to exist today.
    pub(crate) fn classify_fleet_row(
        id: NativeAppId,
        seam: CarrySeam,
        published: bool,
        debt: Option<&str>,
        raw: Option<&str>,
    ) -> (FleetAction, String) {
        use carry_receipt::ReceiptVerdict;

        let Some(text) = raw else {
            return match (published, debt) {
                (true, Some(defect)) => (
                    FleetAction::DebtRecorded,
                    format!("published on no live receipt, against {defect}"),
                ),
                (true, None) => (
                    FleetAction::MustEarn,
                    "published with no receipt and no recorded debt".to_owned(),
                ),
                (false, _) => (
                    FleetAction::NoReceiptNotPublished,
                    "not published; no receipt earned yet".to_owned(),
                ),
            };
        };

        // ONE derivation. Everything below chooses a label for a verdict it did
        // not compute; nothing below can turn a refusal into a pass.
        match carry_receipt::verify_receipt_text(id, seam, text) {
            ReceiptVerdict::Earned => (
                FleetAction::Ok,
                match carry_receipt::seam_contract(id) {
                    Ok(contract) => format!(
                        "seam {} over {} declarations",
                        short(&contract.sha256),
                        contract.items.len()
                    ),
                    // Unreachable: `Earned` already required this contract to
                    // compute. Reported rather than unwrapped, because a report
                    // that panics is a report nobody reads.
                    Err(why) => format!("earned, but the seam contract no longer computes: {why}"),
                },
            ),
            ReceiptVerdict::EarnedWithSubstrateDrift(note) => (FleetAction::SubstrateDrift, note),
            ReceiptVerdict::Stale(why) => (
                if published {
                    FleetAction::MustRerun
                } else {
                    FleetAction::RerunBeforePublishing
                },
                why,
            ),
            // `ContractUnavailable` and `Unusable` are the same fatal class to
            // `seam_ledger::classify`; the split exists only so the operator can
            // tell "this receipt was never sound" from "this tree cannot even
            // compute the seam". Choosing between two labels for an already-fatal
            // verdict cannot make anything pass.
            ReceiptVerdict::Invalid(why) => (
                if carry_receipt::seam_contract(id).is_err() {
                    FleetAction::ContractUnavailable
                } else {
                    FleetAction::Unusable
                },
                why,
            ),
            // `verify_receipt_text` is handed bytes, so it cannot answer
            // `Absent`. If it ever does, the receipt is on disk and unreadable to
            // the verifier, which is the fatal class -- never the quiet
            // "no receipt here" row.
            ReceiptVerdict::Absent => (
                FleetAction::Unusable,
                "the verifier reported no receipt for bytes that are on disk".to_owned(),
            ),
        }
    }

    fn short(hash: &str) -> String {
        hash.chars().take(12).collect()
    }

    pub(crate) fn render_fleet_report(rows: &[FleetRow]) -> String {
        let mut out = String::from(
            "\nCARRY RECEIPT FLEET -- what must be re-proven against a live client\n\
             ------------------------------------------------------------------\n",
        );
        for row in rows {
            out.push_str(&format!(
                "  {:<10} {:<14} {:<22} {:<22} {}\n",
                carry_receipt::provider_slug(row.id),
                // The public claim, derived. Printing `adapter_support` for a
                // published row and the literal "ComingSoon" for everything
                // else was the fleet report's copy of the collapse.
                crate::claim_state::public_claim(claim_surface(row.id)).slug(),
                carry_receipt::seam_slug(row.seam),
                format!("{:?}", row.action),
                row.detail
            ));
        }
        let must: Vec<&str> = rows
            .iter()
            .filter(|row| row.action == FleetAction::MustRerun)
            .map(|row| carry_receipt::provider_slug(row.id))
            .collect();
        let later: Vec<&str> = rows
            .iter()
            .filter(|row| row.action == FleetAction::RerunBeforePublishing)
            .map(|row| carry_receipt::provider_slug(row.id))
            .collect();
        out.push_str(&format!(
            "\nMUST RE-RUN NOW (published, seam changed): {}\n",
            if must.is_empty() {
                "none".to_owned()
            } else {
                must.join(", ")
            }
        ));
        out.push_str(&format!(
            "RE-RUN BEFORE PUBLISHING (ComingSoon, seam changed): {}\n",
            if later.is_empty() {
                "none".to_owned()
            } else {
                later.join(", ")
            }
        ));
        out.push_str(&format!("{}\n", receipt_debt_summary()));
        out
    }

    /// **One-time v1 -> v2 migration, and it cannot mint a receipt.**
    ///
    /// A v2 receipt records the seam contract the live run was measured against.
    /// For a receipt already in the tree that value is *recoverable* rather than
    /// invented -- but only if the run's own bindings still hold exactly: the
    /// adapter and the whole substrate file must hash to what the receipt already
    /// records, which means the tree has not moved since the run, which means the
    /// contract extracted now is the contract that run would have written.
    ///
    /// Every one of those conditions is a refusal, and the function reads an
    /// existing receipt rather than creating one, so it cannot be used to
    /// manufacture a proof for a provider that has never been driven. `#[ignore]`
    /// because it writes to the tree.
    #[test]
    #[ignore = "writes a receipt file; run deliberately, once, per receipt"]
    fn migrate_a_v1_receipt_to_the_seam_binding() {
        let id = std::env::var("OSL_MIGRATE_RECEIPT").unwrap_or_else(|_| "telegram".to_owned());
        let id = NATIVE_APPS
            .iter()
            .map(|manifest| manifest.id)
            .find(|candidate| carry_receipt::provider_slug(*candidate) == id)
            .unwrap_or_else(|| panic!("OSL_MIGRATE_RECEIPT={id} is not a native app"));

        let path = carry_receipt::receipt_path(id);
        let text = std::fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!(
                "there is no receipt at {} to migrate ({error}). This migration reads an existing \
                 live proof; it cannot create one.",
                path.display()
            )
        });
        let mut value: serde_json::Value =
            serde_json::from_str(&text).expect("the receipt to migrate parses");

        assert_eq!(
            value["schema"].as_str(),
            Some("osl-live-carry-receipt-v1"),
            "only a v1 receipt can be migrated"
        );
        let adapter = carry_receipt::adapter_source(id).expect("the provider has an adapter");
        assert_eq!(
            value["adapter_source_sha256"].as_str(),
            Some(carry_receipt::source_sha256(adapter).as_str()),
            "the adapter has changed since this receipt was earned, so the seam it was measured \
             against is not recoverable. Re-run the live carry instead."
        );
        assert_eq!(
            value["substrate_source_sha256"].as_str(),
            Some(carry_receipt::source_sha256(carry_receipt::SUBSTRATE_SOURCE).as_str()),
            "the substrate has changed since this receipt was earned, so the seam it was measured \
             against is not recoverable. Re-run the live carry instead."
        );

        let contract =
            carry_receipt::seam_contract(id).expect("the seam contract computes for this tree");
        value["schema"] = serde_json::Value::String(carry_receipt::RECEIPT_SCHEMA.to_owned());
        value["seam_contract_sha256"] = serde_json::Value::String(contract.sha256.clone());
        value["seam_contract_items"] = serde_json::Value::from(contract.items.len());
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&value).expect("the migrated receipt serialises") + "\n",
        )
        .expect("the receipt is writable");
        eprintln!(
            "migrated {} to {} -- seam contract {} over {} declarations",
            path.display(),
            carry_receipt::RECEIPT_SCHEMA,
            contract.sha256,
            contract.items.len()
        );
    }

    /// The seam map is exhaustive by construction; assert it is also *populated*,
    /// so a future refactor that collapses every arm to one value is visible.
    #[test]
    fn the_carry_seam_map_distinguishes_the_providers() {
        assert_eq!(carry_seam(NativeAppId::Telegram), CarrySeam::Uia2Substrate);
        assert_eq!(carry_seam(NativeAppId::Whatsapp), CarrySeam::Uia2Substrate);
        assert_eq!(
            carry_seam(NativeAppId::Discord),
            CarrySeam::NativeWindowHost
        );
        assert_eq!(
            carry_seam(NativeAppId::Signal),
            CarrySeam::ProviderOwnedBackend
        );
        assert_eq!(carry_seam(NativeAppId::Outlook), CarrySeam::NoCarryPath);
    }

    /// **D-203's unguarded axis.** The lane's first guard bound TS to Rust, which
    /// was never in dispute; the change travelled Rust -> `support-matrix.json`,
    /// which nothing watched.
    ///
    /// The relation is deliberately one-directional: Rust may not sit in the
    /// *claim* tier while the matrix sits in the *no-claim* tier. Equality would be
    /// the wrong assertion — the two files answer different questions and the
    /// matrix is the more conservative one.
    #[test]
    fn rust_never_claims_more_than_the_public_support_matrix() {
        let matrix: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(repo_path("docs/status/support-matrix.json"))
                .expect("support-matrix.json is readable"),
        )
        .expect("support-matrix.json parses");

        // Every public status the matrix states for a native app, from all three
        // places it states them, so a change to one section cannot hide behind
        // another.
        let mut stated: std::collections::BTreeMap<String, Vec<String>> = Default::default();
        collect_matrix_statuses(&matrix, &mut stated);
        assert!(
            stated.len() >= 3,
            "read {} services out of the matrix; the shape has changed and this guard is \
             measuring nothing",
            stated.len()
        );

        let claim_tier = |status: &str| matches!(status, "available" | "beta" | "verified_live");

        /// The matrix slug for a public claim. Exhaustive on purpose: a new
        /// label cannot be added without deciding what the matrix comparison
        /// makes of it.
        fn matrix_slug(status: NativeAppSupportStatus) -> &'static str {
            match status {
                NativeAppSupportStatus::Available => "available",
                NativeAppSupportStatus::Beta => "beta",
                // Master §8.2 badge, and NOT a capability claim: allowlist §B
                // permits it for exactly these services, and it is absent from
                // `check-app-claims.mjs` FORBIDDEN_PROMOTION_STATUSES while
                // appearing in its limitation-language set.
                NativeAppSupportStatus::Experimental => "experimental",
                NativeAppSupportStatus::ComingSoon => "coming_soon",
                NativeAppSupportStatus::ExternallyBlocked => "externally_blocked",
                // Allowlist §E: no badge, no claim. Weaker than every matrix
                // state, so it can never contradict one.
                NativeAppSupportStatus::NoClaim => "no_claim",
            }
        }

        let mut contradictions = Vec::new();
        for manifest in NATIVE_APPS {
            let rust_public = matrix_slug(native_app_support_status(manifest.id));
            let Some(matrix_states) = stated.get(manifest.display_name) else {
                continue;
            };
            if claim_tier(rust_public) && !matrix_states.iter().any(|s| claim_tier(s)) {
                contradictions.push(format!(
                    "{} is {rust_public} in native_apps.rs but {:?} in support-matrix.json",
                    manifest.display_name, matrix_states
                ));
            }
        }

        /// **Native apps whose in-app claim is knowingly stronger than the
        /// public support matrix. Empty, and it used to hold Discord.**
        ///
        /// D-203's contradiction was "`Beta` in `native_apps.rs`, `unavailable`
        /// / `not-qualified` in the matrix", and it existed because the public
        /// label was a function of `adapter_support`. `claim_state` derives it
        /// from evidence instead, and Discord's two extinguishing blockers put
        /// it at **no claim** -- weaker than the matrix, not stronger.
        ///
        /// This is deliberately NOT `PUBLISHED_WITHOUT_A_LIVE_RECEIPT`. That
        /// list records a different fact -- a provider published above
        /// `ComingSoon` with no receipt -- and Discord is still on it, because
        /// it still has no receipt. Deriving one from the other is what made
        /// this assertion read as "the label is fine because the debt is
        /// recorded". They are separate, and both must shrink on their own.
        const RECORDED_MATRIX_OVERCLAIMS: &[&str] = &[];

        let expected: Vec<String> = RECORDED_MATRIX_OVERCLAIMS
            .iter()
            .filter_map(|name| {
                let m = NATIVE_APPS.iter().find(|m| m.display_name == *name)?;
                stated.get(m.display_name).map(|states| {
                    format!(
                        "{} is {} in native_apps.rs but {:?} in support-matrix.json",
                        m.display_name,
                        matrix_slug(native_app_support_status(m.id)),
                        states
                    )
                })
            })
            .collect();

        assert_eq!(
            contradictions, expected,
            "Rust and the public support matrix disagree in a way that is not the one already \
             recorded as D-203. The matrix feeds the claim gate; a provider that claims more in \
             code than the matrix allows is a public overclaim."
        );
    }

    fn collect_matrix_statuses(
        matrix: &serde_json::Value,
        out: &mut std::collections::BTreeMap<String, Vec<String>>,
    ) {
        let versioned = &matrix["versioned_public_support_matrix"];
        for section in ["entries", "rows"] {
            for row in versioned[section].as_array().into_iter().flatten() {
                if let (Some(service), Some(status)) =
                    (row["service"].as_str(), row["public_status"].as_str())
                {
                    out.entry(service.to_owned())
                        .or_default()
                        .push(status.to_owned());
                }
            }
        }
        for app in matrix["chat_app_qualification_evidence"]["apps"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if let (Some(name), Some(status)) =
                (app["display_name"].as_str(), app["public_status"].as_str())
            {
                out.entry(name.to_owned())
                    .or_default()
                    .push(status.to_owned());
            }
        }
    }

    fn repo_path(relative: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(relative)
    }

    /// Live carry receipts: the only thing that may lift a provider above
    /// `ComingSoon`.
    ///
    /// A receipt is written **by a live run against the real client** and read
    /// **by the gate**, with no host in between. It is bound by content hash to
    /// the adapter and substrate sources that produced it, so editing either one
    /// invalidates every receipt earned before the edit -- which is the property
    /// that stops a proof from being a thing you register once.
    pub(crate) mod carry_receipt {
        use super::{CarrySeam, NativeAppId, FLEET_RERUN_COMMAND};
        use std::path::{Path, PathBuf};

        /// **v2 rebinds the substrate half of the receipt.** v1 bound a receipt to
        /// `native_a11y.rs`'s bytes, so one edit to the shared substrate
        /// invalidated every provider's proof at once and demanded a live signed-in
        /// Windows run per published provider before any merge. At the owner's
        /// ruled surface list -- Discord, Signal, WhatsApp, Telegram and ~10 email
        /// surfaces -- that made the substrate unrefactorable. v2 binds to the
        /// **seam contract** instead (`crate::carry_seam_contract`), keeps the
        /// whole-file hash as a *drift* signal, and is a hard schema break so no
        /// v1 receipt can be read under v2 rules.
        pub(crate) const RECEIPT_SCHEMA: &str = "osl-live-carry-receipt-v2";

        pub(crate) fn provider_slug(id: NativeAppId) -> &'static str {
            match id {
                NativeAppId::Discord => "discord",
                NativeAppId::Telegram => "telegram",
                NativeAppId::Signal => "signal",
                NativeAppId::Whatsapp => "whatsapp",
                NativeAppId::Outlook => "outlook",
            }
        }

        /// The adapter module whose source a provider's receipt is bound to.
        /// `None` means the provider has no adapter module of its own, so no
        /// receipt can be bound and none may be written.
        pub(crate) fn adapter_source(id: NativeAppId) -> Option<&'static str> {
            match id {
                NativeAppId::Discord => Some("src/native_discord_adapter.rs"),
                NativeAppId::Telegram => Some("src/native_telegram_adapter.rs"),
                NativeAppId::Signal => Some("src/native_signal_adapter.rs"),
                NativeAppId::Whatsapp => Some("src/native_whatsapp_adapter.rs"),
                NativeAppId::Outlook => None,
            }
        }

        pub(crate) const SUBSTRATE_SOURCE: &str = crate::carry_seam_contract::SUBSTRATE_SOURCE;

        fn crate_path(relative: &str) -> PathBuf {
            Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
        }

        /// The seam this provider's adapter actually consumes, read out of the two
        /// sources rather than declared anywhere.
        ///
        /// An `Err` here is a **refusal**: the gate reports `Invalid`, which is
        /// fatal wherever it is found. A contract that cannot be computed must
        /// never read as a contract that matches.
        pub(crate) fn seam_contract(
            id: NativeAppId,
        ) -> Result<crate::carry_seam_contract::SeamContract, String> {
            let Some(adapter) = adapter_source(id) else {
                return Err(format!(
                    "{id:?} has no adapter module, so it consumes no seam and can hold no receipt"
                ));
            };
            let adapter_src = std::fs::read_to_string(crate_path(adapter))
                .map_err(|error| format!("{adapter} is unreadable: {error}"))?;
            let substrate_src = std::fs::read_to_string(crate_path(SUBSTRATE_SOURCE))
                .map_err(|error| format!("{SUBSTRATE_SOURCE} is unreadable: {error}"))?;
            crate::carry_seam_contract::seam_contract_from_sources(&adapter_src, &substrate_src)
                .map_err(|error| format!("{adapter} against {SUBSTRATE_SOURCE}: {error}"))
        }

        pub(crate) fn receipt_path(id: NativeAppId) -> PathBuf {
            crate_path("carry-receipts").join(format!("{}.json", provider_slug(id)))
        }

        pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
            crate::carry_seam_contract::sha256_hex(bytes)
        }

        pub(crate) fn source_sha256(relative: &str) -> String {
            sha256_hex(
                &std::fs::read(crate_path(relative))
                    .unwrap_or_else(|error| panic!("{relative} is readable: {error}")),
            )
        }

        /// **D-244 — the two live runs, named, because ONE OF THEM CANNOT EARN A
        /// RECEIPT AND TWO LANES LOST TIME REDISCOVERING THAT.**
        ///
        /// A carrier surface has exactly two shapes of live run, and on Discord
        /// they are mutually exclusive by construction:
        ///
        /// * **Run A — [`LiveRun::PlacementProbe`].** The landing-oracle probe,
        ///   borrowing the shipping write primitive
        ///   (`native_discord_adapter::shipping_type_text`) with `send_enter` out
        ///   of reach. Nothing is committed and nothing appears in anyone's chat.
        ///   **This is the only run that can earn a [`LiveCarryReceipt`].**
        /// * **Run B — [`LiveRun::ShippingSend`].** The shipping path,
        ///   `native_discord_adapter`'s `place()`, which ends in `send_enter` and
        ///   whose two success returns both report `enter_sent: true`. It posts.
        ///   It produces the keystone's screenshots and the peer decode. **It
        ///   earns no receipt and must not be made to claim one.**
        ///
        /// **Why a type and not the comment that was here before.** Two lanes
        /// re-derived this split from source, a day apart, because the only thing
        /// naming it was prose. The runs are now named where they are used:
        /// [`LiveCarryReceipt::live_run`] answers *which run* rather than *a
        /// boolean*, [`LiveCarryReceipt::write`] refuses a receipt Run B is
        /// claiming before any artifact reaches disk, and
        /// `a_run_that_can_commit_the_message_cannot_author_a_receipt` refuses to
        /// let a file hold both a receipt and a verb that commits a message.
        ///
        /// **This does not weaken `verify_receipt`.** The serialized field is
        /// still `enter_sent`, still `false` for Run A, and the verifier's
        /// `enter_sent == false` clause is untouched. Two independent refusals now
        /// stand between Run B and a receipt: this one at authorship, that one at
        /// verification. Neither subsumes the other.
        ///
        /// The receipt's field stays a plain `bool` on purpose: a provider's
        /// receipt is bound to its adapter's source hash, so retyping the field
        /// would have edited `native_telegram_adapter.rs` and invalidated an
        /// **earned** Telegram receipt. Re-earning a live proof to make a
        /// refactor land is exactly backwards.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub(crate) enum LiveRun {
            /// Run A. No Enter. Nothing posted. Proves **placement**.
            PlacementProbe,
            /// Run B. Presses Enter. Posts. Proves **carry end to end**, and
            /// earns **no** receipt.
            ShippingSend,
        }

        impl LiveRun {
            /// What this run reports into the receipt's `enter_sent` field.
            pub(crate) const fn enter_sent(self) -> bool {
                match self {
                    LiveRun::PlacementProbe => false,
                    LiveRun::ShippingSend => true,
                }
            }

            /// Whether a run of this shape may author a [`LiveCarryReceipt`] at
            /// all. Only Run A may.
            pub(crate) const fn earns_a_receipt(self) -> bool {
                !self.enter_sent()
            }

            /// The name the plan and the tasklogs use, so a refusal names the run
            /// the operator was actually driving.
            pub(crate) const fn label(self) -> &'static str {
                match self {
                    LiveRun::PlacementProbe => "Run A (the oracle placement probe, no Enter)",
                    LiveRun::ShippingSend => "Run B (the shipping send, which posts)",
                }
            }
        }

        /// Everything a receipt states. Every field is something only a live run
        /// can observe, or a binding that makes a stale receipt detectable.
        #[derive(Clone, Debug)]
        pub(crate) struct LiveCarryReceipt {
            pub schema: String,
            pub provider: String,
            pub seam: String,
            pub adapter_source: String,
            pub adapter_source_sha256: String,
            /// **The substrate binding.** Hash of the declarations this adapter
            /// consumes from `native_a11y.rs`, not of the file. A mismatch is
            /// `Stale`.
            pub seam_contract_sha256: String,
            /// How many declarations that contract held when the receipt was
            /// earned. Recorded so a contract that silently collapsed to nothing
            /// is visible in the artifact itself, not only in the code that
            /// computes it.
            pub seam_contract_items: usize,
            /// Kept from v1 and still recorded: the whole substrate file's hash.
            /// A mismatch is **drift**, not staleness -- see
            /// `EarnedWithSubstrateDrift`.
            pub substrate_source_sha256: String,
            pub client_process: String,
            pub element_count: usize,
            pub carrier_bytes: usize,
            pub readback_bytes: usize,
            pub byte_exact: bool,
            pub payload_sha256: String,
            pub readback_sha256: String,
            pub recovered_sha256: String,
            /// **Which of D-244's two runs authored this, in one byte.** Read it
            /// through [`LiveCarryReceipt::live_run`], which names the run rather
            /// than answering a boolean, and never author one without going
            /// through [`LiveCarryReceipt::write`] — it refuses Run B.
            pub enter_sent: bool,
            pub composer_empty_after_clear: bool,
            pub recorded_utc: String,
        }

        impl LiveCarryReceipt {
            pub(crate) fn to_json(&self) -> String {
                serde_json::to_string_pretty(&serde_json::json!({
                    "schema": self.schema,
                    "provider": self.provider,
                    "seam": self.seam,
                    "adapter_source": self.adapter_source,
                    "adapter_source_sha256": self.adapter_source_sha256,
                    "seam_contract_sha256": self.seam_contract_sha256,
                    "seam_contract_items": self.seam_contract_items,
                    "substrate_source_sha256": self.substrate_source_sha256,
                    "client_process": self.client_process,
                    "element_count": self.element_count,
                    "carrier_bytes": self.carrier_bytes,
                    "readback_bytes": self.readback_bytes,
                    "byte_exact": self.byte_exact,
                    "payload_sha256": self.payload_sha256,
                    "readback_sha256": self.readback_sha256,
                    "recovered_sha256": self.recovered_sha256,
                    "enter_sent": self.enter_sent,
                    "composer_empty_after_clear": self.composer_empty_after_clear,
                    "recorded_utc": self.recorded_utc,
                }))
                .expect("receipt serialises")
                    + "\n"
            }

            /// **The D-244 authorship gate.** A run that commits the message has
            /// no receipt to write, and this refuses before any artifact reaches
            /// disk — one refusal earlier, and by a different mechanism, than
            /// `verify_receipt`'s `enter_sent == false` clause.
            ///
            /// Kept separate on purpose. The verifier reads *bytes* and can only
            /// speak after a file exists; this reads the *run* and stops a Run B
            /// artifact from existing at all. Neither subsumes the other, and
            /// weakening either is weakening the proof.
            pub(crate) fn refuse_unless_the_run_can_earn_one(&self) {
                assert!(
                    self.live_run().earns_a_receipt(),
                    "D-244: {} cannot author a LiveCarryReceipt. A receipt is a PLACEMENT proof -- \
                     it means `proven without touching anyone's chat`, which is exactly what \
                     `enter_sent == false` records. A run that pressed Enter might have been \
                     carried by the provider itself rather than by placement, so it earns the \
                     screenshots and the peer decode and NOTHING ELSE. The keystone is two runs; \
                     do not try to make it one.",
                    self.live_run().label()
                );
            }

            /// **Which of D-244's two runs this receipt is claiming**, named
            /// rather than answered as a boolean.
            ///
            /// A receipt does not carry a run label of its own — it carries the
            /// one observable that distinguishes the runs, and this is the
            /// mapping. Anything that decides *about* a run reads this, so the
            /// decision is made in one place and reads as the question it is.
            pub(crate) fn live_run(&self) -> LiveRun {
                if self.enter_sent {
                    LiveRun::ShippingSend
                } else {
                    LiveRun::PlacementProbe
                }
            }

            pub(crate) fn write(&self, id: NativeAppId) {
                self.refuse_unless_the_run_can_earn_one();
                let path = receipt_path(id);
                std::fs::create_dir_all(path.parent().expect("receipt has a parent"))
                    .expect("receipt directory is creatable");
                std::fs::write(&path, self.to_json()).expect("receipt is writable");
            }
        }

        #[derive(Debug)]
        pub(crate) enum ReceiptVerdict {
            Earned,
            /// Earned, every declaration this adapter consumes is unchanged, and
            /// `native_a11y.rs` has nonetheless been edited since. **Not fatal, and
            /// not silent.** This is the residual the seam binding cannot close: a
            /// substrate change that alters *behaviour* behind an unchanged
            /// declaration is invisible to a declaration hash. It is carried as its
            /// own verdict so it is printed by the fleet report and by every gate
            /// failure rather than being folded into `Earned`.
            EarnedWithSubstrateDrift(String),
            Absent,
            /// Sound when it was written, but the adapter or the **seam contract**
            /// has changed since. Fatal for a published provider -- it must be
            /// re-earned before the label moves -- and tolerated for one still at
            /// `ComingSoon`, so a neighbouring lane does not turn every branch red
            /// over a proof nobody is relying on yet.
            Stale(String),
            /// Never sound. Fatal wherever it is found.
            Invalid(String),
        }

        impl ReceiptVerdict {
            /// Whether this verdict lets a provider be published. Drift does; it is
            /// reported, not fatal.
            pub(crate) fn is_earned(&self) -> bool {
                matches!(
                    self,
                    ReceiptVerdict::Earned | ReceiptVerdict::EarnedWithSubstrateDrift(_)
                )
            }
        }

        pub(crate) fn verify_receipt(id: NativeAppId, seam: CarrySeam) -> ReceiptVerdict {
            let Ok(bytes) = std::fs::read(receipt_path(id)) else {
                return ReceiptVerdict::Absent;
            };
            verify_receipt_text(id, seam, &String::from_utf8_lossy(&bytes))
        }

        /// **The whole verdict, over bytes** -- [`verify_receipt_bytes`] plus the
        /// seam-map check [`verify_receipt`] used to apply on its own.
        ///
        /// Split out for **D-240**. The operator-facing fleet report used to
        /// classify a receipt from its own reading of three fields, which is a
        /// weaker predicate than this one, so a receipt with `byte_exact: false`
        /// printed `Ok` in the table while the gate rated it `Invalid`. There is
        /// now exactly one derivation and both callers go through it: the report
        /// cannot be more forgiving than the gate, because it is no longer
        /// deciding anything.
        ///
        /// Never returns [`ReceiptVerdict::Absent`] -- it is handed bytes, so the
        /// receipt is by definition present.
        pub(crate) fn verify_receipt_text(
            id: NativeAppId,
            seam: CarrySeam,
            json: &str,
        ) -> ReceiptVerdict {
            let verdict = verify_receipt_bytes(id, json);
            if !verdict.is_earned() {
                return verdict;
            }
            // The seam the receipt claims must be the seam the map declares, so a
            // receipt earned through one mechanism cannot be presented as evidence
            // for another.
            let claimed = seam_slug(seam);
            let value: serde_json::Value = serde_json::from_str(json).expect("already parsed once");
            if value["seam"].as_str() == Some(claimed) {
                verdict
            } else {
                ReceiptVerdict::Invalid(format!(
                    "seam mismatch: receipt says {:?}, the seam map says {claimed:?}",
                    value["seam"].as_str().unwrap_or("<missing>")
                ))
            }
        }

        pub(crate) const fn seam_slug(seam: CarrySeam) -> &'static str {
            match seam {
                CarrySeam::Uia2Substrate => "uia2_substrate",
                CarrySeam::NativeWindowHost => "native_window_host",
                CarrySeam::ProviderOwnedBackend => "provider_owned_backend",
                CarrySeam::NoCarryPath => "no_carry_path",
            }
        }

        /// Read a receipt and decide whether it was earned. Split from
        /// [`verify_receipt`] so every rejection path can be driven from a string
        /// rather than from the filesystem.
        pub(crate) fn verify_receipt_bytes(id: NativeAppId, json: &str) -> ReceiptVerdict {
            macro_rules! bad {
                ($($arg:tt)*) => {
                    return ReceiptVerdict::Invalid(format!($($arg)*))
                };
            }

            let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
                bad!("receipt is not valid JSON")
            };
            let string = |key: &str| value[key].as_str().unwrap_or_default().to_owned();

            // A receipt written under the superseded v1 binding was *sound when it
            // was written*; what changed is the rule, not the run. That is the
            // definition of `Stale`, so it is reported as stale -- fatal for a
            // published provider, tolerated at `ComingSoon` -- rather than as
            // never-sound. The artifact of a real live run therefore stays in the
            // tree as evidence, and still cannot publish anything.
            if string("schema") == "osl-live-carry-receipt-v1" {
                return ReceiptVerdict::Stale(format!(
                    "this receipt was earned under {:?}, which bound the whole of \
                     {SUBSTRATE_SOURCE} by content hash. That binding is superseded by \
                     {RECEIPT_SCHEMA}'s seam-contract binding, so the run must be repeated before \
                     the label can move. Re-run list: {FLEET_RERUN_COMMAND}",
                    "osl-live-carry-receipt-v1"
                ));
            }
            if string("schema") != RECEIPT_SCHEMA {
                bad!(
                    "schema is {:?}, expected {RECEIPT_SCHEMA:?}",
                    string("schema")
                )
            }
            if string("provider") != provider_slug(id) {
                bad!(
                    "provider is {:?}, expected {:?}",
                    string("provider"),
                    provider_slug(id)
                )
            }

            let Some(adapter) = adapter_source(id) else {
                bad!(
                    "{:?} has no adapter module, so no receipt can bind to one",
                    id
                )
            };
            if string("adapter_source") != adapter {
                bad!(
                    "adapter_source is {:?}, expected {adapter:?}",
                    string("adapter_source")
                )
            }
            if string("substrate_source_sha256").is_empty() {
                bad!(
                    "substrate_source_sha256 is missing, so nothing records which {SUBSTRATE_SOURCE} \
                     the run was measured against"
                )
            }
            if string("seam_contract_sha256").is_empty() {
                bad!("seam_contract_sha256 is missing, so nothing binds the receipt to the seam")
            }
            let recorded_items = value["seam_contract_items"].as_u64().unwrap_or_default() as usize;
            if recorded_items < crate::carry_seam_contract::SEAM_CONTRACT_MIN_ITEMS {
                bad!(
                    "seam contract items is {recorded_items}, below the floor of {}; a receipt that \
                     records a collapsed contract is a receipt bound to nothing",
                    crate::carry_seam_contract::SEAM_CONTRACT_MIN_ITEMS
                )
            }

            // THE STALENESS BINDING. Deliberately checked AFTER the never-sound
            // cases above, so a malformed receipt is reported as malformed rather
            // than as merely out of date.
            //
            // The adapter is bound by content: it is the thing being proven, so
            // any edit to it must cost its own proof. The substrate is bound by
            // its SEAM CONTRACT -- the declarations this adapter consumes -- so a
            // behaviour-preserving refactor of the shared substrate does not
            // invalidate 12-14 providers' proofs at once, while a change to a
            // signature, a return type, a parameter, a variant, a field or a
            // measured constant still does.
            if string("adapter_source_sha256") != source_sha256(adapter) {
                return ReceiptVerdict::Stale(format!(
                    "adapter source hash does not match {adapter}; the adapter changed since this \
                     receipt was earned, so re-run the live carry"
                ));
            }
            let contract = match seam_contract(id) {
                Ok(contract) => contract,
                // A contract that cannot be computed is a refusal, never a pass.
                Err(why) => bad!("the seam contract could not be computed: {why}"),
            };
            if string("seam_contract_sha256") != contract.sha256 {
                return ReceiptVerdict::Stale(format!(
                    "seam contract hash does not match the {} declarations {adapter} consumes from \
                     {SUBSTRATE_SOURCE}; the seam changed since this receipt was earned, so re-run \
                     the live carry. Re-run list: {FLEET_RERUN_COMMAND}",
                    contract.items.len()
                ));
            }

            // THE CARRY ITSELF. The payload the receiving decoder recovered must be
            // the payload that was sent.
            if string("recovered_sha256").is_empty()
                || string("recovered_sha256") != string("payload_sha256")
            {
                bad!(
                    "recovered payload does not match the payload sent: {:?} vs {:?}",
                    string("recovered_sha256"),
                    string("payload_sha256")
                )
            }
            if string("readback_sha256").is_empty() {
                bad!("readback hash is missing, so nothing binds the provider's own answer")
            }
            if value["byte_exact"].as_bool() != Some(true) {
                bad!("byte_exact is not true: the provider did not hand back what was placed")
            }
            if value["enter_sent"].as_bool() != Some(false) {
                bad!("enter_sent is not false")
            }
            if value["composer_empty_after_clear"].as_bool() != Some(true) {
                bad!("composer_empty_after_clear is not true: the run left text in a real chat")
            }
            let carrier = value["carrier_bytes"].as_u64().unwrap_or_default();
            let readback = value["readback_bytes"].as_u64().unwrap_or_default();
            if carrier == 0 || readback == 0 {
                bad!("carrier_bytes and readback_bytes must both be non-zero")
            }
            if value["element_count"].as_u64().unwrap_or_default() == 0 {
                bad!("element_count is zero, so no tree was read")
            }
            if string("client_process").is_empty() {
                bad!("client_process is missing, so nothing names the client that was driven")
            }
            if string("recorded_utc").is_empty() {
                bad!("recorded_utc is missing")
            }

            // Last, and deliberately not fatal: the substrate file has moved but
            // no declaration this adapter consumes has. That is the case the whole
            // rebinding exists to tolerate -- and also the one case the seam
            // binding cannot see through, so it is reported rather than dropped.
            if string("substrate_source_sha256") != source_sha256(SUBSTRATE_SOURCE) {
                return ReceiptVerdict::EarnedWithSubstrateDrift(format!(
                    "{SUBSTRATE_SOURCE} has changed since this receipt was earned, but none of the \
                     {} declarations {adapter} consumes moved. Behaviour behind an unchanged \
                     declaration is NOT covered by the seam binding: if the edit changed what the \
                     substrate does, re-run the live carry anyway.",
                    contract.items.len()
                ));
            }

            ReceiptVerdict::Earned
        }

        /// A receipt that is sound *for the tree as it stands right now*, so the
        /// rejection cases below each break exactly one thing.
        pub(crate) fn sample_sound_receipt() -> LiveCarryReceipt {
            let payload = sha256_hex(b"sample payload");
            let contract = seam_contract(NativeAppId::Telegram)
                .expect("the Telegram seam contract computes against the tree as it stands");
            LiveCarryReceipt {
                schema: RECEIPT_SCHEMA.to_owned(),
                provider: "telegram".to_owned(),
                seam: "uia2_substrate".to_owned(),
                adapter_source: "src/native_telegram_adapter.rs".to_owned(),
                adapter_source_sha256: source_sha256("src/native_telegram_adapter.rs"),
                seam_contract_sha256: contract.sha256,
                seam_contract_items: contract.items.len(),
                substrate_source_sha256: source_sha256(SUBSTRATE_SOURCE),
                client_process: "Telegram".to_owned(),
                element_count: 877,
                carrier_bytes: 276,
                readback_bytes: 276,
                byte_exact: true,
                payload_sha256: payload.clone(),
                readback_sha256: sha256_hex(b"sample readback"),
                recovered_sha256: payload,
                enter_sent: LiveRun::PlacementProbe.enter_sent(),
                composer_empty_after_clear: true,
                recorded_utc: "2026-08-04T00:00:00Z".to_owned(),
            }
        }

        type Mutation = (fn(&mut LiveCarryReceipt), &'static str);

        /// Every way a receipt can be wrong, and the phrase the rejection must
        /// carry. Driven in
        /// `the_receipt_verifier_rejects_every_way_a_receipt_can_be_wrong`.
        pub(crate) fn rejection_cases() -> Vec<Mutation> {
            vec![
                (|r| r.schema = "other".into(), "schema"),
                (|r| r.schema = "osl-live-carry-receipt-v0".into(), "schema"),
                (|r| r.provider = "whatsapp".into(), "provider"),
                (
                    |r| r.adapter_source = "src/native_signal_adapter.rs".into(),
                    "adapter_source",
                ),
                (
                    |r| r.adapter_source_sha256 = "0".repeat(64),
                    "adapter source hash",
                ),
                // D-206 recorded a mutation here that set the substrate file hash
                // to zeros and required a REJECTION. That spec is not removed --
                // it is re-recorded, with its meaning corrected, in
                // [`substrate_drift_cases`]: under the v2 seam binding a moved
                // substrate file whose declarations are unchanged is *drift*, an
                // explicit non-fatal verdict, not staleness. What must still be
                // rejected outright is a receipt that records no substrate at all.
                (
                    |r| r.substrate_source_sha256 = String::new(),
                    "substrate_source_sha256 is missing",
                ),
                (
                    |r| r.seam_contract_sha256 = "0".repeat(64),
                    "seam contract hash",
                ),
                (
                    |r| r.seam_contract_sha256 = String::new(),
                    "seam_contract_sha256 is missing",
                ),
                (|r| r.seam_contract_items = 0, "seam contract items"),
                (
                    |r| {
                        r.seam_contract_items =
                            crate::carry_seam_contract::SEAM_CONTRACT_MIN_ITEMS - 1
                    },
                    "seam contract items",
                ),
                (|r| r.recovered_sha256 = "0".repeat(64), "recovered payload"),
                (|r| r.recovered_sha256 = String::new(), "recovered payload"),
                (|r| r.readback_sha256 = String::new(), "readback hash"),
                (|r| r.byte_exact = false, "byte_exact"),
                // D-244: this mutation is literally *"claim Run B produced a
                // receipt"*, and it is written that way so it reads as one. The
                // verifier must reject the bytes on the `enter_sent == false`
                // clause -- independently of, and after, the authorship gate that
                // stops such a receipt reaching disk at all.
                (
                    |r| r.enter_sent = LiveRun::ShippingSend.enter_sent(),
                    "enter_sent",
                ),
                (
                    |r| r.composer_empty_after_clear = false,
                    "composer_empty_after_clear",
                ),
                (|r| r.carrier_bytes = 0, "carrier_bytes"),
                (|r| r.readback_bytes = 0, "readback_bytes"),
                (|r| r.element_count = 0, "element_count"),
                (|r| r.client_process = String::new(), "client_process"),
                (|r| r.recorded_utc = String::new(), "recorded_utc"),
            ]
        }

        /// Mutations that must produce [`ReceiptVerdict::EarnedWithSubstrateDrift`]
        /// -- earned, reported, and **not** fatal.
        ///
        /// This is the v1 spec that changed meaning rather than disappearing. It
        /// is asserted just as strictly as a rejection: the verdict must be drift
        /// exactly, so neither a silent `Earned` nor a `Stale` can hide here.
        pub(crate) fn substrate_drift_cases() -> Vec<Mutation> {
            vec![(
                |r| r.substrate_source_sha256 = "0".repeat(64),
                "has changed since this receipt was earned",
            )]
        }
    }

    /// **D-244 — every file allowed to author a `LiveCarryReceipt`, and it is a
    /// closed list on purpose.**
    ///
    /// A receipt is a **Run A** artifact. Adding a new author is therefore a
    /// decision about which run a surface's live probe is, not a detail — so the
    /// scan below requires this list to match the source *exactly*. A lane that
    /// writes a fourth author gets a red test naming D-244 before it gets a
    /// receipt, which is the whole point: the previous guard was a comment, and a
    /// comment is what two lanes walked past.
    ///
    /// `native_apps.rs` is deliberately absent. It **defines** both runs and has
    /// to be able to name [`carry_receipt::LiveRun::ShippingSend`] in order to
    /// prove the refusals; it authors no live artifact.
    const RECEIPT_AUTHORS: &[&str] = &[
        "src/native_telegram_adapter.rs",
        "src/landing_oracle/live_telegram.rs",
        "src/landing_oracle/live_whatsapp.rs",
    ];

    /// Verbs that COMMIT a message. A file holding one of these is driving
    /// **Run B**, and Run B earns no receipt.
    ///
    /// Matched call-shaped (`name(`) so the prose in `live_telegram.rs`'s module
    /// header — which explains this very split — is not mistaken for a call.
    const COMMIT_VERBS: &[&str] = &[
        "send_enter(",
        "place_carrier(",
        "send_native_discord_overlay_carrier(",
    ];

    /// Every `.rs` under `apps/osl-hub/src`, with line comments removed so prose
    /// about a verb is never read as a use of it.
    ///
    /// Truncating each line at its first `//` can in principle cut a string
    /// literal short. That direction is safe here: it can only ever *hide* text
    /// from the scan on a line that already contained a comment marker, and the
    /// exact-list assertion below fails closed if an author disappears.
    fn hub_sources_without_comments() -> Vec<(String, String)> {
        fn walk(dir: &std::path::Path, root: &std::path::Path, out: &mut Vec<(String, String)>) {
            let entries = std::fs::read_dir(dir).expect("hub source tree is readable");
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, root, out);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    let text = std::fs::read_to_string(&path).expect("hub source is readable");
                    let stripped: String = text
                        .lines()
                        .map(|line| match line.find("//") {
                            Some(at) => &line[..at],
                            None => line,
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    let relative = path
                        .strip_prefix(root)
                        .expect("hub source is under the crate")
                        .to_string_lossy()
                        .replace('\\', "/");
                    out.push((relative, stripped));
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut out = Vec::new();
        walk(&root.join("src"), root, &mut out);
        out
    }

    /// **D-244 — A RUN THAT CAN COMMIT THE MESSAGE CANNOT AUTHOR A RECEIPT, and
    /// the separation is checked rather than described.**
    ///
    /// The keystone asked one live run for an artifact no single run can produce:
    /// clauses that require a post need `enter_sent == true`, and
    /// [`carry_receipt::verify_receipt`] rejects any receipt whose `enter_sent`
    /// is not `false`. Two lanes rediscovered that from the inside because the
    /// only thing separating the runs was prose.
    ///
    /// So: a file that authors a receipt must state `enter_sent: false` outright
    /// at every construction site, must never be able to write `enter_sent:
    /// true` there, and must hold **no verb that commits a message**. Wire the
    /// shipping send into a receipt author and this goes red, naming the defect,
    /// before anything reaches a live chat.
    #[test]
    fn a_run_that_can_commit_the_message_cannot_author_a_receipt() {
        let sources = hub_sources_without_comments();
        assert!(
            sources.len() > 20,
            "the source scan found {} files, which is too few to be reading the hub tree",
            sources.len()
        );

        let mut authors: Vec<String> = sources
            .iter()
            .filter(|(name, text)| {
                name != "src/native_apps.rs" && text.contains("LiveCarryReceipt {")
            })
            .map(|(name, _)| name.clone())
            .collect();
        authors.sort();
        let mut expected: Vec<String> = RECEIPT_AUTHORS.iter().map(|s| (*s).to_owned()).collect();
        expected.sort();
        assert_eq!(
            authors, expected,
            "the set of files that construct a LiveCarryReceipt changed. A receipt is a Run A \
             artifact (D-244): decide which run the new surface's live probe is, then record it in \
             RECEIPT_AUTHORS. Do not widen this list to make a red test pass."
        );

        for (name, text) in &sources {
            if !authors.iter().any(|author| author == name) {
                continue;
            }
            // Scoped to the receipt literals themselves. A file may well hold
            // other receipt types with their own `enter_sent` -- the Telegram
            // adapter's placement receipt does -- and those are not this gate's
            // business.
            let literals: Vec<&str> = text
                .split("LiveCarryReceipt {")
                .skip(1)
                .map(|tail| &tail[..tail.find("};").expect("a receipt literal is terminated")])
                .collect();
            assert!(!literals.is_empty());
            for literal in literals {
                assert!(
                    literal.contains("enter_sent: false"),
                    "{name} builds a LiveCarryReceipt without writing `enter_sent: false` as a \
                     literal. D-244: a receipt is a Run A artifact, and whether the run committed \
                     must be a fact the author states outright -- never a value it computes, which \
                     is how a Run B outcome would reach the field."
                );
                assert!(
                    !literal.contains("enter_sent: true"),
                    "{name} builds a LiveCarryReceipt claiming `enter_sent: true`. Run B posts, so \
                     it can never earn a receipt -- see D-244. Split the run; do not merge the \
                     artifact."
                );
            }
            for verb in COMMIT_VERBS {
                assert!(
                    !text.contains(verb),
                    "{name} authors a receipt and can reach `{verb}`, which COMMITS the message. A \
                     receipt means `proven without touching anyone's chat`; a run that committed \
                     might have been carried by the provider rather than by placement. D-244: two \
                     runs, two artifacts."
                );
            }
        }
    }

    /// The negative half of the gate above: the verbs it forbids must actually
    /// exist in the tree, or it is checking for nothing.
    ///
    /// A gate that cannot fail is decoration. If `send_enter(` is renamed and
    /// this is not updated, the separation check silently stops separating
    /// anything — so the rename has to come through here.
    ///
    /// **`native_apps.rs` is excluded, and that exclusion is the whole test.**
    /// [`COMMIT_VERBS`] lives in this file, so every verb is trivially "present"
    /// in it as its own string literal. Scanning this file too made the check
    /// self-satisfying: renaming `send_enter(` to a verb that exists nowhere
    /// still passed, measured. It is the file *being asked about* that has to be
    /// left out.
    #[test]
    fn the_commit_verbs_the_receipt_gate_forbids_are_real() {
        let sources = hub_sources_without_comments();
        for verb in COMMIT_VERBS {
            let holders: Vec<&str> = sources
                .iter()
                .filter(|(name, text)| name != "src/native_apps.rs" && text.contains(verb))
                .map(|(name, _)| name.as_str())
                .collect();
            assert!(
                !holders.is_empty(),
                "no file in the hub calls `{verb}`, so forbidding it in a receipt author proves \
                 nothing. Either the verb was renamed -- update COMMIT_VERBS -- or the gate has \
                 quietly become decoration."
            );
        }
    }

    /// **D-244 — the two runs disagree about `enter_sent` BY CONSTRUCTION, and
    /// that is the whole reason the keystone is two runs.**
    #[test]
    fn only_the_placement_probe_run_can_earn_a_receipt() {
        use carry_receipt::LiveRun;

        assert!(!LiveRun::PlacementProbe.enter_sent());
        assert!(LiveRun::ShippingSend.enter_sent());
        assert!(LiveRun::PlacementProbe.earns_a_receipt());
        assert!(
            !LiveRun::ShippingSend.earns_a_receipt(),
            "Run B posts. A receipt requires enter_sent == false. They cannot be the same run."
        );
    }

    /// **D-244 mutation (b) — claiming Run B produced a receipt is REFUSED at
    /// authorship**, before any artifact reaches disk, and independently of
    /// `verify_receipt`.
    #[test]
    #[should_panic(expected = "cannot author a LiveCarryReceipt")]
    fn a_receipt_claimed_for_the_shipping_send_run_is_refused_before_it_is_written() {
        let mut receipt = carry_receipt::sample_sound_receipt();
        receipt.enter_sent = carry_receipt::LiveRun::ShippingSend.enter_sent();
        assert_eq!(receipt.live_run(), carry_receipt::LiveRun::ShippingSend);
        receipt.refuse_unless_the_run_can_earn_one();
    }

    /// The control for the refusal above: the same receipt, authored by Run A,
    /// passes the authorship gate. Without this the `should_panic` test could be
    /// passing on any panic at all.
    #[test]
    fn a_receipt_authored_by_the_placement_probe_run_passes_the_authorship_gate() {
        let receipt = carry_receipt::sample_sound_receipt();
        assert_eq!(receipt.live_run(), carry_receipt::LiveRun::PlacementProbe);
        receipt.refuse_unless_the_run_can_earn_one();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&receipt.to_json())
                .expect("a receipt serialises")["enter_sent"],
            serde_json::json!(false),
            "Run A's receipt must still serialise enter_sent: false -- the verifier's clause is \
             untouched and is what reads this byte."
        );
    }

    #[test]
    fn ids_and_results_have_stable_camel_case_json() {
        assert_eq!(
            serde_json::to_string(&NativeAppId::Whatsapp).unwrap(),
            r#""whatsapp""#
        );
        let result = NativeInstallResult {
            id: NativeAppId::Signal,
            started: true,
            package_id: "OpenWhisperSystems.Signal",
        };
        let json = serde_json::to_value(result).unwrap();
        assert_eq!(json["id"], "signal");
        assert_eq!(json["started"], true);
        assert_eq!(json["packageId"], "OpenWhisperSystems.Signal");

        let firefox = FirefoxLaunchResult {
            service_id: FirefoxServiceId::Maildotcom,
            started: true,
        };
        let json = serde_json::to_value(firefox).unwrap();
        assert_eq!(json["serviceId"], "maildotcom");
        assert_eq!(json["started"], true);
    }

    #[test]
    fn ipc_enums_reject_paths_urls_and_argument_shaped_values() {
        for value in [
            r#""discord --enable-logging""#,
            r#""..\\evil.exe""#,
            r#""C:\\Windows\\System32\\cmd.exe""#,
            r#""https://example.test/""#,
            r#"{"discord":"telegram"}"#,
        ] {
            assert!(serde_json::from_str::<NativeAppId>(value).is_err());
        }
        for value in [
            r#""chrome --new-window https://example.test""#,
            r#""file:///C:/Windows/System32/calc.exe""#,
            r#""javascript:alert(1)""#,
            r#""..\\browser.exe""#,
            r#"{"id":"chrome","url":"https://example.test"}"#,
        ] {
            assert!(serde_json::from_str::<BrowserImportId>(value).is_err());
        }
    }

    #[test]
    fn firefox_manifest_is_exhaustive_and_https_only() {
        assert_eq!(FIREFOX_SERVICES.len(), 10);
        assert_eq!(FIREFOX_PACKAGE_ID, "Mozilla.Firefox");
        assert_eq!(FIREFOX_MIGRATION_SWITCH, "--migration");
        assert_eq!(FIREFOX_WAIT_FOR_BROWSER_SWITCH, "-wait-for-browser");
        assert_eq!(
            FIREFOX_UIA_USER_PREF,
            "user_pref(\"accessibility.uia.enable\", 1);\n"
        );
        assert_eq!(FIREFOX_CANDIDATES.len(), 4);
        assert_eq!(FIREFOX_PROFILE_BASE_COMPONENT, "service-profiles-v2");
        assert_eq!(FIREFOX_PROFILE_COMPONENT, "firefox-browser");
        let owner_a = firefox_profile_relative_path("owner-a");
        let owner_b = firefox_profile_relative_path("owner-b");
        assert!(owner_a.starts_with("service-profiles-v2"));
        assert_ne!(owner_a, owner_b);
        assert!([FIREFOX_PROFILE_BASE_COMPONENT, FIREFOX_PROFILE_COMPONENT]
            .iter()
            .all(|component| {
                !component.is_empty()
                    && *component != "."
                    && *component != ".."
                    && !component.contains(['/', '\\'])
            }));
        for (index, (service_id, url)) in FIREFOX_SERVICES.iter().enumerate() {
            assert_eq!(firefox_service_url(*service_id), *url);
            assert!(url.starts_with("https://"));
            assert!(!url.contains('@'));
            assert!(!url.contains('?'));
            assert!(!url.contains('#'));
            assert!(FIREFOX_SERVICES[..index]
                .iter()
                .all(|(previous, _)| previous != service_id));
        }
    }

    #[test]
    fn browser_import_manifest_is_fixed_complete_and_has_no_profile_inputs() {
        assert!(
            serde_json::from_str::<BrowserImportId>(r#""chrome --load-extension=evil""#).is_err()
        );
        assert!(serde_json::from_str::<BrowserImportId>(r#""../firefox""#).is_err());
        let expected_targets = [
            (
                BrowserImportId::Chrome,
                "chrome://password-manager/settings",
            ),
            (BrowserImportId::Edge, "edge://settings/passwords"),
            (BrowserImportId::Firefox, "about:logins"),
            (BrowserImportId::Brave, "brave://password-manager/settings"),
            (BrowserImportId::Opera, "opera://password-manager/settings"),
        ];
        assert_eq!(BROWSER_IMPORTS.len(), 6);
        for (index, browser) in BROWSER_IMPORTS.iter().enumerate() {
            assert!(!browser.display_name.is_empty());
            if browser.id == BrowserImportId::DuckDuckGo {
                assert!(browser.candidates.is_empty());
                assert!(browser.import_arguments.is_empty());
            } else {
                assert!(!browser.candidates.is_empty());
                assert_eq!(browser.import_arguments.first(), Some(&"--new-window"));
                assert_eq!(browser.import_arguments.len(), 2);
                assert_eq!(
                    browser.import_arguments[1],
                    expected_targets
                        .iter()
                        .find_map(|(id, target)| (*id == browser.id).then_some(*target))
                        .expect("every unpackaged browser has one reviewed internal target")
                );
                assert!(!browser.import_arguments[1].starts_with("http:"));
                assert!(!browser.import_arguments[1].starts_with("https:"));
                assert!(!browser.import_arguments[1].starts_with("file:"));
                assert!(!browser.import_arguments[1].starts_with("javascript:"));
            }
            assert!(BROWSER_IMPORTS[..index]
                .iter()
                .all(|previous| previous.id != browser.id));
            for candidate in browser.candidates {
                assert!(!candidate.relative_path.is_empty());
                assert!(!candidate.relative_path.starts_with(['/', '\\']));
                assert!(!candidate.relative_path.contains(".."));
                assert!(!candidate.relative_path.contains(':'));
                assert!(candidate.relative_path.ends_with(".exe"));
            }
        }
        assert_eq!(
            browser_import_manifest(BrowserImportId::Chrome).display_name,
            "Chrome"
        );
        assert_eq!(
            browser_import_manifest(BrowserImportId::DuckDuckGo).display_name,
            "DuckDuckGo"
        );
        assert_eq!(
            browser_import_manifest(BrowserImportId::Opera).candidates,
            &[ExecutableCandidate {
                folder: KnownFolder::Local,
                relative_path: r"Programs\Opera\opera.exe",
            }]
        );
        assert_eq!(
            browser_import_manifest(BrowserImportId::DuckDuckGo).candidates,
            &[]
        );
        assert_eq!(
            browser_import_manifest(BrowserImportId::Chrome).publisher_attestation,
            BrowserPublisherAttestation::Verified(ExecutablePublisher::Chrome)
        );
        assert_eq!(
            browser_import_manifest(BrowserImportId::Edge).publisher_attestation,
            BrowserPublisherAttestation::Verified(ExecutablePublisher::Edge)
        );
        assert_eq!(
            browser_import_manifest(BrowserImportId::Firefox).publisher_attestation,
            BrowserPublisherAttestation::Verified(ExecutablePublisher::Firefox)
        );
        assert_eq!(
            browser_import_manifest(BrowserImportId::Brave).publisher_attestation,
            BrowserPublisherAttestation::Verified(ExecutablePublisher::Brave)
        );
        assert_eq!(
            browser_import_manifest(BrowserImportId::Opera).publisher_attestation,
            BrowserPublisherAttestation::Verified(ExecutablePublisher::Opera)
        );
        assert_eq!(
            browser_import_manifest(BrowserImportId::DuckDuckGo).publisher_attestation,
            BrowserPublisherAttestation::Verified(ExecutablePublisher::DuckDuckGo)
        );
        assert_eq!(
            attested_browser_publisher(browser_import_manifest(BrowserImportId::Opera)),
            Some(ExecutablePublisher::Opera)
        );
        assert_eq!(
            attested_browser_publisher(browser_import_manifest(BrowserImportId::DuckDuckGo)),
            Some(ExecutablePublisher::DuckDuckGo)
        );
        for browser_id in [
            BrowserImportId::Chrome,
            BrowserImportId::Edge,
            BrowserImportId::Brave,
            BrowserImportId::Opera,
        ] {
            assert!(browser_uses_chromium_app_mode(browser_id));
        }
        assert!(!browser_uses_chromium_app_mode(BrowserImportId::Firefox));
        assert!(!browser_uses_chromium_app_mode(BrowserImportId::DuckDuckGo));
    }

    #[test]
    fn automatic_browser_migration_uses_a_fixed_fail_closed_preference_order() {
        assert_eq!(preferred_browser_import_source(&[]), None);
        assert_eq!(
            preferred_browser_import_source(&[BrowserImportId::Chrome]),
            Some(BrowserImportId::Chrome)
        );
        assert_eq!(
            preferred_browser_import_source(&[
                BrowserImportId::Chrome,
                BrowserImportId::Edge,
                BrowserImportId::Firefox,
            ]),
            Some(BrowserImportId::Edge)
        );
        assert_eq!(
            preferred_browser_import_source(&[
                BrowserImportId::Firefox,
                BrowserImportId::DuckDuckGo,
            ]),
            Some(BrowserImportId::DuckDuckGo)
        );
    }

    #[test]
    fn only_packaged_app_installer_winget_is_trusted() {
        assert_eq!(
            DESKTOP_APP_INSTALLER_FAMILY_NAME,
            "Microsoft.DesktopAppInstaller_8wekyb3d8bbwe"
        );
        let program_files = Path::new("C:/Program Files");
        let packaged_winget = program_files.join(
            "WindowsApps/Microsoft.DesktopAppInstaller_1.29.279.0_x64__8wekyb3d8bbwe/winget.exe",
        );
        let alias = PathBuf::from("C:/Users/alice/AppData/Local/Microsoft/WindowsApps/winget.exe");

        assert!(!is_trusted_winget_path(&alias, program_files));
        assert!(is_trusted_winget_path(&packaged_winget, program_files));
        assert_eq!(
            resolve_winget_executable(std::slice::from_ref(&packaged_winget), program_files),
            Some(packaged_winget)
        );
    }

    #[test]
    fn whatsapp_store_identity_is_exact_and_rejects_non_application_packages() {
        assert_eq!(
            whatsapp_store_package_family_name(),
            WHATSAPP_PACKAGE_FAMILY_NAME
        );
        assert_eq!(
            manifest(NativeAppId::Whatsapp).store_package_family_name,
            Some(WHATSAPP_PACKAGE_FAMILY_NAME)
        );
        assert_eq!(
            WHATSAPP_PACKAGE_FAMILY_NAME,
            "5319275A.WhatsAppDesktop_cv1g1gvanyjgm"
        );
        assert!(whatsapp_package_identity_matches(
            "5319275A.WhatsAppDesktop",
            "cv1g1gvanyjgm",
            ""
        ));
        assert!(whatsapp_package_full_name_matches(
            "5319275A.WhatsAppDesktop_2.2627.101.0_x64__cv1g1gvanyjgm"
        ));
        for rejected in [
            "5319275A.WhatsAppDesktop_2.2627.101_x64__cv1g1gvanyjgm",
            "5319275A.WhatsAppDesktop_2.2627.101.0_x64_en-us_cv1g1gvanyjgm",
            "5319275A.WhatsAppDesktop_2.2627.101.0_x64__attacker",
            "Attacker_2.2627.101.0_x64__cv1g1gvanyjgm",
            "5319275A.WhatsAppDesktop_bad_x64__cv1g1gvanyjgm",
        ] {
            assert!(!whatsapp_package_full_name_matches(rejected));
        }
        assert!(!whatsapp_package_identity_matches(
            "5319275A.WhatsAppDesktop.Resource",
            "cv1g1gvanyjgm",
            ""
        ));
        assert!(!whatsapp_package_identity_matches(
            "5319275A.WhatsAppDesktop",
            "attacker",
            ""
        ));
        assert!(!whatsapp_package_identity_matches(
            "5319275A.WhatsAppDesktop",
            "cv1g1gvanyjgm",
            "en-us"
        ));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn current_user_whatsapp_registration_resolves_exact_packaged_executable() {
        // Two different things were tangled here: the invariant (if we resolve
        // a WhatsApp registration at all, it is the exact packaged executable
        // under a trusted package root -- never a fuzzy match) and the
        // environment fact (WhatsApp happens to be installed). The `expect`
        // asserted the environment fact, so the test failed on a bare runner
        // where nothing is installed.
        //
        // The invariant is asserted below in both directions, and this is not
        // a silent skip: if WhatsApp *is* registered and the resolved path is
        // not the exact packaged executable in a trusted root, this fails
        // loudly. If it is not registered, resolution must refuse cleanly
        // rather than hand back some approximate path.
        match whatsapp_store_executable_path() {
            Some(executable) => {
                assert!(
                    path_file_name_eq(&executable, "WhatsApp.Root.exe"),
                    "a resolved WhatsApp registration must be the exact packaged \
                     executable, got {executable:?}"
                );
                let package_root = executable.parent().expect("packaged executable has a root");
                let program_files = known_folder(KnownFolder::ProgramFiles)
                    .expect("Program Files known folder should resolve");
                assert!(
                    is_trusted_whatsapp_package_path(package_root, &program_files),
                    "a resolved WhatsApp package root must be trusted, got {package_root:?}"
                );
            }
            None => {
                // Refusing is the only other acceptable outcome. Prove the
                // refusal is the identity check doing its job and not an
                // accident of this machine: the exact-match predicates that
                // gate resolution must still reject every near-miss identity.
                assert!(!whatsapp_package_full_name_matches(
                    "5319275A.WhatsAppDesktop_bad_x64__cv1g1gvanyjgm"
                ));
                assert!(!whatsapp_package_identity_matches(
                    "5319275A.WhatsAppDesktop",
                    "attacker",
                    ""
                ));
            }
        }
    }

    #[test]
    fn package_api_text_accepts_the_main_packages_empty_resource_id() {
        let buffer = [0u16, b'x' as u16, 0];
        assert_eq!(
            utf16_string_from_api_buffer(buffer.as_ptr(), &buffer),
            Some(String::new())
        );
        assert_eq!(
            utf16_string_from_api_buffer(unsafe { buffer.as_ptr().add(1) }, &buffer),
            Some("x".to_owned())
        );
    }

    #[test]
    fn whatsapp_store_path_must_be_an_immediate_windows_apps_child() {
        let program_files = Path::new("C:/Program Files");
        let package = program_files
            .join("WindowsApps/5319275A.WhatsAppDesktop_2.2527.4.0_x64__cv1g1gvanyjgm");
        assert!(is_trusted_whatsapp_package_path(&package, program_files));
        for rejected in [
            PathBuf::from("C:/Users/alice/5319275A.WhatsAppDesktop_2.2527.4.0_x64__cv1g1gvanyjgm"),
            program_files
                .join("WindowsApps/nested/5319275A.WhatsAppDesktop_2.2527.4.0_x64__cv1g1gvanyjgm"),
            program_files.join("WindowsApps/5319275A.WhatsAppDesktop_2.2527.4.0_x64__attacker"),
            program_files.join("WindowsApps/../Windows/whatsapp.exe"),
        ] {
            assert!(!is_trusted_whatsapp_package_path(&rejected, program_files));
        }
    }

    #[test]
    fn duckduckgo_store_registration_and_executable_are_exact() {
        assert!(
            DUCKDUCKGO_LOCATION_SCRIPT.contains("Get-AppxPackage -Name DuckDuckGo.DesktopBrowser")
        );
        assert!(DUCKDUCKGO_LOCATION_SCRIPT.contains("UTF8Encoding"));
        assert_eq!(
            DUCKDUCKGO_EXECUTABLE_RELATIVE_PATH,
            r"WindowsBrowser\DuckDuckGo.exe"
        );
        let location =
            "C:/Program Files/WindowsApps/DuckDuckGo.DesktopBrowser_0.165.4.0_x64__abcdefghijklm";
        assert_eq!(
            decode_duckduckgo_location(location.as_bytes()),
            Some(PathBuf::from(location))
        );
        for rejected in [
            b"".as_slice(),
            b" C:/Program Files/WindowsApps/DuckDuckGo.DesktopBrowser_0.165.4.0_x64__abcdefghijklm",
            b"C:/Program Files/WindowsApps/DuckDuckGo.DesktopBrowser_0.165.4.0_x64__abcdefghijklm\n",
            b"C:/one\nC:/two",
        ] {
            assert_eq!(decode_duckduckgo_location(rejected), None);
        }
    }

    #[test]
    fn duckduckgo_store_path_must_be_an_immediate_windows_apps_child() {
        let program_files = Path::new("C:/Program Files");
        let package = program_files
            .join("WindowsApps/DuckDuckGo.DesktopBrowser_0.165.4.0_x64__abcdefghijklm");
        assert!(duckduckgo_package_full_name_matches(
            "DuckDuckGo.DesktopBrowser_0.165.4.0_x64__abcdefghijklm"
        ));
        assert!(is_trusted_duckduckgo_package_path(&package, program_files));
        for rejected in [
            PathBuf::from("C:/Users/alice/DuckDuckGo.DesktopBrowser_0.165.4.0_x64__abcdefghijklm"),
            program_files
                .join("WindowsApps/nested/DuckDuckGo.DesktopBrowser_0.165.4.0_x64__abcdefghijklm"),
            program_files
                .join("WindowsApps/FakeDuckDuckGo.DesktopBrowser_0.165.4.0_x64__abcdefghijklm"),
            program_files.join("WindowsApps/DuckDuckGo.DesktopBrowser_latest_x64__abcdefghijklm"),
            program_files.join(
                "WindowsApps/../Windows/DuckDuckGo.DesktopBrowser_0.165.4.0_x64__abcdefghijklm",
            ),
        ] {
            assert!(!is_trusted_duckduckgo_package_path(
                &rejected,
                program_files
            ));
        }
    }

    #[test]
    fn app_installer_probe_emits_and_decodes_utf8_deterministically() {
        assert!(APP_INSTALLER_LOCATION_SCRIPT.contains("UTF8Encoding"));
        assert!(APP_INSTALLER_LOCATION_SCRIPT.contains("OpenStandardOutput"));

        let location = "C:/Program Files/WindowsApps/Microsoft.DesktopAppInstaller_1.29.279.0_x64__8wekyb3d8bbwe";
        assert_eq!(
            decode_app_installer_locations(location.as_bytes()),
            vec![PathBuf::from(location).join("winget.exe")]
        );

        let utf16_stdout = location
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        assert!(decode_app_installer_locations(&utf16_stdout).is_empty());
    }

    #[test]
    fn discord_update_stub_without_a_versioned_executable_is_not_installed() {
        let root = unique_discord_test_root("discord-stub");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("Update.exe"), b"stale updater").unwrap();

        assert_eq!(newest_discord_executable_under(&root), None);

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn discord_detection_selects_the_newest_valid_versioned_executable() {
        let root = unique_discord_test_root("discord-versions");
        let older = root.join("app-1.0.9012");
        let newer = root.join("app-1.0.9189");
        std::fs::create_dir_all(&older).unwrap();
        std::fs::create_dir_all(&newer).unwrap();
        std::fs::write(older.join("Discord.exe"), b"older").unwrap();
        std::fs::write(newer.join("Discord.exe"), b"newer").unwrap();
        std::fs::create_dir_all(root.join("app-current")).unwrap();
        std::fs::write(root.join("app-current/Discord.exe"), b"invalid").unwrap();

        assert_eq!(
            newest_discord_executable_under(&root),
            Some(newer.join("Discord.exe"))
        );

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn discord_detection_compares_version_components_numerically() {
        let root = unique_discord_test_root("discord-numeric-versions");
        let older = root.join("app-1.0.99");
        let newer = root.join("app-1.0.100");
        std::fs::create_dir_all(&older).unwrap();
        std::fs::create_dir_all(&newer).unwrap();
        std::fs::write(older.join("Discord.exe"), b"older").unwrap();
        std::fs::write(newer.join("Discord.exe"), b"newer").unwrap();

        assert_eq!(
            newest_discord_executable_under(&root),
            Some(newer.join("Discord.exe"))
        );

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn discord_detection_maps_each_official_channel_to_its_signed_executable() {
        let channels = [
            (r"Discord\Update.exe", "Discord.exe"),
            (r"DiscordPTB\Update.exe", "DiscordPTB.exe"),
            (r"DiscordCanary\Update.exe", "DiscordCanary.exe"),
        ];
        for (index, (update_path, executable_name)) in channels.into_iter().enumerate() {
            assert_eq!(discord_executable_name(update_path), Some(executable_name));
            let root = unique_discord_test_root(&format!("discord-channel-{index}"));
            let version = root.join("app-1.0.9999");
            std::fs::create_dir_all(&version).unwrap();
            std::fs::write(version.join(executable_name), b"channel executable").unwrap();
            assert_eq!(
                newest_discord_channel_executable_under(&root, executable_name),
                Some(version.join(executable_name))
            );
            std::fs::remove_dir_all(&root).unwrap();
        }
        assert_eq!(discord_executable_name(r"DiscordBeta\Update.exe"), None);
    }

    #[test]
    fn native_app_refresh_probes_app_installer_once() {
        let calls = Cell::new(0);
        let statuses = list_native_apps_with_installer_probe(|| {
            calls.set(calls.get() + 1);
            true
        });

        assert_eq!(calls.get(), 1);
        assert_eq!(statuses.len(), NATIVE_APPS.len());
        #[cfg(not(target_os = "windows"))]
        for status in statuses {
            let expected = if status.id == NativeAppId::Outlook {
                NativeAppAvailability::Unavailable
            } else {
                NativeAppAvailability::Installable
            };
            assert_eq!(status.availability, expected, "{:?}", status.id);
        }
    }

    #[test]
    fn list_native_apps() {
        let statuses = super::list_native_apps();

        assert_eq!(statuses.len(), NATIVE_APPS.len());
        for (status, manifest) in statuses.iter().zip(NATIVE_APPS) {
            assert_eq!(status.id, manifest.id);
            assert_eq!(status.display_name, manifest.display_name);
            assert_eq!(
                status.isolated_profile_available,
                isolated_native_profile_available(manifest.id)
            );
            assert!(
                !status.supports_overlay,
                "{:?} must expose status without implying overlay support",
                status.id
            );
        }
    }

    #[test]
    fn changing_a_service_capability_record_changes_home_tile_label() {
        use crate::claim_state::{
            CarrierEvidence, ClaimBlocker, DeliveryEvidence, MatrixPosition, Surface, SurfaceClaim,
        };

        fn tile_label_for(claim: SurfaceClaim) -> String {
            let statuses = list_native_apps_with_claims_and_installer_probe(
                |surface| {
                    if surface == Surface::Signal {
                        claim
                    } else {
                        *crate::claim_state::claim_of(surface)
                    }
                },
                || false,
            );
            let tile = statuses
                .iter()
                .find(|status| status.id == NativeAppId::Signal)
                .expect("Signal tile data is present");
            serde_json::to_value(tile).unwrap()["supportStatus"]
                .as_str()
                .expect("Home tile supportStatus is a string")
                .to_owned()
        }

        let mut capability_record = *crate::claim_state::claim_of(Surface::Signal);
        capability_record.carrier = CarrierEvidence::BuiltNeverProvenLive;
        capability_record.delivery = DeliveryEvidence::NeverProvenLive;
        capability_record.blockers = &[];
        capability_record.matrix = MatrixPosition::NoRow;
        capability_record.reason =
            "0802 test row: a wired service with no live proof is experimental.";
        let experimental_label = tile_label_for(capability_record);
        println!(
            "0802 Signal Home tile supportStatus after BuiltNeverProvenLive: {experimental_label}"
        );
        assert_eq!(experimental_label, "experimental");

        capability_record.carrier = CarrierEvidence::NotBuilt;
        capability_record.delivery = DeliveryEvidence::NotDeliverable;
        capability_record.reason = "0802 test row: an unwired service is coming soon.";
        let coming_soon_label = tile_label_for(capability_record);
        println!("0802 Signal Home tile supportStatus after NotBuilt: {coming_soon_label}");
        assert_eq!(coming_soon_label, "comingSoon");

        capability_record.carrier = CarrierEvidence::ExternallyBlocked;
        capability_record.reason =
            "0802 test row: an externally blocked service is externally blocked.";
        let externally_blocked_label = tile_label_for(capability_record);
        println!(
            "0802 Signal Home tile supportStatus after ExternallyBlocked: {externally_blocked_label}"
        );
        assert_eq!(externally_blocked_label, "externallyBlocked");

        capability_record.blockers = &[ClaimBlocker::OpenSecurityFinding];
        capability_record.reason =
            "0802 test row: an extinguishing blocker removes the public claim.";
        let no_claim_label = tile_label_for(capability_record);
        println!("0802 Signal Home tile supportStatus after OpenSecurityFinding: {no_claim_label}");
        assert_eq!(no_claim_label, "noClaim");
    }

    #[test]
    fn list_native_apps_reports_statuses_without_overlay_support() {
        let statuses = list_native_apps_with_installer_probe(|| true);

        assert_eq!(statuses.len(), NATIVE_APPS.len());
        for (status, manifest) in statuses.iter().zip(NATIVE_APPS) {
            assert_eq!(status.id, manifest.id);
            assert_eq!(status.display_name, manifest.display_name);
            assert_eq!(
                status.isolated_profile_available,
                isolated_native_profile_available(manifest.id)
            );
            assert!(
                !status.supports_overlay,
                "{:?} must not imply native overlay support",
                status.id
            );
        }

        let json = serde_json::to_value(&statuses).unwrap();
        let rows = json
            .as_array()
            .expect("native app statuses serialize as rows");
        assert!(rows.iter().all(|row| row["supportsOverlay"] == false));
        assert!(rows
            .iter()
            .any(|row| row["isolatedProfileAvailable"] == true));
    }

    #[test]
    fn listing_probe_coalesces_failure_briefly_then_caches_success() {
        assert_eq!(INSTALLER_FAILURE_RETRY_DELAY, Duration::from_millis(250));
        let cache = Mutex::new(InstallerAvailabilityCache::default());
        let calls = Cell::new(0);
        let failure_completed = Instant::now();

        assert!(!cached_installer_availability(
            &cache,
            || failure_completed,
            || {
                calls.set(calls.get() + 1);
                false
            }
        ));
        assert!(!cached_installer_availability(
            &cache,
            || failure_completed + INSTALLER_FAILURE_RETRY_DELAY / 2,
            || {
                calls.set(calls.get() + 1);
                true
            }
        ));
        assert!(cached_installer_availability(
            &cache,
            || failure_completed + INSTALLER_FAILURE_RETRY_DELAY,
            || {
                calls.set(calls.get() + 1);
                true
            }
        ));
        assert!(cached_installer_availability(&cache, Instant::now, || {
            calls.set(calls.get() + 1);
            false
        }));
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn winget_resolution_rejects_untrusted_paths() {
        let program_files = Path::new("C:/Program Files");
        let untrusted = [
            PathBuf::from("C:/Windows/System32/winget.exe"),
            PathBuf::from("C:/Users/alice/AppData/Local/Microsoft/WindowsApps/winget.exe"),
            PathBuf::from("C:/Users/alice/AppData/Local/Microsoft/WindowsApps/attacker/winget.exe"),
            program_files.join("WindowsApps/Microsoft.DesktopAppInstaller_1.25.390.0_x64__8wekyb3d8bbwe/cmd.exe"),
            program_files.join("WindowsApps/Contoso.AppInstaller_1.0_x64__8wekyb3d8bbwe/winget.exe"),
            program_files.join("WindowsApps/Microsoft.DesktopAppInstaller_1.25.390.0_x64__8wekyb3d8bbwe/../winget.exe"),
        ];

        assert!(untrusted
            .iter()
            .all(|candidate| !is_trusted_winget_path(candidate, program_files)));
        assert_eq!(resolve_winget_executable(&untrusted, program_files), None);
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn process_actions_fail_closed_off_windows() {
        assert!(super::list_native_apps()
            .iter()
            .all(|app| app.availability == NativeAppAvailability::Unavailable
                && !app.supports_overlay));
        assert!(install_native_app(NativeAppId::Discord).is_err());
        assert_eq!(
            get_mullvad_status().availability,
            NativeAppAvailability::Unavailable
        );
        assert!(install_mullvad().is_err());
        assert!(open_mullvad().is_err());
        assert_eq!(
            get_firefox_status().availability,
            NativeAppAvailability::Unavailable
        );
        assert!(launch_firefox_service(
            std::path::Path::new("/trusted/app-local-data"),
            "owner-test",
            FirefoxServiceId::Gmail
        )
        .is_err());
        assert!(install_firefox().is_err());
        assert!(list_browser_imports()
            .iter()
            .all(|browser| !browser.installed));
        assert!(open_browser_import(BrowserImportId::Chrome).is_err());
        assert!(begin_browser_account_import(
            std::path::Path::new("/trusted/osl-data"),
            "owner-test"
        )
        .is_err());
    }
}
