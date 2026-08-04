//! Shared Windows accessibility primitives for native UIA2 adapters.
//!
//! Chromium enables its accessibility tree lazily.  Its documented handshake is
//! an `EVENT_SYSTEM_ALERT` for custom object id 1, followed by `WM_GETOBJECT`
//! for that same object id.  `OBJID_CLIENT` alone is not that handshake.

/// Chromium's accessibility-presence event.
pub(crate) const EVENT_SYSTEM_ALERT: u32 = 0x0002;

/// Chromium's documented custom accessibility object id.
pub(crate) const ELECTRON_A11Y_OBJECT_ID: i32 = 1;

/// Electron's top-level Chromium host window class.
pub const ELECTRON_OUTER_WINDOW_CLASS: &str = "Chrome_WidgetWin_1";

/// Electron's content-bearing Chromium renderer child window class.
pub const ELECTRON_RENDERER_WINDOW_CLASS: &str = "Chrome_RenderWidgetHostHWND";

/// Telegram Desktop's Qt top-level window class. There is no Chromium renderer.
pub const TELEGRAM_OUTER_WINDOW_CLASS: &str = "Qt51519QWindowIcon";

/// WhatsApp Desktop's WinUI shell window. The useful content is not below it.
pub const WHATSAPP_OUTER_WINDOW_CLASS: &str = "WinUIDesktopWin32WindowClass";

/// WhatsApp's WebView2 content process.
pub const WEBVIEW2_PROCESS_NAME: &str = "msedgewebview2";

/// A UIA tree below this size is treated as not yet asynchronously populated.
pub const ELECTRON_UIA2_POPULATED_MIN_ELEMENTS: usize = 10;

