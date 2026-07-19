//! Fixed Windows-native service launch boundary.
//!
//! The trusted UI can select only one of the enum variants below. It cannot
//! supply an executable, path, URI, command-line option, or package source.
//! This keeps the launcher useful without turning a Tauri command into a
//! general process-execution primitive.

use serde::{Deserialize, Serialize};

#[cfg(target_os = "windows")]
use std::os::windows::ffi::OsStringExt;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
#[cfg(any(target_os = "windows", test))]
use std::path::{Path, PathBuf};
#[cfg(target_os = "windows")]
use std::process::{Command, Output, Stdio};
#[cfg(any(target_os = "windows", test))]
use std::sync::Mutex;
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
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BrowserImportId {
    Chrome,
    Edge,
    Firefox,
    Brave,
    Opera,
    Vivaldi,
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
pub struct NativeAppStatus {
    pub id: NativeAppId,
    pub display_name: &'static str,
    pub availability: NativeAppAvailability,
    /// True only when the current integration has a verified secondary-instance
    /// switch that keeps writable state inside an OSL-owned profile.
    pub isolated_profile_available: bool,
    /// Remains false until a service-specific Windows accessibility adapter
    /// can prove the exact account, conversation, recipients, and composer.
    pub supports_overlay: bool,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NativeAppAvailability {
    Installed,
    Installable,
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
    Instagram,
    Snapchat,
    X,
    Messenger,
    Gmail,
    Outlook,
    Proton,
    Yahoo,
    Aol,
    Gmx,
    Maildotcom,
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
    #[cfg(any(target_os = "windows", test))]
    ProgramFiles,
    #[cfg(any(target_os = "windows", test))]
    ProgramFilesX86,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct ExecutableCandidate {
    folder: KnownFolder,
    relative_path: &'static str,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct NativeAppManifest {
    id: NativeAppId,
    display_name: &'static str,
    package_id: &'static str,
    package_source: &'static str,
    candidates: &'static [ExecutableCandidate],
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg(any(target_os = "windows", test))]
struct BrowserImportManifest {
    id: BrowserImportId,
    display_name: &'static str,
    candidates: &'static [ExecutableCandidate],
    import_arguments: &'static [&'static str],
}

const DISCORD_CANDIDATES: &[ExecutableCandidate] = &[ExecutableCandidate {
    folder: KnownFolder::Local,
    relative_path: r"Discord\Update.exe",
}];

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

#[cfg(any(target_os = "windows", test))]
const WINDOWS_APPS_DIRECTORY: &str = "WindowsApps";

#[cfg(any(target_os = "windows", test))]
const DESKTOP_APP_INSTALLER_PREFIX: &str = "microsoft.desktopappinstaller_";

#[cfg(any(target_os = "windows", test))]
const DESKTOP_APP_INSTALLER_PUBLISHER_ID: &str = "_8wekyb3d8bbwe";

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
        package_id: "Discord.Discord",
        package_source: "winget",
        candidates: DISCORD_CANDIDATES,
    },
    NativeAppManifest {
        id: NativeAppId::Telegram,
        display_name: "Telegram",
        package_id: "Telegram.TelegramDesktop",
        package_source: "winget",
        candidates: TELEGRAM_CANDIDATES,
    },
    NativeAppManifest {
        id: NativeAppId::Signal,
        display_name: "Signal",
        package_id: "OpenWhisperSystems.Signal",
        package_source: "winget",
        candidates: SIGNAL_CANDIDATES,
    },
    NativeAppManifest {
        id: NativeAppId::Whatsapp,
        display_name: "WhatsApp",
        package_id: "9NKSQGP7F2NH",
        package_source: "msstore",
        candidates: WHATSAPP_CANDIDATES,
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
const OPERA_IMPORT_CANDIDATES: &[ExecutableCandidate] = &[
    ExecutableCandidate {
        folder: KnownFolder::Local,
        relative_path: r"Programs\Opera\launcher.exe",
    },
    ExecutableCandidate {
        folder: KnownFolder::ProgramFiles,
        relative_path: r"Opera\launcher.exe",
    },
    ExecutableCandidate {
        folder: KnownFolder::ProgramFilesX86,
        relative_path: r"Opera\launcher.exe",
    },
];

#[cfg(any(target_os = "windows", test))]
const VIVALDI_IMPORT_CANDIDATES: &[ExecutableCandidate] = &[
    ExecutableCandidate {
        folder: KnownFolder::Local,
        relative_path: r"Vivaldi\Application\vivaldi.exe",
    },
    ExecutableCandidate {
        folder: KnownFolder::ProgramFiles,
        relative_path: r"Vivaldi\Application\vivaldi.exe",
    },
    ExecutableCandidate {
        folder: KnownFolder::ProgramFilesX86,
        relative_path: r"Vivaldi\Application\vivaldi.exe",
    },
];

#[cfg(any(target_os = "windows", test))]
const BROWSER_IMPORTS: &[BrowserImportManifest] = &[
    BrowserImportManifest {
        id: BrowserImportId::Chrome,
        display_name: "Chrome",
        candidates: CHROME_IMPORT_CANDIDATES,
        import_arguments: &["--new-window", "chrome://password-manager/settings"],
    },
    BrowserImportManifest {
        id: BrowserImportId::Edge,
        display_name: "Edge",
        candidates: EDGE_IMPORT_CANDIDATES,
        import_arguments: &["--new-window", "edge://settings/passwords"],
    },
    BrowserImportManifest {
        id: BrowserImportId::Firefox,
        display_name: "Firefox",
        candidates: FIREFOX_CANDIDATES,
        import_arguments: &["--new-window", "about:logins"],
    },
    BrowserImportManifest {
        id: BrowserImportId::Brave,
        display_name: "Brave",
        candidates: BRAVE_IMPORT_CANDIDATES,
        import_arguments: &["--new-window", "brave://password-manager/settings"],
    },
    BrowserImportManifest {
        id: BrowserImportId::Opera,
        display_name: "Opera",
        candidates: OPERA_IMPORT_CANDIDATES,
        import_arguments: &["--new-window", "opera://password-manager/settings"],
    },
    BrowserImportManifest {
        id: BrowserImportId::Vivaldi,
        display_name: "Vivaldi",
        candidates: VIVALDI_IMPORT_CANDIDATES,
        import_arguments: &["--new-window", "vivaldi://password-manager/settings"],
    },
];

#[cfg(any(target_os = "windows", test))]
const FIREFOX_PACKAGE_ID: &str = "Mozilla.Firefox";

/// Firefox data shares the same owner-hashed namespace as every embedded
/// service profile. The browser itself receives only the resulting local path;
/// neither the OSL user id nor a caller-controlled component reaches it.
#[cfg(any(target_os = "windows", test))]
const FIREFOX_PROFILE_BASE_COMPONENT: &str = "service-profiles-v2";

#[cfg(any(target_os = "windows", test))]
const FIREFOX_PROFILE_COMPONENT: &str = "firefox-browser";

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
    (FirefoxServiceId::Instagram, "https://www.instagram.com/"),
    (FirefoxServiceId::Snapchat, "https://web.snapchat.com/"),
    (FirefoxServiceId::X, "https://x.com/"),
    // Meta retired the standalone Windows client and messenger.com now routes
    // desktop users into Facebook. Keep the OSL profile on Meta's current,
    // first-party messages surface instead of an unofficial wrapper.
    (
        FirefoxServiceId::Messenger,
        "https://www.facebook.com/messages/",
    ),
    (FirefoxServiceId::Gmail, "https://mail.google.com/"),
    (FirefoxServiceId::Outlook, "https://outlook.live.com/mail/"),
    (FirefoxServiceId::Proton, "https://mail.proton.me/"),
    (FirefoxServiceId::Yahoo, "https://mail.yahoo.com/"),
    (FirefoxServiceId::Aol, "https://mail.aol.com/"),
    (FirefoxServiceId::Gmx, "https://www.gmx.com/"),
    (FirefoxServiceId::Maildotcom, "https://www.mail.com/"),
];

#[cfg(any(target_os = "windows", test))]
fn manifest(id: NativeAppId) -> &'static NativeAppManifest {
    // Exhaustive enum input and a static manifest make this infallible. Avoid
    // accepting a service name string and accidentally widening the boundary.
    NATIVE_APPS
        .iter()
        .find(|manifest| manifest.id == id)
        .expect("every native app enum has a fixed manifest")
}

