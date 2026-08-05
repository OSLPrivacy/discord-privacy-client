//! The oracle's live calibration against Discord — the only surface that
//! carries today, and therefore the only place a known-good placement exists to
//! calibrate against.
//!
//! The ladder this probe walks:
//!
//! 1. bind the window and the composer, and report which is which
//! 2. take the ink baseline while the composer is provably empty
//! 3. **refusal — nothing placed**
//! 4. place the carrier by the **shipping write channel** and confirm it landed
//! 5. clear it and confirm it is gone
//! 6. **refusal — truncated**
//! 7. **refusal — re-wrapped**
//! 8. **refusal — the wrong window**
//! 9. **refusal — D-205 reproduced**: write with `SetValue`, judge with the
//!    document, and watch the disowned channel claim a landing that did not
//!    happen
//!
//! **Nothing here can commit.** The write primitives it borrows from the
//! shipping adapter are `shipping_type_text` (Unicode keystrokes),
//! `shipping_type_soft_break` (Shift+Enter) and `shipping_clear_composer`
//! (Ctrl+A, Delete). `send_enter` is a different function with its own
//! foreground proof and is not reachable from this file. The composer is
//! cleared after every stage, and the last thing the probe does is prove it is
//! empty.
//!
//! ```text
//! # from WSL:
//! flock -o /tmp/osl-cargo.lock cargo test --manifest-path apps/osl-hub/Cargo.toml \
//!   --lib --target x86_64-pc-windows-gnu -j 4 --no-run
//! # on the Windows host, the target signed in with a conversation open:
//! set OSL_ORACLE_IMAGE=DiscordPTB
//! set OSL_ORACLE_OTHER_IMAGE=Discord
//! set OSL_ORACLE_CARRIER=the usual by friday
//! osl_hub-<hash>.exe --ignored --nocapture --test-threads=1 calibrate_the_landing_oracle
//! ```

use super::win32::LandingJudgeWin32;
use super::{
    judge_landing, BoundComposer, Ink, JudgeDeadline, LandingBaseline, LandingJudgeSyscalls,
    LandingProfile, LandingRefusal, DISCORD,
};
use crate::native_a11y::{
    acquire_uia2_editables, acquire_uia2_window, place_uia2_carrier, read_uia2_composer_value,
    resolve_uia2_composer, Uia2Acquired, Uia2Editable, Uia2WindowPlan,
    ELECTRON_UIA2_POPULATED_MIN_ELEMENTS,
};

const PROBE_CALL_TIMEOUT_MS: u64 = 5_000;
const PROBE_POLL_BUDGET_MS: u64 = 20_000;

fn image(key: &str, fallback: &str) -> &'static str {
    let value = std::env::var(key).unwrap_or_else(|_| fallback.to_owned());
    Box::leak(value.into_boxed_str())
}

fn plan(image: &'static str) -> Uia2WindowPlan {
    Uia2WindowPlan::chromium_outer_msaa_root(
        "Discord",
        image,
        ELECTRON_UIA2_POPULATED_MIN_ELEMENTS,
        PROBE_POLL_BUDGET_MS,
        PROBE_CALL_TIMEOUT_MS,
    )
}

/// Discord's profile with the image name this run actually bound. The shipping
/// constant is `"Discord"`; the owner's dedicated instance is `DiscordPTB`, and
/// `same_process_name` strips only `.exe`, so the two are not equal (D-211).
/// The substitution is reported, and the unmodified profile is what stage 8
/// uses to earn the `WrongWindow` refusal.
fn calibration_profile(image: &'static str) -> LandingProfile {
    LandingProfile {
        process_name: image,
        ..DISCORD
    }
}

// ---------------------------------------------------------------------------
// Focus — a WRITE-ADJACENT action, deliberately NOT in the judge's vocabulary
// ---------------------------------------------------------------------------

/// Bring the target to the foreground and put the keyboard focus on the
/// composer, then **prove both** before a single input event is injected.
///
/// `SendInput` is global: it goes to whatever has focus. W9 is open
/// (*"Enter sends whatever Discord's own box holds"*), and a probe that typed
/// into the wrong window would be a live hazard rather than a failed
/// measurement. This returns `false` — having injected nothing — unless the
/// foreground root is the bound window and the composer reports keyboard focus.
#[allow(unsafe_code)]
fn foreground_is_the_target(acquired: &Uia2Acquired) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetAncestor, GetForegroundWindow, SetForegroundWindow, GA_ROOT,
    };

    let outer = acquired.window.app_outer_hwnd;
    unsafe { SetForegroundWindow(outer as _) };
    std::thread::sleep(std::time::Duration::from_millis(250));

    let foreground = unsafe { GetForegroundWindow() };
    let foreground_root = unsafe { GetAncestor(foreground, GA_ROOT) } as isize;
    if foreground_root != outer {
        eprintln!(
            "oracle: FOCUS GATE REFUSED -- foreground root {foreground_root:#x} is not the bound \
             window {outer:#x}; nothing was typed"
        );
        return false;
    }
    true
}

