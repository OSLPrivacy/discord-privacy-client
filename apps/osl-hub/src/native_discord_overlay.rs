//! OSL-owned, capability-minimal native Discord composer overlay.
//!
//! The overlay is a separate local Tauri window. It never adopts, reparents,
//! scrapes message history, or accesses Discord credentials, cookies, tokens,
//! private APIs, or process memory. The native adapter reads bounded visible
//! accessibility names/focus/bounds and checks only whether the current
//! composer is empty or contains its own fixed marker. It never retains or
//! returns that value. After one explicit gesture, it types the fixed
//! non-secret marker through ordinary Windows input.

use osl_privacy_hub::service_host::ActiveServiceHost;
use osl_privacy_hub::{
    broker::HubBrokerState, core_bridge::HubCoreState, native_window_host::NativeWindowHostState,
};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Mutex,
};
use std::time::{Duration, Instant};
use tauri::{
    webview::NewWindowResponse, window::Color, Manager, PhysicalPosition, PhysicalSize, WebviewUrl,
};
use zeroize::Zeroizing;

use osl_privacy_hub::native_discord_adapter::{AccessibilityBounds, NativeDiscordComposerState};

pub(crate) const OVERLAY_LABEL: &str = "native-discord-overlay";
pub(crate) const SHIELD_LABEL: &str = "native-discord-shield";
const OVERLAY_ASSET: &str = "overlay.html";
const SHIELD_ASSET: &str = "shield.html";
const FIRST_GUARD_GRACE: Duration = Duration::from_secs(3);

#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    BeginDeferWindowPos, DeferWindowPos, EndDeferWindowPos, GetForegroundWindow, GetWindow,
    GetWindowThreadProcessId, SetWindowPos, GW_HWNDNEXT, GW_HWNDPREV, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW,
};

