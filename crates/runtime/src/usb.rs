//! USB device monitoring for video-capture-class arrivals.
//!
//! Spec: `docs/design/sender-keys.md` "USB device classes that trigger
//! rotation" subsection. Triggers a `Suspicious(UsbCaptureDevice)`
//! rotation when a UVC capture-class device is plugged in (cameras,
//! HDMI capture cards, etc.). Other USB classes (HID, mass storage,
//! audio, comms, printers, hubs, smart-card readers) explicitly do
//! NOT trigger rotation per the design table.
//!
//! ## What "capture-class" means here
//!
//! - **USB-IF base class `0x0E`** (Video).
//! - **At least one interface with subclass `0x02`** (`SC_VIDEOSTREAMING`).
//! - **At least one Input Terminal** in the VideoControl interface
//!   whose `wTerminalType` is in the Input Terminal range
//!   (`0x0200..=0x02FF` — includes `ITT_CAMERA = 0x0201` and
//!   `ITT_MEDIA_TRANSPORT_INPUT = 0x0202`) **or** the External
//!   Terminal range (`0x0400..=0x04FF` — includes
//!   `COMPOSITE_CONNECTOR = 0x0401`, `SVIDEO_CONNECTOR = 0x0402`,
//!   `COMPONENT_CONNECTOR = 0x0403`).
//!
//! A device exposing only video-control or video-output descriptors
//! (e.g. a video display, or a webcam in loopback mode) does NOT
//! trigger — its `input_terminal_types` will be empty / output-only.
//!
//! ## Monitor (Windows only)
//!
//! [`UsbMonitor`] runs a hidden message-only window on a dedicated
//! thread, registers for `KSCATEGORY_CAPTURE` device-interface
//! arrivals via `RegisterDeviceNotificationW`, and invokes the
//! user-supplied callback on each `WM_DEVICECHANGE / DBT_DEVICEARRIVAL`.
//! On non-Windows targets [`UsbMonitor::start`] is a no-op stub so
//! the rest of the binary compiles on Linux / macOS dev hosts.
//!
//! Filtering on Windows is delegated to the OS's class-interface
//! registration: only devices that registered under
//! `KSCATEGORY_CAPTURE` (cameras, capture cards) fire arrival events.
//! The pure [`is_capture_device`] function below is exposed for
//! future deeper-filtering work and Linux-side test coverage — it
//! isn't on the hot path of the Windows monitor in v1 alpha.

use thiserror::Error;

/// Simplified USB device descriptor — just the fields the
/// capture-detection filter needs. Real descriptor parsing on Windows
/// builds one of these from the SetupAPI / WinUSB interfaces.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UsbDeviceDescriptor {
    /// `bDeviceClass` (composite devices) or the most-prominent
    /// `bInterfaceClass`. We use this as the gating "is it a video
    /// device at all?" check — design table requires class `0x0E`.
    pub base_class: u8,
    /// True if the device exposes at least one interface with
    /// `bInterfaceClass = 0x0E` and `bInterfaceSubClass = 0x02`
    /// (`SC_VIDEOSTREAMING`).
    pub video_streaming_present: bool,
    /// `wTerminalType` values for every Input Terminal in the
    /// device's VideoControl interface. A capture device has at
    /// least one entry in the Input Terminal (`0x0200..=0x02FF`) or
    /// External Terminal (`0x0400..=0x04FF`) ranges.
    pub input_terminal_types: Vec<u16>,
}

/// Returns `true` iff the descriptor names a video-capture device per
/// the design doc's UVC + Input Terminal rule.
pub fn is_capture_device(d: &UsbDeviceDescriptor) -> bool {
    if d.base_class != 0x0E {
        return false;
    }
    if !d.video_streaming_present {
        return false;
    }
    d.input_terminal_types
        .iter()
        .any(|&t| (0x0200..=0x02FF).contains(&t) || (0x0400..=0x04FF).contains(&t))
}

