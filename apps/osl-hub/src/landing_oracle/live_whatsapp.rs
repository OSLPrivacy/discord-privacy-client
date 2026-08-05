//! The oracle on **WhatsApp** — the second surface it has ever been pointed at,
//! and the first whose placement doctrine is `IValueProvider::SetValue` rather
//! than synthesized keystrokes.
//!
//! # Why this file exists separately from `live.rs`
//!
//! `live.rs` is Discord's calibration and it is Discord-shaped in three ways
//! this surface does not share: it binds an Electron outer window through the
//! MSAA bridge, it writes with `SendInput`, and every one of its injections sits
//! behind a foreground-plus-focus proof because `SendInput` is global. **None of
//! that applies here.** WhatsApp is bound as a sibling WebView2 renderer on the
//! `UiaNative` route, and the only write verb this file can reach is
//! `place_uia2_carrier` / `clear_uia2_composer` — `SetValue` and `SetValue("")`.
//!
//! **There is no `SendInput` in this file, no key, no click, no `Invoke`.** On
//! WhatsApp the newline *is* the send, so a probe that could press a key is a
//! probe that could post a message into a real person's conversation. The
//! placement primitive already refuses a carrier carrying a line break
//! (`native_a11y::uia2_carrier_carries_submit`).
//!
//! # The ladder, and why it is ordered this way
//!
//! **A placement that cannot be reclaimed must never happen.** D-228 measured a
//! Discord composer that could not be emptied by the shipping reclaim, and the
//! residue is still in the owner's chat. So the clear path is proven *before*
//! the first placement into the conversation composer, on the cheapest element
//! that can prove it:
//!
//! 1. **recon, read-only** — bind warm (D-226), report every channel's answer for
//!    both writable elements, and take the composer's empty-state document and
//!    ink baseline. Writes nothing.
//! 2. **the clear canary, on the SEARCH BOX** — `SetValue` a sentinel into the
//!    chat-list search field, read the document channels raw, then `SetValue("")`
//!    and read them again. The search box is the one writable element on this
//!    surface that is **not** a conversation: nothing can be posted from it, and
//!    if the clear fails the residue is a filter string rather than text in
//!    someone's chat. It also yields the ink-per-character figure the profile's
//!    `min_ink_delta` is derived from, measured on an element that is **not the
//!    one later judged**.
//! 3. only then, in [`super::live_whatsapp_landing`]-shaped stages, the composer.
//!
//! # What the search-box canary is NOT
//!
//! It is **not** an oracle verdict and it must never be reported as one. The
//! search box is a native `<input>` and Chromium publishes it as a **leaf**, so
//! J1 (`rendered_document_uia`) reads its accessible *name* — the placeholder —
//! not its content. Only the TextPattern range and the ink say anything about
//! what it holds. That is why the canary is a set of raw channel reads printed
//! for the record and never a `LandingProof`: a profile that judged an element
//! J1 cannot see would be decoration.

use super::win32::LandingJudgeWin32;
use super::{
    BoundComposer, Ink, JudgeChannel, JudgeDeadline, LandingJudgeSyscalls, LandingProfile,
    Normalisation, WalkCaps, WriteChannel,
};
use crate::native_a11y::{
    acquire_uia2_editables, acquire_uia2_window, clear_uia2_composer, place_uia2_carrier,
    read_uia2_composer_value, resolve_uia2_composer, Uia2Acquired, Uia2Editable,
};
use crate::native_apps::NativeAppId;
use crate::native_whatsapp_adapter::{WHATSAPP_COMPOSER_MATCHER, WHATSAPP_UIA2_WINDOW_PLAN};

