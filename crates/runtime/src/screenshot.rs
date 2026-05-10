//! Screenshot resistance via Win32 `SetWindowDisplayAffinity`.
//!
//! Cross-platform interface: callers identify a target window by its
//! raw HWND value (an `isize`), pick a [`ScreenshotProtection`] state,
//! and call [`apply_to_hwnd`] (parent only) or
//! [`apply_to_hwnd_and_children`] (parent + every descendant via
//! `EnumChildWindows`).
//!
//! - **Windows**: real implementation — wraps
//!   `SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)` /
//!   `WDA_NONE`.
//! - **Non-Windows**: no-op stub so the rest of the binary compiles
//!   on Linux / macOS dev environments. v1 alpha targets Windows
//!   only; macOS / Linux compositor exclusions are deferred.
//!
//! ## Why both `apply_to_hwnd` and `apply_to_hwnd_and_children`?
//!
//! Tauri's main window is a parent HWND that hosts a WebView2 child
//! tree (typically `Chrome_WidgetWin_*` host windows + a
//! `Chrome_RenderWidgetHostHWND` rendering surface). Microsoft's
//! documentation says `WDA_EXCLUDEFROMCAPTURE` propagates to children,
//! but in practice WebView2's hierarchy and certain Windows builds
//! exhibit drift where the child render surface still leaks to
//! capture. The belt-and-suspenders fix is to walk every descendant
//! via `EnumChildWindows` and set the flag on each.
//!
//! `EnumChildWindows` recurses into grandchildren automatically (per
//! Microsoft Learn: "If a child window has created child windows of
//! its own, EnumChildWindows enumerates those windows as well"), so
//! a single call covers the full tree.
//!
//! Cross-process WebView2 children (the render-host HWND on some
//! Edge versions runs in a separate process) may legitimately fail
//! `SetWindowDisplayAffinity` with access-denied — those failures are
//! logged via `tracing` but do not abort the operation. The parent
//! call must succeed; child failures are reported only diagnostically.
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
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM, TRUE};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumChildWindows, SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE, WDA_NONE,
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

    /// State threaded through `EnumChildWindows` via `LPARAM`.
    struct ChildState {
        protection: ScreenshotProtection,
        visited: u32,
        succeeded: u32,
        first_failure: Option<String>,
    }

    /// `EnumChildWindows` callback: applies the chosen affinity to one
    /// descendant HWND. Always returns `TRUE` so enumeration covers
    /// every descendant — per-child failures are recorded for
    /// diagnostics rather than aborting the walk.
    unsafe extern "system" fn enum_child_apply(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: `lparam.0` carries a `*mut ChildState` set up by the
        // caller for the lifetime of the EnumChildWindows call.
        let state = unsafe { &mut *(lparam.0 as *mut ChildState) };
        state.visited += 1;
        let flag = flag_for(state.protection);
        // SAFETY: `hwnd` is supplied by the OS and valid for the
        // duration of this callback.
        match unsafe { SetWindowDisplayAffinity(hwnd, flag) } {
            Ok(()) => state.succeeded += 1,
            Err(e) => {
                if state.first_failure.is_none() {
                    state.first_failure =
                        Some(format!("{} (HRESULT 0x{:08X})", e.message(), e.code().0));
                }
            }
        }
        TRUE
    }

    pub(super) fn apply_with_children(
        hwnd_isize: isize,
        protection: ScreenshotProtection,
    ) -> Result<()> {
        // Parent first: a parent failure is the primary signal that
        // capture protection is unavailable on this build.
        apply(hwnd_isize, protection)?;

        // windows = "0.56.0" `HWND(pub isize)`; see `apply` above.
        let parent = HWND(hwnd_isize);
        let mut state = ChildState {
            protection,
            visited: 0,
            succeeded: 0,
            first_failure: None,
        };
        // SAFETY: `EnumChildWindows` calls `enum_child_apply`
        // synchronously during this call; `state` outlives the call.
        unsafe {
            let lparam = LPARAM(&mut state as *mut _ as isize);
            // Return value ignored: we don't terminate enumeration
            // early, so `EnumChildWindows` only returns FALSE for
            // resource-shortage failures we can't recover from. Any
            // partial coverage we got is still useful.
            let _ = EnumChildWindows(parent, Some(enum_child_apply), lparam);
        }

        if state.visited > 0 {
            tracing::info!(
                visited = state.visited,
                succeeded = state.succeeded,
                "screenshot protection applied to descendant HWNDs",
            );
            if let Some(ref err) = state.first_failure {
                // Cross-process WebView2 children may legitimately
                // fail with access-denied — log at debug so it doesn't
                // pollute the warning channel during normal operation.
                tracing::debug!(
                    first_failure = %err,
                    failed = state.visited - state.succeeded,
                    "some descendants could not be set (often expected for cross-process WebView2 children)",
                );
            }
        }
        Ok(())
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

/// Apply the chosen `protection` to the parent HWND **and every
/// descendant** discovered via `EnumChildWindows`. Use this for the
/// Tauri main window where a WebView2 child tree must each carry the
/// affinity flag for capture protection to actually hold (see module
/// docs for the WebView2-propagation rationale).
///
/// The parent call is fail-closed: a parent error short-circuits.
/// Descendant failures are best-effort — they're logged via `tracing`
/// at `info` (counts) and `debug` (first error message) but do not
/// fail the call. Cross-process WebView2 children may legitimately
/// fail with access-denied; that's not a reason to drop the
/// protection on every other HWND.
///
/// On non-Windows targets this is a no-op stub returning `Ok(())`.
pub fn apply_to_hwnd_and_children(
    hwnd_isize: isize,
    protection: ScreenshotProtection,
) -> Result<()> {
    imp::apply_with_children(hwnd_isize, protection)
}
