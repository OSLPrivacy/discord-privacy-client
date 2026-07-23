//! Consented, non-destructive hosting for the user's existing Mullvad window.
//!
//! Mullvad is device-wide and does not expose a reviewed isolated-profile
//! launch mode. This boundary therefore borrows only the one visible Mullvad
//! window belonging to the current Windows logon session. It never reads VPN
//! state, account data, UI text, process memory, or configuration, and it never
//! touches Mullvad's daemon or service. If the verified GUI survives with no
//! usable window after an ordinary relaunch, one bounded recovery terminates
//! only that exact locked GUI process tree and relaunches the same binary.
//! Mullvad 2026.2.0's official Program Files executable is not Authenticode
//! signed, so this module deliberately makes no publisher-trust claim. It
//! accepts only the canonical system Program Files location and
//! retains a restrictive file handle across discovery, recovery, and launch.
//! After a forced GUI-only recovery, one second launch activates Mullvad's
//! official Electron single-instance handler, whose only effect is to show the
//! existing GUI window. It does not invoke the daemon or alter GUI settings.
//! Dropping the state restores the original owner, styles, and placement.

use serde::Serialize;
#[cfg(any(target_os = "windows", test))]
use std::path::Path;

#[cfg(any(target_os = "windows", test))]
const PRESENTATION_ATTEMPTS: usize = 3;

#[cfg(any(target_os = "windows", test))]
fn usable_rect(rect: [i32; 4]) -> Option<[i32; 4]> {
    (rect[2] > rect[0] && rect[3] > rect[1]).then_some(rect)
}

#[cfg(any(target_os = "windows", test))]
fn saved_rect_for_window(
    actual: Option<[i32; 4]>,
    iconic: bool,
    normal: Option<[i32; 4]>,
) -> Option<[i32; 4]> {
    actual.and_then(usable_rect).or_else(|| {
        iconic
            .then_some(())
            .and_then(|_| normal.and_then(usable_rect))
    })
}

#[cfg(any(target_os = "windows", test))]
fn presentation_matches(visible: bool, iconic: bool, expected: [i32; 4], actual: [i32; 4]) -> bool {
    visible && !iconic && expected == actual
}

#[cfg(any(target_os = "windows", test))]
fn focus_matches(visible: bool, iconic: bool, _foreground: bool) -> bool {
    // Windows foreground arbitration is advisory: SetForegroundWindow can be
    // denied based on the user's most recent input even when the exact hosted
    // window was validly raised and focused inside its own thread. Truthful
    // success therefore requires presentation, not foreground ownership.
    visible && !iconic
}

#[cfg(any(target_os = "windows", test))]
fn hosted_style_invariants(style: isize, ex_style: isize) -> bool {
    const CHROME: isize = 0x00cf_0000;
    const POPUP: isize = 0x8000_0000u32 as isize;
    const CHILD: isize = 0x4000_0000;
    const APP_WINDOW: isize = 0x0004_0000;
    const TOOL_WINDOW: isize = 0x0000_0080;

    style & CHROME == 0
        && style & POPUP != 0
        && style & CHILD == 0
        && ex_style & APP_WINDOW == 0
        && ex_style & TOOL_WINDOW != 0
}

#[cfg(any(target_os = "windows", test))]
fn exact_gui_path_matches(expected_gui: &Path, actual: &Path) -> bool {
    expected_gui == actual
}

#[cfg(any(target_os = "windows", test))]
fn fixed_program_files_gui_path_matches(program_files: &Path, candidate: &Path) -> bool {
    candidate == program_files.join("Mullvad VPN").join("Mullvad VPN.exe")
}

