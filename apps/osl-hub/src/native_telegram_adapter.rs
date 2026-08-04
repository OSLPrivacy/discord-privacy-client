//! Telegram Desktop accessibility selectors and live placement contract.
//!
//! Telegram exposes each message as a `ListItem` whose direct children are
//! column sub-items.  This module deliberately selects only those direct
//! columns; it never walks a row's arbitrary descendant tree.
//!
//! The live half does not contain a driver.  Every window, wake, poll, write
//! and read-back decision belongs to [`crate::native_a11y`]; this module
//! contributes the three per-provider facts A-00 measured on the owner's
//! Windows host and nothing else:
//!
//! 1. which window to bind -- Telegram is Qt, so the outer `Qt51519QWindowIcon`
//!    window *is* the UIA root;
//! 2. whether a wake is needed -- it is not, Qt publishes UIA natively;
//! 3. which editable element is the composer -- the one named
//!    `Write a message...`, and never `Search` or `Phone number`.
//!
//! Placement only.  There is no verb in this module, or in the syscall seam it
//! drives, that can commit a Telegram message.

use crate::adapters::{Bounds, PaintConfidence, PaintTarget};
use crate::native_a11y::{
    acquire_uia2_window, clear_uia2_composer, place_uia2_carrier, resolve_uia2_composer,
    Uia2AcquireError, Uia2Acquired, Uia2CallTimeout, Uia2ComposerError, Uia2ComposerMatcher,
    Uia2Deadline, Uia2Editable, Uia2OwnedWindow, Uia2PlacementRefusal, Uia2Syscalls, Uia2TreeRoute,
    Uia2WindowPlan, Uia2WindowResolveError,
};

pub use crate::native_a11y::TELEGRAM_OUTER_WINDOW_CLASS;

/// Telegram Desktop's image name. `resolve_uia2_window` strips `.exe`.
pub const TELEGRAM_DESKTOP_PROCESS_NAME: &str = "Telegram";

/// The deadline every cross-process accessibility call against Telegram
/// carries. Same budget as the other providers: a provider answering slower
/// than this is the state that precedes a cross-process freeze.
pub const TELEGRAM_UIA2_DEFAULT_CALL_TIMEOUT_MS: u64 = 750;

/// What A-00 counted on the live client. Recorded so a future probe that reads
/// a fraction of it is visibly a regression rather than a new baseline.
pub const TELEGRAM_UIA2_MEASURED_ELEMENTS: usize = 743;

/// The accessible name A-00 measured on the live composer, with the ellipsis
/// Telegram renders. `uia2_name_is_composer` normalises the trailing dots away.
pub const TELEGRAM_COMPOSER_MEASURED_NAME: &str = "Write a message...";

pub const TELEGRAM_LIVE_CARRIER_MAX_BYTES: usize = 4096;

/// Telegram's measured UIA2 window shape.
///
/// `DirectOuterWindow`, `Uia2WakePolicy::None`, `poll_until_populated: false`,
/// and no `renderer_child_class` -- all four fall out of one measurement:
/// Telegram Desktop is **Qt**, not Chromium. There is no
/// `Chrome_RenderWidgetHostHWND` anywhere below `Qt51519QWindowIcon`, and Qt
/// publishes its UI Automation tree eagerly, so there is nothing to wake and
/// nothing to wait for. A-00 read 743 elements on a cold probe.
///
/// This is the counter-trap to WhatsApp's. Code that *requires* a renderer
/// child does not merely lose a wake it did not need -- it fails to resolve a
/// window at all and reports Telegram as undrivable. That failure mode is the
/// mutant this module's tests exist to catch.
///
/// # Tree route: `UiaNative`, and why it is not Discord's
///
/// Discord's shipping route is `Uia2TreeRoute::MsaaBridge`: it wakes the outer
/// Chromium window, takes the *custom MSAA client object* Chromium hands back
/// for object id 1, and bridges that object into UI Automation. That route is
/// available only because Chromium implements that handshake.
///
/// Telegram implements no such handshake. Qt answers `WM_GETOBJECT` for
/// `OBJID_CLIENT`, not for Chromium's custom object id, so `MsaaBridge` has
/// nothing to bridge *from* -- and it would also be pointless, because the
/// thing the bridge exists to reach is already there: `ElementFromHandle` on
/// the Qt window returns a populated native UIA root directly. So Telegram is
/// the one provider whose window shape and tree route agree that the outer
/// window is the whole answer, and it reaches it by the plain
/// `Uia2TreeRoute::UiaNative` route.
pub const TELEGRAM_UIA2_WINDOW_PLAN: Uia2WindowPlan = Uia2WindowPlan::direct_outer_window(
    "Telegram",
    TELEGRAM_DESKTOP_PROCESS_NAME,
    TELEGRAM_OUTER_WINDOW_CLASS,
    TELEGRAM_UIA2_DEFAULT_CALL_TIMEOUT_MS,
);

