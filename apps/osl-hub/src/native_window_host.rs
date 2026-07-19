//! Experimental Windows native-window hosting boundary.
//!
//! The public API accepts only [`NativeAppId`]. It never accepts an executable,
//! profile path, process id, window handle, URL, or command-line argument from
//! IPC. A native client is eligible only after its current first-party binary
//! has a verified secondary-instance switch that isolates all writable state in
//! an OSL-owned profile. Unsupported clients fail closed; callers must not fall
//! back to a web surface or the user's ordinary desktop-client session.
//!
//! Windows 10 1703 and later may reset a cross-process child's DPI awareness
//! during `SetParent`. This module deliberately does not reparent or subclass a
//! foreign process. The experimental mode instead makes the separately spawned
//! client a borderless tool window owned through an OSL-created owner HWND and
//! keeps it aligned with the trusted OSL content rectangle.

use crate::native_apps::NativeAppId;
use serde::Serialize;
use std::path::Path;
#[cfg(target_os = "windows")]
use std::sync::Mutex;

#[cfg(target_os = "windows")]
const TRUSTED_VERTICAL_RESERVE: i32 = 98;
#[cfg(any(target_os = "windows", test))]
const PROFILE_NAMESPACE: &str = "native-window-profiles-v1";

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NativeWindowHostStatus {
    Hosted,
    Resized,
    Focused,
    Detached,
    Unsupported,
    Failed,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NativeWindowHostReason {
    None,
    PlatformUnsupported,
    SecondaryInstanceUnverified,
    AppNotInstalled,
    ProfileUnavailable,
    LaunchFailed,
    WindowNotFound,
    WindowIdentityChanged,
    OwnerWindowUnavailable,
    HostWindowUnavailable,
    WindowOperationRejected,
    NotHosted,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeWindowHostResult {
    pub id: NativeAppId,
    pub status: NativeWindowHostStatus,
    pub reason: NativeWindowHostReason,
    /// This is a fixed enum-like label, never a path, PID, HWND, title, or
    /// process error. It is safe to expose to the bundled UI.
    pub mode: &'static str,
}

impl NativeWindowHostResult {
    fn unsupported(id: NativeAppId, reason: NativeWindowHostReason) -> Self {
        Self {
            id,
            status: NativeWindowHostStatus::Unsupported,
            reason,
            mode: "none",
        }
    }

    #[cfg(target_os = "windows")]
    fn failed(id: NativeAppId, reason: NativeWindowHostReason) -> Self {
        Self {
            id,
            status: NativeWindowHostStatus::Failed,
            reason,
            mode: "none",
        }
    }

    #[cfg(target_os = "windows")]
    fn success(id: NativeAppId, status: NativeWindowHostStatus) -> Self {
        Self {
            id,
            status,
            reason: NativeWindowHostReason::None,
            mode: "ownedBorderless",
        }
    }
}

#[derive(Debug, Default)]
pub struct NativeWindowHostState {
    #[cfg(target_os = "windows")]
    inner: Mutex<Option<HostedWindow>>,
}

#[cfg(target_os = "windows")]
impl Drop for NativeWindowHostState {
    fn drop(&mut self) {
        if let Ok(slot) = self.inner.get_mut() {
            if let Some(mut hosted) = slot.take() {
                unsafe { windows::restore_owned_window(&hosted) };
                let _ = hosted.child.kill();
                let _ = hosted.child.wait();
            }
        }
    }
}

// Handles are stored as integers so the state remains Send + Sync without
// claiming that foreign HWND pointer values may be dereferenced.
#[derive(Debug)]
#[cfg(target_os = "windows")]
struct HostedWindow {
    id: NativeAppId,
    process_id: u32,
    child: std::process::Child,
    window: isize,
    owner_window: isize,
    previous_owner: isize,
    previous_style: isize,
    previous_ex_style: isize,
    previous_rect: [i32; 4],
}

impl NativeWindowHostState {
    /// Launch a distinct, empty OSL-owned client profile and visually dock only
    /// the window created by that exact spawned process.
    pub fn host(
        &self,
        id: NativeAppId,
        osl_profile_root: &Path,
        owner_osl_user_id: &str,
        trusted_parent: isize,
    ) -> NativeWindowHostResult {
        #[cfg(target_os = "windows")]
        {
            windows::host(
                self,
                id,
                osl_profile_root,
                owner_osl_user_id,
                trusted_parent,
            )
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (osl_profile_root, owner_osl_user_id, trusted_parent);
            NativeWindowHostResult::unsupported(id, NativeWindowHostReason::PlatformUnsupported)
        }
    }

    pub fn resize(&self, trusted_parent: isize) -> NativeWindowHostResult {
        #[cfg(target_os = "windows")]
        {
            windows::resize(self, trusted_parent)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = trusted_parent;
            NativeWindowHostResult::unsupported(
                NativeAppId::Discord,
                NativeWindowHostReason::PlatformUnsupported,
            )
        }
    }

    pub fn focus(&self) -> NativeWindowHostResult {
        #[cfg(target_os = "windows")]
        {
            windows::focus(self)
        }
        #[cfg(not(target_os = "windows"))]
        {
            NativeWindowHostResult::unsupported(
                NativeAppId::Discord,
                NativeWindowHostReason::PlatformUnsupported,
            )
        }
    }

    /// Restore ownership, styles, and bounds for only the exact OSL-spawned
    /// process window. The user's ordinary app instance is never enumerated or
    /// closed by this operation.
    pub fn detach(&self) -> NativeWindowHostResult {
        #[cfg(target_os = "windows")]
        {
            windows::detach(self)
        }
        #[cfg(not(target_os = "windows"))]
        {
            NativeWindowHostResult::unsupported(
                NativeAppId::Discord,
                NativeWindowHostReason::PlatformUnsupported,
            )
        }
    }
}

#[cfg(any(target_os = "windows", test))]
fn profile_component(id: NativeAppId) -> &'static str {
    match id {
        NativeAppId::Discord => "discord",
        NativeAppId::Telegram => "telegram",
        NativeAppId::Signal => "signal",
        NativeAppId::Whatsapp => "whatsapp",
    }
}

#[cfg(any(target_os = "windows", test))]
fn profile_relative_components(
    owner_osl_user_id: &str,
    id: NativeAppId,
) -> Result<[String; 3], NativeWindowHostReason> {
    let owner_namespace = crate::service_host::owner_profile_namespace(owner_osl_user_id)
        .map_err(|_| NativeWindowHostReason::ProfileUnavailable)?;
    Ok([
        PROFILE_NAMESPACE.to_owned(),
        owner_namespace,
        profile_component(id).to_owned(),
    ])
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg(any(target_os = "windows", test))]
enum FixedSecondaryLaunch {
    DiscordUserDataDir,
    TelegramManyWorkdir,
    SignalUserDataDir,
    Unsupported,
}

#[cfg(any(target_os = "windows", test))]
fn fixed_secondary_launch(id: NativeAppId) -> FixedSecondaryLaunch {
    match id {
        NativeAppId::Discord => FixedSecondaryLaunch::DiscordUserDataDir,
        NativeAppId::Telegram => FixedSecondaryLaunch::TelegramManyWorkdir,
        NativeAppId::Signal => FixedSecondaryLaunch::SignalUserDataDir,
        NativeAppId::Whatsapp => FixedSecondaryLaunch::Unsupported,
    }
}

// Flip only one compile-time gate during a local Windows probe. It must return
// to false unless that exact first-party release proves that it starts a second
// process, writes exclusively below the supplied empty OSL profile, and never
// activates or mutates the user's ordinary instance.
#[cfg(any(target_os = "windows", test))]
const ENABLE_DISCORD_SECONDARY_HOST: bool = false;
#[cfg(any(target_os = "windows", test))]
const ENABLE_TELEGRAM_SECONDARY_HOST: bool = true;
#[cfg(any(target_os = "windows", test))]
const ENABLE_SIGNAL_SECONDARY_HOST: bool = true;

/// No entry is enabled merely because an app is Electron or happens to accept
/// a Chromium switch. Telegram and Signal are enabled only after a local probe
/// proved a second visible process and writes inside the supplied empty OSL
/// profile while the ordinary client remained live. Discord ignored its
/// isolated profile without an unsafe shared-profile multi-instance switch;
/// WhatsApp exposes no fixed secondary-profile switch, so both fail closed.
#[cfg(any(target_os = "windows", test))]
fn secondary_instance_verified(id: NativeAppId) -> bool {
    match fixed_secondary_launch(id) {
        FixedSecondaryLaunch::DiscordUserDataDir => ENABLE_DISCORD_SECONDARY_HOST,
        FixedSecondaryLaunch::TelegramManyWorkdir => ENABLE_TELEGRAM_SECONDARY_HOST,
        FixedSecondaryLaunch::SignalUserDataDir => ENABLE_SIGNAL_SECONDARY_HOST,
        FixedSecondaryLaunch::Unsupported => false,
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use std::ffi::OsString;
    use std::fs;
    use std::os::windows::ffi::OsStringExt;
    use std::os::windows::fs::MetadataExt;
    use std::process::{Child, Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};
    use windows_sys::Win32::Foundation::{
        GetLastError, SetLastError, BOOL, HWND, LPARAM, POINT, RECT,
    };
    use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    use windows_sys::Win32::UI::Shell::{
        FOLDERID_LocalAppData, FOLDERID_RoamingAppData, SHGetKnownFolderPath, KF_FLAG_DEFAULT,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, EnumWindows, GetClientRect, GetWindowLongPtrW,
        GetWindowRect, GetWindowThreadProcessId, IsWindowVisible, SetForegroundWindow,
        SetWindowLongPtrW, SetWindowPos, ShowWindow, GWLP_HWNDPARENT, GWL_EXSTYLE, GWL_STYLE,
        HWND_TOP, SWP_FRAMECHANGED, SWP_SHOWWINDOW, SW_RESTORE, WS_CAPTION, WS_EX_APPWINDOW,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU,
        WS_THICKFRAME,
    };

    const ERROR_SUCCESS: u32 = 0;
    const WINDOW_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(8);

    #[derive(Debug)]
    struct LaunchSpec {
        executable: std::path::PathBuf,
        arguments: Vec<OsString>,
        profile: std::path::PathBuf,
    }

    struct WindowSearch {
        expected_pid: u32,
        best: HWND,
        best_area: i64,
    }

    pub(super) fn host(
        state: &NativeWindowHostState,
        id: NativeAppId,
        root: &Path,
        owner_osl_user_id: &str,
        parent: isize,
    ) -> NativeWindowHostResult {
        if parent == 0 {
            return NativeWindowHostResult::failed(
                id,
                NativeWindowHostReason::OwnerWindowUnavailable,
            );
        }
        if !secondary_instance_verified(id) {
            return NativeWindowHostResult::unsupported(
                id,
                NativeWindowHostReason::SecondaryInstanceUnverified,
            );
        }

        let spec = match build_launch_spec(id, root, owner_osl_user_id) {
            Ok(spec) => spec,
            Err(reason) => return NativeWindowHostResult::failed(id, reason),
        };
        let mut guard = match state.inner.lock() {
            Ok(guard) => guard,
            Err(_) => {
                return NativeWindowHostResult::failed(
                    id,
                    NativeWindowHostReason::HostWindowUnavailable,
                )
            }
        };
        if guard.is_some() {
            return NativeWindowHostResult::failed(
                id,
                NativeWindowHostReason::HostWindowUnavailable,
            );
        }
        let mut child = match launch_isolated(&spec) {
            Ok(child) => child,
            Err(_) => {
                return NativeWindowHostResult::failed(id, NativeWindowHostReason::LaunchFailed)
            }
        };
        let pid = child.id();
        let window = match wait_for_process_window(&mut child) {
            Some(window) => window,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return NativeWindowHostResult::failed(id, NativeWindowHostReason::WindowNotFound);
            }
        };
        // Re-verify the exact spawned process after discovery and immediately
        // before changing ownership or style.
        if unsafe { window_process_id(window) } != Some(pid) {
            let _ = child.kill();
            let _ = child.wait();
            return NativeWindowHostResult::failed(
                id,
                NativeWindowHostReason::WindowIdentityChanged,
            );
        }
        let hosted =
            match unsafe { adopt_borderless_owned_window(id, child, window, parent as HWND) } {
                Ok(hosted) => hosted,
                Err(reason) => {
                    return NativeWindowHostResult::failed(id, reason);
                }
            };
        *guard = Some(hosted);
        NativeWindowHostResult::success(id, NativeWindowHostStatus::Hosted)
    }

    pub(super) fn resize(state: &NativeWindowHostState, parent: isize) -> NativeWindowHostResult {
        let mut guard = match state.inner.lock() {
            Ok(guard) => guard,
            Err(_) => {
                return NativeWindowHostResult::failed(
                    NativeAppId::Discord,
                    NativeWindowHostReason::HostWindowUnavailable,
                )
            }
        };
        let Some(hosted) = guard.as_mut() else {
            return NativeWindowHostResult::failed(
                NativeAppId::Discord,
                NativeWindowHostReason::NotHosted,
            );
        };
        if hosted.child.try_wait().ok().flatten().is_some()
            || parent == 0
            || unsafe { window_process_id(hosted.window as HWND) } != Some(hosted.process_id)
        {
            return NativeWindowHostResult::failed(
                hosted.id,
                NativeWindowHostReason::WindowIdentityChanged,
            );
        }
        if unsafe { align_to_parent(hosted.window as HWND, parent as HWND) } {
            NativeWindowHostResult::success(hosted.id, NativeWindowHostStatus::Resized)
        } else {
            NativeWindowHostResult::failed(
                hosted.id,
                NativeWindowHostReason::WindowOperationRejected,
            )
        }
    }

    pub(super) fn focus(state: &NativeWindowHostState) -> NativeWindowHostResult {
        let mut guard = match state.inner.lock() {
            Ok(guard) => guard,
            Err(_) => {
                return NativeWindowHostResult::failed(
                    NativeAppId::Discord,
                    NativeWindowHostReason::HostWindowUnavailable,
                )
            }
        };
        let Some(hosted) = guard.as_mut() else {
            return NativeWindowHostResult::failed(
                NativeAppId::Discord,
                NativeWindowHostReason::NotHosted,
            );
        };
        if hosted.child.try_wait().ok().flatten().is_some()
            || unsafe { window_process_id(hosted.window as HWND) } != Some(hosted.process_id)
        {
            return NativeWindowHostResult::failed(
                hosted.id,
                NativeWindowHostReason::WindowIdentityChanged,
            );
        }
        unsafe {
            ShowWindow(hosted.window as HWND, SW_RESTORE);
            let _ = SetForegroundWindow(hosted.window as HWND);
            let _ = SetFocus(hosted.window as HWND);
        }
        NativeWindowHostResult::success(hosted.id, NativeWindowHostStatus::Focused)
    }

    pub(super) fn detach(state: &NativeWindowHostState) -> NativeWindowHostResult {
        let mut guard = match state.inner.lock() {
            Ok(guard) => guard,
            Err(_) => {
                return NativeWindowHostResult::failed(
                    NativeAppId::Discord,
                    NativeWindowHostReason::HostWindowUnavailable,
                )
            }
        };
        let Some(mut hosted) = guard.take() else {
            return NativeWindowHostResult::failed(
                NativeAppId::Discord,
                NativeWindowHostReason::NotHosted,
            );
        };
        if unsafe { window_process_id(hosted.window as HWND) } != Some(hosted.process_id) {
            unsafe { destroy_owner(hosted.owner_window as HWND) };
            let _ = hosted.child.kill();
            let _ = hosted.child.wait();
            return NativeWindowHostResult::failed(
                hosted.id,
                NativeWindowHostReason::WindowIdentityChanged,
            );
        }
        unsafe { restore_window(&hosted) };
        let _ = hosted.child.kill();
        let _ = hosted.child.wait();
        NativeWindowHostResult::success(hosted.id, NativeWindowHostStatus::Detached)
    }

    fn build_launch_spec(
        id: NativeAppId,
        root: &Path,
        owner_osl_user_id: &str,
    ) -> Result<LaunchSpec, NativeWindowHostReason> {
        let profile = prepare_profile(root, owner_osl_user_id, id)?;
        let (executable, arguments) = match fixed_secondary_launch(id) {
            FixedSecondaryLaunch::DiscordUserDataDir => {
                let executable =
                    newest_discord_executable().ok_or(NativeWindowHostReason::AppNotInstalled)?;
                let mut profile_arg = OsString::from("--user-data-dir=");
                profile_arg.push(profile.as_os_str());
                (executable, vec![profile_arg])
            }
            FixedSecondaryLaunch::TelegramManyWorkdir => {
                let executable =
                    telegram_executable().ok_or(NativeWindowHostReason::AppNotInstalled)?;
                (
                    executable,
                    vec![
                        OsString::from("-many"),
                        OsString::from("-workdir"),
                        profile.as_os_str().to_owned(),
                    ],
                )
            }
            FixedSecondaryLaunch::SignalUserDataDir => {
                let executable =
                    signal_executable().ok_or(NativeWindowHostReason::AppNotInstalled)?;
                let mut profile_arg = OsString::from("--user-data-dir=");
                profile_arg.push(profile.as_os_str());
                (executable, vec![profile_arg])
            }
            FixedSecondaryLaunch::Unsupported => {
                return Err(NativeWindowHostReason::SecondaryInstanceUnverified)
            }
        };
        Ok(LaunchSpec {
            executable,
            arguments,
            profile,
        })
    }

    fn newest_discord_executable() -> Option<std::path::PathBuf> {
        let install_root = known_folder(&FOLDERID_LocalAppData)?.join("Discord");
        crate::native_apps::newest_discord_executable_under(&install_root)
    }

    fn telegram_executable() -> Option<std::path::PathBuf> {
        [
            known_folder(&FOLDERID_RoamingAppData)
                .map(|root| root.join("Telegram Desktop").join("Telegram.exe")),
            known_folder(&FOLDERID_LocalAppData).map(|root| {
                root.join("Programs")
                    .join("Telegram Desktop")
                    .join("Telegram.exe")
            }),
        ]
        .into_iter()
        .flatten()
        .find(|candidate| candidate.is_file())
    }

    fn signal_executable() -> Option<std::path::PathBuf> {
        known_folder(&FOLDERID_LocalAppData)
            .map(|root| {
                root.join("Programs")
                    .join("signal-desktop")
                    .join("Signal.exe")
            })
            .filter(|candidate| candidate.is_file())
    }

    fn known_folder(id: *const windows_sys::core::GUID) -> Option<std::path::PathBuf> {
        let mut raw = std::ptr::null_mut();
        let result = unsafe {
            SHGetKnownFolderPath(id, KF_FLAG_DEFAULT as u32, std::ptr::null_mut(), &mut raw)
        };
        if result < 0 || raw.is_null() {
            return None;
        }
        let mut length = 0usize;
        unsafe {
            while *raw.add(length) != 0 {
                length += 1;
            }
        }
        let value = unsafe { std::slice::from_raw_parts(raw, length) };
        let path = std::path::PathBuf::from(OsString::from_wide(value));
        unsafe { CoTaskMemFree(raw.cast()) };
        Some(path)
    }

    fn prepare_profile(
        root: &Path,
        owner_osl_user_id: &str,
        id: NativeAppId,
    ) -> Result<std::path::PathBuf, NativeWindowHostReason> {
        if !root.is_absolute() {
            return Err(NativeWindowHostReason::ProfileUnavailable);
        }
        fs::create_dir_all(root).map_err(|_| NativeWindowHostReason::ProfileUnavailable)?;
        let canonical_root = root
            .canonicalize()
            .map_err(|_| NativeWindowHostReason::ProfileUnavailable)?;
        let mut profile = canonical_root.clone();
        for component in profile_relative_components(owner_osl_user_id, id)? {
            profile.push(component);
            ensure_plain_profile_directory(&profile)?;
        }
        let canonical_profile = profile
            .canonicalize()
            .map_err(|_| NativeWindowHostReason::ProfileUnavailable)?;
        canonical_profile
            .starts_with(&canonical_root)
            .then_some(canonical_profile)
            .ok_or(NativeWindowHostReason::ProfileUnavailable)
    }

    fn ensure_plain_profile_directory(path: &Path) -> Result<(), NativeWindowHostReason> {
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

        let verify = |metadata: fs::Metadata| {
            (metadata.is_dir()
                && !metadata.file_type().is_symlink()
                && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0)
                .then_some(())
                .ok_or(NativeWindowHostReason::ProfileUnavailable)
        };

        match fs::symlink_metadata(path) {
            Ok(metadata) => verify(metadata),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(path).map_err(|_| NativeWindowHostReason::ProfileUnavailable)?;
                // Verify the created path again so a symlink or junction
                // substituted during creation never becomes a client profile.
                verify(
                    fs::symlink_metadata(path)
                        .map_err(|_| NativeWindowHostReason::ProfileUnavailable)?,
                )
            }
            Err(_) => Err(NativeWindowHostReason::ProfileUnavailable),
        }
    }

    fn launch_isolated(spec: &LaunchSpec) -> std::io::Result<Child> {
        debug_assert!(spec.profile.is_absolute());
        Command::new(&spec.executable)
            .args(&spec.arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    }

    fn wait_for_process_window(child: &mut Child) -> Option<HWND> {
        let pid = child.id();
        let deadline = Instant::now() + WINDOW_DISCOVERY_TIMEOUT;
        loop {
            if child.try_wait().ok().flatten().is_some() {
                return None;
            }
            if let Some(window) = find_process_window(pid) {
                return Some(window);
            }
            if Instant::now() >= deadline {
                return None;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    fn find_process_window(pid: u32) -> Option<HWND> {
        let mut search = WindowSearch {
            expected_pid: pid,
            best: std::ptr::null_mut(),
            best_area: 0,
        };
        unsafe {
            EnumWindows(
                Some(enum_window),
                (&mut search as *mut WindowSearch) as LPARAM,
            );
        }
        (!search.best.is_null()).then_some(search.best)
    }

    unsafe extern "system" fn enum_window(window: HWND, parameter: LPARAM) -> BOOL {
        let search = &mut *(parameter as *mut WindowSearch);
        if IsWindowVisible(window) == 0 || window_process_id(window) != Some(search.expected_pid) {
            return 1;
        }
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if GetWindowRect(window, &mut rect) == 0 {
            return 1;
        }
        let area =
            i64::from((rect.right - rect.left).max(0)) * i64::from((rect.bottom - rect.top).max(0));
        if area > search.best_area {
            search.best = window;
            search.best_area = area;
        }
        1
    }

    unsafe fn window_process_id(window: HWND) -> Option<u32> {
        if window.is_null() {
            return None;
        }
        let mut pid = 0;
        (GetWindowThreadProcessId(window, &mut pid) != 0 && pid != 0).then_some(pid)
    }

    unsafe fn adopt_borderless_owned_window(
        id: NativeAppId,
        mut child: Child,
        window: HWND,
        parent: HWND,
    ) -> Result<HostedWindow, NativeWindowHostReason> {
        let pid = child.id();
        let owner = create_owner_window(parent)?;
        let previous_owner = GetWindowLongPtrW(window, GWLP_HWNDPARENT);
        let previous_style = GetWindowLongPtrW(window, GWL_STYLE);
        let previous_ex_style = GetWindowLongPtrW(window, GWL_EXSTYLE);
        let mut previous_rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if GetWindowRect(window, &mut previous_rect) == 0 {
            let _ = child.kill();
            let _ = child.wait();
            destroy_owner(owner);
            return Err(NativeWindowHostReason::WindowOperationRejected);
        }
        SetLastError(ERROR_SUCCESS);
        SetWindowLongPtrW(window, GWLP_HWNDPARENT, owner as isize);
        if GetLastError() != ERROR_SUCCESS {
            let _ = child.kill();
            let _ = child.wait();
            destroy_owner(owner);
            return Err(NativeWindowHostReason::WindowOperationRejected);
        }
        let chrome =
            (WS_CAPTION | WS_THICKFRAME | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX) as isize;
        SetWindowLongPtrW(
            window,
            GWL_STYLE,
            (previous_style & !chrome) | WS_POPUP as isize,
        );
        SetWindowLongPtrW(
            window,
            GWL_EXSTYLE,
            (previous_ex_style & !(WS_EX_APPWINDOW as isize)) | WS_EX_TOOLWINDOW as isize,
        );
        if !align_to_parent(window, parent) {
            SetWindowLongPtrW(window, GWLP_HWNDPARENT, previous_owner);
            SetWindowLongPtrW(window, GWL_STYLE, previous_style);
            SetWindowLongPtrW(window, GWL_EXSTYLE, previous_ex_style);
            destroy_owner(owner);
            let _ = child.kill();
            let _ = child.wait();
            return Err(NativeWindowHostReason::WindowOperationRejected);
        }
        Ok(HostedWindow {
            id,
            process_id: pid,
            child,
            window: window as isize,
            owner_window: owner as isize,
            previous_owner,
            previous_style,
            previous_ex_style,
            previous_rect: [
                previous_rect.left,
                previous_rect.top,
                previous_rect.right,
                previous_rect.bottom,
            ],
        })
    }

    unsafe fn create_owner_window(parent: HWND) -> Result<HWND, NativeWindowHostReason> {
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let title: Vec<u16> = "OSL Native Window Owner\0".encode_utf16().collect();
        let owner = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            parent,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
        );
        (!owner.is_null())
            .then_some(owner)
            .ok_or(NativeWindowHostReason::HostWindowUnavailable)
    }

    unsafe fn align_to_parent(window: HWND, parent: HWND) -> bool {
        let mut client = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if GetClientRect(parent, &mut client) == 0 {
            return false;
        }
        let mut origin = POINT {
            x: 0,
            y: TRUSTED_VERTICAL_RESERVE,
        };
        if ClientToScreen(parent, &mut origin) == 0 {
            return false;
        }
        let width = (client.right - client.left).max(1);
        let height = (client.bottom - TRUSTED_VERTICAL_RESERVE).max(1);
        SetWindowPos(
            window,
            HWND_TOP,
            origin.x,
            origin.y,
            width,
            height,
            SWP_FRAMECHANGED | SWP_SHOWWINDOW,
        ) != 0
    }

    pub(super) unsafe fn restore_owned_window(hosted: &HostedWindow) {
        restore_window(hosted);
    }

    unsafe fn restore_window(hosted: &HostedWindow) {
        let window = hosted.window as HWND;
        if window_process_id(window) == Some(hosted.process_id) {
            SetWindowLongPtrW(window, GWLP_HWNDPARENT, hosted.previous_owner);
            SetWindowLongPtrW(window, GWL_STYLE, hosted.previous_style);
            SetWindowLongPtrW(window, GWL_EXSTYLE, hosted.previous_ex_style);
            let [left, top, right, bottom] = hosted.previous_rect;
            let _ = SetWindowPos(
                window,
                HWND_TOP,
                left,
                top,
                (right - left).max(1),
                (bottom - top).max(1),
                SWP_FRAMECHANGED | SWP_SHOWWINDOW,
            );
        }
        destroy_owner(hosted.owner_window as HWND);
    }

    unsafe fn destroy_owner(owner: HWND) {
        if !owner.is_null() {
            let _ = DestroyWindow(owner);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_and_profile_names_are_fixed_and_path_free() {
        let ids = [
            NativeAppId::Discord,
            NativeAppId::Telegram,
            NativeAppId::Signal,
            NativeAppId::Whatsapp,
        ];
        for id in ids {
            let components = profile_relative_components("owner-a", id).unwrap();
            assert_eq!(components[0], PROFILE_NAMESPACE);
            assert!(components[1].starts_with("owner-"));
            assert!(!components[2].is_empty());
            assert!(components.iter().all(|component| {
                !component.contains(['/', '\\'])
                    && component.as_str() != "."
                    && component.as_str() != ".."
            }));
        }
    }

    #[test]
    fn native_profiles_are_namespaced_by_osl_owner() {
        let owner_a = profile_relative_components("owner-a", NativeAppId::Telegram).unwrap();
        let owner_b = profile_relative_components("owner-b", NativeAppId::Telegram).unwrap();
        let path_a: std::path::PathBuf = owner_a.iter().collect();
        let path_b: std::path::PathBuf = owner_b.iter().collect();

        assert_eq!(owner_a[0], owner_b[0]);
        assert_ne!(owner_a[1], owner_b[1]);
        assert_eq!(owner_a[2], owner_b[2]);
        assert_ne!(path_a, path_b);
    }

    #[test]
    fn invalid_native_profile_owner_fails_closed() {
        assert_eq!(
            profile_relative_components("", NativeAppId::Telegram),
            Err(NativeWindowHostReason::ProfileUnavailable)
        );
        assert_eq!(
            profile_relative_components(&"x".repeat(129), NativeAppId::Telegram),
            Err(NativeWindowHostReason::ProfileUnavailable)
        );
    }

    #[test]
    fn only_locally_verified_secondary_instance_modes_are_enabled() {
        assert!(!secondary_instance_verified(NativeAppId::Discord));
        assert!(secondary_instance_verified(NativeAppId::Telegram));
        assert!(secondary_instance_verified(NativeAppId::Signal));
        assert!(!secondary_instance_verified(NativeAppId::Whatsapp));
    }

    #[test]
    fn probe_specs_are_fixed_per_allowlisted_app() {
        assert_eq!(
            fixed_secondary_launch(NativeAppId::Discord),
            FixedSecondaryLaunch::DiscordUserDataDir
        );
        assert_eq!(
            fixed_secondary_launch(NativeAppId::Telegram),
            FixedSecondaryLaunch::TelegramManyWorkdir
        );
        assert_eq!(
            fixed_secondary_launch(NativeAppId::Signal),
            FixedSecondaryLaunch::SignalUserDataDir
        );
        assert_eq!(
            fixed_secondary_launch(NativeAppId::Whatsapp),
            FixedSecondaryLaunch::Unsupported
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn off_windows_host_actions_are_explicitly_unsupported() {
        let state = NativeWindowHostState::default();
        for result in [
            state.host(
                NativeAppId::Discord,
                Path::new("/trusted/osl-data"),
                "owner-a",
                1,
            ),
            state.resize(1),
            state.focus(),
            state.detach(),
        ] {
            assert_eq!(result.status, NativeWindowHostStatus::Unsupported);
            assert_eq!(result.reason, NativeWindowHostReason::PlatformUnsupported);
            assert_eq!(result.mode, "none");
        }
    }
}
