//! Find the foreground Signal Desktop window and the open conversation surface.
//!
//! The returned receipt is deliberately redacted: it contains only structural
//! node indices and geometry, never the conversation title, contact name, phone
//! number, draft, or message text.

use crate::native_signal_adapter::{
    discover_signal_transcript, resolve_signal_composer, SignalNode, SignalRect, SignalRole,
    SignalSelectorError,
};

pub const SIGNAL_WINDOW_TITLE: &str = "Signal";
pub const SIGNAL_WINDOW_CLASS: &str = "Chrome_WidgetWin_1";
pub const SIGNAL_PROCESS_IMAGE: &str = "Signal.exe";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignalWindowCandidate {
    pub process_image: String,
    pub class_name: String,
    pub title: String,
    pub visible: bool,
    pub foreground: bool,
    pub bounds: SignalRect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SignalSurfaceMatch {
    pub active_window_index: usize,
    pub conversation_node_index: usize,
    pub typing_box_node_index: usize,
    pub window_bounds: SignalRect,
    pub conversation_bounds: SignalRect,
    pub typing_box_bounds: SignalRect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SignalFinderError {
    ActiveWindowMissing,
    ActiveWindowAmbiguous,
    TypingBoxMissing,
    TypingBoxAmbiguous,
    ConversationMissing,
    ConversationAmbiguous,
    AccessibilityUnavailable,
    PlatformUnsupported,
}

impl std::fmt::Display for SignalFinderError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::ActiveWindowMissing => "the foreground window is not the exact Signal window",
            Self::ActiveWindowAmbiguous => "more than one foreground Signal window matched",
            Self::TypingBoxMissing => "the Signal typing box was not found",
            Self::TypingBoxAmbiguous => "the Signal typing box was ambiguous",
            Self::ConversationMissing => "the open Signal conversation was not found",
            Self::ConversationAmbiguous => "the open Signal conversation was ambiguous",
            Self::AccessibilityUnavailable => "the Signal accessibility tree was unavailable",
            Self::PlatformUnsupported => "live Signal finding is supported only on Windows",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for SignalFinderError {}

/// Find exactly one foreground Signal window, its open conversation transcript,
/// and the writable typing box paired geometrically with that transcript.
pub fn find_signal_open_direct_message(
    windows: &[SignalWindowCandidate],
    nodes: &[SignalNode],
) -> Result<SignalSurfaceMatch, SignalFinderError> {
    let matches = windows
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| exact_active_signal_window(candidate).then_some(index))
        .collect::<Vec<_>>();
    let active_window_index = match matches.as_slice() {
        [index] => *index,
        [] => return Err(SignalFinderError::ActiveWindowMissing),
        _ => return Err(SignalFinderError::ActiveWindowAmbiguous),
    };
    let window_bounds = windows[active_window_index].bounds;
    let typing_box_node_index = resolve_signal_composer(nodes, window_bounds)
        .map_err(typing_box_error)?
        .node_index;
    let conversation_node_index =
        discover_signal_transcript(nodes, typing_box_node_index, window_bounds)
            .map_err(conversation_error)?;
    let typing_box_bounds = nodes
        .get(typing_box_node_index)
        .ok_or(SignalFinderError::AccessibilityUnavailable)?
        .bounds;
    let conversation_bounds = nodes
        .get(conversation_node_index)
        .ok_or(SignalFinderError::AccessibilityUnavailable)?
        .bounds;

    Ok(SignalSurfaceMatch {
        active_window_index,
        conversation_node_index,
        typing_box_node_index,
        window_bounds,
        conversation_bounds,
        typing_box_bounds,
    })
}

fn exact_active_signal_window(candidate: &SignalWindowCandidate) -> bool {
    candidate.visible
        && candidate.foreground
        && candidate.bounds.valid()
        && candidate
            .process_image
            .eq_ignore_ascii_case(SIGNAL_PROCESS_IMAGE)
        && candidate.class_name == SIGNAL_WINDOW_CLASS
        && candidate.title == SIGNAL_WINDOW_TITLE
}

fn typing_box_error(error: SignalSelectorError) -> SignalFinderError {
    match error {
        SignalSelectorError::Missing => SignalFinderError::TypingBoxMissing,
        SignalSelectorError::Ambiguous => SignalFinderError::TypingBoxAmbiguous,
        SignalSelectorError::Invalid
        | SignalSelectorError::LimitExceeded
        | SignalSelectorError::ProofMismatch => SignalFinderError::AccessibilityUnavailable,
    }
}

fn conversation_error(error: SignalSelectorError) -> SignalFinderError {
    match error {
        SignalSelectorError::Missing => SignalFinderError::ConversationMissing,
        SignalSelectorError::Ambiguous => SignalFinderError::ConversationAmbiguous,
        SignalSelectorError::Invalid
        | SignalSelectorError::LimitExceeded
        | SignalSelectorError::ProofMismatch => SignalFinderError::AccessibilityUnavailable,
    }
}

/// Deterministic open-direct-message fixture for the direct build gate.
///
/// This is public so the example and integration test exercise exactly the same
/// selector entry point used by the Windows live path.
pub fn task_1031_open_direct_message_fixture() -> (Vec<SignalWindowCandidate>, Vec<SignalNode>) {
    let window_bounds = SignalRect {
        left: 0,
        top: 0,
        right: 1200,
        bottom: 900,
    };
    let windows = vec![SignalWindowCandidate {
        process_image: SIGNAL_PROCESS_IMAGE.to_owned(),
        class_name: SIGNAL_WINDOW_CLASS.to_owned(),
        title: SIGNAL_WINDOW_TITLE.to_owned(),
        visible: true,
        foreground: true,
        bounds: window_bounds,
    }];
    let mut typing_box = SignalNode::structural(
        SignalRole::EditableText,
        SignalRect {
            left: 455,
            top: 735,
            right: 1125,
            bottom: 820,
        },
    );
    typing_box.focusable = true;
    typing_box.editable = true;
    typing_box.read_only = false;
    typing_box.localized_name = Some("Message".to_owned());
    let nodes = vec![
        SignalNode::structural(SignalRole::Window, window_bounds),
        SignalNode::structural(
            SignalRole::List,
            SignalRect {
                left: 430,
                top: 90,
                right: 1130,
                bottom: 710,
            },
        ),
        typing_box,
    ];
    (windows, nodes)
}

#[cfg(not(target_os = "windows"))]
pub fn find_active_signal_open_direct_message() -> Result<SignalSurfaceMatch, SignalFinderError> {
    Err(SignalFinderError::PlatformUnsupported)
}

#[cfg(target_os = "windows")]
pub fn find_active_signal_open_direct_message() -> Result<SignalSurfaceMatch, SignalFinderError> {
    windows::find_active_signal_open_direct_message()
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use std::collections::HashMap;
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    use ::windows::Win32::Foundation::HWND;
    use ::windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    };
    use ::windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTreeWalker,
        IUIAutomationValuePattern, UIA_ButtonControlTypeId, UIA_DocumentControlTypeId,
        UIA_EditControlTypeId, UIA_ListControlTypeId, UIA_PaneControlTypeId, UIA_TextControlTypeId,
        UIA_ValuePatternId, UIA_WindowControlTypeId, UIA_CONTROLTYPE_ID,
    };
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
        IsWindowVisible,
    };

    const MAX_NODES: usize = 4_096;
    const MAX_DEPTH: usize = 64;

    struct ComGuard(bool);

    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    pub(super) fn find_active_signal_open_direct_message(
    ) -> Result<SignalSurfaceMatch, SignalFinderError> {
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.is_null() || unsafe { IsWindowVisible(hwnd) } == 0 {
            return Err(SignalFinderError::ActiveWindowMissing);
        }
        let mut process_id = 0u32;
        if unsafe { GetWindowThreadProcessId(hwnd, &mut process_id) } == 0 || process_id == 0 {
            return Err(SignalFinderError::ActiveWindowMissing);
        }
        let executable =
            process_executable_path(process_id).ok_or(SignalFinderError::ActiveWindowMissing)?;
        let process_image = executable
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or_default()
            .to_owned();
        let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let _com = ComGuard(initialized.is_ok());
        let automation: IUIAutomation =
            unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }
                .map_err(|_| SignalFinderError::AccessibilityUnavailable)?;
        let root = unsafe { automation.ElementFromHandle(HWND(hwnd as _)) }
            .map_err(|_| SignalFinderError::AccessibilityUnavailable)?;
        let root_rect = unsafe { root.CurrentBoundingRectangle() }
            .map_err(|_| SignalFinderError::AccessibilityUnavailable)?;
        let window = SignalWindowCandidate {
            process_image,
            class_name: window_class(hwnd),
            title: window_text(hwnd),
            visible: true,
            foreground: true,
            bounds: SignalRect {
                left: root_rect.left,
                top: root_rect.top,
                right: root_rect.right,
                bottom: root_rect.bottom,
            },
        };
        if !exact_active_signal_window(&window) {
            return Err(SignalFinderError::ActiveWindowMissing);
        }

        let walker = unsafe { automation.RawViewWalker() }
            .map_err(|_| SignalFinderError::AccessibilityUnavailable)?;
        let mut trust = ProcessTrust::new(executable);
        let nodes = snapshot_nodes(&walker, root, &mut trust)?;
        find_signal_open_direct_message(&[window], &nodes)
    }

    struct ProcessTrust {
        expected_executable: String,
        cache: HashMap<u32, bool>,
    }

    impl ProcessTrust {
        fn new(expected_executable: String) -> Self {
            Self {
                expected_executable,
                cache: HashMap::new(),
            }
        }

        fn accepts(&mut self, process_id: u32) -> bool {
            if let Some(accepted) = self.cache.get(&process_id) {
                return *accepted;
            }
            let accepted = process_executable_path(process_id)
                .is_some_and(|path| path.eq_ignore_ascii_case(&self.expected_executable));
            self.cache.insert(process_id, accepted);
            accepted
        }
    }

    fn snapshot_nodes(
        walker: &IUIAutomationTreeWalker,
        root: IUIAutomationElement,
        trust: &mut ProcessTrust,
    ) -> Result<Vec<SignalNode>, SignalFinderError> {
        let root_node = project_node(&root, trust)?;
        let mut nodes = vec![root_node];
        let mut stack = vec![(0usize, root, 0usize)];
        while let Some((parent_index, parent, depth)) = stack.pop() {
            if depth >= MAX_DEPTH {
                return Err(SignalFinderError::AccessibilityUnavailable);
            }
            let children = child_elements(walker, &parent)?;
            for child in children.into_iter().rev() {
                if nodes.len() >= MAX_NODES {
                    return Err(SignalFinderError::AccessibilityUnavailable);
                }
                let child_node = project_node(&child, trust)?;
                let child_index = nodes.len();
                nodes.push(child_node);
                nodes[parent_index].children.push(child_index);
                stack.push((child_index, child, depth + 1));
            }
        }
        Ok(nodes)
    }

    fn child_elements(
        walker: &IUIAutomationTreeWalker,
        parent: &IUIAutomationElement,
    ) -> Result<Vec<IUIAutomationElement>, SignalFinderError> {
        let mut children = Vec::new();
        let mut current = match unsafe { walker.GetFirstChildElement(parent) } {
            Ok(element) => Some(element),
            Err(error) if error.code().0 == 0 => None,
            Err(_) => return Err(SignalFinderError::AccessibilityUnavailable),
        };
        while let Some(element) = current {
            if children.len() >= MAX_NODES {
                return Err(SignalFinderError::AccessibilityUnavailable);
            }
            current = match unsafe { walker.GetNextSiblingElement(&element) } {
                Ok(next) => Some(next),
                Err(error) if error.code().0 == 0 => None,
                Err(_) => return Err(SignalFinderError::AccessibilityUnavailable),
            };
            children.push(element);
        }
        Ok(children)
    }

    fn project_node(
        element: &IUIAutomationElement,
        trust: &mut ProcessTrust,
    ) -> Result<SignalNode, SignalFinderError> {
        let process_id = unsafe { element.CurrentProcessId() }
            .map_err(|_| SignalFinderError::AccessibilityUnavailable)?;
        if process_id <= 0 || !trust.accepts(process_id as u32) {
            return Err(SignalFinderError::AccessibilityUnavailable);
        }
        let rect = unsafe { element.CurrentBoundingRectangle() }
            .map_err(|_| SignalFinderError::AccessibilityUnavailable)?;
        let control_type = unsafe { element.CurrentControlType() }
            .map_err(|_| SignalFinderError::AccessibilityUnavailable)?;
        let focusable = unsafe { element.CurrentIsKeyboardFocusable() }
            .map_err(|_| SignalFinderError::AccessibilityUnavailable)?
            .as_bool();
        let localized_name = unsafe { element.CurrentName() }
            .ok()
            .map(|name| name.to_string())
            .filter(|name| !name.is_empty());
        let editable = control_type == UIA_EditControlTypeId
            || (control_type == UIA_DocumentControlTypeId && focusable);
        let read_only = if control_type == UIA_EditControlTypeId {
            unsafe { element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) }
                .and_then(|pattern| unsafe { pattern.CurrentIsReadOnly() })
                .map(|value| value.as_bool())
                .unwrap_or(true)
        } else {
            !editable
        };
        let mut node = SignalNode::structural(
            role_from_control_type(control_type, editable),
            SignalRect {
                left: rect.left,
                top: rect.top,
                right: rect.right,
                bottom: rect.bottom,
            },
        );
        node.visible = !unsafe { element.CurrentIsOffscreen() }
            .map_err(|_| SignalFinderError::AccessibilityUnavailable)?
            .as_bool();
        node.enabled = unsafe { element.CurrentIsEnabled() }
            .map_err(|_| SignalFinderError::AccessibilityUnavailable)?
            .as_bool();
        node.focusable = focusable;
        node.editable = editable;
        node.read_only = read_only;
        node.localized_name = localized_name;
        Ok(node)
    }

    fn role_from_control_type(control_type: UIA_CONTROLTYPE_ID, editable: bool) -> SignalRole {
        if editable {
            return SignalRole::EditableText;
        }
        match control_type {
            value if value == UIA_WindowControlTypeId => SignalRole::Window,
            value if value == UIA_PaneControlTypeId => SignalRole::Pane,
            value if value == UIA_ListControlTypeId => SignalRole::List,
            value if value == UIA_TextControlTypeId => SignalRole::Text,
            value if value == UIA_ButtonControlTypeId => SignalRole::Button,
            _ => SignalRole::Unknown,
        }
    }

    fn process_executable_path(process_id: u32) -> Option<String> {
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
        if process.is_null() {
            return None;
        }
        let mut buffer = vec![0u16; 32_768];
        let mut length = buffer.len() as u32;
        let ok =
            unsafe { QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut length) };
        unsafe { CloseHandle(process) };
        (ok != 0 && length > 0).then(|| {
            OsString::from_wide(&buffer[..length as usize])
                .to_string_lossy()
                .into_owned()
        })
    }

    fn window_text(hwnd: windows_sys::Win32::Foundation::HWND) -> String {
        let mut buffer = vec![0u16; 512];
        let length = unsafe { GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32) };
        OsString::from_wide(&buffer[..length.max(0) as usize])
            .to_string_lossy()
            .into_owned()
    }

    fn window_class(hwnd: windows_sys::Win32::Foundation::HWND) -> String {
        let mut buffer = vec![0u16; 256];
        let length = unsafe { GetClassNameW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32) };
        OsString::from_wide(&buffer[..length.max(0) as usize])
            .to_string_lossy()
            .into_owned()
    }
}
