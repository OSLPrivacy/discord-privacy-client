//! Fixed Windows-native service launch boundary.
//!
//! The trusted UI can select only one of the enum variants below. It cannot
//! supply an executable, path, URI, command-line option, or package source.
//! This keeps the launcher useful without turning a Tauri command into a
//! general process-execution primitive.

use serde::{Deserialize, Serialize};

use crate::windows_executable_trust::ExecutablePublisher;
#[cfg(target_os = "windows")]
use crate::windows_executable_trust::{verify_executable, TrustedExecutable};

#[cfg(target_os = "windows")]
use std::os::windows::ffi::{OsStrExt, OsStringExt};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
#[cfg(any(target_os = "windows", test))]
use std::path::{Path, PathBuf};
#[cfg(target_os = "windows")]
use std::process::{Command, Output, Stdio};
#[cfg(target_os = "windows")]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(any(target_os = "windows", test))]
use std::sync::Mutex;
#[cfg(target_os = "windows")]
use std::sync::OnceLock;
#[cfg(target_os = "windows")]
use std::thread;
#[cfg(any(target_os = "windows", test))]
use std::time::{Duration, Instant};
#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_INVALID_PARAMETER, STILL_ACTIVE, WAIT_OBJECT_0,
};
#[cfg(target_os = "windows")]
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
#[cfg(target_os = "windows")]
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
#[cfg(target_os = "windows")]
use windows_sys::Win32::System::StationsAndDesktops::{CloseDesktop, CreateDesktopW};
#[cfg(target_os = "windows")]
use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;
#[cfg(target_os = "windows")]
use windows_sys::Win32::System::Threading::{
    CreateProcessW, GetExitCodeProcess, OpenProcess, ResumeThread, TerminateProcess,
    WaitForSingleObject, CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, INFINITE,
    PROCESS_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
    STARTUPINFOW,
};

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
    pub password_follow_up_sources: Vec<BrowserImportId>,
    pub session_only_sources: Vec<BrowserImportId>,
    pub started: bool,
    pub mode: &'static str,
    pub source_selected: bool,
    pub manual_fallback: Option<String>,
}

const MAX_PROTECTED_BROWSER_IMPORT_SOURCES: usize = 6;

fn validate_protected_browser_import_sources(
    selected_sources: Vec<BrowserImportId>,
    available_sources: &[BrowserImportId],
) -> Result<Vec<BrowserImportId>, String> {
    if selected_sources.is_empty() || selected_sources.len() > MAX_PROTECTED_BROWSER_IMPORT_SOURCES
    {
        return Err("Choose between one and six browsers to import".to_owned());
    }
    let mut unique = Vec::with_capacity(selected_sources.len());
    for source in selected_sources {
        if unique.contains(&source) {
            return Err("A browser was selected more than once".to_owned());
        }
        if !available_sources.contains(&source) {
            return Err("A selected browser is unavailable".to_owned());
        }
        unique.push(source);
    }
    Ok(unique)
}

fn browser_import_uses_existing_session(source: BrowserImportId) -> bool {
    matches!(source, BrowserImportId::Firefox | BrowserImportId::Opera)
}