/// How Telegram's composer is told apart from every other writable element.
///
/// The negative stems are checked first and they are the load-bearing half.
/// Telegram's chat list carries a writable `Search` box on every screen, and a
/// client that is not signed in shows a writable `Phone number` field with no
/// composer at all. Both satisfy "editable, enabled, keyboard-focusable,
/// `ValuePattern`" exactly as the real composer does. Placing a carrier into
/// either is a disclosure, so they are refused by name before any positive
/// stem is considered.
///
/// Only `"write a message"` is measured. `"message"` is a deliberate widening
/// for Telegram's reply and channel variants of the same field; anything
/// beyond these two -- in particular a localised placeholder -- must be added
/// from a measurement on a live client, never guessed, because a wrong stem
/// here does not fail closed.
pub const TELEGRAM_COMPOSER_MATCHER: Uia2ComposerMatcher = Uia2ComposerMatcher {
    composer_stems: &["write a message", "message"],
    non_composer_stems: &[
        "search",
        "filter",
        "phone number",
        "username",
        "password",
        "code",
        "caption",
        "link",
        "url",
        "name",
    ],
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TelegramPlacementStatus {
    Placed,
    PlatformUnsupported,
    InvalidCarrier,
    /// No Telegram window matched the measured shape. Under the renderer-child
    /// mutant this is what Telegram degrades to, which is why it is distinct
    /// from `AccessibilityUnavailable`.
    WindowUnavailable,
    AccessibilityUnavailable,
    ComposerUnavailable,
    ComposerAmbiguous,
    ComposerNotWritable,
    ComposerNotEmpty,
    ReadbackMismatch,
    ProbeClearFailed,
    /// A cross-process call overran the plan's `call_timeout_ms`.
    CallTimedOut,
    /// The syscall backend reported a submit-shaped interaction. Placement is
    /// abandoned rather than trusted for the rest of the path.
    SubmitShapedCallObserved,
}

pub struct TelegramLivePlacementRequest<'a> {
    pub carrier: &'a str,
    pub allow_replace_existing: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TelegramLivePlacementReceipt {
    pub placed: bool,
    /// Never a literal in this module's success path: it is set true only from
    /// the backend's own submit-shaped counter, and setting it true refuses.
    pub enter_sent: bool,
    pub status: TelegramPlacementStatus,
    pub element_count: usize,
    /// Must be `false` for Telegram. A `true` here means something woke a Qt
    /// window with Chromium's handshake, which is a plan defect, not a quirk.
    pub woke: bool,
    /// Must be `0` for Telegram: there is no asynchronously populated tree.
    pub settled_ms: u64,
    pub writable_composer_count: usize,
    pub readback_contains_carrier: bool,
    /// The substrate's own error, carried verbatim so a mutant transcript names
    /// the rung that broke instead of a status that lost the detail.
    pub acquire_error: Option<Uia2AcquireError>,
}

impl TelegramLivePlacementReceipt {
    fn refused(status: TelegramPlacementStatus) -> Self {
        Self {
            placed: false,
            enter_sent: false,
            status,
            element_count: 0,
            woke: false,
            settled_ms: 0,
            writable_composer_count: 0,
            readback_contains_carrier: false,
            acquire_error: None,
        }
    }

    /// The backend admitted a submit-shaped interaction. This is the only
    /// constructor in the module that can set `enter_sent`, and it is reached
    /// only from a backend-reported delta.
    fn refused_after_submit_shaped_call() -> Self {
        Self {
            enter_sent: true,
            ..Self::refused(TelegramPlacementStatus::SubmitShapedCallObserved)
        }
    }
}

/// Borrow the deadline the substrate issued, so the one syscall the substrate
/// does not wrap in a public entry point can still be reached.
///
/// [`Uia2Deadline`]'s constructor is private to `native_a11y` on purpose: a
/// caller must not be able to invent its own budget. The substrate wraps five
/// of its six deadline-carrying syscalls in public functions
/// (`acquire_uia2_window` covers enumerate/wake/count, `place_uia2_carrier` and
/// `clear_uia2_composer` cover set/read). `editable_elements` is the sixth and
/// has no public wrapper, so no module outside `native_a11y` can list a
/// composer candidate -- which is every consumer's second step.
///
/// This relay does not weaken that encapsulation: it never constructs a
/// deadline, it records the one `acquire_uia2_window` derived from
/// `TELEGRAM_UIA2_WINDOW_PLAN.call_timeout_ms` and hands that same value back.
/// It forwards every method, including `submit_shaped_calls` -- a relay that
/// answered that one itself would blind the placement guard, so it is pinned by
/// a test.
///
/// It should be deleted the moment `native_a11y` grows the wrapper. Logged for
/// that lane, not worked around silently.
struct Uia2DeadlineRelay<'a> {
    inner: &'a dyn Uia2Syscalls,
    issued: std::cell::Cell<Option<Uia2Deadline>>,
}

impl<'a> Uia2DeadlineRelay<'a> {
    fn new(inner: &'a dyn Uia2Syscalls) -> Self {
        Self {
            inner,
            issued: std::cell::Cell::new(None),
        }
    }

    fn record(&self, deadline: Uia2Deadline) -> Uia2Deadline {
        self.issued.set(Some(deadline));
        deadline
    }

    fn issued_deadline(&self) -> Option<Uia2Deadline> {
        self.issued.get()
    }
}

impl Uia2Syscalls for Uia2DeadlineRelay<'_> {
    fn enumerate_windows(
        &self,
        deadline: Uia2Deadline,
    ) -> Result<Vec<Uia2OwnedWindow>, Uia2CallTimeout> {
        self.inner.enumerate_windows(self.record(deadline))
    }

    fn wake_chromium(
        &self,
        hwnd: isize,
        deadline: Uia2Deadline,
    ) -> Result<bool, Uia2CallTimeout> {
        self.inner.wake_chromium(hwnd, self.record(deadline))
    }

    fn element_count(
        &self,
        hwnd: isize,
        route: Uia2TreeRoute,
        deadline: Uia2Deadline,
    ) -> Result<usize, Uia2CallTimeout> {
        self.inner
            .element_count(hwnd, route, self.record(deadline))
    }

    fn editable_elements(
        &self,
        hwnd: isize,
        route: Uia2TreeRoute,
        deadline: Uia2Deadline,
    ) -> Result<Vec<Uia2Editable>, Uia2CallTimeout> {
        self.inner
            .editable_elements(hwnd, route, self.record(deadline))
    }

    fn set_value(
        &self,
        hwnd: isize,
        route: Uia2TreeRoute,
        element: &Uia2Editable,
        value: &str,
        deadline: Uia2Deadline,
    ) -> Result<bool, Uia2CallTimeout> {
        self.inner
            .set_value(hwnd, route, element, value, self.record(deadline))
    }

    fn value_of(
        &self,
        hwnd: isize,
        route: Uia2TreeRoute,
        element: &Uia2Editable,
        deadline: Uia2Deadline,
    ) -> Result<Option<String>, Uia2CallTimeout> {
        self.inner
            .value_of(hwnd, route, element, self.record(deadline))
    }

    fn submit_shaped_calls(&self) -> usize {
        self.inner.submit_shaped_calls()
    }

    fn settle(&self, millis: u64) {
        self.inner.settle(millis)
    }
}

