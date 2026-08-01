//! A fail-closed admission gate for view-once pixels on Windows.
//!
//! `SetWindowDisplayAffinity` is not proof that a window can be safely
//! rendered. In particular, pre-2004 Windows can accept the request while
//! downgrading `WDA_EXCLUDEFROMCAPTURE` to `WDA_MONITOR`. Call
//! [`verify_capture_protection`] before unsealing or rendering sensitive
//! content; it checks the platform/window prerequisites, sets the requested
//! affinity, and verifies the exact value by readback.

/// Windows 10 version 2004 is build 19041.
pub const MINIMUM_CAPTURE_PROTECTION_BUILD: u32 = 19_041;

/// The exact affinity value required for a capture-protected view-once window.
pub const EXCLUDE_FROM_CAPTURE_AFFINITY: u32 = 0x11;

/// The prerequisites that must hold before requesting display affinity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaptureProtectionPrerequisites {
    pub os_build: u32,
    pub is_top_level_window: bool,
    pub dwm_is_composing: bool,
    pub is_layered_window: bool,
}

/// Why a surface is not safe to render on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaptureProtectionError {
    UnsupportedPlatform,
    UnsupportedWindowsBuild { observed: u32, minimum: u32 },
    NotTopLevelWindow,
    DwmNotComposing,
    LayeredWindow,
    SetAffinityFailed(String),
    ReadAffinityFailed(String),
    AffinityReadbackMismatch { requested: u32, observed: u32 },
}

impl core::fmt::Display for CaptureProtectionError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnsupportedPlatform => formatter.write_str("capture protection requires Windows"),
            Self::UnsupportedWindowsBuild { observed, minimum } => write!(
                formatter,
                "Windows build {observed} is older than required build {minimum}"
            ),
            Self::NotTopLevelWindow => {
                formatter.write_str("capture protection requires a top-level HWND")
            }
            Self::DwmNotComposing => formatter.write_str("DWM composition is not enabled"),
            Self::LayeredWindow => {
                formatter.write_str("layered windows cannot be capture protected")
            }
            Self::SetAffinityFailed(message) => {
                write!(formatter, "setting display affinity failed: {message}")
            }
            Self::ReadAffinityFailed(message) => {
                write!(formatter, "reading display affinity failed: {message}")
            }
            Self::AffinityReadbackMismatch {
                requested,
                observed,
            } => write!(
                formatter,
                "display affinity readback mismatch: requested 0x{requested:08X}, observed 0x{observed:08X}"
            ),
        }
    }
}

impl std::error::Error for CaptureProtectionError {}

/// Validate only the four conditions that must be true before an affinity
/// request. Kept platform-neutral so every rejection path has deterministic
/// tests, including on non-Windows CI.
pub fn validate_prerequisites(
    prerequisites: CaptureProtectionPrerequisites,
) -> Result<(), CaptureProtectionError> {
    if prerequisites.os_build < MINIMUM_CAPTURE_PROTECTION_BUILD {
        return Err(CaptureProtectionError::UnsupportedWindowsBuild {
            observed: prerequisites.os_build,
            minimum: MINIMUM_CAPTURE_PROTECTION_BUILD,
        });
    }
    if !prerequisites.is_top_level_window {
        return Err(CaptureProtectionError::NotTopLevelWindow);
    }
    if !prerequisites.dwm_is_composing {
        return Err(CaptureProtectionError::DwmNotComposing);
    }
    if prerequisites.is_layered_window {
        return Err(CaptureProtectionError::LayeredWindow);
    }
    Ok(())
}

/// Reject a successful set operation when the OS reports any value other than
/// `WDA_EXCLUDEFROMCAPTURE` on readback.
pub fn verify_affinity_readback(observed: u32) -> Result<(), CaptureProtectionError> {
    if observed != EXCLUDE_FROM_CAPTURE_AFFINITY {
        return Err(CaptureProtectionError::AffinityReadbackMismatch {
            requested: EXCLUDE_FROM_CAPTURE_AFFINITY,
            observed,
        });
    }
    Ok(())
}

