use runtime::{
    match_recorders, scan_for_recorders, snapshot_running_processes, RecorderScanError,
    RECORDER_PROCESS_NAMES,
};

// ---- pure match logic ----

#[test]
fn match_finds_obs() {
    let processes = ["explorer.exe", "obs64.exe", "discord.exe"];
    let m = match_recorders(&processes);
    assert!(m.contains(&"obs64.exe"));
    assert_eq!(m.len(), 1);
}

#[test]
fn match_finds_multiple_recorders() {
    let processes = vec![
        "obs64.exe".to_string(),
        "bandicam.exe".to_string(),
        "Sharex.exe".to_string(), // case mix to test ascii_lowercase
        "notepad.exe".to_string(),
    ];
    let m = match_recorders(&processes);
    assert!(m.contains(&"obs64.exe"));
    assert!(m.contains(&"bandicam.exe"));
    assert!(m.contains(&"sharex.exe"));
    assert_eq!(m.len(), 3);
}

#[test]
fn match_is_case_insensitive() {
    let processes = ["OBS64.EXE", "BandICam.exe", "SHAREX.exe"];
    let m = match_recorders(&processes);
    assert_eq!(m.len(), 3);
}

#[test]
fn match_returns_empty_on_innocuous_processes() {
    let processes = [
        "explorer.exe",
        "discord.exe",
        "chrome.exe",
        "firefox.exe",
        "code.exe",
        "cmd.exe",
        "powershell.exe",
        "system",
        "svchost.exe",
        "notepad.exe",
    ];
    let m = match_recorders(&processes);
    assert!(m.is_empty(), "false positives: {:?}", m);
}

#[test]
fn match_handles_empty_input() {
    let processes: [&str; 0] = [];
    assert!(match_recorders(&processes).is_empty());
}

#[test]
fn match_handles_full_paths_after_basename_strip() {
    // Caller is responsible for path-stripping before calling
    // match_recorders. This test demonstrates the contract: we
    // match the EXACT string supplied (case-insensitive).
    let processes = [r"C:\Windows\System32\notepad.exe"];
    assert!(
        match_recorders(&processes).is_empty(),
        "match_recorders compares full strings — caller must basename-strip"
    );

    let processes = ["obs64.exe"];
    assert_eq!(match_recorders(&processes), vec!["obs64.exe"]);
}

#[test]
fn match_dedupes_by_match_list_order() {
    // Two basenames can never match the same RECORDER_PROCESS_NAMES
    // entry in our current list (entries are unique). Test that
    // ordering of returned matches is the match-list order, not the
    // input order.
    let processes = ["bandicam.exe", "obs64.exe"]; // input has obs second
    let m = match_recorders(&processes);
    let obs_pos = m.iter().position(|&s| s == "obs64.exe");
    let band_pos = m.iter().position(|&s| s == "bandicam.exe");
    assert!(obs_pos.is_some());
    assert!(band_pos.is_some());
    // RECORDER_PROCESS_NAMES has obs64 before bandicam — confirm that
    // ordering survives.
    let list_obs = RECORDER_PROCESS_NAMES
        .iter()
        .position(|&s| s == "obs64.exe")
        .unwrap();
    let list_band = RECORDER_PROCESS_NAMES
        .iter()
        .position(|&s| s == "bandicam.exe")
        .unwrap();
    assert!(list_obs < list_band);
    assert!(obs_pos.unwrap() < band_pos.unwrap());
}

// ---- match list invariants ----

#[test]
fn recorder_list_entries_are_lowercase() {
    for entry in RECORDER_PROCESS_NAMES {
        assert_eq!(
            entry.to_ascii_lowercase(),
            *entry,
            "entry {entry:?} not lowercase",
        );
    }
}

#[test]
fn recorder_list_entries_are_unique() {
    let mut seen = std::collections::HashSet::new();
    for entry in RECORDER_PROCESS_NAMES {
        assert!(seen.insert(*entry), "duplicate entry {entry:?}");
    }
}

#[test]
fn recorder_list_entries_end_in_exe() {
    // Sanity: every entry should look like a Windows executable.
    for entry in RECORDER_PROCESS_NAMES {
        assert!(
            entry.ends_with(".exe"),
            "entry {entry:?} does not end in .exe — Windows process enumeration emits .exe basenames",
        );
    }
}

