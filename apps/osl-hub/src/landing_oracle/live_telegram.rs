//! The oracle on **Telegram** — the third surface it has been pointed at, and
//! the first that is not Chromium at all.
//!
//! # Why this file exists separately from `live.rs` and `live_whatsapp.rs`
//!
//! `live.rs` is Discord's calibration: an Electron outer window on the MSAA
//! bridge, written by `SendInput`, every injection behind a foreground-plus-focus
//! proof because `SendInput` is global. `live_whatsapp.rs` is a sibling WebView2
//! renderer on the `UiaNative` route, written by `SetValue`. **Telegram is
//! neither.** It is **Qt** — `Qt51519QWindowIcon` is itself the UIA root, there
//! is no `Chrome_RenderWidgetHostHWND` below it, nothing to wake, and no
//! Chromium accessibility subtree anywhere. Every number Discord and WhatsApp
//! measured is therefore inadmissible here, which is exactly what
//! `ProfileLookup::Unmeasured` says about this provider today.
//!
//! **There is no `SendInput` in this file, no key, no click, no `Invoke`, no
//! `PostMessage`.** The only write verbs reachable from here are
//! `place_uia2_carrier` and `clear_uia2_composer` — `SetValue` and
//! `SetValue("")` — which is precisely the shipping placement path
//! `native_telegram_adapter::place_through_substrate` drives. On Telegram the
//! newline *is* the send, and `native_a11y::uia2_carrier_carries_submit`
//! refuses a carrier containing one before it reaches a live composer.
//!
//! # Telegram's shipping path can place WITHOUT sending, and Discord's cannot
//!
//! This distinction cost another lane a day (D-233 B). Discord's `place()` ends
//! in `send_enter`, and `DiscordCarrierStatus::Sent` is *defined* as
//! `placed && enter_sent` — so "place by the shipping path but send nothing"
//! names a path Discord does not have. **Telegram's does.**
//! `place_through_substrate` (`native_telegram_adapter.rs`) resolves the Qt
//! window, lists editables, matches the composer and calls `place_uia2_carrier`.
//! There is no send verb in the module or in the `Uia2Syscalls` seam it drives,
//! and `enter_sent` can only be set from a **backend-reported** submit-shaped
//! delta. That is why `LiveCarryReceipt` can require `enter_sent == false` and
//! still be earned through the shipping write on this surface.
//!
//! # The ladder, and why it is ordered this way
//!
//! **A placement that cannot be reclaimed must never happen.** D-228 measured a
//! Discord composer the shipping reclaim could not empty, and the residue is
//! still in the owner's chat. So:
//!
//! 1. **recon, read-only** — bind, report every channel's answer for every
//!    writable element, poll for stability so no finding rests on a single read
//!    (D-226), and take the composer's empty-state document and ink baseline.
//!    Writes nothing, presses nothing.
//! 2. **the clear canary, on the chat-list SEARCH BOX** — the one writable
//!    element on this surface that is not a conversation, and the one
//!    `TELEGRAM_COMPOSER_MATCHER` refuses **by name** (`"search"` is a rejecting
//!    stem). If the clear fails there, the residue is a chat-list filter rather
//!    than text in a real conversation. It also yields the ink-per-character
//!    figure a profile's `min_ink_delta` is derived from, measured on an element
//!    the oracle never judges.
//! 3. **the two-character canary in the composer**, placed and proven removed
//!    before any full-size cover text exists in a real chat.
//! 4. only then the carrier, and only then a receipt.
//!
//! # The Qt question this file exists to answer
//!
//! `walk_leaves_uia` reads a leaf's **`CurrentName`**, because Chromium
//! publishes a static text node's content there. **Qt does not have to.** A Qt
//! text control's accessible *name* is typically its label or placeholder —
//! here, `"Write a message..."` — while its content is served through
//! `ITextProvider`. If J1 publishes the name, it is a constant that never moves
//! with the document, and the oracle's primary channel would be blind on this
//! surface. That is a measurement, not a guess, and the recon step takes it
//! before anything is written. **Whatever it says is reported; nothing is
//! widened to make a channel appear to work.**

use super::win32::LandingJudgeWin32;
use super::{
    BoundComposer, Ink, JudgeChannel, JudgeDeadline, LandingJudgeSyscalls, LandingProfile,
    WalkCaps, WriteChannel,
};
use crate::native_a11y::{
    acquire_uia2_editables, acquire_uia2_window, clear_uia2_composer, place_uia2_carrier,
    read_uia2_composer_value, resolve_uia2_composer, Uia2Acquired, Uia2Editable,
};
use crate::native_apps::NativeAppId;
use crate::native_telegram_adapter::{
    TELEGRAM_COMPOSER_MATCHER, TELEGRAM_DESKTOP_PROCESS_NAME, TELEGRAM_UIA2_WINDOW_PLAN,
};

