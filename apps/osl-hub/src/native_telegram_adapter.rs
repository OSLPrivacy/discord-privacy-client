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
    acquire_uia2_editables, acquire_uia2_window, clear_uia2_composer, place_uia2_carrier,
    resolve_uia2_composer, Uia2AcquireError, Uia2Acquired, Uia2ComposerError, Uia2ComposerMatcher,
    Uia2Editable, Uia2PlacementRefusal, Uia2Syscalls, Uia2WindowPlan, Uia2WindowResolveError,
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
/// Chromium window with Chromium's activation handshake and bridges the MSAA
/// client object into UI Automation.
///
/// **Corrected by D-176.** This paragraph used to say the bridged object is the
/// one Chromium hands back "for object id 1", and that Qt differs from Chromium
/// by answering `OBJID_CLIENT` instead. Both halves are false, measured live on
/// the owner's host: object id 1 is Chromium's screen-reader honeypot and is
/// answered with `LRESULT 0` by design, and Telegram's own Qt window answers
/// exactly as Chromium's windows do -- `0` at object id 1, an object at
/// `OBJID_CLIENT` (`hwnd=327920 -> 0xC0CF`). Chromium and Qt agree on where the
/// object lives; what Chromium adds is the honeypot that switches its lazily
/// built tree on.
///
/// So `MsaaBridge` is not unavailable to Telegram for want of a handshake. It
/// is simply pointless here, because the
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
    /// The composer was resolved and deliberately not written to. Only
    /// `probe_telegram_composer_reachable` returns this.
    ComposerResolved,
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

/// Everything up to, and not including, the write: acquire the Qt window, list
/// its editable elements, and pick the one that is the composer.
///
/// Split out because "can OSL reach Telegram's composer?" is a question worth
/// answering without writing into a real person's chat to find out.
type TelegramResolved = (TelegramLivePlacementReceipt, Uia2Acquired, Uia2Editable);

fn resolve_through_substrate(
    host: &dyn Uia2Syscalls,
) -> Result<TelegramResolved, TelegramLivePlacementReceipt> {
    let acquired = match acquire_uia2_window(TELEGRAM_UIA2_WINDOW_PLAN, host) {
        Ok(acquired) => acquired,
        Err(error) => {
            let mut receipt = TelegramLivePlacementReceipt::refused(acquire_status(error));
            receipt.acquire_error = Some(error);
            return Err(receipt);
        }
    };

    let mut receipt = TelegramLivePlacementReceipt::refused(TelegramPlacementStatus::Placed);
    receipt.element_count = acquired.elements;
    receipt.woke = acquired.woke;
    receipt.settled_ms = acquired.settled_ms;

    // The substrate derives this call's deadline from the plan's own
    // `call_timeout_ms`, the same budget the acquisition above spent.
    let editables = match acquire_uia2_editables(host, acquired) {
        Ok(editables) => editables,
        Err(_) => {
            receipt.status = TelegramPlacementStatus::CallTimedOut;
            return Err(receipt);
        }
    };

    let composer = match resolve_uia2_composer(TELEGRAM_COMPOSER_MATCHER, &editables) {
        Ok(composer) => composer,
        Err(error) => {
            let (status, count) = composer_status(error);
            receipt.status = status;
            receipt.writable_composer_count = count;
            return Err(receipt);
        }
    };
    receipt.writable_composer_count = 1;
    Ok((receipt, acquired, composer))
}