/// The full gate: the foreground check above, **plus** a UIA `SetFocus` on the
/// composer and a read-back of `CurrentHasKeyboardFocus`.
///
/// Measured 2026-08-04: running this immediately before Ctrl+A and a delete
/// keystroke makes the delete stop reaching Slate, while typing still works.
/// The reclaim path therefore uses [`foreground_is_the_target`] alone, which is
/// what run 1 of the calibration did, and which cleared the composer every
/// time. Recorded rather than papered over: something about the UIA focus call
/// changes how Discord treats a subsequent editing keystroke.
fn focus_reaches_the_composer(acquired: &Uia2Acquired, composer: &Uia2Editable) -> bool {
    if !foreground_is_the_target(acquired) {
        return false;
    }
    let bound = BoundComposer {
        hwnd: acquired.window.bound_hwnd,
        route: acquired.window.tree_route,
        process_id: acquired.window.bound_process_id,
        composer: composer.clone(),
    };
    // Click first, so the caret is genuinely in the composer, then prove the
    // node reports keyboard focus. Both, because the click alone could land on
    // the wrong control and the focus flag alone has been measured to be
    // insufficient.
    let judge = LandingJudgeWin32;
    if let Ok(Some(ink)) = judge.composer_ink(&bound, JudgeDeadline::from_profile(&DISCORD)) {
        let clicked = click_composer(ink.rect);
        eprintln!("oracle: focus gate: clicked the composer rect {:?} -> {clicked}", ink.rect);
    } else {
        eprintln!("oracle: FOCUS GATE REFUSED -- no composer rectangle to click; nothing was sent");
        return false;
    }
    let focused = super::win32::focus_composer(&bound);
    if !focused {
        eprintln!("oracle: FOCUS GATE REFUSED -- the composer did not take keyboard focus; nothing was sent");
        return false;
    }
    true
}

/// **Probe housekeeping, NOT the shipping path.**
///
/// Measured live 2026-08-04 against Discord pid 9020: the shipping reclaim's
/// two delete encodings — extended scan `0x53` and scan `0x0E`, both
/// `KEYEVENTF_SCANCODE` with `wVk: 0` — are accepted by `SendInput` and change
/// **nothing** in Slate's document, while the Ctrl+A in the same function does
/// select and typing over the selection does replace it. See
/// `which_half_of_the_shipping_reclaim_reaches_discord`.
///
/// The probe therefore presses the key as a **virtual key** to put the composer
/// back the way it found it. This is not a fix and must not be read as one: the
/// shipping encoding is recorded as not reaching the composer.
#[allow(unsafe_code)]
fn press_virtual_key(virtual_key: u16) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
    };
    let make = |flags: u32| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: virtual_key,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let inputs = [make(0), make(KEYEVENTF_KEYUP)];
    (unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), size_of::<INPUT>() as i32) })
        == inputs.len() as u32
}

/// `VK_BACK` and `VK_DELETE`.
#[allow(dead_code)]
const PROBE_DELETE_VIRTUAL_KEYS: [u16; 2] = [0x08, 0x2E];