#[derive(Debug)]
struct OverlaySession {
    epoch: u64,
    context_token: Zeroizing<String>,
    host: ActiveServiceHost,
    phase: OverlayPhase,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum OverlayPhase {
    Guarding,
    Ready,
}

pub(crate) struct OverlaySessionState {
    inner: Mutex<Option<OverlaySession>>,
    next_epoch: AtomicU64,
    covertext_enabled: AtomicBool,
}

impl Default for OverlaySessionState {
    fn default() -> Self {
        Self {
            inner: Mutex::new(None),
            next_epoch: AtomicU64::new(0),
            covertext_enabled: AtomicBool::new(true),
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct OverlayRect {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

fn overlay_rect(discord: [i32; 4]) -> Option<OverlayRect> {
    let width = discord[2].checked_sub(discord[0])?;
    let height = discord[3].checked_sub(discord[1])?;
    if width < 640 || height < 400 {
        return None;
    }
    // Cover only Discord's central message viewport and composer. The native
    // server/channel chrome and sidebars remain visible and interactive.
    // These presentation-only insets are never conversation evidence; the
    // native accessibility guard remains authoritative.
    let sidebar_inset = 312.min(width / 3).max(240);
    let horizontal_gutter = (width / 100).clamp(12, 24);
    let bottom_gutter = (height / 50).clamp(12, 24);
    let header_inset = (height / 15).clamp(48, 72);
    let overlay_width = width
        .checked_sub(sidebar_inset)?
        .checked_sub(horizontal_gutter.checked_mul(2)?)?;
    let x = discord[0]
        .checked_add(sidebar_inset)?
        .checked_add(horizontal_gutter)?;
    let y = discord[1].checked_add(header_inset)?;
    let bottom = discord[3].checked_sub(bottom_gutter)?;
    let overlay_height = bottom.checked_sub(y)?;
    let rect = OverlayRect {
        x,
        y,
        width: overlay_width.try_into().ok()?,
        height: overlay_height.try_into().ok()?,
    };
    // Fail closed if arithmetic or an unusual window shape would let the
    // protected viewport escape Discord or consume its native header/sidebar.
    let right = rect.x.checked_add(i32::try_from(rect.width).ok()?)?;
    let bottom = rect.y.checked_add(i32::try_from(rect.height).ok()?)?;
    (rect.x >= discord[0]
        && rect.y >= discord[1]
        && right <= discord[2]
        && bottom <= discord[3]
        && overlay_width >= 320
        && overlay_height >= height / 2)
        .then_some(rect)
}

fn overlay_rect_with_composer(
    discord: [i32; 4],
    composer: Option<AccessibilityBounds>,
) -> Option<OverlayRect> {
    let fallback = || overlay_rect(discord);
    let composer = match composer {
        Some(value) => value,
        None => return fallback(),
    };
    let discord_width = discord[2].checked_sub(discord[0])?;
    let discord_height = discord[3].checked_sub(discord[1])?;
    let width = composer.right.checked_sub(composer.left)?;
    let source_height = composer.bottom.checked_sub(composer.top)?;
    if composer.left < discord[0]
        || composer.top < discord[1] + discord_height / 2
        || composer.right > discord[2]
        || composer.bottom > discord[3]
        || width < 320
        || width > discord_width
        || !(24..=200).contains(&source_height)
    {
        return fallback();
    }
    let header_inset = (discord_height / 15).clamp(48, 72);
    let y = discord[1].checked_add(header_inset)?;
    let height = composer.bottom.checked_sub(y)?;
    (height >= discord_height / 2)
        .then_some(OverlayRect {
            x: composer.left,
            y,
            width: width.try_into().ok()?,
            height: height.try_into().ok()?,
        })
        .or_else(fallback)
}

fn bundled_overlay_navigation(url: &url::Url) -> bool {
    let local_origin = (url.scheme() == "tauri" && url.host_str() == Some("localhost"))
        || (url.scheme() == "http"
            && url.host_str() == Some("tauri.localhost")
            && url.port().is_none());
    local_origin
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && matches!(url.path(), "/overlay.html" | "/overlay.html/")
}

fn bundled_shield_navigation(url: &url::Url) -> bool {
    let local_origin = (url.scheme() == "tauri" && url.host_str() == Some("localhost"))
        || (url.scheme() == "http"
            && url.host_str() == Some("tauri.localhost")
            && url.port().is_none());
    local_origin
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && matches!(url.path(), "/shield.html" | "/shield.html/")
}

impl OverlaySessionState {
    pub(crate) fn covertext_enabled(&self) -> bool {
        self.covertext_enabled.load(Ordering::Acquire)
    }

    pub(crate) fn set_covertext_enabled(&self, enabled: bool) {
        self.covertext_enabled.store(enabled, Ordering::Release);
    }

    pub(crate) fn activate(
        &self,
        context_token: String,
        host: ActiveServiceHost,
    ) -> Result<u64, String> {
        if context_token.is_empty() || context_token.len() > 256 {
            return Err("The native Discord protection context is invalid".to_owned());
        }
        let epoch = self
            .next_epoch
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1);
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| "The native Discord overlay state is unavailable".to_owned())?;
        *guard = Some(OverlaySession {
            epoch,
            context_token: Zeroizing::new(context_token),
            host,
            phase: OverlayPhase::Guarding,
        });
        Ok(epoch)
    }

    pub(crate) fn clear(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            *guard = None;
        }
        self.next_epoch.fetch_add(1, Ordering::AcqRel);
    }

    pub(crate) fn with_bootstrap_context<T>(
        &self,
        operation: impl FnOnce(&str, &ActiveServiceHost) -> Result<T, String>,
    ) -> Result<T, String> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| "The native Discord overlay state is unavailable".to_owned())?;
        let session = guard
            .as_ref()
            .ok_or_else(|| "The native Discord overlay is not active".to_owned())?;
        operation(session.context_token.as_str(), &session.host)
    }

