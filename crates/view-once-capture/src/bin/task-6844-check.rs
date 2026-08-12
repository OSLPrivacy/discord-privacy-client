//! TASK 6844 check: a real Windows capture of a real open view-once viewer
//! produces exactly one authenticated sender notification, and nothing else
//! does.
//!
//! Run in two phases, as two separate processes, because "survives a restart"
//! is not a claim one process can make about itself:
//!
//! ```text
//! task-6844-check --dir D:\... --phase open      # viewer open, real capture, event queued offline
//! task-6844-check --dir D:\... --phase deliver   # fresh process: reconnect, deliver, notify once
//! ```
//!
//! `--starve <item>` removes one ingredient so the check can be shown to fail
//! without it. The assertions do not know the knob exists; starving an item
//! changes only what the harness feeds them.

#[cfg(not(windows))]
fn main() {
    eprintln!(
        "TASK 6844: this check needs a real Windows desktop. Build it for \
         x86_64-pc-windows-gnu and run it there."
    );
    std::process::exit(1);
}

#[cfg(windows)]
fn main() {
    match windows_check::run() {
        Ok(()) => println!("6844 RESULT=GREEN"),
        Err(error) => {
            println!("6844 FAIL {error}");
            println!("6844 RESULT=RED");
            std::process::exit(1);
        }
    }
}

