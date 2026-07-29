//! Minimal fail-closed Windows host for an already-running Signal Desktop.
//!
//! Signal is never launched with a secondary profile and the renderer cannot
//! provide a path, PID, HWND, title, or command line. OSL claims one visible
//! first-party Signal primary window in its own Windows session, continuously
//! revalidates its process identity, and never terminates the borrowed process.

use crate::native_apps::NativeAppId;
use serde::Serialize;
#[cfg(any(target_os = "windows", test))]
use sha2::{Digest, Sha256};
use std::path::Path;
#[cfg(target_os = "windows")]
use std::sync::Mutex;

#[cfg(target_os = "windows")]
const TRUSTED_VERTICAL_RESERVE: i32 = 98;
#[cfg(any(target_os = "windows", test))]
const SIGNAL_PRIMARY_WINDOW_CLASS: &str = "Chrome_WidgetWin_1";
#[cfg(any(target_os = "windows", test))]
const SIGNAL_PRIMARY_WINDOW_TITLE: &str = "Signal";

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NativeWindowHostStatus {
    ExistingSession,
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
    ExistingSessionUnavailable,
    ExistingSessionAmbiguous,
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
    /// Fixed label only; no provider data, path, PID, HWND, or window text.
    pub mode: &'static str,
    /// Borrowed top-level windows are not children of OSL's capture-excluded
    /// window, so this must remain false.
    pub capture_protected: bool,
}

/// Opaque native-only identity used to bind Signal UIA evidence to the exact
/// claimed process/window lifetime. It is never serialized to the renderer.
#[derive(Clone, Eq, PartialEq)]
pub struct SignalNativeWindowBinding {
    pub host_generation: u64,
    pub window_identity_sha256: [u8; 32],
}

impl NativeWindowHostResult {
    fn unsupported(id: NativeAppId, reason: NativeWindowHostReason) -> Self {
        Self {
            id,
            status: NativeWindowHostStatus::Unsupported,
            reason,
            mode: "none",
            capture_protected: false,
        }
    }

    #[cfg(any(target_os = "windows", test))]
    fn failed(id: NativeAppId, reason: NativeWindowHostReason) -> Self {
        Self {
            id,
            status: NativeWindowHostStatus::Failed,
            reason,
            mode: "none",
            capture_protected: false,
        }
    }

    #[cfg(any(target_os = "windows", test))]
    fn existing(id: NativeAppId, status: NativeWindowHostStatus) -> Self {
        Self {
            id,
            status,
            reason: NativeWindowHostReason::None,
            mode: "existingNativeCompanion",
            capture_protected: false,
        }
    }
}

#[derive(Debug, Default)]
pub struct NativeWindowHostState {
    #[cfg(target_os = "windows")]
    inner: Mutex<Option<HostedSignalWindow>>,
}

#[cfg(target_os = "windows")]
impl Drop for NativeWindowHostState {
    fn drop(&mut self) {
        if let Ok(slot) = self.inner.get_mut() {
            if let Some(mut hosted) = slot.take() {
                hosted.tether.take();
                hosted.recovery_guardian.take();
                // Best-effort restoration only. The borrowed Signal process is
                // deliberately never killed, even if identity or restoration
                // verification fails.
                if !unsafe { windows::restore_borrowed_window(&hosted) } {
                    // Do not emit provider/window/process details. Detach has
                    // already failed closed, and the borrowed app stays live.
                }
            }
        }
    }
}

#[cfg(target_os = "windows")]
#[derive(Debug)]
struct HostedSignalWindow {
    id: NativeAppId,
    process_id: u32,
    creation_time: u64,
    session_id: u32,
    executable_path: std::path::PathBuf,
    process: windows::ProcessHandle,
    // Keeps the publisher-verified executable file identity pinned while the
    // companion is claimed.
    _executable: crate::windows_executable_trust::TrustedExecutable,
    window: isize,
    previous_placement: windows::BorrowedPlacement,
    tether: Option<windows::BorrowedTether>,
    recovery_guardian: Option<windows::RecoveryGuardian>,
}

