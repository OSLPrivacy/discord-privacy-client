//! Screenshot resistance via Win32 `SetWindowDisplayAffinity`.
//!
//! Cross-platform interface: callers identify a target window by its
//! raw HWND value (an `isize`), pick a [`ScreenshotProtection`] state,
//! and call [`apply_to_hwnd`] or the compatibility wrapper
//! [`apply_to_hwnd_and_children`]. Windows supports display affinity
//! only on a top-level window owned by the calling process, so both
//! functions protect that OSL-owned top-level window.
//!
//! - **Windows**: real implementation — wraps
//!   `SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)` /
//!   `WDA_NONE`.
//! - **Non-Windows**: no-op stub so the rest of the binary compiles
//!   on Linux / macOS dev environments. v1 alpha targets Windows
//!   only; macOS / Linux compositor exclusions are deferred.
//!
//! The longer function name remains for API compatibility with the Tauri
//! glue. It deliberately does not call the API on child or foreign HWNDs:
//! those calls are unsupported and do not extend the assurance boundary.
//!
//! ## Caveats
//!
//! - `WDA_EXCLUDEFROMCAPTURE` blocks **OS-level** capture (Snipping
//!   Tool, Game Bar, OBS via `BitBlt`/`PrintWindow`/desktop
//!   duplication APIs). It does **not** block:
//!     - A camera pointed at the screen.
//!     - Kernel-mode capture drivers.
//!     - HDMI capture cards downstream of the GPU.
//! - The capture-blocking guarantee is best-effort; the threat model
//!   labels this a deterrent, not a hard guarantee.
//! - Requires Windows 10 build 2004+ for `WDA_EXCLUDEFROMCAPTURE`.
//!   Older builds silently downgrade to `WDA_MONITOR` which still
//!   excludes capture but blacks out the window to the user too.
//!
//! ## Errors
//!
//! Returns [`ScreenshotError::Win32`] with the GetLastError code on
//! Windows-side failure of the parent call. Non-Windows always
//! returns `Ok(())`.

use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenshotProtection {
    /// Allow OS screen-capture as normal (`WDA_NONE`).
    Off,
    /// `WDA_EXCLUDEFROMCAPTURE`: window contents render as a black
    /// rectangle to OS capture APIs.
    On,
}

#[derive(Debug, Error)]
pub enum ScreenshotError {
    #[error("SetWindowDisplayAffinity failed: {0}")]
    Win32(String),
}

pub type Result<T> = core::result::Result<T, ScreenshotError>;

#[cfg(windows)]
mod imp {
    use super::{Result, ScreenshotError, ScreenshotProtection};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE, WDA_NONE,
    };

    fn flag_for(
        protection: ScreenshotProtection,
    ) -> windows::Win32::UI::WindowsAndMessaging::WINDOW_DISPLAY_AFFINITY {
        match protection {
            ScreenshotProtection::Off => WDA_NONE,
            ScreenshotProtection::On => WDA_EXCLUDEFROMCAPTURE,
        }
    }

    pub(super) fn apply(hwnd_isize: isize, protection: ScreenshotProtection) -> Result<()> {
        // windows = "0.56.0": `HWND` is `pub struct HWND(pub isize);`
        // — wrap the raw isize directly. Earlier `*mut c_void` casts
        // here were a holdover from a different windows-rs version.
        let hwnd = HWND(hwnd_isize);
        let flag = flag_for(protection);
        unsafe {
            SetWindowDisplayAffinity(hwnd, flag).map_err(|e| {
                ScreenshotError::Win32(format!("{} (HRESULT 0x{:08X})", e.message(), e.code().0))
            })?;
        }
        Ok(())
    }

    pub(super) fn apply_with_children(
        hwnd_isize: isize,
        protection: ScreenshotProtection,
    ) -> Result<()> {
        apply(hwnd_isize, protection)
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{Result, ScreenshotProtection};

    /// Non-Windows no-op. Always succeeds.
    pub(super) fn apply(_hwnd: isize, _protection: ScreenshotProtection) -> Result<()> {
        Ok(())
    }

    /// Non-Windows no-op. Always succeeds.
    pub(super) fn apply_with_children(
        _hwnd: isize,
        _protection: ScreenshotProtection,
    ) -> Result<()> {
        Ok(())
    }
}

/// Apply the chosen `protection` to the window with the given raw
/// HWND value. On non-Windows targets this is a no-op that returns
/// `Ok(())` so cross-platform callers don't need their own cfg-gates.
pub fn apply_to_hwnd(hwnd_isize: isize, protection: ScreenshotProtection) -> Result<()> {
    imp::apply(hwnd_isize, protection)
}

/// Compatibility wrapper used by the Tauri layer. Windows only permits
/// display affinity on a current-process top-level HWND, so this applies
/// `protection` to the supplied OSL-owned top-level window and does not
/// attempt unsupported child or cross-process calls.
///
/// On non-Windows targets this is a no-op stub returning `Ok(())`.
pub fn apply_to_hwnd_and_children(
    hwnd_isize: isize,
    protection: ScreenshotProtection,
) -> Result<()> {
    imp::apply_with_children(hwnd_isize, protection)
}