fn acquire_status(error: Uia2AcquireError) -> TelegramPlacementStatus {
    match error {
        Uia2AcquireError::Resolve(Uia2WindowResolveError::MissingAppOuter)
        | Uia2AcquireError::Resolve(Uia2WindowResolveError::MissingRendererChild)
        | Uia2AcquireError::Resolve(Uia2WindowResolveError::MissingSiblingContentOuter) => {
            TelegramPlacementStatus::WindowUnavailable
        }
        Uia2AcquireError::WakeRefused | Uia2AcquireError::TreeNeverPopulated { .. } => {
            TelegramPlacementStatus::AccessibilityUnavailable
        }
        Uia2AcquireError::CallTimedOut(_) => TelegramPlacementStatus::CallTimedOut,
    }
}

fn composer_status(error: Uia2ComposerError) -> (TelegramPlacementStatus, usize) {
    match error {
        Uia2ComposerError::NoEditable
        | Uia2ComposerError::NoWritableEditable
        | Uia2ComposerError::NoComposerName => (TelegramPlacementStatus::ComposerUnavailable, 0),
        Uia2ComposerError::Ambiguous(count) => (TelegramPlacementStatus::ComposerAmbiguous, count),
    }
}

fn refusal_status(refusal: Uia2PlacementRefusal) -> TelegramPlacementStatus {
    match refusal {
        Uia2PlacementRefusal::EmptyCarrier | Uia2PlacementRefusal::CarrierCarriesSubmit => {
            TelegramPlacementStatus::InvalidCarrier
        }
        Uia2PlacementRefusal::ExistingDraft => TelegramPlacementStatus::ComposerNotEmpty,
        Uia2PlacementRefusal::SetValueRefused => TelegramPlacementStatus::ComposerNotWritable,
        Uia2PlacementRefusal::ReadbackMissingCarrier => TelegramPlacementStatus::ReadbackMismatch,
        Uia2PlacementRefusal::SubmitShaped => TelegramPlacementStatus::SubmitShapedCallObserved,
        Uia2PlacementRefusal::CallTimedOut(_) => TelegramPlacementStatus::CallTimedOut,
    }
}

/// What a successful placement leaves behind, so the probe path can clear the
/// exact element it wrote to rather than re-resolving one.
struct TelegramPlacedComposer {
    acquired: Uia2Acquired,
    composer: Uia2Editable,
}

fn place_through_substrate(
    relay: &Uia2DeadlineRelay<'_>,
    request: TelegramLivePlacementRequest<'_>,
) -> (
    TelegramLivePlacementReceipt,
    Option<TelegramPlacedComposer>,
) {
    if !valid_candidate_text(request.carrier, TELEGRAM_LIVE_CARRIER_MAX_BYTES) {
        return (
            TelegramLivePlacementReceipt::refused(TelegramPlacementStatus::InvalidCarrier),
            None,
        );
    }

    let acquired = match acquire_uia2_window(TELEGRAM_UIA2_WINDOW_PLAN, relay) {
        Ok(acquired) => acquired,
        Err(error) => {
            let mut receipt = TelegramLivePlacementReceipt::refused(acquire_status(error));
            receipt.acquire_error = Some(error);
            return (receipt, None);
        }
    };

    let mut receipt = TelegramLivePlacementReceipt::refused(TelegramPlacementStatus::Placed);
    receipt.element_count = acquired.elements;
    receipt.woke = acquired.woke;
    receipt.settled_ms = acquired.settled_ms;

    // The one rung the substrate does not expose publicly; see Uia2DeadlineRelay.
    let Some(deadline) = relay.issued_deadline() else {
        receipt.status = TelegramPlacementStatus::AccessibilityUnavailable;
        return (receipt, None);
    };
    let editables = match relay.editable_elements(
        acquired.window.bound_hwnd,
        acquired.window.tree_route,
        deadline,
    ) {
        Ok(editables) => editables,
        Err(_) => {
            receipt.status = TelegramPlacementStatus::CallTimedOut;
            return (receipt, None);
        }
    };

    let composer = match resolve_uia2_composer(TELEGRAM_COMPOSER_MATCHER, &editables) {
        Ok(composer) => composer,
        Err(error) => {
            let (status, count) = composer_status(error);
            receipt.status = status;
            receipt.writable_composer_count = count;
            return (receipt, None);
        }
    };
    receipt.writable_composer_count = 1;

    match place_uia2_carrier(
        relay,
        acquired,
        &composer,
        request.carrier,
        request.allow_replace_existing,
    ) {
        Ok(placement) => {
            if placement.submit_shaped_observed {
                return (
                    TelegramLivePlacementReceipt::refused_after_submit_shaped_call(),
                    None,
                );
            }
            receipt.placed = placement.placed;
            receipt.readback_contains_carrier = placement.readback_holds_carrier;
            receipt.status = TelegramPlacementStatus::Placed;
            (
                receipt,
                Some(TelegramPlacedComposer { acquired, composer }),
            )
        }
        Err(Uia2PlacementRefusal::SubmitShaped) => (
            TelegramLivePlacementReceipt::refused_after_submit_shaped_call(),
            None,
        ),
        Err(refusal) => {
            receipt.status = refusal_status(refusal);
            // A read-back that lost the carrier still wrote: the composer must
            // be cleared even though the placement failed.
            let wrote = matches!(refusal, Uia2PlacementRefusal::ReadbackMissingCarrier);
            (
                receipt,
                wrote.then_some(TelegramPlacedComposer { acquired, composer }),
            )
        }
    }
}