impl NativeWindowHostState {
    /// Claim the exact already-running Signal Desktop primary window.
    ///
    /// Profile arguments are intentionally ignored: Signal linking state stays
    /// in its existing official profile and no second instance is launched.
    pub fn host(
        &self,
        id: NativeAppId,
        osl_profile_root: &Path,
        owner_osl_user_id: &str,
        trusted_parent: isize,
    ) -> NativeWindowHostResult {
        #[cfg(target_os = "windows")]
        {
            let _ = (osl_profile_root, owner_osl_user_id);
            windows::host(self, id, trusted_parent)
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
                NativeAppId::Signal,
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
                NativeAppId::Signal,
                NativeWindowHostReason::PlatformUnsupported,
            )
        }
    }

    pub fn detach(&self) -> NativeWindowHostResult {
        #[cfg(target_os = "windows")]
        {
            windows::detach(self)
        }
        #[cfg(not(target_os = "windows"))]
        {
            NativeWindowHostResult::unsupported(
                NativeAppId::Signal,
                NativeWindowHostReason::PlatformUnsupported,
            )
        }
    }

    /// Returns an opaque identity only after revalidating the retained exact
    /// Signal process and HWND. No PID, HWND, path, or provider text escapes.
    pub fn current_signal_binding(&self) -> Option<SignalNativeWindowBinding> {
        #[cfg(target_os = "windows")]
        {
            windows::current_signal_binding(self)
        }
        #[cfg(not(target_os = "windows"))]
        {
            None
        }
    }
}