    pub(crate) fn with_context<T>(
        &self,
        operation: impl FnOnce(&str, &ActiveServiceHost) -> Result<T, String>,
    ) -> Result<T, String> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| "The native Discord overlay state is unavailable".to_owned())?;
        let session = guard
            .as_ref()
            .filter(|session| session.phase == OverlayPhase::Ready)
            .ok_or_else(|| {
                "The native Discord overlay has not passed its safety check".to_owned()
            })?;
        operation(session.context_token.as_str(), &session.host)
    }

    fn mark_ready(&self, epoch: u64, expected_host: &ActiveServiceHost) -> Result<(), String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| "The native Discord overlay state is unavailable".to_owned())?;
        let session = guard
            .as_mut()
            .filter(|session| session.epoch == epoch && &session.host == expected_host)
            .ok_or_else(|| "The native Discord overlay context changed".to_owned())?;
        session.phase = OverlayPhase::Ready;
        Ok(())
    }

    pub(crate) fn validated_marker(
        &self,
        validate: impl FnOnce(&str, &ActiveServiceHost) -> Result<(), String>,
    ) -> Result<(u64, ActiveServiceHost), String> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| "The native Discord overlay state is unavailable".to_owned())?;
        let session = guard
            .as_ref()
            .filter(|session| session.phase == OverlayPhase::Ready)
            .ok_or_else(|| "The native Discord overlay is not active".to_owned())?;
        validate(session.context_token.as_str(), &session.host)?;
        Ok((session.epoch, session.host.clone()))
    }

    pub(crate) fn validate_marker(
        &self,
        epoch: u64,
        expected_host: &ActiveServiceHost,
        validate: impl FnOnce(&str, &ActiveServiceHost) -> Result<(), String>,
    ) -> Result<(), String> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| "The native Discord overlay state is unavailable".to_owned())?;
        let session = guard
            .as_ref()
            .filter(|session| {
                session.phase == OverlayPhase::Ready
                    && session.epoch == epoch
                    && &session.host == expected_host
            })
            .ok_or_else(|| "The native Discord overlay context changed".to_owned())?;
        validate(session.context_token.as_str(), &session.host)
    }

    fn is_epoch(&self, epoch: u64) -> bool {
        self.inner
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|session| session.epoch == epoch))
            .unwrap_or(false)
    }

    fn is_ready(&self, epoch: u64) -> bool {
        self.inner
            .lock()
            .ok()
            .and_then(|guard| {
                guard
                    .as_ref()
                    .map(|session| session.epoch == epoch && session.phase == OverlayPhase::Ready)
            })
            .unwrap_or(false)
    }
}

fn hide_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window(OVERLAY_LABEL) {
        let _ = window.hide();
        let _ = window.close();
    }
    if let Some(window) = app.get_webview_window(SHIELD_LABEL) {
        let _ = window.hide();
        let _ = window.close();
    }
}

pub(crate) fn clear_and_hide(app: &tauri::AppHandle) {
    app.state::<OverlaySessionState>().clear();
    hide_window(app);
}

fn position_window(
    window: &tauri::WebviewWindow,
    discord_rect: [i32; 4],
    composer_bounds: Option<AccessibilityBounds>,
) -> Result<(), String> {
    let rect = overlay_rect_with_composer(discord_rect, composer_bounds)
        .ok_or_else(|| "The native Discord window is too small for safe protection".to_owned())?;
    window
        .set_size(PhysicalSize::new(rect.width, rect.height))
        .map_err(|_| "The native Discord overlay size could not be verified".to_owned())?;
    window
        .set_position(PhysicalPosition::new(rect.x, rect.y))
        .map_err(|_| "The native Discord overlay position could not be verified".to_owned())
}

