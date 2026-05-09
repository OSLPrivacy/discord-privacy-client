use runtime::{apply_to_hwnd, ScreenshotProtection};

// On Linux / macOS, `apply_to_hwnd` is a no-op stub. We exercise the
// no-op path here to lock its behaviour in across cfg permutations.
//
// Win32 behaviour is documented in `runtime::screenshot` and verified
// by the user on a Windows host — there is no automated test for the
// actual SetWindowDisplayAffinity call (capture protection is OS-level
// and can only be confirmed visually with a screenshot tool).

#[cfg(not(windows))]
#[test]
fn linux_macos_stub_is_a_noop_for_both_states() {
    apply_to_hwnd(0, ScreenshotProtection::On).expect("no-op On");
    apply_to_hwnd(0, ScreenshotProtection::Off).expect("no-op Off");
    // Any HWND-shaped value is accepted on the stub.
    apply_to_hwnd(0xDEADBEEF, ScreenshotProtection::On).expect("arbitrary hwnd");
    apply_to_hwnd(-1, ScreenshotProtection::Off).expect("negative hwnd value");
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
    let _ = format!("{:?}", on); // Debug impl exists
}