/// **Reads only.** A profile is required to construct a [`JudgeDeadline`] and a
/// [`WalkCaps`] and for nothing else: this value is never passed to
/// `judge_landing` or `judge_empty_composer`. The measured profile those take
/// lives in `landing_oracle.rs` and is pinned from what this probe reports.
///
/// `min_ink_delta` is `u32::MAX` so that if this value ever did reach a judge by
/// mistake, every ink comparison would refuse rather than pass.
fn recon_profile() -> LandingProfile {
    LandingProfile {
        provider: NativeAppId::Telegram,
        provider_name: "Telegram (recon, not a judging profile)",
        process_name: TELEGRAM_DESKTOP_PROCESS_NAME,
        write_channel: WriteChannel::ValueSet,
        judges: &[JudgeChannel::RenderedDocumentUia],
        document_channel: JudgeChannel::RenderedDocumentUia,
        empty_document_chars: &[],
        leaf_join: "",
        matcher: TELEGRAM_COMPOSER_MATCHER,
        // Qt publishes its UIA tree eagerly; A-00 read 743 elements on a cold
        // probe and `TELEGRAM_UIA2_WINDOW_PLAN` is `Uia2WakePolicy::None`.
        wake: false,
        settle_ms: 400,
        walk: WalkCaps {
            max_nodes: 256,
            max_depth: 8,
        },
        judge_timeout_ms: 5_000,
        commit_key: "Enter",
        min_ink_delta: u32::MAX,
        normalisations: &[],
    }
}

pub(crate) fn bind_telegram() -> Option<(Uia2Acquired, Vec<Uia2Editable>)> {
    let host = crate::native_a11y::win32::Uia2Win32Host::desktop();
    let acquired = match acquire_uia2_window(TELEGRAM_UIA2_WINDOW_PLAN, &host) {
        Ok(acquired) => acquired,
        Err(error) => {
            eprintln!("tg-oracle: acquire refused: {error:?}");
            return None;
        }
    };
    eprintln!(
        "tg-oracle: bound hwnd={:#x} outer={:#x} pid={} route={:?} elements={} woke={} settled_ms={}",
        acquired.window.bound_hwnd,
        acquired.window.app_outer_hwnd,
        acquired.window.bound_process_id,
        acquired.window.tree_route,
        acquired.elements,
        acquired.woke,
        acquired.settled_ms
    );
    let editables = match acquire_uia2_editables(&host, acquired) {
        Ok(editables) => editables,
        Err(error) => {
            eprintln!("tg-oracle: editable scan timed out: {error:?}");
            return None;
        }
    };
    eprintln!(
        "tg-oracle: editable={} writable={}",
        editables.len(),
        editables.iter().filter(|e| e.writable()).count()
    );
    for element in &editables {
        eprintln!(
            "  edit name={:?} value_pattern={} enabled={} kbd={} read_only={}",
            element.name,
            element.value_pattern,
            element.enabled,
            element.keyboard_focusable,
            element.read_only
        );
    }
    Some((acquired, editables))
}

/// The chat-list search field, resolved **by the shipping matcher's own
/// rejecting stems** rather than by a second guess: `TELEGRAM_COMPOSER_MATCHER`
/// refuses this element as a composer, and this asks for exactly the element it
/// refuses. Probe housekeeping — nothing in production resolves it.
///
/// **Telegram publishes more than one writable `Search`** (measured: two, plus
/// two non-focusable ones), so unlike WhatsApp this cannot be resolved by
/// uniqueness. The tie is broken by geometry — the chat-list search sits in the
/// left column and the composer's own rectangle starts at x=792 — and every
/// candidate is printed with its rectangle so the choice is auditable rather
/// than implicit. A candidate that publishes no rectangle is discarded: the ink
/// channel could not measure it either.
pub(crate) fn resolve_search_box(
    acquired: &Uia2Acquired,
    editables: &[Uia2Editable],
) -> Option<Uia2Editable> {
    let mut candidates: Vec<(i32, Uia2Editable)> = Vec::new();
    for element in editables.iter().filter(|element| element.writable()) {
        let name = element.name.to_lowercase();
        if !TELEGRAM_COMPOSER_MATCHER
            .non_composer_stems
            .iter()
            .any(|stem| name.contains(stem))
        {
            continue;
        }
        let bound = bound_of(acquired, element);
        let rect = ink_of(&bound).map(|ink| ink.rect);
        eprintln!(
            "tg-oracle: search candidate name={:?} rect={rect:?}",
            element.name
        );
        if let Some(rect) = rect {
            candidates.push((rect.left, element.clone()));
        }
    }
    if candidates.is_empty() {
        eprintln!(
            "tg-oracle: no writable element matches a rejecting stem; the canary has no target"
        );
        return None;
    }
    candidates.sort_by_key(|(left, _)| *left);
    let (left, chosen) = candidates.remove(0);
    eprintln!(
        "tg-oracle: canary target chosen by leftmost rectangle: name={:?} left={left}",
        chosen.name
    );
    Some(chosen)
}

pub(crate) fn bound_of(acquired: &Uia2Acquired, element: &Uia2Editable) -> BoundComposer {
    BoundComposer {
        hwnd: acquired.window.bound_hwnd,
        route: acquired.window.tree_route,
        process_id: acquired.window.bound_process_id,
        composer: element.clone(),
    }
}

