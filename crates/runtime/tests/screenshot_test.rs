use runtime::ScreenshotProtection;
#[cfg(not(windows))]
use runtime::{apply_to_hwnd, apply_to_hwnd_and_children, CaptureProtectionError, ScreenshotError};

// On Linux / macOS, `apply_to_hwnd` and `apply_to_hwnd_and_children`
// can only disable protection. Asking to enable it fails closed because no
// compositor primitive can prove capture exclusion on these targets.
//
// Win32 behaviour is documented in `runtime::screenshot` and verified
// by the user on a Windows host — there is no automated test for the
// actual SetWindowDisplayAffinity call or for `EnumChildWindows`
// recursion across the WebView2 child tree (capture protection is
// OS-level and can only be confirmed visually with a screenshot tool).

#[cfg(not(windows))]
#[test]
fn linux_macos_stub_disables_but_refuses_to_claim_protection() {
    apply_to_hwnd(0, ScreenshotProtection::Off).expect("no-op Off");
    apply_to_hwnd(-1, ScreenshotProtection::Off).expect("negative hwnd value");
    assert!(matches!(
        apply_to_hwnd(0, ScreenshotProtection::On),
        Err(ScreenshotError::CaptureProtection(
            CaptureProtectionError::UnsupportedPlatform
        ))
    ));
    assert!(apply_to_hwnd(0xDEADBEEF, ScreenshotProtection::On).is_err());
}

#[cfg(not(windows))]
#[test]
fn linux_macos_stub_with_children_disables_but_refuses_to_claim_protection() {
    apply_to_hwnd_and_children(0, ScreenshotProtection::Off).expect("no-op Off");
    apply_to_hwnd_and_children(-1, ScreenshotProtection::Off).expect("negative hwnd value");
    assert!(matches!(
        apply_to_hwnd_and_children(0, ScreenshotProtection::On),
        Err(ScreenshotError::CaptureProtection(
            CaptureProtectionError::UnsupportedPlatform
        ))
    ));
    assert!(apply_to_hwnd_and_children(0xDEADBEEF, ScreenshotProtection::On).is_err());
}

#[test]
fn protection_states_distinct() {
    assert_ne!(ScreenshotProtection::On, ScreenshotProtection::Off);
}

#[test]
fn protection_states_copy_eq_debug() {
    let on = ScreenshotProtection::On;
    let copy = on;
    assert_eq!(on, copy);
    let _ = format!("{on:?}"); // Debug impl exists
}
