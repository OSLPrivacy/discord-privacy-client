//! Optional browser companion.
//!
//! This boundary is intentionally weaker than OSL's isolated service host. It
//! accepts only fixed service/browser/account-mode enums, verifies the selected
//! executable against OSL's fixed browser manifest, and opens a new app-style
//! window. Existing-browser mode uses the browser's normal profile. Isolated
//! mode uses an owner-scoped OSL-created profile path that is never supplied by
//! the renderer. It never claims screenshot protection or shortcut containment
//! and never installs policy, hooks, or extensions.

use crate::native_apps::{BrowserImportId, FirefoxServiceId};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BrowserAccountMode {
    ExistingBrowser,
    IsolatedOsl,
}

impl BrowserAccountMode {
    fn action_mode(self) -> &'static str {
        match self {
            Self::ExistingBrowser => "existingBrowserCompanion",
            Self::IsolatedOsl => "isolatedBrowserCompanion",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BrowserCompanionStatusKind {
    Available,
    Unsupported,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BrowserCompanionActionKind {
    Hosted,
    Resized,
    Focused,
    Detached,
    Unsupported,
    Failed,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BrowserCompanionReason {
    None,
    PlatformUnsupported,
    DefaultBrowserUnsupported,
    DefaultBrowserUntrusted,
    SelectedBrowserUnavailable,
    NativeAppRequired,
    IsolatedProfileUnsupported,
    ProfileUnavailable,
    LaunchFailed,
    WindowNotFound,
    WindowAmbiguous,
    WindowIdentityChanged,
    OwnerWindowUnavailable,
    WindowOperationRejected,
    NotHosted,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserCompanionStatus {
    pub status: BrowserCompanionStatusKind,
    pub browser_id: Option<BrowserImportId>,
    pub display_name: Option<&'static str>,
    pub reason: BrowserCompanionReason,
    pub capture_protected: bool,
    pub containment: &'static str,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserCompanionAction {
    pub status: BrowserCompanionActionKind,
    pub browser_id: Option<BrowserImportId>,
    pub reason: BrowserCompanionReason,
    pub mode: &'static str,
    pub capture_protected: bool,
    pub containment: &'static str,
}

impl BrowserCompanionAction {
    fn failed(reason: BrowserCompanionReason) -> Self {
        Self {
            status: BrowserCompanionActionKind::Failed,
            browser_id: None,
            reason,
            mode: "none",
            capture_protected: false,
            containment: "bestEffort",
        }
    }

    fn success(
        status: BrowserCompanionActionKind,
        browser_id: BrowserImportId,
        account_mode: BrowserAccountMode,
    ) -> Self {
        Self {
            status,
            browser_id: Some(browser_id),
            reason: BrowserCompanionReason::None,
            mode: account_mode.action_mode(),
            capture_protected: false,
            containment: "bestEffort",
        }
    }
}

#[cfg(any(target_os = "windows", test))]
fn window_candidate_is_eligible(
    excluded: bool,
    has_parent: bool,
    require_presentable: bool,
    visible: bool,
    iconic: bool,
    usable_rect: bool,
) -> bool {
    !excluded && !has_parent && (!require_presentable || (visible && !iconic && usable_rect))
}

fn isolated_browser_profile_supported(browser_id: BrowserImportId) -> bool {
    browser_id != BrowserImportId::DuckDuckGo
}

#[derive(Default)]
pub struct BrowserCompanionState {
    #[cfg(target_os = "windows")]
    inner: std::sync::Mutex<Option<windows::HostedBrowserWindow>>,
}

impl BrowserCompanionState {
    pub fn status(&self) -> BrowserCompanionStatus {
        #[cfg(target_os = "windows")]
        {
            windows::status()
        }
        #[cfg(not(target_os = "windows"))]
        {
            BrowserCompanionStatus {
                status: BrowserCompanionStatusKind::Unsupported,
                browser_id: None,
                display_name: None,
                reason: BrowserCompanionReason::PlatformUnsupported,
                capture_protected: false,
                containment: "bestEffort",
            }
        }
    }

    pub fn host(
        &self,
        service_id: FirefoxServiceId,
        browser_id: Option<BrowserImportId>,
        account_mode: BrowserAccountMode,
        app_local_data_dir: &Path,
        owner_osl_user_id: &str,
        trusted_parent: isize,
    ) -> BrowserCompanionAction {
        if service_id == FirefoxServiceId::Outlook {
            return BrowserCompanionAction::failed(BrowserCompanionReason::NativeAppRequired);
        }
        #[cfg(target_os = "windows")]
        {
            windows::host(
                self,
                service_id,
                browser_id,
                account_mode,
                app_local_data_dir,
                owner_osl_user_id,
                trusted_parent,
            )
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (
                service_id,
                browser_id,
                account_mode,
                app_local_data_dir,
                owner_osl_user_id,
                trusted_parent,
            );
            BrowserCompanionAction::failed(BrowserCompanionReason::PlatformUnsupported)
        }
    }

    pub fn resize(&self, trusted_parent: isize) -> BrowserCompanionAction {
        #[cfg(target_os = "windows")]
        {
            windows::resize(self, trusted_parent)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = trusted_parent;
            BrowserCompanionAction::failed(BrowserCompanionReason::PlatformUnsupported)
        }
    }

    pub fn focus(&self) -> BrowserCompanionAction {
        #[cfg(target_os = "windows")]
        {
            windows::focus(self)
        }
        #[cfg(not(target_os = "windows"))]
        {
            BrowserCompanionAction::failed(BrowserCompanionReason::PlatformUnsupported)
        }
    }

    pub fn detach(&self) -> BrowserCompanionAction {
        #[cfg(target_os = "windows")]
        {
            windows::detach(self)
        }
        #[cfg(not(target_os = "windows"))]
        {
            BrowserCompanionAction::failed(BrowserCompanionReason::PlatformUnsupported)
        }
    }

    pub fn terminate(&self) -> BrowserCompanionAction {
        #[cfg(target_os = "windows")]
        {
            windows::terminate(self)
        }
        #[cfg(not(target_os = "windows"))]
        {
            BrowserCompanionAction::failed(BrowserCompanionReason::PlatformUnsupported)
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for BrowserCompanionState {
    fn drop(&mut self) {
        if let Ok(slot) = self.inner.get_mut() {
            if let Some(hosted) = slot.take() {
                unsafe { windows::restore_and_close(hosted) };
            }
        }
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use std::collections::HashSet;
    use std::ffi::{c_void, OsString};
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};
    use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, POINT, RECT};
    use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, EnumWindows, GetClientRect, GetParent, GetWindowLongPtrW,
        GetWindowPlacement, GetWindowRect, GetWindowThreadProcessId, IsIconic, IsWindow,
        IsWindowVisible, PostMessageW, SetForegroundWindow, SetWindowLongPtrW, SetWindowPlacement,
        SetWindowPos, ShowWindow, GWL_EXSTYLE, GWL_STYLE, HWND_TOP, SWP_FRAMECHANGED,
        SWP_NOACTIVATE, SWP_NOZORDER, SWP_SHOWWINDOW, SW_HIDE, SW_RESTORE, WINDOWPLACEMENT,
        WM_CLOSE, WS_CAPTION, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW, WS_MAXIMIZEBOX, WS_MINIMIZEBOX,
        WS_POPUP, WS_SYSMENU, WS_THICKFRAME,
    };

    const ASSOCSTR_EXECUTABLE: u32 = 2;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const TRUSTED_VERTICAL_RESERVE: i32 = 48;
    const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(12);
    const STABLE_SAMPLES: usize = 3;

    #[link(name = "shlwapi")]
    unsafe extern "system" {
        fn AssocQueryStringW(
            flags: u32,
            string: u32,
            association: *const u16,
            extra: *const u16,
            output: *mut u16,
            output_length: *mut u32,
        ) -> i32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: BOOL, process_id: u32) -> *mut c_void;
        fn CloseHandle(handle: *mut c_void) -> BOOL;
        fn QueryFullProcessImageNameW(
            process: *mut c_void,
            flags: u32,
            path: *mut u16,
            length: *mut u32,
        ) -> BOOL;
        fn ProcessIdToSessionId(process_id: u32, session_id: *mut u32) -> BOOL;
    }

    pub(super) struct HostedBrowserWindow {
        browser_id: BrowserImportId,
        service_id: FirefoxServiceId,
        account_mode: BrowserAccountMode,
        window: isize,
        process_id: u32,
        executable: PathBuf,
        trusted_parent: isize,
        previous_style: isize,
        previous_ex_style: isize,
        previous_placement: WINDOWPLACEMENT,
        attached: bool,
    }

    struct WindowSearch<'a> {
        executable: &'a Path,
        excluded: &'a HashSet<isize>,
        require_presentable: bool,
        windows: Vec<(isize, u32)>,
    }

    pub(super) fn status() -> BrowserCompanionStatus {
        match default_browser() {
            Ok((browser_id, _)) => BrowserCompanionStatus {
                status: BrowserCompanionStatusKind::Available,
                browser_id: Some(browser_id),
                display_name: Some(crate::native_apps::browser_display_name(browser_id)),
                reason: BrowserCompanionReason::None,
                capture_protected: false,
                containment: "bestEffort",
            },
            Err(reason) => BrowserCompanionStatus {
                status: BrowserCompanionStatusKind::Unsupported,
                browser_id: None,
                display_name: None,
                reason,
                capture_protected: false,
                containment: "bestEffort",
            },
        }
    }

    pub(super) fn host(
        state: &BrowserCompanionState,
        service_id: FirefoxServiceId,
        requested_browser_id: Option<BrowserImportId>,
        account_mode: BrowserAccountMode,
        app_local_data_dir: &Path,
        owner_osl_user_id: &str,
        trusted_parent: isize,
    ) -> BrowserCompanionAction {
        if trusted_parent == 0 {
            return BrowserCompanionAction::failed(BrowserCompanionReason::OwnerWindowUnavailable);
        }
        // Resolve and re-verify the requested executable before considering a
        // retained window. A null choice means the current trusted Windows
        // default, not whichever browser happened to be hosted previously.
        let (browser_id, executable) = match requested_browser_id {
            Some(browser_id) => match crate::native_apps::trusted_browser_executable(browser_id) {
                Some(executable) => (browser_id, executable),
                None => {
                    return BrowserCompanionAction::failed(
                        BrowserCompanionReason::SelectedBrowserUnavailable,
                    )
                }
            },
            None => match default_browser() {
                Ok(value) => value,
                Err(reason) => return BrowserCompanionAction::failed(reason),
            },
        };
        if account_mode == BrowserAccountMode::IsolatedOsl
            && !isolated_browser_profile_supported(browser_id)
        {
            return BrowserCompanionAction::failed(
                BrowserCompanionReason::IsolatedProfileUnsupported,
            );
        }
        let mut guard = match state.inner.lock() {
            Ok(guard) => guard,
            Err(_) => {
                return BrowserCompanionAction::failed(
                    BrowserCompanionReason::WindowOperationRejected,
                )
            }
        };
        if let Some(hosted) = guard.as_mut() {
            if hosted.service_id == service_id
                && hosted.account_mode == account_mode
                && browser_id == hosted.browser_id
                && hosted_window_is_valid(hosted)
            {
                hosted.trusted_parent = trusted_parent;
                if unsafe { present(hosted) } {
                    return BrowserCompanionAction::success(
                        BrowserCompanionActionKind::Hosted,
                        hosted.browser_id,
                        hosted.account_mode,
                    );
                }
            }
            if let Some(stale) = guard.take() {
                unsafe { restore_and_close(stale) };
            }
        }

        let executable_path = executable.path().to_path_buf();
        // Snapshot every pre-existing top-level window for this executable,
        // including hidden and minimized ones. A normal-profile browser may
        // restore one of those windows while processing the launch request;
        // it must never become eligible for adoption merely because its
        // visibility changed after this snapshot.
        let before = enumerate_windows_for_path(&executable_path, &HashSet::new(), false)
            .into_iter()
            .map(|(window, _)| window)
            .collect::<HashSet<_>>();
        let url = crate::native_apps::firefox_service_url(service_id);
        let mut command = Command::new(executable.path());
        match account_mode {
            BrowserAccountMode::ExistingBrowser => {
                if crate::native_apps::browser_uses_chromium_app_mode(browser_id) {
                    command.arg("--new-window").arg(format!("--app={url}"));
                } else {
                    command.arg("--new-window").arg(url);
                }
            }
            BrowserAccountMode::IsolatedOsl => {
                let profile = match prepare_isolated_profile(
                    app_local_data_dir,
                    owner_osl_user_id,
                    browser_id,
                ) {
                    Ok(profile) => profile,
                    Err(_) => {
                        return BrowserCompanionAction::failed(
                            BrowserCompanionReason::ProfileUnavailable,
                        )
                    }
                };
                if crate::native_apps::browser_uses_chromium_app_mode(browser_id) {
                    let mut profile_argument = OsString::from("--user-data-dir=");
                    profile_argument.push(profile.as_os_str());
                    command
                        .arg(profile_argument)
                        .arg("--new-window")
                        .arg(format!("--app={url}"));
                } else if browser_id == BrowserImportId::Firefox {
                    command
                        .arg("-no-remote")
                        .arg("-profile")
                        .arg(profile)
                        .arg("--new-window")
                        .arg(url);
                } else {
                    return BrowserCompanionAction::failed(
                        BrowserCompanionReason::IsolatedProfileUnsupported,
                    );
                }
            }
        }
        let launched = command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
        if launched.is_err() {
            return BrowserCompanionAction::failed(BrowserCompanionReason::LaunchFailed);
        }
        let (window, process_id) = match discover_new_window(&executable_path, &before) {
            Ok(candidate) => candidate,
            Err(reason) => return BrowserCompanionAction::failed(reason),
        };
        let mut placement: WINDOWPLACEMENT = unsafe { std::mem::zeroed() };
        placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
        if unsafe { GetWindowPlacement(window as HWND, &mut placement) } == 0 {
            return BrowserCompanionAction::failed(BrowserCompanionReason::WindowOperationRejected);
        }
        let mut hosted = HostedBrowserWindow {
            browser_id,
            service_id,
            account_mode,
            window,
            process_id,
            executable: executable_path,
            trusted_parent,
            previous_style: unsafe { GetWindowLongPtrW(window as HWND, GWL_STYLE) },
            previous_ex_style: unsafe { GetWindowLongPtrW(window as HWND, GWL_EXSTYLE) },
            previous_placement: placement,
            attached: false,
        };
        if !unsafe { present(&mut hosted) } {
            unsafe { restore_and_close(hosted) };
            return BrowserCompanionAction::failed(BrowserCompanionReason::WindowOperationRejected);
        }
        *guard = Some(hosted);
        BrowserCompanionAction::success(
            BrowserCompanionActionKind::Hosted,
            browser_id,
            account_mode,
        )
    }

    pub(super) fn resize(
        state: &BrowserCompanionState,
        trusted_parent: isize,
    ) -> BrowserCompanionAction {
        let mut guard = match state.inner.lock() {
            Ok(guard) => guard,
            Err(_) => {
                return BrowserCompanionAction::failed(
                    BrowserCompanionReason::WindowOperationRejected,
                )
            }
        };
        let Some(hosted) = guard.as_mut() else {
            return BrowserCompanionAction::failed(BrowserCompanionReason::NotHosted);
        };
        hosted.trusted_parent = trusted_parent;
        if !hosted_window_is_valid(hosted) || !unsafe { present(hosted) } {
            return BrowserCompanionAction::failed(BrowserCompanionReason::WindowIdentityChanged);
        }
        BrowserCompanionAction::success(
            BrowserCompanionActionKind::Resized,
            hosted.browser_id,
            hosted.account_mode,
        )
    }

    pub(super) fn focus(state: &BrowserCompanionState) -> BrowserCompanionAction {
        let mut guard = match state.inner.lock() {
            Ok(guard) => guard,
            Err(_) => {
                return BrowserCompanionAction::failed(
                    BrowserCompanionReason::WindowOperationRejected,
                )
            }
        };
        let Some(hosted) = guard.as_mut() else {
            return BrowserCompanionAction::failed(BrowserCompanionReason::NotHosted);
        };
        if !hosted_window_is_valid(hosted) || !unsafe { present(hosted) } {
            return BrowserCompanionAction::failed(BrowserCompanionReason::WindowIdentityChanged);
        }
        BrowserCompanionAction::success(
            BrowserCompanionActionKind::Focused,
            hosted.browser_id,
            hosted.account_mode,
        )
    }

    pub(super) fn detach(state: &BrowserCompanionState) -> BrowserCompanionAction {
        let mut guard = match state.inner.lock() {
            Ok(guard) => guard,
            Err(_) => {
                return BrowserCompanionAction::failed(
                    BrowserCompanionReason::WindowOperationRejected,
                )
            }
        };
        let Some(hosted) = guard.as_mut() else {
            return BrowserCompanionAction::failed(BrowserCompanionReason::NotHosted);
        };
        if !hosted_window_is_valid(hosted) {
            return BrowserCompanionAction::failed(BrowserCompanionReason::WindowIdentityChanged);
        }
        unsafe {
            restore(hosted);
            ShowWindow(hosted.window as HWND, SW_HIDE);
        }
        hosted.attached = false;
        BrowserCompanionAction::success(
            BrowserCompanionActionKind::Detached,
            hosted.browser_id,
            hosted.account_mode,
        )
    }

    pub(super) fn terminate(state: &BrowserCompanionState) -> BrowserCompanionAction {
        let hosted = match state.inner.lock() {
            Ok(mut guard) => guard.take(),
            Err(_) => {
                return BrowserCompanionAction::failed(
                    BrowserCompanionReason::WindowOperationRejected,
                )
            }
        };
        let Some(hosted) = hosted else {
            return BrowserCompanionAction::failed(BrowserCompanionReason::NotHosted);
        };
        let browser_id = hosted.browser_id;
        let account_mode = hosted.account_mode;
        unsafe { restore_and_close(hosted) };
        BrowserCompanionAction::success(
            BrowserCompanionActionKind::Detached,
            browser_id,
            account_mode,
        )
    }

    fn prepare_isolated_profile(
        app_local_data_dir: &Path,
        owner_osl_user_id: &str,
        browser_id: BrowserImportId,
    ) -> Result<PathBuf, ()> {
        use std::fs;

        let owner =
            crate::service_host::owner_profile_namespace(owner_osl_user_id).map_err(|_| ())?;
        let browser = match browser_id {
            BrowserImportId::Chrome => "chrome",
            BrowserImportId::Edge => "edge",
            BrowserImportId::Firefox => "firefox",
            BrowserImportId::Brave => "brave",
            BrowserImportId::Opera => "opera",
            BrowserImportId::DuckDuckGo => return Err(()),
        };
        let root = app_local_data_dir.join("browser-companion-profiles-v1");
        fs::create_dir_all(&root).map_err(|_| ())?;
        let root_metadata = fs::symlink_metadata(&root).map_err(|_| ())?;
        if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
            return Err(());
        }
        let canonical_root = fs::canonicalize(&root).map_err(|_| ())?;
        let owner_dir = canonical_root.join(owner);
        fs::create_dir_all(&owner_dir).map_err(|_| ())?;
        let owner_metadata = fs::symlink_metadata(&owner_dir).map_err(|_| ())?;
        if !owner_metadata.is_dir() || owner_metadata.file_type().is_symlink() {
            return Err(());
        }
        let canonical_owner = fs::canonicalize(&owner_dir).map_err(|_| ())?;
        if !canonical_owner.starts_with(&canonical_root) {
            return Err(());
        }
        let profile = canonical_owner.join(browser);
        fs::create_dir_all(&profile).map_err(|_| ())?;
        let profile_metadata = fs::symlink_metadata(&profile).map_err(|_| ())?;
        if !profile_metadata.is_dir() || profile_metadata.file_type().is_symlink() {
            return Err(());
        }
        let canonical_profile = fs::canonicalize(&profile).map_err(|_| ())?;
        canonical_profile
            .starts_with(&canonical_owner)
            .then_some(canonical_profile)
            .ok_or(())
    }

    fn default_browser() -> Result<
        (
            BrowserImportId,
            crate::windows_executable_trust::TrustedExecutable,
        ),
        BrowserCompanionReason,
    > {
        let path =
            default_browser_path().ok_or(BrowserCompanionReason::DefaultBrowserUnsupported)?;
        crate::native_apps::trusted_browser_executable_at(&path)
            .ok_or(BrowserCompanionReason::DefaultBrowserUntrusted)
    }

    fn default_browser_path() -> Option<PathBuf> {
        let association = OsString::from("https")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let mut length = 0u32;
        unsafe {
            let _ = AssocQueryStringW(
                0,
                ASSOCSTR_EXECUTABLE,
                association.as_ptr(),
                std::ptr::null(),
                std::ptr::null_mut(),
                &mut length,
            );
        }
        if length == 0 || length > 32_768 {
            return None;
        }
        let mut buffer = vec![0u16; length as usize];
        if unsafe {
            AssocQueryStringW(
                0,
                ASSOCSTR_EXECUTABLE,
                association.as_ptr(),
                std::ptr::null(),
                buffer.as_mut_ptr(),
                &mut length,
            )
        } != 0
            || length == 0
        {
            return None;
        }
        let value = OsString::from_wide(&buffer[..length.saturating_sub(1) as usize]);
        PathBuf::from(value).canonicalize().ok()
    }

    fn discover_new_window(
        executable: &Path,
        before: &HashSet<isize>,
    ) -> Result<(isize, u32), BrowserCompanionReason> {
        let deadline = Instant::now() + DISCOVERY_TIMEOUT;
        let mut stable = None;
        let mut samples = 0usize;
        loop {
            let candidates = enumerate_windows_for_path(executable, before, true);
            match candidates.as_slice() {
                [candidate] => {
                    if stable == Some(*candidate) {
                        samples += 1;
                    } else {
                        stable = Some(*candidate);
                        samples = 1;
                    }
                    if samples >= STABLE_SAMPLES {
                        return Ok(*candidate);
                    }
                }
                [] => {
                    stable = None;
                    samples = 0;
                }
                _ => return Err(BrowserCompanionReason::WindowAmbiguous),
            }
            if Instant::now() >= deadline {
                return Err(BrowserCompanionReason::WindowNotFound);
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    fn enumerate_windows_for_path(
        executable: &Path,
        excluded: &HashSet<isize>,
        require_presentable: bool,
    ) -> Vec<(isize, u32)> {
        let mut search = WindowSearch {
            executable,
            excluded,
            require_presentable,
            windows: Vec::new(),
        };
        unsafe {
            EnumWindows(
                Some(enum_window),
                (&mut search as *mut WindowSearch<'_>) as LPARAM,
            );
        }
        search.windows
    }

    unsafe extern "system" fn enum_window(window: HWND, parameter: LPARAM) -> BOOL {
        let search = &mut *(parameter as *mut WindowSearch<'_>);
        let excluded = search.excluded.contains(&(window as isize));
        let has_parent = !GetParent(window).is_null();
        let visible = IsWindowVisible(window) != 0;
        let iconic = IsIconic(window) != 0;
        let usable_rect = if search.require_presentable {
            let mut rect: RECT = std::mem::zeroed();
            GetWindowRect(window, &mut rect) != 0
                && rect.right > rect.left
                && rect.bottom > rect.top
        } else {
            false
        };
        if !window_candidate_is_eligible(
            excluded,
            has_parent,
            search.require_presentable,
            visible,
            iconic,
            usable_rect,
        ) {
            return 1;
        }
        let mut process_id = 0u32;
        GetWindowThreadProcessId(window, &mut process_id);
        if process_path(process_id).as_deref() == Some(search.executable) {
            search.windows.push((window as isize, process_id));
        }
        1
    }

    fn process_path(process_id: u32) -> Option<PathBuf> {
        if process_id == 0 {
            return None;
        }
        let mut osl_session = 0u32;
        let mut process_session = 0u32;
        if unsafe {
            ProcessIdToSessionId(std::process::id(), &mut osl_session) == 0
                || ProcessIdToSessionId(process_id, &mut process_session) == 0
        } || osl_session != process_session
        {
            return None;
        }
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
        if process.is_null() {
            return None;
        }
        let result = (|| {
            let mut buffer = vec![0u16; 32_768];
            let mut length = buffer.len() as u32;
            if unsafe { QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut length) }
                == 0
                || length == 0
            {
                return None;
            }
            PathBuf::from(OsString::from_wide(&buffer[..length as usize]))
                .canonicalize()
                .ok()
        })();
        unsafe {
            CloseHandle(process);
        }
        result
    }

    fn hosted_window_is_valid(hosted: &HostedBrowserWindow) -> bool {
        if unsafe { IsWindow(hosted.window as HWND) } == 0 {
            return false;
        }
        let mut process_id = 0u32;
        unsafe {
            GetWindowThreadProcessId(hosted.window as HWND, &mut process_id);
        }
        process_id == hosted.process_id
            && process_path(process_id).as_deref() == Some(hosted.executable.as_path())
            && crate::native_apps::trusted_browser_executable_at(&hosted.executable)
                .is_some_and(|(id, _)| id == hosted.browser_id)
    }

    unsafe fn target_rect(parent: HWND) -> Option<RECT> {
        if parent.is_null() || IsWindow(parent) == 0 {
            return None;
        }
        let mut client: RECT = std::mem::zeroed();
        if GetClientRect(parent, &mut client) == 0
            || client.right <= client.left
            || client.bottom - client.top <= TRUSTED_VERTICAL_RESERVE
        {
            return None;
        }
        let mut origin = POINT {
            x: 0,
            y: TRUSTED_VERTICAL_RESERVE,
        };
        if ClientToScreen(parent, &mut origin) == 0 {
            return None;
        }
        Some(RECT {
            left: origin.x,
            top: origin.y,
            right: origin.x + client.right - client.left,
            bottom: origin.y + client.bottom - TRUSTED_VERTICAL_RESERVE,
        })
    }

    unsafe fn present(hosted: &mut HostedBrowserWindow) -> bool {
        if !hosted_window_is_valid(hosted) {
            return false;
        }
        let Some(target) = target_rect(hosted.trusted_parent as HWND) else {
            return false;
        };
        let chrome =
            (WS_CAPTION | WS_THICKFRAME | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX) as isize;
        SetWindowLongPtrW(
            hosted.window as HWND,
            GWL_STYLE,
            (hosted.previous_style & !chrome) | WS_POPUP as isize,
        );
        SetWindowLongPtrW(
            hosted.window as HWND,
            GWL_EXSTYLE,
            (hosted.previous_ex_style | WS_EX_APPWINDOW as isize) & !(WS_EX_TOOLWINDOW as isize),
        );
        ShowWindow(hosted.window as HWND, SW_RESTORE);
        if SetWindowPos(
            hosted.window as HWND,
            HWND_TOP,
            target.left,
            target.top,
            target.right - target.left,
            target.bottom - target.top,
            SWP_FRAMECHANGED | SWP_SHOWWINDOW,
        ) == 0
        {
            return false;
        }
        hosted.attached = true;
        let _ = BringWindowToTop(hosted.window as HWND);
        let _ = SetForegroundWindow(hosted.window as HWND);
        true
    }

    unsafe fn restore(hosted: &HostedBrowserWindow) {
        if !hosted_window_is_valid(hosted) {
            return;
        }
        SetWindowLongPtrW(hosted.window as HWND, GWL_STYLE, hosted.previous_style);
        SetWindowLongPtrW(hosted.window as HWND, GWL_EXSTYLE, hosted.previous_ex_style);
        let _ = SetWindowPlacement(hosted.window as HWND, &hosted.previous_placement);
        let _ = SetWindowPos(
            hosted.window as HWND,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOACTIVATE | SWP_NOZORDER,
        );
    }

    pub(super) unsafe fn restore_and_close(hosted: HostedBrowserWindow) {
        if hosted_window_is_valid(&hosted) {
            restore(&hosted);
            // Close only the exact app-style window created by OSL. Browser
            // processes and every pre-existing browser window remain intact.
            let _ = PostMessageW(hosted.window as HWND, WM_CLOSE, 0, 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_result_is_truthful_about_capture_and_containment() {
        let success = BrowserCompanionAction::success(
            BrowserCompanionActionKind::Hosted,
            BrowserImportId::Chrome,
            BrowserAccountMode::ExistingBrowser,
        );
        assert!(!success.capture_protected);
        assert_eq!(success.containment, "bestEffort");
        assert_eq!(success.mode, "existingBrowserCompanion");
        let isolated = BrowserCompanionAction::success(
            BrowserCompanionActionKind::Hosted,
            BrowserImportId::Firefox,
            BrowserAccountMode::IsolatedOsl,
        );
        assert_eq!(isolated.mode, "isolatedBrowserCompanion");
        let failure = BrowserCompanionAction::failed(BrowserCompanionReason::LaunchFailed);
        assert!(!failure.capture_protected);
        assert_eq!(failure.containment, "bestEffort");
    }

    #[test]
    fn account_modes_and_isolated_browser_gate_are_bounded() {
        assert_eq!(
            serde_json::from_str::<BrowserAccountMode>(r#""existingBrowser""#).unwrap(),
            BrowserAccountMode::ExistingBrowser
        );
        assert_eq!(
            serde_json::from_str::<BrowserAccountMode>(r#""isolatedOsl""#).unwrap(),
            BrowserAccountMode::IsolatedOsl
        );
        assert!(serde_json::from_str::<BrowserAccountMode>(r#""profile=C:\\Users""#).is_err());
        for browser in [
            BrowserImportId::Chrome,
            BrowserImportId::Edge,
            BrowserImportId::Firefox,
            BrowserImportId::Brave,
            BrowserImportId::Opera,
        ] {
            assert!(isolated_browser_profile_supported(browser));
        }
        assert!(!isolated_browser_profile_supported(
            BrowserImportId::DuckDuckGo
        ));
    }

    #[test]
    fn outlook_is_rejected_before_any_browser_resolution() {
        let action = BrowserCompanionState::default().host(
            FirefoxServiceId::Outlook,
            None,
            BrowserAccountMode::ExistingBrowser,
            Path::new("."),
            "owner-a",
            0,
        );
        assert_eq!(action.status, BrowserCompanionActionKind::Failed);
        assert_eq!(action.reason, BrowserCompanionReason::NativeAppRequired);
        assert_eq!(action.mode, "none");
    }

    #[test]
    fn prelaunch_snapshot_excludes_hidden_and_minimized_top_level_windows() {
        assert!(window_candidate_is_eligible(
            false, false, false, false, true, false
        ));
        assert!(window_candidate_is_eligible(
            false, false, false, true, false, true
        ));
        assert!(!window_candidate_is_eligible(
            true, false, false, true, false, true
        ));
        assert!(!window_candidate_is_eligible(
            false, true, false, true, false, true
        ));

        assert!(!window_candidate_is_eligible(
            false, false, true, false, false, true
        ));
        assert!(!window_candidate_is_eligible(
            false, false, true, true, true, true
        ));
        assert!(!window_candidate_is_eligible(
            false, false, true, true, false, false
        ));
        assert!(window_candidate_is_eligible(
            false, false, true, true, false, true
        ));
    }
}