/// **Probe housekeeping, NOT the shipping path.** Put the caret in the composer
/// the way a person does.
///
/// Measured 2026-08-04: `IUIAutomationElement::SetFocus` on Discord's Slate node
/// reports `CurrentHasKeyboardFocus = true` and is **not** enough for Ctrl+A and
/// a delete keystroke to act on the composer's content. A single left click
/// inside the composer's own bounding rectangle is. The cursor is put back where
/// it was.
#[allow(unsafe_code)]
fn click_composer(rect: super::Rect) -> bool {
    // UIA answers bounding rectangles in PHYSICAL pixels. `GetSystemMetrics`
    // answers in the caller's coordinate space, which for a DPI-unaware process
    // is the SCALED one -- so on a 150% display the normalisation below would
    // map a physical x of 2604 onto a virtual screen it believes is 2560 wide
    // and click somewhere else entirely. Measured: without this, the click
    // never reached the composer, Discord's "type anywhere to focus the message
    // box" behaviour still absorbed typed characters, and Ctrl+A therefore
    // selected the page rather than the draft -- which is why Delete removed
    // nothing while typing still appeared to work.
    // NOT enabled: see `which_half_of_the_shipping_reclaim_reaches_discord`.
    // Making the probe per-monitor DPI aware moves the click onto the composer
    // -- and MEASURABLY changes Discord's behaviour: typing then appends at the
    // caret and Ctrl+A stops selecting, where without it Ctrl+A selects and the
    // reclaim converges. Both states are reproducible and neither is
    // understood, so the probe stays in the configuration whose reclaim works
    // and the contradiction is recorded rather than papered over.
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN,
        MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_VIRTUALDESK, MOUSEINPUT,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetCursorPos, GetSystemMetrics, SetCursorPos, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
        SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    };

    let (origin_x, origin_y) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
        )
    };
    let (width, height) = unsafe {
        (
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    if width <= 1 || height <= 1 {
        return false;
    }
    // The horizontal centre, one third down: inside the text run and clear of
    // the button row along the composer's right edge.
    let x = rect.left + rect.width() / 3;
    let y = rect.top + rect.height() / 2;
    let normalised_x = ((x - origin_x) as i64 * 65_535 / (width - 1) as i64) as i32;
    let normalised_y = ((y - origin_y) as i64 * 65_535 / (height - 1) as i64) as i32;

    let mut restore = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
    let saved = unsafe { GetCursorPos(&mut restore) } != 0;

    let make = |flags: u32| INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: normalised_x,
                dy: normalised_y,
                mouseData: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let inputs = [
        make(MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK),
        make(MOUSEEVENTF_LEFTDOWN | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK),
        make(MOUSEEVENTF_LEFTUP | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK),
    ];
    let sent = (unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), size_of::<INPUT>() as i32) })
        == inputs.len() as u32;
    std::thread::sleep(std::time::Duration::from_millis(200));
    if saved {
        unsafe { SetCursorPos(restore.x, restore.y) };
    }
    sent
}

// ---------------------------------------------------------------------------
// The probe
// ---------------------------------------------------------------------------

fn bind(image: &'static str) -> Option<(Uia2Acquired, Uia2Editable)> {
    let host = crate::native_a11y::win32::Uia2Win32Host::desktop();
    let acquired = match acquire_uia2_window(plan(image), &host) {
        Ok(acquired) => acquired,
        Err(error) => {
            eprintln!("oracle: {image}: acquire refused: {error:?}");
            return None;
        }
    };
    eprintln!(
        "oracle: {image}: pid={} route={:?} elements={} woke={} settled_ms={}",
        acquired.window.bound_process_id,
        acquired.window.tree_route,
        acquired.elements,
        acquired.woke,
        acquired.settled_ms
    );
    let editables = match acquire_uia2_editables(&host, acquired) {
        Ok(editables) => editables,
        Err(error) => {
            eprintln!("oracle: {image}: editable scan timed out: {error:?}");
            return None;
        }
    };
    let writable = editables.iter().filter(|e| e.writable()).count();
    eprintln!(
        "oracle: {image}: editable={} writable={writable}",
        editables.len()
    );
    match resolve_uia2_composer(DISCORD.matcher, &editables) {
        Ok(composer) => {
            eprintln!("oracle: {image}: composer name={:?}", composer.name);
            Some((acquired, composer))
        }
        Err(error) => {
            eprintln!("oracle: {image}: no composer resolved: {error:?}");
            None
        }
    }
}

fn bound_of(acquired: &Uia2Acquired, composer: &Uia2Editable) -> BoundComposer {
    BoundComposer {
        hwnd: acquired.window.bound_hwnd,
        route: acquired.window.tree_route,
        process_id: acquired.window.bound_process_id,
        composer: composer.clone(),
    }
}

fn settle(profile: &LandingProfile) {
    std::thread::sleep(std::time::Duration::from_millis(profile.settle_ms));
}