/// Handle the private crash-restoration subprocess mode before constructing
/// Tauri. The desktop entry point must return immediately when this returns
/// true. No renderer-controlled values reach this parser.
pub fn run_signal_guardian_if_requested() -> bool {
    #[cfg(target_os = "windows")]
    {
        windows::run_guardian_if_requested(&std::env::args().collect::<Vec<_>>())
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

#[cfg(any(target_os = "windows", test))]
fn secondary_instance_verified(_id: NativeAppId) -> bool {
    false
}

#[cfg(any(target_os = "windows", test))]
fn existing_session_supported(id: NativeAppId) -> bool {
    id == NativeAppId::Signal
}

#[cfg(any(target_os = "windows", test))]
fn signal_window_identity_allowed(visible: bool, class_name: &str, title: &str) -> bool {
    visible && class_name == SIGNAL_PRIMARY_WINDOW_CLASS && title == SIGNAL_PRIMARY_WINDOW_TITLE
}

#[cfg(any(target_os = "windows", test))]
fn exact_existing_candidate_count(count: usize) -> Result<(), NativeWindowHostReason> {
    match count {
        1 => Ok(()),
        0 => Err(NativeWindowHostReason::ExistingSessionUnavailable),
        _ => Err(NativeWindowHostReason::ExistingSessionAmbiguous),
    }
}

#[cfg(any(target_os = "windows", test))]
fn borrowed_identity_fields_match(
    stored_pid: u32,
    current_pid: u32,
    stored_creation_time: u64,
    current_creation_time: u64,
    stored_session: u32,
    current_session: u32,
    stored_path: &Path,
    current_path: &Path,
) -> bool {
    stored_pid != 0
        && stored_pid == current_pid
        && stored_creation_time != 0
        && stored_creation_time == current_creation_time
        && stored_session == current_session
        && stored_path == current_path
}

#[cfg(any(target_os = "windows", test))]
fn guardian_should_restore(parent_identity_alive: bool, target_identity_exact: bool) -> bool {
    !parent_identity_alive && target_identity_exact
}

#[cfg(any(target_os = "windows", test))]
fn tether_should_continue(target_identity_exact: bool, parent_identity_exact: bool) -> bool {
    target_identity_exact && parent_identity_exact
}

#[cfg(any(target_os = "windows", test))]
fn native_binding_identity_digest(
    process_id: u32,
    creation_time: u64,
    session_id: u32,
    window: isize,
    executable_path_identity: &[u8],
) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"OSL/signal-native-window-binding/v1");
    hash.update(process_id.to_be_bytes());
    hash.update(creation_time.to_be_bytes());
    hash.update(session_id.to_be_bytes());
    hash.update(window.to_be_bytes());
    hash.update(executable_path_identity);
    hash.finalize().into()
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use crate::windows_executable_trust::{verify_executable, ExecutablePublisher};
    use std::ffi::OsString;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::os::windows::process::CommandExt;
    use std::process::{Child, Command, Stdio};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use std::thread;
    use std::time::Duration;
    use windows_sys::Win32::Foundation::{
        CloseHandle, BOOL, FILETIME, HANDLE, HWND, LPARAM, POINT, RECT,
    };
    use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    use windows_sys::Win32::UI::Shell::{
        FOLDERID_LocalAppData, SHGetKnownFolderPath, KF_FLAG_DEFAULT,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetClientRect, GetWindowPlacement, GetWindowTextW,
        GetWindowThreadProcessId, IsWindowVisible, SetForegroundWindow, SetWindowPlacement,
        SetWindowPos, ShowWindow, HWND_TOP, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_RESTORE,
        WINDOWPLACEMENT,
    };

    const MAX_CANDIDATES: usize = 2;
    const TETHER_INTERVAL: Duration = Duration::from_millis(200);
    const GUARDIAN_INTERVAL: Duration = Duration::from_millis(200);
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const GUARDIAN_MARKER: &str = "--osl-signal-window-guardian-v1";

    #[link(name = "kernel32")]
    extern "system" {
        fn ProcessIdToSessionId(process_id: u32, session_id: *mut u32) -> BOOL;
    }

    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    pub(super) struct BorrowedPlacement {
        flags: u32,
        show_command: u32,
        minimum: [i32; 2],
        maximum: [i32; 2],
        normal_rect: [i32; 4],
    }

    #[derive(Debug, Clone)]
    struct BorrowedSnapshot {
        window: isize,
        process_id: u32,
        creation_time: u64,
        session_id: u32,
        executable_path: std::path::PathBuf,
        trusted_parent: isize,
    }

    #[derive(Debug)]
    pub(super) struct BorrowedTether {
        stop: Arc<AtomicBool>,
        worker: Option<thread::JoinHandle<()>>,
    }

    impl BorrowedTether {
        fn start(snapshot: BorrowedSnapshot) -> Self {
            let stop = Arc::new(AtomicBool::new(false));
            let worker_stop = Arc::clone(&stop);
            let worker = thread::spawn(move || {
                while !worker_stop.load(Ordering::Acquire) {
                    let valid = unsafe {
                        tether_should_continue(
                            snapshot_identity_is_valid(&snapshot),
                            trusted_parent_is_current_process(snapshot.trusted_parent as HWND),
                        )
                    };
                    if !valid {
                        break;
                    }
                    if unsafe {
                        !align_to_parent(snapshot.window as HWND, snapshot.trusted_parent as HWND)
                    } {
                        break;
                    }
                    thread::sleep(TETHER_INTERVAL);
                }
            });
            Self {
                stop,
                worker: Some(worker),
            }
        }
    }

    impl Drop for BorrowedTether {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }

    #[derive(Debug)]
    pub(super) struct RecoveryGuardian {
        child: Child,
    }

    impl Drop for RecoveryGuardian {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    #[derive(Debug)]
    pub(super) struct ProcessHandle(HANDLE);

    unsafe impl Send for ProcessHandle {}

    impl Drop for ProcessHandle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { CloseHandle(self.0) };
            }
        }
    }

    impl ProcessHandle {
        fn open(process_id: u32) -> Option<Self> {
            let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
            (!handle.is_null()).then_some(Self(handle))
        }
    }

    struct Candidate {
        window: HWND,
        process_id: u32,
        creation_time: u64,
        session_id: u32,
        path: std::path::PathBuf,
        process: ProcessHandle,
    }

    struct Search {
        expected_path: std::path::PathBuf,
        expected_session: u32,
        overflowed: bool,
        candidates: Vec<Candidate>,
    }

    pub(super) fn host(
        state: &NativeWindowHostState,
        id: NativeAppId,
        parent: isize,
    ) -> NativeWindowHostResult {
        if !existing_session_supported(id) {
            return NativeWindowHostResult::unsupported(
                id,
                NativeWindowHostReason::SecondaryInstanceUnverified,
            );
        }
        if parent == 0 {
            return NativeWindowHostResult::failed(
                id,
                NativeWindowHostReason::OwnerWindowUnavailable,
            );
        }
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
        match unsafe { claim_existing_signal(parent as HWND) } {
            Ok(hosted) => {
                *guard = Some(hosted);
                NativeWindowHostResult::existing(id, NativeWindowHostStatus::ExistingSession)
            }
            Err(reason) => NativeWindowHostResult::failed(id, reason),
        }
    }

    unsafe fn claim_existing_signal(
        parent: HWND,
    ) -> Result<HostedSignalWindow, NativeWindowHostReason> {
        let configured_path = signal_executable().ok_or(NativeWindowHostReason::AppNotInstalled)?;
        let executable = verify_executable(&configured_path, ExecutablePublisher::Signal)
            .map_err(|_| NativeWindowHostReason::AppNotInstalled)?;
        let expected_path = executable
            .path()
            .canonicalize()
            .map_err(|_| NativeWindowHostReason::AppNotInstalled)?;
        let mut expected_session = 0u32;
        if ProcessIdToSessionId(std::process::id(), &mut expected_session) == 0 {
            return Err(NativeWindowHostReason::ExistingSessionUnavailable);
        }
        let mut search = Search {
            expected_path,
            expected_session,
            overflowed: false,
            candidates: Vec::with_capacity(MAX_CANDIDATES),
        };
        EnumWindows(
            Some(enum_signal_window),
            (&mut search as *mut Search) as LPARAM,
        );
        if search.overflowed {
            return Err(NativeWindowHostReason::ExistingSessionAmbiguous);
        }
        exact_existing_candidate_count(search.candidates.len())?;
        let candidate = search.candidates.pop().expect("count checked");
        let previous_placement = capture_placement(candidate.window)
            .ok_or(NativeWindowHostReason::WindowOperationRejected)?;
        let snapshot = BorrowedSnapshot {
            window: candidate.window as isize,
            process_id: candidate.process_id,
            creation_time: candidate.creation_time,
            session_id: candidate.session_id,
            executable_path: candidate.path.clone(),
            trusted_parent: parent as isize,
        };
        let recovery_guardian = spawn_recovery_guardian(&snapshot, previous_placement)?;
        let mut hosted = HostedSignalWindow {
            id: NativeAppId::Signal,
            process_id: candidate.process_id,
            creation_time: candidate.creation_time,
            session_id: candidate.session_id,
            executable_path: candidate.path,
            process: candidate.process,
            _executable: executable,
            window: candidate.window as isize,
            previous_placement,
            tether: None,
            recovery_guardian: Some(recovery_guardian),
        };
        if !hosted_identity_is_valid(&hosted) || !align_to_parent(candidate.window, parent) {
            return Err(NativeWindowHostReason::WindowIdentityChanged);
        }
        hosted.tether = Some(BorrowedTether::start(snapshot));
        Ok(hosted)
    }

    unsafe extern "system" fn enum_signal_window(window: HWND, parameter: LPARAM) -> BOOL {
        let search = &mut *(parameter as *mut Search);
        if search.candidates.len() >= MAX_CANDIDATES {
            search.overflowed = true;
            return 0;
        }
        let mut class_name = [0u16; 64];
        let class_len = GetClassNameW(window, class_name.as_mut_ptr(), class_name.len() as i32);
        let mut title = [0u16; 64];
        let title_len = GetWindowTextW(window, title.as_mut_ptr(), title.len() as i32);
        if class_len <= 0
            || title_len <= 0
            || !signal_window_identity_allowed(
                IsWindowVisible(window) != 0,
                &String::from_utf16_lossy(&class_name[..class_len as usize]),
                &String::from_utf16_lossy(&title[..title_len as usize]),
            )
        {
            return 1;
        }
        let Some(process_id) = window_process_id(window) else {
            return 1;
        };
        let Some((process, path, creation_time, session_id)) = process_identity(process_id) else {
            return 1;
        };
        if session_id != search.expected_session || path != search.expected_path {
            return 1;
        }
        // Re-verify the exact discovered image. This object is intentionally
        // short-lived; the separately verified configured image is retained.
        if verify_executable(&path, ExecutablePublisher::Signal).is_err() {
            return 1;
        }
        search.candidates.push(Candidate {
            window,
            process_id,
            creation_time,
            session_id,
            path,
            process,
        });
        1
    }

    fn process_identity(process_id: u32) -> Option<(ProcessHandle, std::path::PathBuf, u64, u32)> {
        let process = ProcessHandle::open(process_id)?;
        let (path, creation_time, session_id) = process_identity_from_handle(&process, process_id)?;
        Some((process, path, creation_time, session_id))
    }

    fn process_identity_from_handle(
        process: &ProcessHandle,
        process_id: u32,
    ) -> Option<(std::path::PathBuf, u64, u32)> {
        let mut path = vec![0u16; 32_768];
        let mut path_len = path.len() as u32;
        if unsafe { QueryFullProcessImageNameW(process.0, 0, path.as_mut_ptr(), &mut path_len) }
            == 0
            || path_len == 0
        {
            return None;
        }
        path.truncate(path_len as usize);
        let path = std::path::PathBuf::from(OsString::from_wide(&path))
            .canonicalize()
            .ok()?;
        let mut creation: FILETIME = unsafe { std::mem::zeroed() };
        let mut exit: FILETIME = unsafe { std::mem::zeroed() };
        let mut kernel: FILETIME = unsafe { std::mem::zeroed() };
        let mut user: FILETIME = unsafe { std::mem::zeroed() };
        if unsafe { GetProcessTimes(process.0, &mut creation, &mut exit, &mut kernel, &mut user) }
            == 0
        {
            return None;
        }
        let creation_time =
            (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime);
        let mut session_id = 0u32;
        if unsafe { ProcessIdToSessionId(process_id, &mut session_id) } == 0 {
            return None;
        }
        Some((path, creation_time, session_id))
    }

    unsafe fn hosted_identity_is_valid(hosted: &HostedSignalWindow) -> bool {
        window_identity_is_exact(hosted.window as HWND)
            && window_process_id(hosted.window as HWND) == Some(hosted.process_id)
            && process_identity_from_handle(&hosted.process, hosted.process_id).is_some_and(
                |(path, creation_time, session_id)| {
                    borrowed_identity_fields_match(
                        hosted.process_id,
                        hosted.process_id,
                        hosted.creation_time,
                        creation_time,
                        hosted.session_id,
                        session_id,
                        &hosted.executable_path,
                        &path,
                    ) && verify_executable(&path, ExecutablePublisher::Signal).is_ok()
                },
            )
    }

    unsafe fn snapshot_identity_is_valid(snapshot: &BorrowedSnapshot) -> bool {
        if !window_identity_is_exact(snapshot.window as HWND)
            || window_process_id(snapshot.window as HWND) != Some(snapshot.process_id)
        {
            return false;
        }
        process_identity(snapshot.process_id).is_some_and(|(_, path, creation_time, session_id)| {
            borrowed_identity_fields_match(
                snapshot.process_id,
                snapshot.process_id,
                snapshot.creation_time,
                creation_time,
                snapshot.session_id,
                session_id,
                &snapshot.executable_path,
                &path,
            )
        })
    }

    unsafe fn window_identity_is_exact(window: HWND) -> bool {
        if window.is_null() || IsWindowVisible(window) == 0 {
            return false;
        }
        let mut class_name = [0u16; 64];
        let class_len = GetClassNameW(window, class_name.as_mut_ptr(), class_name.len() as i32);
        let mut title = [0u16; 64];
        let title_len = GetWindowTextW(window, title.as_mut_ptr(), title.len() as i32);
        class_len > 0
            && title_len > 0
            && signal_window_identity_allowed(
                true,
                &String::from_utf16_lossy(&class_name[..class_len as usize]),
                &String::from_utf16_lossy(&title[..title_len as usize]),
            )
    }

    unsafe fn trusted_parent_is_current_process(parent: HWND) -> bool {
        window_process_id(parent) == Some(std::process::id())
    }

    unsafe fn window_process_id(window: HWND) -> Option<u32> {
        if window.is_null() {
            return None;
        }
        let mut process_id = 0u32;
        (GetWindowThreadProcessId(window, &mut process_id) != 0 && process_id != 0)
            .then_some(process_id)
    }

    unsafe fn capture_placement(window: HWND) -> Option<BorrowedPlacement> {
        let mut placement: WINDOWPLACEMENT = std::mem::zeroed();
        placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
        (GetWindowPlacement(window, &mut placement) != 0).then_some(BorrowedPlacement {
            flags: placement.flags,
            show_command: placement.showCmd,
            minimum: [placement.ptMinPosition.x, placement.ptMinPosition.y],
            maximum: [placement.ptMaxPosition.x, placement.ptMaxPosition.y],
            normal_rect: [
                placement.rcNormalPosition.left,
                placement.rcNormalPosition.top,
                placement.rcNormalPosition.right,
                placement.rcNormalPosition.bottom,
            ],
        })
    }

    unsafe fn restore_placement(window: HWND, placement: BorrowedPlacement) -> bool {
        let [left, top, right, bottom] = placement.normal_rect;
        let mut raw = WINDOWPLACEMENT {
            length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
            flags: placement.flags,
            showCmd: placement.show_command,
            ptMinPosition: POINT {
                x: placement.minimum[0],
                y: placement.minimum[1],
            },
            ptMaxPosition: POINT {
                x: placement.maximum[0],
                y: placement.maximum[1],
            },
            rcNormalPosition: RECT {
                left,
                top,
                right,
                bottom,
            },
        };
        SetWindowPlacement(window, &mut raw) != 0 && capture_placement(window) == Some(placement)
    }

    unsafe fn align_to_parent(window: HWND, parent: HWND) -> bool {
        let mut client: RECT = std::mem::zeroed();
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
        SetWindowPos(
            window,
            HWND_TOP,
            origin.x,
            origin.y,
            (client.right - client.left).max(1),
            (client.bottom - TRUSTED_VERTICAL_RESERVE).max(1),
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        ) != 0
    }

    pub(super) fn resize(state: &NativeWindowHostState, parent: isize) -> NativeWindowHostResult {
        let guard = match state.inner.lock() {
            Ok(guard) => guard,
            Err(_) => {
                return NativeWindowHostResult::failed(
                    NativeAppId::Signal,
                    NativeWindowHostReason::HostWindowUnavailable,
                )
            }
        };
        let Some(hosted) = guard.as_ref() else {
            return NativeWindowHostResult::failed(
                NativeAppId::Signal,
                NativeWindowHostReason::NotHosted,
            );
        };
        if parent == 0 || unsafe { !hosted_identity_is_valid(hosted) } {
            return NativeWindowHostResult::failed(
                hosted.id,
                NativeWindowHostReason::WindowIdentityChanged,
            );
        }
        if unsafe { align_to_parent(hosted.window as HWND, parent as HWND) } {
            NativeWindowHostResult::existing(hosted.id, NativeWindowHostStatus::Resized)
        } else {
            NativeWindowHostResult::failed(
                hosted.id,
                NativeWindowHostReason::WindowOperationRejected,
            )
        }
    }

    pub(super) fn focus(state: &NativeWindowHostState) -> NativeWindowHostResult {
        let guard = match state.inner.lock() {
            Ok(guard) => guard,
            Err(_) => {
                return NativeWindowHostResult::failed(
                    NativeAppId::Signal,
                    NativeWindowHostReason::HostWindowUnavailable,
                )
            }
        };
        let Some(hosted) = guard.as_ref() else {
            return NativeWindowHostResult::failed(
                NativeAppId::Signal,
                NativeWindowHostReason::NotHosted,
            );
        };
        if unsafe { !hosted_identity_is_valid(hosted) } {
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
        NativeWindowHostResult::existing(hosted.id, NativeWindowHostStatus::Focused)
    }

    pub(super) fn detach(state: &NativeWindowHostState) -> NativeWindowHostResult {
        let mut guard = match state.inner.lock() {
            Ok(guard) => guard,
            Err(_) => {
                return NativeWindowHostResult::failed(
                    NativeAppId::Signal,
                    NativeWindowHostReason::HostWindowUnavailable,
                )
            }
        };
        let Some(mut hosted) = guard.take() else {
            return NativeWindowHostResult::failed(
                NativeAppId::Signal,
                NativeWindowHostReason::NotHosted,
            );
        };
        let id = hosted.id;
        hosted.tether.take();
        hosted.recovery_guardian.take();
        if unsafe { restore_borrowed_window(&hosted) } {
            NativeWindowHostResult::existing(id, NativeWindowHostStatus::Detached)
        } else {
            // Never terminate or close borrowed Signal. Failure remains honest.
            NativeWindowHostResult::failed(id, NativeWindowHostReason::WindowOperationRejected)
        }
    }

    pub(super) fn current_signal_binding(
        state: &NativeWindowHostState,
    ) -> Option<SignalNativeWindowBinding> {
        let guard = state.inner.lock().ok()?;
        let hosted = guard.as_ref()?;
        if unsafe { !hosted_identity_is_valid(hosted) } {
            return None;
        }
        let path_identity = hosted
            .executable_path
            .as_os_str()
            .encode_wide()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        Some(SignalNativeWindowBinding {
            // Process creation time is nonzero and changes across reconnects;
            // the destination guard also advances its own lifecycle on every
            // claim, including a reconnect to the same live process.
            host_generation: hosted.creation_time,
            window_identity_sha256: native_binding_identity_digest(
                hosted.process_id,
                hosted.creation_time,
                hosted.session_id,
                hosted.window,
                &path_identity,
            ),
        })
    }

    pub(super) unsafe fn restore_borrowed_window(hosted: &HostedSignalWindow) -> bool {
        if !hosted_identity_is_valid(hosted) {
            return false;
        }
        restore_placement(hosted.window as HWND, hosted.previous_placement)
    }

    fn spawn_recovery_guardian(
        snapshot: &BorrowedSnapshot,
        placement: BorrowedPlacement,
    ) -> Result<RecoveryGuardian, NativeWindowHostReason> {
        let parent_pid = std::process::id();
        let (_, _, parent_creation_time, _) =
            process_identity(parent_pid).ok_or(NativeWindowHostReason::HostWindowUnavailable)?;
        let executable =
            std::env::current_exe().map_err(|_| NativeWindowHostReason::HostWindowUnavailable)?;
        let [min_x, min_y] = placement.minimum;
        let [max_x, max_y] = placement.maximum;
        let [left, top, right, bottom] = placement.normal_rect;
        let child = Command::new(executable)
            .arg(GUARDIAN_MARKER)
            .arg(snapshot.window.to_string())
            .arg(snapshot.process_id.to_string())
            .arg(snapshot.creation_time.to_string())
            .arg(snapshot.session_id.to_string())
            .arg(parent_pid.to_string())
            .arg(parent_creation_time.to_string())
            .arg(placement.flags.to_string())
            .arg(placement.show_command.to_string())
            .arg(min_x.to_string())
            .arg(min_y.to_string())
            .arg(max_x.to_string())
            .arg(max_y.to_string())
            .arg(left.to_string())
            .arg(top.to_string())
            .arg(right.to_string())
            .arg(bottom.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|_| NativeWindowHostReason::HostWindowUnavailable)?;
        Ok(RecoveryGuardian { child })
    }

    struct GuardianRequest {
        snapshot: BorrowedSnapshot,
        parent_pid: u32,
        parent_creation_time: u64,
        placement: BorrowedPlacement,
    }

    fn parse_guardian_request(arguments: &[String]) -> Option<GuardianRequest> {
        if arguments.len() != 18 || arguments.get(1).map(String::as_str) != Some(GUARDIAN_MARKER) {
            return None;
        }
        let parse_isize = |index: usize| arguments.get(index)?.parse::<isize>().ok();
        let parse_u32 = |index: usize| arguments.get(index)?.parse::<u32>().ok();
        let parse_u64 = |index: usize| arguments.get(index)?.parse::<u64>().ok();
        let parse_i32 = |index: usize| arguments.get(index)?.parse::<i32>().ok();
        let executable_path = signal_executable()?.canonicalize().ok()?;
        Some(GuardianRequest {
            snapshot: BorrowedSnapshot {
                window: parse_isize(2)?,
                process_id: parse_u32(3)?,
                creation_time: parse_u64(4)?,
                session_id: parse_u32(5)?,
                executable_path,
                trusted_parent: 0,
            },
            parent_pid: parse_u32(6)?,
            parent_creation_time: parse_u64(7)?,
            placement: BorrowedPlacement {
                flags: parse_u32(8)?,
                show_command: parse_u32(9)?,
                minimum: [parse_i32(10)?, parse_i32(11)?],
                maximum: [parse_i32(12)?, parse_i32(13)?],
                normal_rect: [
                    parse_i32(14)?,
                    parse_i32(15)?,
                    parse_i32(16)?,
                    parse_i32(17)?,
                ],
            },
        })
    }

    fn parent_identity_alive(process_id: u32, creation_time: u64) -> bool {
        process_identity(process_id)
            .is_some_and(|(_, _, current_creation, _)| current_creation == creation_time)
    }

    pub(super) fn run_guardian_if_requested(arguments: &[String]) -> bool {
        if arguments.get(1).map(String::as_str) != Some(GUARDIAN_MARKER) {
            return false;
        }
        let Some(request) = parse_guardian_request(arguments) else {
            // Marker is private and malformed guardian invocations must never
            // continue into the normal desktop startup path.
            return true;
        };
        while parent_identity_alive(request.parent_pid, request.parent_creation_time) {
            thread::sleep(GUARDIAN_INTERVAL);
        }
        // Signature verification is intentionally outside the 200 ms tether.
        // The guardian repeats it immediately before its one restoration
        // mutation, after all cheap process/window identity checks pass.
        let target_exact = unsafe { snapshot_identity_is_valid(&request.snapshot) }
            && verify_executable(
                &request.snapshot.executable_path,
                ExecutablePublisher::Signal,
            )
            .is_ok();
        if guardian_should_restore(false, target_exact) {
            let _ =
                unsafe { restore_placement(request.snapshot.window as HWND, request.placement) };
        }
        true
    }

    fn signal_executable() -> Option<std::path::PathBuf> {
        known_folder(&FOLDERID_LocalAppData)
            .map(|root| {
                root.join("Programs")
                    .join("signal-desktop")
                    .join("Signal.exe")
            })
            .filter(|path| path.is_file())
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
        let path = std::path::PathBuf::from(OsString::from_wide(unsafe {
            std::slice::from_raw_parts(raw, length)
        }));
        unsafe { CoTaskMemFree(raw.cast()) };
        Some(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_secondary_launch_is_always_disabled() {
        for id in [
            NativeAppId::Discord,
            NativeAppId::Telegram,
            NativeAppId::Signal,
            NativeAppId::Whatsapp,
            NativeAppId::Outlook,
        ] {
            assert!(!secondary_instance_verified(id));
        }
        assert!(existing_session_supported(NativeAppId::Signal));
        assert!(!existing_session_supported(NativeAppId::Discord));
    }

    #[test]
    fn signal_primary_window_identity_is_exact() {
        assert!(signal_window_identity_allowed(
            true,
            "Chrome_WidgetWin_1",
            "Signal"
        ));
        assert!(!signal_window_identity_allowed(
            false,
            "Chrome_WidgetWin_1",
            "Signal"
        ));
        assert!(!signal_window_identity_allowed(
            true,
            "Chrome_WidgetWin_0",
            "Signal"
        ));
        assert!(!signal_window_identity_allowed(
            true,
            "Chrome_WidgetWin_1",
            "Signal Beta"
        ));
        assert!(!signal_window_identity_allowed(
            true,
            "Chrome_WidgetWin_1",
            "Signal - chat"
        ));
    }

    #[test]
    fn signal_candidate_selection_fails_closed() {
        assert_eq!(exact_existing_candidate_count(1), Ok(()));
        assert_eq!(
            exact_existing_candidate_count(0),
            Err(NativeWindowHostReason::ExistingSessionUnavailable)
        );
        assert_eq!(
            exact_existing_candidate_count(2),
            Err(NativeWindowHostReason::ExistingSessionAmbiguous)
        );
    }

    #[test]
    fn borrowed_identity_requires_pid_creation_session_and_path() {
        let path = Path::new(r"C:\Program Files\Signal\Signal.exe");
        assert!(borrowed_identity_fields_match(
            7, 7, 11, 11, 2, 2, path, path
        ));
        assert!(!borrowed_identity_fields_match(
            7, 8, 11, 11, 2, 2, path, path
        ));
        assert!(!borrowed_identity_fields_match(
            7, 7, 11, 12, 2, 2, path, path
        ));
        assert!(!borrowed_identity_fields_match(
            7, 7, 11, 11, 2, 3, path, path
        ));
        assert!(!borrowed_identity_fields_match(
            7,
            7,
            11,
            11,
            2,
            2,
            path,
            Path::new(r"C:\Temp\Signal.exe")
        ));
    }

    #[test]
    fn native_binding_digest_changes_with_every_window_identity_dimension() {
        let baseline = native_binding_identity_digest(7, 11, 2, 17, b"signal-path");
        assert_ne!(baseline, [0; 32]);
        assert_ne!(
            baseline,
            native_binding_identity_digest(8, 11, 2, 17, b"signal-path")
        );
        assert_ne!(
            baseline,
            native_binding_identity_digest(7, 12, 2, 17, b"signal-path")
        );
        assert_ne!(
            baseline,
            native_binding_identity_digest(7, 11, 3, 17, b"signal-path")
        );
        assert_ne!(
            baseline,
            native_binding_identity_digest(7, 11, 2, 18, b"signal-path")
        );
        assert_ne!(
            baseline,
            native_binding_identity_digest(7, 11, 2, 17, b"other-path")
        );
    }

    #[test]
    fn every_result_denies_capture_protection() {
        let success = NativeWindowHostResult::existing(
            NativeAppId::Signal,
            NativeWindowHostStatus::ExistingSession,
        );
        let failure = NativeWindowHostResult::failed(
            NativeAppId::Signal,
            NativeWindowHostReason::ExistingSessionUnavailable,
        );
        assert!(!success.capture_protected);
        assert!(!failure.capture_protected);
        assert_eq!(success.mode, "existingNativeCompanion");
    }

    #[test]
    fn tether_stops_on_target_or_parent_identity_drift() {
        assert!(tether_should_continue(true, true));
        assert!(!tether_should_continue(false, true));
        assert!(!tether_should_continue(true, false));
        assert!(!tether_should_continue(false, false));
    }

    #[test]
    fn guardian_restores_only_after_exact_parent_death_and_exact_target_match() {
        assert!(!guardian_should_restore(true, true));
        assert!(!guardian_should_restore(true, false));
        assert!(!guardian_should_restore(false, false));
        assert!(guardian_should_restore(false, true));
    }

    #[test]
    fn tether_avoids_signature_work_but_guardian_rechecks_before_restore() {
        let source = include_str!("native_window_host.rs");
        let snapshot_start = source
            .find("unsafe fn snapshot_identity_is_valid")
            .expect("snapshot validator exists");
        let snapshot_end = source[snapshot_start..]
            .find("unsafe fn window_identity_is_exact")
            .map(|offset| snapshot_start + offset)
            .expect("snapshot validator boundary exists");
        assert!(!source[snapshot_start..snapshot_end].contains("verify_executable"));

        let guardian_start = source
            .find("pub(super) fn run_guardian_if_requested")
            .expect("guardian runner exists");
        let guardian_end = source[guardian_start..]
            .find("fn signal_executable")
            .map(|offset| guardian_start + offset)
            .expect("guardian runner boundary exists");
        let guardian = &source[guardian_start..guardian_end];
        let verify = guardian
            .find("verify_executable")
            .expect("guardian publisher check exists");
        let restore = guardian
            .find("restore_placement")
            .expect("guardian restore exists");
        assert!(verify < restore);
    }

    #[test]
    fn guardian_mode_is_not_available_off_windows() {
        #[cfg(not(target_os = "windows"))]
        assert!(!run_signal_guardian_if_requested());
    }

    #[test]
    fn desktop_entrypoint_dispatches_guardian_before_tauri_startup() {
        let source = include_str!("main.rs");
        let main = source.find("fn main() {").expect("desktop main exists");
        let guardian = source[main..]
            .find("run_signal_guardian_if_requested()")
            .expect("guardian dispatch exists");
        let tauri = source[main..]
            .find("tauri::Builder::default()")
            .expect("Tauri startup exists");
        assert!(guardian < tauri);
        let between = &source[main + guardian..main + tauri];
        assert!(between.contains("return;"));
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn off_windows_actions_are_explicitly_unsupported() {
        let state = NativeWindowHostState::default();
        for result in [
            state.host(NativeAppId::Signal, Path::new("/unused"), "owner", 1),
            state.resize(1),
            state.focus(),
            state.detach(),
        ] {
            assert_eq!(result.status, NativeWindowHostStatus::Unsupported);
            assert_eq!(result.reason, NativeWindowHostReason::PlatformUnsupported);
            assert!(!result.capture_protected);
        }
    }
}