/// Bring Telegram to the foreground **without injecting anything**, then prove
/// the element's own rectangle is what a person would see there.
///
/// The ink channel reads the desktop device context, so an occluded composer
/// would be measured as whatever is on top of it. `SetForegroundWindow` is a
/// window-manager call, not input: it presses nothing, moves nothing, resizes
/// nothing and cannot commit. **No window is repositioned by this file** — the
/// owner's Telegram stays exactly where he put it, on the display he put it on.
#[allow(unsafe_code)]
pub(crate) fn composer_rect_is_on_screen(acquired: &Uia2Acquired, rect: super::Rect) -> bool {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetAncestor, GetForegroundWindow, SetForegroundWindow, WindowFromPoint, GA_ROOT,
    };

    let outer = acquired.window.app_outer_hwnd;
    unsafe { SetForegroundWindow(outer as _) };
    std::thread::sleep(std::time::Duration::from_millis(400));
    let foreground_root = unsafe { GetAncestor(GetForegroundWindow(), GA_ROOT) } as isize;

    let point = POINT {
        x: rect.left + rect.width() / 2,
        y: rect.top + rect.height() / 2,
    };
    let at_point = unsafe { WindowFromPoint(point) } as isize;
    let at_point_root = unsafe { GetAncestor(at_point as _, GA_ROOT) } as isize;
    let visible = at_point_root == outer;
    eprintln!(
        "tg-oracle: on-screen test: outer={outer:#x} foreground_root={foreground_root:#x} \
         point=({},{}) window_at_point={at_point:#x} its_root={at_point_root:#x} visible={visible}",
        point.x, point.y
    );
    visible
}

/// Every channel's answer for one element, printed. **No verdict is formed
/// here** — this is the raw material a profile is later pinned from.
pub(crate) fn read_every_channel(label: &str, bound: &BoundComposer) {
    let judge = LandingJudgeWin32;
    let profile = recon_profile();
    let deadline = JudgeDeadline::from_profile(&profile);

    let identity = judge.window_identity(bound.hwnd, deadline);
    eprintln!("tg-oracle: {label}: window identity = {identity:?}");
    eprintln!(
        "tg-oracle: {label}: element accessible name = {:?}",
        bound.composer.name
    );
    match judge.rendered_document_uia(bound, profile.walk, profile.leaf_join, deadline) {
        Ok(Some(document)) => eprintln!(
            "tg-oracle: {label}: J1 UIA leaves={:?} text={:?} nodes={} depth={}",
            document.leaves, document.text, document.nodes_visited, document.depth_reached
        ),
        other => eprintln!("tg-oracle: {label}: J1 UIA = {other:?}"),
    }
    match judge.rendered_document_text_pattern(bound, profile.walk, deadline) {
        Ok(Some(document)) => eprintln!(
            "tg-oracle: {label}: J2 TextPattern text={:?}",
            document.text
        ),
        other => eprintln!("tg-oracle: {label}: J2 TextPattern = {other:?}"),
    }
    match judge.rendered_document_msaa(bound, profile.walk, profile.leaf_join, deadline) {
        Ok(Some(document)) => eprintln!(
            "tg-oracle: {label}: J2b MSAA leaves={:?} text={:?}",
            document.leaves, document.text
        ),
        other => eprintln!("tg-oracle: {label}: J2b MSAA = {other:?}"),
    }
    match judge.composer_ink(bound, deadline) {
        Ok(Some(ink)) => eprintln!(
            "tg-oracle: {label}: J3 ink rect={:?} sampled={} inked={}",
            ink.rect, ink.sampled, ink.inked
        ),
        other => eprintln!("tg-oracle: {label}: J3 ink = {other:?}"),
    }
    match judge.disowned_value_property(bound, deadline) {
        Ok(value) => eprintln!("tg-oracle: {label}: D  disowned value property = {value:?}"),
        Err(timeout) => eprintln!("tg-oracle: {label}: D  disowned value property = {timeout:?}"),
    }
}

fn ink_of(bound: &BoundComposer) -> Option<Ink> {
    let profile = recon_profile();
    LandingJudgeWin32
        .composer_ink(bound, JudgeDeadline::from_profile(&profile))
        .ok()
        .flatten()
}

fn document_of(bound: &BoundComposer) -> Option<String> {
    let profile = recon_profile();
    LandingJudgeWin32
        .rendered_document_uia(
            bound,
            profile.walk,
            profile.leaf_join,
            JudgeDeadline::from_profile(&profile),
        )
        .ok()
        .flatten()
        .map(|document| document.text)
}

fn leaves_of(bound: &BoundComposer) -> Option<Vec<String>> {
    let profile = recon_profile();
    LandingJudgeWin32
        .rendered_document_uia(
            bound,
            profile.walk,
            profile.leaf_join,
            JudgeDeadline::from_profile(&profile),
        )
        .ok()
        .flatten()
        .map(|document| document.leaves)
}

