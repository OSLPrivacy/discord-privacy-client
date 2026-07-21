//! Narrow Windows accessibility bridge for Firefox's first-party migration wizard.
//!
//! This module can only select one compiled-in browser source and activate one
//! compiled-in wizard action. It cannot inspect browser content, type text,
//! choose files, or respond to operating-system authentication prompts.

use crate::native_apps::BrowserImportId;

const WIZARD_HEADINGS: &[&str] = &[
    "Import browser data",
    "Import Browser Data",
    "Import Data from Another Browser",
    "Import Wizard",
];
const SOURCE_SELECTOR_AUTOMATION_ID: &str = "browser-profile-selector";
const IMPORT_AUTOMATION_ID: &str = "import";
const COMPLETE_HEADINGS: &[&str] = &["Data import complete"];
const COMPLETE_BUTTONS: &[&str] = &["Done"];
const MANUAL_PASSWORD_SKIP_BUTTONS: &[&str] = &["Skip"];
const FIREFOX_MIGRATION_WINDOW_CLASSES: &[&str] = &["MozillaWindowClass", "MozillaDialogClass"];

fn is_firefox_migration_window_class(class_name: &str) -> bool {
    FIREFOX_MIGRATION_WINDOW_CLASSES.contains(&class_name)
}

fn source_names(source: BrowserImportId) -> &'static [&'static str] {
    match source {
        BrowserImportId::Chrome => &["Google Chrome"],
        BrowserImportId::Edge => &["Microsoft Edge"],
        BrowserImportId::Firefox => &["Firefox"],
        BrowserImportId::Brave => &["Brave"],
        BrowserImportId::Opera => &["Opera"],
        BrowserImportId::DuckDuckGo => &["DuckDuckGo"],
    }
}

fn source_profile_prefix(source: BrowserImportId) -> &'static str {
    match source {
        BrowserImportId::Chrome => "Chrome — ",
        BrowserImportId::Edge => "Microsoft Edge — ",
        BrowserImportId::Firefox => "Firefox — ",
        BrowserImportId::Brave => "Brave — ",
        BrowserImportId::Opera => "Opera — ",
        BrowserImportId::DuckDuckGo => "DuckDuckGo — ",
    }
}

fn manual_password_heading(source: BrowserImportId) -> String {
    let source_name = match source {
        BrowserImportId::Chrome => "Chrome",
        BrowserImportId::Edge => "Microsoft Edge",
        BrowserImportId::Firefox => "Firefox",
        BrowserImportId::Brave => "Brave",
        BrowserImportId::Opera => "Opera",
        BrowserImportId::DuckDuckGo => "DuckDuckGo",
    };
    format!("How to import passwords from {source_name}")
}

fn exact_unique_match(names: &[String], allowlist: &[&str]) -> Result<usize, String> {
    let matches = names
        .iter()
        .enumerate()
        .filter(|(_, name)| allowlist.iter().any(|allowed| name.as_str() == *allowed))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [index] => Ok(*index),
        [] => Err("Firefox did not expose the exact migration control".to_owned()),
        _ => Err("Firefox exposed an ambiguous migration control".to_owned()),
    }
}