#[cfg(any(target_os = "windows", test))]
fn process_lineage(root_process_id: u32, processes: &[(u32, u32)]) -> Vec<u32> {
    let mut lineage = vec![root_process_id];
    for _ in 0..processes.len() {
        let mut changed = false;
        for &(process_id, parent_process_id) in processes {
            if process_id != 0
                && !lineage.contains(&process_id)
                && lineage.contains(&parent_process_id)
            {
                lineage.push(process_id);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    lineage
}

#[cfg(target_os = "windows")]
struct HiddenFirefoxProcess {
    process_handle: isize,
    job_handle: isize,
    process_id: u32,
    desktop_handle: isize,
    desktop_name: String,
}

#[cfg(target_os = "windows")]
impl HiddenFirefoxProcess {
    fn id(&self) -> u32 {
        self.process_id
    }

    fn is_running(&self) -> std::io::Result<bool> {
        let mut exit_code = 0u32;
        if unsafe { GetExitCodeProcess(self.process_handle as _, &mut exit_code) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(exit_code == STILL_ACTIVE as u32)
    }

    fn terminate_tree(&self) -> std::io::Result<()> {
        let process_ids = firefox_process_lineage(self.process_id)?;
        let mut handles = Vec::with_capacity(process_ids.len());
        handles.push((self.process_handle as _, false));
        for process_id in process_ids {
            if process_id == self.process_id {
                continue;
            }
            let handle = unsafe {
                OpenProcess(
                    PROCESS_TERMINATE | PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
                    0,
                    process_id,
                )
            };
            if handle.is_null() {
                let error = std::io::Error::last_os_error();
                // A captured descendant can exit between the snapshot and
                // OpenProcess. Access-denied and every other failure remain a
                // hard cleanup error; only a no-longer-valid PID is skipped.
                if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) {
                    continue;
                }
                for (handle, close_after) in handles {
                    if close_after {
                        unsafe { CloseHandle(handle) };
                    }
                }
                return Err(error);
            }
            handles.push((handle, true));
        }

        // The job is the primary containment boundary. The exact captured PID
        // lineage below is a bounded fallback for Firefox children that did
        // not exit with the job on a real Windows host.
        let _ = unsafe { TerminateJobObject(self.job_handle as _, 1) };
        let mut result = Ok(());
        for (handle, close_after) in handles {
            let mut exit_code = 0u32;
            let mut handle_error = None;
            if unsafe { GetExitCodeProcess(handle, &mut exit_code) } == 0 {
                handle_error = Some(std::io::Error::last_os_error());
            } else if exit_code == STILL_ACTIVE as u32
                && unsafe { TerminateProcess(handle, 1) } == 0
            {
                let terminate_error = std::io::Error::last_os_error();
                if unsafe { GetExitCodeProcess(handle, &mut exit_code) } == 0
                    || exit_code == STILL_ACTIVE as u32
                {
                    handle_error = Some(terminate_error);
                }
            }
            if handle_error.is_none()
                && unsafe { WaitForSingleObject(handle, 5_000) } != WAIT_OBJECT_0
            {
                handle_error = Some(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "OSL Firefox process did not exit before the cleanup deadline",
                ));
            }
            if handle_error.is_none()
                && (unsafe { GetExitCodeProcess(handle, &mut exit_code) } == 0
                    || exit_code == STILL_ACTIVE as u32)
            {
                handle_error = Some(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "OSL Firefox process remained active after termination",
                ));
            }
            if result.is_ok() {
                if let Some(error) = handle_error {
                    result = Err(error);
                }
            }
            if close_after {
                unsafe { CloseHandle(handle) };
            }
        }
        result
    }

    fn wait(&self) -> std::io::Result<()> {
        (unsafe { WaitForSingleObject(self.process_handle as _, INFINITE) } == WAIT_OBJECT_0)
            .then_some(())
            .ok_or_else(std::io::Error::last_os_error)
    }
}

#[cfg(target_os = "windows")]
fn firefox_process_lineage(root_process_id: u32) -> std::io::Result<Vec<u32>> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot.is_null() || snapshot == -1isize as _ {
        return Err(std::io::Error::last_os_error());
    }
    let mut processes = Vec::new();
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    let mut found = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
    while found {
        if processes.len() >= 16_384 {
            unsafe { CloseHandle(snapshot) };
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Windows process snapshot exceeded its safety bound",
            ));
        }
        processes.push((entry.th32ProcessID, entry.th32ParentProcessID));
        found = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
    }
    unsafe { CloseHandle(snapshot) };
    Ok(process_lineage(root_process_id, &processes))
}

#[cfg(target_os = "windows")]
impl Drop for HiddenFirefoxProcess {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.job_handle as _);
            CloseHandle(self.process_handle as _);
            CloseDesktop(self.desktop_handle as _);
        }
    }
}