fn place_through_substrate(
    host: &dyn Uia2Syscalls,
    request: TelegramLivePlacementRequest<'_>,
) -> (TelegramLivePlacementReceipt, Option<TelegramPlacedComposer>) {
    if !valid_candidate_text(request.carrier, TELEGRAM_LIVE_CARRIER_MAX_BYTES) {
        return (
            TelegramLivePlacementReceipt::refused(TelegramPlacementStatus::InvalidCarrier),
            None,
        );
    }

    let (mut receipt, acquired, composer) = match resolve_through_substrate(host) {
        Ok(resolved) => resolved,
        Err(receipt) => return (receipt, None),
    };

    match place_uia2_carrier(
        host,
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
            (receipt, Some(TelegramPlacedComposer { acquired, composer }))
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
    place_through_substrate(host, request).0
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
    let (mut receipt, placed) = place_through_substrate(host, request);
    if let Some(placed) = placed {
        let before_clear = host.submit_shaped_calls();
        if clear_uia2_composer(host, placed.acquired, &placed.composer).is_err() {
            receipt.placed = false;
            receipt.status = TelegramPlacementStatus::ProbeClearFailed;
        }
        if host.submit_shaped_calls() > before_clear {
            return TelegramLivePlacementReceipt::refused_after_submit_shaped_call();
        }
    }
    receipt
}

/// Report whether OSL can reach Telegram's composer, writing nothing at all.
///
/// This is the read-only half of the capability question, and it is the run the
/// conductor should do first: it proves the Qt window resolved, how many
/// elements it exposed, and that exactly one editable element is the composer,
/// without touching the owner's chat.
pub fn probe_telegram_composer_reachable(host: &dyn Uia2Syscalls) -> TelegramLivePlacementReceipt {
    match resolve_through_substrate(host) {
        Ok((mut receipt, _, _)) => {
            receipt.status = TelegramPlacementStatus::ComposerResolved;
            receipt
        }
        Err(receipt) => receipt,
    }
}

/// Drive the live Windows host. Defined under both cfgs deliberately: A-01
/// shipped a placement function that existed only under `cfg(not(windows))`,
/// so the platform that matters had no entry point at all and a Linux build
/// could not tell.
#[cfg(target_os = "windows")]
pub fn place_telegram_desktop_carrier(
    request: TelegramLivePlacementRequest<'_>,
) -> TelegramLivePlacementReceipt {
    drive_telegram_composer_placement(
        &crate::native_a11y::win32::Uia2Win32Host::desktop(),
        request,
    )
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

#[cfg(target_os = "windows")]
pub fn telegram_desktop_composer_reachable() -> TelegramLivePlacementReceipt {
    probe_telegram_composer_reachable(&crate::native_a11y::win32::Uia2Win32Host::desktop())
}

#[cfg(not(target_os = "windows"))]
pub fn telegram_desktop_composer_reachable() -> TelegramLivePlacementReceipt {
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
pub(crate) mod tests {
    use super::*;

    // Only the recorded-host fakes below still name the syscall seam's own
    // types; the production half reaches `editable_elements` through
    // `acquire_uia2_editables` and never handles a deadline itself.
    use crate::native_a11y::tests::{composer, telegram_graph, RecordedHost};
    use crate::native_a11y::{
        uia2_name_is_composer, Uia2WakePolicy, Uia2WindowShape, ELECTRON_OUTER_WINDOW_CLASS,
        ELECTRON_RENDERER_WINDOW_CLASS,
    };
    use crate::native_a11y::{Uia2CallTimeout, Uia2Deadline, Uia2OwnedWindow, Uia2TreeRoute};

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
        assert!(
            !receipt.woke,
            "a Qt window must never be sent Chromium's wake"
        );
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
    fn asking_whether_telegram_is_reachable_writes_nothing() {
        let host = signed_in();
        let receipt = probe_telegram_composer_reachable(&host);

        assert_eq!(receipt.status, TelegramPlacementStatus::ComposerResolved);
        assert!(!receipt.placed);
        assert_eq!(receipt.element_count, TELEGRAM_UIA2_MEASURED_ELEMENTS);
        assert_eq!(receipt.writable_composer_count, 1);
        assert!(
            host.set_values.borrow().is_empty(),
            "a capability question must not answer itself by writing"
        );
        assert_eq!(*host.value.borrow(), None);

        let signed_out = recorded_telegram(vec![writable("Phone number")]);
        assert_eq!(
            probe_telegram_composer_reachable(&signed_out).status,
            TelegramPlacementStatus::ComposerUnavailable
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

    /// Was `the_deadline_relay_cannot_blind_the_submit_shaped_guard_or_invent_a_budget`.
    /// D-155 gave the substrate a public `acquire_uia2_editables`, so the relay
    /// that stood between this adapter and the backend is gone; the two things
    /// it was pinned for still have to hold of the direct path.
    #[test]
    fn nothing_stands_between_the_placement_guard_and_the_backends_own_counter() {
        // The submit-shaped counter is read straight off the backend. Anything
        // in the way that answered it itself would blind every placement guard.
        let host = signed_in();
        host.submit_shaped.set(3);
        let receipt = drive_telegram_composer_placement(&host, request(CARRIER));
        assert_eq!(
            receipt.status,
            TelegramPlacementStatus::SubmitShapedCallObserved,
            "a backend already admitting a submit-shaped call must refuse the path"
        );
        assert!(!receipt.placed);

        let host = signed_in();
        let receipt = drive_telegram_composer_placement(&host, request(CARRIER));
        assert_eq!(receipt.status, TelegramPlacementStatus::Placed);
        let deadlines = host.deadlines.borrow();
        assert!(!deadlines.is_empty());
        assert!(
            deadlines
                .iter()
                .all(|deadline| *deadline == TELEGRAM_UIA2_DEFAULT_CALL_TIMEOUT_MS),
            "every call, including the editable scan, must carry the plan's own \
             deadline, saw {deadlines:?}"
        );
    }

    /// The adapter must not have grown its own budget along the way. It reaches
    /// `editable_elements` only through the substrate's door, which derives the
    /// deadline from the plan; a `Uia2Deadline` built in this module would be
    /// the invented budget the token exists to prevent.
    #[test]
    fn the_adapter_never_builds_a_deadline_of_its_own() {
        let production = production_code(include_str!("native_telegram_adapter.rs"));
        assert!(
            production.contains("fn drive_telegram_composer_placement"),
            "the scanned region lost its production code, so the scan is vacuous"
        );
        assert!(
            production.contains("acquire_uia2_editables"),
            "the editable scan must go through the substrate's public door"
        );
        assert!(
            !production.contains("uia2deadline"),
            "this module must not name the deadline token at all: the substrate \
             derives every budget from the plan"
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

    /// Drive the REAL Telegram composer on the owner's Windows host.
    ///
    /// `native_a11y`'s own live probe resolves the composer with that module's
    /// generic test matcher. This one runs the code that ships:
    /// [`TELEGRAM_UIA2_WINDOW_PLAN`], [`TELEGRAM_COMPOSER_MATCHER`] and the
    /// probe entry point, so what the conductor observes is what OSL will do.
    ///
    /// Telegram is the one provider signed in on that host, so this is the only
    /// one of the four whose *real* composer can be reached today.
    ///
    /// ```text
    /// # from WSL, build the Windows test binary:
    /// flock /tmp/osl-cargo.lock cargo test --manifest-path apps/osl-hub/Cargo.toml \
    ///   --lib --target x86_64-pc-windows-gnu -j 4 --no-run
    /// # then, on the Windows host, with Telegram open on a conversation:
    /// set OSL_TELEGRAM_PROBE_CARRIER=a03c-telegram-7731-osl
    /// osl_privacy_hub-<hash>.exe --ignored --test-threads=1 --nocapture \
    ///   native_telegram_adapter::tests::drive_the_real_telegram_composer
    /// ```
    ///
    /// Without `OSL_TELEGRAM_PROBE_CARRIER` this is a read-only probe: it
    /// resolves and reports, and writes nothing. With it, it places, reads back
    /// by containment and clears immediately. Nothing here can commit.
    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "drives the owner's live Telegram client; run explicitly"]
    fn drive_the_real_telegram_composer() {
        let carrier = std::env::var("OSL_TELEGRAM_PROBE_CARRIER").ok();
        let receipt = match carrier.as_deref() {
            Some(carrier) => probe_telegram_desktop_composer(TelegramLivePlacementRequest {
                carrier,
                allow_replace_existing: false,
            }),
            None => telegram_desktop_composer_reachable(),
        };
        eprintln!(
            "telegram: status={:?} placed={} readback_contains_carrier={} \
             elements={} woke={} settled_ms={} writable_composers={} enter_sent={} \
             acquire_error={:?}",
            receipt.status,
            receipt.placed,
            receipt.readback_contains_carrier,
            receipt.element_count,
            receipt.woke,
            receipt.settled_ms,
            receipt.writable_composer_count,
            receipt.enter_sent,
            receipt.acquire_error
        );
        assert!(!receipt.enter_sent, "nothing may be committed, ever");
        assert!(!receipt.woke, "Qt must never be sent Chromium's handshake");
        assert_eq!(receipt.settled_ms, 0, "Qt needs no settle");
        if carrier.is_some() {
            assert_eq!(receipt.status, TelegramPlacementStatus::Placed);
            assert!(receipt.readback_contains_carrier);
            assert!(
                receipt.element_count >= TELEGRAM_UIA2_MEASURED_ELEMENTS / 2,
                "A-00 measured {TELEGRAM_UIA2_MEASURED_ELEMENTS}; reading a \
                 fraction of that is a regression, not a new baseline"
            );
        } else {
            assert_eq!(receipt.status, TelegramPlacementStatus::ComposerResolved);
            assert!(!receipt.placed, "a read-only probe writes nothing");
        }
    }

    /// **The carry, not the placement.** Encode a real payload into real OSL
    /// cover text, put that exact cover text into the owner's live Telegram
    /// composer, read back *what Telegram hands out*, and decode the payload
    /// from that string.
    ///
    /// Why this is a different claim from `drive_the_real_telegram_composer`:
    /// that test asserts `readback_holds_carrier`, which is
    /// `readback.contains(carrier)` inside `place_uia2_carrier`. This one never
    /// looks at the carrier again. It hands the *returned* string to
    /// `decode_mode1` and requires the original bytes back, so the round trip
    /// is judged by the receiving side's own decoder rather than by a
    /// comparison OSL performs against its own input.
    ///
    /// **Nothing is sent.** The composer is cleared before any assertion runs,
    /// so a failed decode still leaves the owner's chat as it was found, and
    /// the backend's submit-shaped counter is read at the end.
    ///
    /// ```text
    /// osl_privacy_hub-<hash>.exe --ignored --test-threads=1 --nocapture \
    ///   native_telegram_adapter::tests::carry_real_cover_text_through_live_telegram
    /// ```
    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "drives the owner's live Telegram client; run explicitly"]
    fn carry_real_cover_text_through_live_telegram() {
        use crate::native_a11y::read_uia2_composer_value;
        use stego::{decode_mode1, encode_mode1, ConversationCipher};

        // A fixed salt and a fixed payload, so the run is reproducible and the
        // conductor can re-derive the expected bytes without this test's help.
        let cipher = ConversationCipher::from_salt(b"osl/telegram-ungate/carry-proof/v1");
        let secret: &[u8] = b"telegram carries osl";
        let cover = encode_mode1(&cipher, secret).expect("mode 1 encodes the payload");

        // Unshaped cover is space-separated: line shaping is Discord's row-count
        // lever, and a newline in a carrier *is* the send on every provider.
        assert!(
            !crate::native_a11y::uia2_carrier_carries_submit(&cover),
            "cover text carrying a line break must never reach a live composer"
        );

        let host = crate::native_a11y::win32::Uia2Win32Host::desktop();
        let acquired = acquire_uia2_window(TELEGRAM_UIA2_WINDOW_PLAN, &host)
            .expect("Telegram's Qt outer window resolves");
        let editables =
            acquire_uia2_editables(&host, acquired).expect("Telegram lists its editable elements");
        let composer = resolve_uia2_composer(TELEGRAM_COMPOSER_MATCHER, &editables)
            .expect("exactly one writable composer; open Telegram on a conversation");

        let elements = acquired.elements;
        let placement = place_uia2_carrier(&host, acquired, &composer, &cover, false);
        // Read before clearing, and clear before asserting: the owner's chat is
        // restored whatever the outcome.
        let readback = read_uia2_composer_value(&host, acquired, &composer);
        let cleared = clear_uia2_composer(&host, acquired, &composer);
        // A post-condition that CAN fail, unlike the submit-shaped counter below:
        // ask the live client what the composer holds *after* the clear. D-206
        // caught `assert_eq!(submit_shaped, 0)` reading a counter the win32 host
        // never increments -- `native_a11y.rs:1995` says so in as many words, so it
        // is structurally zero and could not bite. It is kept only because a future
        // backend that grew a submit-shaped verb would raise it; the assertion that
        // actually guards this run is the one below it.
        let after_clear = read_uia2_composer_value(&host, acquired, &composer);

        placement.expect("cover text places into the live composer");
        cleared.expect("the composer is always cleared");
        assert_eq!(host.submit_shaped_calls(), 0, "nothing may be committed, ever");

        let composer_empty_after_clear = after_clear
            .expect("Telegram answers the read after the clear")
            .is_none_or(|value| value.trim().is_empty());
        assert!(
            composer_empty_after_clear,
            "the live composer still held text after the clear -- a real chat was left dirty"
        );

        let returned = readback
            .expect("Telegram answers the read")
            .expect("the composer holds a value after placement");

        let recovered = decode_mode1(&cipher, returned.trim())
            .expect("the string Telegram handed back still decodes");
        assert_eq!(
            recovered.as_slice(),
            secret,
            "the payload recovered from Telegram's own read-back must be the payload sent"
        );

        // Negative stem, inline: the decoder must actually be reading these
        // words. Drop one and the recovery has to fail, or this test would pass
        // on a decoder that ignored its input.
        let mut words: Vec<&str> = returned.trim().split_whitespace().collect();
        assert!(words.len() > 3, "cover text is a word sequence");
        words.remove(words.len() / 2);
        let starved = words.join(" ");
        assert!(
            !decode_mode1(&cipher, &starved).is_ok_and(|bytes| bytes == secret),
            "removing a word from the read-back must not still recover the payload"
        );

        // D-206: `byte_exact` was the strongest claim in this lane and existed only
        // as prose. It is now asserted here and recorded in the artifact the
        // publication gate reads, which refuses a receipt whose `byte_exact` is
        // false.
        let byte_exact = returned == cover;
        assert!(
            byte_exact,
            "the provider did not hand back exactly what was placed"
        );

        use crate::native_apps::tests::carry_receipt as receipt_io;
        let receipt = receipt_io::LiveCarryReceipt {
            schema: receipt_io::RECEIPT_SCHEMA.to_owned(),
            provider: "telegram".to_owned(),
            seam: "uia2_substrate".to_owned(),
            adapter_source: "src/native_telegram_adapter.rs".to_owned(),
            adapter_source_sha256: receipt_io::source_sha256("src/native_telegram_adapter.rs"),
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
        receipt.write(crate::native_apps::NativeAppId::Telegram);

        eprintln!(
            "telegram-carry: elements={elements} cover_bytes={} readback_bytes={} \
             recovered_bytes={} byte_exact={byte_exact} \
             composer_empty_after_clear={composer_empty_after_clear} enter_sent=false",
            cover.len(),
            returned.len(),
            recovered.len(),
        );
        eprintln!(
            "telegram-carry: receipt written to {}",
            receipt_io::receipt_path(crate::native_apps::NativeAppId::Telegram).display()
        );
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