#[test]
fn recorder_list_covers_design_doc_examples() {
    // The build-order doc names: OBS, Bandicam, Camtasia, ShareX,
    // NVIDIA ShadowPlay, Windows Game Bar. Each must have at least
    // one entry in the list.
    let categories = [
        ("obs", "OBS"),
        ("bandicam", "Bandicam"),
        ("camtasia", "Camtasia"),
        ("sharex", "ShareX"),
        ("nv", "NVIDIA"),
        ("gamebar", "Windows Game Bar"),
    ];
    for (substr, label) in categories {
        let any_match = RECORDER_PROCESS_NAMES.iter().any(|e| e.contains(substr));
        assert!(any_match, "{label} not represented in match list");
    }
}

// ---- snapshot path ----

#[cfg(not(windows))]
#[test]
fn snapshot_returns_unsupported_on_non_windows() {
    let res = snapshot_running_processes();
    assert!(matches!(res, Err(RecorderScanError::Win32(_))));
    assert!(matches!(
        scan_for_recorders(),
        Err(RecorderScanError::Win32(_))
    ));
}

#[cfg(windows)]
#[test]
fn snapshot_yields_at_least_some_processes_on_windows() {
    // CI / dev verification: we should always see at least the
    // current process (the test runner) plus its parent shell.
    let names = snapshot_running_processes().expect("snapshot");
    assert!(!names.is_empty(), "expected at least one running process");
}

// ---- periodic scanner ----

#[test]
fn scanner_starts_and_stops_cleanly() {
    use runtime::{RecorderScanner, RecorderScannerConfig};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    let counter = Arc::new(AtomicUsize::new(0));
    let counter_for_cb = counter.clone();
    let cb = Box::new(move |_: &[&'static str]| {
        counter_for_cb.fetch_add(1, Ordering::SeqCst);
    });

    let cfg = RecorderScannerConfig {
        interval: Duration::from_millis(50),
        tick: Duration::from_millis(10),
    };
    let scanner = RecorderScanner::start(cfg, cb);
    std::thread::sleep(Duration::from_millis(120));
    drop(scanner);
    // On non-Windows the scan() call returns Err and the callback
    // never fires — counter stays at zero. On Windows the scanner
    // would only fire if a real recorder process happens to be
    // running on the dev box; we don't assert > 0 to keep this
    // robust across hosts.
    let _ = counter.load(Ordering::SeqCst);
}

#[test]
fn scanner_drop_terminates_thread_promptly() {
    use runtime::{RecorderScanner, RecorderScannerConfig};
    use std::time::{Duration, Instant};

    let cb = Box::new(|_: &[&'static str]| {});
    let cfg = RecorderScannerConfig {
        interval: Duration::from_secs(60 * 60),
        tick: Duration::from_millis(50),
    };
    let scanner = RecorderScanner::start(cfg, cb);
    let start = Instant::now();
    drop(scanner);
    let elapsed = start.elapsed();
    // Drop must return within a small multiple of the tick (we set
    // 50ms tick; allow 5x for CI noise).
    assert!(
        elapsed < Duration::from_millis(500),
        "scanner drop took {elapsed:?}, expected < 500ms"
    );
}

// ---- integration with rotation controller ----

#[test]
fn detected_recorders_drive_rotation_controller() {
    use runtime::{
        Clock, MockClock, RotationConfig, RotationController, RotationReason, SuspiciousEventKind,
    };
    use std::sync::Arc;
    use std::time::Instant;

    struct ClockProxy(Arc<MockClock>);
    impl Clock for ClockProxy {
        fn now(&self) -> Instant {
            self.0.now()
        }
    }

    let clock = Arc::new(MockClock::new());
    let mut ctrl = RotationController::new(
        Box::new(ClockProxy(clock.clone())),
        RotationConfig::default(),
    );

    let processes = ["explorer.exe", "obs64.exe", "discord.exe"];
    let matches = match_recorders(&processes);
    if !matches.is_empty() {
        ctrl.note_suspicious_event(SuspiciousEventKind::ScreenRecorder);
    }

    assert_eq!(
        ctrl.check_for_rotation(),
        Some(RotationReason::Suspicious(
            SuspiciousEventKind::ScreenRecorder
        ))
    );
}