/// Read whatever channel the given profile declares as its **primary document
/// channel**, rather than a channel this file picked.
///
/// The first run of the ladder asserted on `document_of` — the leaf walk —
/// which is exactly the channel the recon step had just proven blind on Qt. It
/// refused a canary that had in fact landed. A probe that hard-codes a channel
/// reproduces, in the probe, the defect the oracle exists to prevent.
fn primary_document_of(profile: &LandingProfile, bound: &BoundComposer) -> Option<String> {
    match profile.document_channel {
        JudgeChannel::RenderedDocumentUia => document_of(bound),
        JudgeChannel::RenderedDocumentTextPattern => text_pattern_of(bound),
        other => panic!("tg-oracle: no reader is wired for a primary channel of {other:?}"),
    }
}

fn text_pattern_of(bound: &BoundComposer) -> Option<String> {
    let profile = recon_profile();
    LandingJudgeWin32
        .rendered_document_text_pattern(bound, profile.walk, JudgeDeadline::from_profile(&profile))
        .ok()
        .flatten()
        .map(|document| document.text)
}

/// **Step 1 — read-only reconnaissance.** Writes nothing, presses nothing,
/// moves nothing.
///
/// Run this before anything else on any host. It reports what every channel
/// answers for every writable element Telegram publishes, with a bounded poll so
/// no finding rests on a single read (D-226), and it takes the empty-composer
/// ink baseline a profile's `min_ink_delta` is later checked against.
///
/// ```text
/// osl_privacy_hub-<hash>.exe --ignored --test-threads=1 --nocapture \
///   landing_oracle::live_telegram::report_what_the_landing_oracle_can_see_on_telegram
/// ```
#[test]
#[ignore = "reads a live Telegram on a Windows host; run explicitly"]
fn report_what_the_landing_oracle_can_see_on_telegram() {
    let Some((acquired, editables)) = bind_telegram() else {
        panic!("tg-oracle: nothing to read");
    };
    let composer = resolve_uia2_composer(TELEGRAM_COMPOSER_MATCHER, &editables)
        .unwrap_or_else(|error| panic!("tg-oracle: no composer resolved: {error:?}"));
    eprintln!("tg-oracle: composer name={:?}", composer.name);
    let bound = bound_of(&acquired, &composer);

    // D-226: a single reading is not a measurement. Six reads over three
    // seconds, and the run reports whether the answer ever moved.
    let mut documents = Vec::new();
    for index in 0..6 {
        let document = document_of(&bound);
        eprintln!("tg-oracle: warm poll {index}: J1 document = {document:?}");
        documents.push(document);
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    let stable = documents.windows(2).all(|pair| pair[0] == pair[1]);
    eprintln!("tg-oracle: J1 stable across 6 reads over 3 s = {stable}");

    read_every_channel("composer", &bound);

    // **The Qt question.** If J1's text is exactly the composer's accessible
    // name, the leaf channel is publishing the element's label rather than its
    // content, and it cannot see a document at all on this surface. Reported as
    // a suspicion here and settled by the search-box canary, which is the first
    // point at which a known string exists to look for.
    let j1 = document_of(&bound);
    let j1_is_the_name = j1.as_deref() == Some(composer.name.as_str());
    eprintln!(
        "tg-oracle: SUSPICION J1_text_equals_element_name={j1_is_the_name} \
         (J1={j1:?} name={:?}) -- if true, the leaf channel is reading a label, not a document",
        composer.name
    );
    eprintln!("tg-oracle: J1 leaves = {:?}", leaves_of(&bound));

    // Visibility, and then the ink baseline a profile's `min_ink_delta` is
    // measured against. Reported whether or not the rectangle is on screen, and
    // labelled either way.
    if let Some(ink) = ink_of(&bound) {
        let on_screen = composer_rect_is_on_screen(&acquired, ink.rect);
        let after = ink_of(&bound);
        eprintln!(
            "tg-oracle: EMPTY-COMPOSER INK BASELINE on_screen={on_screen} rect={:?} sampled={} \
             before_foreground={} after_foreground={:?}",
            ink.rect,
            ink.sampled,
            ink.inked,
            after.map(|ink| ink.inked)
        );
    }

    if let Some(search) = resolve_search_box(&acquired, &editables) {
        eprintln!("tg-oracle: search box name={:?}", search.name);
        read_every_channel("search box", &bound_of(&acquired, &search));
    }

    // Exit state, so the tasklog can show the host was returned as found.
    let host = crate::native_a11y::win32::Uia2Win32Host::desktop();
    for element in editables.iter().filter(|element| element.writable()) {
        let value = read_uia2_composer_value(&host, acquired, element);
        eprintln!(
            "tg-oracle: EXIT-STATE name={:?} value={value:?}",
            element.name
        );
    }
    eprintln!("tg-oracle: read-only probe: nothing was written, nothing was pressed");
}

/// **Step 2 — the clear canary, on the chat-list search box.** The only element
/// this surface offers that is writable and is not a conversation.
///
/// It answers the question that gates every later stage: **does `SetValue("")`
/// actually empty a Telegram field, or does it merely return success?** That is
/// D-228's shape, and D-228 is why this runs on the search box before it runs on
/// a real person's composer.
///
/// It is also the first point at which a **known string** exists on this
/// surface, so it settles the Qt question the recon step could only suspect: if
/// the sentinel appears in J1, the leaf channel reads content; if J1 does not
/// move while J2 and the ink do, the leaf channel reads a label.
///
/// It also measures ink-per-character on Telegram's own UI font at this host's
/// DPI, **on an element the oracle never judges**, which is what keeps a later
/// `min_ink_delta` from being fitted to the landing it gates.
///
/// The search box filters the chat list while the sentinel is in it and is
/// restored by `SetValue("")` immediately afterwards. Nothing is opened, nothing
/// is marked read, and no conversation is touched.
///
/// ```text
/// osl_privacy_hub-<hash>.exe --ignored --test-threads=1 --nocapture \
///   landing_oracle::live_telegram::prove_the_clear_path_before_any_composer_placement
/// ```
#[test]
#[ignore = "writes into a live Telegram search box on a Windows host; run explicitly"]
fn prove_the_clear_path_before_any_composer_placement() {
    use crate::native_a11y::Uia2Syscalls as _;

    let sentinel = std::env::var("OSL_TG_CANARY")
        .unwrap_or_else(|_| "osl clear canary seven three one".to_owned());
    assert!(
        !crate::native_a11y::uia2_carrier_carries_submit(&sentinel),
        "a sentinel carrying a line break must never reach a live Telegram field"
    );

    let Some((acquired, editables)) = bind_telegram() else {
        panic!("tg-oracle: nothing to probe");
    };
    let Some(search) = resolve_search_box(&acquired, &editables) else {
        panic!("tg-oracle: the search box did not resolve; the canary has no target");
    };
    eprintln!("tg-oracle: canary target name={:?}", search.name);
    let bound = bound_of(&acquired, &search);
    let host = crate::native_a11y::win32::Uia2Win32Host::desktop();

    let first = ink_of(&bound);
    let on_screen = first
        .map(|ink| composer_rect_is_on_screen(&acquired, ink.rect))
        .unwrap_or(false);
    let ink_empty = ink_of(&bound);
    eprintln!(
        "tg-oracle: canary: before: on_screen={on_screen} ink={:?} J1={:?} J2={:?} value={:?}",
        ink_empty.map(|ink| ink.inked),
        document_of(&bound),
        text_pattern_of(&bound),
        read_uia2_composer_value(&host, acquired, &search),
    );

    // --- the write -----------------------------------------------------------
    let placed = place_uia2_carrier(&host, acquired, &search, &sentinel, false);
    eprintln!("tg-oracle: canary: place_uia2_carrier -> {placed:?}");
    std::thread::sleep(std::time::Duration::from_millis(600));

    let ink_written = ink_of(&bound);
    let j1_written = document_of(&bound);
    let j2_written = text_pattern_of(&bound);
    let value_written = read_uia2_composer_value(&host, acquired, &search);
    eprintln!(
        "tg-oracle: canary: after write: ink={:?} J1={j1_written:?} J2={j2_written:?} \
         value={value_written:?}",
        ink_written.map(|ink| ink.inked)
    );

    // --- the clear, which is the whole point ---------------------------------
    let cleared = clear_uia2_composer(&host, acquired, &search);
    eprintln!("tg-oracle: canary: clear_uia2_composer -> {cleared:?}");
    std::thread::sleep(std::time::Duration::from_millis(600));

    let ink_cleared = ink_of(&bound);
    let j1_cleared = document_of(&bound);
    let j2_cleared = text_pattern_of(&bound);
    let value_cleared = read_uia2_composer_value(&host, acquired, &search);
    eprintln!(
        "tg-oracle: canary: after clear: ink={:?} J1={j1_cleared:?} J2={j2_cleared:?} \
         value={value_cleared:?}",
        ink_cleared.map(|ink| ink.inked)
    );

    // --- what this measured --------------------------------------------------
    //
    // Which document channel moved with the write is the finding. The value
    // property is read and printed and is a judge of nothing.
    let j1_holds = j1_written
        .as_deref()
        .is_some_and(|text| text.contains(&sentinel));
    let j2_holds = j2_written
        .as_deref()
        .is_some_and(|text| text.contains(&sentinel));
    let j1_cleared_it = !j1_cleared
        .as_deref()
        .is_some_and(|text| text.contains(&sentinel));
    let j2_cleared_it = !j2_cleared
        .as_deref()
        .is_some_and(|text| text.contains(&sentinel));
    eprintln!(
        "tg-oracle: canary VERDICT j1_holds_sentinel={j1_holds} j2_holds_sentinel={j2_holds} \
         j1_cleared_it={j1_cleared_it} j2_cleared_it={j2_cleared_it}"
    );

    if let (Some(empty), Some(written), Some(back)) = (ink_empty, ink_written, ink_cleared) {
        let delta = written.inked.saturating_sub(empty.inked);
        let residue = back.inked.saturating_sub(empty.inked);
        eprintln!(
            "tg-oracle: INK empty={} written={} cleared={} delta={delta} residue={residue} \
             chars={} px_per_char={:.2}",
            empty.inked,
            written.inked,
            back.inked,
            sentinel.chars().count(),
            f64::from(delta) / sentinel.chars().count() as f64
        );
    }

    // The clear is the claim under test, and it can only be trusted if the
    // write was seen first. Asserting the write BEFORE the clear is what stops
    // "a clear that removed nothing because nothing was there" passing as a
    // working clear.
    assert!(
        j1_holds || j2_holds,
        "no document channel ever held the sentinel, so this run proves nothing about the clear"
    );
    assert!(
        j1_cleared_it && j2_cleared_it,
        "SetValue(\"\") did not remove the sentinel from Telegram's document. This is D-228's \
         shape on Telegram and the lane stops here: nothing is placed into a real conversation \
         when the clear cannot be trusted."
    );
    assert_eq!(
        host.submit_shaped_calls(),
        0,
        "nothing may be committed, ever"
    );
    placed.expect("the sentinel places into the search box");
    cleared.expect("the search box is always cleared");
}

/// **Step 3 — the carry, judged by the landing oracle.**
///
/// Encode a real payload into real OSL cover text, place that cover text into
/// the owner's live Telegram composer through the shipping write, judge the
/// landing through channels that did **not** write it, decode the payload back
/// out of the **oracle's document** rather than out of the value property, clear
/// the composer, prove it empty, and only then write the receipt.
///
/// The ladder refuses at the first stage that cannot be proven, and stage 4
/// costs **two characters** so that a clear that fails leaves a real chat as
/// close to untouched as a live proof can be.
///
/// **Nothing is sent.** There is no key, click or `Invoke` in this file, and
/// `place_uia2_carrier` refuses a carrier carrying a line break, which on
/// Telegram is the send.
///
/// ```text
/// osl_privacy_hub-<hash>.exe --ignored --test-threads=1 --nocapture \
///   landing_oracle::live_telegram::carry_a_real_carrier_through_live_telegram
/// ```
#[test]
#[ignore = "places a carrier in a live Telegram conversation composer; run explicitly"]
fn carry_a_real_carrier_through_live_telegram() {
    use super::{judge_empty_composer, judge_landing, LandingBaseline, ProfileLookup};
    use crate::native_a11y::Uia2Syscalls as _;
    use stego::{decode_mode1, encode_mode1, ConversationCipher};

    // 0 — the profile. An unmeasured provider is refused BY NAME here, before
    // anything is bound, and no neighbour's numbers are borrowed.
    let profile = match super::landing_profile(NativeAppId::Telegram) {
        ProfileLookup::Measured(profile) => *profile,
        ProfileLookup::Unmeasured { provider, missing } => {
            panic!("tg-oracle: ProviderNotMeasured {provider:?}: {missing}")
        }
    };
    eprintln!(
        "tg-oracle: profile provider={} process_name={} write={} judges={:?} settle_ms={} \
         walk={:?} empty_chars={:?} min_ink_delta={} commit_key={} normalisations={:?}",
        profile.provider_name,
        profile.process_name,
        profile.write_channel.name(),
        profile.judges,
        profile.settle_ms,
        profile.walk,
        profile.empty_document_chars,
        profile.min_ink_delta,
        profile.commit_key,
        profile.normalisations,
    );

    // The carrier is real OSL cover text over a real payload, so the run can be
    // judged by a decoder rather than by a string comparison OSL performs
    // against its own input.
    let cipher = ConversationCipher::from_salt(b"osl/telegram-carry-receipt/v1");
    let secret: &[u8] = b"telegram carries osl";
    let cover = encode_mode1(&cipher, secret).expect("mode 1 encodes the payload");
    assert!(
        !crate::native_a11y::uia2_carrier_carries_submit(&cover),
        "cover text carrying a line break must never reach a live composer -- on Telegram the \
         newline is the send"
    );
    assert!(
        cover.len() <= crate::native_telegram_adapter::TELEGRAM_LIVE_CARRIER_MAX_BYTES,
        "the shipping path refuses a carrier over its own bound, so this probe must not exceed it"
    );

    let Some((acquired, editables)) = bind_telegram() else {
        panic!("tg-oracle: nothing to carry into");
    };
    let composer = resolve_uia2_composer(TELEGRAM_COMPOSER_MATCHER, &editables)
        .unwrap_or_else(|error| panic!("tg-oracle: no composer resolved: {error:?}"));
    eprintln!("tg-oracle: composer name={:?}", composer.name);
    let bound = bound_of(&acquired, &composer);
    let host = crate::native_a11y::win32::Uia2Win32Host::desktop();
    let judge = LandingJudgeWin32;
    let elements = acquired.elements;
    let settle = || std::thread::sleep(std::time::Duration::from_millis(profile.settle_ms));

    // 1 — visibility, then the ink baseline while the composer is provably
    // empty. An occluded rectangle would be measured as whatever is on top of
    // it, so the baseline is refused rather than taken blind.
    let first = ink_of(&bound).expect("the composer publishes a bounding rectangle");
    assert!(
        composer_rect_is_on_screen(&acquired, first.rect),
        "Telegram's composer is not the window on screen at its own rectangle; the ink channel \
         would be measuring something else and no proof is available"
    );
    let empty_ink = ink_of(&bound).expect("the composer publishes ink once it is on screen");
    let baseline = LandingBaseline { empty_ink };
    eprintln!(
        "tg-oracle: EMPTY BASELINE rect={:?} sampled={} inked={}",
        empty_ink.rect, empty_ink.sampled, empty_ink.inked
    );

    // 2 — the composer must be provably empty at entry. If the owner has a
    // draft in it, this lane does not touch it.
    let empty_at_entry = judge_empty_composer(&judge, &profile, &bound, baseline);
    eprintln!("tg-oracle: stage 2 (empty at entry): {empty_at_entry:?}");
    assert!(
        empty_at_entry.is_ok(),
        "the composer is not provably empty at entry -- it may hold the owner's own draft, and \
         this lane will not overwrite one"
    );

    // 3 — the refusal that proves the oracle is not simply agreeing: ask
    // whether the carrier is there before anything is placed.
    let nothing_yet = judge_landing(&judge, &profile, &bound, &cover, baseline, &[]);
    eprintln!(
        "tg-oracle: stage 3 (nothing placed): {}",
        match &nothing_yet {
            Ok(proof) => format!("LANDED (WRONG) {proof:?}"),
            Err(refusal) => format!("REFUSED {} -- {refusal:?}", refusal.name()),
        }
    );
    assert!(
        nothing_yet.is_err(),
        "the oracle claimed a carrier had landed before anything was placed"
    );

    // 4 — THE CANARY IN THE COMPOSER ITSELF. Two characters, so that if the
    // clear fails the residue in a real chat is as small as it can be. Nothing
    // larger is placed until this has been written AND removed.
    const CANARY: &str = "ok";
    let canary_placed = place_uia2_carrier(&host, acquired, &composer, CANARY, false);
    settle();
    // Read through the channel THE PROFILE declares, never one this file chose.
    let canary_document = primary_document_of(&profile, &bound);
    let canary_leaf_walk = document_of(&bound);
    let canary_verdict = judge_landing(&judge, &profile, &bound, CANARY, baseline, &[]);
    let canary_cleared = clear_uia2_composer(&host, acquired, &composer);
    settle();
    let canary_empty = judge_empty_composer(&judge, &profile, &bound, baseline);
    eprintln!(
        "tg-oracle: stage 4 (canary): place={canary_placed:?} primary={canary_document:?} \
         leaf_walk={canary_leaf_walk:?} verdict={} clear={canary_cleared:?} empty_after={}",
        match &canary_verdict {
            Ok(proof) => format!(
                "LANDED document={:?} ink Δ{}",
                proof.document, proof.ink_delta
            ),
            Err(refusal) => format!("REFUSED {} -- {refusal:?}", refusal.name()),
        },
        match &canary_empty {
            Ok(proof) => format!(
                "PROVEN EMPTY document={:?} ink {}→{}",
                proof.document, proof.ink_before.inked, proof.ink_after.inked
            ),
            Err(refusal) => format!("REFUSED {} -- {refusal:?}", refusal.name()),
        }
    );
    let canary_reached_the_document = canary_document
        .as_deref()
        .is_some_and(|document| document.contains(CANARY));
    assert!(
        canary_empty.is_ok(),
        "the two-character canary could not be cleared from the composer. The carrier was NOT \
         placed. This is D-228's shape on Telegram and the lane stops here."
    );
    assert!(
        canary_reached_the_document,
        "the shipping write did not reach Telegram's rendered document -- read through the \
         channel the profile declares as its document, not through one this probe chose -- so \
         the clear above proved nothing and a carrier could not land either. D-205's shape, \
         measured on Telegram."
    );

    // 5 — THE CARRIER. Read before clearing, clear before asserting: the
    // conversation is restored whatever the verdict turns out to be.
    let placement = place_uia2_carrier(&host, acquired, &composer, &cover, false);
    settle();
    let document = primary_document_of(&profile, &bound);
    let verdict = judge_landing(&judge, &profile, &bound, &cover, baseline, &[]);
    let disowned = read_uia2_composer_value(&host, acquired, &composer);
    let cleared = clear_uia2_composer(&host, acquired, &composer);
    settle();
    let empty_after_clear = judge_empty_composer(&judge, &profile, &bound, baseline);
    let after_all = judge_landing(&judge, &profile, &bound, &cover, baseline, &[]);

    eprintln!("tg-oracle: stage 5: place_uia2_carrier -> {placement:?}");
    eprintln!("tg-oracle: stage 5: rendered document = {document:?}");
    eprintln!(
        "tg-oracle: stage 5: VERDICT {}",
        match &verdict {
            Ok(proof) => format!(
                "LANDED document={:?} leaves={:?} nodes={} corroborated={:?} ink {}→{} (Δ{}) \
                 disowned_value={:?} disowned_disagrees={} submit_shaped={} commit_key_not_sent={}",
                proof.document,
                proof.leaves,
                proof.nodes_visited,
                proof.corroborating_document,
                proof.ink_before.inked,
                proof.ink_after.inked,
                proof.ink_delta,
                proof.disowned_value_property,
                proof.disowned_value_property_disagrees,
                proof.submit_shaped_calls,
                proof.commit_key_not_sent,
            ),
            Err(refusal) => format!("REFUSED {} -- {refusal:?}", refusal.name()),
        }
    );
    eprintln!("tg-oracle: stage 5: disowned value property (counted by nothing) = {disowned:?}");
    eprintln!("tg-oracle: stage 6: clear_uia2_composer -> {cleared:?}");
    eprintln!(
        "tg-oracle: stage 6: EMPTY AFTER CLEAR {}",
        match &empty_after_clear {
            Ok(proof) => format!(
                "PROVEN document={:?} corroborated={:?} ink {}→{} (Δ{}) disowned={:?}",
                proof.document,
                proof.corroborating_document,
                proof.ink_before.inked,
                proof.ink_after.inked,
                proof.ink_delta,
                proof.disowned_value_property,
            ),
            Err(refusal) => format!("REFUSED {} -- {refusal:?}", refusal.name()),
        }
    );
    eprintln!(
        "tg-oracle: stage 6: asking whether the CARRIER is still there -> {}",
        match &after_all {
            Ok(proof) => format!("STILL LANDED (WRONG) {:?}", proof.document),
            Err(refusal) => format!("REFUSED {} -- {refusal:?}", refusal.name()),
        }
    );
    eprintln!("tg-oracle: no Enter was sent: there is no key, click or Invoke in this file");

    // 6 — the assertions, now that the chat is back the way it was found.
    let composer_empty_after_clear = empty_after_clear.is_ok();
    assert!(
        composer_empty_after_clear,
        "the composer still held text after the clear -- a real chat was left dirty"
    );
    assert_eq!(
        host.submit_shaped_calls(),
        0,
        "nothing may be committed, ever"
    );
    placement.expect("the carrier places into the live composer");
    cleared.expect("the composer is always cleared");
    let proof = verdict.unwrap_or_else(|refusal| {
        panic!(
            "tg-oracle: the carrier did not land: {} -- {refusal:?}",
            refusal.name()
        )
    });

    // 7 — the carry, judged by a decoder reading the ORACLE's document rather
    // than the value property this write moved.
    let returned = proof.document.clone();
    let recovered = decode_mode1(&cipher, returned.trim())
        .expect("the string Telegram's rendered document hands back still decodes");
    assert_eq!(
        recovered.as_slice(),
        secret,
        "the payload recovered from Telegram's own rendered document must be the payload sent"
    );
    // Negative stem: drop a word and the recovery must fail, or this would pass
    // on a decoder that ignored its input.
    let mut words: Vec<&str> = returned.trim().split_whitespace().collect();
    assert!(words.len() > 3, "cover text is a word sequence");
    words.remove(words.len() / 2);
    let starved = words.join(" ");
    assert!(
        !decode_mode1(&cipher, &starved).is_ok_and(|bytes| bytes == secret),
        "removing a word from the rendered document must not still recover the payload"
    );
    let byte_exact = returned == cover;

    // 8 — the receipt, and only now.
    use crate::native_apps::tests::carry_receipt as receipt_io;
    let contract = receipt_io::seam_contract(NativeAppId::Telegram)
        .expect("the Telegram seam contract computes against the tree being proven");
    let receipt = receipt_io::LiveCarryReceipt {
        schema: receipt_io::RECEIPT_SCHEMA.to_owned(),
        provider: "telegram".to_owned(),
        seam: "uia2_substrate".to_owned(),
        adapter_source: "src/native_telegram_adapter.rs".to_owned(),
        adapter_source_sha256: receipt_io::source_sha256("src/native_telegram_adapter.rs"),
        seam_contract_sha256: contract.sha256.clone(),
        seam_contract_items: contract.items.len(),
        substrate_source_sha256: receipt_io::source_sha256(receipt_io::SUBSTRATE_SOURCE),
        client_process: TELEGRAM_DESKTOP_PROCESS_NAME.to_owned(),
        element_count: elements,
        carrier_bytes: cover.len(),
        readback_bytes: returned.len(),
        byte_exact,
        payload_sha256: receipt_io::sha256_hex(secret),
        readback_sha256: receipt_io::sha256_hex(returned.as_bytes()),
        recovered_sha256: receipt_io::sha256_hex(&recovered),
        enter_sent: false,
        composer_empty_after_clear,
        recorded_utc: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| format!("unix:{}", since.as_secs()))
            .unwrap_or_else(|_| "unix:0".to_owned()),
    };
    eprintln!("tg-oracle: RECEIPT JSON\n{}", receipt.to_json());
    assert!(
        byte_exact,
        "the rendered document is not byte-exactly the carrier, so no receipt is written"
    );
    receipt.write(NativeAppId::Telegram);
    eprintln!(
        "tg-oracle: receipt written to {}",
        receipt_io::receipt_path(NativeAppId::Telegram).display()
    );
    eprintln!(
        "tg-oracle: seam contract {} over {} declarations",
        contract.sha256,
        contract.items.len()
    );
}
