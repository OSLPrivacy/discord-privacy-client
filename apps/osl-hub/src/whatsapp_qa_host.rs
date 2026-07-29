//! Narrow, existing-session-only host for the official Microsoft Store WhatsApp app.
//!
//! This module never launches, installs, relinks, or terminates WhatsApp. It accepts
//! no path, URL, process, window, account, or credential from IPC. The only eligible
//! process is the exact executable resolved through the current user's fixed AppX
//! package-family registration, and exactly one exact main window must exist.

use serde::Serialize;

#[cfg(any(target_os = "windows", test))]
use std::path::Path;
#[cfg(target_os = "windows")]
use std::path::PathBuf;
#[cfg(target_os = "windows")]
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
#[cfg(target_os = "windows")]
use std::sync::{mpsc, Arc, Mutex};

const WHATSAPP_PACKAGE_NAME: &str = "5319275A.WhatsAppDesktop";
const WHATSAPP_PACKAGE_PUBLISHER_ID: &str = "cv1g1gvanyjgm";
#[cfg(target_os = "windows")]
const WHATSAPP_PACKAGE_FAMILY: &str = "5319275A.WhatsAppDesktop_cv1g1gvanyjgm";
const WHATSAPP_WINDOW_CLASS: &str = "WinUIDesktopWin32WindowClass";
const WHATSAPP_WINDOW_TITLE: &str = "WhatsApp";

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WhatsAppQaStatus {
    Hosted,
    Resized,
    Focused,
    Detached,
    Failed,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WhatsAppQaReason {
    None,
    PlatformUnsupported,
    AppNotInstalled,
    ExistingSessionUnavailable,
    ExistingSessionAmbiguous,
    WindowIdentityChanged,
    OwnerWindowUnavailable,
    WindowOperationRejected,
    NotHosted,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WhatsAppQaResult {
    pub provider: &'static str,
    pub status: WhatsAppQaStatus,
    pub reason: WhatsAppQaReason,
    pub mode: &'static str,
    pub capture_protected: bool,
}

impl WhatsAppQaResult {
    fn failed(reason: WhatsAppQaReason) -> Self {
        Self {
            provider: "whatsapp",
            status: WhatsAppQaStatus::Failed,
            reason,
            mode: "none",
            capture_protected: false,
        }
    }
    #[cfg(target_os = "windows")]
    fn success(status: WhatsAppQaStatus) -> Self {
        Self {
            provider: "whatsapp",
            status,
            reason: WhatsAppQaReason::None,
            mode: "existingNativeCompanion",
            capture_protected: false,
        }
    }
}

#[derive(Default)]
pub struct WhatsAppQaHostState {
    #[cfg(target_os = "windows")]
    inner: Mutex<Option<windows::ClaimedWindow>>,
    #[cfg(target_os = "windows")]
    next_generation: AtomicU64,
}

#[derive(Debug, Clone, Copy)]
#[cfg(target_os = "windows")]
pub(crate) struct WhatsAppQaAccessibilityTarget {
    pub(crate) generation: u64,
    pub(crate) window: isize,
    pub(crate) process_id: u32,
    pub(crate) window_rect: [i32; 4],
}

impl WhatsAppQaHostState {
    #[cfg(target_os = "windows")]
    pub(crate) fn with_current_accessibility_target<T>(
        &self,
        operation: impl FnOnce(WhatsAppQaAccessibilityTarget) -> Result<T, String>,
    ) -> Result<T, String> {
        let (target, identity) = {
            let slot = self
                .inner
                .lock()
                .map_err(|_| "WhatsApp QA host unavailable".to_owned())?;
            let claimed = slot
                .as_ref()
                .ok_or_else(|| "WhatsApp QA host is not active".to_owned())?;
            let identity = (
                claimed.hwnd,
                claimed.parent,
                claimed.pid,
                claimed.creation,
                claimed.session,
                claimed.path.clone(),
            );
            if !claimed.healthy.load(Ordering::Acquire) || !windows::identity_valid(&identity) {
                return Err("WhatsApp QA window identity changed".to_owned());
            }
            let mut rect: windows_sys::Win32::Foundation::RECT = unsafe { std::mem::zeroed() };
            if unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect(
                    claimed.hwnd as _,
                    &mut rect,
                )
            } == 0
            {
                return Err("WhatsApp QA window geometry unavailable".to_owned());
            }
            (
                WhatsAppQaAccessibilityTarget {
                    generation: claimed.generation,
                    window: claimed.hwnd,
                    process_id: claimed.pid,
                    window_rect: [rect.left, rect.top, rect.right, rect.bottom],
                },
                identity,
            )
        };
        let result = operation(target)?;
        let slot = self
            .inner
            .lock()
            .map_err(|_| "WhatsApp QA host unavailable".to_owned())?;
        let current = slot
            .as_ref()
            .ok_or_else(|| "WhatsApp QA host changed during accessibility work".to_owned())?;
        if current.generation != target.generation
            || !current.healthy.load(Ordering::Acquire)
            || !windows::identity_valid(&identity)
        {
            return Err("WhatsApp QA window identity changed during accessibility work".to_owned());
        }
        Ok(result)
    }

    pub fn claim(&self, trusted_parent: isize) -> WhatsAppQaResult {
        #[cfg(target_os = "windows")]
        {
            windows::claim(self, trusted_parent)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = trusted_parent;
            WhatsAppQaResult::failed(WhatsAppQaReason::PlatformUnsupported)
        }
    }

    pub fn resize(&self, trusted_parent: isize) -> WhatsAppQaResult {
        #[cfg(target_os = "windows")]
        {
            windows::resize(self, trusted_parent)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = trusted_parent;
            WhatsAppQaResult::failed(WhatsAppQaReason::PlatformUnsupported)
        }
    }

    pub fn focus(&self) -> WhatsAppQaResult {
        #[cfg(target_os = "windows")]
        {
            windows::focus(self)
        }
        #[cfg(not(target_os = "windows"))]
        {
            WhatsAppQaResult::failed(WhatsAppQaReason::PlatformUnsupported)
        }
    }

    pub fn detach(&self) -> WhatsAppQaResult {
        #[cfg(target_os = "windows")]
        {
            windows::detach(self)
        }
        #[cfg(not(target_os = "windows"))]
        {
            WhatsAppQaResult::failed(WhatsAppQaReason::PlatformUnsupported)
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for WhatsAppQaHostState {
    fn drop(&mut self) {
        if let Ok(slot) = self.inner.get_mut() {
            if let Some(claimed) = slot.take() {
                windows::stop_and_restore(claimed);
            }
        }
    }
}

#[cfg(any(target_os = "windows", test))]
fn package_full_name_matches(value: &str) -> bool {
    let parts = value.split('_').collect::<Vec<_>>();
    if parts.len() != 5
        || parts[0] != WHATSAPP_PACKAGE_NAME
        || parts[3] != ""
        || parts[4] != WHATSAPP_PACKAGE_PUBLISHER_ID
        || !matches!(parts[2], "x64" | "x86" | "arm64" | "neutral")
    {
        return false;
    }
    let version = parts[1].split('.').collect::<Vec<_>>();
    version.len() == 4
        && version
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

#[cfg(any(target_os = "windows", test))]
fn trusted_package_path(path: &Path, program_files: &Path) -> bool {
    if path
        .components()
        .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return false;
    }
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let Some(parent) = path.parent() else {
        return false;
    };
    parent
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("WindowsApps"))
        && parent.parent().is_some_and(|root| {
            root.as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case(&program_files.as_os_str().to_string_lossy())
        })
        && package_full_name_matches(name)
}

#[cfg(any(target_os = "windows", test))]
fn exact_main_window(class_name: &str, title: &str) -> bool {
    class_name == WHATSAPP_WINDOW_CLASS && title == WHATSAPP_WINDOW_TITLE
}

#[cfg(any(target_os = "windows", test))]
fn exact_candidate_count(count: usize) -> Result<(), WhatsAppQaReason> {
    match count {
        1 => Ok(()),
        0 => Err(WhatsAppQaReason::ExistingSessionUnavailable),
        _ => Err(WhatsAppQaReason::ExistingSessionAmbiguous),
    }
}

#[cfg(any(target_os = "windows", test))]
fn identity_matches(
    stored_pid: u32,
    pid: u32,
    stored_creation: u64,
    creation: u64,
    stored_session: u32,
    session: u32,
    expected: &Path,
    actual: &Path,
) -> bool {
    stored_pid == pid
        && stored_creation == creation
        && stored_session == session
        && expected == actual
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use std::ffi::{OsStr, OsString};
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::time::Duration;
    use windows_sys::Win32::Foundation::{
        CloseHandle, BOOL, ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, FILETIME, HANDLE, HWND,
        LPARAM, RECT,
    };
    use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
    use windows_sys::Win32::Storage::Packaging::Appx::{
        GetPackagePathByFullName, GetPackagesByPackageFamily,
    };
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::Shell::{
        FOLDERID_ProgramFiles, SHGetKnownFolderPath, KF_FLAG_DEFAULT,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetClientRect, GetWindowRect, GetWindowTextLengthW,
        GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible,
        SetForegroundWindow, SetWindowPos, ShowWindow, HWND_TOP, SWP_NOACTIVATE, SWP_SHOWWINDOW,
        SW_MINIMIZE, SW_RESTORE,
    };

    const MAX_PACKAGES: u32 = 32;
    const MAX_BUFFER: u32 = 32_768;

    #[link(name = "kernel32")]
    extern "system" {
        fn ProcessIdToSessionId(process_id: u32, session_id: *mut u32) -> BOOL;
    }

    pub(super) struct ClaimedWindow {
        pub(super) generation: u64,
        pub(super) hwnd: isize,
        pub(super) parent: isize,
        pub(super) pid: u32,
        pub(super) creation: u64,
        pub(super) session: u32,
        pub(super) path: PathBuf,
        original_rect: [i32; 4],
        stop: mpsc::Sender<()>,
        worker: Option<std::thread::JoinHandle<()>>,
        pub(super) healthy: Arc<AtomicBool>,
    }

    struct CandidateSearch {
        expected: PathBuf,
        candidates: Vec<(isize, u32, u64, u32, PathBuf)>,
    }

    pub(super) fn claim(state: &WhatsAppQaHostState, parent: isize) -> WhatsAppQaResult {
        if parent == 0 || unsafe { IsWindow(parent as HWND) } == 0 {
            return WhatsAppQaResult::failed(WhatsAppQaReason::OwnerWindowUnavailable);
        }
        let mut slot = state
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if slot.is_some() {
            return WhatsAppQaResult::failed(WhatsAppQaReason::WindowOperationRejected);
        }
        let Some(expected) = package_executable() else {
            return WhatsAppQaResult::failed(WhatsAppQaReason::AppNotInstalled);
        };
        let expected = expected.canonicalize().unwrap_or(expected);
        let mut search = CandidateSearch {
            expected,
            candidates: Vec::new(),
        };
        unsafe {
            EnumWindows(
                Some(enumerate_candidate),
                &mut search as *mut CandidateSearch as LPARAM,
            );
        }
        if let Err(reason) = exact_candidate_count(search.candidates.len()) {
            return WhatsAppQaResult::failed(reason);
        }
        let (hwnd, pid, creation, session, path) = search.candidates.remove(0);
        let mut rect: RECT = unsafe { std::mem::zeroed() };
        if unsafe { GetWindowRect(hwnd as HWND, &mut rect) } == 0 {
            return WhatsAppQaResult::failed(WhatsAppQaReason::WindowOperationRejected);
        }
        let original_rect = [rect.left, rect.top, rect.right, rect.bottom];
        let snapshot = (hwnd, parent, pid, creation, session, path.clone());
        if !reconcile(&snapshot) {
            return WhatsAppQaResult::failed(WhatsAppQaReason::WindowOperationRejected);
        }
        let (send, receive) = mpsc::channel();
        let healthy = Arc::new(AtomicBool::new(true));
        let worker_health = Arc::clone(&healthy);
        let worker = std::thread::spawn(move || {
            while receive.recv_timeout(Duration::from_millis(250)).is_err() {
                if !reconcile(&snapshot) {
                    worker_health.store(false, Ordering::Release);
                    break;
                }
            }
        });
        let generation = state
            .next_generation
            .fetch_add(1, Ordering::AcqRel)
            .saturating_add(1);
        *slot = Some(ClaimedWindow {
            generation,
            hwnd,
            parent,
            pid,
            creation,
            session,
            path,
            original_rect,
            stop: send,
            worker: Some(worker),
            healthy,
        });
        WhatsAppQaResult::success(WhatsAppQaStatus::Hosted)
    }

    pub(super) fn resize(state: &WhatsAppQaHostState, parent: isize) -> WhatsAppQaResult {
        let mut slot = state
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(claimed) = slot.as_mut() else {
            return WhatsAppQaResult::failed(WhatsAppQaReason::NotHosted);
        };
        if parent != claimed.parent || !claimed.healthy.load(Ordering::Acquire) {
            return WhatsAppQaResult::failed(WhatsAppQaReason::WindowIdentityChanged);
        }
        let snapshot = (
            claimed.hwnd,
            claimed.parent,
            claimed.pid,
            claimed.creation,
            claimed.session,
            claimed.path.clone(),
        );
        if !reconcile(&snapshot) {
            claimed.healthy.store(false, Ordering::Release);
            return WhatsAppQaResult::failed(WhatsAppQaReason::WindowIdentityChanged);
        }
        WhatsAppQaResult::success(WhatsAppQaStatus::Resized)
    }

    pub(super) fn focus(state: &WhatsAppQaHostState) -> WhatsAppQaResult {
        let slot = state
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(claimed) = slot.as_ref() else {
            return WhatsAppQaResult::failed(WhatsAppQaReason::NotHosted);
        };
        let snapshot = (
            claimed.hwnd,
            claimed.parent,
            claimed.pid,
            claimed.creation,
            claimed.session,
            claimed.path.clone(),
        );
        if !claimed.healthy.load(Ordering::Acquire) || !identity_valid(&snapshot) {
            return WhatsAppQaResult::failed(WhatsAppQaReason::WindowIdentityChanged);
        }
        if unsafe { SetForegroundWindow(claimed.hwnd as HWND) } == 0 {
            return WhatsAppQaResult::failed(WhatsAppQaReason::WindowOperationRejected);
        }
        WhatsAppQaResult::success(WhatsAppQaStatus::Focused)
    }

    pub(super) fn detach(state: &WhatsAppQaHostState) -> WhatsAppQaResult {
        let claimed = state
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        let Some(claimed) = claimed else {
            return WhatsAppQaResult::failed(WhatsAppQaReason::NotHosted);
        };
        stop_and_restore(claimed);
        WhatsAppQaResult::success(WhatsAppQaStatus::Detached)
    }

    pub(super) fn stop_and_restore(mut claimed: ClaimedWindow) {
        let _ = claimed.stop.send(());
        if let Some(worker) = claimed.worker.take() {
            let _ = worker.join();
        }
        let [left, top, right, bottom] = claimed.original_rect;
        unsafe {
            SetWindowPos(
                claimed.hwnd as HWND,
                std::ptr::null_mut(),
                left,
                top,
                right - left,
                bottom - top,
                SWP_NOACTIVATE,
            );
        }
    }

    unsafe extern "system" fn enumerate_candidate(hwnd: HWND, parameter: LPARAM) -> BOOL {
        let search = &mut *(parameter as *mut CandidateSearch);
        let Some(class) = window_class(hwnd) else {
            return 1;
        };
        let Some(title) = window_title(hwnd) else {
            return 1;
        };
        if !exact_main_window(&class, &title) {
            return 1;
        }
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if let Some((path, creation, session)) = process_identity(pid) {
            let actual = path.canonicalize().unwrap_or(path);
            if actual == search.expected {
                search
                    .candidates
                    .push((hwnd as isize, pid, creation, session, actual));
            }
        }
        1
    }

    fn reconcile(snapshot: &(isize, isize, u32, u64, u32, PathBuf)) -> bool {
        if !identity_valid(snapshot) {
            return false;
        }
        let (hwnd, parent, ..) = snapshot;
        if unsafe { IsIconic(*parent as HWND) } != 0 {
            if unsafe { IsIconic(*hwnd as HWND) } == 0 {
                unsafe {
                    ShowWindow(*hwnd as HWND, SW_MINIMIZE);
                }
            }
            return true;
        }
        if unsafe { IsIconic(*hwnd as HWND) } != 0 {
            unsafe {
                ShowWindow(*hwnd as HWND, SW_RESTORE);
            }
        }
        let Some(rect) = parent_rect(*parent as HWND) else {
            return false;
        };
        let mut actual: RECT = unsafe { std::mem::zeroed() };
        let matches = unsafe { GetWindowRect(*hwnd as HWND, &mut actual) } != 0
            && [actual.left, actual.top, actual.right, actual.bottom] == rect
            && unsafe { IsWindowVisible(*hwnd as HWND) } != 0;
        matches
            || unsafe {
                SetWindowPos(
                    *hwnd as HWND,
                    HWND_TOP,
                    rect[0],
                    rect[1],
                    rect[2] - rect[0],
                    rect[3] - rect[1],
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                ) != 0
            }
    }

    pub(super) fn identity_valid(snapshot: &(isize, isize, u32, u64, u32, PathBuf)) -> bool {
        let (hwnd, parent, pid, creation, session, expected) = snapshot;
        if unsafe { IsWindow(*hwnd as HWND) } == 0 || unsafe { IsWindow(*parent as HWND) } == 0 {
            return false;
        }
        let mut current_pid = 0;
        unsafe {
            GetWindowThreadProcessId(*hwnd as HWND, &mut current_pid);
        }
        let Some(class) = (unsafe { window_class(*hwnd as HWND) }) else {
            return false;
        };
        let Some(title) = (unsafe { window_title(*hwnd as HWND) }) else {
            return false;
        };
        let Some((path, current_creation, current_session)) = process_identity(current_pid) else {
            return false;
        };
        let mut parent_pid = 0;
        unsafe {
            GetWindowThreadProcessId(*parent as HWND, &mut parent_pid);
        }
        if parent_pid != std::process::id() {
            return false;
        }
        let mut osl_session = 0;
        if unsafe { ProcessIdToSessionId(parent_pid, &mut osl_session) } == 0
            || osl_session != *session
        {
            return false;
        }
        identity_matches(
            *pid,
            current_pid,
            *creation,
            current_creation,
            *session,
            current_session,
            expected,
            &path.canonicalize().unwrap_or(path),
        ) && exact_main_window(&class, &title)
    }

    fn parent_rect(parent: HWND) -> Option<[i32; 4]> {
        let mut rect: RECT = unsafe { std::mem::zeroed() };
        if unsafe { GetClientRect(parent, &mut rect) } == 0 {
            return None;
        }
        let mut point = windows_sys::Win32::Foundation::POINT {
            x: rect.left,
            y: rect.top + 98,
        };
        if unsafe { ClientToScreen(parent, &mut point) } == 0 {
            return None;
        }
        Some([
            point.x,
            point.y,
            point.x + rect.right - rect.left,
            point.y + (rect.bottom - rect.top - 98).max(1),
        ])
    }

    fn process_identity(pid: u32) -> Option<(PathBuf, u64, u32)> {
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            return None;
        }
        let result = process_identity_from_handle(handle, pid);
        unsafe {
            CloseHandle(handle);
        }
        result
    }

    fn process_identity_from_handle(handle: HANDLE, pid: u32) -> Option<(PathBuf, u64, u32)> {
        let mut path = vec![0u16; 32_768];
        let mut size = path.len() as u32;
        if unsafe { QueryFullProcessImageNameW(handle, 0, path.as_mut_ptr(), &mut size) } == 0
            || size == 0
        {
            return None;
        }
        let mut creation: FILETIME = unsafe { std::mem::zeroed() };
        let mut exit: FILETIME = unsafe { std::mem::zeroed() };
        let mut kernel: FILETIME = unsafe { std::mem::zeroed() };
        let mut user: FILETIME = unsafe { std::mem::zeroed() };
        if unsafe { GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user) } == 0
        {
            return None;
        }
        let mut session = 0;
        if unsafe { ProcessIdToSessionId(pid, &mut session) } == 0 {
            return None;
        }
        Some((
            PathBuf::from(OsString::from_wide(&path[..size as usize])),
            (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime),
            session,
        ))
    }

    unsafe fn window_class(hwnd: HWND) -> Option<String> {
        let mut value = vec![0u16; 256];
        let length = GetClassNameW(hwnd, value.as_mut_ptr(), value.len() as i32);
        (length > 0).then(|| String::from_utf16_lossy(&value[..length as usize]))
    }
    unsafe fn window_title(hwnd: HWND) -> Option<String> {
        let length = GetWindowTextLengthW(hwnd);
        if length < 0 || length > 256 {
            return None;
        }
        let mut value = vec![0u16; length as usize + 1];
        let copied = GetWindowTextW(hwnd, value.as_mut_ptr(), value.len() as i32);
        (copied >= 0).then(|| String::from_utf16_lossy(&value[..copied as usize]))
    }

    fn package_executable() -> Option<PathBuf> {
        let family = wide(WHATSAPP_PACKAGE_FAMILY);
        let mut count = 0;
        let mut units = 0;
        if unsafe {
            GetPackagesByPackageFamily(
                family.as_ptr(),
                &mut count,
                std::ptr::null_mut(),
                &mut units,
                std::ptr::null_mut(),
            )
        } != ERROR_INSUFFICIENT_BUFFER
            || count == 0
            || count > MAX_PACKAGES
            || !(1..=MAX_BUFFER).contains(&units)
        {
            return None;
        }
        let mut names = vec![std::ptr::null_mut(); count as usize];
        let mut buffer = vec![0u16; units as usize];
        if unsafe {
            GetPackagesByPackageFamily(
                family.as_ptr(),
                &mut count,
                names.as_mut_ptr(),
                &mut units,
                buffer.as_mut_ptr(),
            )
        } != ERROR_SUCCESS
        {
            return None;
        }
        let program_files = program_files()?;
        let candidates = names[..count as usize]
            .iter()
            .filter_map(|pointer| text_in_buffer(*pointer, &buffer))
            .filter(|name| package_full_name_matches(name))
            .filter_map(|name| {
                let root = package_path(&name)?;
                trusted_package_path(&root, &program_files).then(|| root.join("WhatsApp.Root.exe"))
            })
            .collect::<Vec<_>>();
        (candidates.len() == 1).then(|| candidates[0].clone())
    }

    fn package_path(name: &str) -> Option<PathBuf> {
        let name = wide(name);
        let mut units = 0;
        if unsafe { GetPackagePathByFullName(name.as_ptr(), &mut units, std::ptr::null_mut()) }
            != ERROR_INSUFFICIENT_BUFFER
            || !(2..=MAX_BUFFER).contains(&units)
        {
            return None;
        }
        let mut path = vec![0u16; units as usize];
        if unsafe { GetPackagePathByFullName(name.as_ptr(), &mut units, path.as_mut_ptr()) }
            != ERROR_SUCCESS
        {
            return None;
        }
        let end = path.iter().position(|unit| *unit == 0)?;
        Some(PathBuf::from(OsString::from_wide(&path[..end])))
    }
    fn text_in_buffer(pointer: *const u16, buffer: &[u16]) -> Option<String> {
        if pointer.is_null() {
            return None;
        }
        let start = buffer.as_ptr() as usize;
        let end = start.checked_add(buffer.len() * 2)?;
        let address = pointer as usize;
        if address < start || address >= end || (address - start) % 2 != 0 {
            return None;
        }
        let tail = &buffer[(address - start) / 2..];
        let nul = tail.iter().position(|unit| *unit == 0)?;
        String::from_utf16(&tail[..nul]).ok()
    }
    fn wide(value: &str) -> Vec<u16> {
        OsStr::new(value)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }
    fn program_files() -> Option<PathBuf> {
        let mut raw = std::ptr::null_mut();
        if unsafe {
            SHGetKnownFolderPath(
                &FOLDERID_ProgramFiles,
                KF_FLAG_DEFAULT as u32,
                std::ptr::null_mut(),
                &mut raw,
            )
        } < 0
            || raw.is_null()
        {
            return None;
        }
        let mut length = 0usize;
        unsafe {
            while *raw.add(length) != 0 {
                length += 1;
            }
        }
        let path = PathBuf::from(OsString::from_wide(unsafe {
            std::slice::from_raw_parts(raw, length)
        }));
        unsafe {
            CoTaskMemFree(raw.cast());
        }
        Some(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_store_identity_and_path_are_fail_closed() {
        assert!(package_full_name_matches(
            "5319275A.WhatsAppDesktop_2.2627.101.0_x64__cv1g1gvanyjgm"
        ));
        for rejected in [
            "5319275A.WhatsAppDesktop_2.2627.101_x64__cv1g1gvanyjgm",
            "5319275A.WhatsAppDesktop_2.2627.101.0_x64__attacker",
            "Fake_2.2627.101.0_x64__cv1g1gvanyjgm",
        ] {
            assert!(!package_full_name_matches(rejected));
        }
        let root = Path::new("C:/Program Files");
        assert!(trusted_package_path(
            &root.join("WindowsApps/5319275A.WhatsAppDesktop_2.2627.101.0_x64__cv1g1gvanyjgm"),
            root
        ));
        assert!(!trusted_package_path(
            &root.join(
                "WindowsApps/nested/5319275A.WhatsAppDesktop_2.2627.101.0_x64__cv1g1gvanyjgm"
            ),
            root
        ));
    }

    #[test]
    fn exact_window_and_single_candidate_are_required() {
        assert!(exact_main_window(
            "WinUIDesktopWin32WindowClass",
            "WhatsApp"
        ));
        assert!(!exact_main_window("Chrome_WidgetWin_1", "WhatsApp"));
        assert_eq!(
            exact_candidate_count(0),
            Err(WhatsAppQaReason::ExistingSessionUnavailable)
        );
        assert_eq!(
            exact_candidate_count(2),
            Err(WhatsAppQaReason::ExistingSessionAmbiguous)
        );
    }

    #[test]
    fn pid_reuse_cross_session_and_path_changes_are_rejected() {
        let path = Path::new("C:/Program Files/WindowsApps/WhatsApp/WhatsApp.Root.exe");
        assert!(identity_matches(7, 7, 8, 8, 9, 9, path, path));
        assert!(!identity_matches(7, 7, 8, 10, 9, 9, path, path));
        assert!(!identity_matches(7, 7, 8, 8, 9, 10, path, path));
        assert!(!identity_matches(
            7,
            7,
            8,
            8,
            9,
            9,
            path,
            Path::new("C:/Temp/WhatsApp.Root.exe")
        ));
    }
}
