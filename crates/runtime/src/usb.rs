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
//! It also registers for volume notifications and forwards
//! `DBT_DEVICEREMOVECOMPLETE` for `GUID_DEVINTERFACE_VOLUME` to a separate callback.
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

/// A Windows volume device-interface path.
///
/// Windows may assign a different drive letter when the same device returns,
/// so a drive letter must never be used as the dead-man switch's device key.
/// The `GUID_DEVINTERFACE_VOLUME` notification supplies this symbolic-link
/// path instead.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StorageDeviceId(String);

impl StorageDeviceId {
    /// Creates an identity from a volume device-interface symbolic-link path.
    /// A drive letter is not valid: Windows can reassign one whenever a
    /// volume is mounted. Device-interface paths begin with `\\?\`.
    pub fn new(device_interface_path: impl Into<String>) -> Option<Self> {
        let device_interface_path = device_interface_path.into();
        device_interface_path
            .starts_with(r"\\?\")
            .then_some(Self(device_interface_path))
    }

    /// The opaque Windows device-interface symbolic-link path.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Callback invoked when Windows reports that a mounted volume was removed.
/// This is deliberately separate from [`ArrivalCallback`]: capture-device
/// arrivals continue to drive rotation, while a volume removal is consumed by
/// the dead-man switch with a stable, non-drive-letter identity.
pub type VolumeRemovalCallback = Box<dyn Fn(StorageDeviceId) + Send + Sync + 'static>;

/// USB monitor events forwarded from the platform-specific message pump.
///
/// Keeping this event boundary platform-independent lets callers and tests
/// exercise the same callback routing without manufacturing a Win32 message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UsbMonitorEvent {
    /// A device registered in the capture interface class arrived.
    CaptureArrival,
    /// A volume interface reappeared. This is not a capture-device arrival;
    /// retaining the event keeps decoding explicit while avoiding a false
    /// rotation trigger when Windows assigns a new drive letter.
    StorageDeviceArrived(StorageDeviceId),
    /// A mounted volume was removed, identified by its Windows device
    /// interface path rather than its reassignable drive letter.
    StorageDeviceRemoved(StorageDeviceId),
}

/// Decodes the subset of `WM_DEVICECHANGE` messages consumed by this monitor.
///
/// The numeric values are Win32's `WM_DEVICECHANGE`, `DBT_DEVICEARRIVAL`,
/// `DBT_DEVICEREMOVECOMPLETE`, and `DBT_DEVTYP_DEVICEINTERFACE` respectively.
/// Keeping this decoder independent of the
/// Windows bindings makes the real message routing testable on every target.
pub fn usb_monitor_event_from_device_change(
    message: u32,
    change: u32,
    device_type: u32,
    device_interface_path: Option<&str>,
) -> Option<UsbMonitorEvent> {
    match (message, change, device_type) {
        (0x0219, 0x8000, 0x0005) => device_interface_path
            .and_then(StorageDeviceId::new)
            .map(UsbMonitorEvent::StorageDeviceArrived)
            .or(Some(UsbMonitorEvent::CaptureArrival)),
        (0x0219, 0x8004, 0x0005) => device_interface_path
            .and_then(StorageDeviceId::new)
            .map(UsbMonitorEvent::StorageDeviceRemoved),
        _ => None,
    }
}

/// The two independent consumers of USB monitor events.
pub struct UsbMonitorCallbacks {
    arrival: ArrivalCallback,
    volume_removal: VolumeRemovalCallback,
}

impl UsbMonitorCallbacks {
    /// Construct the callbacks used by [`UsbMonitor`].
    pub fn new(arrival: ArrivalCallback, volume_removal: VolumeRemovalCallback) -> Self {
        Self {
            arrival,
            volume_removal,
        }
    }