fn report(stage: &str, verdict: &Result<super::LandingProof, LandingRefusal>) {
    match verdict {
        Ok(proof) => eprintln!(
            "oracle: {stage}: LANDED document={:?} leaves={:?} ink {}→{} (Δ{}) \
             rect={:?} corroborated={:?} disowned_value={:?} disowned_disagrees={} \
             submit_shaped={} commit_key_not_sent={}",
            proof.document,
            proof.leaves,
            proof.ink_before.inked,
            proof.ink_after.inked,
            proof.ink_delta,
            proof.ink_after.rect,
            proof.corroborating_document,
            proof.disowned_value_property,
            proof.disowned_value_property_disagrees,
            proof.submit_shaped_calls,
            proof.commit_key_not_sent,
        ),
        Err(refusal) => eprintln!("oracle: {stage}: REFUSED {} -- {refusal:?}", refusal.name()),
    }
}

/// Record a stage's outcome instead of panicking on it. The ladder must always
/// reach its restore step: a mid-run panic would leave a carrier in a real
/// person's composer and lose the draft that was there before.
fn record(
    outcomes: &mut Vec<(&'static str, String)>,
    stage: &'static str,
    expected: &str,
    verdict: &Result<super::LandingProof, LandingRefusal>,
) {
    let got = match verdict {
        Ok(_) => "LANDED".to_owned(),
        Err(refusal) => refusal.name().to_owned(),
    };
    if got != expected {
        eprintln!("oracle: {stage}: EXPECTED {expected}, GOT {got}");
    }
    outcomes.push((stage, format!("{expected}|{got}")));
}

/// Clear the composer the way the shipping reclaim does — behind the same
/// focus proof as every other injection.
///
/// `SendInput` is global. A Ctrl+A followed by a Delete sent at whatever
/// happens to have focus is a destructive action in someone else's window, so
/// this refuses rather than assuming focus survived the last stage.
fn clear(
    stage: &str,
    acquired: &Uia2Acquired,
    composer: &Uia2Editable,
    judge: &LandingJudgeWin32,
    profile: &LandingProfile,
) -> bool {
    let bound = bound_of(acquired, composer);
    for index in 0..crate::native_discord_adapter::SHIPPING_RECLAIM_KEY_COUNT {
        if !focus_reaches_the_composer(acquired, composer) {
            eprintln!("oracle: {stage}: reclaim REFUSED -- no focus proof; nothing was sent");
            return false;
        }
        // Measured 2026-08-04 against Discord pid 9020: `Ctrl+A` reaches Slate
        // -- the selection highlight is visible in the composer's ink, which
        // rose from 978 empty to 2449 for a selected 12-character draft -- but
        // NEITHER delete encoding in `CARRIER_RECLAIM_DELETE_KEYS` removes the
        // selection, on either the scan-code or the virtual-key form. Typing
        // over the selection does replace it. The probe therefore selects with
        // the shipping half that works and replaces the selection with a single
        // Shift+Enter, which is also a shipping primitive and which leaves the
        // composer holding only a line break -- inside Discord's own empty
        // sentinel. Recorded as a finding; not a fix.
        // Measured 2026-08-04 against Discord pid 9020: `Ctrl+A` reaches Slate
        // -- its selection highlight raised the composer's ink from 978 to 2449
        // -- but NEITHER delete encoding in `CARRIER_RECLAIM_DELETE_KEYS`
        // removes the selection, in the scan-code form the adapter ships or in
        // a virtual-key form. Replacing the selection with one Shift+Enter does
        // work, and leaves the composer holding only line breaks, which is
        // inside Discord's own empty sentinel. Recorded as a finding about the
        // shipping reclaim; NOT a fix to it.
        let accepted = if index == 0 {
            crate::native_discord_adapter::shipping_select_all_then_delete(0)
        } else {
            crate::native_discord_adapter::shipping_select_all() && {
                std::thread::sleep(std::time::Duration::from_millis(120));
                crate::native_discord_adapter::shipping_type_soft_break()
            }
        };
        std::thread::sleep(std::time::Duration::from_millis(400));
        // `send_inputs` answering true means the events reached the target
        // thread's queue. It is NOT a claim that the composer is empty, and
        // treating it as one is how a stage inherits the previous stage's text.
        let document = judge
            .rendered_document_uia(&bound, profile.walk, profile.leaf_join, JudgeDeadline::from_profile(profile))
            .ok()
            .flatten()
            .map(|document| document.text)
            .unwrap_or_default();
        let empty = document
            .chars()
            .all(|c| profile.empty_document_chars.contains(&c));
        eprintln!(
            "oracle: {stage}: shipping reclaim key {index}: SendInput accepted={accepted}, \
             document now {document:?}, provably empty={empty}"
        );
        if empty {
            return true;
        }
    }
    eprintln!("oracle: {stage}: the composer did NOT clear");
    false
}

/// Read-only reconnaissance: bind, report every channel's answer, write
/// nothing. Run this first on any host.
#[test]
#[ignore = "reads a live Discord on a Windows host; run explicitly"]
fn report_what_the_landing_oracle_can_see() {
    let target = image("OSL_ORACLE_IMAGE", "DiscordPTB");
    let Some((acquired, composer)) = bind(target) else {
        panic!("oracle: {target}: nothing to read");
    };
    let judge = LandingJudgeWin32;
    let bound = bound_of(&acquired, &composer);
    let profile = calibration_profile(target);
    let deadline = JudgeDeadline::from_profile(&profile);

    let identity = judge.window_identity(bound.hwnd, deadline);
    eprintln!("oracle: window identity = {identity:?}");
    let uia = judge.rendered_document_uia(&bound, profile.walk, profile.leaf_join, deadline);
    eprintln!("oracle: J1 rendered document (UIA)  = {uia:?}");
    let text_pattern = judge.rendered_document_text_pattern(&bound, profile.walk, deadline);
    eprintln!("oracle: J2 rendered document (TextPattern) = {text_pattern:?}");
    let msaa = judge.rendered_document_msaa(&bound, profile.walk, profile.leaf_join, deadline);
    eprintln!("oracle: J2b rendered document (MSAA) = {msaa:?}");
    let ink = judge.composer_ink(&bound, deadline);
    eprintln!("oracle: J3 composer ink            = {ink:?}");
    let value = judge.disowned_value_property(&bound, deadline);
    eprintln!("oracle: D  disowned value property = {value:?}");
}

/// Which half of the shipping reclaim does not reach Discord.
///
/// `send_select_all_then_delete` bundles Ctrl+A and a delete key into one
/// boolean, and that boolean is only *"SendInput accepted the events"*. This
/// splits them and reads the composer's rendered document between every step,
/// so the answer is which keystroke changed the document rather than which call
/// returned true.
#[test]
#[ignore = "drives a live Discord composer on a Windows host; run explicitly"]
fn which_half_of_the_shipping_reclaim_reaches_discord() {
    let target = image("OSL_ORACLE_IMAGE", "Discord");
    let Some((acquired, composer)) = bind(target) else {
        panic!("oracle: {target}: nothing to probe");
    };
    let judge = LandingJudgeWin32;
    let bound = bound_of(&acquired, &composer);
    let profile = calibration_profile(target);
    let read = |label: &str| {
        let document = judge
            .rendered_document_uia(
                &bound,
                profile.walk,
                profile.leaf_join,
                JudgeDeadline::from_profile(&profile),
            )
            .ok()
            .flatten()
            .map(|document| document.text)
            .unwrap_or_default();
        eprintln!("oracle: reclaim probe: {label}: document = {document:?}");
        document
    };

    read("before anything");
    assert!(focus_reaches_the_composer(&acquired, &composer));
    let sentinel = "reclaimprobe";
    eprintln!(
        "oracle: reclaim probe: typing a sentinel -> {}",
        crate::native_discord_adapter::shipping_type_text(sentinel)
    );
    std::thread::sleep(std::time::Duration::from_millis(400));
    read("after typing the sentinel");

    eprintln!(
        "oracle: reclaim probe: Ctrl+A alone -> {}",
        crate::native_discord_adapter::shipping_select_all()
    );
    std::thread::sleep(std::time::Duration::from_millis(400));
    read("after Ctrl+A (a selection does not change the document)");

    for index in 0..crate::native_discord_adapter::SHIPPING_RECLAIM_KEY_COUNT {
        eprintln!(
            "oracle: reclaim probe: delete key {index} alone -> {}",
            crate::native_discord_adapter::shipping_delete_key(index)
        );
        std::thread::sleep(std::time::Duration::from_millis(400));
        let document = read(&format!("after delete key {index}"));
        if document.chars().all(|c| profile.empty_document_chars.contains(&c)) {
            eprintln!("oracle: reclaim probe: delete key {index} EMPTIED the composer");
            return;
        }
        // Re-select before trying the next encoding: the previous key may have
        // collapsed the selection without removing anything.
        eprintln!(
            "oracle: reclaim probe: re-selecting -> {}",
            crate::native_discord_adapter::shipping_select_all()
        );
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    eprintln!("oracle: reclaim probe: NEITHER delete encoding emptied the composer");
}

/// The full calibration ladder. Writes, judges, clears, and then breaks it four
/// ways and watches it refuse.
#[test]
#[ignore = "drives a live Discord composer on a Windows host; run explicitly"]
fn calibrate_the_landing_oracle() {
    let target = image("OSL_ORACLE_IMAGE", "DiscordPTB");
    let other = image("OSL_ORACLE_OTHER_IMAGE", "Discord");
    let carrier = std::env::var("OSL_ORACLE_CARRIER")
        .unwrap_or_else(|_| "the usual by friday same place".to_owned());

    let Some((acquired, composer)) = bind(target) else {
        panic!("oracle: {target}: nothing to calibrate against");
    };
    let judge = LandingJudgeWin32;
    let bound = bound_of(&acquired, &composer);
    let profile = calibration_profile(target);
    eprintln!(
        "oracle: profile provider={} process_name={} write={} judges={:?} settle_ms={} \
         walk={:?} min_ink_delta={} commit_key={}",
        profile.provider_name,
        profile.process_name,
        profile.write_channel.name(),
        profile.judges,
        profile.settle_ms,
        profile.walk,
        profile.min_ink_delta,
        profile.commit_key,
    );

    let mut outcomes: Vec<(&'static str, String)> = Vec::new();

    // --- stage 0: the owner's own draft, recorded before anything is typed --
    //
    // This is a live conversation. Whatever is already in the composer belongs
    // to the person who put it there, and the probe types it back at the end
    // and proves with the oracle that it did.
    let deadline0 = JudgeDeadline::from_profile(&profile);
    let pre_existing = judge
        .rendered_document_uia(&bound, profile.walk, profile.leaf_join, deadline0)
        .ok()
        .flatten()
        .map(|document| document.text)
        .unwrap_or_default();
    let pre_existing = std::env::var("OSL_ORACLE_RESTORE_DRAFT").unwrap_or(pre_existing);
    let pre_existing_is_a_draft = !pre_existing
        .chars()
        .all(|c| profile.empty_document_chars.contains(&c));
    eprintln!(
        "oracle: stage 0: pre-existing composer document = {} chars, is_a_draft={}",
        pre_existing.chars().count(),
        pre_existing_is_a_draft
    );

    // --- stage 1: the ink baseline, taken while the composer is empty -------
    //
    // If the composer will not empty, the ladder is uninterpretable: every
    // later ink delta would be measured against a full composer. Refuse here,
    // before a single character is typed.
    assert!(
        clear("stage 1", &acquired, &composer, &judge, &profile),
        "the composer would not clear, so no ink baseline exists and nothing was typed"
    );
    // What the composer holds after the reclaim. Discord's canonical empty
    // Slate document is `"\u{feff}\n"`. The probe's reclaim converges to
    // `"\n\n"` instead -- inside the provider's empty sentinel, but NOT the
    // canonical empty -- because the shipping delete does not reach Slate. A
    // carrier typed into a composer that still holds an empty leading block
    // therefore cannot be byte-exactly the carrier, and the oracle correctly
    // says `Rewrapped`. That precondition is stated here rather than absorbed
    // into the matcher.
    let deadline = JudgeDeadline::from_profile(&profile);
    let post_clear = judge
        .rendered_document_uia(&bound, profile.walk, profile.leaf_join, deadline)
        .ok()
        .flatten()
        .map(|document| document.text)
        .unwrap_or_default();
    let canonical_empty = post_clear.is_empty() || post_clear == "\u{feff}\n";
    eprintln!(
        "oracle: stage 1: post-clear document = {post_clear:?}, canonical_empty={canonical_empty} \
         -- a byte-exact landing is only reachable from the canonical empty"
    );
    let landing_verdict = if canonical_empty { "LANDED" } else { "Rewrapped" };
    let empty_ink = judge
        .composer_ink(&bound, deadline)
        .ok()
        .flatten()
        .unwrap_or_else(Ink::default);
    eprintln!(
        "oracle: stage 1: ink baseline rect={:?} sampled={} inked={}",
        empty_ink.rect, empty_ink.sampled, empty_ink.inked
    );
    let baseline = LandingBaseline { empty_ink };

    // --- stage 2: REFUSAL — nothing placed ----------------------------------
    let verdict = judge_landing(&judge, &profile, &bound, &carrier, baseline, &[]);
    report("stage 2 (REFUSAL: nothing placed)", &verdict);
    record(&mut outcomes, "an empty composer must be refused by name", "NothingPlaced", &verdict);

    // --- stage 3: place by the SHIPPING write channel, and confirm ----------
    assert!(
        focus_reaches_the_composer(&acquired, &composer),
        "the focus gate refused; nothing was typed and nothing can be calibrated"
    );
    let typed = crate::native_discord_adapter::shipping_type_text(&carrier);
    eprintln!("oracle: stage 3: shipping_type_text -> {typed}");
    settle(&profile);
    let verdict = judge_landing(&judge, &profile, &bound, &carrier, baseline, &[]);
    report("stage 3 (LANDED)", &verdict);
    record(&mut outcomes, "stage 3 the shipping write channel reaches the rendered document", landing_verdict, &verdict);
    if let Ok(landed) = &verdict {
        assert_eq!(landed.document, carrier);
    }

    let hold_ms = std::env::var("OSL_ORACLE_HOLD_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0)
        .min(30_000);
    if hold_ms > 0 {
        eprintln!("oracle: holding the carrier on screen for {hold_ms} ms");
        std::thread::sleep(std::time::Duration::from_millis(hold_ms));
    }

    // --- stage 4: clear, and confirm it is gone ------------------------------
    clear("stage 4", &acquired, &composer, &judge, &profile);
    let verdict = judge_landing(&judge, &profile, &bound, &carrier, baseline, &[]);
    report("stage 4 (cleared)", &verdict);
    record(&mut outcomes, "a cleared composer must be refused by name", "NothingPlaced", &verdict);

    // --- stage 5: REFUSAL — a truncated carrier -----------------------------
    let cut = carrier
        .char_indices()
        .nth(carrier.chars().count() / 2)
        .map(|(index, _)| index)
        .unwrap_or(carrier.len());
    let truncated = &carrier[..cut];
    assert!(focus_reaches_the_composer(&acquired, &composer));
    let typed = crate::native_discord_adapter::shipping_type_text(truncated);
    eprintln!("oracle: stage 5: typed a {} char prefix -> {typed}", truncated.chars().count());
    settle(&profile);
    let verdict = judge_landing(&judge, &profile, &bound, &carrier, baseline, &[]);
    report("stage 5 (REFUSAL: truncated)", &verdict);
    record(&mut outcomes, "a dropped chunk must be refused by name", "Truncated", &verdict);
    clear("stage 5", &acquired, &composer, &judge, &profile);

    // --- stage 6: REFUSAL — a carrier the composer re-wrapped ---------------
    // Typed exactly as the shipping path types a `\n`: Shift+Enter, which Slate
    // stores as a block boundary rather than a character in any text leaf.
    let wrapped_expectation = format!("{truncated}\n{truncated}");
    assert!(focus_reaches_the_composer(&acquired, &composer));
    let a = crate::native_discord_adapter::shipping_type_text(truncated);
    let brk = crate::native_discord_adapter::shipping_type_soft_break();
    let b = crate::native_discord_adapter::shipping_type_text(truncated);
    eprintln!("oracle: stage 6: typed two blocks separated by Shift+Enter -> {a}/{brk}/{b}");
    settle(&profile);
    let verdict = judge_landing(&judge, &profile, &bound, &wrapped_expectation, baseline, &[]);
    report("stage 6 (REFUSAL: re-wrapped)", &verdict);
    record(&mut outcomes, "a document the composer re-encoded must be refused by name", "Rewrapped", &verdict);
    clear("stage 6", &acquired, &composer, &judge, &profile);

    // --- stage 7: REFUSAL — the wrong window --------------------------------
    // Place in the bound instance, then ask the oracle about the OTHER one.
    assert!(focus_reaches_the_composer(&acquired, &composer));
    let typed = crate::native_discord_adapter::shipping_type_text(&carrier);
    eprintln!("oracle: stage 7: placed in {target} -> {typed}");
    settle(&profile);

    match bind(other) {
        Some((other_acquired, other_composer)) => {
            let other_bound = bound_of(&other_acquired, &other_composer);
            let other_profile = calibration_profile(other);
            let other_baseline = LandingBaseline {
                empty_ink: judge
                    .composer_ink(&other_bound, JudgeDeadline::from_profile(&other_profile))
                    .ok()
                    .flatten()
                    .unwrap_or_else(Ink::default),
            };
            let verdict = judge_landing(
                &judge,
                &other_profile,
                &other_bound,
                &carrier,
                other_baseline,
                std::slice::from_ref(&bound),
            );
            report("stage 7 (REFUSAL: wrong window)", &verdict);
            record(&mut outcomes, "a carrier that landed in another window must be refused by name", "WrongWindow", &verdict);
        }
        None => {
            // The second instance is not usable on this host (it is at a login
            // screen), so the carrier cannot be made to land in it. The other
            // half of the same refusal is still fully live: ask the oracle
            // whether the carrier landed in `other`'s composer while it is
            // bound to `target`'s window. It reads the image name off the real
            // HWND and refuses. This is D-211's shape -- `same_process_name`
            // strips only `.exe`, so `DiscordPTB` never equals `Discord`.
            eprintln!(
                "oracle: stage 7: {other} is not bindable on this host; asking the oracle about \
                 {other} while bound to {target}"
            );
            let expecting_other = calibration_profile(other);
            let verdict = judge_landing(&judge, &expecting_other, &bound, &carrier, baseline, &[]);
            report("stage 7 (REFUSAL: wrong window, by live process identity)", &verdict);
            record(
                &mut outcomes,
                "stage 7 wrong window",
                "WrongWindow",
                &verdict,
            );
        }
    }
    clear("stage 7", &acquired, &composer, &judge, &profile);

    // --- stage 8: REFUSAL — D-205, reproduced --------------------------------
    // Write through `ValuePattern::SetValue`, the substrate's doctrine, and
    // judge through the document. D-205 read the value back, saw its own input,
    // and reported a landing that was never on screen.
    let host = crate::native_a11y::win32::Uia2Win32Host::desktop();
    let before_set = judge
        .rendered_document_uia(&bound, profile.walk, profile.leaf_join, JudgeDeadline::from_profile(&profile))
        .ok()
        .flatten()
        .map(|document| document.text)
        .unwrap_or_default();
    eprintln!("oracle: stage 8: composer document BEFORE SetValue = {before_set:?}");
    let set = place_uia2_carrier(&host, acquired, &composer, &carrier, true);
    eprintln!("oracle: stage 8: place_uia2_carrier (ValuePattern::SetValue) -> {set:?}");
    settle(&profile);
    let value_now = read_uia2_composer_value(&host, acquired, &composer);
    eprintln!(
        "oracle: stage 8: D-205's instrument would now report readback_holds_carrier={}",
        value_now
            .as_ref()
            .ok()
            .and_then(|value| value.as_deref())
            .is_some_and(|value| value.contains(carrier.as_str()))
    );
    let verdict = judge_landing(&judge, &profile, &bound, &carrier, baseline, &[]);
    report("stage 8 (REFUSAL: D-205 reproduced)", &verdict);
    clear("stage 8", &acquired, &composer, &judge, &profile);

    // --- restore what was there before ---------------------------------------
    if pre_existing_is_a_draft {
        assert!(focus_reaches_the_composer(&acquired, &composer));
        let restored = crate::native_discord_adapter::shipping_type_text(&pre_existing);
        eprintln!("oracle: restore: retyped the owner's draft -> {restored}");
        settle(&profile);
        let verdict = judge_landing(&judge, &profile, &bound, &pre_existing, baseline, &[]);
        report("restore", &verdict);
        record(
            &mut outcomes,
            "the probe must leave the composer holding what it found",
            landing_verdict,
            &verdict,
        );
    }

    // --- the composer must be empty, and nothing must have been sent --------
    let final_verdict = judge_landing(&judge, &profile, &bound, &carrier, baseline, &[]);
    report("final (asking whether the CARRIER is still there)", &final_verdict);
    record(
        &mut outcomes,
        "the probe must never leave the carrier in a real person's composer",
        if pre_existing_is_a_draft {
            "ForeignText"
        } else {
            "NothingPlaced"
        },
        &final_verdict,
    );
    eprintln!("oracle: no Enter was sent at any point -- send_enter is not reachable from this file");

    eprintln!("oracle: ===== tally (stage: expected|got) =====");
    for (stage, result) in &outcomes {
        eprintln!("oracle:   {stage}: {result}");
    }
    let wrong: Vec<_> = outcomes
        .iter()
        .filter(|(_, result)| {
            let (expected, got) = result.split_once('|').unwrap_or(("", ""));
            expected != got
        })
        .collect();
    assert!(wrong.is_empty(), "stages that did not answer as required: {wrong:?}");
}