/// Place a carrier into Telegram's live composer through the shared substrate.
///
/// Not a send path. The whole vocabulary is: resolve a window, list its
/// editable elements, write one value, read it back by containment. Nothing
/// here, and nothing in [`Uia2Syscalls`], can commit the message.
pub fn drive_telegram_composer_placement(
    host: &dyn Uia2Syscalls,
    request: TelegramLivePlacementRequest<'_>,
) -> TelegramLivePlacementReceipt {
    let relay = Uia2DeadlineRelay::new(host);
    place_through_substrate(&relay, request).0
}

/// Probe Telegram's composer and then clear it.
///
/// Telegram is the one provider signed in on the owner's host, so this is the
/// path that touches a real person's chat. It always clears after any write --
/// including a write whose read-back failed -- and the clear is itself covered:
/// the backend's submit-shaped counter is re-read afterwards, so a clear that
/// commits the composer is caught rather than assumed impossible.
pub fn probe_telegram_composer_write_then_clear(
    host: &dyn Uia2Syscalls,
    request: TelegramLivePlacementRequest<'_>,
) -> TelegramLivePlacementReceipt {
    let relay = Uia2DeadlineRelay::new(host);
    let (mut receipt, placed) = place_through_substrate(&relay, request);
    if let Some(placed) = placed {
        let before_clear = relay.submit_shaped_calls();
        if clear_uia2_composer(&relay, placed.acquired, &placed.composer).is_err() {
            receipt.placed = false;
            receipt.status = TelegramPlacementStatus::ProbeClearFailed;
        }
        if relay.submit_shaped_calls() > before_clear {
            return TelegramLivePlacementReceipt::refused_after_submit_shaped_call();
        }
    }
    receipt
}

/// Drive the live Windows host. Defined under both cfgs deliberately: A-01
/// shipped a placement function that existed only under `cfg(not(windows))`,
/// so the platform that matters had no entry point at all and a Linux build
/// could not tell.
#[cfg(target_os = "windows")]
pub fn place_telegram_desktop_carrier(
    request: TelegramLivePlacementRequest<'_>,
) -> TelegramLivePlacementReceipt {
    drive_telegram_composer_placement(&crate::native_a11y::win32::Uia2Win32Host::desktop(), request)
}

#[cfg(not(target_os = "windows"))]
pub fn place_telegram_desktop_carrier(
    _request: TelegramLivePlacementRequest<'_>,
) -> TelegramLivePlacementReceipt {
    TelegramLivePlacementReceipt::refused(TelegramPlacementStatus::PlatformUnsupported)
}

#[cfg(target_os = "windows")]
pub fn probe_telegram_desktop_composer(
    request: TelegramLivePlacementRequest<'_>,
) -> TelegramLivePlacementReceipt {
    probe_telegram_composer_write_then_clear(
        &crate::native_a11y::win32::Uia2Win32Host::desktop(),
        request,
    )
}