#[cfg(target_os = "windows")]
mod windows_impl {
    use super::{
        exact_unique_match, is_firefox_migration_window_class, source_names, source_profile_prefix,
        BrowserImportId, COMPLETE_BUTTONS, COMPLETE_HEADINGS, IMPORT_AUTOMATION_ID,
        MANUAL_PASSWORD_SKIP_BUTTONS, SOURCE_SELECTOR_AUTOMATION_ID, WIZARD_HEADINGS,
    };
    use std::collections::HashMap;
    use std::time::{Duration, Instant};
    use windows::core::VARIANT;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    };
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, ExpandCollapseState_Expanded, IUIAutomation, IUIAutomationElement,
        IUIAutomationExpandCollapsePattern, IUIAutomationInvokePattern,
        IUIAutomationSelectionItemPattern, TreeScope_Descendants, UIA_ButtonControlTypeId,
        UIA_ExpandCollapsePatternId, UIA_InvokePatternId, UIA_ListItemControlTypeId,
        UIA_MenuItemControlTypeId, UIA_ProcessIdPropertyId, UIA_RadioButtonControlTypeId,
        UIA_SelectionItemPatternId, UIA_TextControlTypeId, UIA_WindowControlTypeId,
        UIA_CONTROLTYPE_ID,
    };
    use windows_sys::Win32::Foundation::{CloseHandle, BOOL, HANDLE, HWND as SysHwnd, LPARAM};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::StationsAndDesktops::{
        CloseDesktop, GetThreadDesktop, OpenDesktopW, SetThreadDesktop,
    };
    use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetForegroundWindow, GetWindowThreadProcessId, IsWindow,
        IsWindowVisible, PostMessageW, SetForegroundWindow, ShowWindow, SW_SHOWNOACTIVATE,
        WM_CLOSE,
    };

    const WINDOW_WAIT: Duration = Duration::from_secs(8);
    const PROFILE_ITEM_WAIT: Duration = Duration::from_secs(2);
    const IMPORT_WAIT: Duration = Duration::from_secs(300);
    const TREE_LIMIT: usize = 512;

    fn restore_foreground(window: isize) {
        if window == 0 || unsafe { IsWindow(window as SysHwnd) } == 0 {
            return;
        }
        let target = window as SysHwnd;
        let current_foreground = unsafe { GetForegroundWindow() };
        if current_foreground == target {
            return;
        }
        let current_thread = unsafe { GetCurrentThreadId() };
        let foreground_thread = if !current_foreground.is_null() {
            unsafe { GetWindowThreadProcessId(current_foreground, std::ptr::null_mut()) }
        } else {
            0
        };
        let target_thread = unsafe { GetWindowThreadProcessId(target, std::ptr::null_mut()) };
        let attached_foreground = foreground_thread != 0
            && foreground_thread != current_thread
            && unsafe { AttachThreadInput(current_thread, foreground_thread, 1) } != 0;
        let attached_target = target_thread != 0
            && target_thread != current_thread
            && target_thread != foreground_thread
            && unsafe { AttachThreadInput(current_thread, target_thread, 1) } != 0;
        unsafe {
            ShowWindow(target, SW_SHOWNOACTIVATE);
            SetForegroundWindow(target);
            if attached_target {
                AttachThreadInput(current_thread, target_thread, 0);
            }
            if attached_foreground {
                AttachThreadInput(current_thread, foreground_thread, 0);
            }
        }
    }

    struct ComGuard(bool);

    struct DesktopGuard {
        original: isize,
        hidden: isize,
    }

    impl DesktopGuard {
        fn enter(name: &str) -> Result<Self, String> {
            let name_wide = name
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect::<Vec<_>>();
            let original = unsafe { GetThreadDesktop(GetCurrentThreadId()) };
            let hidden = unsafe { OpenDesktopW(name_wide.as_ptr(), 0, 0, 0x1000_0000) };
            if hidden.is_null() || unsafe { SetThreadDesktop(hidden) } == 0 {
                if !hidden.is_null() {
                    unsafe { CloseDesktop(hidden) };
                }
                return Err("The private OSL browser-import desktop is unavailable".to_owned());
            }
            Ok(Self {
                original: original as isize,
                hidden: hidden as isize,
            })
        }
    }

    impl Drop for DesktopGuard {
        fn drop(&mut self) {
            unsafe {
                SetThreadDesktop(self.original as _);
                CloseDesktop(self.hidden as _);
            }
        }
    }

    struct WindowCandidates {
        root_process_id: u32,
        parents: HashMap<u32, u32>,
        windows: Vec<(SysHwnd, u32)>,
    }

    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    unsafe extern "system" fn collect_firefox_window(window: SysHwnd, parameter: LPARAM) -> BOOL {
        if unsafe { IsWindowVisible(window) } == 0 {
            return 1;
        }
        let candidates = unsafe { &mut *(parameter as *mut WindowCandidates) };
        let mut process_id = 0u32;
        unsafe { GetWindowThreadProcessId(window, &mut process_id) };
        if !process_is_descendant_or_self(
            process_id,
            candidates.root_process_id,
            &candidates.parents,
        ) {
            return 1;
        }
        let mut class_name = [0u16; 64];
        let length =
            unsafe { GetClassNameW(window, class_name.as_mut_ptr(), class_name.len() as i32) };
        if length > 0
            && is_firefox_migration_window_class(&String::from_utf16_lossy(
                &class_name[..length as usize],
            ))
        {
            candidates.windows.push((window, process_id));
        }
        1
    }

    fn process_parents() -> HashMap<u32, u32> {
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snapshot.is_null() || snapshot == -1isize as HANDLE {
            return HashMap::new();
        }
        let mut parents = HashMap::new();
        let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut found = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
        while found {
            if parents.len() >= 4096 {
                parents.clear();
                break;
            }
            parents.insert(entry.th32ProcessID, entry.th32ParentProcessID);
            found = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
        }
        let _ = unsafe { CloseHandle(snapshot) };
        parents
    }

    fn process_is_descendant_or_self(
        mut process_id: u32,
        root_process_id: u32,
        parents: &HashMap<u32, u32>,
    ) -> bool {
        for _ in 0..64 {
            if process_id == root_process_id {
                return true;
            }
            let Some(parent) = parents.get(&process_id).copied() else {
                return false;
            };
            if parent == 0 || parent == process_id {
                return false;
            }
            process_id = parent;
        }
        false
    }

    fn firefox_windows(process_id: u32) -> Vec<(SysHwnd, u32)> {
        let mut state = WindowCandidates {
            root_process_id: process_id,
            parents: process_parents(),
            windows: Vec::new(),
        };
        unsafe {
            EnumWindows(
                Some(collect_firefox_window),
                (&mut state as *mut WindowCandidates) as LPARAM,
            );
        }
        state.windows
    }

    fn descendants(
        automation: &IUIAutomation,
        root: &IUIAutomationElement,
        process_id: u32,
    ) -> Result<Vec<IUIAutomationElement>, String> {
        // Firefox's native dialog can place an accessibility-provider pane
        // from another process at the top of the raw tree. Traverse only the
        // already verified window subtree, then retain elements owned by the
        // exact Firefox child. Firefox's provider omits popup menu items when
        // ProcessId is supplied as the FindAll condition, so filter afterward.
        let condition = unsafe { automation.CreateTrueCondition() }
            .map_err(|_| "Windows accessibility is unavailable".to_owned())?;
        let found = unsafe { root.FindAll(TreeScope_Descendants, &condition) }
            .map_err(|_| "Windows accessibility is unavailable".to_owned())?;
        let count = unsafe { found.Length() }
            .map_err(|_| "Windows accessibility is unavailable".to_owned())?;
        if count < 0 || count as usize > TREE_LIMIT {
            return Err("Firefox migration accessibility exceeded its safe limit".to_owned());
        }
        let mut elements = Vec::with_capacity(count as usize);
        for index in 0..count {
            let element = unsafe { found.GetElement(index) }
                .map_err(|_| "Windows accessibility is unavailable".to_owned())?;
            if unsafe { element.CurrentProcessId() }.ok() == Some(process_id as i32) {
                elements.push(element);
            }
        }
        Ok(elements)
    }

    fn process_descendants(
        automation: &IUIAutomation,
        process_id: u32,
    ) -> Result<Vec<IUIAutomationElement>, String> {
        let desktop = unsafe { automation.GetRootElement() }
            .map_err(|_| "Windows accessibility is unavailable".to_owned())?;
        let condition = unsafe {
            automation
                .CreatePropertyCondition(UIA_ProcessIdPropertyId, &VARIANT::from(process_id as i32))
        }
        .map_err(|_| "Windows accessibility is unavailable".to_owned())?;
        let found = unsafe { desktop.FindAll(TreeScope_Descendants, &condition) }
            .map_err(|_| "Windows accessibility is unavailable".to_owned())?;
        let count = unsafe { found.Length() }
            .map_err(|_| "Windows accessibility is unavailable".to_owned())?;
        if count < 0 || count as usize > TREE_LIMIT {
            return Err("Firefox migration accessibility exceeded its safe limit".to_owned());
        }
        let mut elements = Vec::with_capacity(count as usize);
        for index in 0..count {
            let element = unsafe { found.GetElement(index) }
                .map_err(|_| "Windows accessibility is unavailable".to_owned())?;
            if unsafe { element.CurrentProcessId() }.ok() == Some(process_id as i32) {
                elements.push(element);
            }
        }
        Ok(elements)
    }

    fn visible_enabled(element: &IUIAutomationElement) -> bool {
        unsafe { element.CurrentIsEnabled() }.is_ok_and(|value| value.as_bool())
            && !unsafe { element.CurrentIsOffscreen() }.is_ok_and(|value| value.as_bool())
    }

    fn exact_elements(
        elements: &[IUIAutomationElement],
        names: &[&str],
        control_types: &[UIA_CONTROLTYPE_ID],
    ) -> Vec<IUIAutomationElement> {
        elements
            .iter()
            .filter(|element| visible_enabled(element))
            .filter(|element| {
                unsafe { element.CurrentControlType() }
                    .ok()
                    .is_some_and(|kind| control_types.contains(&kind))
            })
            .filter(|element| {
                unsafe { element.CurrentName() }
                    .ok()
                    .map(|name| name.to_string())
                    .is_some_and(|name| names.iter().any(|expected| name == *expected))
            })
            .cloned()
            .collect()
    }

    fn require_exact_one(
        elements: Vec<IUIAutomationElement>,
        names: &[&str],
    ) -> Result<IUIAutomationElement, String> {
        let observed = elements
            .iter()
            .map(|element| {
                unsafe { element.CurrentName() }
                    .unwrap_or_default()
                    .to_string()
            })
            .collect::<Vec<_>>();
        let index = exact_unique_match(&observed, names)?;
        Ok(elements[index].clone())
    }

    fn exact_automation_id(
        elements: &[IUIAutomationElement],
        automation_id: &str,
        control_type: UIA_CONTROLTYPE_ID,
    ) -> Result<IUIAutomationElement, String> {
        let matches = elements
            .iter()
            .filter(|element| visible_enabled(element))
            .filter(|element| unsafe { element.CurrentControlType() }.ok() == Some(control_type))
            .filter(|element| {
                unsafe { element.CurrentAutomationId() }
                    .ok()
                    .is_some_and(|value| value.to_string() == automation_id)
            })
            .cloned()
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [element] => Ok(element.clone()),
            [] => Err("Firefox did not expose the exact migration control".to_owned()),
            _ => Err("Firefox exposed an ambiguous migration control".to_owned()),
        }
    }

    fn expand_source_selector(element: &IUIAutomationElement) -> Result<(), String> {
        unsafe { element.SetFocus() }
            .map_err(|_| "Firefox source selector could not receive focus".to_owned())?;
        let pattern = unsafe {
            element.GetCurrentPatternAs::<IUIAutomationExpandCollapsePattern>(
                UIA_ExpandCollapsePatternId,
            )
        }
        .map_err(|_| "Firefox source selector is not safely actionable".to_owned())?;
        if unsafe { pattern.CurrentExpandCollapseState() }.ok()
            == Some(ExpandCollapseState_Expanded)
        {
            unsafe { pattern.Collapse() }
                .map_err(|_| "Firefox would not reset the migration source selector".to_owned())?;
            std::thread::sleep(Duration::from_millis(50));
        }
        unsafe { pattern.Expand() }
            .map_err(|_| "Firefox would not open the migration source selector".to_owned())
    }

    fn selector_already_matches(element: &IUIAutomationElement, source: BrowserImportId) -> bool {
        unsafe { element.CurrentName() }
            .ok()
            .is_some_and(|name| name.to_string().starts_with(source_profile_prefix(source)))
    }

    fn first_profile_item(
        elements: &[IUIAutomationElement],
        prefix: &str,
    ) -> Result<IUIAutomationElement, String> {
        elements
            .iter()
            .find(|element| {
                visible_enabled(element)
                    && unsafe { element.CurrentControlType() }.ok()
                        == Some(UIA_MenuItemControlTypeId)
                    && unsafe { element.CurrentName() }
                        .ok()
                        .is_some_and(|name| name.to_string().starts_with(prefix))
            })
            .cloned()
            .ok_or_else(|| "Firefox did not expose a profile for the selected browser".to_owned())
    }

    fn wait_for_profile_item(
        automation: &IUIAutomation,
        process_id: u32,
        prefix: &str,
    ) -> Result<IUIAutomationElement, String> {
        let deadline = Instant::now() + PROFILE_ITEM_WAIT;
        loop {
            if let Ok(mut element) = unsafe { automation.GetFocusedElement() } {
                let walker = unsafe { automation.RawViewWalker() }
                    .map_err(|_| "Windows accessibility is unavailable".to_owned())?;
                for _ in 0..8 {
                    let is_verified_popup = unsafe { element.CurrentProcessId() }.ok()
                        == Some(process_id as i32)
                        && unsafe { element.CurrentControlType() }.ok()
                            == Some(UIA_WindowControlTypeId)
                        && unsafe { element.CurrentNativeWindowHandle() }
                            .ok()
                            .is_some_and(|handle| handle.0 != 0)
                        && unsafe { element.CurrentClassName() }
                            .ok()
                            .is_some_and(|name| {
                                is_firefox_migration_window_class(&name.to_string())
                            });
                    if is_verified_popup {
                        let elements = descendants(automation, &element, process_id)?;
                        if let Ok(profile) = first_profile_item(&elements, prefix) {
                            return Ok(profile);
                        }
                        break;
                    }
                    let Ok(parent) = (unsafe { walker.GetParentElement(&element) }) else {
                        break;
                    };
                    element = parent;
                }
            }
            // Firefox exposes the open XUL popup as a separate top-level UIA
            // window. This query is still limited to the exact verified OSL
            // Firefox child process and is only performed after the exact
            // source selector has been expanded.
            if let Ok(elements) = process_descendants(automation, process_id) {
                if let Ok(profile) = first_profile_item(&elements, prefix) {
                    return Ok(profile);
                }
            }
            if Instant::now() >= deadline {
                return Err("Firefox did not expose a profile for the selected browser".to_owned());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn select_source(element: &IUIAutomationElement) -> Result<(), String> {
        if let Ok(pattern) = unsafe {
            element.GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(
                UIA_SelectionItemPatternId,
            )
        } {
            return unsafe { pattern.Select() }
                .map_err(|_| "Firefox would not select the migration source".to_owned());
        }
        let pattern = unsafe {
            element.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId)
        }
        .map_err(|_| "Firefox source control is not safely actionable".to_owned())?;
        unsafe { pattern.Invoke() }
            .map_err(|_| "Firefox would not select the migration source".to_owned())
    }

    fn invoke_action(element: &IUIAutomationElement) -> Result<(), String> {
        let pattern = unsafe {
            element.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId)
        }
        .map_err(|_| "Firefox import control is not safely actionable".to_owned())?;
        unsafe { pattern.Invoke() }
            .map_err(|_| "Firefox would not start the selected import".to_owned())
    }

    fn finish_import(
        automation: &IUIAutomation,
        root: &IUIAutomationElement,
        window: SysHwnd,
        window_process_id: u32,
        source: BrowserImportId,
        restore_window: isize,
    ) -> Result<bool, String> {
        let deadline = Instant::now() + IMPORT_WAIT;
        let manual_heading = super::manual_password_heading(source);
        let mut skipped_manual_password_export = false;
        while Instant::now() < deadline {
            restore_foreground(restore_window);
            if unsafe { IsWindow(window) } == 0 {
                return Ok(skipped_manual_password_export);
            }
            let elements = descendants(automation, root, window_process_id)?;
            if !exact_elements(
                &elements,
                &[manual_heading.as_str()],
                &[UIA_TextControlTypeId],
            )
            .is_empty()
            {
                let skip = exact_elements(
                    &elements,
                    MANUAL_PASSWORD_SKIP_BUTTONS,
                    &[UIA_ButtonControlTypeId],
                );
                match skip.as_slice() {
                    [button] => {
                        invoke_action(button)?;
                        skipped_manual_password_export = true;
                        std::thread::sleep(Duration::from_millis(150));
                        continue;
                    }
                    [] => {}
                    _ => {
                        return Err("Firefox exposed ambiguous password-import controls".to_owned())
                    }
                }
            }
            if !exact_elements(&elements, COMPLETE_HEADINGS, &[UIA_TextControlTypeId]).is_empty() {
                let done = exact_elements(&elements, COMPLETE_BUTTONS, &[UIA_ButtonControlTypeId]);
                match done.as_slice() {
                    [button] => {
                        invoke_action(button)?;
                        restore_foreground(restore_window);
                        return Ok(skipped_manual_password_export);
                    }
                    [] => {}
                    _ => return Err("Firefox exposed ambiguous completion controls".to_owned()),
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Err(
            "Firefox import did not reach a verified completion screen within five minutes"
                .to_owned(),
        )
    }

    fn coordinate_window(
        automation: &IUIAutomation,
        window: SysHwnd,
        window_process_id: u32,
        source: BrowserImportId,
        owner_window: isize,
    ) -> Result<bool, String> {
        unsafe {
            ShowWindow(window, SW_SHOWNOACTIVATE);
        }
        restore_foreground(owner_window);
        let root = unsafe { automation.ElementFromHandle(HWND(window as isize)) }
            .map_err(|_| "Firefox migration window is unavailable".to_owned())?;
        let elements = descendants(automation, &root, window_process_id)?;
        require_exact_one(
            exact_elements(&elements, WIZARD_HEADINGS, &[UIA_TextControlTypeId]),
            WIZARD_HEADINGS,
        )?;
        let legacy_source = require_exact_one(
            exact_elements(
                &elements,
                source_names(source),
                &[UIA_RadioButtonControlTypeId, UIA_ListItemControlTypeId],
            ),
            source_names(source),
        );
        if let Ok(source_element) = legacy_source {
            select_source(&source_element)?;
        } else {
            let selector = exact_automation_id(
                &elements,
                SOURCE_SELECTOR_AUTOMATION_ID,
                UIA_ButtonControlTypeId,
            )?;
            if !selector_already_matches(&selector, source) {
                expand_source_selector(&selector)?;
                // The popup remains in the verified migration window's UIA
                // subtree. A desktop-wide ProcessId query is both broader and
                // rejected by Firefox's current accessibility provider.
                // Poll this same root rather than retrying the whole window:
                // refocusing the root collapses Firefox's newly opened menu.
                let profile = wait_for_profile_item(
                    automation,
                    window_process_id,
                    source_profile_prefix(source),
                )?;
                select_source(&profile)?;
            }
        }
        restore_foreground(owner_window);
        std::thread::sleep(Duration::from_millis(150));
        let refreshed = descendants(automation, &root, window_process_id)?;
        let action =
            exact_automation_id(&refreshed, IMPORT_AUTOMATION_ID, UIA_ButtonControlTypeId)?;
        invoke_action(&action)?;
        restore_foreground(owner_window);
        finish_import(
            automation,
            &root,
            window,
            window_process_id,
            source,
            owner_window,
        )
    }

    pub(super) fn coordinate(
        process_id: u32,
        source: BrowserImportId,
        owner_window: isize,
        desktop_name: &str,
    ) -> Result<bool, String> {
        let _desktop = DesktopGuard::enter(desktop_name)?;
        let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
        let _guard = ComGuard(initialized);
        let automation: IUIAutomation =
            unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }
                .map_err(|_| "Windows accessibility is unavailable".to_owned())?;
        let deadline = Instant::now() + WINDOW_WAIT;
        let mut last_error = "Firefox migration window did not appear".to_owned();
        while Instant::now() < deadline {
            restore_foreground(owner_window);
            let windows = firefox_windows(process_id);
            if windows.len() > 1 {
                return Err("Firefox exposed ambiguous migration windows".to_owned());
            }
            if let Some((window, window_process_id)) = windows.first() {
                match coordinate_window(
                    &automation,
                    *window,
                    *window_process_id,
                    source,
                    owner_window,
                ) {
                    Ok(password_follow_up) => {
                        restore_foreground(owner_window);
                        return Ok(password_follow_up);
                    }
                    Err(error) => last_error = error,
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Err(last_error)
    }

    pub(super) fn close(process_id: u32, desktop_name: &str) -> Result<(), String> {
        let _desktop = DesktopGuard::enter(desktop_name)?;
        let windows = firefox_windows(process_id);
        match windows.as_slice() {
            [] => Ok(()),
            [(window, _)] => (unsafe { PostMessageW(*window, WM_CLOSE, 0, 0) } != 0)
                .then_some(())
                .ok_or_else(|| "The OSL Firefox import window could not be closed".to_owned()),
            _ => Err("Firefox exposed ambiguous migration windows".to_owned()),
        }
    }

    pub(super) fn is_closed(process_id: u32, desktop_name: &str) -> Result<(), String> {
        let _desktop = DesktopGuard::enter(desktop_name)?;
        firefox_windows(process_id)
            .is_empty()
            .then_some(())
            .ok_or_else(|| "The OSL Firefox import window did not close".to_owned())
    }
}

#[cfg(target_os = "windows")]
pub fn coordinate(
    process_id: u32,
    source: BrowserImportId,
    owner_window: isize,
    desktop_name: &str,
) -> Result<bool, String> {
    windows_impl::coordinate(process_id, source, owner_window, desktop_name)
}

pub fn close(process_id: u32, desktop_name: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        windows_impl::close(process_id, desktop_name)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (process_id, desktop_name);
        Err("Firefox migration coordination is available only on Windows".to_owned())
    }
}

pub fn is_closed(process_id: u32, desktop_name: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        windows_impl::is_closed(process_id, desktop_name)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (process_id, desktop_name);
        Err("Firefox migration coordination is available only on Windows".to_owned())
    }
}

#[cfg(not(target_os = "windows"))]
pub fn coordinate(
    _process_id: u32,
    _source: BrowserImportId,
    _owner_window: isize,
    _desktop_name: &str,
) -> Result<bool, String> {
    Err("Firefox migration coordination is available only on Windows".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_labels_are_exact_and_browser_specific() {
        assert!(WIZARD_HEADINGS.contains(&"Import browser data"));
        assert_eq!(source_names(BrowserImportId::Chrome), &["Google Chrome"]);
        assert_eq!(source_names(BrowserImportId::Edge), &["Microsoft Edge"]);
        assert_eq!(source_names(BrowserImportId::Firefox), &["Firefox"]);
        assert_eq!(source_names(BrowserImportId::Brave), &["Brave"]);
        assert_eq!(source_names(BrowserImportId::Opera), &["Opera"]);
        assert_eq!(source_names(BrowserImportId::DuckDuckGo), &["DuckDuckGo"]);
        assert_eq!(source_profile_prefix(BrowserImportId::Chrome), "Chrome — ");
        assert_eq!(
            source_profile_prefix(BrowserImportId::Edge),
            "Microsoft Edge — "
        );
        assert_eq!(
            manual_password_heading(BrowserImportId::Chrome),
            "How to import passwords from Chrome"
        );
        assert_eq!(COMPLETE_HEADINGS, &["Data import complete"]);
        assert_eq!(COMPLETE_BUTTONS, &["Done"]);
        assert_eq!(MANUAL_PASSWORD_SKIP_BUTTONS, &["Skip"]);
    }

    #[test]
    fn exact_matching_rejects_missing_partial_and_ambiguous_controls() {
        let expected = source_names(BrowserImportId::Chrome);
        assert!(exact_unique_match(&["Chrome".to_owned()], expected).is_err());
        assert!(exact_unique_match(&["Google Chrome beta".to_owned()], expected).is_err());
        assert!(exact_unique_match(&[], expected).is_err());
        assert!(exact_unique_match(
            &["Google Chrome".to_owned(), "Google Chrome".to_owned()],
            expected,
        )
        .is_err());
        assert_eq!(
            exact_unique_match(&["Google Chrome".to_owned()], expected),
            Ok(0)
        );
    }

    #[test]
    fn migration_window_class_allowlist_covers_current_firefox_dialog() {
        assert!(is_firefox_migration_window_class("MozillaWindowClass"));
        assert!(is_firefox_migration_window_class("MozillaDialogClass"));
        assert!(!is_firefox_migration_window_class("Chrome_WidgetWin_1"));
        assert!(!is_firefox_migration_window_class(
            "MozillaDropShadowWindowClass"
        ));
        assert!(!is_firefox_migration_window_class(""));
    }
}