#[cfg(target_os = "windows")]
fn position_window_pair(
    overlay: &tauri::WebviewWindow,
    shield: &tauri::WebviewWindow,
    discord_rect: [i32; 4],
    composer_bounds: Option<AccessibilityBounds>,
) -> Result<(), String> {
    let rect = overlay_rect_with_composer(discord_rect, composer_bounds)
        .ok_or_else(|| "The native Discord window is too small for safe protection".to_owned())?;
    let overlay_hwnd = overlay
        .hwnd()
        .map_err(|_| "The native Discord overlay position could not be verified".to_owned())?
        .0 as windows_sys::Win32::Foundation::HWND;
    let shield_hwnd = shield
        .hwnd()
        .map_err(|_| "The OSL capture shield position could not be verified".to_owned())?
        .0 as windows_sys::Win32::Foundation::HWND;
    if overlay_hwnd.is_null() || shield_hwnd.is_null() || overlay_hwnd == shield_hwnd {
        return Err("The OSL protected window pair is unavailable".to_owned());
    }
    let deferred = unsafe { BeginDeferWindowPos(2) };
    if deferred.is_null() {
        return Err("The OSL protected window pair could not be moved safely".to_owned());
    }
    let deferred = unsafe {
        DeferWindowPos(
            deferred,
            shield_hwnd,
            std::ptr::null_mut(),
            rect.x,
            rect.y,
            rect.width as i32,
            rect.height as i32,
            SWP_NOACTIVATE | SWP_NOZORDER,
        )
    };
    if deferred.is_null() {
        return Err("The OSL capture shield could not follow Discord safely".to_owned());
    }
    let deferred = unsafe {
        DeferWindowPos(
            deferred,
            overlay_hwnd,
            std::ptr::null_mut(),
            rect.x,
            rect.y,
            rect.width as i32,
            rect.height as i32,
            SWP_NOACTIVATE | SWP_NOZORDER,
        )
    };
    if deferred.is_null() || unsafe { EndDeferWindowPos(deferred) } == 0 {
        return Err("The OSL protected window pair could not follow Discord safely".to_owned());
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn position_window_pair(
    overlay: &tauri::WebviewWindow,
    shield: &tauri::WebviewWindow,
    discord_rect: [i32; 4],
    composer_bounds: Option<AccessibilityBounds>,
) -> Result<(), String> {
    position_window(shield, discord_rect, composer_bounds)?;
    position_window(overlay, discord_rect, composer_bounds)
}

fn exact_shield_stack(
    overlay: isize,
    shield: isize,
    immediately_below_overlay: isize,
    immediately_above_shield: isize,
) -> bool {
    overlay != 0
        && shield != 0
        && overlay != shield
        && immediately_below_overlay == shield
        && immediately_above_shield == overlay
}

#[cfg(target_os = "windows")]
fn ensure_shield_stack(
    overlay: &tauri::WebviewWindow,
    shield: &tauri::WebviewWindow,
) -> Result<(), String> {
    let overlay_hwnd = overlay
        .hwnd()
        .map_err(|_| "The OSL capture shield stack is unavailable".to_owned())?
        .0 as windows_sys::Win32::Foundation::HWND;
    let shield_hwnd = shield
        .hwnd()
        .map_err(|_| "The OSL capture shield stack is unavailable".to_owned())?
        .0 as windows_sys::Win32::Foundation::HWND;
    if overlay_hwnd.is_null() || shield_hwnd.is_null() || overlay_hwnd == shield_hwnd {
        return Err("The OSL capture shield stack is unavailable".to_owned());
    }
    // Put the opaque shield immediately behind the capture-excluded overlay.
    // SWP_NOACTIVATE ensures the shield can never take typing focus.
    if unsafe {
        SetWindowPos(
            shield_hwnd,
            overlay_hwnd,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        )
    } == 0
    {
        return Err("The OSL capture shield could not be stacked safely".to_owned());
    }
    let below_overlay = unsafe { GetWindow(overlay_hwnd, GW_HWNDNEXT) };
    let above_shield = unsafe { GetWindow(shield_hwnd, GW_HWNDPREV) };
    if !exact_shield_stack(
        overlay_hwnd as isize,
        shield_hwnd as isize,
        below_overlay as isize,
        above_shield as isize,
    ) {
        return Err("The OSL capture shield stacking could not be verified".to_owned());
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn ensure_shield_stack(
    _overlay: &tauri::WebviewWindow,
    _shield: &tauri::WebviewWindow,
) -> Result<(), String> {
    Err("The OSL capture shield requires Windows".to_owned())
}

fn ensure_shield_window(
    app: &tauri::AppHandle,
    discord_rect: [i32; 4],
) -> Result<tauri::WebviewWindow, String> {
    let shield = if let Some(window) = app.get_webview_window(SHIELD_LABEL) {
        window
    } else {
        let composer_bounds = app
            .state::<NativeDiscordComposerState>()
            .verified_composer_bounds();
        let rect = overlay_rect_with_composer(discord_rect, composer_bounds).ok_or_else(|| {
            "The native Discord window is too small for safe protection".to_owned()
        })?;
        tauri::WebviewWindowBuilder::new(
            app,
            SHIELD_LABEL,
            WebviewUrl::App(PathBuf::from(SHIELD_ASSET)),
        )
        .title("OSL capture shield")
        .position(f64::from(rect.x), f64::from(rect.y))
        .inner_size(f64::from(rect.width), f64::from(rect.height))
        .transparent(false)
        .background_color(Color(0, 0, 0, 255))
        .decorations(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .closable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .shadow(false)
        .focused(false)
        .focusable(false)
        .visible(false)
        .devtools(false)
        .on_navigation(bundled_shield_navigation)
        .on_new_window(|_, _| NewWindowResponse::Deny)
        .on_download(|_, _| false)
        .build()
        .map_err(|_| "The OSL capture shield could not be created safely".to_owned())?
    };
    position_window(
        &shield,
        discord_rect,
        app.state::<NativeDiscordComposerState>()
            .verified_composer_bounds(),
    )?;
    Ok(shield)
}

fn ensure_window(
    app: &tauri::AppHandle,
    discord_rect: [i32; 4],
) -> Result<tauri::WebviewWindow, String> {
    let window = if let Some(window) = app.get_webview_window(OVERLAY_LABEL) {
        window
    } else {
        let composer_bounds = app
            .state::<NativeDiscordComposerState>()
            .verified_composer_bounds();
        let rect = overlay_rect_with_composer(discord_rect, composer_bounds).ok_or_else(|| {
            "The native Discord window is too small for safe protection".to_owned()
        })?;
        let window = tauri::WebviewWindowBuilder::new(
            app,
            OVERLAY_LABEL,
            WebviewUrl::App(PathBuf::from(OVERLAY_ASSET)),
        )
        .title("OSL private composer")
        .position(f64::from(rect.x), f64::from(rect.y))
        .inner_size(f64::from(rect.width), f64::from(rect.height))
        .transparent(true)
        .decorations(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .closable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .shadow(false)
        // A minimized or disconnected RDP desktop has no foreground queue.
        // Asking WebView2 to take initial focus there can block window
        // creation indefinitely. Focus is requested after creation only when
        // Windows reports an interactive foreground desktop.
        .focused(false)
        .visible(false)
        .devtools(false)
        .on_navigation(bundled_overlay_navigation)
        .on_new_window(|_, _| NewWindowResponse::Deny)
        .on_download(|_, _| false)
        .build()
        .map_err(|_| "The native Discord overlay could not be created safely".to_owned())?;
        let focus_window = window.clone();
        let focus_app = app.clone();
        window.on_window_event(move |event| {
            if matches!(event, tauri::WindowEvent::Focused(true))
                && super::screenshot::apply_to_window(
                    &focus_window,
                    runtime::ScreenshotProtection::On,
                )
                .is_err()
            {
                clear_and_hide(&focus_app);
            }
            // A newly shown WebView can emit Focused(false) before Windows
            // exposes its HWND to the foreground queue. The signed-host,
            // owner, generation, context, geometry, and focus guard below
            // performs the authoritative fail-closed check every 500 ms.
            // Closing here races legitimate initial focus on VM/RDP desktops.
        });
        window
    };
    position_window(
        &window,
        discord_rect,
        app.state::<NativeDiscordComposerState>()
            .verified_composer_bounds(),
    )?;
    Ok(window)
}

pub(crate) fn show(
    app: &tauri::AppHandle,
    discord_rect: [i32; 4],
    epoch: u64,
) -> Result<(), String> {
    let _shield = ensure_shield_window(app, discord_rect)?;
    let window = ensure_window(app, discord_rect)?;
    // OSL plaintext may appear only after the new HWND is capture-resistant.
    if super::screenshot::apply_to_window(&window, runtime::ScreenshotProtection::On).is_err() {
        let _ = window.hide();
        let _ = window.close();
        app.state::<OverlaySessionState>().clear();
        return Err("The native Discord overlay could not enable capture resistance".to_owned());
    }
    // Both HWNDs remain hidden until the first complete host/context/focus
    // guard succeeds. Renderer IPC is phase-gated independently, so a hidden
    // WebView cannot fetch or render protected plaintext while it initializes.
    start_guard(app.clone(), epoch, discord_rect);
    Ok(())
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum FirstGuardDecision {
    WaitHidden,
    Reveal,
    Close,
}

fn first_guard_decision(
    desktop_has_foreground: bool,
    discord_foreground: bool,
    osl_foreground: bool,
    startup_grace_active: bool,
) -> FirstGuardDecision {
    if !desktop_has_foreground || discord_foreground || osl_foreground {
        FirstGuardDecision::Reveal
    } else if startup_grace_active {
        FirstGuardDecision::WaitHidden
    } else {
        FirstGuardDecision::Close
    }
}

fn trusted_foreground(app: &tauri::AppHandle, discord_foreground: bool) -> bool {
    trusted_focus_state(
        desktop_has_foreground_window(),
        discord_foreground,
        osl_process_is_foreground(app),
    )
}

#[cfg(target_os = "windows")]
fn osl_process_is_foreground(_app: &tauri::AppHandle) -> bool {
    let foreground = unsafe { GetForegroundWindow() };
    if foreground.is_null() {
        return false;
    }
    let mut process_id = 0u32;
    unsafe { GetWindowThreadProcessId(foreground, &mut process_id) };
    process_id == std::process::id()
}

#[cfg(not(target_os = "windows"))]
fn osl_process_is_foreground(app: &tauri::AppHandle) -> bool {
    app.get_webview_window(OVERLAY_LABEL)
        .and_then(|window| window.is_focused().ok())
        .unwrap_or(false)
        || app
            .get_webview_window("main")
            .and_then(|window| window.is_focused().ok())
            .unwrap_or(false)
}

fn trusted_focus_state(
    desktop_has_foreground: bool,
    discord_foreground: bool,
    osl_foreground: bool,
) -> bool {
    !desktop_has_foreground || discord_foreground || osl_foreground
}

#[cfg(target_os = "windows")]
fn desktop_has_foreground_window() -> bool {
    // GetForegroundWindow legitimately returns NULL while an RDP desktop is
    // minimized/disconnected. That is not evidence that a foreign app took
    // focus, so the protected OSL window may open or remain open there.
    // As soon as the desktop has a foreground window again, the normal exact
    // Discord/OSL focus checks above resume and fail closed on foreign focus.
    !unsafe { GetForegroundWindow() }.is_null()
}

#[cfg(not(target_os = "windows"))]
fn desktop_has_foreground_window() -> bool {
    true
}

fn start_guard(app: tauri::AppHandle, epoch: u64, mut last_rect: [i32; 4]) {
    let first_guard_deadline = Instant::now() + FIRST_GUARD_GRACE;
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(100));
        let overlay_state = app.state::<OverlaySessionState>();
        if !overlay_state.is_epoch(epoch) {
            return;
        }
        let ready = overlay_state.is_ready(epoch);
        let verified: Result<bool, String> = (|| {
            let owner = super::active_unlocked_osl_user_id(&app.state::<HubCoreState>())?;
            let target = app
                .state::<NativeWindowHostState>()
                .discord_overlay_target(&owner)?;
            let (guarded, ready_host) =
                overlay_state.with_bootstrap_context(|context_token, stored_host| {
                    let current = super::require_current_context_host(
                        &app,
                        &app.state::<HubCoreState>(),
                        &app.state::<HubBrokerState>(),
                        context_token,
                    )?;
                    if &current != stored_host || current.generation != target.generation {
                        return Err("The native Discord protection context changed".to_owned());
                    }
                    if !ready {
                        match first_guard_decision(
                            desktop_has_foreground_window(),
                            target.foreground,
                            osl_process_is_foreground(&app),
                            Instant::now() < first_guard_deadline,
                        ) {
                            FirstGuardDecision::WaitHidden => return Ok((false, None)),
                            FirstGuardDecision::Close => {
                                return Err(
                                    "The native Discord window is no longer foreground".to_owned()
                                )
                            }
                            FirstGuardDecision::Reveal => {}
                        }
                    }
                    if ready && !trusted_foreground(&app, target.foreground) {
                        return Err("The native Discord window is no longer foreground".to_owned());
                    }
                    if target.rect != last_rect {
                        let window = app
                            .get_webview_window(OVERLAY_LABEL)
                            .ok_or_else(|| "The native Discord overlay closed".to_owned())?;
                        let shield = app
                            .get_webview_window(SHIELD_LABEL)
                            .ok_or_else(|| "The OSL capture shield closed".to_owned())?;
                        position_window_pair(
                            &window,
                            &shield,
                            target.rect,
                            app.state::<NativeDiscordComposerState>()
                                .verified_composer_bounds(),
                        )?;
                        last_rect = target.rect;
                    }
                    let window = app
                        .get_webview_window(OVERLAY_LABEL)
                        .ok_or_else(|| "The native Discord overlay closed".to_owned())?;
                    let shield = app
                        .get_webview_window(SHIELD_LABEL)
                        .ok_or_else(|| "The OSL capture shield closed".to_owned())?;
                    if !ready {
                        super::screenshot::apply_to_window(
                            &window,
                            runtime::ScreenshotProtection::On,
                        )
                        .map_err(|_| {
                            "The native Discord overlay could not enable capture resistance"
                                .to_owned()
                        })?;
                        shield.show().map_err(|_| {
                            "The OSL capture shield could not be shown safely".to_owned()
                        })?;
                        window.show().map_err(|_| {
                            "The native Discord overlay could not be shown safely".to_owned()
                        })?;
                        window.set_focus().map_err(|_| {
                            "The native Discord overlay could not receive trusted input focus"
                                .to_owned()
                        })?;
                    }
                    ensure_shield_stack(&window, &shield)?;
                    if !ready {
                        let confirmed = app
                            .state::<NativeWindowHostState>()
                            .discord_overlay_target(&owner)?;
                        let confirmed_current = super::require_current_context_host(
                            &app,
                            &app.state::<HubCoreState>(),
                            &app.state::<HubBrokerState>(),
                            context_token,
                        )?;
                        if &confirmed_current != stored_host
                            || confirmed.generation != target.generation
                            || confirmed.rect != target.rect
                            || first_guard_decision(
                                desktop_has_foreground_window(),
                                confirmed.foreground,
                                osl_process_is_foreground(&app),
                                Instant::now() < first_guard_deadline,
                            ) != FirstGuardDecision::Reveal
                        {
                            return Err(
                                "The native Discord protection context changed while opening"
                                    .to_owned(),
                            );
                        }
                    }
                    Ok((true, (!ready).then(|| stored_host.clone())))
                })?;
            if let Some(host) = ready_host {
                // Do not attempt to reacquire the session mutex from inside
                // with_bootstrap_context; the complete guard result is first
                // copied out, then the phase transition is committed.
                overlay_state.mark_ready(epoch, &host)?;
            }
            Ok(guarded)
        })();
        if verified.is_err() {
            clear_and_hide(&app);
            return;
        }
        if verified == Ok(false) {
            continue;
        }
        std::thread::sleep(Duration::from_millis(400));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_geometry_stays_inside_windowed_discord() {
        let target = [100, 80, 1380, 800];
        let rect = overlay_rect(target).expect("valid overlay geometry");
        assert!(rect.x >= target[0]);
        assert!(rect.y >= target[1]);
        assert!(rect.x + rect.width as i32 <= target[2]);
        assert!(rect.y + rect.height as i32 <= target[3]);
    }

    #[test]
    fn overlay_geometry_stays_inside_fullscreen_discord() {
        let target = [0, 0, 1920, 1080];
        let rect = overlay_rect(target).expect("valid overlay geometry");
        assert_eq!(rect.x, 331);
        assert_eq!(rect.y, 72);
        assert_eq!(rect.width, 1_570);
        assert_eq!(rect.height, 987);
        assert_eq!(rect.x + rect.width as i32, target[2] - 19);
        assert_eq!(rect.y + rect.height as i32, target[3] - 21);
    }

    #[test]
    fn overlay_geometry_preserves_native_header_and_sidebar() {
        let target = [100, 80, 1_380, 800];
        let rect = overlay_rect(target).expect("valid overlay geometry");
        assert_eq!(rect.y - target[1], 48);
        assert_eq!(rect.x - target[0], 324);
        assert!(rect.height >= 600);
        assert_eq!(rect.y + rect.height as i32, target[3] - 14);
    }

    #[test]
    fn verified_composer_bounds_drive_exact_horizontal_anchor() {
        let target = [0, 0, 1_920, 1_080];
        let composer = AccessibilityBounds {
            left: 336,
            top: 996,
            right: 1_888,
            bottom: 1_052,
        };
        let rect = overlay_rect_with_composer(target, Some(composer)).expect("verified composer");
        assert_eq!(rect.x, composer.left);
        assert_eq!(rect.width, (composer.right - composer.left) as u32);
        assert_eq!(rect.y + rect.height as i32, composer.bottom);
        assert_eq!(rect.y, 72);
        assert_eq!(rect.height, 980);
    }

    #[test]
    fn implausible_composer_bounds_use_the_bounded_bottom_fallback() {
        let target = [0, 0, 1_920, 1_080];
        let implausible = AccessibilityBounds {
            left: 20,
            top: 40,
            right: 200,
            bottom: 80,
        };
        assert_eq!(
            overlay_rect_with_composer(target, Some(implausible)),
            overlay_rect(target)
        );
    }

    #[test]
    fn overlay_rejects_unusable_native_geometry() {
        assert_eq!(overlay_rect([0, 0, 639, 900]), None);
        assert_eq!(overlay_rect([0, 0, 900, 399]), None);
        assert_eq!(overlay_rect([100, 100, 50, 50]), None);
    }

    #[test]
    fn navigation_is_bundled_overlay_only() {
        assert!(bundled_overlay_navigation(
            &url::Url::parse("tauri://localhost/overlay.html").unwrap()
        ));
        assert!(!bundled_overlay_navigation(
            &url::Url::parse("tauri://localhost/index.html").unwrap()
        ));
        assert!(!bundled_overlay_navigation(
            &url::Url::parse("https://discord.com/overlay.html").unwrap()
        ));
        assert!(!bundled_overlay_navigation(
            &url::Url::parse("tauri://localhost/overlay.html?token=secret").unwrap()
        ));
        assert!(bundled_shield_navigation(
            &url::Url::parse("tauri://localhost/shield.html").unwrap()
        ));
        assert!(!bundled_shield_navigation(
            &url::Url::parse("https://discord.com/shield.html").unwrap()
        ));
    }

    #[test]
    fn shield_must_be_immediately_behind_overlay() {
        assert!(exact_shield_stack(10, 20, 20, 10));
        assert!(!exact_shield_stack(10, 20, 30, 10));
        assert!(!exact_shield_stack(10, 20, 20, 30));
        assert!(!exact_shield_stack(10, 10, 10, 10));
    }

    #[test]
    fn first_guard_waits_hidden_without_a_foreground_desktop() {
        assert_eq!(
            first_guard_decision(false, false, false, true),
            FirstGuardDecision::Reveal
        );
        assert_eq!(
            first_guard_decision(false, false, false, false),
            FirstGuardDecision::Reveal
        );
        assert!(trusted_focus_state(false, false, false));
    }

    #[test]
    fn first_guard_reveals_only_for_discord_or_osl_focus() {
        assert_eq!(
            first_guard_decision(true, true, false, true),
            FirstGuardDecision::Reveal
        );
        assert_eq!(
            first_guard_decision(true, false, true, false),
            FirstGuardDecision::Reveal
        );
        assert_eq!(
            first_guard_decision(true, false, false, false),
            FirstGuardDecision::Close
        );
    }

    #[test]
    fn foreign_focus_can_only_wait_hidden_during_startup_grace() {
        assert_eq!(
            first_guard_decision(true, false, false, true),
            FirstGuardDecision::WaitHidden
        );
        assert_eq!(
            first_guard_decision(true, false, false, false),
            FirstGuardDecision::Close
        );
    }

    #[test]
    fn interactive_foreign_focus_still_fails_closed() {
        assert!(!trusted_focus_state(true, false, false));
        assert!(trusted_focus_state(true, true, false));
        assert!(trusted_focus_state(true, false, true));
    }
}