/// Measured UIA2 window shapes for providers OSL drives through native a11y.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Uia2WindowShape {
    /// App outer window -> `Chrome_RenderWidgetHostHWND`, wake, then poll.
    ChromiumRendererChild,
    /// The app's outer window is the UIA root; no renderer child is expected.
    DirectOuterWindow,
    /// App shell proves ownership, then bind a sibling WebView2 Chromium host.
    SiblingChromiumRenderer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Uia2WakePolicy {
    None,
    WmGetObjectChromium,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Uia2WindowPlan {
    pub provider_name: &'static str,
    pub app_process_name: &'static str,
    pub app_outer_class: &'static str,
    pub shape: Uia2WindowShape,
    pub sibling_process_name: Option<&'static str>,
    pub sibling_outer_class: Option<&'static str>,
    pub renderer_child_class: Option<&'static str>,
    pub wake_policy: Uia2WakePolicy,
    pub poll_until_populated: bool,
    pub populated_min_elements: usize,
    pub default_wait_ms: u64,
    pub call_timeout_ms: u64,
}

impl Uia2WindowPlan {
    pub const fn chromium_renderer_child(
        provider_name: &'static str,
        app_process_name: &'static str,
        default_wait_ms: u64,
        call_timeout_ms: u64,
    ) -> Self {
        Self {
            provider_name,
            app_process_name,
            app_outer_class: ELECTRON_OUTER_WINDOW_CLASS,
            shape: Uia2WindowShape::ChromiumRendererChild,
            sibling_process_name: None,
            sibling_outer_class: None,
            renderer_child_class: Some(ELECTRON_RENDERER_WINDOW_CLASS),
            wake_policy: Uia2WakePolicy::WmGetObjectChromium,
            poll_until_populated: true,
            populated_min_elements: ELECTRON_UIA2_POPULATED_MIN_ELEMENTS,
            default_wait_ms,
            call_timeout_ms,
        }
    }

    pub const fn direct_outer_window(
        provider_name: &'static str,
        app_process_name: &'static str,
        app_outer_class: &'static str,
        call_timeout_ms: u64,
    ) -> Self {
        Self {
            provider_name,
            app_process_name,
            app_outer_class,
            shape: Uia2WindowShape::DirectOuterWindow,
            sibling_process_name: None,
            sibling_outer_class: None,
            renderer_child_class: None,
            wake_policy: Uia2WakePolicy::None,
            poll_until_populated: false,
            populated_min_elements: 1,
            default_wait_ms: 0,
            call_timeout_ms,
        }
    }

    pub const fn sibling_chromium_renderer(
        provider_name: &'static str,
        app_process_name: &'static str,
        app_outer_class: &'static str,
        sibling_process_name: &'static str,
        default_wait_ms: u64,
        call_timeout_ms: u64,
    ) -> Self {
        Self {
            provider_name,
            app_process_name,
            app_outer_class,
            shape: Uia2WindowShape::SiblingChromiumRenderer,
            sibling_process_name: Some(sibling_process_name),
            sibling_outer_class: Some(ELECTRON_OUTER_WINDOW_CLASS),
            renderer_child_class: Some(ELECTRON_RENDERER_WINDOW_CLASS),
            wake_policy: Uia2WakePolicy::WmGetObjectChromium,
            poll_until_populated: true,
            populated_min_elements: ELECTRON_UIA2_POPULATED_MIN_ELEMENTS,
            default_wait_ms,
            call_timeout_ms,
        }
    }

    pub const fn chromium_outer_mutant(
        provider_name: &'static str,
        app_process_name: &'static str,
        default_wait_ms: u64,
        call_timeout_ms: u64,
    ) -> Self {
        Self {
            provider_name,
            app_process_name,
            app_outer_class: ELECTRON_OUTER_WINDOW_CLASS,
            shape: Uia2WindowShape::DirectOuterWindow,
            sibling_process_name: None,
            sibling_outer_class: None,
            renderer_child_class: None,
            wake_policy: Uia2WakePolicy::WmGetObjectChromium,
            poll_until_populated: true,
            populated_min_elements: ELECTRON_UIA2_POPULATED_MIN_ELEMENTS,
            default_wait_ms,
            call_timeout_ms,
        }
    }

    pub const fn chromium_renderer_no_wake_mutant(
        provider_name: &'static str,
        app_process_name: &'static str,
        default_wait_ms: u64,
        call_timeout_ms: u64,
    ) -> Self {
        Self {
            provider_name,
            app_process_name,
            app_outer_class: ELECTRON_OUTER_WINDOW_CLASS,
            shape: Uia2WindowShape::ChromiumRendererChild,
            sibling_process_name: None,
            sibling_outer_class: None,
            renderer_child_class: Some(ELECTRON_RENDERER_WINDOW_CLASS),
            wake_policy: Uia2WakePolicy::None,
            poll_until_populated: true,
            populated_min_elements: ELECTRON_UIA2_POPULATED_MIN_ELEMENTS,
            default_wait_ms,
            call_timeout_ms,
        }
    }

    pub const fn chromium_renderer_immediate_mutant(
        provider_name: &'static str,
        app_process_name: &'static str,
        call_timeout_ms: u64,
    ) -> Self {
        Self {
            provider_name,
            app_process_name,
            app_outer_class: ELECTRON_OUTER_WINDOW_CLASS,
            shape: Uia2WindowShape::ChromiumRendererChild,
            sibling_process_name: None,
            sibling_outer_class: None,
            renderer_child_class: Some(ELECTRON_RENDERER_WINDOW_CLASS),
            wake_policy: Uia2WakePolicy::WmGetObjectChromium,
            poll_until_populated: false,
            populated_min_elements: ELECTRON_UIA2_POPULATED_MIN_ELEMENTS,
            default_wait_ms: 0,
            call_timeout_ms,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Uia2WindowCandidate<'a> {
    pub hwnd: isize,
    pub parent_hwnd: Option<isize>,
    /// For out-of-process content roots such as WhatsApp WebView2, this ties
    /// the sibling Chromium window back to the app shell that OSL claimed.
    pub associated_app_hwnd: Option<isize>,
    pub process_id: u32,
    pub process_name: &'a str,
    pub class_name: &'a str,
    pub visible: bool,
    pub area: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Uia2ResolvedWindow {
    pub app_outer_hwnd: isize,
    pub bound_hwnd: isize,
    pub bound_process_id: u32,
    pub wake_policy: Uia2WakePolicy,
    pub poll_until_populated: bool,
    pub populated_min_elements: usize,
    pub call_timeout_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Uia2WindowResolveError {
    MissingAppOuter,
    MissingRendererChild,
    MissingSiblingContentOuter,
}

/// Resolve a provider's measured UIA2 root from an already-enumerated window
/// graph. The graph boundary is kept small so Windows enumeration, process
/// trust, visibility and geometry checks can feed one shared resolver.
pub fn resolve_uia2_window(
    plan: Uia2WindowPlan,
    windows: &[Uia2WindowCandidate<'_>],
) -> Result<Uia2ResolvedWindow, Uia2WindowResolveError> {
    let app_outer = largest_visible(windows.iter().copied().filter(|window| {
        same_process_name(window.process_name, plan.app_process_name)
            && window.class_name == plan.app_outer_class
    }))
    .ok_or(Uia2WindowResolveError::MissingAppOuter)?;

    match plan.shape {
        Uia2WindowShape::DirectOuterWindow => Ok(resolved(plan, app_outer.hwnd, app_outer)),
        Uia2WindowShape::ChromiumRendererChild => {
            let renderer_class = plan
                .renderer_child_class
                .ok_or(Uia2WindowResolveError::MissingRendererChild)?;
            let renderer = largest_visible(windows.iter().copied().filter(|window| {
                window.class_name == renderer_class
                    && is_descendant_of(window.hwnd, app_outer.hwnd, windows)
            }))
            .ok_or(Uia2WindowResolveError::MissingRendererChild)?;
            Ok(resolved(plan, app_outer.hwnd, renderer))
        }
        Uia2WindowShape::SiblingChromiumRenderer => {
            let sibling_process = plan
                .sibling_process_name
                .ok_or(Uia2WindowResolveError::MissingSiblingContentOuter)?;
            let sibling_outer_class = plan
                .sibling_outer_class
                .ok_or(Uia2WindowResolveError::MissingSiblingContentOuter)?;
            let renderer_class = plan
                .renderer_child_class
                .ok_or(Uia2WindowResolveError::MissingRendererChild)?;
            let content_outer = largest_visible(windows.iter().copied().filter(|window| {
                same_process_name(window.process_name, sibling_process)
                    && window.class_name == sibling_outer_class
                    && window.associated_app_hwnd == Some(app_outer.hwnd)
            }))
            .ok_or(Uia2WindowResolveError::MissingSiblingContentOuter)?;
            let renderer = largest_visible(windows.iter().copied().filter(|window| {
                window.class_name == renderer_class
                    && is_descendant_of(window.hwnd, content_outer.hwnd, windows)
            }))
            .ok_or(Uia2WindowResolveError::MissingRendererChild)?;
            Ok(resolved(plan, app_outer.hwnd, renderer))
        }
    }
}

fn resolved(
    plan: Uia2WindowPlan,
    app_outer_hwnd: isize,
    bound: Uia2WindowCandidate<'_>,
) -> Uia2ResolvedWindow {
    Uia2ResolvedWindow {
        app_outer_hwnd,
        bound_hwnd: bound.hwnd,
        bound_process_id: bound.process_id,
        wake_policy: plan.wake_policy,
        poll_until_populated: plan.poll_until_populated,
        populated_min_elements: plan.populated_min_elements,
        call_timeout_ms: plan.call_timeout_ms,
    }
}

fn largest_visible<'a>(
    windows: impl Iterator<Item = Uia2WindowCandidate<'a>>,
) -> Option<Uia2WindowCandidate<'a>> {
    windows
        .filter(|window| window.visible && window.hwnd != 0)
        .max_by_key(|window| window.area)
}

fn is_descendant_of(
    child_hwnd: isize,
    ancestor_hwnd: isize,
    windows: &[Uia2WindowCandidate<'_>],
) -> bool {
    let mut current = windows
        .iter()
        .find(|window| window.hwnd == child_hwnd)
        .and_then(|window| window.parent_hwnd);
    while let Some(hwnd) = current {
        if hwnd == ancestor_hwnd {
            return true;
        }
        current = windows
            .iter()
            .find(|window| window.hwnd == hwnd)
            .and_then(|window| window.parent_hwnd);
    }
    false
}

fn same_process_name(actual: &str, expected: &str) -> bool {
    let actual = actual.strip_suffix(".exe").unwrap_or(actual);
    actual.eq_ignore_ascii_case(expected)
}

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

    const WAIT_MS: u64 = 90_000;
    const CALL_TIMEOUT_MS: u64 = 750;

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

    fn window(
        hwnd: isize,
        parent_hwnd: Option<isize>,
        associated_app_hwnd: Option<isize>,
        process_id: u32,
        process_name: &'static str,
        class_name: &'static str,
        area: u32,
    ) -> Uia2WindowCandidate<'static> {
        Uia2WindowCandidate {
            hwnd,
            parent_hwnd,
            associated_app_hwnd,
            process_id,
            process_name,
            class_name,
            visible: true,
            area,
        }
    }

    #[test]
    fn chromium_shape_binds_renderer_wakes_and_polls() {
        let plan =
            Uia2WindowPlan::chromium_renderer_child("Signal", "Signal", WAIT_MS, CALL_TIMEOUT_MS);
        let windows = [
            window(
                10,
                None,
                None,
                100,
                "Signal.exe",
                ELECTRON_OUTER_WINDOW_CLASS,
                900,
            ),
            window(11, Some(10), None, 100, "Signal.exe", "Intermediate", 800),
            window(
                12,
                Some(11),
                None,
                100,
                "Signal.exe",
                ELECTRON_RENDERER_WINDOW_CLASS,
                700,
            ),
        ];

        let resolved = resolve_uia2_window(plan, &windows).expect("Signal renderer should resolve");
        assert_eq!(resolved.app_outer_hwnd, 10);
        assert_eq!(resolved.bound_hwnd, 12);
        assert_eq!(resolved.wake_policy, Uia2WakePolicy::WmGetObjectChromium);
        assert!(resolved.poll_until_populated);
        assert_eq!(
            resolved.populated_min_elements,
            ELECTRON_UIA2_POPULATED_MIN_ELEMENTS
        );
        assert_eq!(resolved.call_timeout_ms, CALL_TIMEOUT_MS);
    }

    #[test]
    fn chromium_shape_requires_renderer_child() {
        let plan =
            Uia2WindowPlan::chromium_renderer_child("Signal", "Signal", WAIT_MS, CALL_TIMEOUT_MS);
        let windows = [window(
            10,
            None,
            None,
            100,
            "Signal",
            ELECTRON_OUTER_WINDOW_CLASS,
            900,
        )];

        assert_eq!(
            resolve_uia2_window(plan, &windows),
            Err(Uia2WindowResolveError::MissingRendererChild)
        );
    }

    #[test]
    fn direct_outer_shape_does_not_require_renderer_or_wake() {
        let plan = Uia2WindowPlan::direct_outer_window(
            "Telegram",
            "Telegram",
            TELEGRAM_OUTER_WINDOW_CLASS,
            CALL_TIMEOUT_MS,
        );
        let windows = [window(
            20,
            None,
            None,
            200,
            "Telegram",
            TELEGRAM_OUTER_WINDOW_CLASS,
            900,
        )];

        let resolved = resolve_uia2_window(plan, &windows).expect("Telegram outer should resolve");
        assert_eq!(resolved.bound_hwnd, 20);
        assert_eq!(resolved.wake_policy, Uia2WakePolicy::None);
        assert!(!resolved.poll_until_populated);
    }

    #[test]
    fn sibling_chromium_shape_binds_webview2_renderer_not_app_root() {
        let plan = Uia2WindowPlan::sibling_chromium_renderer(
            "WhatsApp",
            "WhatsApp",
            WHATSAPP_OUTER_WINDOW_CLASS,
            WEBVIEW2_PROCESS_NAME,
            WAIT_MS,
            CALL_TIMEOUT_MS,
        );
        let windows = [
            window(
                30,
                None,
                None,
                300,
                "WhatsApp",
                WHATSAPP_OUTER_WINDOW_CLASS,
                900,
            ),
            window(
                31,
                Some(30),
                None,
                300,
                "WhatsApp",
                "WinUIChildThatNeverPopulates",
                800,
            ),
            window(
                40,
                None,
                Some(30),
                400,
                WEBVIEW2_PROCESS_NAME,
                ELECTRON_OUTER_WINDOW_CLASS,
                850,
            ),
            window(
                41,
                Some(40),
                Some(30),
                400,
                WEBVIEW2_PROCESS_NAME,
                ELECTRON_RENDERER_WINDOW_CLASS,
                840,
            ),
        ];

        let resolved =
            resolve_uia2_window(plan, &windows).expect("WhatsApp WebView2 renderer should resolve");
        assert_eq!(resolved.app_outer_hwnd, 30);
        assert_eq!(resolved.bound_hwnd, 41);
        assert_eq!(resolved.bound_process_id, 400);
        assert_eq!(resolved.wake_policy, Uia2WakePolicy::WmGetObjectChromium);
        assert!(resolved.poll_until_populated);
    }

    #[test]
    fn sibling_chromium_shape_rejects_app_root_only() {
        let plan = Uia2WindowPlan::sibling_chromium_renderer(
            "WhatsApp",
            "WhatsApp",
            WHATSAPP_OUTER_WINDOW_CLASS,
            WEBVIEW2_PROCESS_NAME,
            WAIT_MS,
            CALL_TIMEOUT_MS,
        );
        let windows = [window(
            30,
            None,
            None,
            300,
            "WhatsApp",
            WHATSAPP_OUTER_WINDOW_CLASS,
            900,
        )];

        assert_eq!(
            resolve_uia2_window(plan, &windows),
            Err(Uia2WindowResolveError::MissingSiblingContentOuter)
        );
    }
}
