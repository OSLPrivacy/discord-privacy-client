//! Tauri-window glue for the cross-platform screenshot-resistance
//! primitive in [`runtime::screenshot`].
//!
//! Tauri's `WebviewWindow` is the v1 alpha source of truth for the
//! main window's HWND; this module unwraps it and forwards to
//! [`runtime::apply_to_hwnd_and_children`]. Splitting the unwrap from
//! the Win32 call keeps the cross-platform logic (and its tests)
//! inside the `runtime` crate, which builds on Linux dev environments
//! without GTK system libs.
//!
//! Since Layer 9 the main window loads `https://discord.com/app`
//! directly, so the HWND tree includes WebView2's child host windows
//! and render surface. Setting affinity only on the parent has been
//! reported to leak to capture on some Windows builds; the propagated
//! variant in `runtime` walks every descendant via `EnumChildWindows`
//! to close that gap.

use ipc::{IpcError, IpcResult};
use runtime::ScreenshotProtection;

/// Apply `protection` to `window` and every WebView2 descendant HWND.
/// Maps Tauri / Win32 errors to [`ipc::IpcError`] for consistent
/// return shape across the bridge.
#[cfg(windows)]
pub fn apply_to_window(
    window: &tauri::WebviewWindow,
    protection: ScreenshotProtection,
) -> IpcResult<()> {
    let hwnd = window
        .hwnd()
        .map_err(|e| IpcError::Crypto(format!("Tauri window HWND: {e}")))?;
    runtime::apply_to_hwnd_and_children(hwnd.0 as isize, protection)
        .map_err(|e| IpcError::Crypto(e.to_string()))
}

#[cfg(not(windows))]
pub fn apply_to_window(
    _window: &tauri::WebviewWindow,
    _protection: ScreenshotProtection,
) -> IpcResult<()> {
    // Non-Windows: runtime::apply_to_hwnd_and_children is a no-op; we
    // still call through it for symmetry, even though we don't have
    // an HWND.
    runtime::apply_to_hwnd_and_children(0, _protection)
        .map_err(|e| IpcError::Crypto(e.to_string()))
}

/// Apply `protection` to the window containing `webview` and every
/// descendant HWND. This is the variant used from the app-level
/// `Builder::on_page_load` callback, which delivers `&tauri::Webview`
/// (the page-load event is webview-scoped because Tauri 2 supports
/// multiple webviews per window).
///
/// `tauri::Webview` doesn't expose `hwnd()` directly — Tauri 2's
/// HWND lives on the parent `Window`, reachable via
/// `Webview::window()`. The `EnumChildWindows` walk inside
/// `runtime::apply_to_hwnd_and_children` then descends into the
/// WebView2 host + render surface beneath that parent.
#[cfg(windows)]
pub fn apply_to_webview(
    webview: &tauri::Webview,
    protection: ScreenshotProtection,
) -> IpcResult<()> {
    let hwnd = webview
        .window()
        .hwnd()
        .map_err(|e| IpcError::Crypto(format!("Tauri webview-window HWND: {e}")))?;
    runtime::apply_to_hwnd_and_children(hwnd.0 as isize, protection)
        .map_err(|e| IpcError::Crypto(e.to_string()))
}

#[cfg(not(windows))]
pub fn apply_to_webview(
    _webview: &tauri::Webview,
    _protection: ScreenshotProtection,
) -> IpcResult<()> {
    runtime::apply_to_hwnd_and_children(0, _protection)
        .map_err(|e| IpcError::Crypto(e.to_string()))
}