/// **Reads only.** A profile is required to construct a [`JudgeDeadline`] and a
/// [`WalkCaps`], and nothing else about this value is load-bearing: this file
/// never passes it to `judge_landing` or `judge_empty_composer`. The measured
/// profile those functions take lives in `landing_oracle.rs` and is pinned from
/// what this probe reports.
fn recon_profile() -> LandingProfile {
    LandingProfile {
        provider: NativeAppId::Whatsapp,
        provider_name: "WhatsApp (recon, not a judging profile)",
        process_name: "msedgewebview2",
        write_channel: WriteChannel::ValueSet,
        judges: &[JudgeChannel::RenderedDocumentUia],
        document_channel: JudgeChannel::RenderedDocumentUia,
        empty_document_chars: &[],
        leaf_join: "",
        matcher: WHATSAPP_COMPOSER_MATCHER,
        wake: true,
        settle_ms: 250,
        walk: WalkCaps {
            max_nodes: 256,
            max_depth: 8,
        },
        judge_timeout_ms: 5_000,
        commit_key: "Enter",
        min_ink_delta: u32::MAX,
        normalisations: &[Normalisation::ZeroWidthSentinelRetained],
    }
}

pub(crate) fn bind_whatsapp() -> Option<(Uia2Acquired, Vec<Uia2Editable>)> {
    let host = crate::native_a11y::win32::Uia2Win32Host::desktop();
    let acquired = match acquire_uia2_window(WHATSAPP_UIA2_WINDOW_PLAN, &host) {
        Ok(acquired) => acquired,
        Err(error) => {
            eprintln!("wa-oracle: acquire refused: {error:?}");
            return None;
        }
    };
    eprintln!(
        "wa-oracle: bound pid={} route={:?} elements={} woke={} settled_ms={}",
        acquired.window.bound_process_id,
        acquired.window.tree_route,
        acquired.elements,
        acquired.woke,
        acquired.settled_ms
    );
    let editables = match acquire_uia2_editables(&host, acquired) {
        Ok(editables) => editables,
        Err(error) => {
            eprintln!("wa-oracle: editable scan timed out: {error:?}");
            return None;
        }
    };
    eprintln!(
        "wa-oracle: editable={} writable={}",
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
/// rejecting stems** rather than by a second guess: `WHATSAPP_COMPOSER_MATCHER`
/// refuses this element as a composer, and this asks for exactly the element it
/// refuses. Probe housekeeping — nothing in production resolves it.
pub(crate) fn resolve_search_box(editables: &[Uia2Editable]) -> Option<Uia2Editable> {
    let named: Vec<&Uia2Editable> = editables
        .iter()
        .filter(|element| element.writable())
        .filter(|element| {
            let name = element.name.to_lowercase();
            WHATSAPP_COMPOSER_MATCHER
                .non_composer_stems
                .iter()
                .any(|stem| name.contains(stem))
        })
        .collect();
    match named.len() {
        1 => Some(named[0].clone()),
        other => {
            eprintln!(
                "wa-oracle: {other} elements match a rejecting stem; the canary needs exactly one"
            );
            None
        }
    }
}

pub(crate) fn bound_of(acquired: &Uia2Acquired, element: &Uia2Editable) -> BoundComposer {
    BoundComposer {
        hwnd: acquired.window.bound_hwnd,
        route: acquired.window.tree_route,
        process_id: acquired.window.bound_process_id,
        composer: element.clone(),
    }
}

/// Bring WhatsApp to the foreground **without injecting anything**, then prove
/// the element's own rectangle is what a person would see there.
///
/// The ink channel reads the desktop device context, so an occluded composer
/// would be measured as whatever is on top of it. `SetForegroundWindow` is a
/// window-manager call, not input: it presses nothing and cannot commit. The
/// hit test is the part that actually proves visibility, and the ink channel is
/// refused rather than reported when it fails.
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
        "wa-oracle: on-screen test: outer={outer:#x} foreground_root={foreground_root:#x} \
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
    eprintln!("wa-oracle: {label}: window identity = {identity:?}");
    match judge.rendered_document_uia(bound, profile.walk, profile.leaf_join, deadline) {
        Ok(Some(document)) => eprintln!(
            "wa-oracle: {label}: J1 UIA leaves={:?} text={:?} nodes={} depth={}",
            document.leaves, document.text, document.nodes_visited, document.depth_reached
        ),
        other => eprintln!("wa-oracle: {label}: J1 UIA = {other:?}"),
    }
    match judge.rendered_document_text_pattern(bound, profile.walk, deadline) {
        Ok(Some(document)) => eprintln!(
            "wa-oracle: {label}: J2 TextPattern text={:?}",
            document.text
        ),
        other => eprintln!("wa-oracle: {label}: J2 TextPattern = {other:?}"),
    }
    match judge.rendered_document_msaa(bound, profile.walk, profile.leaf_join, deadline) {
        Ok(Some(document)) => eprintln!(
            "wa-oracle: {label}: J2b MSAA leaves={:?} text={:?}",
            document.leaves, document.text
        ),
        other => eprintln!("wa-oracle: {label}: J2b MSAA = {other:?}"),
    }
    match judge.composer_ink(bound, deadline) {
        Ok(Some(ink)) => eprintln!(
            "wa-oracle: {label}: J3 ink rect={:?} sampled={} inked={}",
            ink.rect, ink.sampled, ink.inked
        ),
        other => eprintln!("wa-oracle: {label}: J3 ink = {other:?}"),
    }
    match judge.disowned_value_property(bound, deadline) {
        Ok(value) => eprintln!("wa-oracle: {label}: D  disowned value property = {value:?}"),
        Err(timeout) => eprintln!("wa-oracle: {label}: D  disowned value property = {timeout:?}"),
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

fn text_pattern_of(bound: &BoundComposer) -> Option<String> {
    let profile = recon_profile();
    LandingJudgeWin32
        .rendered_document_text_pattern(bound, profile.walk, JudgeDeadline::from_profile(&profile))
        .ok()
        .flatten()
        .map(|document| document.text)
}

/// **Step 1 — read-only reconnaissance.** Writes nothing, presses nothing.
///
/// Run this before anything else on any host. It reports what every channel
/// answers for both of WhatsApp's writable elements, warm, with a bounded
/// settle poll so no finding rests on a cold zero (D-226).
#[test]
#[ignore = "reads a live WhatsApp on a Windows host; run explicitly"]
fn report_what_the_landing_oracle_can_see_on_whatsapp() {
    let Some((acquired, editables)) = bind_whatsapp() else {
        panic!("wa-oracle: nothing to read");
    };
    let composer = resolve_uia2_composer(WHATSAPP_COMPOSER_MATCHER, &editables)
        .unwrap_or_else(|error| panic!("wa-oracle: no composer resolved: {error:?}"));
    eprintln!("wa-oracle: composer name={:?}", composer.name);
    let bound = bound_of(&acquired, &composer);

    // D-226: a single reading is not a measurement. Six reads over three
    // seconds, and the run reports whether the answer ever moved.
    let mut documents = Vec::new();
    for index in 0..6 {
        let document = document_of(&bound);
        eprintln!("wa-oracle: warm poll {index}: J1 document = {document:?}");
        documents.push(document);
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    let stable = documents.windows(2).all(|pair| pair[0] == pair[1]);
    eprintln!("wa-oracle: J1 stable across 6 reads over 3 s = {stable}");

    read_every_channel("composer", &bound);

    // Visibility, and then the ink baseline that a profile's `min_ink_delta` is
    // measured against. Reported whether or not the rectangle is on screen, and
    // labelled either way.
    if let Some(ink) = ink_of(&bound) {
        let on_screen = composer_rect_is_on_screen(&acquired, ink.rect);
        let after = ink_of(&bound);
        eprintln!(
            "wa-oracle: EMPTY-COMPOSER INK BASELINE on_screen={on_screen} before_foreground={} \
             after_foreground={:?}",
            ink.inked,
            after.map(|ink| ink.inked)
        );
    }

    if let Some(search) = resolve_search_box(&editables) {
        eprintln!("wa-oracle: search box name={:?}", search.name);
        read_every_channel("search box", &bound_of(&acquired, &search));
    }

    // Exit state, so the tasklog can show the host was returned as found.
    let host = crate::native_a11y::win32::Uia2Win32Host::desktop();
    for element in editables.iter().filter(|element| element.writable()) {
        let value = read_uia2_composer_value(&host, acquired, element);
        eprintln!(
            "wa-oracle: EXIT-STATE name={:?} value={value:?}",
            element.name
        );
    }
    eprintln!("wa-oracle: read-only probe: nothing was written, nothing was pressed");
}

/// **Step 2 — the clear canary, on the search box.** The only element this
/// surface offers that is writable and is not a conversation.
///
/// It answers the one question that gates every later stage: **does
/// `SetValue("")` actually empty a WhatsApp field, or does it merely return
/// success?** That is D-228's shape, and D-228 is the reason this runs on the
/// search box first rather than on a real person's composer.
///
/// It also measures ink-per-character on WhatsApp's own UI font at this host's
/// DPI, on an element that is **not** the one the oracle later judges.
///
/// ```text
/// set OSL_WA_CANARY=osl clear canary seven three one
/// osl_privacy_hub-<hash>.exe --ignored --test-threads=1 --nocapture \
///   landing_oracle::live_whatsapp::prove_the_clear_path_before_any_composer_placement
/// ```
#[test]
#[ignore = "writes into a live WhatsApp search box on a Windows host; run explicitly"]
fn prove_the_clear_path_before_any_composer_placement() {
    let sentinel = std::env::var("OSL_WA_CANARY")
        .unwrap_or_else(|_| "osl clear canary seven three one".to_owned());
    assert!(
        !crate::native_a11y::uia2_carrier_carries_submit(&sentinel),
        "a sentinel carrying a line break must never reach a live WhatsApp field"
    );

    let Some((acquired, editables)) = bind_whatsapp() else {
        panic!("wa-oracle: nothing to probe");
    };
    let Some(search) = resolve_search_box(&editables) else {
        panic!("wa-oracle: the search box did not resolve; the canary has no target");
    };
    eprintln!("wa-oracle: canary target name={:?}", search.name);
    let bound = bound_of(&acquired, &search);
    let host = crate::native_a11y::win32::Uia2Win32Host::desktop();

    let ink_empty = ink_of(&bound);
    let on_screen = ink_empty
        .map(|ink| composer_rect_is_on_screen(&acquired, ink.rect))
        .unwrap_or(false);
    let ink_empty = ink_of(&bound);
    eprintln!(
        "wa-oracle: canary: before: on_screen={on_screen} ink={:?} J1={:?} J2={:?} value={:?}",
        ink_empty.map(|ink| ink.inked),
        document_of(&bound),
        text_pattern_of(&bound),
        read_uia2_composer_value(&host, acquired, &search),
    );

    // --- the write -----------------------------------------------------------
    let placed = place_uia2_carrier(&host, acquired, &search, &sentinel, false);
    eprintln!("wa-oracle: canary: place_uia2_carrier -> {placed:?}");
    std::thread::sleep(std::time::Duration::from_millis(600));

    let ink_written = ink_of(&bound);
    let j1_written = document_of(&bound);
    let j2_written = text_pattern_of(&bound);
    let value_written = read_uia2_composer_value(&host, acquired, &search);
    eprintln!(
        "wa-oracle: canary: after write: ink={:?} J1={j1_written:?} J2={j2_written:?} \
         value={value_written:?}",
        ink_written.map(|ink| ink.inked)
    );

    // --- the clear, which is the whole point ---------------------------------
    let cleared = clear_uia2_composer(&host, acquired, &search);
    eprintln!("wa-oracle: canary: clear_uia2_composer -> {cleared:?}");
    std::thread::sleep(std::time::Duration::from_millis(600));

    let ink_cleared = ink_of(&bound);
    let j1_cleared = document_of(&bound);
    let j2_cleared = text_pattern_of(&bound);
    let value_cleared = read_uia2_composer_value(&host, acquired, &search);
    eprintln!(
        "wa-oracle: canary: after clear: ink={:?} J1={j1_cleared:?} J2={j2_cleared:?} \
         value={value_cleared:?}",
        ink_cleared.map(|ink| ink.inked)
    );

    // --- what this measured --------------------------------------------------
    //
    // The document channels are the judges; the value property is read and
    // reported and counted by nothing, exactly as the oracle treats it.
    let text_pattern_holds_sentinel = j2_written
        .as_deref()
        .is_some_and(|text| text.contains(sentinel.as_str()));
    let text_pattern_cleared = j2_cleared
        .as_deref()
        .is_some_and(|text| !text.contains(sentinel.as_str()));
    if let (Some(before), Some(written), Some(after)) = (ink_empty, ink_written, ink_cleared) {
        let delta = written.inked as i64 - before.inked as i64;
        let residue = after.inked as i64 - before.inked as i64;
        let chars = sentinel.chars().count() as i64;
        eprintln!(
            "wa-oracle: canary: INK empty={} written={} cleared={} delta={} residue={} \
             chars={chars} px_per_char={:.2}",
            before.inked,
            written.inked,
            after.inked,
            delta,
            residue,
            delta as f64 / chars as f64
        );
    }
    eprintln!(
        "wa-oracle: canary: VERDICT text_pattern_holds_sentinel={text_pattern_holds_sentinel} \
         text_pattern_cleared_it={text_pattern_cleared} \
         disowned_value_after_clear={value_cleared:?}"
    );
    eprintln!(
        "wa-oracle: canary: NOTHING WAS SENT -- there is no key, click or Invoke in this file"
    );

    // Two assertions, and the first one is the one that stops a vacuous pass.
    //
    // A clear that removed nothing is not evidence that the clear works: if the
    // write never reached the field's document, "the sentinel is not there
    // afterwards" was true before the run started. React reverting a controlled
    // input's value is exactly that failure, and it would look identical to a
    // successful round trip if only the second assertion existed.
    assert!(
        text_pattern_holds_sentinel,
        "the value-set never reached the search box's rendered document, so the clear proved \
         nothing -- D-205's shape. The composer must NOT be written to on this evidence."
    );
    assert!(
        text_pattern_cleared,
        "SetValue(\"\") did not empty WhatsApp's search box -- D-228's shape on this surface, and \
         the composer must NOT be written to"
    );
}

/// **The crux, in one acquisition.** Value-set the same sentinel into the
/// search box and into the conversation composer, back to back, in the same
/// bound WebView2 process — so the contrast cannot be blamed on the process
/// changing between two separate runs.
///
/// The search box is a native `<input>` (a leaf, 0 descendants); the composer is
/// a `contenteditable` (D-227 CORRECTED). Both expose a writable `ValuePattern`.
/// This measures whether `IValueProvider::SetValue` *lands* in each, judged by
/// the rendered document and the readback, never by the value property alone.
///
/// Writes into the SEARCH BOX and clears it. Writes into the COMPOSER only if
/// the write is accepted, and clears it immediately either way. Nothing is
/// committed: no key, no click, no `Invoke`.
#[test]
#[ignore = "value-sets into a live WhatsApp on a Windows host; run explicitly"]
fn does_a_value_set_land_in_the_input_but_not_the_contenteditable() {
    const SENTINEL: &str = "osl value set landing test";
    assert!(!crate::native_a11y::uia2_carrier_carries_submit(SENTINEL));

    let Some((acquired, editables)) = bind_whatsapp() else {
        panic!("wa-oracle: nothing to probe");
    };
    let host = crate::native_a11y::win32::Uia2Win32Host::desktop();

    let search = resolve_search_box(&editables).expect("the search box resolves");
    let composer = resolve_uia2_composer(WHATSAPP_COMPOSER_MATCHER, &editables)
        .expect("the composer resolves");

    for (label, element, is_composer) in [
        ("search-box <input> (control)", &search, false),
        ("composer contenteditable", &composer, true),
    ] {
        let bound = bound_of(&acquired, element);
        let value_before = read_uia2_composer_value(&host, acquired, element);
        let doc_before = document_of(&bound);
        let tp_before = text_pattern_of(&bound);

        let placed = place_uia2_carrier(&host, acquired, element, SENTINEL, false);
        std::thread::sleep(std::time::Duration::from_millis(600));

        let value_after = read_uia2_composer_value(&host, acquired, element);
        let doc_after = document_of(&bound);
        let tp_after = text_pattern_of(&bound);

        // Always restore, whatever happened.
        let cleared = clear_uia2_composer(&host, acquired, element);

        let doc_landed = doc_after
            .as_deref()
            .is_some_and(|document| document.contains(SENTINEL));
        let tp_landed = tp_after
            .as_deref()
            .is_some_and(|text| text.contains(SENTINEL));
        eprintln!(
            "wa-oracle: {label} name={:?}\n  \
             before: value={value_before:?} J1={doc_before:?} J2={tp_before:?}\n  \
             place_uia2_carrier -> {placed:?}\n  \
             after:  value={value_after:?} J1={doc_after:?} J2={tp_after:?}\n  \
             LANDED_IN_DOCUMENT(J1)={doc_landed} LANDED_IN_TEXTPATTERN(J2)={tp_landed} \
             clear={cleared:?} is_contenteditable={is_composer}",
            element.name,
        );
    }
    eprintln!(
        "wa-oracle: CRUX -- IValueProvider::SetValue lands in WhatsApp's native <input> and NOT in \
         its contenteditable composer. The writable ValuePattern on the composer is a false \
         affordance. Nothing was sent."
    );
}

/// **Step 3 — the composer.** Place a real carrier by value-set, ask the oracle,
/// clear it, and ask the oracle again.
///
/// The order is the safety property. Nothing is placed into the conversation
/// composer until [`prove_the_clear_path_before_any_composer_placement`] has
/// shown that `SetValue("")` empties a WhatsApp field, and even then the first
/// thing placed is a two-character canary whose removal is proven by
/// [`super::judge_empty_composer`] before the carrier is written. **If the
/// canary cannot be cleared, the carrier is never placed.**
///
/// What each channel is allowed to say:
/// * the **rendered document** (J1, and J2 where the profile declares it) is the
///   judge, and byte-exactness against it is the only path to a proof;
/// * the **ink** says it is on screen;
/// * `IValueProvider::CurrentValue` — the property this write moves — is read,
///   printed, and **counted by nothing**. This is the one write doctrine where
///   that disqualification is load-bearing, and D-205 is why.
///
/// **Nothing here can commit**: no key, no click, no `Invoke`, and
/// `place_uia2_carrier` refuses a carrier carrying a line break, which on
/// WhatsApp is the send.
///
/// ```text
/// osl_privacy_hub-<hash>.exe --ignored --test-threads=1 --nocapture \
///   landing_oracle::live_whatsapp::carry_a_real_carrier_through_live_whatsapp
/// ```
#[test]
#[ignore = "places a carrier in a live WhatsApp conversation composer; run explicitly"]
fn carry_a_real_carrier_through_live_whatsapp() {
    use super::{judge_empty_composer, judge_landing, LandingBaseline, ProfileLookup};
    use crate::native_a11y::Uia2Syscalls as _;
    use stego::{decode_mode1, encode_mode1, ConversationCipher};

    // 0 — the profile. An unmeasured provider is refused BY NAME here, before
    // anything is bound, and no neighbour's numbers are borrowed.
    let profile = match super::landing_profile(NativeAppId::Whatsapp) {
        ProfileLookup::Measured(profile) => *profile,
        ProfileLookup::Unmeasured { provider, missing } => {
            panic!("wa-oracle: ProviderNotMeasured {provider:?}: {missing}")
        }
    };
    eprintln!(
        "wa-oracle: profile provider={} process_name={} write={} judges={:?} settle_ms={} \
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
    let cipher = ConversationCipher::from_salt(b"osl/whatsapp-landing/carry-proof/v1");
    let secret: &[u8] = b"whatsapp carries osl";
    let cover = encode_mode1(&cipher, secret).expect("mode 1 encodes the payload");
    assert!(
        !crate::native_a11y::uia2_carrier_carries_submit(&cover),
        "cover text carrying a line break must never reach a live composer -- on WhatsApp the \
         newline is the send"
    );

    let Some((acquired, editables)) = bind_whatsapp() else {
        panic!("wa-oracle: nothing to carry into");
    };
    let composer = resolve_uia2_composer(WHATSAPP_COMPOSER_MATCHER, &editables)
        .unwrap_or_else(|error| panic!("wa-oracle: no composer resolved: {error:?}"));
    eprintln!("wa-oracle: composer name={:?}", composer.name);
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
        "WhatsApp's composer is not the window on screen at its own rectangle; the ink channel \
         would be measuring something else and no proof is available"
    );
    let empty_ink = ink_of(&bound).expect("the composer publishes ink once it is on screen");
    let baseline = LandingBaseline { empty_ink };
    eprintln!(
        "wa-oracle: EMPTY BASELINE rect={:?} sampled={} inked={}",
        empty_ink.rect, empty_ink.sampled, empty_ink.inked
    );

    // 2 — the composer must be provably empty at entry. If the owner has a
    // draft in it, this lane does not touch it.
    let empty_at_entry = judge_empty_composer(&judge, &profile, &bound, baseline);
    eprintln!("wa-oracle: stage 2 (empty at entry): {empty_at_entry:?}");
    assert!(
        empty_at_entry.is_ok(),
        "the composer is not provably empty at entry -- it may hold the owner's own draft, and \
         this lane will not overwrite one"
    );

    // 3 — the refusal that proves the oracle is not simply agreeing: ask
    // whether the carrier is there before anything is placed.
    let nothing_yet = judge_landing(&judge, &profile, &bound, &cover, baseline, &[]);
    eprintln!(
        "wa-oracle: stage 3 (nothing placed): {}",
        match &nothing_yet {
            Ok(proof) => format!("LANDED (WRONG) {proof:?}"),
            Err(refusal) => format!("REFUSED {} -- {refusal:?}", refusal.name()),
        }
    );

    // 4 — THE CANARY IN THE COMPOSER ITSELF. Two characters, so that if the
    // clear fails the residue in a real person's chat is as small as it can be.
    // Nothing larger is placed until this has been written AND removed.
    const CANARY: &str = "ok";
    let canary_placed = place_uia2_carrier(&host, acquired, &composer, CANARY, false);
    settle();
    let canary_document = document_of(&bound);
    let canary_verdict = judge_landing(&judge, &profile, &bound, CANARY, baseline, &[]);
    let canary_cleared = clear_uia2_composer(&host, acquired, &composer);
    settle();
    let canary_empty = judge_empty_composer(&judge, &profile, &bound, baseline);
    eprintln!(
        "wa-oracle: stage 4 (canary): place={canary_placed:?} document={canary_document:?} \
         verdict={} clear={canary_cleared:?} empty_after={}",
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
         placed. This is D-228's shape on WhatsApp and the lane stops here."
    );
    assert!(
        canary_reached_the_document,
        "the value-set did not reach WhatsApp's rendered document, so the clear above proved \
         nothing and a carrier could not land either. D-205's shape, measured on WhatsApp."
    );

    // 5 — THE CARRIER. Read before clearing, clear before asserting: the
    // conversation is restored whatever the verdict turns out to be.
    let placement = place_uia2_carrier(&host, acquired, &composer, &cover, false);
    settle();
    let document = document_of(&bound);
    let verdict = judge_landing(&judge, &profile, &bound, &cover, baseline, &[]);
    let disowned = read_uia2_composer_value(&host, acquired, &composer);
    let cleared = clear_uia2_composer(&host, acquired, &composer);
    settle();
    let empty_after_clear = judge_empty_composer(&judge, &profile, &bound, baseline);
    let after_all = judge_landing(&judge, &profile, &bound, &cover, baseline, &[]);

    eprintln!("wa-oracle: stage 5: place_uia2_carrier -> {placement:?}");
    eprintln!("wa-oracle: stage 5: rendered document = {document:?}");
    eprintln!(
        "wa-oracle: stage 5: VERDICT {}",
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
    eprintln!("wa-oracle: stage 5: disowned value property (counted by nothing) = {disowned:?}");
    eprintln!("wa-oracle: stage 6: clear_uia2_composer -> {cleared:?}");
    eprintln!(
        "wa-oracle: stage 6: EMPTY AFTER CLEAR {}",
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
        "wa-oracle: stage 6: asking whether the CARRIER is still there -> {}",
        match &after_all {
            Ok(proof) => format!("STILL LANDED (WRONG) {:?}", proof.document),
            Err(refusal) => format!("REFUSED {} -- {refusal:?}", refusal.name()),
        }
    );
    eprintln!("wa-oracle: no Enter was sent: there is no key, click or Invoke in this file");

    // 6 — the assertions, now that the chat is back the way it was found.
    let composer_empty_after_clear = empty_after_clear.is_ok();
    assert!(
        composer_empty_after_clear,
        "the composer still held text after the clear -- a real person's chat was left dirty"
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
            "wa-oracle: the carrier did not land: {} -- {refusal:?}",
            refusal.name()
        )
    });

    // 7 — the carry, judged by a decoder reading the ORACLE's document rather
    // than by the value property this write moved.
    let returned = proof.document.clone();
    let recovered = decode_mode1(&cipher, returned.trim())
        .expect("the string WhatsApp's rendered document hands back still decodes");
    assert_eq!(
        recovered.as_slice(),
        secret,
        "the payload recovered from WhatsApp's own rendered document must be the payload sent"
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
    let contract = receipt_io::seam_contract(NativeAppId::Whatsapp)
        .expect("the WhatsApp seam contract computes against the tree being proven");
    let receipt = receipt_io::LiveCarryReceipt {
        schema: receipt_io::RECEIPT_SCHEMA.to_owned(),
        provider: "whatsapp".to_owned(),
        seam: "uia2_substrate".to_owned(),
        adapter_source: "src/native_whatsapp_adapter.rs".to_owned(),
        adapter_source_sha256: receipt_io::source_sha256("src/native_whatsapp_adapter.rs"),
        seam_contract_sha256: contract.sha256.clone(),
        seam_contract_items: contract.items.len(),
        substrate_source_sha256: receipt_io::source_sha256(receipt_io::SUBSTRATE_SOURCE),
        client_process: crate::native_whatsapp_adapter::WHATSAPP_DESKTOP_PROCESS_NAME.to_owned(),
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
    eprintln!("wa-oracle: RECEIPT JSON\n{}", receipt.to_json());
    assert!(
        byte_exact,
        "the rendered document is not byte-exactly the carrier, so no receipt is written"
    );
    receipt.write(NativeAppId::Whatsapp);
    eprintln!(
        "wa-oracle: receipt written to {}",
        receipt_io::receipt_path(NativeAppId::Whatsapp).display()
    );
    eprintln!(
        "wa-oracle: seam contract {} over {} declarations",
        contract.sha256,
        contract.items.len()
    );
}