/// Verify that `hwnd` is protected *before* rendering a sensitive pixel.
///
/// This deliberately treats a successful `SetWindowDisplayAffinity` call as
/// incomplete: the exact requested affinity must be observed on readback.
#[cfg(windows)]
pub fn verify_capture_protection(hwnd_isize: isize) -> Result<(), CaptureProtectionError> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GA_ROOT, GWL_EXSTYLE, GetAncestor, GetWindowDisplayAffinity, GetWindowLongPtrW,
        SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE, WS_EX_LAYERED,
    };

    let hwnd = HWND(hwnd_isize);
    let prerequisites = CaptureProtectionPrerequisites {
        os_build: windows_build()?,
        is_top_level_window: unsafe { GetAncestor(hwnd, GA_ROOT) == hwnd },
        dwm_is_composing: dwm_is_composing()?,
        is_layered_window: unsafe {
            GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_LAYERED != 0
        },
    };
    validate_prerequisites(prerequisites)?;

    unsafe {
        SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE).map_err(|error| {
            CaptureProtectionError::SetAffinityFailed(format_win32_error(error))
        })?;
        let mut observed = 0u32;
        GetWindowDisplayAffinity(hwnd, &mut observed).map_err(|error| {
            CaptureProtectionError::ReadAffinityFailed(format_win32_error(error))
        })?;
        verify_affinity_readback(observed)?;
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn verify_capture_protection(_hwnd_isize: isize) -> Result<(), CaptureProtectionError> {
    Err(CaptureProtectionError::UnsupportedPlatform)
}

#[cfg(windows)]
fn format_win32_error(error: windows::core::Error) -> String {
    format!("{} (HRESULT 0x{:08X})", error.message(), error.code().0)
}

#[cfg(windows)]
fn windows_build() -> Result<u32, CaptureProtectionError> {
    #[repr(C)]
    struct RtlOsVersionInfo {
        size: u32,
        major: u32,
        minor: u32,
        build: u32,
        platform_id: u32,
        service_pack: [u16; 128],
    }

    unsafe extern "system" {
        fn RtlGetVersion(version: *mut RtlOsVersionInfo) -> i32;
    }

    let mut version = RtlOsVersionInfo {
        size: core::mem::size_of::<RtlOsVersionInfo>() as u32,
        major: 0,
        minor: 0,
        build: 0,
        platform_id: 0,
        service_pack: [0; 128],
    };
    if unsafe { RtlGetVersion(&mut version) } != 0 {
        return Err(CaptureProtectionError::UnsupportedWindowsBuild {
            observed: 0,
            minimum: MINIMUM_CAPTURE_PROTECTION_BUILD,
        });
    }
    Ok(version.build)
}

#[cfg(windows)]
fn dwm_is_composing() -> Result<bool, CaptureProtectionError> {
    #[link(name = "dwmapi")]
    unsafe extern "system" {
        fn DwmIsCompositionEnabled(enabled: *mut i32) -> i32;
    }

    let mut enabled = 0;
    let result = unsafe { DwmIsCompositionEnabled(&mut enabled) };
    if result < 0 {
        return Err(CaptureProtectionError::DwmNotComposing);
    }
    Ok(enabled != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAFE: CaptureProtectionPrerequisites = CaptureProtectionPrerequisites {
        os_build: MINIMUM_CAPTURE_PROTECTION_BUILD,
        is_top_level_window: true,
        dwm_is_composing: true,
        is_layered_window: false,
    };

    #[test]
    fn accepts_only_a_window_that_meets_all_four_prerequisites() {
        assert_eq!(validate_prerequisites(SAFE), Ok(()));
    }

    #[test]
    fn rejects_each_missing_prerequisite_before_a_pixel_can_render() {
        assert!(matches!(
            validate_prerequisites(CaptureProtectionPrerequisites {
                os_build: 19040,
                ..SAFE
            }),
            Err(CaptureProtectionError::UnsupportedWindowsBuild { .. })
        ));
        assert_eq!(
            validate_prerequisites(CaptureProtectionPrerequisites {
                is_top_level_window: false,
                ..SAFE
            }),
            Err(CaptureProtectionError::NotTopLevelWindow)
        );
        assert_eq!(
            validate_prerequisites(CaptureProtectionPrerequisites {
                dwm_is_composing: false,
                ..SAFE
            }),
            Err(CaptureProtectionError::DwmNotComposing)
        );
        assert_eq!(
            validate_prerequisites(CaptureProtectionPrerequisites {
                is_layered_window: true,
                ..SAFE
            }),
            Err(CaptureProtectionError::LayeredWindow)
        );
    }

    #[test]
    fn silent_affinity_downgrade_is_not_success() {
        assert_eq!(
            verify_affinity_readback(1),
            Err(CaptureProtectionError::AffinityReadbackMismatch {
                requested: EXCLUDE_FROM_CAPTURE_AFFINITY,
                observed: 1,
            })
        );
    }
}
