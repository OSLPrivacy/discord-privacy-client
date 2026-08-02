use runtime::{
    is_capture_device, usb_monitor_event_from_device_change, ArrivalCallback, UsbDeviceDescriptor,
    UsbMonitor, UsbMonitorCallbacks,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

// ---- pure filter: capture cases ----

#[test]
fn webcam_with_camera_input_terminal_is_capture() {
    let d = UsbDeviceDescriptor {
        base_class: 0x0E,
        video_streaming_present: true,
        input_terminal_types: vec![0x0201], // ITT_CAMERA
    };
    assert!(is_capture_device(&d));
}

#[test]
fn webcam_with_media_transport_input_terminal_is_capture() {
    let d = UsbDeviceDescriptor {
        base_class: 0x0E,
        video_streaming_present: true,
        input_terminal_types: vec![0x0202], // ITT_MEDIA_TRANSPORT_INPUT
    };
    assert!(is_capture_device(&d));
}

#[test]
fn hdmi_capture_card_with_composite_external_terminal_is_capture() {
    let d = UsbDeviceDescriptor {
        base_class: 0x0E,
        video_streaming_present: true,
        input_terminal_types: vec![0x0401], // COMPOSITE_CONNECTOR
    };
    assert!(is_capture_device(&d));
}

#[test]
fn capture_card_with_svideo_or_component_external_terminal_is_capture() {
    let d = UsbDeviceDescriptor {
        base_class: 0x0E,
        video_streaming_present: true,
        input_terminal_types: vec![0x0402, 0x0403], // S-Video, component
    };
    assert!(is_capture_device(&d));
}

#[test]
fn capture_with_multiple_terminals_one_input_is_capture() {
    let d = UsbDeviceDescriptor {
        base_class: 0x0E,
        video_streaming_present: true,
        input_terminal_types: vec![0x0301, 0x0201], // one output + one input
    };
    assert!(is_capture_device(&d));
}

// ---- pure filter: non-capture cases ----

#[test]
fn hid_device_is_not_capture() {
    let d = UsbDeviceDescriptor {
        base_class: 0x03,
        video_streaming_present: false,
        input_terminal_types: vec![],
    };
    assert!(!is_capture_device(&d));
}

#[test]
fn mass_storage_device_is_not_capture() {
    let d = UsbDeviceDescriptor {
        base_class: 0x08,
        video_streaming_present: false,
        input_terminal_types: vec![],
    };
    assert!(!is_capture_device(&d));
}

#[test]
fn audio_device_is_not_capture() {
    let d = UsbDeviceDescriptor {
        base_class: 0x01,
        video_streaming_present: false,
        input_terminal_types: vec![],
    };
    assert!(!is_capture_device(&d));
}

#[test]
fn hub_device_is_not_capture() {
    let d = UsbDeviceDescriptor {
        base_class: 0x09,
        video_streaming_present: false,
        input_terminal_types: vec![],
    };
    assert!(!is_capture_device(&d));
}

#[test]
fn smart_card_reader_is_not_capture() {
    let d = UsbDeviceDescriptor {
        base_class: 0x0B,
        video_streaming_present: false,
        input_terminal_types: vec![],
    };
    assert!(!is_capture_device(&d));
}

#[test]
fn comms_class_modem_is_not_capture() {
    let d = UsbDeviceDescriptor {
        base_class: 0x02,
        video_streaming_present: false,
        input_terminal_types: vec![],
    };
    assert!(!is_capture_device(&d));
}

#[test]
fn printer_is_not_capture() {
    let d = UsbDeviceDescriptor {
        base_class: 0x07,
        video_streaming_present: false,
        input_terminal_types: vec![],
    };
    assert!(!is_capture_device(&d));
}

// ---- pure filter: video-class but not capture ----

#[test]
fn video_class_without_streaming_is_not_capture() {
    // VideoControl-only interface (e.g., a video-output-only display
    // with no streaming endpoint).
    let d = UsbDeviceDescriptor {
        base_class: 0x0E,
        video_streaming_present: false,
        input_terminal_types: vec![0x0201], // would-be camera, but no streaming
    };
    assert!(!is_capture_device(&d));
}

#[test]
fn video_class_streaming_with_only_output_terminals_is_not_capture() {
    // Output Terminal Types are 0x0300-0x03FF. A device with only
    // those is a video display / output, not a capture device.
    let d = UsbDeviceDescriptor {
        base_class: 0x0E,
        video_streaming_present: true,
        input_terminal_types: vec![0x0301, 0x0302, 0x0303],
    };
    assert!(!is_capture_device(&d));
}

#[test]
fn video_class_streaming_with_no_terminals_is_not_capture() {
    let d = UsbDeviceDescriptor {
        base_class: 0x0E,
        video_streaming_present: true,
        input_terminal_types: vec![],
    };
    assert!(!is_capture_device(&d));
}

#[test]
fn vendor_specific_input_terminal_types_outside_input_range_is_not_capture() {
    // 0x01FF — just below the Input Terminal range. Not a capture
    // input by spec.
    let d = UsbDeviceDescriptor {
        base_class: 0x0E,
        video_streaming_present: true,
        input_terminal_types: vec![0x01FF],
    };
    assert!(!is_capture_device(&d));
}

#[test]
fn terminal_at_input_range_boundary_is_capture() {
    // Boundary cases: 0x0200 (start of input) and 0x02FF (end), plus
    // 0x0400 / 0x04FF (start/end of external).
    for &t in &[0x0200u16, 0x02FFu16, 0x0400u16, 0x04FFu16] {
        let d = UsbDeviceDescriptor {
            base_class: 0x0E,
            video_streaming_present: true,
            input_terminal_types: vec![t],
        };
        assert!(is_capture_device(&d), "boundary terminal {t:#x}");
    }
}

#[test]
fn terminal_just_outside_external_range_is_not_capture() {
    // 0x0500 is just above the External Terminal range and not a
    // capture indicator.
    let d = UsbDeviceDescriptor {
        base_class: 0x0E,
        video_streaming_present: true,
        input_terminal_types: vec![0x0500],
    };
    assert!(!is_capture_device(&d));
}

// ---- monitor stub (Linux/macOS) and Windows construction ----

#[test]
fn synthetic_volume_removal_reaches_only_the_removal_callback() {
    let arrivals = Arc::new(AtomicUsize::new(0));
    let removals = Arc::new(AtomicUsize::new(0));
    let arrival_counter = arrivals.clone();
    let removal_counter = removals.clone();
    let callbacks = UsbMonitorCallbacks::new(
        Box::new(move || {
            arrival_counter.fetch_add(1, Ordering::SeqCst);
        }),
        Box::new(move || {
            removal_counter.fetch_add(1, Ordering::SeqCst);
        }),
    );

    let event = usb_monitor_event_from_device_change(0x0219, 0x8004, 0x0002)
        .expect("synthetic volume removal message is monitored");
    callbacks.dispatch(event);

    assert_eq!(arrivals.load(Ordering::SeqCst), 0);
    assert_eq!(removals.load(Ordering::SeqCst), 1);
}

#[test]
fn synthetic_capture_arrival_still_reaches_only_the_arrival_callback() {
    let arrivals = Arc::new(AtomicUsize::new(0));
    let removals = Arc::new(AtomicUsize::new(0));
    let arrival_counter = arrivals.clone();
    let removal_counter = removals.clone();
    let callbacks = UsbMonitorCallbacks::new(
        Box::new(move || {
            arrival_counter.fetch_add(1, Ordering::SeqCst);
        }),
        Box::new(move || {
            removal_counter.fetch_add(1, Ordering::SeqCst);
        }),
    );

    let event = usb_monitor_event_from_device_change(0x0219, 0x8000, 0x0005)
        .expect("synthetic capture-arrival message is monitored");
    callbacks.dispatch(event);

    assert_eq!(arrivals.load(Ordering::SeqCst), 1);
    assert_eq!(removals.load(Ordering::SeqCst), 0);
}

#[test]
fn monitor_start_with_callback_compiles_on_all_targets() {
    // The non-Windows stub never fires the callback; on Windows the
    // monitor spawns a message-pump thread but no events arrive in
    // CI. Either way, construction must succeed.
    //
    // Bounded on purpose. `UsbMonitor::start` deadlocked outright on
    // Windows -- it waited for a thread id the monitor thread only sent
    // once its message pump had exited, and nothing but `Monitor::drop`
    // could stop that pump -- and with no bound here the symptom was a CI
    // job sitting for 45 minutes on "has been running for over 60
    // seconds" rather than a test that failed. Completing is part of the
    // contract, so assert it.
    let counter = Arc::new(AtomicUsize::new(0));
    let counter_for_cb = counter.clone();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let lifecycle = std::thread::spawn(move || {
        let cb: ArrivalCallback = Box::new(move || {
            counter_for_cb.fetch_add(1, Ordering::SeqCst);
        });
        let monitor = UsbMonitor::start(cb).expect("monitor start");
        // Drop the monitor; no events are expected on Linux.
        drop(monitor);
        let _ = done_tx.send(());
    });
    done_rx
        .recv_timeout(std::time::Duration::from_secs(30))
        .expect("UsbMonitor start + drop must finish, not block forever");
    lifecycle.join().expect("monitor lifecycle thread panicked");
    assert_eq!(counter.load(Ordering::SeqCst), 0);
}

#[cfg(not(windows))]
#[test]
fn linux_macos_monitor_never_fires_callback() {
    // Specifically on the stub platforms, confirm zero events.
    let counter = Arc::new(AtomicUsize::new(0));
    let counter_for_cb = counter.clone();
    let cb: ArrivalCallback = Box::new(move || {
        counter_for_cb.fetch_add(1, Ordering::SeqCst);
    });
    let monitor = UsbMonitor::start(cb).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    drop(monitor);
    assert_eq!(counter.load(Ordering::SeqCst), 0);
}

// ---- integration: callback drives RotationController ----

#[test]
fn callback_drives_rotation_controller() {
    use runtime::{Clock, MockClock, RotationConfig, RotationController, SuspiciousEventKind};
    use std::sync::Mutex;
    use std::time::Instant;

    struct ClockProxy(Arc<MockClock>);
    impl Clock for ClockProxy {
        fn now(&self) -> Instant {
            self.0.now()
        }
    }

    let clock = Arc::new(MockClock::new());
    let ctrl = Arc::new(Mutex::new(RotationController::new(
        Box::new(ClockProxy(clock.clone())),
        RotationConfig::default(),
    )));
    let ctrl_for_cb = ctrl.clone();

    let cb: ArrivalCallback = Box::new(move || {
        ctrl_for_cb
            .lock()
            .unwrap()
            .note_suspicious_event(SuspiciousEventKind::UsbCaptureDevice);
    });

    // Simulate a USB capture device event by invoking the callback
    // directly (the wiring would do this on an actual arrival).
    cb();

    let r = ctrl.lock().unwrap().check_for_rotation();
    assert_eq!(
        r,
        Some(runtime::RotationReason::Suspicious(
            SuspiciousEventKind::UsbCaptureDevice,
        ))
    );
}