    /// Forward a monitor event to its dedicated callback.
    pub fn dispatch(&self, event: UsbMonitorEvent) {
        match event {
            UsbMonitorEvent::CaptureArrival => (self.arrival)(),
            UsbMonitorEvent::StorageDeviceArrived(_) => {},
            UsbMonitorEvent::StorageDeviceRemoved(device_id) => (self.volume_removal)(device_id),
        }
    }
}

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

/// `GUID_DEVINTERFACE_VOLUME` (`{53F5630D-B6BF-11D0-94F2-00A0C91EFB8B}`).
/// Registering this interface class provides an opaque device path in the
/// notification payload; `DBT_DEVTYP_VOLUME` only supplies a drive-letter
/// bitmask, which is not a stable identity.
pub const GUID_DEVINTERFACE_VOLUME_BYTES: [u8; 16] = [
    0x0D, 0x63, 0xF5, 0x53, // Data1 little-endian: 0x53F5630D
    0xBF, 0xB6, // Data2: 0xB6BF
    0xD0, 0x11, // Data3: 0x11D0
    0x94, 0xF2, 0x00, 0xA0, 0xC9, 0x1E, 0xFB, 0x8B, // Data4
];

#[cfg(windows)]
mod imp {
    use super::{
        usb_monitor_event_from_device_change, Result, UsbMonitorCallbacks, UsbMonitorError,
        UsbMonitorEvent, GUID_DEVINTERFACE_VOLUME_BYTES, KSCATEGORY_CAPTURE_GUID_BYTES,
    };
    use std::sync::Arc;
    use std::thread::JoinHandle;
    use windows::core::{w, GUID, PCWSTR};
    use windows::Win32::Foundation::{HMODULE, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
        RegisterClassExW, RegisterDeviceNotificationW, SetWindowLongPtrW, TranslateMessage,
        UnregisterClassW, UnregisterDeviceNotification, DBT_DEVTYP_DEVICEINTERFACE,
        DEVICE_NOTIFY_WINDOW_HANDLE, DEV_BROADCAST_DEVICEINTERFACE_W, DEV_BROADCAST_HDR,
        GWLP_USERDATA, HWND_MESSAGE, MSG, REGISTER_NOTIFICATION_FLAGS, WINDOW_EX_STYLE, WM_DESTROY,
        WM_DEVICECHANGE, WM_QUIT, WNDCLASSEXW,
    };

    fn ksc_capture_guid() -> GUID {
        guid_from_le_bytes(KSCATEGORY_CAPTURE_GUID_BYTES)
    }

    fn volume_interface_guid() -> GUID {
        guid_from_le_bytes(GUID_DEVINTERFACE_VOLUME_BYTES)
    }