#[cfg(target_os = "windows")]
fn protected_browser_import_process() -> &'static Mutex<Option<HiddenFirefoxProcess>> {
    static PROCESS: OnceLock<Mutex<Option<HiddenFirefoxProcess>>> = OnceLock::new();
    PROCESS.get_or_init(|| Mutex::new(None))
}

#[cfg(target_os = "windows")]
fn close_protected_browser_import_process() -> Result<(), String> {
    let mut process = protected_browser_import_process()
        .lock()
        .map_err(|_| "The OSL Firefox import process state is unavailable".to_owned())?;
    let Some(child) = process.take() else {
        return Ok(());
    };
    let root_process_id = child.id();
    match child.is_running() {
        Ok(false) => {}
        Ok(true) => {
            if crate::firefox_migration_coordinator::close(child.id(), &child.desktop_name).is_ok()
            {
                let deadline = Instant::now() + Duration::from_secs(1);
                loop {
                    match child.is_running() {
                        Ok(false) => break,
                        Ok(true) if Instant::now() < deadline => {
                            thread::sleep(Duration::from_millis(50));
                        }
                        Ok(true) | Err(_) => break,
                    }
                }
            }
        }
        // Cleanup below uses the retained exact process handle and a fresh
        // rooted lineage snapshot, so it remains safe even when this status
        // query itself fails.
        Err(_) => {}
    }

    // Enforce cleanup even when Firefox's root has already exited: retained
    // descendants can outlive it, and are still discoverable through the
    // exact root PID lineage in the Toolhelp snapshot.
    child
        .terminate_tree()
        .map_err(|_| "The OSL Firefox import window could not be closed".to_owned())?;
    child
        .wait()
        .map_err(|_| "The OSL Firefox import process could not be reaped".to_owned())?;
    thread::sleep(Duration::from_millis(200));
    crate::firefox_migration_coordinator::is_closed(root_process_id, &child.desktop_name)
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

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct NativeAppManifest {
    id: NativeAppId,
    display_name: &'static str,
    package_id: &'static str,
    package_source: &'static str,
    candidates: &'static [ExecutableCandidate],
    publisher: Option<ExecutablePublisher>,
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

#[cfg(any(target_os = "windows", test))]
const WHATSAPP_PACKAGE_FAMILY_NAME: &str = "5319275A.WhatsAppDesktop_cv1g1gvanyjgm";

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
        package_id: "Discord.Discord",
        package_source: "winget",
        candidates: DISCORD_CANDIDATES,
        publisher: Some(ExecutablePublisher::Discord),
    },
    NativeAppManifest {
        id: NativeAppId::Telegram,
        display_name: "Telegram",
        package_id: "Telegram.TelegramDesktop",
        package_source: "winget",
        candidates: TELEGRAM_CANDIDATES,
        publisher: Some(ExecutablePublisher::Telegram),
    },
    NativeAppManifest {
        id: NativeAppId::Signal,
        display_name: "Signal",
        package_id: "OpenWhisperSystems.Signal",
        package_source: "winget",
        candidates: SIGNAL_CANDIDATES,
        publisher: Some(ExecutablePublisher::Signal),
    },
    NativeAppManifest {
        id: NativeAppId::Whatsapp,
        display_name: "WhatsApp",
        package_id: "9NKSQGP7F2NH",
        package_source: "msstore",
        candidates: WHATSAPP_CANDIDATES,
        publisher: None,
    },
    NativeAppManifest {
        id: NativeAppId::Outlook,
        display_name: "Outlook",
        // Outlook is commonly provisioned with Microsoft 365 rather than as
        // an independently safe winget action. Missing Outlook remains
        // unavailable instead of exposing a guessed installer command.
        package_id: "",
        package_source: "unavailable",
        candidates: OUTLOOK_CLASSIC_CANDIDATES,
        publisher: Some(ExecutablePublisher::Microsoft),
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
    (FirefoxServiceId::Icloud, "https://www.icloud.com/mail/"),
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

#[cfg(any(target_os = "windows", test))]
pub(crate) fn native_app_publisher(id: NativeAppId) -> Option<ExecutablePublisher> {
    manifest(id).publisher
}

fn isolated_native_profile_available(id: NativeAppId) -> bool {
    matches!(id, NativeAppId::Discord | NativeAppId::Telegram)
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
            NativeAppStatus {
                id: app.id,
                display_name: app.display_name,
                availability,
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
        let result = begin_protected_browser_import(
            app_local_data_dir,
            owner_osl_user_id,
            vec![preferred_source],
            0,
        )?;
        Ok(BrowserAccountImportResult {
            preferred_source,
            detected_sources,
            opened: true,
            mode: "firefoxMigrationWizard",
            manual_export_required: result
                .password_follow_up_sources
                .contains(&preferred_source),
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
    _owner_window: isize,
) -> Result<ProtectedBrowserImportResult, String> {
    #[cfg(target_os = "windows")]
    {
        close_protected_browser_import_process()?;
        let available_sources = BROWSER_IMPORTS
            .iter()
            .filter(|browser| browser_import_executable(browser).is_some())
            .map(|browser| browser.id)
            .collect::<Vec<_>>();
        let unique =
            validate_protected_browser_import_sources(selected_sources, &available_sources)?;
        let firefox = firefox_executable().ok_or_else(|| {
            "Firefox is required for OSL's browser-owned account migration".to_owned()
        })?;
        let profile = ensure_firefox_profile(app_local_data_dir, owner_osl_user_id)?;
        let mut password_follow_up_sources = Vec::new();
        let mut session_only_sources = Vec::new();
        for (index, source) in unique.iter().copied().enumerate() {
            // Current Firefox cannot complete Firefox-profile migration here,
            // and Opera's migration never completed in a bounded hidden live
            // run. Preserve those real sessions through OSL's existing-browser
            // companion instead of opening a flow that cannot finish.
            if browser_import_uses_existing_session(source) {
                session_only_sources.push(source);
                continue;
            }
            let firefox_process = spawn_firefox_migration_wizard(&firefox, &profile)
                .map_err(|_| "The OSL Firefox migration wizard could not be opened".to_owned())?;
            if !firefox_process
                .is_running()
                .map_err(|_| "The OSL Firefox import process could not be verified".to_owned())?
            {
                return Err("The OSL Firefox migration wizard closed before opening".to_owned());
            }
            let process_id = firefox_process.id();
            let desktop_name = firefox_process.desktop_name.clone();
            *protected_browser_import_process()
                .lock()
                .map_err(|_| "The OSL Firefox import process state is unavailable".to_owned())? =
                Some(firefox_process);
            match crate::firefox_migration_coordinator::coordinate(
                process_id,
                source,
                0,
                &desktop_name,
            ) {
                Ok(true) => password_follow_up_sources.push(source),
                Ok(false) => {}
                Err(error) => {
                    close_protected_browser_import_process()?;
                    return Err(error);
                }
            }
            // Firefox can leave a normal new-tab window behind after Done.
            // Close the exact retained OSL-owned process before the next
            // source; ordinary Firefox processes and profiles are untouched.
            close_protected_browser_import_process()?;
            if index + 1 < unique.len() {
                thread::sleep(Duration::from_millis(300));
            }
        }
        Ok(ProtectedBrowserImportResult {
            selected_sources: unique,
            password_follow_up_sources,
            session_only_sources,
            started: true,
            mode: "firefoxMigrationWizard",
            source_selected: true,
            manual_fallback: None,
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (
            app_local_data_dir,
            owner_osl_user_id,
            selected_sources,
            _owner_window,
        );
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
pub fn install_discord_dedicated_channel() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let winget = installer_executable()
            .ok_or_else(|| "Windows App Installer (winget) is unavailable".to_owned())?;
        let arguments = [
            "install",
            "--id",
            DISCORD_DEDICATED_PACKAGE_ID,
            "--exact",
            "--source",
            "winget",
            "--accept-source-agreements",
            "--accept-package-agreements",
            "--silent",
        ];
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
    if service_id == FirefoxServiceId::Outlook {
        return Err("Outlook opens only through the verified native app".to_owned());
    }
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
    FIREFOX_SERVICES
        .iter()
        .find_map(|(candidate, url)| (*candidate == service_id).then_some(*url))
        .expect("every Firefox service enum has one fixed URL")
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

#[cfg(any(target_os = "windows", test))]
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
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS};
    use windows_sys::Win32::Storage::Packaging::Appx::GetPackagesByPackageFamily;

    let family = std::ffi::OsStr::new(WHATSAPP_PACKAGE_FAMILY_NAME)
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
                .and_then(|full_name| package_path_from_full_name(&full_name))
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
fn outlook_store_package_root() -> Option<PathBuf> {
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
            is_trusted_outlook_package_path(&path, &program_files).then_some(path)
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
) -> std::io::Result<HiddenFirefoxProcess> {
    static NEXT_DESKTOP: AtomicU64 = AtomicU64::new(1);
    let desktop_name = format!(
        "OSLBrowserImport-{}-{}",
        std::process::id(),
        NEXT_DESKTOP.fetch_add(1, Ordering::Relaxed)
    );
    let desktop_name_wide = desktop_name
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let desktop = unsafe {
        CreateDesktopW(
            desktop_name_wide.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            0x1000_0000,
            std::ptr::null(),
        )
    };
    if desktop.is_null() {
        return Err(std::io::Error::last_os_error());
    }
    let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if job.is_null() {
        unsafe { CloseDesktop(desktop) };
        return Err(std::io::Error::last_os_error());
    }
    let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    if unsafe {
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    } == 0
    {
        unsafe {
            CloseHandle(job);
            CloseDesktop(desktop);
        }
        return Err(std::io::Error::last_os_error());
    }

    let mut command_line = Vec::<u16>::new();
    for argument in [
        executable.path().as_os_str(),
        std::ffi::OsStr::new("--no-remote"),
        std::ffi::OsStr::new("--profile"),
        profile.as_os_str(),
        std::ffi::OsStr::new(FIREFOX_WAIT_FOR_BROWSER_SWITCH),
        std::ffi::OsStr::new(FIREFOX_MIGRATION_SWITCH),
    ] {
        if !command_line.is_empty() {
            command_line.push(b' ' as u16);
        }
        append_windows_quoted_argument(&mut command_line, argument);
    }
    command_line.push(0);
    let executable_wide = executable
        .path()
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
    startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    startup.lpDesktop = desktop_name_wide.as_ptr() as *mut _;
    let mut process: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    let created = unsafe {
        CreateProcessW(
            executable_wide.as_ptr(),
            command_line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            CREATE_NO_WINDOW | CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT,
            std::ptr::null(),
            std::ptr::null(),
            &startup,
            &mut process,
        )
    };
    if created == 0 {
        unsafe {
            CloseHandle(job);
            CloseDesktop(desktop);
        }
        return Err(std::io::Error::last_os_error());
    }
    if unsafe { AssignProcessToJobObject(job, process.hProcess) } == 0
        || unsafe { ResumeThread(process.hThread) } == u32::MAX
    {
        unsafe {
            TerminateProcess(process.hProcess, 1);
            CloseHandle(process.hThread);
            CloseHandle(process.hProcess);
            CloseHandle(job);
            CloseDesktop(desktop);
        }
        return Err(std::io::Error::last_os_error());
    }
    unsafe { CloseHandle(process.hThread) };
    Ok(HiddenFirefoxProcess {
        process_handle: process.hProcess as isize,
        job_handle: job as isize,
        process_id: process.dwProcessId,
        desktop_handle: desktop as isize,
        desktop_name,
    })
}

#[cfg(target_os = "windows")]
fn append_windows_quoted_argument(command_line: &mut Vec<u16>, argument: &std::ffi::OsStr) {
    command_line.push(b'"' as u16);
    let mut backslashes = 0usize;
    for unit in argument.encode_wide() {
        if unit == b'\\' as u16 {
            backslashes += 1;
            continue;
        }
        if unit == b'"' as u16 {
            command_line.extend(std::iter::repeat_n(b'\\' as u16, backslashes * 2 + 1));
        } else {
            command_line.extend(std::iter::repeat_n(b'\\' as u16, backslashes));
        }
        backslashes = 0;
        command_line.push(unit);
    }
    command_line.extend(std::iter::repeat_n(b'\\' as u16, backslashes * 2));
    command_line.push(b'"' as u16);
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
        assert_eq!(NATIVE_APPS.len(), 5);
        for (index, app) in NATIVE_APPS.iter().enumerate() {
            assert!(!app.display_name.is_empty());
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
    fn outlook_has_no_firefox_fallback() {
        assert!(
            launch_firefox_service(Path::new("."), "owner-a", FirefoxServiceId::Outlook).is_err()
        );
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
    fn dedicated_discord_fallback_is_one_fixed_official_channel() {
        assert_eq!(DISCORD_DEDICATED_PACKAGE_ID, "Discord.Discord.PTB");
        assert!(install_discord_dedicated_channel().is_err());
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
        assert_eq!(FIREFOX_SERVICES.len(), 12);
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
    fn protected_browser_import_validates_the_entire_bounded_queue() {
        let available = [
            BrowserImportId::Chrome,
            BrowserImportId::Edge,
            BrowserImportId::Firefox,
            BrowserImportId::Brave,
            BrowserImportId::Opera,
            BrowserImportId::DuckDuckGo,
        ];
        assert_eq!(
            validate_protected_browser_import_sources(
                vec![
                    BrowserImportId::Edge,
                    BrowserImportId::Chrome,
                    BrowserImportId::Firefox,
                ],
                &available,
            ),
            Ok(vec![
                BrowserImportId::Edge,
                BrowserImportId::Chrome,
                BrowserImportId::Firefox,
            ])
        );
        assert!(validate_protected_browser_import_sources(Vec::new(), &available).is_err());
        assert!(validate_protected_browser_import_sources(
            vec![BrowserImportId::Chrome, BrowserImportId::Chrome],
            &available,
        )
        .is_err());
        assert!(validate_protected_browser_import_sources(
            vec![BrowserImportId::DuckDuckGo],
            &available[..5],
        )
        .is_err());
    }

    #[test]
    fn unsupported_migration_sources_reuse_existing_sessions() {
        assert!(browser_import_uses_existing_session(
            BrowserImportId::Firefox
        ));
        assert!(browser_import_uses_existing_session(BrowserImportId::Opera));
        assert!(!browser_import_uses_existing_session(
            BrowserImportId::Chrome
        ));
        assert!(!browser_import_uses_existing_session(BrowserImportId::Edge));
        assert!(!browser_import_uses_existing_session(
            BrowserImportId::Brave
        ));
        assert!(!browser_import_uses_existing_session(
            BrowserImportId::DuckDuckGo
        ));
    }

    #[test]
    fn process_lineage_is_rooted_and_excludes_unrelated_processes() {
        let processes = [
            (10, 1),
            (11, 10),
            (12, 11),
            (13, 10),
            (20, 1),
            (21, 20),
            (30, 999),
        ];
        let mut lineage = process_lineage(10, &processes);
        lineage.sort_unstable();
        assert_eq!(lineage, vec![10, 11, 12, 13]);
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
        let executable = whatsapp_store_executable_path()
            .expect("current user's exact WhatsApp AppX registration should resolve");
        assert!(path_file_name_eq(&executable, "WhatsApp.Root.exe"));
        let package_root = executable.parent().expect("packaged executable has a root");
        let program_files = known_folder(KnownFolder::ProgramFiles)
            .expect("Program Files known folder should resolve");
        assert!(is_trusted_whatsapp_package_path(
            package_root,
            &program_files
        ));
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