#[derive(Debug, Error)]
pub enum UsbMonitorError {
    #[error("USB monitor Win32 error: {0}")]
    Win32(String),
    #[error("USB monitor not supported on this platform")]
    Unsupported,
}

pub type Result<T> = core::result::Result<T, UsbMonitorError>;

/// Callback signature: invoked on each capture-class device arrival.
/// `Send + Sync + 'static` because the Windows monitor calls it from
/// a dedicated thread.
pub type ArrivalCallback = Box<dyn Fn() + Send + Sync + 'static>;

/// `KSCATEGORY_CAPTURE` GUID
/// (`{65E8773D-8F56-11D0-A3B9-00A0C9223196}`). Webcams, USB capture
/// cards, and HDMI capture devices register under this category.
/// Exposed as bytes here for use both by the Windows monitor and by
/// any future caller that wants the raw GUID.
pub const KSCATEGORY_CAPTURE_GUID_BYTES: [u8; 16] = [
    0x3D, 0x77, 0xE8, 0x65, // Data1 little-endian: 0x65E8773D
    0x56, 0x8F, // Data2: 0x8F56
    0xD0, 0x11, // Data3: 0x11D0
    0xA3, 0xB9, 0x00, 0xA0, 0xC9, 0x22, 0x31, 0x96, // Data4
];

#[cfg(windows)]
mod imp {
    use super::{ArrivalCallback, Result, UsbMonitorError, KSCATEGORY_CAPTURE_GUID_BYTES};
    use std::sync::Arc;
    use std::thread::JoinHandle;
    use windows::core::{w, GUID, PCWSTR};
    use windows::Win32::Foundation::{HMODULE, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
        RegisterClassExW, RegisterDeviceNotificationW, SetWindowLongPtrW,
        TranslateMessage, UnregisterClassW, UnregisterDeviceNotification,
        DBT_DEVICEARRIVAL, DBT_DEVTYP_DEVICEINTERFACE, DEVICE_NOTIFY_WINDOW_HANDLE,
        DEV_BROADCAST_DEVICEINTERFACE_W, DEV_BROADCAST_HDR, GWLP_USERDATA, HWND_MESSAGE,
        MSG, REGISTER_NOTIFICATION_FLAGS, WINDOW_EX_STYLE, WM_DESTROY, WM_DEVICECHANGE,
        WM_QUIT, WNDCLASSEXW,
    };

