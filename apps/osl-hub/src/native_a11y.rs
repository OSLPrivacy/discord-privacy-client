//! Shared Windows accessibility primitives for Electron-family native adapters.
//!
//! Chromium enables its accessibility tree lazily.  Its documented handshake is
//! an `EVENT_SYSTEM_ALERT` for custom object id 1, followed by `WM_GETOBJECT`
//! for that same object id.  `OBJID_CLIENT` alone is not that handshake.

/// Chromium's accessibility-presence event.
pub(crate) const EVENT_SYSTEM_ALERT: u32 = 0x0002;

/// Chromium's documented custom accessibility object id.
pub(crate) const ELECTRON_A11Y_OBJECT_ID: i32 = 1;

/// Execute Chromium's two-part accessibility activation handshake.
///
/// Kept generic so the ordering and object-id invariant are unit-testable on
/// non-Windows hosts.  The returned value is the owned accessibility reference
/// obtained by the second half of the handshake.
pub(crate) fn wake_electron_accessibility_with<T>(
    notify_alert: impl FnOnce(u32, i32),
    get_object: impl FnOnce(i32) -> Option<T>,
) -> Option<T> {
    notify_alert(EVENT_SYSTEM_ALERT, ELECTRON_A11Y_OBJECT_ID);
    get_object(ELECTRON_A11Y_OBJECT_ID)
}

/// The class of an `ElementFromIAccessible` HRESULT.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MsaaBridgeCallClass {
    Apartment,
    ProviderGone,
    Busy,
    Refused,
    AccessDenied,
    Resources,
    Unclassified,
}

/// Classify one `ElementFromIAccessible` HRESULT without requiring Windows
/// headers, so every native adapter shares the same retry boundary.
pub(crate) fn msaa_bridge_call_class(hresult: i32) -> MsaaBridgeCallClass {
    const CO_E_NOTINITIALIZED: i32 = 0x8004_01F0u32 as i32;
    const RPC_E_CHANGED_MODE: i32 = 0x8001_0106u32 as i32;
    const RPC_E_WRONG_THREAD: i32 = 0x8001_010Eu32 as i32;
    const RPC_E_THREAD_NOT_INIT: i32 = 0x8001_010Fu32 as i32;
    const RPC_E_DISCONNECTED: i32 = 0x8001_0108u32 as i32;
    const RPC_E_SERVERFAULT: i32 = 0x8001_0105u32 as i32;
    const RPC_S_SERVER_UNAVAILABLE: i32 = 0x8007_06BAu32 as i32;
    const RPC_S_CALL_FAILED: i32 = 0x8007_06BEu32 as i32;
    const UIA_E_ELEMENTNOTAVAILABLE: i32 = 0x8004_0201u32 as i32;
    const RPC_E_CALL_REJECTED: i32 = 0x8001_0001u32 as i32;
    const RPC_E_SERVERCALL_RETRYLATER: i32 = 0x8001_010Au32 as i32;
    const RPC_E_TIMEOUT: i32 = 0x8001_011Fu32 as i32;
    const UIA_E_TIMEOUT: i32 = 0x8013_1505u32 as i32;
    const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
    const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
    const E_POINTER: i32 = 0x8000_4003u32 as i32;
    const E_FAIL: i32 = 0x8000_4005u32 as i32;
    const UIA_E_ELEMENTNOTENABLED: i32 = 0x8004_0200u32 as i32;
    const UIA_E_NOTSUPPORTED: i32 = 0x8004_0204u32 as i32;
    const E_ACCESSDENIED: i32 = 0x8007_0005u32 as i32;
    const E_OUTOFMEMORY: i32 = 0x8007_000Eu32 as i32;

    match hresult {
        CO_E_NOTINITIALIZED | RPC_E_CHANGED_MODE | RPC_E_WRONG_THREAD | RPC_E_THREAD_NOT_INIT => {
            MsaaBridgeCallClass::Apartment
        }
        RPC_E_DISCONNECTED
        | RPC_E_SERVERFAULT
        | RPC_S_SERVER_UNAVAILABLE
        | RPC_S_CALL_FAILED
        | UIA_E_ELEMENTNOTAVAILABLE => MsaaBridgeCallClass::ProviderGone,
        RPC_E_CALL_REJECTED | RPC_E_SERVERCALL_RETRYLATER | RPC_E_TIMEOUT | UIA_E_TIMEOUT => {
            MsaaBridgeCallClass::Busy
        }
        E_INVALIDARG
        | E_NOINTERFACE
        | E_POINTER
        | E_FAIL
        | UIA_E_ELEMENTNOTENABLED
        | UIA_E_NOTSUPPORTED => MsaaBridgeCallClass::Refused,
        E_ACCESSDENIED => MsaaBridgeCallClass::AccessDenied,
        E_OUTOFMEMORY => MsaaBridgeCallClass::Resources,
        _ => MsaaBridgeCallClass::Unclassified,
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn wake_electron_accessibility(
    window: isize,
) -> Option<::windows::Win32::UI::Accessibility::IAccessible> {
    use std::ffi::c_void;
    use windows::core::Interface;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Accessibility::{
        AccessibleObjectFromWindow, IAccessible, NotifyWinEvent,
    };

    if window == 0 {
        return None;
    }
    wake_electron_accessibility_with(
        |event, object_id| unsafe { NotifyWinEvent(event, HWND(window as _), object_id, 0) },
        |object_id| {
            let mut object: *mut c_void = std::ptr::null_mut();
            unsafe {
                AccessibleObjectFromWindow(
                    HWND(window as _),
                    object_id as u32,
                    &IAccessible::IID,
                    &mut object,
                )
            }
            .ok()?;
            (!object.is_null()).then(|| unsafe { IAccessible::from_raw(object) })
        },
    )
}

/// Bridge one MSAA object into UI Automation. This is intentionally only the
/// provider call: each service adapter retains responsibility for validating
/// process ownership, visibility, and geometry of the returned element.
#[cfg(target_os = "windows")]
pub(crate) fn element_from_ia_accessible(
    automation: &::windows::Win32::UI::Accessibility::IUIAutomation,
    accessible: &::windows::Win32::UI::Accessibility::IAccessible,
) -> Result<::windows::Win32::UI::Accessibility::IUIAutomationElement, MsaaBridgeCallClass> {
    unsafe { automation.ElementFromIAccessible(accessible, 0) }
        .map_err(|error| msaa_bridge_call_class(error.code().0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn electron_wake_alerts_then_requests_the_same_custom_object() {
        let calls = std::cell::RefCell::new(Vec::new());
        let reference = wake_electron_accessibility_with(
            |event, object_id| calls.borrow_mut().push(("alert", event as i32, object_id)),
            |object_id| {
                calls.borrow_mut().push(("get_object", 0, object_id));
                Some("owned reference")
            },
        );

        assert_eq!(reference, Some("owned reference"));
        assert_eq!(
            calls.into_inner(),
            vec![
                ("alert", EVENT_SYSTEM_ALERT as i32, ELECTRON_A11Y_OBJECT_ID),
                ("get_object", 0, ELECTRON_A11Y_OBJECT_ID),
            ]
        );
    }

    #[test]
    fn bridge_hresult_classes_keep_retryable_provider_busy_distinct() {
        assert_eq!(
            msaa_bridge_call_class(0x8001_010Au32 as i32),
            MsaaBridgeCallClass::Busy
        );
        assert_eq!(
            msaa_bridge_call_class(0x8004_0201u32 as i32),
            MsaaBridgeCallClass::ProviderGone
        );
    }
}