#[cfg(windows)]
mod windows_check {
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use rand::RngCore;
    use serde::{Deserialize, Serialize};
    use view_once_capture::disclosure::{discloses_capture_limits, CAPTURE_DISCLOSURE_SENDER};
    use view_once_capture::event::{
        sign_capture_event, CaptureEvidence, CaptureRealness, SupportedCapturePath,
        UnsupportedCapturePath, ViewOnceOpenBinding,
    };
    use view_once_capture::notifier::{
        notification_sentence, AcceptOutcome, SenderCaptureNotifier, SentViewOnceRecord,
    };
    use view_once_capture::outbox::CaptureOutbox;
    use view_once_capture::sig;
    use view_once_capture::windows_watch::SupportedCaptureWatcher;
    use view_once_capture::{absolute_capture_claims_in, CAPTURE_DISCLOSURE_VIEWER};

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{
        BeginPaint, CreateSolidBrush, EndPaint, FillRect, UpdateWindow, PAINTSTRUCT,
    };
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetWindowDisplayAffinity,
        PeekMessageW, RegisterClassW, SetForegroundWindow, SetWindowDisplayAffinity, ShowWindow,
        TranslateMessage, MSG, PM_REMOVE, SW_SHOW, WM_PAINT, WNDCLASSW, WS_OVERLAPPEDWINDOW,
        WS_VISIBLE,
    };

    /// `WDA_EXCLUDEFROMCAPTURE`. Windows 10 2004 and later.
    const WDA_EXCLUDEFROMCAPTURE: u32 = 0x0000_0011;
    /// `WDA_MONITOR` — the older, weaker exclusion, used when the above is
    /// refused.
    const WDA_MONITOR: u32 = 0x0000_0001;

    const MESSAGE_ID: &str = "msg-6844-view-once";
    const SENDER_ID: &str = "sender-6844";
    const VIEWER_ID: &str = "viewer-6844";
    const VIEWER_DEVICE_ID: &str = "viewer-device-6844";

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct OpenPhaseReport {
        message_id: String,
        sender_osl_user_id: String,
        viewer_osl_user_id: String,
        viewer_device_id: String,
        viewer_public_key_hex: String,
        open_nonce_hex: String,
        path_label: String,
        screen_width: i32,
        screen_height: i32,
        dib_byte_len: u64,
        distinct_sampled_colors: usize,
        live_match_ppm: u32,
        print_screen_key_seen: bool,
        clipboard_sequence_before: u32,
        clipboard_sequence_after: u32,
        viewer_display_affinity: u32,
        offline_attempts: u32,
        /// The out-of-process grab that OSL is blind to: it really ran, and it
        /// produced this many observations (which must be zero).
        unsupported_grab_bytes: u64,
        unsupported_grab_observations: usize,
        unsupported_path_label: String,
        ignored_clipboard_updates: u32,
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn unhex(text: &str) -> Result<Vec<u8>, String> {
        if text.len() % 2 != 0 {
            return Err("odd-length hex".to_owned());
        }
        (0..text.len() / 2)
            .map(|index| {
                u8::from_str_radix(&text[index * 2..index * 2 + 2], 16)
                    .map_err(|_| "bad hex".to_owned())
            })
            .collect()
    }

    /// Which ingredient, if any, this run is being denied.
    ///
    /// A command-line argument rather than an environment variable: the check
    /// is launched from WSL across the Windows process boundary, and only
    /// variables listed in `WSLENV` survive that crossing. An argument always
    /// arrives, so a starvation can never be silently ignored and read as a
    /// pass.
    fn starve() -> String {
        let arguments: Vec<String> = std::env::args().collect();
        arguments
            .iter()
            .position(|argument| argument == "--starve")
            .and_then(|index| arguments.get(index + 1))
            .cloned()
            .unwrap_or_default()
    }

    fn starving(item: &str) -> bool {
        starve() == item
    }

    fn sibling_exe(name: &str) -> Result<PathBuf, String> {
        let current = std::env::current_exe().map_err(|error| format!("current exe: {error}"))?;
        let directory = current
            .parent()
            .ok_or_else(|| "the check has no directory".to_owned())?;
        let candidate = directory.join(format!("{name}.exe"));
        candidate
            .exists()
            .then_some(candidate)
            .ok_or_else(|| format!("helper {name}.exe was not built next to the check"))
    }

    // ---------------------------------------------------------------- viewer

    unsafe extern "system" fn viewer_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if message == WM_PAINT {
            let mut paint: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(hwnd, &mut paint);
            let brush = CreateSolidBrush(0x00_45_2a_1a);
            FillRect(dc, &paint.rcPaint, brush);
            EndPaint(hwnd, &paint);
            return 0;
        }
        DefWindowProcW(hwnd, message, wparam, lparam)
    }

    struct ViewOnceViewer {
        hwnd: HWND,
        affinity: u32,
    }

    impl ViewOnceViewer {
        /// Open a real, visible, capture-protected viewer window.
        ///
        /// Capture protection is applied and read back, the same order the
        /// shipping view-once viewer uses. It is deliberately *not* what this
        /// check is about: protection is best effort, and the detection under
        /// test is what happens when a capture goes through anyway.
        fn open() -> Result<Self, String> {
            unsafe {
                let instance = GetModuleHandleW(std::ptr::null());
                let class: Vec<u16> = "OslTask6844ViewOnceViewer\0".encode_utf16().collect();
                let mut definition: WNDCLASSW = std::mem::zeroed();
                definition.lpfnWndProc = Some(viewer_proc);
                definition.hInstance = instance;
                definition.lpszClassName = class.as_ptr();
                RegisterClassW(&definition);

                let title: Vec<u16> = "OSL view-once\0".encode_utf16().collect();
                let hwnd = CreateWindowExW(
                    0,
                    class.as_ptr(),
                    title.as_ptr(),
                    WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                    120,
                    120,
                    720,
                    480,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    instance,
                    std::ptr::null(),
                );
                if hwnd.is_null() {
                    return Err("the view-once viewer window was refused".to_owned());
                }
                let mut affinity = 0u32;
                if SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) == 0 {
                    SetWindowDisplayAffinity(hwnd, WDA_MONITOR);
                }
                GetWindowDisplayAffinity(hwnd, &mut affinity);
                ShowWindow(hwnd, SW_SHOW);
                UpdateWindow(hwnd);
                SetForegroundWindow(hwnd);
                Ok(Self { hwnd, affinity })
            }
        }

        /// Keep the viewer alive and responsive for `duration`. A window that
        /// stops pumping is a window Windows will treat as hung, and a hung
        /// window is not an open viewer.
        fn pump(&self, duration: Duration) {
            let deadline = std::time::Instant::now() + duration;
            while std::time::Instant::now() < deadline {
                unsafe {
                    let mut message: MSG = std::mem::zeroed();
                    while PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                        TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }

        fn close(self) {
            unsafe {
                DestroyWindow(self.hwnd);
            }
        }
    }

    // ----------------------------------------------------------- disclosure

    /// The words the viewer or sender is shown. Starving the disclosure means
    /// the surface ships copy that no longer names what OSL is blind to; an
    /// absolute claim means it ships a promise it cannot keep.
    fn surface_copy(base: &'static str) -> String {
        if starving("unsupported-path-disclosure") {
            return base
                .replace("a camera pointed at your screen, ", "")
                .replace("a camera pointed at their screen, ", "")
                .replace("an external capture device, ", "")
                .replace("or every capture tool, ", "");
        }
        if starving("absolute-claim") {
            return format!("{base} OSL detects all screenshots.");
        }
        base.to_owned()
    }

    fn require_honest_copy(surface: &str, text: &str) -> Result<(), String> {
        let claims = absolute_capture_claims_in(text);
        if !claims.is_empty() {
            return Err(format!(
                "the {surface} copy makes an absolute screenshot claim: {claims:?}"
            ));
        }
        if !discloses_capture_limits(text) {
            return Err(format!(
                "the {surface} copy does not disclose the capture-detection limits before use"
            ));
        }
        println!(
            "6844 disclosure surface={surface} discloses_limits=true absolute_claims=0 chars={}",
            text.chars().count()
        );
        Ok(())
    }

    // ------------------------------------------------------------ phase one

    fn phase_open(dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|error| format!("check directory: {error}"))?;

        // The viewer is told what detection can and cannot see before any
        // content is revealed.
        require_honest_copy("viewer", &surface_copy(CAPTURE_DISCLOSURE_VIEWER))?;

        let viewer = ViewOnceViewer::open()?;
        println!(
            "6844 viewer opened=true display_affinity=0x{:08x}",
            viewer.affinity
        );

        let watcher = SupportedCaptureWatcher::start()?;
        println!("6844 watcher started=true");
        viewer.pump(Duration::from_millis(600));

        // A real capture, requested by a different process, performed by
        // Windows.
        if starving("real-capture") {
            println!("6844 starved item=real-capture (no PrintScreen was pressed)");
        } else {
            let helper = sibling_exe("task-6844-press-print-screen")?;
            let status = std::process::Command::new(&helper)
                .status()
                .map_err(|error| format!("cannot run the PrintScreen helper: {error}"))?;
            if !status.success() {
                return Err("the PrintScreen helper failed".to_owned());
            }
            println!("6844 print_screen pressed_by=separate-process pid_exit=0");
        }

        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        let mut observed = None;
        while std::time::Instant::now() < deadline {
            viewer.pump(Duration::from_millis(200));
            if let Some(first) = watcher.observations().into_iter().next() {
                observed = Some(first);
                break;
            }
        }
        let observed = observed.ok_or_else(|| {
            "no supported Windows capture was observed while the view-once viewer was open"
                .to_owned()
        })?;

        let evidence = if starving("simulated-event") {
            println!("6844 starved item=simulated-event (evidence is fabricated, not measured)");
            CaptureEvidence {
                distinct_sampled_colors: 1,
                live_match_ppm: 0,
                ..observed.evidence
            }
        } else {
            observed.evidence
        };

        match evidence.realness() {
            CaptureRealness::Real => {}
            CaptureRealness::Simulated(reason) => {
                return Err(format!("the observed capture is not real: {reason}"))
            }
        }
        println!(
            "6844 capture path={} screen={}x{} dib={}x{} bits={} bytes={} distinct_colors={} live_match_ppm={} key_seen={} clipboard_seq={}->{}",
            observed.path.label(),
            evidence.screen_width,
            evidence.screen_height,
            evidence.dib_width,
            evidence.dib_height,
            evidence.dib_bit_count,
            evidence.dib_byte_len,
            evidence.distinct_sampled_colors,
            evidence.live_match_ppm,
            evidence.print_screen_key_seen,
            evidence.clipboard_sequence_before,
            evidence.clipboard_sequence_after,
        );

        // A capture OSL cannot see, performed for real, while the same viewer
        // is still open.
        let observations_before = watcher.observations().len();
        let grab_helper = sibling_exe("task-6844-unsupported-grab")?;
        let grab_path = dir.join("unsupported-grab.bmp");
        let grab = std::process::Command::new(&grab_helper)
            .arg(&grab_path)
            .output()
            .map_err(|error| format!("cannot run the unsupported-path grab: {error}"))?;
        if !grab.status.success() {
            return Err(format!(
                "the unsupported-path grab failed: {}",
                String::from_utf8_lossy(&grab.stderr).trim()
            ));
        }
        viewer.pump(Duration::from_secs(3));
        let grab_bytes = std::fs::metadata(&grab_path)
            .map_err(|error| format!("the unsupported-path grab wrote nothing: {error}"))?
            .len();
        let expected_grab =
            54 + (evidence.screen_width as u64) * (evidence.screen_height as u64) * 4;
        if grab_bytes != expected_grab {
            return Err(format!(
                "the unsupported-path grab wrote {grab_bytes} bytes, not the {expected_grab} a real \
                 {}x{} capture needs",
                evidence.screen_width, evidence.screen_height
            ));
        }
        let observations_after = watcher.observations().len();
        let from_unsupported = observations_after - observations_before;
        if from_unsupported != 0 {
            return Err(format!(
                "the unsupported path produced {from_unsupported} observation(s); it must produce none"
            ));
        }
        println!(
            "6844 unsupported_path label={} really_captured_bytes={grab_bytes} observations={from_unsupported}",
            UnsupportedCapturePath::OutOfProcessGrab.label()
        );
        println!(
            "6844 watcher ignored_clipboard_updates={} print_screen_presses={}",
            watcher.ignored_clipboard_updates(),
            watcher.print_screen_presses()
        );

        // Bind the event to this viewer, this device, this message, this open.
        let (viewer_secret, viewer_public) = sig::generate_keypair();
        let mut open_nonce = [0u8; 16];
        rand::rngs::OsRng.fill_bytes(&mut open_nonce);
        let binding = if starving("binding") {
            println!("6844 starved item=binding (the event names a message nobody sent)");
            ViewOnceOpenBinding {
                message_id: "msg-6844-not-this-one".to_owned(),
                sender_osl_user_id: SENDER_ID.to_owned(),
                viewer_osl_user_id: "someone-else".to_owned(),
                viewer_device_id: VIEWER_DEVICE_ID.to_owned(),
                open_nonce,
            }
        } else {
            ViewOnceOpenBinding {
                message_id: MESSAGE_ID.to_owned(),
                sender_osl_user_id: SENDER_ID.to_owned(),
                viewer_osl_user_id: VIEWER_ID.to_owned(),
                viewer_device_id: VIEWER_DEVICE_ID.to_owned(),
                open_nonce,
            }
        };

        let mut event = sign_capture_event(
            binding,
            observed.path,
            observed.observed_at_ms,
            evidence,
            &viewer_secret,
        )?;
        if starving("signature") {
            println!("6844 starved item=signature (one signature byte is flipped)");
            event.signature[0] ^= 0x01;
        }
        println!(
            "6844 event signed=true open_nonce={} viewer_key={} signature={}",
            event.binding.open_nonce_hex(),
            hex(&event.viewer_public_key),
            &hex(&event.signature)[..16]
        );

        // The sender is unreachable at capture time. This is the normal case,
        // not the edge case: a capture happens while someone is looking at
        // their screen, and nothing about that moment promises a network.
        let mut offline_attempts = 0;
        let outbox_dir = if starving("restart") {
            println!(
                "6844 starved item=restart (the outbox is written somewhere a restart cannot find)"
            );
            dir.join("viewer-volatile")
        } else {
            dir.join("viewer")
        };
        if starving("offline-delivery") {
            println!("6844 starved item=offline-delivery (the event is dropped instead of queued)");
        } else {
            let mut outbox = CaptureOutbox::open(&outbox_dir)?;
            let queued = outbox.enqueue(event.clone())?;
            if !queued {
                return Err("the capture event was already queued before this open".to_owned());
            }
            // One delivery attempt, which fails, because the sender is offline.
            offline_attempts = outbox.note_attempt(&event.binding.open_nonce)?;
            println!(
                "6844 offline queued=1 delivery_attempts={offline_attempts} pending={}",
                outbox.pending_count()
            );
        }

        let report = OpenPhaseReport {
            message_id: MESSAGE_ID.to_owned(),
            sender_osl_user_id: SENDER_ID.to_owned(),
            viewer_osl_user_id: VIEWER_ID.to_owned(),
            viewer_device_id: VIEWER_DEVICE_ID.to_owned(),
            viewer_public_key_hex: hex(viewer_public.as_bytes()),
            open_nonce_hex: event.binding.open_nonce_hex(),
            path_label: observed.path.label().to_owned(),
            screen_width: evidence.screen_width,
            screen_height: evidence.screen_height,
            dib_byte_len: evidence.dib_byte_len,
            distinct_sampled_colors: evidence.distinct_sampled_colors,
            live_match_ppm: evidence.live_match_ppm,
            print_screen_key_seen: evidence.print_screen_key_seen,
            clipboard_sequence_before: evidence.clipboard_sequence_before,
            clipboard_sequence_after: evidence.clipboard_sequence_after,
            viewer_display_affinity: viewer.affinity,
            offline_attempts,
            unsupported_grab_bytes: grab_bytes,
            unsupported_grab_observations: from_unsupported,
            unsupported_path_label: UnsupportedCapturePath::OutOfProcessGrab.label().to_owned(),
            ignored_clipboard_updates: watcher.ignored_clipboard_updates(),
        };
        std::fs::write(
            dir.join("open-phase.json"),
            serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?,
        )
        .map_err(|error| format!("cannot write the open-phase report: {error}"))?;

        drop(watcher);
        viewer.close();
        println!("6844 viewer closed=true watcher stopped=true");
        Ok(())
    }

    // ------------------------------------------------------------ phase two

    fn phase_deliver(dir: &Path) -> Result<(), String> {
        // The sender is told what a notification, and the absence of one,
        // means — before they ever send view-once media.
        require_honest_copy("sender", &surface_copy(CAPTURE_DISCLOSURE_SENDER))?;

        let report: OpenPhaseReport = serde_json::from_slice(
            &std::fs::read(dir.join("open-phase.json"))
                .map_err(|error| format!("the open phase left no report: {error}"))?,
        )
        .map_err(|error| format!("the open-phase report is unreadable: {error}"))?;

        // A fresh process reading the outbox off disk is what a restart is.
        let outbox_dir = dir.join("viewer");
        let mut outbox = CaptureOutbox::open(&outbox_dir)?;
        let pending = outbox.pending().into_iter().cloned().collect::<Vec<_>>();
        if pending.len() != 1 {
            return Err(format!(
                "after restart the viewer holds {} capture event(s) to deliver, not 1",
                pending.len()
            ));
        }
        let event = pending.into_iter().next().expect("one pending event");
        println!(
            "6844 restart process=new pending_after_restart=1 open_nonce={} offline_attempts_before={}",
            event.binding.open_nonce_hex(),
            report.offline_attempts
        );

        let key_bytes = unhex(&report.viewer_public_key_hex)?;
        let viewer_public = sig::PublicKey::from_bytes(
            key_bytes
                .try_into()
                .map_err(|_| "the viewer key is not 32 bytes".to_owned())?,
        );
        let sent = SentViewOnceRecord {
            message_id: report.message_id.clone(),
            sender_osl_user_id: report.sender_osl_user_id.clone(),
            viewer_osl_user_id: report.viewer_osl_user_id.clone(),
            viewer_device_id: report.viewer_device_id.clone(),
            viewer_public_key: viewer_public,
        };

        let sender_dir = dir.join("sender");
        let mut notifier = SenderCaptureNotifier::open(&sender_dir)?;

        // Reconnect: the queued event is delivered.
        let mut notifications: Vec<String> = Vec::new();
        match notifier.accept(&event, &sent)? {
            AcceptOutcome::Notify(sentence) => notifications.push(sentence),
            other => {
                return Err(format!(
                    "the real capture did not notify the sender: {other:?}"
                ))
            }
        }
        outbox.acknowledge(&event.binding.open_nonce)?;
        println!(
            "6844 delivery notified={} outbox_pending_after_ack={}",
            notifications.len(),
            outbox.pending_count()
        );

        // Replay. Twice, and through a sender restart, because a ledger held
        // only in memory would pass a single in-process replay.
        for round in 1..=2 {
            let mut replay_notifier = if starving("dedupe") {
                println!(
                    "6844 starved item=dedupe (the sender keeps no record of what it notified)"
                );
                SenderCaptureNotifier::open(&dir.join(format!("sender-forgetful-{round}")))?
            } else {
                SenderCaptureNotifier::open(&sender_dir)?
            };
            match replay_notifier.accept(&event, &sent)? {
                AcceptOutcome::AlreadyNotified => {}
                AcceptOutcome::Notify(sentence) => notifications.push(sentence),
                other => return Err(format!("replay {round} was refused oddly: {other:?}")),
            }
        }
        println!(
            "6844 replay rounds=2 notifications_total={}",
            notifications.len()
        );

        // Forgery: an adversary signs their own accusation, about a view-once
        // open they were never part of. It carries its own nonce, so nothing
        // but the signature check stands between it and a second notification.
        let (attacker_secret, attacker_public) = sig::generate_keypair();
        let mut forged_binding = event.binding.clone();
        rand::rngs::OsRng.fill_bytes(&mut forged_binding.open_nonce);
        let forged = sign_capture_event(
            forged_binding,
            event.path,
            event.observed_at_ms,
            event.evidence,
            &attacker_secret,
        )?;
        let adversarial_record = if starving("adversarial-event") {
            println!("6844 starved item=adversarial-event (the sender trusts whatever key the event carries)");
            SentViewOnceRecord {
                viewer_public_key: attacker_public,
                ..sent.clone()
            }
        } else {
            sent.clone()
        };
        match notifier.accept(&forged, &adversarial_record)? {
            AcceptOutcome::RejectedForgedSignature(reason) => {
                println!("6844 forgery rejected=true reason=\"{reason}\"")
            }
            AcceptOutcome::Notify(sentence) => notifications.push(sentence),
            other => return Err(format!("the forged event was refused oddly: {other:?}")),
        }

        // Tampering: the real event, re-pointed at another message.
        let mut other_message = event.clone();
        other_message.binding.message_id = "msg-6844-a-different-message".to_owned();
        match notifier.accept(&other_message, &sent)? {
            AcceptOutcome::RejectedWrongBinding(reason) => {
                println!("6844 another_message rejected=true reason=\"{reason}\"")
            }
            AcceptOutcome::Notify(sentence) => notifications.push(sentence),
            other => {
                return Err(format!(
                    "the other-message event was refused oddly: {other:?}"
                ))
            }
        }

        // The unsupported path produced no event in phase one, so there is
        // nothing here to deliver and nothing to notify.
        if report.unsupported_grab_observations != 0 {
            return Err(format!(
                "the unsupported path produced {} observation(s) while the viewer was open",
                report.unsupported_grab_observations
            ));
        }
        if report.unsupported_grab_bytes == 0 {
            return Err(
                "the unsupported path never actually captured anything, so its \
                        undetectability was not demonstrated"
                    .to_owned(),
            );
        }
        println!(
            "6844 unsupported_path label={} captured_bytes={} notifications_from_it=0",
            report.unsupported_path_label, report.unsupported_grab_bytes
        );

        if notifications.len() != 1 {
            return Err(format!(
                "the sender was notified {} times; exactly 1 is required",
                notifications.len()
            ));
        }
        let sentence = &notifications[0];
        require_honest_copy("notification", sentence)?;
        println!("6844 notification text=\"{sentence}\"");

        // A second restart: the acknowledged event does not come back and the
        // dedupe ledger still refuses the replay.
        let restarted_outbox = CaptureOutbox::open(&outbox_dir)?;
        if restarted_outbox.pending_count() != 0 {
            return Err(format!(
                "after acknowledgement and restart the viewer still holds {} event(s)",
                restarted_outbox.pending_count()
            ));
        }
        let mut restarted_notifier = SenderCaptureNotifier::open(&sender_dir)?;
        if restarted_notifier.accept(&event, &sent)? != AcceptOutcome::AlreadyNotified {
            return Err("after restart the sender would notify again for the same open".to_owned());
        }
        if restarted_notifier.notified_count() != 1 {
            return Err(format!(
                "the sender ledger holds {} notified opens, not 1",
                restarted_notifier.notified_count()
            ));
        }
        println!(
            "6844 second_restart outbox_pending=0 ledger_notified_opens={} replay=AlreadyNotified",
            restarted_notifier.notified_count()
        );

        // Nothing anywhere in this feature's copy may claim prevention or
        // universal detection.
        let sample = sign_capture_event(
            event.binding.clone(),
            SupportedCapturePath::SnipToClipboard,
            event.observed_at_ms,
            event.evidence,
            &attacker_secret,
        )?;
        require_honest_copy("snip-notification", &notification_sentence(&sample))?;

        println!(
            "6844 summary real_captures=1 notifications=1 replays_refused=2 forgeries_refused=1 \
             other_message_refused=1 unsupported_path_notifications=0"
        );
        Ok(())
    }

    pub fn run() -> Result<(), String> {
        let arguments: Vec<String> = std::env::args().collect();
        let value = |name: &str| -> Option<String> {
            arguments
                .iter()
                .position(|argument| argument == name)
                .and_then(|index| arguments.get(index + 1))
                .cloned()
        };
        let dir = value("--dir").ok_or_else(|| "--dir is required".to_owned())?;
        let phase = value("--phase").ok_or_else(|| "--phase is required".to_owned())?;
        let dir = PathBuf::from(dir);
        let knob = starve();
        if !knob.is_empty() {
            println!("6844 starvation={knob}");
        }
        match phase.as_str() {
            "open" => phase_open(&dir),
            "deliver" => phase_deliver(&dir),
            other => Err(format!("unknown phase {other}")),
        }
    }
}