pub fn list_native_apps() -> Vec<NativeAppStatus> {
    list_native_apps_with_installer_probe(installer_available_for_listing)
}

fn list_native_apps_with_installer_probe(
    mut installer_available: impl FnMut() -> bool,
) -> Vec<NativeAppStatus> {
    let mut statuses: Vec<_> = NATIVE_APPS
        .iter()
        .map(|app| {
            let availability = if installed_executable(app).is_some() {
                NativeAppAvailability::Installed
            } else {
                NativeAppAvailability::Unavailable
            };
            NativeAppStatus {
                id: app.id,
                display_name: app.display_name,
                availability,
                isolated_profile_available: matches!(
                    app.id,
                    NativeAppId::Telegram | NativeAppId::Signal
                ),
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
            if status.availability == NativeAppAvailability::Unavailable {
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
                installed: browser_import_executable(browser).is_some(),
            })
            .collect()
    }
    #[cfg(not(any(target_os = "windows", test)))]
    {
        Vec::new()
    }
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
        spawn_detached(&executable, browser.import_arguments).map_err(|_| {
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
/// Detection, source choice, profile path, executable, arguments, and target
/// page are all fixed natively. Firefox—not OSL—reads supported browser data,
/// requests OS/browser authorization, and owns any CSV file picker.
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
        spawn_firefox_migration_wizard(&firefox, &profile)
            .map_err(|_| "The OSL Firefox migration wizard could not be opened".to_owned())?;
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

#[cfg(any(target_os = "windows", test))]
fn preferred_browser_import_source(sources: &[BrowserImportId]) -> Option<BrowserImportId> {
    // Prefer Firefox's direct Windows migrators before Chrome's specialized
    // manual CSV flow. The wizard remains visible and lets the user change the
    // detected source before confirming anything.
    [
        BrowserImportId::Edge,
        BrowserImportId::Brave,
        BrowserImportId::Opera,
        BrowserImportId::Vivaldi,
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

#[cfg(target_os = "windows")]
fn browser_import_executable(browser: &BrowserImportManifest) -> Option<PathBuf> {
    browser.candidates.iter().find_map(|candidate| {
        let executable = known_folder(candidate.folder)?.join(candidate.relative_path);
        executable.is_file().then_some(executable)
    })
}

#[cfg(all(test, not(target_os = "windows")))]
fn browser_import_executable(_browser: &BrowserImportManifest) -> Option<std::path::PathBuf> {
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
        let arguments = [
            "install",
            "--id",
            MULLVAD_PACKAGE_ID,
            "--exact",
            "--source",
            "winget",
            "--accept-source-agreements",
            "--accept-package-agreements",
            "--silent",
        ];
        spawn_detached(&winget, &arguments)
            .map_err(|_| "The Mullvad installer could not be started".to_owned())?;
        Ok(MullvadActionResult { started: true })
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("Mullvad installation is available only on Windows".to_owned())
    }
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
    MULLVAD_CANDIDATES.iter().find_map(|candidate| {
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
fn firefox_service_url(service_id: FirefoxServiceId) -> &'static str {
    FIREFOX_SERVICES
        .iter()
        .find_map(|(candidate, url)| (*candidate == service_id).then_some(*url))
        .expect("every Firefox service enum has one fixed URL")
}

#[cfg(target_os = "windows")]
fn firefox_executable() -> Option<PathBuf> {
    FIREFOX_CANDIDATES.iter().find_map(|candidate| {
        let executable = known_folder(candidate.folder)?.join(candidate.relative_path);
        executable.is_file().then_some(executable)
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
    Ok(canonical_profile)
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
fn firefox_executable() -> Option<std::path::PathBuf> {
    None
}

#[cfg(target_os = "windows")]
fn installed_executable(app: &NativeAppManifest) -> Option<PathBuf> {
    if app.id == NativeAppId::Discord {
        let install_root = known_folder(KnownFolder::Local)?.join("Discord");
        return newest_discord_executable_under(&install_root);
    }
    app.candidates.iter().find_map(|candidate| {
        let root = known_folder(candidate.folder)?;
        let executable = root.join(candidate.relative_path);
        executable.is_file().then_some(executable)
    })
}

#[cfg(any(target_os = "windows", test))]
pub(crate) fn newest_discord_executable_under(install_root: &Path) -> Option<PathBuf> {
    let mut candidates = std::fs::read_dir(install_root)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_str()?;
            let version = discord_version_key(name)?;
            let executable = entry.path().join("Discord.exe");
            executable.is_file().then_some((version, executable))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.0.cmp(&right.0));
    candidates.pop().map(|(_, executable)| executable)
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
fn installed_executable(_app: &NativeAppManifest) -> Option<std::path::PathBuf> {
    None
}

#[cfg(target_os = "windows")]
fn installer_executable() -> Option<PathBuf> {
    let program_files = known_folder(KnownFolder::ProgramFiles)?;
    let executable = resolve_winget_executable(&app_installer_winget_candidates(), &program_files)?;
    executable.is_file().then_some(executable)
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
fn spawn_firefox_tab(executable: &Path, profile: &Path, url: &str) -> std::io::Result<()> {
    Command::new(executable)
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
fn spawn_firefox_migration_wizard(executable: &Path, profile: &Path) -> std::io::Result<()> {
    Command::new(executable)
        .arg("--no-remote")
        .arg("--profile")
        .arg(profile)
        .arg("--migration")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000)
        .spawn()
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
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
        assert_eq!(NATIVE_APPS.len(), 4);
        for (index, app) in NATIVE_APPS.iter().enumerate() {
            assert!(!app.display_name.is_empty());
            assert!(!app.package_id.is_empty());
            assert!(!app.package_id.starts_with('-'));
            assert!(app
                .package_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'.'));
            assert!(matches!(app.package_source, "winget" | "msstore"));
            assert!(!app.candidates.is_empty());
            assert!(NATIVE_APPS[..index]
                .iter()
                .all(|previous| previous.id != app.id));
            for candidate in app.candidates {
                assert!(!candidate.relative_path.is_empty());
                assert!(!candidate.relative_path.starts_with(['/', '\\']));
                assert!(!candidate.relative_path.contains(".."));
                assert!(!candidate.relative_path.contains(':'));
                assert!(candidate.relative_path.ends_with(".exe"));
            }
        }
        assert_eq!(manifest(NativeAppId::Discord).package_id, "Discord.Discord");
        assert_eq!(manifest(NativeAppId::Whatsapp).package_source, "msstore");
    }

    #[test]
    fn mullvad_actions_have_one_fixed_safe_manifest() {
        assert_eq!(MULLVAD_PACKAGE_ID, "MullvadVPN.MullvadVPN");
        assert_eq!(MULLVAD_CANDIDATES.len(), 2);
        for candidate in MULLVAD_CANDIDATES {
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
        assert_eq!(FIREFOX_SERVICES.len(), 11);
        assert_eq!(FIREFOX_PACKAGE_ID, "Mozilla.Firefox");
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
            (
                BrowserImportId::Vivaldi,
                "vivaldi://password-manager/settings",
            ),
        ];
        assert_eq!(BROWSER_IMPORTS.len(), 6);
        for (index, browser) in BROWSER_IMPORTS.iter().enumerate() {
            assert!(!browser.display_name.is_empty());
            assert!(!browser.candidates.is_empty());
            assert_eq!(browser.import_arguments.first(), Some(&"--new-window"));
            assert_eq!(browser.import_arguments.len(), 2);
            assert_eq!(
                browser.import_arguments[1],
                expected_targets
                    .iter()
                    .find_map(|(id, target)| (*id == browser.id).then_some(*target))
                    .expect("every browser id has one reviewed internal target")
            );
            assert!(!browser.import_arguments[1].starts_with("http:"));
            assert!(!browser.import_arguments[1].starts_with("https:"));
            assert!(!browser.import_arguments[1].starts_with("file:"));
            assert!(!browser.import_arguments[1].starts_with("javascript:"));
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
            browser_import_manifest(BrowserImportId::Vivaldi).display_name,
            "Vivaldi"
        );
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
            preferred_browser_import_source(&[BrowserImportId::Firefox, BrowserImportId::Vivaldi,]),
            Some(BrowserImportId::Vivaldi)
        );
    }

    #[test]
    fn only_packaged_app_installer_winget_is_trusted() {
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
    fn native_app_refresh_probes_app_installer_once() {
        let calls = Cell::new(0);
        let statuses = list_native_apps_with_installer_probe(|| {
            calls.set(calls.get() + 1);
            true
        });

        assert_eq!(calls.get(), 1);
        assert_eq!(statuses.len(), NATIVE_APPS.len());
        #[cfg(not(target_os = "windows"))]
        assert!(statuses
            .iter()
            .all(|status| status.availability == NativeAppAvailability::Installable));
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
        assert!(list_native_apps()
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
            FirefoxServiceId::Instagram
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