    fn ksc_capture_guid() -> GUID {
        let b = KSCATEGORY_CAPTURE_GUID_BYTES;
        GUID {
            data1: u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            data2: u16::from_le_bytes([b[4], b[5]]),
            data3: u16::from_le_bytes([b[6], b[7]]),
            data4: [b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]],
        }
    }

    /// Per-window state kept alive in HWND user data; reclaimed in
    /// `WM_DESTROY`.
    struct WindowState {
        callback: Arc<ArrivalCallback>,
    }

    pub(super) struct Monitor {
        join: Option<JoinHandle<()>>,
        thread_id: u32,
        // Both fields above let us PostMessage(WM_QUIT) on drop.
    }

    impl Drop for Monitor {
        fn drop(&mut self) {
            // Post a WM_QUIT to the monitor thread; its message loop
            // exits, the thread's cleanup destroys the window and
            // unregisters the class.
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW(
                    self.thread_id,
                    WM_QUIT,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
            if let Some(j) = self.join.take() {
                let _ = j.join();
            }
        }
    }

    pub(super) fn start(callback: ArrivalCallback) -> Result<Monitor> {
        let cb = Arc::new(callback);
        let cb_for_thread = cb.clone();
        let (tx, rx) = std::sync::mpsc::channel::<Result<u32>>();

        let join = std::thread::Builder::new()
            .name("dpc-usb-monitor".into())
            .spawn(move || {
                // Run on this thread: register class, create window,
                // register device notifications, run the pump.
                let res = run_pump(cb_for_thread);
                let _ = tx.send(res.map(|_| current_thread_id()));
            })
            .map_err(|e| UsbMonitorError::Win32(format!("spawn monitor thread: {e}")))?;

        // Wait for the thread to either finish startup (returning its
        // id) or report an error before the message pump began.
        let thread_id = match rx.recv() {
            Ok(Ok(id)) => id,
            Ok(Err(e)) => {
                let _ = join.join();
                return Err(e);
            }
            Err(_) => {
                let _ = join.join();
                return Err(UsbMonitorError::Win32(
                    "monitor thread exited without status".into(),
                ));
            }
        };

        // Bind cb so it isn't dropped — the window state already holds
        // a clone, but tying it to Monitor's lifetime keeps the type
        // expressive.
        std::mem::drop(cb);

        Ok(Monitor {
            join: Some(join),
            thread_id,
        })
    }

    fn current_thread_id() -> u32 {
        unsafe { windows::Win32::System::Threading::GetCurrentThreadId() }
    }

    /// Owning handle bundle. Constructed on the monitor thread, lives
    /// only there. Dropping it cleans up the window + class.
    struct PumpState {
        hwnd: HWND,
        notify_handle: windows::Win32::UI::WindowsAndMessaging::HDEVNOTIFY,
        class_atom: u16,
        hinstance: HMODULE,
        // Stored boxed so we can null the user-data pointer on
        // destroy without freeing twice.
        _state: *mut WindowState,
    }

    impl Drop for PumpState {
        fn drop(&mut self) {
            unsafe {
                let _ = UnregisterDeviceNotification(self.notify_handle);
                let _ = DestroyWindow(self.hwnd);
                let class_name = make_class_name();
                let _ = UnregisterClassW(PCWSTR(class_name.as_ptr()), self.hinstance);
                if !self._state.is_null() {
                    drop(Box::from_raw(self._state));
                }
            }
        }
    }

    fn make_class_name() -> Vec<u16> {
        // UTF-16 NUL-terminated.
        "DPC_UsbMonitor_v1\0".encode_utf16().collect()
    }

    fn run_pump(callback: Arc<ArrivalCallback>) -> Result<()> {
        unsafe {
            let hinstance = GetModuleHandleW(PCWSTR::null())
                .map_err(|e| UsbMonitorError::Win32(format!("GetModuleHandleW: {e}")))?;
            let class_name = make_class_name();
            // windows 0.56.0: `WNDCLASSEXW.hInstance` is `HINSTANCE`,
            // and `GetModuleHandleW` returns `HMODULE`. The two are
            // distinct tuple structs in this version; `.into()` calls
            // the upstream `From<HMODULE> for HINSTANCE` impl.
            let wnd_class = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(wnd_proc),
                hInstance: hinstance.into(),
                lpszClassName: PCWSTR(class_name.as_ptr()),
                ..Default::default()
            };
            let atom = RegisterClassExW(&wnd_class);
            if atom == 0 {
                return Err(UsbMonitorError::Win32(
                    "RegisterClassExW returned 0".into(),
                ));
            }

            let state = Box::into_raw(Box::new(WindowState {
                callback: callback.clone(),
            }));

            // windows 0.56.0: `CreateWindowExW` returns `HWND` directly
            // (not `Result<HWND>`); a NULL/zero return signals failure
            // and the caller is expected to read `GetLastError`.
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                PCWSTR(class_name.as_ptr()),
                w!("DPC USB Monitor"),
                windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(0),
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                None,
                hinstance,
                None,
            );
            if hwnd.0 == 0 {
                let err = windows::core::Error::from_win32();
                return Err(UsbMonitorError::Win32(format!(
                    "CreateWindowExW: {} (HRESULT 0x{:08X})",
                    err.message(),
                    err.code().0
                )));
            }

            // Stash pointer to our WindowState so WndProc can find it.
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize);

            // Register for KSCATEGORY_CAPTURE arrivals on this HWND.
            // windows 0.56.0: `DBT_DEVTYP_DEVICEINTERFACE` is the
            // typed wrapper `DEV_BROADCAST_HDR_DEVICE_TYPE(pub u32)`;
            // the struct field expects raw `u32`, so unwrap with `.0`.
            let mut filter = DEV_BROADCAST_DEVICEINTERFACE_W {
                dbcc_size: std::mem::size_of::<DEV_BROADCAST_DEVICEINTERFACE_W>() as u32,
                dbcc_devicetype: DBT_DEVTYP_DEVICEINTERFACE.0,
                dbcc_reserved: 0,
                dbcc_classguid: ksc_capture_guid(),
                dbcc_name: [0],
            };
            let notify_handle = RegisterDeviceNotificationW(
                hwnd,
                &mut filter as *mut _ as *mut _,
                REGISTER_NOTIFICATION_FLAGS(DEVICE_NOTIFY_WINDOW_HANDLE.0),
            )
            .map_err(|e| {
                UsbMonitorError::Win32(format!("RegisterDeviceNotificationW: {e}"))
            })?;

            // Bundle for RAII cleanup on pump exit.
            let _bundle = PumpState {
                hwnd,
                notify_handle,
                class_atom: atom,
                hinstance,
                _state: state,
            };

            // Run message pump until WM_QUIT.
            let mut msg = MSG::default();
            loop {
                let r = GetMessageW(&mut msg, None, 0, 0);
                if r.0 <= 0 {
                    break;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        Ok(())
    }

    /// `WndProc` for the hidden monitor window. Handles
    /// `WM_DEVICECHANGE / DBT_DEVICEARRIVAL` and forwards to the
    /// stored callback.
    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_DEVICECHANGE && wparam.0 as u32 == DBT_DEVICEARRIVAL {
            let hdr = lparam.0 as *const DEV_BROADCAST_HDR;
            // windows 0.56.0 quirk: `DEV_BROADCAST_HDR.dbch_devicetype`
            // is the typed wrapper `DEV_BROADCAST_HDR_DEVICE_TYPE`,
            // whereas `DEV_BROADCAST_DEVICEINTERFACE_W.dbcc_devicetype`
            // (used at registration time above) is raw u32. Same Win32
            // constant, two struct field types — compare without `.0`
            // here, with `.0` there.
            if !hdr.is_null() && (*hdr).dbch_devicetype == DBT_DEVTYP_DEVICEINTERFACE {
                let user_data = windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
                    hwnd,
                    GWLP_USERDATA,
                );
                if user_data != 0 {
                    let state = &*(user_data as *const WindowState);
                    let cb = state.callback.clone();
                    cb();
                }
            }
            return LRESULT(0);
        }
        if msg == WM_DESTROY {
            // Don't free WindowState here — PumpState's Drop owns it.
            // Just signal the pump to exit.
            windows::Win32::UI::WindowsAndMessaging::PostQuitMessage(0);
            return LRESULT(0);
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{ArrivalCallback, Result};

    /// Non-Windows stub. Returns a monitor that holds the callback
    /// and never fires it. A real macOS / Linux impl would hook
    /// `IOHIDManager` (macOS) / `udev` (Linux); both are out of
    /// scope for v1 alpha (Windows-only target).
    pub(super) struct Monitor {
        _callback: ArrivalCallback,
    }

    pub(super) fn start(callback: ArrivalCallback) -> Result<Monitor> {
        Ok(Monitor {
            _callback: callback,
        })
    }
}

/// USB capture-device monitor. Construction registers the callback;
/// dropping the monitor unregisters and (on Windows) tears down the
/// hidden window + message-pump thread.
pub struct UsbMonitor {
    _inner: imp::Monitor,
}

impl UsbMonitor {
    /// Start monitoring for capture-class USB device arrivals.
    /// `callback` is invoked once per arrival event.
    pub fn start(callback: ArrivalCallback) -> Result<Self> {
        Ok(UsbMonitor {
            _inner: imp::start(callback)?,
        })
    }
}
