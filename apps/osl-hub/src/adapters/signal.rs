//! Live accessibility-node source for the pure Signal selectors.
//!
//! This boundary owns only the conversion from an already-claimed Signal UIA
//! tree into `SignalNode`s.  It deliberately does not discover windows, read
//! accessible names or text, place input, or attest a destination.

use super::AdapterRefusal;
use crate::native_signal_adapter::{
    discover_signal_composer, discover_signal_transcript, SignalNode, SignalRect, SignalRole,
    SignalSelectorError,
};

const MAX_SIGNAL_A11Y_NODES: usize = 4_096;
const MAX_SIGNAL_A11Y_DEPTH: usize = 64;

/// The result of one bounded read of the exact Signal accessibility root.
#[derive(Clone)]
pub struct SignalNodeSnapshot {
    pub nodes: Vec<SignalNode>,
    pub window_bounds: SignalRect,
}

impl SignalNodeSnapshot {
    /// Feed the live node source to the selector layer without adding any
    /// Signal-specific selection policy at this boundary.
    pub fn locate(&self) -> Result<SignalLocatedNodes, AdapterRefusal> {
        let composer =
            discover_signal_composer(&self.nodes, self.window_bounds).map_err(composer_refusal)?;
        let transcript = discover_signal_transcript(&self.nodes, composer, self.window_bounds)
            .map_err(transcript_refusal)?;
        Ok(SignalLocatedNodes {
            composer,
            transcript,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SignalLocatedNodes {
    pub composer: usize,
    pub transcript: usize,
}

fn composer_refusal(error: SignalSelectorError) -> AdapterRefusal {
    match error {
        SignalSelectorError::Missing => AdapterRefusal::ComposerNotFound,
        SignalSelectorError::Ambiguous => AdapterRefusal::ComposerAmbiguous,
        SignalSelectorError::Invalid
        | SignalSelectorError::LimitExceeded
        | SignalSelectorError::ProofMismatch => AdapterRefusal::AccessibilityUnavailable,
    }
}

fn transcript_refusal(error: SignalSelectorError) -> AdapterRefusal {
    match error {
        SignalSelectorError::Missing => AdapterRefusal::TranscriptNotFound,
        SignalSelectorError::Ambiguous
        | SignalSelectorError::Invalid
        | SignalSelectorError::LimitExceeded
        | SignalSelectorError::ProofMismatch => AdapterRefusal::AccessibilityUnavailable,
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn snapshot_claimed_signal_nodes(
    target: crate::native_window_host::NativeDiscordAccessibilityTarget,
    process_is_trusted: &dyn Fn(u32) -> bool,
) -> Result<SignalNodeSnapshot, AdapterRefusal> {
    windows::snapshot_claimed_signal_nodes(target, process_is_trusted)
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use ::windows::Win32::Foundation::HWND;
    use ::windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    };
    use ::windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTreeWalker,
        IUIAutomationValuePattern, UIA_ButtonControlTypeId, UIA_EditControlTypeId,
        UIA_ListControlTypeId, UIA_PaneControlTypeId, UIA_TextControlTypeId, UIA_ValuePatternId,
        UIA_WindowControlTypeId,
    };

    struct ComGuard(bool);

    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    pub(super) fn snapshot_claimed_signal_nodes(
        target: crate::native_window_host::NativeDiscordAccessibilityTarget,
        process_is_trusted: &dyn Fn(u32) -> bool,
    ) -> Result<SignalNodeSnapshot, AdapterRefusal> {
        let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let _com = ComGuard(initialized.is_ok());
        let automation: IUIAutomation =
            unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }
                .map_err(|_| AdapterRefusal::AccessibilityUnavailable)?;
        let root = unsafe { automation.ElementFromHandle(HWND(target.window as _)) }
            .map_err(|_| AdapterRefusal::AccessibilityUnavailable)?;
        let root_node = project_node(&root, target.process_id, process_is_trusted)?;
        let window_bounds = root_node.bounds;
        if !window_bounds.valid() {
            return Err(AdapterRefusal::AccessibilityUnavailable);
        }
        let walker = unsafe { automation.RawViewWalker() }
            .map_err(|_| AdapterRefusal::AccessibilityUnavailable)?;
        let mut nodes = vec![root_node];
        let mut stack = vec![(0usize, root, 0usize)];
        while let Some((parent_index, parent, depth)) = stack.pop() {
            if depth >= MAX_SIGNAL_A11Y_DEPTH {
                return Err(AdapterRefusal::AccessibilityUnavailable);
            }
            let children = child_elements(&walker, &parent)?;
            for child in children.into_iter().rev() {
                if nodes.len() >= MAX_SIGNAL_A11Y_NODES {
                    return Err(AdapterRefusal::AccessibilityUnavailable);
                }
                let child_node = project_node(&child, target.process_id, process_is_trusted)?;
                let child_index = nodes.len();
                nodes.push(child_node);
                nodes[parent_index].children.push(child_index);
                stack.push((child_index, child, depth + 1));
            }
        }
        Ok(SignalNodeSnapshot {
            nodes,
            window_bounds,
        })
    }

    fn child_elements(
        walker: &IUIAutomationTreeWalker,
        parent: &IUIAutomationElement,
    ) -> Result<Vec<IUIAutomationElement>, AdapterRefusal> {
        let mut children = Vec::new();
        let mut current = unsafe { walker.GetFirstChildElement(parent) }
            .map_err(|_| AdapterRefusal::AccessibilityUnavailable)?;
        while let Some(element) = current {
            if children.len() >= MAX_SIGNAL_A11Y_NODES {
                return Err(AdapterRefusal::AccessibilityUnavailable);
            }
            current = unsafe { walker.GetNextSiblingElement(&element) }
                .map_err(|_| AdapterRefusal::AccessibilityUnavailable)?;
            children.push(element);
        }
        Ok(children)
    }

    fn project_node(
        element: &IUIAutomationElement,
        expected_process_id: u32,
        process_is_trusted: &dyn Fn(u32) -> bool,
    ) -> Result<SignalNode, AdapterRefusal> {
        let process_id = unsafe { element.CurrentProcessId() }
            .map_err(|_| AdapterRefusal::AccessibilityUnavailable)?;
        if process_id <= 0
            || process_id as u32 != expected_process_id
            || !process_is_trusted(process_id as u32)
        {
            return Err(AdapterRefusal::AccessibilityUnavailable);
        }
        let rect = unsafe { element.CurrentBoundingRectangle() }
            .map_err(|_| AdapterRefusal::AccessibilityUnavailable)?;
        let control_type = unsafe { element.CurrentControlType() }
            .map_err(|_| AdapterRefusal::AccessibilityUnavailable)?;
        let mut node = SignalNode::structural(
            role_from_control_type(control_type),
            SignalRect {
                left: rect.left,
                top: rect.top,
                right: rect.right,
                bottom: rect.bottom,
            },
        );
        node.visible = !unsafe { element.CurrentIsOffscreen() }
            .map_err(|_| AdapterRefusal::AccessibilityUnavailable)?
            .as_bool();
        node.enabled = unsafe { element.CurrentIsEnabled() }
            .map_err(|_| AdapterRefusal::AccessibilityUnavailable)?
            .as_bool();
        node.focusable = unsafe { element.CurrentIsKeyboardFocusable() }
            .map_err(|_| AdapterRefusal::AccessibilityUnavailable)?
            .as_bool();
        node.editable = control_type == UIA_EditControlTypeId.0;
        node.read_only = if node.editable {
            let value_pattern = unsafe {
                element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
            }
            .map_err(|_| AdapterRefusal::AccessibilityUnavailable)?;
            unsafe { value_pattern.CurrentIsReadOnly() }
                .map_err(|_| AdapterRefusal::AccessibilityUnavailable)?
                .as_bool()
        } else {
            true
        };
        Ok(node)
    }

    fn role_from_control_type(control_type: i32) -> SignalRole {
        match control_type {
            value if value == UIA_WindowControlTypeId.0 => SignalRole::Window,
            value if value == UIA_PaneControlTypeId.0 => SignalRole::Pane,
            value if value == UIA_ListControlTypeId.0 => SignalRole::List,
            value if value == UIA_TextControlTypeId.0 => SignalRole::Text,
            value if value == UIA_EditControlTypeId.0 => SignalRole::EditableText,
            value if value == UIA_ButtonControlTypeId.0 => SignalRole::Button,
            _ => SignalRole::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_signal_adapter::SignalNode;

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> SignalRect {
        SignalRect {
            left,
            top,
            right,
            bottom,
        }
    }

    fn editable(bounds: SignalRect) -> SignalNode {
        let mut node = SignalNode::structural(SignalRole::EditableText, bounds);
        node.focusable = true;
        node.editable = true;
        node.read_only = false;
        node
    }

    #[test]
    fn t3_t14_live_nodes_feed_the_existing_signal_selectors() {
        let window_bounds = rect(0, 0, 1200, 900);
        let snapshot = SignalNodeSnapshot {
            nodes: vec![
                SignalNode::structural(SignalRole::Window, window_bounds),
                SignalNode::structural(SignalRole::List, rect(430, 90, 1130, 710)),
                editable(rect(455, 735, 1125, 820)),
            ],
            window_bounds,
        };

        assert_eq!(
            snapshot.locate(),
            Ok(SignalLocatedNodes {
                composer: 2,
                transcript: 1,
            })
        );
    }

    #[test]
    fn t3_t14_empty_live_tree_refuses_instead_of_locating_a_composer() {
        let snapshot = SignalNodeSnapshot {
            nodes: Vec::new(),
            window_bounds: rect(0, 0, 1200, 900),
        };

        assert_eq!(snapshot.locate(), Err(AdapterRefusal::ComposerNotFound));
    }
}
