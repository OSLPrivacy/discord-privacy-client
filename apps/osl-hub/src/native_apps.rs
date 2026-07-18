//! Fixed Windows-native service launch boundary.
//!
//! The trusted UI can select only one of the enum variants below. It cannot
//! supply an executable, path, URI, command-line option, or package source.
//! This keeps the launcher useful without turning a Tauri command into a
//! general process-execution primitive.

use serde::{Deserialize, Serialize};

#[cfg(target_os = "windows")]
use std::path::{Path, PathBuf};
#[cfg(target_os = "windows")]
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NativeAppId {
    Discord,
    Telegram,
    Signal,
    Whatsapp,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeAppStatus {
    pub id: NativeAppId,
    pub display_name: &'static str,
    pub availability: NativeAppAvailability,
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
pub struct NativeLaunchResult {
    pub id: NativeAppId,
    pub started: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeInstallResult {
    pub id: NativeAppId,
    pub started: bool,
    pub package_id: &'static str,
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
    arguments: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct NativeAppManifest {
    id: NativeAppId,
    display_name: &'static str,
    package_id: &'static str,
    package_source: &'static str,
    candidates: &'static [ExecutableCandidate],
}

const DISCORD_CANDIDATES: &[ExecutableCandidate] = &[ExecutableCandidate {
    folder: KnownFolder::Local,
    relative_path: r"Discord\Update.exe",
    arguments: &["--processStart", "Discord.exe"],
}];

const TELEGRAM_CANDIDATES: &[ExecutableCandidate] = &[
    ExecutableCandidate {
        folder: KnownFolder::Roaming,
        relative_path: r"Telegram Desktop\Telegram.exe",
        arguments: &[],
    },
    ExecutableCandidate {
        folder: KnownFolder::Local,
        relative_path: r"Programs\Telegram Desktop\Telegram.exe",
        arguments: &[],
    },
];

const SIGNAL_CANDIDATES: &[ExecutableCandidate] = &[ExecutableCandidate {
    folder: KnownFolder::Local,
    relative_path: r"Programs\signal-desktop\Signal.exe",
    arguments: &[],
}];

const WHATSAPP_CANDIDATES: &[ExecutableCandidate] = &[
    ExecutableCandidate {
        folder: KnownFolder::Local,
        relative_path: r"Microsoft\WindowsApps\WhatsApp.exe",
        arguments: &[],
    },
    ExecutableCandidate {
        folder: KnownFolder::Local,
        relative_path: r"WhatsApp\WhatsApp.exe",
        arguments: &[],
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
const FIREFOX_PACKAGE_ID: &str = "Mozilla.Firefox";

/// One per-Windows-user browsing profile for the initial native-browser
/// prototype. It lives inside Tauri's app-local-data service-profile target,
/// so full OSL cleanup removes its cookies without touching Firefox's default
/// profile. A later multi-identity design must split this namespace by a
/// validated OSL identity identifier before advertising account isolation.
#[cfg(any(target_os = "windows", test))]
const FIREFOX_PROFILE_COMPONENTS: &[&str] = &["service-profiles-v2", "firefox-shared"];

#[cfg(any(target_os = "windows", test))]
const FIREFOX_CANDIDATES: &[ExecutableCandidate] = &[
    ExecutableCandidate {
        folder: KnownFolder::ProgramFiles,
        relative_path: r"Mozilla Firefox\firefox.exe",
        arguments: &[],
    },
    ExecutableCandidate {
        folder: KnownFolder::ProgramFilesX86,
        relative_path: r"Mozilla Firefox\firefox.exe",
        arguments: &[],
    },
    ExecutableCandidate {
        folder: KnownFolder::Local,
        relative_path: r"Mozilla Firefox\firefox.exe",
        arguments: &[],
    },
    ExecutableCandidate {
        folder: KnownFolder::Local,
        relative_path: r"Programs\Mozilla Firefox\firefox.exe",
        arguments: &[],
    },
];

#[cfg(any(target_os = "windows", test))]
const FIREFOX_SERVICES: &[(FirefoxServiceId, &str)] = &[
    (FirefoxServiceId::Instagram, "https://www.instagram.com/"),
    (FirefoxServiceId::Snapchat, "https://web.snapchat.com/"),
    (FirefoxServiceId::X, "https://x.com/"),
    (FirefoxServiceId::Messenger, "https://www.messenger.com/"),
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
    NATIVE_APPS
        .iter()
        .map(|app| {
            let availability = if installed_executable(app).is_some() {
                NativeAppAvailability::Installed
            } else if installer_executable().is_some() {
                NativeAppAvailability::Installable
            } else {
                NativeAppAvailability::Unavailable
            };
            NativeAppStatus {
                id: app.id,
                display_name: app.display_name,
                availability,
                supports_overlay: false,
            }
        })
        .collect()
}

pub fn launch_native_app(id: NativeAppId) -> Result<NativeLaunchResult, String> {
    #[cfg(target_os = "windows")]
    {
        let app = manifest(id);
        let (executable, arguments) = installed_executable(app).ok_or_else(|| {
            format!(
                "{} is not installed in a supported Windows location",
                app.display_name
            )
        })?;
        spawn_detached(&executable, arguments)
            .map_err(|_| format!("{} could not be launched", app.display_name))?;
        Ok(NativeLaunchResult { id, started: true })
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = id;
        Err("Native app launching is available only on Windows".to_owned())
    }
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

pub fn get_firefox_status() -> FirefoxStatus {
    let availability = if firefox_executable().is_some() {
        NativeAppAvailability::Installed
    } else if installer_executable().is_some() {
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
    service_id: FirefoxServiceId,
) -> Result<FirefoxLaunchResult, String> {
    #[cfg(target_os = "windows")]
    {
        let firefox = firefox_executable()
            .ok_or_else(|| "Firefox is not installed in a supported Windows location".to_owned())?;
        let profile = ensure_firefox_profile(app_local_data_dir)?;
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
        let _ = app_local_data_dir;
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
fn ensure_firefox_profile(app_local_data_dir: &Path) -> Result<PathBuf, String> {
    if !app_local_data_dir.is_absolute() || app_local_data_dir.parent().is_none() {
        return Err("The OSL app-local-data directory is invalid".to_owned());
    }
    let base = app_local_data_dir.to_owned();
    ensure_plain_directory(&base)?;
    let canonical_base = base
        .canonicalize()
        .map_err(|_| "The OSL app-local-data directory could not be verified".to_owned())?;
    let mut profile = base;
    for component in FIREFOX_PROFILE_COMPONENTS {
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
pub(crate) fn firefox_profile_relative_path() -> std::path::PathBuf {
    FIREFOX_PROFILE_COMPONENTS.iter().collect()
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
fn installed_executable(app: &NativeAppManifest) -> Option<(PathBuf, &'static [&'static str])> {
    app.candidates.iter().find_map(|candidate| {
        let root = known_folder(candidate.folder)?;
        let executable = root.join(candidate.relative_path);
        executable
            .is_file()
            .then_some((executable, candidate.arguments))
    })
}

#[cfg(not(target_os = "windows"))]
fn installed_executable(
    _app: &NativeAppManifest,
) -> Option<(std::path::PathBuf, &'static [&'static str])> {
    None
}

#[cfg(target_os = "windows")]
fn installer_executable() -> Option<PathBuf> {
    let executable = known_folder(KnownFolder::Local)?.join(r"Microsoft\WindowsApps\winget.exe");
    executable.is_file().then_some(executable)
}

#[cfg(not(target_os = "windows"))]
fn installer_executable() -> Option<std::path::PathBuf> {
    None
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_is_exhaustive_unique_and_uses_fixed_packages() {
        assert_eq!(NATIVE_APPS.len(), 4);
        for (index, app) in NATIVE_APPS.iter().enumerate() {
            assert!(!app.display_name.is_empty());
            assert!(!app.package_id.is_empty());
            assert!(matches!(app.package_source, "winget" | "msstore"));
            assert!(!app.candidates.is_empty());
            assert!(NATIVE_APPS[..index]
                .iter()
                .all(|previous| previous.id != app.id));
            for candidate in app.candidates {
                assert!(!candidate.relative_path.is_empty());
                assert!(!candidate.relative_path.starts_with(['/', '\\']));
                assert!(!candidate.relative_path.contains(".."));
            }
        }
        assert_eq!(manifest(NativeAppId::Discord).package_id, "Discord.Discord");
        assert_eq!(manifest(NativeAppId::Whatsapp).package_source, "msstore");
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
    fn firefox_manifest_is_exhaustive_and_https_only() {
        assert_eq!(FIREFOX_SERVICES.len(), 11);
        assert_eq!(FIREFOX_PACKAGE_ID, "Mozilla.Firefox");
        assert_eq!(FIREFOX_CANDIDATES.len(), 4);
        assert_eq!(
            FIREFOX_PROFILE_COMPONENTS,
            ["service-profiles-v2", "firefox-shared"]
        );
        assert!(firefox_profile_relative_path().starts_with("service-profiles-v2"));
        assert!(FIREFOX_PROFILE_COMPONENTS.iter().all(|component| {
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

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn process_actions_fail_closed_off_windows() {
        assert!(list_native_apps()
            .iter()
            .all(|app| app.availability == NativeAppAvailability::Unavailable
                && !app.supports_overlay));
        assert!(launch_native_app(NativeAppId::Discord).is_err());
        assert!(install_native_app(NativeAppId::Discord).is_err());
        assert_eq!(
            get_firefox_status().availability,
            NativeAppAvailability::Unavailable
        );
        assert!(launch_firefox_service(
            std::path::Path::new("/trusted/app-local-data"),
            FirefoxServiceId::Instagram
        )
        .is_err());
        assert!(install_firefox().is_err());
    }
}
