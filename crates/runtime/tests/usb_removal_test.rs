//! Regression coverage for the dead-man switch's removable-volume signal.
//!
//! Windows reports both a normal safe eject and an unexpected yank as
//! `DBT_DEVICEREMOVECOMPLETE`.  The monitor must route both to the removal
//! callback, never to the capture-arrival callback, and retain the volume
//! interface path rather than a reassignable drive letter.

use runtime::{
    usb_monitor_event_from_device_change, ArrivalCallback, UsbMonitor, UsbMonitorCallbacks,
    VolumeRemovalCallback,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

const WM_DEVICECHANGE: u32 = 0x0219;
const DBT_DEVICEARRIVAL: u32 = 0x8000;
const DBT_DEVICEREMOVECOMPLETE: u32 = 0x8004;
const DBT_DEVTYP_DEVICEINTERFACE: u32 = 0x0005;

fn callbacks(removals: Arc<AtomicUsize>, identities: Arc<std::sync::Mutex<Vec<String>>>) -> UsbMonitorCallbacks {
    let arrival: ArrivalCallback = Box::new(|| panic!("volume removal must not reach arrival callback"));
    let removal: VolumeRemovalCallback = Box::new(move |device| {
        removals.fetch_add(1, Ordering::SeqCst);
        identities.lock().unwrap().push(device.as_str().to_owned());
    });
    UsbMonitorCallbacks::new(arrival, removal)
}

fn complete_volume_removal(path: &str) -> runtime::UsbMonitorEvent {
    usb_monitor_event_from_device_change(
        WM_DEVICECHANGE,
        DBT_DEVICEREMOVECOMPLETE,
        DBT_DEVTYP_DEVICEINTERFACE,
        Some(path),
    )
    .expect("a volume removal must reach the monitor decoder")
}

#[test]
fn safe_eject_notifies_the_removal_callback() {
    let removals = Arc::new(AtomicUsize::new(0));
    let identities = Arc::new(std::sync::Mutex::new(Vec::new()));
    let monitor = callbacks(removals.clone(), identities.clone());

    monitor.dispatch(complete_volume_removal(r"\\?\Volume{safe-eject}"));

    assert_eq!(removals.load(Ordering::SeqCst), 1);
    assert_eq!(identities.lock().unwrap().as_slice(), [r"\\?\Volume{safe-eject}"]);
}

#[test]
fn surprise_yank_notifies_the_removal_callback() {
    let removals = Arc::new(AtomicUsize::new(0));
    let identities = Arc::new(std::sync::Mutex::new(Vec::new()));
    let monitor = callbacks(removals.clone(), identities.clone());

    // A physical yank has the same completion notification as a safe eject.
    // Treating only arrival as actionable would leave this counter at zero.
    monitor.dispatch(complete_volume_removal(r"\\?\Volume{surprise-yank}"));

    assert_eq!(removals.load(Ordering::SeqCst), 1);
    assert_eq!(identities.lock().unwrap().as_slice(), [r"\\?\Volume{surprise-yank}"]);
}

#[test]
fn reappearance_with_a_different_letter_keeps_the_volume_identity() {
    let removals = Arc::new(AtomicUsize::new(0));
    let identities = Arc::new(std::sync::Mutex::new(Vec::new()));
    let monitor = callbacks(removals.clone(), identities.clone());
    let stable_volume = r"\\?\Volume{same-device-reassigned}";

    monitor.dispatch(complete_volume_removal(stable_volume));
    // On reappearance Windows can mount the same volume as a different letter.
    // The monitor receives the interface path, so no letter is part of the key.
    let reappearance = usb_monitor_event_from_device_change(
        WM_DEVICECHANGE,
        DBT_DEVICEARRIVAL,
        DBT_DEVTYP_DEVICEINTERFACE,
        Some(stable_volume),
    )
    .expect("arrival remains a valid monitor event");
    monitor.dispatch(reappearance);

    assert_eq!(removals.load(Ordering::SeqCst), 1);
    assert_eq!(identities.lock().unwrap().as_slice(), [stable_volume]);
    assert!(usb_monitor_event_from_device_change(
        WM_DEVICECHANGE,
        DBT_DEVICEREMOVECOMPLETE,
        DBT_DEVTYP_DEVICEINTERFACE,
        Some("F:"),
    )
    .is_none());
}

#[test]
fn start_and_stop_complete_within_a_bounded_timeout() {
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let lifecycle = std::thread::spawn(move || {
        let monitor = UsbMonitor::start_with_volume_removal(Box::new(|| {}), Box::new(|_| {}))
            .expect("monitor start");
        drop(monitor);
        done_tx.send(()).expect("test receiver remains available");
    });

    done_rx
        .recv_timeout(Duration::from_secs(30))
        .expect("UsbMonitor start + stop must not deadlock");
    lifecycle.join().expect("monitor lifecycle thread must not panic");
}
