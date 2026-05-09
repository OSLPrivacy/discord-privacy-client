//! Runtime support for OS-level signals that drive cryptographic
//! decisions in the client.
//!
//! Per `docs/design/sender-keys.md` "Suspicious-event auto-rotation"
//! and `docs/design/build-order.md` Group A:
//!
//! - [`rotation`] — `RotationController`: state machine that decides
//!   when to rotate sender keys, fed by message-sent / suspicious-
//!   event / membership / recipient signals.
//! - [`clock`] — `Clock` trait + `SystemClock` + test-only
//!   `MockClock`. Lets the rotation controller (and future timers) be
//!   driven deterministically in unit tests.
//!
//! Subsequent A-group layers will add USB monitoring (`A3`),
//! recorder-process scanning (`A4`), and screenshot resistance (`A2`,
//! Win32-only — lives in `src-tauri`).

pub mod clock;
pub mod recorder;
pub mod revalidation;
pub mod rotation;
pub mod screenshot;
pub mod usb;

pub use clock::{Clock, MockClock, SystemClock};
pub use recorder::{
    match_recorders, scan as scan_for_recorders, snapshot_running_processes,
    DetectionCallback, RecorderScanError, RecorderScanner, RecorderScannerConfig,
    RECORDER_PROCESS_NAMES,
};
pub use revalidation::{
    render_decision, ContentKind, Probe, ProbeError, RenderDecision,
    RevalidationConfig, RevalidationLoop, TransitionCallback, WrappedKeyState,
};
pub use rotation::{
    RotationConfig, RotationController, RotationReason, SuspiciousEventKind,
};
pub use screenshot::{
    apply_to_hwnd, apply_to_hwnd_and_children, ScreenshotError, ScreenshotProtection,
};
pub use usb::{
    is_capture_device, ArrivalCallback, UsbDeviceDescriptor, UsbMonitor, UsbMonitorError,
};