#[cfg(not(target_os = "windows"))]
pub fn probe_telegram_desktop_composer(
    _request: TelegramLivePlacementRequest<'_>,
) -> TelegramLivePlacementReceipt {
    TelegramLivePlacementReceipt::refused(TelegramPlacementStatus::PlatformUnsupported)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TelegramRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl TelegramRect {
    pub fn valid(self) -> bool {
        self.right > self.left && self.bottom > self.top
    }
    pub fn width(self) -> i32 {
        self.right.saturating_sub(self.left)
    }
    pub fn height(self) -> i32 {
        self.bottom.saturating_sub(self.top)
    }
    pub fn contained_by(self, parent: Self) -> bool {
        self.valid()
            && parent.valid()
            && self.left >= parent.left
            && self.top >= parent.top
            && self.right <= parent.right
            && self.bottom <= parent.bottom
    }
    pub fn horizontal_overlap(self, other: Self) -> i32 {
        self.right
            .min(other.right)
            .saturating_sub(self.left.max(other.left))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TelegramRole {
    List,
    ListItem,
    Column,
    EditableText,
    Text,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TelegramColumnEvidence {
    None,
    MessageBody,
}

#[derive(Clone)]
pub struct TelegramNode {
    pub role: TelegramRole,
    pub column_evidence: TelegramColumnEvidence,
    pub bounds: TelegramRect,
    pub visible: bool,
    pub enabled: bool,
    pub focusable: bool,
    pub editable: bool,
    pub read_only: bool,
    pub text: Option<String>,
    pub children: Vec<usize>,
}

impl TelegramNode {
    pub fn structural(role: TelegramRole, bounds: TelegramRect) -> Self {
        Self {
            role,
            column_evidence: TelegramColumnEvidence::None,
            bounds,
            visible: true,
            enabled: true,
            focusable: false,
            editable: false,
            read_only: true,
            text: None,
            children: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TelegramSelectorError {
    Missing,
    Ambiguous,
    Invalid,
    LimitExceeded,
}

#[derive(Clone, Eq, PartialEq)]
pub struct TelegramRowCandidate {
    pub node_index: usize,
    pub text: String,
    pub body_bounds: TelegramRect,
}

/// Associate an already-derived carrier digest with the exact accessible body
/// rectangle. Provider text is deliberately not part of the paint target.
pub fn telegram_row_paint_target(
    carrier_sha256: String,
    row: &TelegramRowCandidate,
) -> Result<PaintTarget, TelegramSelectorError> {
    if carrier_sha256.is_empty() || !row.body_bounds.valid() {
        return Err(TelegramSelectorError::Invalid);
    }
    Ok(PaintTarget {
        carrier_sha256,
        rect: Bounds {
            x: row.body_bounds.left,
            y: row.body_bounds.top,
            width: row.body_bounds.width(),
            height: row.body_bounds.height(),
        },
        clipped_by: None,
        confidence: PaintConfidence::Exact,
    })
}

pub fn discover_telegram_composer(
    nodes: &[TelegramNode],
    window: TelegramRect,
) -> Result<usize, TelegramSelectorError> {
    unique(
        nodes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| telegram_composer_candidate(node, window).then_some(index)),
    )
}

pub fn discover_telegram_transcript(
    nodes: &[TelegramNode],
    composer_index: usize,
    window: TelegramRect,
) -> Result<usize, TelegramSelectorError> {
    let composer = nodes
        .get(composer_index)
        .ok_or(TelegramSelectorError::Missing)?;
    if !telegram_composer_candidate(composer, window) {
        return Err(TelegramSelectorError::Invalid);
    }
    unique(nodes.iter().enumerate().filter_map(|(index, node)| {
        telegram_transcript_candidate(node, composer, window).then_some(index)
    }))
}

pub fn extract_telegram_row_candidates(
    nodes: &[TelegramNode],
    row_index: usize,
    max_candidates: usize,
    max_text_bytes: usize,
) -> Result<Vec<TelegramRowCandidate>, TelegramSelectorError> {
    let row = nodes.get(row_index).ok_or(TelegramSelectorError::Missing)?;
    if row.role != TelegramRole::ListItem
        || !row.visible
        || !row.bounds.valid()
        || max_candidates == 0
        || max_text_bytes == 0
    {
        return Err(TelegramSelectorError::Invalid);
    }
    let mut candidates = Vec::new();
    for index in &row.children {
        let column = nodes.get(*index).ok_or(TelegramSelectorError::Invalid)?;
        if column.column_evidence != TelegramColumnEvidence::MessageBody {
            continue;
        }
        let text = column.text.as_ref().ok_or(TelegramSelectorError::Invalid)?;
        if column.role != TelegramRole::Column
            || !column.visible
            || !column.bounds.contained_by(row.bounds)
            || !valid_candidate_text(text, max_text_bytes)
        {
            return Err(TelegramSelectorError::Invalid);
        }
        if candidates.len() >= max_candidates {
            return Err(TelegramSelectorError::LimitExceeded);
        }
        candidates.push(TelegramRowCandidate {
            node_index: *index,
            text: text.clone(),
            body_bounds: column.bounds,
        });
    }
    Ok(candidates)
}

/// A successful scan without readable message bodies is incomplete, never an
/// empty-but-valid transcript.
pub fn telegram_read_was_complete(rows: &[TelegramRowCandidate]) -> bool {
    !rows.is_empty()
}

fn unique(indices: impl Iterator<Item = usize>) -> Result<usize, TelegramSelectorError> {
    let matches = indices.collect::<Vec<_>>();
    match matches.as_slice() {
        [index] => Ok(*index),
        [] => Err(TelegramSelectorError::Missing),
        _ => Err(TelegramSelectorError::Ambiguous),
    }
}

fn telegram_composer_candidate(node: &TelegramNode, window: TelegramRect) -> bool {
    node.role == TelegramRole::EditableText
        && node.visible
        && node.enabled
        && node.focusable
        && node.editable
        && !node.read_only
        && node.bounds.contained_by(window)
        && node.bounds.left >= window.left.saturating_add(window.width() / 3)
        && node.bounds.top >= window.top.saturating_add(window.height() * 3 / 5)
}

fn telegram_transcript_candidate(
    node: &TelegramNode,
    composer: &TelegramNode,
    window: TelegramRect,
) -> bool {
    node.role == TelegramRole::List
        && node.visible
        && node.bounds.contained_by(window)
        && node.bounds.bottom <= composer.bounds.top
        && node.bounds.height() >= window.height() / 4
        && node.bounds.horizontal_overlap(composer.bounds)
            >= composer.bounds.width().saturating_mul(2) / 3
}

fn valid_candidate_text(value: &str, max_text_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_text_bytes
        && !value
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_a11y::tests::{composer, telegram_graph, RecordedHost};
    use crate::native_a11y::{
        uia2_name_is_composer, Uia2WakePolicy, Uia2WindowShape, ELECTRON_OUTER_WINDOW_CLASS,
        ELECTRON_RENDERER_WINDOW_CLASS,
    };

    const CARRIER: &str = "alpha-7731-osl";

    fn writable(name: &str) -> Uia2Editable {
        composer(name)
    }

    fn read_only(name: &str) -> Uia2Editable {
        Uia2Editable {
            read_only: true,
            ..composer(name)
        }
    }

    /// A recorded Telegram: A-00's Qt window graph, its 743 elements, and its
    /// measured composer. Nothing here is a second producer -- the pipeline
    /// under test is `native_a11y`'s.
    fn recorded_telegram(editables: Vec<Uia2Editable>) -> RecordedHost {
        RecordedHost::new(telegram_graph(), TELEGRAM_UIA2_MEASURED_ELEMENTS)
            .with_editables(editables)
    }

    fn signed_in() -> RecordedHost {
        recorded_telegram(vec![
            writable("Search"),
            writable(TELEGRAM_COMPOSER_MEASURED_NAME),
            read_only("Chat list"),
        ])
    }

    fn request(carrier: &str) -> TelegramLivePlacementRequest<'_> {
        TelegramLivePlacementRequest {
            carrier,
            allow_replace_existing: false,
        }
    }

    /// A composer that accepts a write and then reports something else, like a
    /// field whose provider rewrites what was placed. Decorates the shared
    /// recorded host rather than replacing it: everything but the read-back is
    /// still `native_a11y`'s fake.
    struct RewritesWhatWasPlaced<'a> {
        inner: &'a RecordedHost,
    }

    impl Uia2Syscalls for RewritesWhatWasPlaced<'_> {
        fn enumerate_windows(
            &self,
            deadline: Uia2Deadline,
        ) -> Result<Vec<Uia2OwnedWindow>, Uia2CallTimeout> {
            self.inner.enumerate_windows(deadline)
        }
        fn wake_chromium(
            &self,
            hwnd: isize,
            deadline: Uia2Deadline,
        ) -> Result<bool, Uia2CallTimeout> {
            self.inner.wake_chromium(hwnd, deadline)
        }
        fn element_count(
            &self,
            hwnd: isize,
            route: Uia2TreeRoute,
            deadline: Uia2Deadline,
        ) -> Result<usize, Uia2CallTimeout> {
            self.inner.element_count(hwnd, route, deadline)
        }
        fn editable_elements(
            &self,
            hwnd: isize,
            route: Uia2TreeRoute,
            deadline: Uia2Deadline,
        ) -> Result<Vec<Uia2Editable>, Uia2CallTimeout> {
            self.inner.editable_elements(hwnd, route, deadline)
        }
        fn set_value(
            &self,
            hwnd: isize,
            route: Uia2TreeRoute,
            element: &Uia2Editable,
            value: &str,
            deadline: Uia2Deadline,
        ) -> Result<bool, Uia2CallTimeout> {
            self.inner.set_value(hwnd, route, element, value, deadline)
        }
        fn value_of(
            &self,
            hwnd: isize,
            route: Uia2TreeRoute,
            element: &Uia2Editable,
            deadline: Uia2Deadline,
        ) -> Result<Option<String>, Uia2CallTimeout> {
            Ok(self
                .inner
                .value_of(hwnd, route, element, deadline)?
                .map(|_| "the provider rewrote it".to_owned()))
        }
        fn submit_shaped_calls(&self) -> usize {
            self.inner.submit_shaped_calls()
        }
        fn settle(&self, millis: u64) {
            self.inner.settle(millis)
        }
    }

    #[test]
    fn telegram_binds_the_qt_outer_window_natively_and_never_asks_for_a_renderer_child() {
        let plan = TELEGRAM_UIA2_WINDOW_PLAN;
        assert_eq!(plan.app_outer_class, TELEGRAM_OUTER_WINDOW_CLASS);
        assert_eq!(plan.shape, Uia2WindowShape::DirectOuterWindow);
        assert_eq!(
            plan.renderer_child_class, None,
            "Telegram is Qt: there is no Chromium renderer child to require"
        );
        assert_eq!(
            plan.wake_policy,
            Uia2WakePolicy::None,
            "Qt publishes UIA eagerly; Chromium's handshake is a Chromium cost"
        );
        assert_eq!(
            plan.tree_route,
            Uia2TreeRoute::UiaNative,
            "Telegram exposes no Chromium MSAA client object to bridge from"
        );
        assert!(!plan.poll_until_populated);
        assert_ne!(plan.app_outer_class, ELECTRON_OUTER_WINDOW_CLASS);
    }

    #[test]
    fn telegram_places_and_reads_back_without_waking_or_settling() {
        let host = signed_in();
        let receipt = drive_telegram_composer_placement(&host, request(CARRIER));

        assert_eq!(receipt.status, TelegramPlacementStatus::Placed);
        assert!(receipt.placed);
        assert!(receipt.readback_contains_carrier);
        assert!(!receipt.enter_sent);
        assert_eq!(receipt.element_count, TELEGRAM_UIA2_MEASURED_ELEMENTS);
        assert_eq!(receipt.writable_composer_count, 1);
        assert!(!receipt.woke, "a Qt window must never be sent Chromium's wake");
        assert_eq!(receipt.settled_ms, 0);
        assert!(!host.woken.get());
        assert!(
            host.settles.borrow().is_empty(),
            "Telegram needs no settle at all"
        );
        assert_eq!(host.set_values.borrow().as_slice(), [CARRIER.to_owned()]);
        assert_eq!(host.submit_shaped.get(), 0);
    }

    #[test]
    fn telegram_reads_back_by_containment_because_a_live_ui_augments_its_own_fields() {
        let mut host = signed_in();
        host.readback_suffix = " (edited)";

        let receipt = drive_telegram_composer_placement(&host, request(CARRIER));

        assert_eq!(receipt.status, TelegramPlacementStatus::Placed);
        assert!(receipt.readback_contains_carrier);
        assert_ne!(
            host.value.borrow().clone(),
            Some(CARRIER.to_owned()),
            "the fake must not be handing back an exact echo, or contains proves nothing"
        );
    }

    #[test]
    fn telegram_probe_always_clears_the_composer_it_wrote_into() {
        let host = signed_in();
        let receipt = probe_telegram_composer_write_then_clear(&host, request(CARRIER));

        assert_eq!(receipt.status, TelegramPlacementStatus::Placed);
        assert_eq!(
            host.set_values.borrow().as_slice(),
            [CARRIER.to_owned(), String::new()],
            "a probe must never leave text in a real person's chat"
        );
        assert_eq!(*host.value.borrow(), None);
        assert!(!receipt.enter_sent);
        assert_eq!(host.submit_shaped.get(), 0);
    }

    #[test]
    fn telegram_probe_clears_even_when_the_readback_lost_the_carrier() {
        let inner = signed_in();
        let host = RewritesWhatWasPlaced { inner: &inner };

        let receipt = probe_telegram_composer_write_then_clear(&host, request(CARRIER));

        assert_eq!(receipt.status, TelegramPlacementStatus::ReadbackMismatch);
        assert!(!receipt.placed);
        assert_eq!(
            inner.set_values.borrow().as_slice(),
            [CARRIER.to_owned(), String::new()],
            "the write happened, so the clear must happen even though the \
             read-back failed"
        );
    }

    #[test]
    fn telegram_refuses_cleanly_when_no_composer_exists_and_writes_into_nothing_else() {
        // The signed-out client: a writable phone-number field and a writable
        // search box, and no composer. Both are writable ValuePattern elements
        // exactly like the real composer.
        let host = recorded_telegram(vec![writable("Phone number"), writable("Search")]);

        let receipt = drive_telegram_composer_placement(&host, request(CARRIER));

        assert_eq!(receipt.status, TelegramPlacementStatus::ComposerUnavailable);
        assert!(!receipt.placed);
        assert_eq!(receipt.element_count, TELEGRAM_UIA2_MEASURED_ELEMENTS);
        assert!(
            host.set_values.borrow().is_empty(),
            "refusing must mean writing nowhere, not writing somewhere else"
        );
    }

    #[test]
    fn telegram_refuses_search_by_name_before_any_positive_stem_is_considered() {
        for refused in [
            "Search",
            "Search messages",
            "Search for messages or users",
            "Phone number",
            "Username",
            "First name",
        ] {
            assert!(
                !uia2_name_is_composer(TELEGRAM_COMPOSER_MATCHER, refused),
                "{refused:?} must not resolve as Telegram's composer"
            );
        }
        for admitted in [
            TELEGRAM_COMPOSER_MEASURED_NAME,
            "Write a message\u{2026}",
            "Reply to message",
        ] {
            assert!(
                uia2_name_is_composer(TELEGRAM_COMPOSER_MATCHER, admitted),
                "{admitted:?} is Telegram's composer"
            );
        }
    }

    #[test]
    fn telegram_refuses_two_composers_rather_than_guessing_which_chat_it_is_in() {
        let host = recorded_telegram(vec![
            writable(TELEGRAM_COMPOSER_MEASURED_NAME),
            writable("Write a message..."),
        ]);

        let receipt = drive_telegram_composer_placement(&host, request(CARRIER));

        assert_eq!(receipt.status, TelegramPlacementStatus::ComposerAmbiguous);
        assert_eq!(receipt.writable_composer_count, 2);
        assert!(host.set_values.borrow().is_empty());
    }

    #[test]
    fn telegram_refuses_a_carrier_that_carries_the_send() {
        for carrier in ["carrier\nsecond line", "carrier\r", ""] {
            let host = signed_in();
            let receipt = drive_telegram_composer_placement(
                &host,
                TelegramLivePlacementRequest {
                    carrier,
                    allow_replace_existing: false,
                },
            );
            assert_eq!(
                receipt.status,
                TelegramPlacementStatus::InvalidCarrier,
                "{carrier:?} must be refused: every provider commits on Enter"
            );
            assert!(host.set_values.borrow().is_empty());
        }
    }

    #[test]
    fn telegram_refuses_to_overwrite_a_draft_the_owner_typed() {
        let host = signed_in();
        *host.value.borrow_mut() = Some("half-written message to mum".to_owned());

        let receipt = drive_telegram_composer_placement(&host, request(CARRIER));

        assert_eq!(receipt.status, TelegramPlacementStatus::ComposerNotEmpty);
        assert!(host.set_values.borrow().is_empty());

        let host = signed_in();
        *host.value.borrow_mut() = Some("half-written message to mum".to_owned());
        let receipt = drive_telegram_composer_placement(
            &host,
            TelegramLivePlacementRequest {
                carrier: CARRIER,
                allow_replace_existing: true,
            },
        );
        assert_eq!(receipt.status, TelegramPlacementStatus::Placed);
    }

    #[test]
    fn telegram_abandons_a_provider_that_stops_answering_instead_of_freezing_osl() {
        let mut host = signed_in();
        host.never_answers = true;

        let receipt = drive_telegram_composer_placement(&host, request(CARRIER));

        assert_eq!(receipt.status, TelegramPlacementStatus::CallTimedOut);
        assert!(!receipt.placed);
    }

    #[test]
    fn telegram_refuses_the_whole_path_when_the_backend_admits_a_submit_shaped_call() {
        let mut host = signed_in();
        host.submit_shaped_on_set = true;

        let receipt = probe_telegram_composer_write_then_clear(&host, request(CARRIER));

        assert_eq!(
            receipt.status,
            TelegramPlacementStatus::SubmitShapedCallObserved
        );
        assert!(!receipt.placed);
        assert!(
            receipt.enter_sent,
            "enter_sent must be reachable from backend evidence, or the guard cannot bite"
        );
    }

    #[test]
    fn the_deadline_relay_cannot_blind_the_submit_shaped_guard_or_invent_a_budget() {
        let host = signed_in();
        host.submit_shaped.set(3);
        let relay = Uia2DeadlineRelay::new(&host);
        assert_eq!(
            relay.submit_shaped_calls(),
            3,
            "a relay that answered this itself would blind every placement guard"
        );

        let host = signed_in();
        let receipt = drive_telegram_composer_placement(&host, request(CARRIER));
        assert_eq!(receipt.status, TelegramPlacementStatus::Placed);
        let deadlines = host.deadlines.borrow();
        assert!(!deadlines.is_empty());
        assert!(
            deadlines
                .iter()
                .all(|deadline| *deadline == TELEGRAM_UIA2_DEFAULT_CALL_TIMEOUT_MS),
            "every call, including the relayed editable_elements, must carry the \
             plan's own deadline, saw {deadlines:?}"
        );
    }

    #[test]
    fn telegram_does_not_bind_an_electron_window_that_happens_to_be_running() {
        let host = RecordedHost::new(
            vec![
                crate::native_a11y::tests::owned(
                    0x9001,
                    None,
                    None,
                    9100,
                    "Telegram.exe",
                    ELECTRON_OUTER_WINDOW_CLASS,
                    1_920 * 1_040,
                ),
                crate::native_a11y::tests::owned(
                    0x9002,
                    Some(0x9001),
                    None,
                    9100,
                    "Telegram.exe",
                    ELECTRON_RENDERER_WINDOW_CLASS,
                    1_920 * 1_000,
                ),
            ],
            TELEGRAM_UIA2_MEASURED_ELEMENTS,
        )
        .with_editables(vec![writable(TELEGRAM_COMPOSER_MEASURED_NAME)]);

        let receipt = drive_telegram_composer_placement(&host, request(CARRIER));

        assert_eq!(receipt.status, TelegramPlacementStatus::WindowUnavailable);
        assert_eq!(
            receipt.acquire_error,
            Some(Uia2AcquireError::Resolve(
                Uia2WindowResolveError::MissingAppOuter
            )),
            "Telegram is identified by its Qt class, not by its process alone"
        );
        assert!(host.set_values.borrow().is_empty());
    }

    /// Every mechanism that could commit a Telegram message without going
    /// through the syscall seam, spelled as it would appear in Rust source.
    const SUBMIT_SHAPED_MECHANISMS: &[&str] = &[
        "sendinput",
        "keybd_event",
        "keyeventf",
        "input_keyboard",
        "vk_return",
        "vk_enter",
        "postmessage",
        "sendmessage",
        "sendnotifymessage",
        "wm_keydown",
        "wm_keyup",
        "wm_char",
        "wm_ime_char",
        "invoke",
        "\\n\"",
        "\\r\"",
        "\\u{000a}",
        "\\u{000d}",
    ];

    fn production_code(source: &str) -> String {
        source
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default()
            .lines()
            .map(|line| line.split("//").next().unwrap_or_default())
            .collect::<Vec<_>>()
            .join("\n")
            .to_ascii_lowercase()
    }

    #[test]
    fn telegram_placement_module_holds_no_submit_shaped_mechanism() {
        // The scanner must be able to fire, or this guard is decoration.
        assert!(production_code("let _ = element.Invoke(0);").contains("invoke"));
        assert!(production_code("set_value(&format!(\"{carrier}\\n\"))").contains("\\n\""));
        assert!(
            production_code("// element.Invoke(0) described in a comment")
                .trim()
                .is_empty(),
            "comments must be stripped before scanning"
        );

        for (module, source, anchor) in [
            (
                "native_telegram_adapter.rs",
                include_str!("native_telegram_adapter.rs"),
                "fn drive_telegram_composer_placement",
            ),
            (
                "native_a11y.rs",
                include_str!("native_a11y.rs"),
                "fn place_uia2_carrier",
            ),
        ] {
            let code = production_code(source);
            assert!(
                code.contains(anchor),
                "{module}: scanned region lost its production code, so the scan is vacuous"
            );
            for mechanism in SUBMIT_SHAPED_MECHANISMS {
                assert!(
                    !code.contains(mechanism),
                    "{module} contains the submit-shaped mechanism {mechanism:?}. \
                     No OSL mode may silently send: the placement path may only write a \
                     value and read it back."
                );
            }
        }
    }

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> TelegramRect {
        TelegramRect {
            left,
            top,
            right,
            bottom,
        }
    }
    fn body(bounds: TelegramRect, text: &str) -> TelegramNode {
        let mut node = TelegramNode::structural(TelegramRole::Column, bounds);
        node.column_evidence = TelegramColumnEvidence::MessageBody;
        node.text = Some(text.to_owned());
        node
    }

    #[test]
    fn t3_t21_zero_rows_are_incomplete() {
        assert!(!telegram_read_was_complete(&[]));
    }

    #[test]
    fn rows_select_direct_body_columns_without_descending() {
        let mut row = TelegramNode::structural(TelegramRole::ListItem, rect(400, 200, 1100, 330));
        row.children = vec![1, 2];
        let mut non_body =
            TelegramNode::structural(TelegramRole::Column, rect(420, 205, 1080, 225));
        non_body.text = Some("timestamp".into());
        let mut nested = body(rect(440, 240, 1060, 270), "must not be reached");
        nested.children = vec![3];
        let nodes = vec![
            row,
            non_body,
            nested,
            body(rect(440, 245, 1060, 275), "message body"),
        ];
        let selected = extract_telegram_row_candidates(&nodes, 0, 2, 128).unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].node_index, 2);
        assert_eq!(selected[0].text, "must not be reached");
    }

    #[test]
    fn row_paint_target_uses_the_accessible_body_rectangle_without_provider_text() {
        let row = TelegramRowCandidate {
            node_index: 3,
            text: "provider message".into(),
            body_bounds: rect(440, 245, 1060, 275),
        };

        let target = telegram_row_paint_target("carrier-digest".into(), &row).unwrap();
        assert_eq!(target.carrier_sha256, "carrier-digest");
        assert_eq!(
            target.rect,
            Bounds {
                x: 440,
                y: 245,
                width: 620,
                height: 30,
            }
        );
        assert_eq!(target.confidence, PaintConfidence::Exact);
        assert_eq!(
            telegram_row_paint_target(String::new(), &row),
            Err(TelegramSelectorError::Invalid)
        );
    }
}
