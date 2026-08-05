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
//! - **Non-Windows**: `Off` is a no-op, but `On` fails closed because
//!   there is no platform primitive that can prove capture exclusion.
//!   v1 alpha targets Windows only; macOS / Linux compositor exclusions
//!   are deferred.
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
//! Windows-side failure of the parent call. Non-Windows returns `Ok(())`
//! only for `Off`; requesting `On` returns an unsupported-platform error.

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
    #[error("capture protection failed: {0}")]
    CaptureProtection(#[from] crate::screenshot_gate::CaptureProtectionError),
}

pub type Result<T> = core::result::Result<T, ScreenshotError>;

#[cfg(windows)]
mod imp {
    use super::{Result, ScreenshotError, ScreenshotProtection};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowDisplayAffinity, SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE, WDA_NONE,
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
        if protection == ScreenshotProtection::On {
            return crate::screenshot_gate::verify_capture_protection(hwnd_isize)
                .map_err(ScreenshotError::from);
        }
        // windows = "0.56.0": `HWND` is `pub struct HWND(pub isize);`
        // — wrap the raw isize directly. Earlier `*mut c_void` casts
        // here were a holdover from a different windows-rs version.
        let hwnd = HWND(hwnd_isize);
        let flag = flag_for(protection);
        unsafe {
            SetWindowDisplayAffinity(hwnd, flag).map_err(|e| {
                ScreenshotError::Win32(format!("{} (HRESULT 0x{:08X})", e.message(), e.code().0))
            })?;
            let mut observed = 0u32;
            GetWindowDisplayAffinity(hwnd, &mut observed).map_err(|e| {
                ScreenshotError::Win32(format!("{} (HRESULT 0x{:08X})", e.message(), e.code().0))
            })?;
            if observed != flag.0 {
                return Err(ScreenshotError::Win32(format!(
                    "display affinity readback mismatch: requested 0x{:08X}, observed 0x{:08X}",
                    flag.0, observed
                )));
            }
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
    use super::{Result, ScreenshotError, ScreenshotProtection};
    use crate::screenshot_gate::CaptureProtectionError;

    /// Non-Windows can explicitly disable protection, but cannot prove it.
    pub(super) fn apply(_hwnd: isize, protection: ScreenshotProtection) -> Result<()> {
        match protection {
            ScreenshotProtection::Off => Ok(()),
            ScreenshotProtection::On => Err(ScreenshotError::CaptureProtection(
                CaptureProtectionError::UnsupportedPlatform,
            )),
        }
    }

    /// Non-Windows can explicitly disable protection, but cannot prove it.
    pub(super) fn apply_with_children(hwnd: isize, protection: ScreenshotProtection) -> Result<()> {
        apply(hwnd, protection)
    }
}

/// Apply the chosen `protection` to the window with the given raw
/// HWND value. On non-Windows targets, `Off` succeeds and `On` fails closed.
pub fn apply_to_hwnd(hwnd_isize: isize, protection: ScreenshotProtection) -> Result<()> {
    imp::apply(hwnd_isize, protection)
}

/// Whether this build actually enforces capture resistance.
///
/// `apply_to_hwnd` returning `Ok(())` is **not** evidence that anything was
/// protected: explicit disable paths can succeed even when no capture
/// protection primitive exists. Callers that put a claim about capture
/// resistance in front of a user must gate that claim on this, not on `Ok(())`.
/// Off Windows the honest answer is "no capture protection exists here", and a
/// UI that says otherwise is making a false public claim.
pub const fn capture_protection_is_enforced() -> bool {
    cfg!(windows)
}

/// Compatibility wrapper used by the Tauri layer. Windows only permits
/// display affinity on a current-process top-level HWND, so this applies
/// `protection` to the supplied OSL-owned top-level window and does not
/// attempt unsupported child or cross-process calls.
///
/// On non-Windows targets, `Off` succeeds and `On` fails closed.
pub fn apply_to_hwnd_and_children(
    hwnd_isize: isize,
    protection: ScreenshotProtection,
) -> Result<()> {
    imp::apply_with_children(hwnd_isize, protection)
}

#[cfg(test)]
mod capture_enforcement_tests {
    use super::{apply_to_hwnd, capture_protection_is_enforced, ScreenshotProtection};

    /// T15 — the false-claim defect. On a non-Windows build the request
    /// "succeeds" against a window that does not exist, because the whole
    /// implementation is a stub. Any UI that reads that `Ok(())` as proof of
    /// capture resistance is lying to the user, so success and enforcement
    /// must not be the same signal.
    #[test]
    fn a_successful_request_is_not_evidence_that_protection_is_enforced() {
        assert!(apply_to_hwnd(0, ScreenshotProtection::Off).is_ok());
        assert_eq!(capture_protection_is_enforced(), cfg!(windows));
        if !cfg!(windows) {
            assert!(apply_to_hwnd(0, ScreenshotProtection::On).is_err());
            assert!(
                !capture_protection_is_enforced(),
                "off Windows there is no display-affinity primitive at all, so no surface may claim capture resistance"
            );
        }
    }
}
