//! Tauri-window glue for the cross-platform screenshot-resistance
//! primitive in [`runtime::screenshot`].
//!
//! Tauri's `WebviewWindow` is the v1 alpha source of truth for the
//! main window's HWND; this module unwraps it and forwards to
//! [`runtime::apply_to_hwnd`]. Splitting the unwrap from the Win32
//! call keeps the cross-platform logic (and its tests) inside the
//! `runtime` crate, which builds on Linux dev environments without
//! GTK system libs.

use ipc::{IpcError, IpcResult};
use runtime::ScreenshotProtection;

/// Apply `protection` to `window`. Maps Tauri / Win32 errors to
/// [`ipc::IpcError`] for consistent return shape across the bridge.
#[cfg(windows)]
pub fn apply_to_window(
    window: &tauri::WebviewWindow,
    protection: ScreenshotProtection,
) -> IpcResult<()> {
    let hwnd = window
        .hwnd()
        .map_err(|e| IpcError::Crypto(format!("Tauri window HWND: {e}")))?;
    runtime::apply_to_hwnd(hwnd.0 as isize, protection)
        .map_err(|e| IpcError::Crypto(e.to_string()))
}

#[cfg(not(windows))]
pub fn apply_to_window(
    _window: &tauri::WebviewWindow,
    _protection: ScreenshotProtection,
) -> IpcResult<()> {
    // Non-Windows: runtime::apply_to_hwnd is a no-op; we still call
    // through it for symmetry, even though we don't have an HWND.
    runtime::apply_to_hwnd(0, _protection).map_err(|e| IpcError::Crypto(e.to_string()))
}