    fn guid_from_le_bytes(b: [u8; 16]) -> GUID {
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
        callbacks: Arc<UsbMonitorCallbacks>,
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

    pub(super) fn start(callbacks: UsbMonitorCallbacks) -> Result<Monitor> {
        let cb = Arc::new(callbacks);
        let cb_for_thread = cb.clone();
        let (tx, rx) = std::sync::mpsc::channel::<Result<u32>>();

        let tx_for_failure = tx.clone();
        let join = std::thread::Builder::new()
            .name("dpc-usb-monitor".into())
            .spawn(move || {
                // Run on this thread: register class, create window,
                // register device notifications, run the pump.
                // `run_pump` reports its own readiness through `tx` once
                // setup is done and before it blocks in `GetMessageW`.
                let res = run_pump(cb_for_thread, tx);
                // Only reachable with an `Err` when setup failed before
                // readiness was reported, in which case `start` is still
                // waiting on `recv`. Once readiness has been sent, `start`
                // has returned and dropped the receiver, so this send fails
                // and is correctly ignored.
                if let Err(e) = res {
                    let _ = tx_for_failure.send(Err(e));
                }
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
        volume_notify_handle: windows::Win32::UI::WindowsAndMessaging::HDEVNOTIFY,
        hinstance: HMODULE,
        // Stored boxed so we can null the user-data pointer on
        // destroy without freeing twice.
        _state: *mut WindowState,
    }

    impl Drop for PumpState {
        fn drop(&mut self) {
            unsafe {
                let _ = UnregisterDeviceNotification(self.notify_handle);
                let _ = UnregisterDeviceNotification(self.volume_notify_handle);
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

    /// Set up the hidden message window and device-notification
    /// registration, report readiness on `ready`, then pump messages
    /// until `WM_QUIT`.
    ///
    /// Reporting readiness *before* entering the pump is load-bearing.
    /// The previous version sent the thread id only after `run_pump`
    /// returned, so `start` blocked on `recv` waiting for a pump that
    /// exits only on the `WM_QUIT` that `Monitor::drop` posts -- and
    /// `drop` cannot run until `start` returns. `UsbMonitor::start` could
    /// therefore never return on Windows; it hung the CI job for 45
    /// minutes ("has been running for over 60 seconds") once the test
    /// binary ahead of it stopped failing and let this one run at all.
    ///
    /// The send has to come after `CreateWindowExW`: creating a window is
    /// what gives this thread a message queue, and without one the
    /// `PostThreadMessageW(WM_QUIT)` in `drop` would be dropped on the
    /// floor and the pump would never be told to stop.
    fn run_pump(
        callbacks: Arc<UsbMonitorCallbacks>,
        ready: std::sync::mpsc::Sender<Result<u32>>,
    ) -> Result<()> {
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
                return Err(UsbMonitorError::Win32("RegisterClassExW returned 0".into()));
            }

            let state = Box::into_raw(Box::new(WindowState {
                callbacks: callbacks.clone(),
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
            .map_err(|e| UsbMonitorError::Win32(format!("RegisterDeviceNotificationW: {e}")))?;

            // Unlike DBT_DEVTYP_VOLUME, GUID_DEVINTERFACE_VOLUME supplies the
            // volume's symbolic-link path, not a reassignable drive letter.
            let mut volume_filter = DEV_BROADCAST_DEVICEINTERFACE_W {
                dbcc_size: std::mem::size_of::<DEV_BROADCAST_DEVICEINTERFACE_W>() as u32,
                dbcc_devicetype: DBT_DEVTYP_DEVICEINTERFACE.0,
                dbcc_reserved: 0,
                dbcc_classguid: volume_interface_guid(),
                dbcc_name: [0],
            };
            let volume_notify_handle = match RegisterDeviceNotificationW(
                hwnd,
                &mut volume_filter as *mut _ as *mut _,
                REGISTER_NOTIFICATION_FLAGS(DEVICE_NOTIFY_WINDOW_HANDLE.0),
            ) {
                Ok(handle) => handle,
                Err(e) => {
                    let _ = UnregisterDeviceNotification(notify_handle);
                    return Err(UsbMonitorError::Win32(format!(
                        "RegisterDeviceNotificationW(volume interface): {e}"
                    )));
                }
            };

            // Bundle for RAII cleanup on pump exit.
            let _bundle = PumpState {
                hwnd,
                notify_handle,
                volume_notify_handle,
                hinstance,
                _state: state,
            };

            // Startup is complete and this thread owns a message queue,
            // so `Monitor::drop` can now reach us with WM_QUIT. A send
            // error means the caller gave up waiting; unwind so `_bundle`
            // tears the window down rather than pumping forever.
            if ready.send(Ok(current_thread_id())).is_err() {
                return Ok(());
            }

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
    /// `WM_DEVICECHANGE` and forwards capture arrivals and storage removals to
    /// their respective callbacks.
    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_DEVICECHANGE {
            let hdr = lparam.0 as *const DEV_BROADCAST_HDR;
            // windows 0.56.0 quirk: `DEV_BROADCAST_HDR.dbch_devicetype`
            // is the typed wrapper `DEV_BROADCAST_HDR_DEVICE_TYPE`,
            // whereas `DEV_BROADCAST_DEVICEINTERFACE_W.dbcc_devicetype`
            // (used at registration time above) is raw u32. Decode the
            // message using the raw value here.
            if !hdr.is_null() {
                let user_data =
                    windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(hwnd, GWLP_USERDATA);
                if user_data != 0 {
                    if let Some(event) = usb_monitor_event_from_device_change(
                        msg,
                        wparam.0 as u32,
                        (*hdr).dbch_devicetype.0,
                        device_interface_path(hdr).as_deref(),
                    ) {
                        let state = &*(user_data as *const WindowState);
                        state.callbacks.dispatch(event);
                    }
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

    /// Extract the NUL-terminated device-interface path from a device-change
    /// payload. The path belongs to the message and is copied before the
    /// callback returns.
    unsafe fn device_interface_path(hdr: *const DEV_BROADCAST_HDR) -> Option<String> {
        if (*hdr).dbch_devicetype.0 != DBT_DEVTYP_DEVICEINTERFACE.0 {
            return None;
        }

        let fixed_size =
            std::mem::size_of::<DEV_BROADCAST_DEVICEINTERFACE_W>() - std::mem::size_of::<u16>();
        let payload_size = (*hdr).dbch_size as usize;
        if payload_size <= fixed_size {
            return None;
        }

        let name_len = (payload_size - fixed_size) / std::mem::size_of::<u16>();
        let interface = hdr.cast::<DEV_BROADCAST_DEVICEINTERFACE_W>();
        let name = std::slice::from_raw_parts((*interface).dbcc_name.as_ptr(), name_len);
        let nul = name
            .iter()
            .position(|&unit| unit == 0)
            .unwrap_or(name.len());
        String::from_utf16(&name[..nul]).ok()
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{Result, UsbMonitorCallbacks};

    /// Non-Windows stub. Returns a monitor that holds the callback
    /// and never fires it. A real macOS / Linux impl would hook
    /// `IOHIDManager` (macOS) / `udev` (Linux); both are out of
    /// scope for v1 alpha (Windows-only target).
    pub(super) struct Monitor {
        _callbacks: UsbMonitorCallbacks,
    }

    pub(super) fn start(callbacks: UsbMonitorCallbacks) -> Result<Monitor> {
        Ok(Monitor {
            _callbacks: callbacks,
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
        Self::start_with_volume_removal(callback, Box::new(|_| {}))
    }

    /// Start monitoring for capture arrivals and mounted-volume removals.
    ///
    /// The capture callback keeps the established rotation behaviour; the
    /// removal callback is reserved for the dead-man switch.
    pub fn start_with_volume_removal(
        arrival: ArrivalCallback,
        volume_removal: VolumeRemovalCallback,
    ) -> Result<Self> {
        Ok(UsbMonitor {
            _inner: imp::start(UsbMonitorCallbacks::new(arrival, volume_removal))?,
        })
    }
}