#[cfg(any(target_os = "windows", test))]
fn exact_gui_tree_termination_order(
    processes: &[(u32, u32)],
) -> Result<Vec<u32>, MullvadWindowHostReason> {
    use std::collections::{HashMap, HashSet};

    if processes.is_empty() || processes.len() > 64 {
        return Err(MullvadWindowHostReason::ExistingSessionUnavailable);
    }
    let parents = processes.iter().copied().collect::<HashMap<_, _>>();
    if parents.len() != processes.len() || parents.contains_key(&0) {
        return Err(MullvadWindowHostReason::ExistingSessionAmbiguous);
    }
    let members = parents.keys().copied().collect::<HashSet<_>>();
    let roots = processes
        .iter()
        .filter(|(_, parent)| !members.contains(parent))
        .map(|(pid, _)| *pid)
        .collect::<Vec<_>>();
    if roots.len() != 1 {
        return Err(MullvadWindowHostReason::ExistingSessionAmbiguous);
    }
    let root = roots[0];
    let mut ordered = Vec::with_capacity(processes.len());
    for &(pid, _) in processes {
        let mut current = pid;
        let mut depth = 0usize;
        let mut seen = HashSet::new();
        while current != root {
            if !seen.insert(current) || depth >= processes.len() {
                return Err(MullvadWindowHostReason::ExistingSessionAmbiguous);
            }
            current = *parents
                .get(&current)
                .ok_or(MullvadWindowHostReason::ExistingSessionAmbiguous)?;
            depth += 1;
        }
        ordered.push((depth, pid));
    }
    ordered.sort_by(|left, right| right.cmp(left));
    Ok(ordered.into_iter().map(|(_, pid)| pid).collect())
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MullvadWindowHostStatus {
    Hosted,
    Resized,
    Focused,
    Restored,
    Unsupported,
    Failed,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MullvadWindowHostReason {
    None,
    PlatformUnsupported,
    AppNotInstalled,
    ExistingSessionUnavailable,
    ExistingSessionAmbiguous,
    WindowIdentityChanged,
    OwnerWindowUnavailable,
    GuiRecoveryRejected,
    WindowHostRejected,
    WindowOwnerRejected,
    WindowStyleRejected,
    WindowDpiRejected,
    WindowVisibilityRejected,
    WindowBoundsRejected,
    WindowSiblingRejected,
    WindowOperationRejected,
    NotHosted,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MullvadWindowHostResult {
    pub status: MullvadWindowHostStatus,
    pub reason: MullvadWindowHostReason,
    pub mode: &'static str,
    pub capture_protected: bool,
}

impl MullvadWindowHostResult {
    fn unavailable(status: MullvadWindowHostStatus, reason: MullvadWindowHostReason) -> Self {
        Self {
            status,
            reason,
            mode: "none",
            capture_protected: false,
        }
    }

    #[cfg(target_os = "windows")]
    fn success(status: MullvadWindowHostStatus) -> Self {
        Self {
            status,
            reason: MullvadWindowHostReason::None,
            mode: "existingMullvadSession",
            capture_protected: false,
        }
    }
}

#[derive(Debug, Default)]
pub struct MullvadWindowHostState {
    #[cfg(target_os = "windows")]
    inner: std::sync::Mutex<Option<windows::BorrowedMullvadWindow>>,
}

#[cfg(target_os = "windows")]
impl Drop for MullvadWindowHostState {
    fn drop(&mut self) {
        if let Ok(slot) = self.inner.get_mut() {
            if let Some(hosted) = slot.take() {
                let _ = unsafe { windows::restore_borrowed(hosted) };
            }
        }
    }
}

impl MullvadWindowHostState {
    pub fn host(&self, trusted_parent: isize) -> MullvadWindowHostResult {
        #[cfg(target_os = "windows")]
        {
            windows::host(self, trusted_parent)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = trusted_parent;
            MullvadWindowHostResult::unavailable(
                MullvadWindowHostStatus::Unsupported,
                MullvadWindowHostReason::PlatformUnsupported,
            )
        }
    }

    pub fn resize(&self, trusted_parent: isize) -> MullvadWindowHostResult {
        #[cfg(target_os = "windows")]
        {
            windows::resize(self, trusted_parent)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = trusted_parent;
            MullvadWindowHostResult::unavailable(
                MullvadWindowHostStatus::Unsupported,
                MullvadWindowHostReason::PlatformUnsupported,
            )
        }
    }

    pub fn focus(&self) -> MullvadWindowHostResult {
        #[cfg(target_os = "windows")]
        {
            windows::focus(self)
        }
        #[cfg(not(target_os = "windows"))]
        {
            MullvadWindowHostResult::unavailable(
                MullvadWindowHostStatus::Unsupported,
                MullvadWindowHostReason::PlatformUnsupported,
            )
        }
    }

    pub fn restore(&self) -> MullvadWindowHostResult {
        #[cfg(target_os = "windows")]
        {
            windows::restore(self)
        }
        #[cfg(not(target_os = "windows"))]
        {
            MullvadWindowHostResult::unavailable(
                MullvadWindowHostStatus::Unsupported,
                MullvadWindowHostReason::PlatformUnsupported,
            )
        }
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use std::collections::HashMap;
    use std::ffi::{c_void, OsString};
    use std::fs::File;
    use std::os::windows::ffi::OsStringExt;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};
    use windows_sys::Win32::Foundation::{
        GetLastError, SetLastError, BOOL, HWND, LPARAM, POINT, RECT,
    };
    use windows_sys::Win32::Graphics::Gdi::{
        ClientToScreen, RedrawWindow, RDW_ALLCHILDREN, RDW_FRAME, RDW_INVALIDATE, RDW_UPDATENOW,
    };
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::HiDpi::GetWindowDpiAwarenessContext;
    use windows_sys::Win32::UI::Shell::{
        FOLDERID_ProgramFiles, SHGetKnownFolderPath, KF_FLAG_DEFAULT,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, EnumWindows, GetAncestor, GetClientRect, GetForegroundWindow, GetParent,
        GetWindowLongPtrW, GetWindowPlacement, GetWindowRect, GetWindowThreadProcessId, IsIconic,
        IsWindowVisible, SetForegroundWindow, SetWindowLongPtrW, SetWindowPlacement, SetWindowPos,
        ShowWindow, GA_ROOT, GWLP_HWNDPARENT, GWL_EXSTYLE, GWL_STYLE, HWND_TOP, SWP_FRAMECHANGED,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SW_RESTORE,
        WINDOWPLACEMENT, WS_CAPTION, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW, WS_MAXIMIZEBOX,
        WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU, WS_THICKFRAME,
    };

    const ERROR_SUCCESS: u32 = 0;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const TRUSTED_VERTICAL_RESERVE: i32 = 98;
    const WINDOW_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(8);
    const GUI_TERMINATION_TIMEOUT: Duration = Duration::from_secs(3);
    const GUI_SINGLETON_START_TIMEOUT: Duration = Duration::from_secs(3);
    // The primary Electron process is visible to Toolhelp before its JS
    // single-instance handler is guaranteed to be installed. Keep this
    // bounded, but allow cold-start/Defender initialization to settle before
    // delivering the one activation launch.
    const GUI_SINGLETON_SETTLE_DELAY: Duration = Duration::from_secs(1);
    const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;
    const PROCESS_TERMINATE: u32 = 0x0001;
    const SYNCHRONIZE: u32 = 0x0010_0000;
    const WAIT_OBJECT_0: u32 = 0;
    const MAX_GUI_PROCESSES: usize = 64;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const MULLVAD_PRESENTATION_ATTEMPTS: usize = 7;
    const MULLVAD_PRESENTATION_SETTLE_DELAY: Duration = Duration::from_millis(500);

    type RawHandle = *mut c_void;

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct LockedFileIdentity {
        volume_serial: u32,
        file_index: u64,
        file_size: u64,
        last_write_time: u64,
    }

    impl LockedFileIdentity {
        fn from_file(file: &File) -> Option<Self> {
            use windows_sys::Win32::Storage::FileSystem::{
                GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
            };

            let mut information: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
            if unsafe { GetFileInformationByHandle(file.as_raw_handle().cast(), &mut information) }
                == 0
            {
                return None;
            }
            Some(Self {
                volume_serial: information.dwVolumeSerialNumber,
                file_index: u64::from(information.nFileIndexHigh) << 32
                    | u64::from(information.nFileIndexLow),
                file_size: u64::from(information.nFileSizeHigh) << 32
                    | u64::from(information.nFileSizeLow),
                last_write_time: u64::from(information.ftLastWriteTime.dwHighDateTime) << 32
                    | u64::from(information.ftLastWriteTime.dwLowDateTime),
            })
        }
    }

    #[derive(Debug)]
    struct LockedMullvadExecutable {
        path: PathBuf,
        identity: LockedFileIdentity,
        locked_file: File,
    }

    impl LockedMullvadExecutable {
        fn path(&self) -> &Path {
            &self.path
        }

        fn identity_is_stable(&self) -> bool {
            LockedFileIdentity::from_file(&self.locked_file) == Some(self.identity)
                && self.path.canonicalize().ok().as_deref() == Some(self.path.as_path())
        }
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit_handle: BOOL, process_id: u32) -> RawHandle;
        fn QueryFullProcessImageNameW(
            process: RawHandle,
            flags: u32,
            path: *mut u16,
            path_len: *mut u32,
        ) -> BOOL;
        fn GetProcessTimes(
            process: RawHandle,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> BOOL;
        fn ProcessIdToSessionId(process_id: u32, session_id: *mut u32) -> BOOL;
        fn GetCurrentProcessId() -> u32;
        fn CreateToolhelp32Snapshot(flags: u32, process_id: u32) -> RawHandle;
        fn Process32FirstW(snapshot: RawHandle, entry: *mut ProcessEntry32W) -> BOOL;
        fn Process32NextW(snapshot: RawHandle, entry: *mut ProcessEntry32W) -> BOOL;
        fn TerminateProcess(process: RawHandle, exit_code: u32) -> BOOL;
        fn WaitForSingleObject(handle: RawHandle, milliseconds: u32) -> u32;
        fn CloseHandle(handle: RawHandle) -> BOOL;
    }

    #[repr(C)]
    struct ProcessEntry32W {
        size: u32,
        usage: u32,
        process_id: u32,
        default_heap_id: usize,
        module_id: u32,
        threads: u32,
        parent_process_id: u32,
        priority_class_base: i32,
        flags: u32,
        executable_file: [u16; 260],
    }

    #[derive(Debug)]
    pub(super) struct BorrowedMullvadWindow {
        process_id: u32,
        process_creation: FileTime,
        executable: LockedMullvadExecutable,
        window: isize,
        trusted_parent: isize,
        owner_window: isize,
        previous_owner: isize,
        previous_style: isize,
        previous_ex_style: isize,
        previous_placement: SavedWindowPlacement,
        hosted_style: isize,
        hosted_ex_style: isize,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct SavedWindowPlacement {
        flags: u32,
        show_cmd: u32,
        min_position: [i32; 2],
        max_position: [i32; 2],
        normal_position: [i32; 4],
    }

    pub(super) fn host(state: &MullvadWindowHostState, parent: isize) -> MullvadWindowHostResult {
        if parent == 0 {
            return failed(MullvadWindowHostReason::OwnerWindowUnavailable);
        }
        let executable = match fixed_mullvad_executable() {
            Some(executable) => executable,
            None => return failed(MullvadWindowHostReason::AppNotInstalled),
        };
        let mut guard = match state.inner.lock() {
            Ok(guard) => guard,
            Err(_) => return failed(MullvadWindowHostReason::WindowOperationRejected),
        };
        if let Some(hosted) = guard.as_ref() {
            if borrowed_is_valid(hosted)
                && hosted.trusted_parent == parent
                && unsafe { present_and_verify(hosted, parent as HWND).is_ok() }
            {
                return MullvadWindowHostResult::success(MullvadWindowHostStatus::Hosted);
            }
        }
        if let Some(stale) = guard.take() {
            if let Err(reason) = unsafe { restore_borrowed(stale) } {
                return failed(reason);
            }
        }
        let existing = match find_single_window(executable.path()) {
            Ok(found) => found,
            Err(reason) => return failed(reason),
        };
        let (window, process_id, process_creation) = match existing {
            Some(found) => found,
            None => {
                if launch_exact_gui(&executable).is_err() {
                    return failed(MullvadWindowHostReason::ExistingSessionUnavailable);
                }
                match wait_for_single_window(executable.path()) {
                    Ok(found) => found,
                    Err(MullvadWindowHostReason::ExistingSessionUnavailable) => {
                        if let Err(reason) = restart_hidden_gui(&executable) {
                            return failed(reason);
                        }
                        match wait_for_single_window(executable.path()) {
                            Ok(found) => found,
                            Err(reason) => return failed(reason),
                        }
                    }
                    Err(reason) => return failed(reason),
                }
            }
        };
        let hosted = match unsafe {
            borrow_borderless(
                executable,
                window,
                process_id,
                process_creation,
                parent as HWND,
            )
        } {
            Ok(hosted) => hosted,
            Err(MullvadWindowHostReason::WindowOperationRejected) => {
                return failed(MullvadWindowHostReason::WindowHostRejected)
            }
            Err(reason) => return failed(reason),
        };
        *guard = Some(hosted);
        MullvadWindowHostResult::success(MullvadWindowHostStatus::Hosted)
    }

    pub(super) fn resize(state: &MullvadWindowHostState, parent: isize) -> MullvadWindowHostResult {
        let mut guard = match state.inner.lock() {
            Ok(guard) => guard,
            Err(_) => return failed(MullvadWindowHostReason::WindowOperationRejected),
        };
        let Some(hosted) = guard.as_ref() else {
            return failed(MullvadWindowHostReason::NotHosted);
        };
        if parent == 0 {
            return failed(MullvadWindowHostReason::OwnerWindowUnavailable);
        }
        if !borrowed_is_valid(hosted) || hosted.trusted_parent != parent {
            let stale = guard.take().expect("checked Mullvad host remains present");
            let _ = unsafe { restore_borrowed(stale) };
            return failed(MullvadWindowHostReason::WindowIdentityChanged);
        }
        match unsafe { present_and_verify(hosted, parent as HWND) } {
            Ok(()) => MullvadWindowHostResult::success(MullvadWindowHostStatus::Resized),
            // Electron can briefly reassert its presentation while reacting
            // to an owner resize. Preserve the verified borrowed state so a
            // bounded retry can repair it; only identity/owner mismatches
            // above are destructive.
            Err(reason) => failed(reason),
        }
    }

    pub(super) fn focus(state: &MullvadWindowHostState) -> MullvadWindowHostResult {
        let mut guard = match state.inner.lock() {
            Ok(guard) => guard,
            Err(_) => return failed(MullvadWindowHostReason::WindowOperationRejected),
        };
        let Some(hosted) = guard.as_ref() else {
            return failed(MullvadWindowHostReason::NotHosted);
        };
        if !borrowed_is_valid(hosted) {
            let stale = guard.take().expect("checked Mullvad host remains present");
            let _ = unsafe { restore_borrowed(stale) };
            return failed(MullvadWindowHostReason::WindowIdentityChanged);
        }
        if let Err(reason) = unsafe { focus_and_verify(hosted) } {
            // Foreground activation and Electron presentation updates can be
            // transient. Keep the already identity-checked borrowed window so
            // the caller can realign and retry instead of turning one rejected
            // focus request into a permanently lost host.
            return failed(reason);
        }
        MullvadWindowHostResult::success(MullvadWindowHostStatus::Focused)
    }

    pub(super) fn restore(state: &MullvadWindowHostState) -> MullvadWindowHostResult {
        let mut guard = match state.inner.lock() {
            Ok(guard) => guard,
            Err(_) => return failed(MullvadWindowHostReason::WindowOperationRejected),
        };
        let Some(hosted) = guard.take() else {
            return failed(MullvadWindowHostReason::NotHosted);
        };
        match unsafe { restore_borrowed(hosted) } {
            Ok(()) => MullvadWindowHostResult::success(MullvadWindowHostStatus::Restored),
            Err(reason) => failed(reason),
        }
    }

    fn failed(reason: MullvadWindowHostReason) -> MullvadWindowHostResult {
        MullvadWindowHostResult::unavailable(MullvadWindowHostStatus::Failed, reason)
    }

    fn fixed_mullvad_executable() -> Option<LockedMullvadExecutable> {
        let program_files =
            known_folder(&FOLDERID_ProgramFiles).and_then(|path| path.canonicalize().ok())?;
        let candidate = program_files
            .join("Mullvad VPN")
            .join("Mullvad VPN.exe")
            .canonicalize()
            .ok()?;
        if !fixed_program_files_gui_path_matches(&program_files, &candidate) {
            return None;
        }
        let locked_file = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(&candidate)
            .ok()?;
        if !locked_file.metadata().ok()?.is_file() {
            return None;
        }
        let identity = LockedFileIdentity::from_file(&locked_file)?;
        let executable = LockedMullvadExecutable {
            path: candidate,
            identity,
            locked_file,
        };
        executable.identity_is_stable().then_some(executable)
    }

    fn known_folder(id: *const windows_sys::core::GUID) -> Option<PathBuf> {
        let mut raw = std::ptr::null_mut();
        let result = unsafe {
            SHGetKnownFolderPath(id, KF_FLAG_DEFAULT as u32, std::ptr::null_mut(), &mut raw)
        };
        if result != 0 || raw.is_null() {
            return None;
        }
        let mut length = 0usize;
        unsafe {
            while *raw.add(length) != 0 {
                length += 1;
            }
        }
        let value = PathBuf::from(OsString::from_wide(unsafe {
            std::slice::from_raw_parts(raw, length)
        }));
        unsafe { CoTaskMemFree(raw.cast()) };
        Some(value)
    }

    fn launch_exact_gui(executable: &LockedMullvadExecutable) -> std::io::Result<()> {
        if !executable.identity_is_stable() {
            return Err(std::io::Error::other("Mullvad executable identity changed"));
        }
        let working_directory = executable
            .path()
            .parent()
            .ok_or_else(|| std::io::Error::other("Mullvad install directory is unavailable"))?;
        Command::new(executable.path())
            .current_dir(working_directory)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
            .spawn()
            .map(|_| ())
    }

    #[derive(Clone, Copy)]
    struct ExactGuiProcess {
        process_id: u32,
        parent_process_id: u32,
        creation: FileTime,
    }

    fn exact_gui_processes(
        expected_path: &Path,
    ) -> Result<Vec<ExactGuiProcess>, MullvadWindowHostReason> {
        let current_session = process_session(unsafe { GetCurrentProcessId() })
            .ok_or(MullvadWindowHostReason::WindowIdentityChanged)?;
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snapshot as isize == -1 {
            return Err(MullvadWindowHostReason::WindowOperationRejected);
        }
        let mut entry: ProcessEntry32W = unsafe { std::mem::zeroed() };
        entry.size = std::mem::size_of::<ProcessEntry32W>() as u32;
        let mut found = Vec::new();
        let mut more = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
        while more {
            let pid = entry.process_id;
            if pid != 0 && process_session(pid) == Some(current_session) {
                if let Some((path, creation)) = process_identity(pid) {
                    if exact_gui_path_matches(expected_path, &path) {
                        found.push(ExactGuiProcess {
                            process_id: pid,
                            parent_process_id: entry.parent_process_id,
                            creation,
                        });
                        if found.len() > MAX_GUI_PROCESSES {
                            unsafe { CloseHandle(snapshot) };
                            return Err(MullvadWindowHostReason::ExistingSessionAmbiguous);
                        }
                    }
                }
            }
            more = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
        }
        unsafe { CloseHandle(snapshot) };
        Ok(found)
    }

    fn restart_hidden_gui(
        executable: &LockedMullvadExecutable,
    ) -> Result<(), MullvadWindowHostReason> {
        if !executable.identity_is_stable() {
            return Err(MullvadWindowHostReason::WindowIdentityChanged);
        }
        match find_single_window(executable.path())? {
            // The window can appear immediately after the caller's discovery
            // timeout. Treat that as successful recovery and never terminate
            // a GUI that has become usable in the meantime.
            Some(_) => return Ok(()),
            None => {}
        }
        let processes = exact_gui_processes(executable.path()).map_err(recovery_stage_reason)?;
        let tree = exact_gui_tree_termination_order(
            &processes
                .iter()
                .map(|process| (process.process_id, process.parent_process_id))
                .collect::<Vec<_>>(),
        )
        .map_err(recovery_stage_reason)?;
        let by_pid = processes
            .iter()
            .map(|process| (process.process_id, *process))
            .collect::<HashMap<_, _>>();
        let current_session = process_session(unsafe { GetCurrentProcessId() })
            .ok_or(MullvadWindowHostReason::WindowIdentityChanged)?;
        let deadline = Instant::now() + GUI_TERMINATION_TIMEOUT;
        for pid in tree {
            let expected = by_pid
                .get(&pid)
                .ok_or(MullvadWindowHostReason::WindowIdentityChanged)?;
            let process = unsafe {
                OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE | SYNCHRONIZE,
                    0,
                    pid,
                )
            };
            if process.is_null() {
                if process_identity(pid).is_none() {
                    continue;
                }
                return Err(MullvadWindowHostReason::WindowIdentityChanged);
            }
            let identity = process_identity_from_handle(process, pid);
            let valid = identity.is_some_and(|(path, creation, session)| {
                exact_gui_path_matches(executable.path(), &path)
                    && creation == expected.creation
                    && session == current_session
            });
            if !valid {
                unsafe { CloseHandle(process) };
                return Err(MullvadWindowHostReason::WindowIdentityChanged);
            }
            let already_exited = unsafe { WaitForSingleObject(process, 0) } == WAIT_OBJECT_0;
            if !already_exited && unsafe { TerminateProcess(process, 0) } == 0 {
                unsafe { CloseHandle(process) };
                return Err(MullvadWindowHostReason::GuiRecoveryRejected);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            let wait_ms = remaining.as_millis().min(u128::from(u32::MAX)) as u32;
            let stopped = unsafe { WaitForSingleObject(process, wait_ms) } == WAIT_OBJECT_0;
            unsafe { CloseHandle(process) };
            if !stopped {
                return Err(MullvadWindowHostReason::GuiRecoveryRejected);
            }
        }
        if !exact_gui_processes(executable.path())
            .map_err(recovery_stage_reason)?
            .is_empty()
        {
            return Err(MullvadWindowHostReason::WindowIdentityChanged);
        }
        launch_exact_gui(executable).map_err(|_| MullvadWindowHostReason::GuiRecoveryRejected)?;
        wait_for_gui_singleton(executable)?;
        if find_single_window(executable.path())?.is_none() {
            // Mullvad's official Electron entry point handles this as a
            // second-instance activation and calls showWindow() on the first
            // process. The first post-recovery launch can legitimately honor
            // the user's start-minimized preference.
            launch_exact_gui(executable)
                .map_err(|_| MullvadWindowHostReason::GuiRecoveryRejected)?;
        }
        Ok(())
    }

    fn recovery_stage_reason(reason: MullvadWindowHostReason) -> MullvadWindowHostReason {
        match reason {
            MullvadWindowHostReason::ExistingSessionAmbiguous
            | MullvadWindowHostReason::WindowIdentityChanged => reason,
            _ => MullvadWindowHostReason::GuiRecoveryRejected,
        }
    }

    fn wait_for_gui_singleton(
        executable: &LockedMullvadExecutable,
    ) -> Result<(), MullvadWindowHostReason> {
        let deadline = Instant::now() + GUI_SINGLETON_START_TIMEOUT;
        loop {
            if !executable.identity_is_stable() {
                return Err(MullvadWindowHostReason::WindowIdentityChanged);
            }
            let processes =
                exact_gui_processes(executable.path()).map_err(recovery_stage_reason)?;
            if !processes.is_empty() {
                exact_gui_tree_termination_order(
                    &processes
                        .iter()
                        .map(|process| (process.process_id, process.parent_process_id))
                        .collect::<Vec<_>>(),
                )
                .map_err(recovery_stage_reason)?;
                thread::sleep(GUI_SINGLETON_SETTLE_DELAY);
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(MullvadWindowHostReason::GuiRecoveryRejected);
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    fn wait_for_single_window(
        executable: &Path,
    ) -> Result<(HWND, u32, FileTime), MullvadWindowHostReason> {
        let deadline = Instant::now() + WINDOW_DISCOVERY_TIMEOUT;
        loop {
            match find_single_window(executable) {
                Ok(Some(found)) => return Ok(found),
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(100)),
                Ok(None) => return Err(MullvadWindowHostReason::ExistingSessionUnavailable),
                Err(reason) => return Err(reason),
            }
        }
    }

    struct WindowSearch<'a> {
        expected_path: &'a Path,
        current_session: u32,
        count: usize,
        found: Option<(HWND, u32, FileTime)>,
    }

    fn find_single_window(
        executable: &Path,
    ) -> Result<Option<(HWND, u32, FileTime)>, MullvadWindowHostReason> {
        let current_session = process_session(unsafe { GetCurrentProcessId() })
            .ok_or(MullvadWindowHostReason::WindowIdentityChanged)?;
        let mut search = WindowSearch {
            expected_path: executable,
            current_session,
            count: 0,
            found: None,
        };
        unsafe {
            EnumWindows(
                Some(enum_window),
                (&mut search as *mut WindowSearch<'_>) as LPARAM,
            )
        };
        match search.count {
            0 => Ok(None),
            1 => Ok(search.found),
            _ => Err(MullvadWindowHostReason::ExistingSessionAmbiguous),
        }
    }

    unsafe extern "system" fn enum_window(window: HWND, parameter: LPARAM) -> BOOL {
        let search = &mut *(parameter as *mut WindowSearch<'_>);
        if IsWindowVisible(window) == 0 {
            return 1;
        }
        if saved_window_rect(window, IsIconic(window) != 0).is_none() {
            return 1;
        }
        let Some(pid) = window_process_id(window) else {
            return 1;
        };
        if process_session(pid) != Some(search.current_session) {
            return 1;
        }
        let Some((path, creation)) = process_identity(pid) else {
            return 1;
        };
        if path != search.expected_path {
            return 1;
        }
        search.count += 1;
        if search.count == 1 {
            search.found = Some((window, pid, creation));
        }
        1
    }

    struct VisibleSiblingSearch<'a> {
        expected_path: &'a Path,
        target: HWND,
        current_session: u32,
        found: bool,
    }

    unsafe fn no_visible_gui_sibling(expected_path: &Path, target: HWND) -> bool {
        let Some(current_session) = process_session(GetCurrentProcessId()) else {
            return false;
        };
        let mut search = VisibleSiblingSearch {
            expected_path,
            target,
            current_session,
            found: false,
        };
        EnumWindows(
            Some(enum_visible_sibling),
            (&mut search as *mut VisibleSiblingSearch<'_>) as LPARAM,
        );
        !search.found
    }

    unsafe extern "system" fn enum_visible_sibling(window: HWND, parameter: LPARAM) -> BOOL {
        let search = &mut *(parameter as *mut VisibleSiblingSearch<'_>);
        if window == search.target || IsWindowVisible(window) == 0 || IsIconic(window) != 0 {
            return 1;
        }
        let Some(pid) = window_process_id(window) else {
            return 1;
        };
        if process_session(pid) != Some(search.current_session) {
            return 1;
        }
        if process_identity(pid)
            .is_some_and(|(path, _)| exact_gui_path_matches(search.expected_path, &path))
        {
            search.found = true;
            return 0;
        }
        1
    }

    fn process_session(pid: u32) -> Option<u32> {
        let mut session = 0u32;
        (unsafe { ProcessIdToSessionId(pid, &mut session) } != 0).then_some(session)
    }

    fn process_identity(pid: u32) -> Option<(PathBuf, FileTime)> {
        if pid == 0 {
            return None;
        }
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if process.is_null() {
            return None;
        }
        let result = process_identity_from_handle(process, pid)
            .map(|(path, creation, _session)| (path, creation));
        unsafe { CloseHandle(process) };
        result
    }

    fn process_identity_from_handle(
        process: RawHandle,
        pid: u32,
    ) -> Option<(PathBuf, FileTime, u32)> {
        (|| {
            let mut path = vec![0u16; 32_768];
            let mut length = path.len() as u32;
            if unsafe { QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut length) }
                == 0
                || length == 0
            {
                return None;
            }
            path.truncate(length as usize);
            let path = PathBuf::from(OsString::from_wide(&path))
                .canonicalize()
                .ok()?;
            let mut creation = FileTime::default();
            let mut exit = FileTime::default();
            let mut kernel = FileTime::default();
            let mut user = FileTime::default();
            if unsafe { GetProcessTimes(process, &mut creation, &mut exit, &mut kernel, &mut user) }
                == 0
            {
                return None;
            }
            let session = process_session(pid)?;
            Some((path, creation, session))
        })()
    }

    unsafe fn window_process_id(window: HWND) -> Option<u32> {
        let mut pid = 0u32;
        (!window.is_null() && GetWindowThreadProcessId(window, &mut pid) != 0 && pid != 0)
            .then_some(pid)
    }

    fn borrowed_is_valid(hosted: &BorrowedMullvadWindow) -> bool {
        hosted.executable.identity_is_stable()
            && (unsafe { window_process_id(hosted.window as HWND) }) == Some(hosted.process_id)
            && process_identity(hosted.process_id).is_some_and(|(path, creation)| {
                exact_gui_path_matches(hosted.executable.path(), &path)
                    && creation == hosted.process_creation
            })
    }

    unsafe fn borrow_borderless(
        executable: LockedMullvadExecutable,
        window: HWND,
        process_id: u32,
        process_creation: FileTime,
        parent: HWND,
    ) -> Result<BorrowedMullvadWindow, MullvadWindowHostReason> {
        if !executable.identity_is_stable() {
            return Err(MullvadWindowHostReason::WindowIdentityChanged);
        }
        if !trusted_parent_window(parent) {
            return Err(MullvadWindowHostReason::OwnerWindowUnavailable);
        }
        // Use the verified, durable OSL top-level window directly as owner.
        // A proxy created on this blocking worker thread would be destroyed
        // when Windows tears down the worker's window queue.
        let owner = parent;
        let previous_owner = GetWindowLongPtrW(window, GWLP_HWNDPARENT);
        let previous_style = GetWindowLongPtrW(window, GWL_STYLE);
        let previous_ex_style = GetWindowLongPtrW(window, GWL_EXSTYLE);
        let previous_iconic = IsIconic(window) != 0;
        let Some(previous_placement) = capture_window_placement(window) else {
            return Err(MullvadWindowHostReason::WindowOperationRejected);
        };
        if saved_window_rect(window, previous_iconic).is_none() {
            return Err(MullvadWindowHostReason::WindowOperationRejected);
        }
        SetLastError(ERROR_SUCCESS);
        SetWindowLongPtrW(window, GWLP_HWNDPARENT, owner as isize);
        if GetLastError() != ERROR_SUCCESS {
            return Err(MullvadWindowHostReason::WindowOperationRejected);
        }
        let chrome =
            (WS_CAPTION | WS_THICKFRAME | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX) as isize;
        let hosted_style = (previous_style & !chrome) | WS_POPUP as isize;
        let hosted_ex_style =
            (previous_ex_style & !(WS_EX_APPWINDOW as isize)) | WS_EX_TOOLWINDOW as isize;
        SetWindowLongPtrW(window, GWL_STYLE, hosted_style);
        SetWindowLongPtrW(window, GWL_EXSTYLE, hosted_ex_style);
        let provisional = BorrowedMullvadWindow {
            process_id,
            process_creation,
            executable,
            window: window as isize,
            trusted_parent: parent as isize,
            owner_window: owner as isize,
            previous_owner,
            previous_style,
            previous_ex_style,
            previous_placement,
            hosted_style,
            hosted_ex_style,
        };
        if let Err(presentation_reason) = present_and_verify(&provisional, parent) {
            let restore_reason = restore_borrowed(provisional).err();
            return Err(restore_reason.unwrap_or(presentation_reason));
        }
        if !borrowed_is_valid(&provisional) {
            let _ = restore_borrowed(provisional);
            return Err(MullvadWindowHostReason::WindowIdentityChanged);
        }
        Ok(provisional)
    }

    fn rect_array(rect: RECT) -> [i32; 4] {
        [rect.left, rect.top, rect.right, rect.bottom]
    }

    unsafe fn capture_window_placement(window: HWND) -> Option<SavedWindowPlacement> {
        let mut placement: WINDOWPLACEMENT = std::mem::zeroed();
        placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
        (GetWindowPlacement(window, &mut placement) != 0).then_some(SavedWindowPlacement {
            flags: placement.flags,
            show_cmd: placement.showCmd,
            min_position: [placement.ptMinPosition.x, placement.ptMinPosition.y],
            max_position: [placement.ptMaxPosition.x, placement.ptMaxPosition.y],
            normal_position: rect_array(placement.rcNormalPosition),
        })
    }

    unsafe fn saved_window_rect(window: HWND, iconic: bool) -> Option<RECT> {
        let mut actual: RECT = std::mem::zeroed();
        let actual = (GetWindowRect(window, &mut actual) != 0).then_some(rect_array(actual));
        let normal = capture_window_placement(window).map(|placement| placement.normal_position);
        saved_rect_for_window(actual, iconic, normal).map(|[left, top, right, bottom]| RECT {
            left,
            top,
            right,
            bottom,
        })
    }

    unsafe fn trusted_parent_window(parent: HWND) -> bool {
        parent != std::ptr::null_mut()
            && window_process_id(parent) == Some(GetCurrentProcessId())
            && GetParent(parent).is_null()
            && GetAncestor(parent, GA_ROOT) == parent
    }

    unsafe fn parent_target_rect(parent: HWND) -> Option<RECT> {
        if !trusted_parent_window(parent) {
            return None;
        }
        let mut client = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if GetClientRect(parent, &mut client) == 0 {
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
            right: origin.x + (client.right - client.left).max(1),
            bottom: origin.y + (client.bottom - TRUSTED_VERTICAL_RESERVE).max(1),
        })
    }

    unsafe fn presentation_failure(
        hosted: &BorrowedMullvadWindow,
        parent: HWND,
    ) -> Option<MullvadWindowHostReason> {
        let Some(expected) = parent_target_rect(parent) else {
            return Some(MullvadWindowHostReason::WindowBoundsRejected);
        };
        let window = hosted.window as HWND;
        if !borrowed_is_valid(hosted) {
            return Some(MullvadWindowHostReason::WindowIdentityChanged);
        }
        let owner = hosted.owner_window as HWND;
        if hosted.trusted_parent != parent as isize
            || owner.is_null()
            || window_process_id(owner) != Some(GetCurrentProcessId())
            || owner != parent
            || GetWindowLongPtrW(window, GWLP_HWNDPARENT) != hosted.owner_window
            || GetAncestor(window, GA_ROOT) != window
        {
            return Some(MullvadWindowHostReason::WindowOwnerRejected);
        }
        if GetWindowDpiAwarenessContext(window).is_null() {
            return Some(MullvadWindowHostReason::WindowDpiRejected);
        }
        if !hosted_style_invariants(
            GetWindowLongPtrW(window, GWL_STYLE),
            GetWindowLongPtrW(window, GWL_EXSTYLE),
        ) {
            return Some(MullvadWindowHostReason::WindowStyleRejected);
        }
        if IsWindowVisible(window) == 0 || IsIconic(window) != 0 {
            return Some(MullvadWindowHostReason::WindowVisibilityRejected);
        }
        let mut actual: RECT = std::mem::zeroed();
        if GetWindowRect(window, &mut actual) == 0
            || !presentation_matches(
                IsWindowVisible(window) != 0,
                IsIconic(window) != 0,
                rect_array(expected),
                rect_array(actual),
            )
        {
            return Some(MullvadWindowHostReason::WindowBoundsRejected);
        }
        if !no_visible_gui_sibling(hosted.executable.path(), window) {
            return Some(MullvadWindowHostReason::WindowSiblingRejected);
        }
        None
    }

    unsafe fn present_and_verify(
        hosted: &BorrowedMullvadWindow,
        parent: HWND,
    ) -> Result<(), MullvadWindowHostReason> {
        let Some(expected) = parent_target_rect(parent) else {
            return Err(MullvadWindowHostReason::WindowBoundsRejected);
        };
        let window = hosted.window as HWND;
        let mut last_reason = MullvadWindowHostReason::WindowOperationRejected;
        for attempt in 0..MULLVAD_PRESENTATION_ATTEMPTS {
            ShowWindow(window, SW_RESTORE);
            if GetWindowLongPtrW(window, GWLP_HWNDPARENT) != hosted.owner_window {
                SetWindowLongPtrW(window, GWLP_HWNDPARENT, hosted.owner_window);
                if GetWindowLongPtrW(window, GWLP_HWNDPARENT) != hosted.owner_window {
                    last_reason = MullvadWindowHostReason::WindowOwnerRejected;
                    continue;
                }
            }
            let current_style = GetWindowLongPtrW(window, GWL_STYLE);
            let current_ex_style = GetWindowLongPtrW(window, GWL_EXSTYLE);
            if !hosted_style_invariants(current_style, current_ex_style) {
                SetWindowLongPtrW(window, GWL_STYLE, hosted.hosted_style);
                SetWindowLongPtrW(window, GWL_EXSTYLE, hosted.hosted_ex_style);
                if !hosted_style_invariants(
                    GetWindowLongPtrW(window, GWL_STYLE),
                    GetWindowLongPtrW(window, GWL_EXSTYLE),
                ) {
                    last_reason = MullvadWindowHostReason::WindowStyleRejected;
                    continue;
                }
            }
            if SetWindowPos(
                window,
                HWND_TOP,
                expected.left,
                expected.top,
                expected.right - expected.left,
                expected.bottom - expected.top,
                SWP_FRAMECHANGED | SWP_SHOWWINDOW,
            ) == 0
            {
                last_reason = MullvadWindowHostReason::WindowBoundsRejected;
            } else if let Some(reason) = presentation_failure(hosted, parent) {
                last_reason = reason;
            } else {
                return Ok(());
            }
            if attempt + 1 < MULLVAD_PRESENTATION_ATTEMPTS {
                thread::sleep(MULLVAD_PRESENTATION_SETTLE_DELAY);
            }
        }
        Err(last_reason)
    }

    unsafe fn focus_and_verify(
        hosted: &BorrowedMullvadWindow,
    ) -> Result<(), MullvadWindowHostReason> {
        let window = hosted.window as HWND;
        let mut last_reason = MullvadWindowHostReason::WindowOperationRejected;
        for attempt in 0..MULLVAD_PRESENTATION_ATTEMPTS {
            if let Err(reason) = present_and_verify(hosted, hosted.trusted_parent as HWND) {
                last_reason = reason;
                if attempt + 1 < MULLVAD_PRESENTATION_ATTEMPTS {
                    thread::sleep(MULLVAD_PRESENTATION_SETTLE_DELAY);
                }
                continue;
            }
            // Keep OSL as the foreground top-level window. Forcing the
            // foreign Electron window itself foreground/focus makes Mullvad's
            // single-instance UI hide back to its tray on some builds. The
            // owned borderless surface remains visible and directly
            // interactive when OSL is foreground and the surface is raised.
            let parent = hosted.trusted_parent as HWND;
            let _ = SetForegroundWindow(parent);
            ShowWindow(window, SW_RESTORE);
            let _ = BringWindowToTop(window);
            if focus_matches(
                IsWindowVisible(window) != 0,
                IsIconic(window) != 0,
                matches!(GetForegroundWindow(), foreground if foreground == window || foreground == parent),
            ) {
                return Ok(());
            }
            if attempt + 1 < MULLVAD_PRESENTATION_ATTEMPTS {
                thread::sleep(MULLVAD_PRESENTATION_SETTLE_DELAY);
            }
        }
        Err(last_reason)
    }

    unsafe fn apply_window_placement(window: HWND, saved: SavedWindowPlacement) -> bool {
        let [left, top, right, bottom] = saved.normal_position;
        let mut placement: WINDOWPLACEMENT = std::mem::zeroed();
        placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
        placement.flags = saved.flags;
        placement.showCmd = saved.show_cmd;
        placement.ptMinPosition = POINT {
            x: saved.min_position[0],
            y: saved.min_position[1],
        };
        placement.ptMaxPosition = POINT {
            x: saved.max_position[0],
            y: saved.max_position[1],
        };
        placement.rcNormalPosition = RECT {
            left,
            top,
            right,
            bottom,
        };
        SetWindowPlacement(window, &placement) != 0
            && SetWindowPos(
                window,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
            ) != 0
    }

    unsafe fn restored_presentation_is_verified(
        hosted: &BorrowedMullvadWindow,
        window: HWND,
    ) -> bool {
        GetWindowLongPtrW(window, GWLP_HWNDPARENT) == hosted.previous_owner
            && GetWindowLongPtrW(window, GWL_STYLE) == hosted.previous_style
            && GetWindowLongPtrW(window, GWL_EXSTYLE) == hosted.previous_ex_style
            && capture_window_placement(window) == Some(hosted.previous_placement)
    }

    pub(super) unsafe fn restore_borrowed(
        hosted: BorrowedMullvadWindow,
    ) -> Result<(), MullvadWindowHostReason> {
        let window = hosted.window as HWND;
        if !borrowed_is_valid(&hosted) {
            return Err(MullvadWindowHostReason::WindowIdentityChanged);
        }
        let mut restored = false;
        for attempt in 0..PRESENTATION_ATTEMPTS {
            SetWindowLongPtrW(window, GWLP_HWNDPARENT, hosted.previous_owner);
            SetWindowLongPtrW(window, GWL_STYLE, hosted.previous_style);
            SetWindowLongPtrW(window, GWL_EXSTYLE, hosted.previous_ex_style);
            let placement_applied = apply_window_placement(window, hosted.previous_placement);
            if placement_applied && restored_presentation_is_verified(&hosted, window) {
                // Electron's map canvas can retain the hosted surface after
                // its owner/style transition. Force one bounded native repaint
                // after the exact original placement has been restored.
                let _ = RedrawWindow(
                    window,
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    RDW_INVALIDATE | RDW_FRAME | RDW_ALLCHILDREN | RDW_UPDATENOW,
                );
                restored = true;
                break;
            }
            if attempt + 1 < PRESENTATION_ATTEMPTS {
                thread::sleep(Duration::from_millis(100));
            }
        }
        restored
            .then_some(())
            .ok_or(MullvadWindowHostReason::WindowOperationRejected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_platform_fails_closed_without_claiming_capture_protection() {
        if cfg!(target_os = "windows") {
            return;
        }
        let result = MullvadWindowHostState::default().host(0);
        assert_eq!(result.status, MullvadWindowHostStatus::Unsupported);
        assert_eq!(result.reason, MullvadWindowHostReason::PlatformUnsupported);
        assert_eq!(result.mode, "none");
        assert!(!result.capture_protected);
    }

    #[test]
    fn minimized_empty_window_uses_saved_normal_placement_only() {
        let saved = [100, 120, 900, 720];
        assert_eq!(saved_rect_for_window(None, true, Some(saved)), Some(saved));
        assert_eq!(
            saved_rect_for_window(Some([0, 0, 0, 0]), true, Some(saved)),
            Some(saved)
        );
        assert_eq!(saved_rect_for_window(None, false, Some(saved)), None);
        assert_eq!(saved_rect_for_window(None, true, Some([0, 0, 0, 0])), None);
        assert_eq!(
            saved_rect_for_window(Some([10, 20, 810, 620]), true, Some(saved)),
            Some([10, 20, 810, 620])
        );
    }

    #[test]
    fn presentation_and_focus_require_truthful_visible_restored_state() {
        let expected = [100, 198, 1380, 900];
        assert_eq!(PRESENTATION_ATTEMPTS, 3);
        assert!(hosted_style_invariants(
            0x9000_0000u32 as isize,
            0x0000_0188
        ));
        assert!(!hosted_style_invariants(
            0x90c0_0000u32 as isize,
            0x0000_0188
        ));
        assert!(!hosted_style_invariants(
            0xd000_0000u32 as isize,
            0x0000_0188
        ));
        assert!(!hosted_style_invariants(
            0x9000_0000u32 as isize,
            0x0004_0188
        ));
        assert!(!hosted_style_invariants(
            0x9000_0000u32 as isize,
            0x0000_0108
        ));
        assert!(presentation_matches(true, false, expected, expected));
        assert!(!presentation_matches(false, false, expected, expected));
        assert!(!presentation_matches(true, true, expected, expected));
        assert!(!presentation_matches(
            true,
            false,
            expected,
            [101, 198, 1381, 900]
        ));
        assert!(focus_matches(true, false, true));
        assert!(focus_matches(true, false, false));
        assert!(!focus_matches(false, false, true));
        assert!(!focus_matches(true, true, true));
    }

    #[test]
    fn hidden_gui_recovery_accepts_one_exact_tree_and_stops_children_first() {
        let program_files = Path::new("C:/Program Files");
        assert!(fixed_program_files_gui_path_matches(
            program_files,
            Path::new("C:/Program Files/Mullvad VPN/Mullvad VPN.exe")
        ));
        for rejected in [
            Path::new("C:/Users/liam/AppData/Local/Programs/Mullvad VPN/Mullvad VPN.exe"),
            Path::new("C:/Program Files/Mullvad VPN/resources/mullvad-daemon.exe"),
            Path::new("C:/Program Files/Mullvad VPN/Mullvad VPN copy.exe"),
        ] {
            assert!(!fixed_program_files_gui_path_matches(
                program_files,
                rejected
            ));
        }
        assert!(exact_gui_path_matches(
            Path::new(r"C:\Program Files\Mullvad VPN\Mullvad VPN.exe"),
            Path::new(r"C:\Program Files\Mullvad VPN\Mullvad VPN.exe")
        ));
        assert!(!exact_gui_path_matches(
            Path::new(r"C:\Program Files\Mullvad VPN\Mullvad VPN.exe"),
            Path::new(r"C:\Program Files\Mullvad VPN\resources\mullvad-daemon.exe")
        ));
        assert_eq!(
            exact_gui_tree_termination_order(&[(10, 2), (11, 10), (12, 11), (13, 10)]),
            Ok(vec![12, 13, 11, 10])
        );
        assert_eq!(
            exact_gui_tree_termination_order(&[(10, 2), (20, 3)]),
            Err(MullvadWindowHostReason::ExistingSessionAmbiguous)
        );
        assert_eq!(
            exact_gui_tree_termination_order(&[(10, 11), (11, 10)]),
            Err(MullvadWindowHostReason::ExistingSessionAmbiguous)
        );
        assert_eq!(
            exact_gui_tree_termination_order(&[]),
            Err(MullvadWindowHostReason::ExistingSessionUnavailable)
        );
    }
}
