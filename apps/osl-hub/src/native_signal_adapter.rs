//! Signal Desktop accessibility selectors and placement contract.
//!
//! The structural selectors are pure. The live-placement contract models one
//! bounded `ValuePattern.SetValue` into an already-bound composer and a read-back
//! proof, but never models Enter, send-button invocation, process launch,
//! credentials, message-history scraping, or network capability. Localized names
//! and placeholder text are modeled only so tests can prove selectors do not
//! depend on them.

use sha2::{Digest, Sha256};

pub use crate::native_a11y::{
    Uia2WakePolicy, Uia2WindowPlan, Uia2WindowShape, ELECTRON_OUTER_WINDOW_CLASS,
    ELECTRON_RENDERER_WINDOW_CLASS, ELECTRON_UIA2_POPULATED_MIN_ELEMENTS,
};

/// Process/window facts for Signal's UIA2 window shape.
///
/// Signal Desktop is an Electron app. The accessibility tree that matters is on
/// the Chromium renderer child, not the outer `Chrome_WidgetWin_1` host.
///
/// This plan is a description, not a binding: `native_a11y` is a taxonomy with
/// no producer yet, so nothing enumerates the windows this plan would be
/// resolved against. `call_timeout_ms` is the exception -- it is read, both by
/// this module's placement budget and by `Uia2ResolvedWindow::bounded_call`.
pub const SIGNAL_DESKTOP_PROCESS_NAME: &str = "Signal";
pub const SIGNAL_UIA2_DEFAULT_WAIT_MS: u64 = 90_000;
pub const SIGNAL_UIA2_DEFAULT_CALL_TIMEOUT_MS: u64 = 750;
pub const SIGNAL_LIVE_CARRIER_MAX_BYTES: usize = 4096;
pub const SIGNAL_UIA2_WINDOW_PLAN: Uia2WindowPlan = Uia2WindowPlan::chromium_renderer_child(
    "Signal",
    SIGNAL_DESKTOP_PROCESS_NAME,
    SIGNAL_UIA2_DEFAULT_WAIT_MS,
    SIGNAL_UIA2_DEFAULT_CALL_TIMEOUT_MS,
);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SignalUia2ProbeConfig {
    Corrected,
    OuterWindow,
    RendererNoWake,
    RendererImmediate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SignalUia2ProbePlan {
    pub window_plan: Uia2WindowPlan,
}

impl SignalUia2ProbeConfig {
    pub const SIDE_BY_SIDE: [Self; 4] = [
        Self::Corrected,
        Self::OuterWindow,
        Self::RendererNoWake,
        Self::RendererImmediate,
    ];

    pub fn plan(self) -> SignalUia2ProbePlan {
        let window_plan = match self {
            Self::Corrected => SIGNAL_UIA2_WINDOW_PLAN,
            Self::OuterWindow => Uia2WindowPlan::chromium_outer_mutant(
                "Signal",
                SIGNAL_DESKTOP_PROCESS_NAME,
                SIGNAL_UIA2_DEFAULT_WAIT_MS,
                SIGNAL_UIA2_DEFAULT_CALL_TIMEOUT_MS,
            ),
            Self::RendererNoWake => Uia2WindowPlan::chromium_renderer_no_wake_mutant(
                "Signal",
                SIGNAL_DESKTOP_PROCESS_NAME,
                SIGNAL_UIA2_DEFAULT_WAIT_MS,
                SIGNAL_UIA2_DEFAULT_CALL_TIMEOUT_MS,
            ),
            Self::RendererImmediate => Uia2WindowPlan::chromium_renderer_immediate_mutant(
                "Signal",
                SIGNAL_DESKTOP_PROCESS_NAME,
                SIGNAL_UIA2_DEFAULT_CALL_TIMEOUT_MS,
            ),
        };
        SignalUia2ProbePlan { window_plan }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SignalPlacementStatus {
    Placed,
    PlatformUnsupported,
    InvalidCarrier,
    AccessibilityUnavailable,
    ComposerUnavailable,
    ComposerAmbiguous,
    ComposerNotWritable,
    ComposerNotEmpty,
    ReadbackMismatch,
    ProbeClearFailed,
    /// A backend call overran the plan's `call_timeout_ms`. A provider that is
    /// answering that slowly is the state that precedes a cross-process freeze,
    /// so the path is abandoned rather than driven further.
    CallTimedOut,
    /// The backend reported a submit-shaped interaction. Placement is abandoned:
    /// OSL never authorizes a send, so a backend that performed one is refused
    /// rather than trusted for the rest of the path.
    SubmitShapedCallObserved,
}

pub struct SignalLivePlacementRequest<'a> {
    pub carrier: &'a str,
    pub allow_replace_existing: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignalLivePlacementReceipt {
    pub placed: bool,
    pub enter_sent: bool,
    pub status: SignalPlacementStatus,
    pub element_count: usize,
    pub writable_composer_count: usize,
    pub readback_contains_carrier: bool,
}

impl SignalLivePlacementReceipt {
    fn refused(status: SignalPlacementStatus) -> Self {
        Self {
            placed: false,
            enter_sent: false,
            status,
            element_count: 0,
            writable_composer_count: 0,
            readback_contains_carrier: false,
        }
    }

    /// The backend admitted a submit-shaped interaction. `enter_sent` is this
    /// report, not a constant: it is the only place in this module that can set
    /// the field true, and it is set from backend evidence.
    fn refused_after_submit_shaped_call() -> Self {
        Self {
            enter_sent: true,
            ..Self::refused(SignalPlacementStatus::SubmitShapedCallObserved)
        }
    }
}

pub trait SignalComposerPlacementBackend {
    fn element_count(&mut self) -> Result<usize, SignalPlacementStatus>;
    fn writable_composer_count(&mut self) -> Result<usize, SignalPlacementStatus>;
    fn current_value(&mut self) -> Result<Option<String>, SignalPlacementStatus>;
    fn set_value(&mut self, carrier: &str) -> Result<(), SignalPlacementStatus>;
    fn read_value(&mut self) -> Result<Option<String>, SignalPlacementStatus>;

    /// How many submit-shaped interactions this backend has performed since it
    /// was bound.
    ///
    /// Submit-shaped means anything that could commit the composer: invoking a
    /// send control, synthesising a key or input event, posting a keyboard
    /// message to the composer window, or writing a value the composer treats
    /// as a commit (one carrying CR or LF). A conforming backend performs none
    /// of these and returns 0. This is a required method precisely so that a
    /// new backend cannot inherit a silent "no" it never checked, and so that
    /// `enter_sent` is derived from what the backend reports rather than from a
    /// literal this module writes about itself.
    fn submit_shaped_calls(&mut self) -> usize;
}

pub trait SignalComposerProbeBackend: SignalComposerPlacementBackend {
    fn clear_value(&mut self) -> Result<(), SignalPlacementStatus>;
}

/// Place text into the already-bound Signal composer and verify by containment.
///
/// This is deliberately not a send path. The only mutation it can authorize is
/// a value-pattern placement followed by read-back.
///
/// The prohibition is enforced by three separate mechanisms, because the field
/// alone proved nothing:
/// 1. the carrier is refused if it carries CR or LF, so a commit cannot ride
///    inside the placed value;
/// 2. the backend is asked, before and after the placement, how many
///    submit-shaped interactions it performed, and any non-zero delta abandons
///    placement with [`SignalPlacementStatus::SubmitShapedCallObserved`] and
///    `enter_sent = true`; and
/// 3. `signal_placement_module_holds_no_submit_shaped_mechanism` scans this
///    module's own source for submit-shaped mechanisms that would bypass the
///    backend entirely (`SendInput`, `keybd_event`, `VK_RETURN`, a posted
///    keyboard message, an invoke pattern, or a newline-carrying literal).
/// Time one backend call against the plan's `call_timeout_ms` and abandon the
/// path if it overran.
///
/// This bounds how long OSL is willing to keep driving a slow provider. It is
/// not the hang guard: a call that never returns is bounded by
/// [`crate::native_a11y::call_with_timeout`], which the Windows implementor of
/// [`SignalComposerPlacementBackend`] must issue its cross-process calls
/// through. Both read the same `call_timeout_ms`.
macro_rules! bounded_backend_call {
    ($call:expr) => {{
        let started = std::time::Instant::now();
        let value = $call;
        if started.elapsed().as_millis() as u64 > SIGNAL_UIA2_WINDOW_PLAN.call_timeout_ms {
            return SignalLivePlacementReceipt::refused(SignalPlacementStatus::CallTimedOut);
        }
        value
    }};
}

pub fn drive_signal_composer_placement(
    backend: &mut dyn SignalComposerPlacementBackend,
    request: SignalLivePlacementRequest<'_>,
) -> SignalLivePlacementReceipt {
    if !valid_candidate_text(request.carrier, SIGNAL_LIVE_CARRIER_MAX_BYTES)
        || carrier_carries_submit(request.carrier)
    {
        return SignalLivePlacementReceipt::refused(SignalPlacementStatus::InvalidCarrier);
    }

    let submit_shaped_baseline = bounded_backend_call!(backend.submit_shaped_calls());
    if submit_shaped_baseline > 0 {
        return SignalLivePlacementReceipt::refused_after_submit_shaped_call();
    }

    let element_count = match bounded_backend_call!(backend.element_count()) {
        Ok(count) if count > ELECTRON_UIA2_POPULATED_MIN_ELEMENTS => count,
        Ok(_) => {
            return SignalLivePlacementReceipt::refused(
                SignalPlacementStatus::AccessibilityUnavailable,
            )
        }
        Err(status) => return SignalLivePlacementReceipt::refused(status),
    };

    let writable_composer_count = match bounded_backend_call!(backend.writable_composer_count()) {
        Ok(1) => 1,
        Ok(0) => {
            let mut receipt =
                SignalLivePlacementReceipt::refused(SignalPlacementStatus::ComposerUnavailable);
            receipt.element_count = element_count;
            return receipt;
        }
        Ok(count) => {
            let mut receipt =
                SignalLivePlacementReceipt::refused(SignalPlacementStatus::ComposerAmbiguous);
            receipt.element_count = element_count;
            receipt.writable_composer_count = count;
            return receipt;
        }
        Err(status) => {
            let mut receipt = SignalLivePlacementReceipt::refused(status);
            receipt.element_count = element_count;
            return receipt;
        }
    };

    if !request.allow_replace_existing {
        match bounded_backend_call!(backend.current_value()) {
            Ok(Some(value)) if !value.is_empty() => {
                let mut receipt =
                    SignalLivePlacementReceipt::refused(SignalPlacementStatus::ComposerNotEmpty);
                receipt.element_count = element_count;
                receipt.writable_composer_count = writable_composer_count;
                return receipt;
            }
            Ok(_) => {}
            Err(status) => {
                let mut receipt = SignalLivePlacementReceipt::refused(status);
                receipt.element_count = element_count;
                receipt.writable_composer_count = writable_composer_count;
                return receipt;
            }
        }
    }

    if let Err(status) = bounded_backend_call!(backend.set_value(request.carrier)) {
        let mut receipt = SignalLivePlacementReceipt::refused(status);
        receipt.element_count = element_count;
        receipt.writable_composer_count = writable_composer_count;
        return receipt;
    }

    let readback_contains_carrier = bounded_backend_call!(backend.read_value())
        .ok()
        .flatten()
        .is_some_and(|readback| readback.contains(request.carrier));
    let enter_sent = bounded_backend_call!(backend.submit_shaped_calls()) > submit_shaped_baseline;
    if enter_sent {
        return SignalLivePlacementReceipt::refused_after_submit_shaped_call();
    }
    SignalLivePlacementReceipt {
        placed: readback_contains_carrier,
        enter_sent,
        status: if readback_contains_carrier {
            SignalPlacementStatus::Placed
        } else {
            SignalPlacementStatus::ReadbackMismatch
        },
        element_count,
        writable_composer_count,
        readback_contains_carrier,
    }
}

/// Probe Signal placement into an already-resolved composer, then clear it.
///
/// This is for live capability checks only. It uses the same contains read-back
/// as production placement, carries the same submit-shaped-call prohibition,
/// and always clears after a successful `SetValue` attempt so a real chat is not
/// left with probe text. The clear is itself covered: the backend is re-asked
/// afterwards, so a clear that commits the composer is caught.
pub fn probe_signal_composer_write_then_clear(
    backend: &mut dyn SignalComposerProbeBackend,
    request: SignalLivePlacementRequest<'_>,
) -> SignalLivePlacementReceipt {
    let mut receipt = drive_signal_composer_placement(backend, request);
    if matches!(
        receipt.status,
        SignalPlacementStatus::Placed | SignalPlacementStatus::ReadbackMismatch
    ) {
        let submit_shaped_before_clear = bounded_backend_call!(backend.submit_shaped_calls());
        if bounded_backend_call!(backend.clear_value()).is_err() {
            receipt.placed = false;
            receipt.status = SignalPlacementStatus::ProbeClearFailed;
        }
        if bounded_backend_call!(backend.submit_shaped_calls()) > submit_shaped_before_clear {
            return SignalLivePlacementReceipt::refused_after_submit_shaped_call();
        }
    }
    receipt
}

#[cfg(not(target_os = "windows"))]
pub fn place_signal_desktop_carrier(
    _request: SignalLivePlacementRequest<'_>,
) -> SignalLivePlacementReceipt {
    SignalLivePlacementReceipt::refused(SignalPlacementStatus::PlatformUnsupported)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SignalRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl SignalRect {
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
pub enum SignalRole {
    Window,
    Pane,
    List,
    Article,
    Row,
    Text,
    EditableText,
    Button,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SignalNodeEvidence {
    None,
    Body,
}

#[derive(Clone)]
pub struct SignalNode {
    pub role: SignalRole,
    pub evidence: SignalNodeEvidence,
    pub bounds: SignalRect,
    pub visible: bool,
    pub enabled: bool,
    pub focusable: bool,
    pub editable: bool,
    pub read_only: bool,
    pub localized_name: Option<String>,
    pub text: Option<String>,
    pub children: Vec<usize>,
}

impl SignalNode {
    pub fn structural(role: SignalRole, bounds: SignalRect) -> Self {
        Self {
            role,
            evidence: SignalNodeEvidence::None,
            bounds,
            visible: true,
            enabled: true,
            focusable: false,
            editable: false,
            read_only: true,
            localized_name: None,
            text: None,
            children: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SignalSelectorError {
    Missing,
    Ambiguous,
    Invalid,
    LimitExceeded,
    ProofMismatch,
}

#[derive(Clone, Eq, PartialEq)]
pub struct SignalRowCandidate {
    pub node_index: usize,
    pub text: String,
    pub body_bounds: SignalRect,
}

pub struct SignalCarrierPlacementRequest<'a> {
    pub composer_anchor_sha256: &'a str,
    pub carrier: &'a str,
    pub exact_prefix: &'a str,
    pub prefix_proof_sha256: &'a str,
}

#[derive(Clone, Eq, PartialEq)]
pub struct SignalCarrierPlacement {
    pub committed_text: String,
    pub exact_prefix: String,
    pub prefix_proof_sha256: String,
}

#[derive(Clone, Eq, PartialEq)]
pub struct SignalPaintGeometry {
    pub paint_bounds: SignalRect,
    pub authenticated_node_indices: Vec<usize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SignalComposerResolutionMethod {
    AccessibleNameAndRole,
    UnnamedGeometryFallback,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SignalComposerResolution {
    pub node_index: usize,
    pub method: SignalComposerResolutionMethod,
}

pub fn discover_signal_composer(
    nodes: &[SignalNode],
    window_bounds: SignalRect,
) -> Result<usize, SignalSelectorError> {
    resolve_signal_composer(nodes, window_bounds).map(|resolution| resolution.node_index)
}

/// Resolve Signal's composer from role and accessible name first.
///
/// Signal link or login screens can expose writable fields such as search or
/// phone-number entry. Those are valid `Edit` controls but not composers, so a
/// named writable field with no composer-like name is refused. The only fallback
/// is geometric, and only for unnamed writable edit controls in the conversation
/// pane; that keeps localized or placeholder-free composers usable without
/// placing text into an unrelated named field.
pub fn resolve_signal_composer(
    nodes: &[SignalNode],
    window_bounds: SignalRect,
) -> Result<SignalComposerResolution, SignalSelectorError> {
    let named_matches = nodes
        .iter()
        .enumerate()
        .filter_map(|(index, node)| {
            signal_named_composer_candidate(node, window_bounds).then_some(index)
        })
        .collect::<Vec<_>>();
    match named_matches.as_slice() {
        [index] => {
            return Ok(SignalComposerResolution {
                node_index: *index,
                method: SignalComposerResolutionMethod::AccessibleNameAndRole,
            })
        }
        [] => {}
        _ => return Err(SignalSelectorError::Ambiguous),
    }

    let fallback_matches = nodes
        .iter()
        .enumerate()
        .filter_map(|(index, node)| {
            signal_unnamed_geometry_composer_candidate(node, window_bounds).then_some(index)
        })
        .collect::<Vec<_>>();
    match fallback_matches.as_slice() {
        [index] => Ok(SignalComposerResolution {
            node_index: *index,
            method: SignalComposerResolutionMethod::UnnamedGeometryFallback,
        }),
        [] => Err(SignalSelectorError::Missing),
        _ => Err(SignalSelectorError::Ambiguous),
    }
}

pub fn discover_signal_transcript(
    nodes: &[SignalNode],
    composer_index: usize,
    window_bounds: SignalRect,
) -> Result<usize, SignalSelectorError> {
    let Some(composer) = nodes.get(composer_index) else {
        return Err(SignalSelectorError::Missing);
    };
    if !signal_composer_candidate(composer, window_bounds) {
        return Err(SignalSelectorError::Invalid);
    }
    let matches = nodes
        .iter()
        .enumerate()
        .filter_map(|(index, node)| {
            signal_transcript_candidate(node, composer, window_bounds).then_some(index)
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [index] => Ok(*index),
        [] => Err(SignalSelectorError::Missing),
        _ => Err(SignalSelectorError::Ambiguous),
    }
}

pub fn extract_signal_row_candidates(
    nodes: &[SignalNode],
    row_index: usize,
    max_candidates: usize,
    max_text_bytes: usize,
) -> Result<Vec<SignalRowCandidate>, SignalSelectorError> {
    let Some(row) = nodes.get(row_index) else {
        return Err(SignalSelectorError::Missing);
    };
    if !row.visible || !row.bounds.valid() || max_candidates == 0 || max_text_bytes == 0 {
        return Err(SignalSelectorError::Invalid);
    }

    let mut candidates = Vec::new();
    for index in descendants(nodes, row_index)? {
        let node = &nodes[index];
        if node.evidence != SignalNodeEvidence::Body {
            continue;
        }
        let Some(text) = node.text.as_ref() else {
            return Err(SignalSelectorError::Invalid);
        };
        if node.role != SignalRole::Text
            || !node.visible
            || !node.bounds.contained_by(row.bounds)
            || !valid_candidate_text(text, max_text_bytes)
        {
            return Err(SignalSelectorError::Invalid);
        }
        if candidates.len() >= max_candidates {
            return Err(SignalSelectorError::LimitExceeded);
        }
        candidates.push(SignalRowCandidate {
            node_index: index,
            text: text.clone(),
            body_bounds: node.bounds,
        });
    }
    Ok(candidates)
}

pub fn place_signal_carrier(
    request: SignalCarrierPlacementRequest<'_>,
) -> Result<SignalCarrierPlacement, SignalSelectorError> {
    if !canonical_sha256(request.composer_anchor_sha256)
        || !canonical_sha256(request.prefix_proof_sha256)
        || request.carrier.is_empty()
        || request.exact_prefix.is_empty()
        || !valid_candidate_text(request.carrier, 4096)
        || !valid_candidate_text(request.exact_prefix, 4096)
        || !request.carrier.starts_with(request.exact_prefix)
    {
        return Err(SignalSelectorError::Invalid);
    }
    let expected_proof = signal_carrier_prefix_proof_sha256(
        request.composer_anchor_sha256,
        request.exact_prefix,
        request.carrier,
    );
    if expected_proof != request.prefix_proof_sha256 {
        return Err(SignalSelectorError::ProofMismatch);
    }
    Ok(SignalCarrierPlacement {
        committed_text: request.carrier.to_owned(),
        exact_prefix: request.exact_prefix.to_owned(),
        prefix_proof_sha256: expected_proof,
    })
}

pub fn signal_carrier_prefix_proof_sha256(
    composer_anchor_sha256: &str,
    exact_prefix: &str,
    carrier: &str,
) -> String {
    hash_joined(
        "signal-carrier-prefix-proof-v1",
        [
            composer_anchor_sha256,
            &exact_prefix.len().to_string(),
            exact_prefix,
            &hash_joined("signal-carrier-full-text-v1", [carrier]),
        ],
    )
}

pub fn signal_paint_geometry(
    row_bounds: SignalRect,
    candidates: &[SignalRowCandidate],
    authenticated_node_indices: &[usize],
) -> Result<SignalPaintGeometry, SignalSelectorError> {
    if !row_bounds.valid() || authenticated_node_indices.is_empty() {
        return Err(SignalSelectorError::Missing);
    }
    let mut paint_bounds: Option<SignalRect> = None;
    let mut accepted = Vec::new();
    for node_index in authenticated_node_indices {
        if accepted.contains(node_index) {
            return Err(SignalSelectorError::Invalid);
        }
        let Some(candidate) = candidates
            .iter()
            .find(|candidate| candidate.node_index == *node_index)
        else {
            return Err(SignalSelectorError::Invalid);
        };
        if !candidate.body_bounds.contained_by(row_bounds) {
            return Err(SignalSelectorError::Invalid);
        }
        paint_bounds = Some(match paint_bounds {
            Some(bounds) => bounds.union(candidate.body_bounds),
            None => candidate.body_bounds,
        });
        accepted.push(*node_index);
    }
    let Some(paint_bounds) = paint_bounds else {
        return Err(SignalSelectorError::Missing);
    };
    Ok(SignalPaintGeometry {
        paint_bounds,
        authenticated_node_indices: accepted,
    })
}

fn signal_composer_candidate(node: &SignalNode, window_bounds: SignalRect) -> bool {
    signal_named_composer_candidate(node, window_bounds)
        || signal_unnamed_geometry_composer_candidate(node, window_bounds)
}

fn signal_named_composer_candidate(node: &SignalNode, window_bounds: SignalRect) -> bool {
    writable_signal_edit(node, window_bounds)
        && node
            .localized_name
            .as_deref()
            .is_some_and(signal_composer_accessible_name)
}

fn signal_unnamed_geometry_composer_candidate(
    node: &SignalNode,
    window_bounds: SignalRect,
) -> bool {
    let right_pane_left = window_bounds.left.saturating_add(window_bounds.width() / 3);
    let lower_band_top = window_bounds
        .top
        .saturating_add(window_bounds.height() * 3 / 5);
    writable_signal_edit(node, window_bounds)
        && node.localized_name.as_deref().is_none_or(str::is_empty)
        && node.bounds.contained_by(window_bounds)
        && node.bounds.left >= right_pane_left
        && node.bounds.top >= lower_band_top
}

fn writable_signal_edit(node: &SignalNode, window_bounds: SignalRect) -> bool {
    node.role == SignalRole::EditableText
        && node.visible
        && node.enabled
        && node.focusable
        && node.editable
        && !node.read_only
        && node.bounds.contained_by(window_bounds)
}

/// Name stems that prove a writable `Edit` is NOT the composer.
///
/// Signal's search, filter and message-request fields all *contain* a composer
/// stem in their own locale -- "Search messages", "Nachrichten durchsuchen",
/// "Buscar mensajes", "Message requests" -- so a positive stem alone cannot
/// decide this. When such a field is the only writable Edit present (the
/// not-signed-in / no-conversation-open state) a substring-only matcher would
/// place the carrier into it.
const SIGNAL_NON_COMPOSER_NAME_STEMS: &[&str] = &[
    "search",
    "find",
    "filter",
    "suchen",
    "suche",
    "filtern",
    "buscar",
    "busca",
    "b\u{fa}squeda",
    "busqueda",
    "filtrar",
    "filtro",
    "request",
    "anfrage",
    "solicitud",
];

const SIGNAL_COMPOSER_NAME_STEMS: &[&str] = &["message", "nachricht", "mensaje"];

fn signal_composer_accessible_name(name: &str) -> bool {
    let normalized = name
        .trim()
        .trim_end_matches('.')
        .to_lowercase()
        .replace('\u{2026}', "");
    if SIGNAL_NON_COMPOSER_NAME_STEMS
        .iter()
        .any(|stem| normalized.contains(stem))
    {
        return false;
    }
    SIGNAL_COMPOSER_NAME_STEMS
        .iter()
        .any(|stem| normalized.contains(stem))
}

fn signal_transcript_candidate(
    node: &SignalNode,
    composer: &SignalNode,
    window_bounds: SignalRect,
) -> bool {
    let required_overlap = composer.bounds.width().saturating_mul(2) / 3;
    node.role == SignalRole::List
        && node.visible
        && node.bounds.contained_by(window_bounds)
        && node.bounds.bottom <= composer.bounds.top
        && node.bounds.height() >= window_bounds.height() / 4
        && node.bounds.horizontal_overlap(composer.bounds) >= required_overlap
}

impl SignalRect {
    fn union(self, other: Self) -> Self {
        Self {
            left: self.left.min(other.left),
            top: self.top.min(other.top),
            right: self.right.max(other.right),
            bottom: self.bottom.max(other.bottom),
        }
    }
}

fn descendants(nodes: &[SignalNode], root: usize) -> Result<Vec<usize>, SignalSelectorError> {
    let Some(root_node) = nodes.get(root) else {
        return Err(SignalSelectorError::Missing);
    };
    let mut result = Vec::new();
    let mut queue = std::collections::VecDeque::from(root_node.children.clone());
    while let Some(index) = queue.pop_front() {
        let Some(node) = nodes.get(index) else {
            return Err(SignalSelectorError::Invalid);
        };
        if result.len() >= nodes.len() {
            return Err(SignalSelectorError::Invalid);
        }
        result.push(index);
        queue.extend(node.children.iter().copied());
    }
    Ok(result)
}

/// A live-placement carrier that carries CR or LF is a submit, not a value.
///
/// `valid_candidate_text` deliberately tolerates CR/LF for transcript row text,
/// which is read, never written. Nothing written into a live composer may carry
/// them: Signal commits the message on Enter, so the newline *is* the send.
fn carrier_carries_submit(carrier: &str) -> bool {
    carrier
        .chars()
        .any(|character| matches!(character, '\n' | '\r' | '\u{000b}' | '\u{2028}' | '\u{2029}'))
}

fn valid_candidate_text(value: &str, max_text_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_text_bytes
        && !value
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
}

fn canonical_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn hash_joined<'a>(domain: &str, values: impl IntoIterator<Item = &'a str>) -> String {
    let mut hash = Sha256::new();
    hash.update(domain.as_bytes());
    for value in values {
        hash.update([0x1f]);
        hash.update(value.as_bytes());
    }
    hex_lower(&hash.finalize())
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[usize::from(byte >> 4)] as char);
        encoded.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeSignalBackend {
        elements: Result<usize, SignalPlacementStatus>,
        writable: Result<usize, SignalPlacementStatus>,
        current: Result<Option<String>, SignalPlacementStatus>,
        readback_suffix: &'static str,
        set_values: Vec<String>,
        cleared: usize,
        clear_result: Result<(), SignalPlacementStatus>,
        /// Submit-shaped interactions this fake has performed. It models the
        /// real composer: a written value carrying CR/LF is a commit.
        submit_shaped: usize,
        /// Models a backend whose write path also invokes the send control or
        /// synthesises a key — the exact thing the prohibition forbids.
        submits_after_set_value: bool,
        submits_after_clear: bool,
        /// Milliseconds every backend call takes to answer.
        call_delay_ms: u64,
    }

    impl FakeSignalBackend {
        fn empty() -> Self {
            Self {
                elements: Ok(696),
                writable: Ok(1),
                current: Ok(Some(String::new())),
                readback_suffix: "",
                set_values: Vec::new(),
                cleared: 0,
                clear_result: Ok(()),
                submit_shaped: 0,
                submits_after_set_value: false,
                submits_after_clear: false,
                call_delay_ms: 0,
            }
        }
    }

    impl FakeSignalBackend {
        fn answer(&self) {
            if self.call_delay_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(self.call_delay_ms));
            }
        }
    }

    impl SignalComposerPlacementBackend for FakeSignalBackend {
        fn element_count(&mut self) -> Result<usize, SignalPlacementStatus> {
            self.answer();
            self.elements.clone()
        }

        fn writable_composer_count(&mut self) -> Result<usize, SignalPlacementStatus> {
            self.writable.clone()
        }

        fn current_value(&mut self) -> Result<Option<String>, SignalPlacementStatus> {
            self.current.clone()
        }

        fn set_value(&mut self, carrier: &str) -> Result<(), SignalPlacementStatus> {
            self.set_values.push(carrier.to_owned());
            if carrier.contains('\n') || carrier.contains('\r') {
                self.submit_shaped += 1;
            }
            if self.submits_after_set_value {
                self.submit_shaped += 1;
            }
            Ok(())
        }

        fn read_value(&mut self) -> Result<Option<String>, SignalPlacementStatus> {
            Ok(self
                .set_values
                .last()
                .map(|value| format!("{value}{}", self.readback_suffix)))
        }

        fn submit_shaped_calls(&mut self) -> usize {
            self.answer();
            self.submit_shaped
        }
    }

    impl SignalComposerProbeBackend for FakeSignalBackend {
        fn clear_value(&mut self) -> Result<(), SignalPlacementStatus> {
            self.cleared += 1;
            if self.submits_after_clear {
                self.submit_shaped += 1;
            }
            self.clear_result
        }
    }

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> SignalRect {
        SignalRect {
            left,
            top,
            right,
            bottom,
        }
    }

    fn editable(bounds: SignalRect, localized_name: &str) -> SignalNode {
        let mut node = SignalNode::structural(SignalRole::EditableText, bounds);
        node.focusable = true;
        node.editable = true;
        node.read_only = false;
        node.localized_name = Some(localized_name.to_owned());
        node
    }

    fn list(bounds: SignalRect, localized_name: &str) -> SignalNode {
        let mut node = SignalNode::structural(SignalRole::List, bounds);
        node.localized_name = Some(localized_name.to_owned());
        node
    }

    fn body_text(bounds: SignalRect, text: &str) -> SignalNode {
        let mut node = SignalNode::structural(SignalRole::Text, bounds);
        node.evidence = SignalNodeEvidence::Body;
        node.text = Some(text.to_owned());
        node
    }

    #[test]
    fn signal_uia2_probe_configs_name_the_three_required_mutants() {
        let corrected = SignalUia2ProbeConfig::Corrected.plan();
        assert_eq!(
            corrected.window_plan.shape,
            Uia2WindowShape::ChromiumRendererChild
        );
        assert_eq!(
            corrected.window_plan.wake_policy,
            Uia2WakePolicy::WmGetObjectChromium
        );
        assert!(corrected.window_plan.poll_until_populated);
        assert_eq!(
            corrected.window_plan.renderer_child_class,
            Some(ELECTRON_RENDERER_WINDOW_CLASS)
        );

        let outer = SignalUia2ProbeConfig::OuterWindow.plan();
        assert_eq!(outer.window_plan.shape, Uia2WindowShape::DirectOuterWindow);
        assert_eq!(
            outer.window_plan.wake_policy,
            Uia2WakePolicy::WmGetObjectChromium
        );
        assert!(outer.window_plan.poll_until_populated);

        let no_wake = SignalUia2ProbeConfig::RendererNoWake.plan();
        assert_eq!(
            no_wake.window_plan.shape,
            Uia2WindowShape::ChromiumRendererChild
        );
        assert_eq!(no_wake.window_plan.wake_policy, Uia2WakePolicy::None);
        assert!(no_wake.window_plan.poll_until_populated);

        let immediate = SignalUia2ProbeConfig::RendererImmediate.plan();
        assert_eq!(
            immediate.window_plan.shape,
            Uia2WindowShape::ChromiumRendererChild
        );
        assert_eq!(
            immediate.window_plan.wake_policy,
            Uia2WakePolicy::WmGetObjectChromium
        );
        assert!(!immediate.window_plan.poll_until_populated);
    }

    #[test]
    fn signal_live_placement_uses_contains_readback_and_never_sends() {
        let mut backend = FakeSignalBackend::empty();
        backend.readback_suffix = " augmented by live UI";

        let receipt = drive_signal_composer_placement(
            &mut backend,
            SignalLivePlacementRequest {
                carrier: "alpha-7731-osl",
                allow_replace_existing: false,
            },
        );

        assert_eq!(backend.set_values, vec!["alpha-7731-osl"]);
        assert_eq!(receipt.status, SignalPlacementStatus::Placed);
        assert!(receipt.placed);
        assert!(receipt.readback_contains_carrier);
        assert!(!receipt.enter_sent);
        assert_eq!(
            backend.submit_shaped, 0,
            "the backend must observe no submit-shaped interaction on the placement path"
        );
    }

    #[test]
    fn signal_live_placement_refuses_and_reports_a_backend_that_submits() {
        let mut backend = FakeSignalBackend::empty();
        backend.submits_after_set_value = true;

        let receipt = drive_signal_composer_placement(
            &mut backend,
            SignalLivePlacementRequest {
                carrier: "echo-4471-osl",
                allow_replace_existing: false,
            },
        );

        assert_eq!(
            receipt.status,
            SignalPlacementStatus::SubmitShapedCallObserved
        );
        assert!(
            receipt.enter_sent,
            "enter_sent must be derived from backend evidence, not written as a literal"
        );
        assert!(!receipt.placed);
    }

    #[test]
    fn signal_live_placement_refuses_a_backend_that_had_already_submitted() {
        let mut backend = FakeSignalBackend::empty();
        backend.submit_shaped = 1;

        let receipt = drive_signal_composer_placement(
            &mut backend,
            SignalLivePlacementRequest {
                carrier: "foxtrot-8820-osl",
                allow_replace_existing: false,
            },
        );

        assert_eq!(
            receipt.status,
            SignalPlacementStatus::SubmitShapedCallObserved
        );
        assert!(receipt.enter_sent);
        assert!(
            backend.set_values.is_empty(),
            "a backend that already submitted must not be written to at all"
        );
    }

    #[test]
    fn signal_live_probe_refuses_a_clear_that_commits_the_composer() {
        let mut backend = FakeSignalBackend::empty();
        backend.submits_after_clear = true;

        let receipt = probe_signal_composer_write_then_clear(
            &mut backend,
            SignalLivePlacementRequest {
                carrier: "golf-3312-osl",
                allow_replace_existing: false,
            },
        );

        assert_eq!(
            receipt.status,
            SignalPlacementStatus::SubmitShapedCallObserved
        );
        assert!(receipt.enter_sent);
        assert!(!receipt.placed);
    }

    #[test]
    fn signal_live_placement_abandons_a_backend_call_that_overruns_its_deadline() {
        let mut backend = FakeSignalBackend::empty();
        backend.call_delay_ms = SIGNAL_UIA2_WINDOW_PLAN.call_timeout_ms + 150;

        let started = std::time::Instant::now();
        let receipt = drive_signal_composer_placement(
            &mut backend,
            SignalLivePlacementRequest {
                carrier: "india-2204-osl",
                allow_replace_existing: false,
            },
        );

        assert_eq!(receipt.status, SignalPlacementStatus::CallTimedOut);
        assert!(!receipt.placed);
        assert!(
            backend.set_values.is_empty(),
            "a provider answering past its deadline must not be written to"
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(3),
            "the path must abandon at the deadline, not accumulate every slow call"
        );
    }

    #[test]
    fn signal_live_placement_refuses_a_carrier_that_carries_a_commit() {
        for carrier in [
            "hotel-1180-osl\n",
            "hotel-1180-osl\r\n",
            "hotel\n1180-osl",
            "hotel-1180-osl\u{2028}",
        ] {
            let mut backend = FakeSignalBackend::empty();
            let receipt = drive_signal_composer_placement(
                &mut backend,
                SignalLivePlacementRequest {
                    carrier,
                    allow_replace_existing: false,
                },
            );

            assert_eq!(
                receipt.status,
                SignalPlacementStatus::InvalidCarrier,
                "a carrier carrying {carrier:?} is a send, not a placement"
            );
            assert!(!receipt.placed);
            assert!(backend.set_values.is_empty());
        }
    }

    /// Every mechanism that could commit a Signal message without going through
    /// the placement backend, spelled as it would appear in Rust source. The
    /// scan is case-insensitive and runs over code with comments removed.
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

    /// The production half of a module, lowercased, with comments removed so a
    /// doc comment describing the prohibition is not mistaken for breaking it.
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
    fn signal_placement_module_holds_no_submit_shaped_mechanism() {
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
                "native_signal_adapter.rs",
                include_str!("native_signal_adapter.rs"),
                "fn drive_signal_composer_placement",
            ),
            (
                "native_a11y.rs",
                include_str!("native_a11y.rs"),
                "fn resolve_uia2_window",
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

    #[test]
    fn signal_live_probe_clears_after_setvalue_without_sending() {
        let mut backend = FakeSignalBackend::empty();
        backend.readback_suffix = " augmented by live UI";

        let receipt = probe_signal_composer_write_then_clear(
            &mut backend,
            SignalLivePlacementRequest {
                carrier: "probe-5019-osl",
                allow_replace_existing: false,
            },
        );

        assert_eq!(backend.set_values, vec!["probe-5019-osl"]);
        assert_eq!(backend.cleared, 1);
        assert_eq!(receipt.status, SignalPlacementStatus::Placed);
        assert!(receipt.placed);
        assert!(!receipt.enter_sent);
        assert_eq!(
            backend.submit_shaped, 0,
            "the probe path must observe no submit-shaped interaction either"
        );
    }

    #[test]
    fn signal_live_probe_does_not_clear_when_it_refuses_before_setvalue() {
        let mut backend = FakeSignalBackend::empty();
        backend.current = Ok(Some("operator draft".to_owned()));

        let receipt = probe_signal_composer_write_then_clear(
            &mut backend,
            SignalLivePlacementRequest {
                carrier: "probe-6284-osl",
                allow_replace_existing: false,
            },
        );

        assert_eq!(receipt.status, SignalPlacementStatus::ComposerNotEmpty);
        assert!(backend.set_values.is_empty());
        assert_eq!(backend.cleared, 0);
        assert!(!receipt.enter_sent);
    }

    #[test]
    fn signal_live_placement_refuses_to_replace_an_operator_draft_by_default() {
        let mut backend = FakeSignalBackend::empty();
        backend.current = Ok(Some("operator draft".to_owned()));

        let receipt = drive_signal_composer_placement(
            &mut backend,
            SignalLivePlacementRequest {
                carrier: "bravo-9042-osl",
                allow_replace_existing: false,
            },
        );

        assert_eq!(receipt.status, SignalPlacementStatus::ComposerNotEmpty);
        assert!(!receipt.placed);
        assert!(!receipt.enter_sent);
        assert!(backend.set_values.is_empty());
    }

    #[test]
    fn signal_live_placement_requires_one_writable_composer() {
        let mut missing = FakeSignalBackend::empty();
        missing.writable = Ok(0);
        assert_eq!(
            drive_signal_composer_placement(
                &mut missing,
                SignalLivePlacementRequest {
                    carrier: "charlie-1207-osl",
                    allow_replace_existing: false,
                },
            )
            .status,
            SignalPlacementStatus::ComposerUnavailable
        );

        let mut ambiguous = FakeSignalBackend::empty();
        ambiguous.writable = Ok(2);
        let receipt = drive_signal_composer_placement(
            &mut ambiguous,
            SignalLivePlacementRequest {
                carrier: "delta-6214-osl",
                allow_replace_existing: false,
            },
        );
        assert_eq!(receipt.status, SignalPlacementStatus::ComposerAmbiguous);
        assert_eq!(receipt.writable_composer_count, 2);
    }

    #[test]
    fn signal_composer() {
        let window = rect(0, 0, 1200, 900);
        let nodes = vec![
            editable(rect(20, 30, 340, 72), "Nach Signal suchen"),
            editable(rect(460, 735, 1120, 820), "Write a message..."),
            {
                let mut node = editable(rect(460, 620, 1120, 680), "message helper");
                node.read_only = true;
                node
            },
            SignalNode::structural(SignalRole::Button, rect(1080, 735, 1160, 820)),
        ];

        assert_eq!(discover_signal_composer(&nodes, window), Ok(1));
        assert_eq!(
            resolve_signal_composer(&nodes, window).map(|resolution| resolution.method),
            Ok(SignalComposerResolutionMethod::AccessibleNameAndRole)
        );

        let mut renamed = nodes.clone();
        renamed[1].localized_name = Some("Escribe un mensaje".to_owned());
        assert_eq!(discover_signal_composer(&renamed, window), Ok(1));

        let mut unnamed = renamed.clone();
        for node in &mut unnamed {
            node.localized_name = None;
        }
        assert_eq!(discover_signal_composer(&unnamed, window), Ok(1));
        assert_eq!(
            resolve_signal_composer(&unnamed, window).map(|resolution| resolution.method),
            Ok(SignalComposerResolutionMethod::UnnamedGeometryFallback)
        );

        let mut ambiguous = renamed;
        ambiguous.push(editable(rect(480, 740, 1130, 825), "Message"));
        assert_eq!(
            discover_signal_composer(&ambiguous, window),
            Err(SignalSelectorError::Ambiguous)
        );
    }

    #[test]
    fn signal_composer_refuses_login_or_search_field_instead_of_placing_there() {
        let window = rect(0, 0, 1200, 900);

        // Realistic search-field names, each carrying its locale's composer stem,
        // each sitting exactly where the composer would be, each the only
        // writable Edit present -- the not-signed-in / no-conversation state.
        for name in [
            "Search messages",
            "Nachrichten durchsuchen",
            "Buscar mensajes",
            "Message requests",
            "Filter chats",
        ] {
            let nodes = vec![editable(rect(460, 735, 1120, 820), name)];
            assert_eq!(
                discover_signal_composer(&nodes, window),
                Err(SignalSelectorError::Missing),
                "{name:?} is a search or filter field, not a composer"
            );
        }

        // A search field must not make a real composer ambiguous either.
        let with_composer = vec![
            editable(rect(20, 30, 340, 72), "Search messages"),
            editable(rect(460, 735, 1120, 820), "Message"),
        ];
        assert_eq!(discover_signal_composer(&with_composer, window), Ok(1));
    }

    #[test]
    fn signal_composer_geometry_fallback_refuses_an_unnamed_login_field() {
        let window = rect(0, 0, 1200, 900);

        // Signal's link/registration screen: one unnamed writable phone-number
        // entry, centred. The named path cannot fire, so this is the fallback's
        // own refusal, which had no test before.
        let centred_phone_entry = vec![{
            let mut node = editable(rect(430, 430, 770, 480), "");
            node.localized_name = None;
            node
        }];
        assert_eq!(
            resolve_signal_composer(&centred_phone_entry, window).map(|r| r.method),
            Err(SignalSelectorError::Missing)
        );

        // Same field, still unnamed, but in the conversation pane's composer
        // band -- the fallback is allowed to accept that one, which proves the
        // refusal above came from geometry and not from an inert fallback.
        let composer_band = vec![{
            let mut node = editable(rect(460, 735, 1120, 820), "");
            node.localized_name = None;
            node
        }];
        assert_eq!(
            resolve_signal_composer(&composer_band, window).map(|r| r.method),
            Ok(SignalComposerResolutionMethod::UnnamedGeometryFallback)
        );
    }

    #[test]
    fn signal_transcript() {
        let window = rect(0, 0, 1200, 900);
        let nodes = vec![
            list(rect(0, 90, 360, 850), "Chats"),
            list(rect(430, 92, 1130, 710), "Nachrichtenverlauf"),
            editable(rect(455, 735, 1125, 820), "Nachricht"),
            list(rect(455, 832, 1125, 880), "suggestions below composer"),
        ];
        let composer = discover_signal_composer(&nodes, window).expect("composer is structural");

        assert_eq!(discover_signal_transcript(&nodes, composer, window), Ok(1));

        let mut renamed = nodes.clone();
        renamed[1].localized_name = Some("Historial de mensajes".to_owned());
        assert_eq!(
            discover_signal_transcript(&renamed, composer, window),
            Ok(1)
        );

        assert_eq!(
            discover_signal_transcript(&renamed, 0, window),
            Err(SignalSelectorError::Invalid)
        );

        let mut ambiguous = renamed;
        ambiguous.push(list(rect(440, 100, 1128, 700), "second paired list"));
        assert_eq!(
            discover_signal_transcript(&ambiguous, composer, window),
            Err(SignalSelectorError::Ambiguous)
        );
    }

    #[test]
    fn signal_row_candidates() {
        let mut row = SignalNode::structural(SignalRole::Row, rect(420, 230, 1130, 330));
        row.children = vec![1, 2, 3];
        let nodes = vec![
            row,
            body_text(rect(500, 245, 970, 270), "first visible body"),
            {
                let mut decorative =
                    SignalNode::structural(SignalRole::Text, rect(500, 274, 970, 292));
                decorative.text = Some("timestamp that is not body evidence".to_owned());
                decorative
            },
            body_text(rect(500, 296, 970, 320), "second visible body"),
        ];

        let candidates = extract_signal_row_candidates(&nodes, 0, 4, 128)
            .expect("body evidence should produce candidates");
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].node_index, 1);
        assert_eq!(candidates[0].text, "first visible body");
        assert_eq!(candidates[1].node_index, 3);
        assert_eq!(candidates[1].text, "second visible body");
        assert!(
            candidates.iter().all(|candidate| candidate.node_index != 2),
            "text without body evidence must not become a row candidate"
        );

        let mut invalid = nodes.clone();
        invalid[1].text = Some("bad\u{0008}body".to_owned());
        assert_eq!(
            extract_signal_row_candidates(&invalid, 0, 4, 128).map(|value| value.len()),
            Err(SignalSelectorError::Invalid)
        );

        let mut missing_body_text = nodes.clone();
        missing_body_text[3].text = None;
        assert_eq!(
            extract_signal_row_candidates(&missing_body_text, 0, 4, 128).map(|value| value.len()),
            Err(SignalSelectorError::Invalid)
        );

        let mut outside_row = nodes.clone();
        outside_row[3].bounds = rect(500, 296, 1170, 320);
        assert_eq!(
            extract_signal_row_candidates(&outside_row, 0, 4, 128).map(|value| value.len()),
            Err(SignalSelectorError::Invalid)
        );

        assert_eq!(
            extract_signal_row_candidates(&nodes, 0, 1, 128).map(|value| value.len()),
            Err(SignalSelectorError::LimitExceeded)
        );
    }

    #[test]
    fn signal_carrier() {
        let composer_anchor = "a".repeat(64);
        let carrier = "OSL: sealed carrier body follows";
        let exact_prefix = "OSL: sealed";
        let proof = signal_carrier_prefix_proof_sha256(&composer_anchor, exact_prefix, carrier);

        let placement = place_signal_carrier(SignalCarrierPlacementRequest {
            composer_anchor_sha256: &composer_anchor,
            carrier,
            exact_prefix,
            prefix_proof_sha256: &proof,
        })
        .expect("exact prefix proof should place carrier");
        assert_eq!(placement.committed_text, carrier);
        assert_eq!(placement.exact_prefix, exact_prefix);
        assert_eq!(placement.prefix_proof_sha256, proof);

        let shorter_prefix_proof =
            signal_carrier_prefix_proof_sha256(&composer_anchor, "OSL:", carrier);
        assert_eq!(
            place_signal_carrier(SignalCarrierPlacementRequest {
                composer_anchor_sha256: &composer_anchor,
                carrier,
                exact_prefix,
                prefix_proof_sha256: &shorter_prefix_proof,
            })
            .map(|placement| placement.prefix_proof_sha256),
            Err(SignalSelectorError::ProofMismatch)
        );

        assert_eq!(
            place_signal_carrier(SignalCarrierPlacementRequest {
                composer_anchor_sha256: &composer_anchor,
                carrier,
                exact_prefix: "sealed",
                prefix_proof_sha256: &proof,
            })
            .map(|placement| placement.committed_text),
            Err(SignalSelectorError::Invalid)
        );
    }

    #[test]
    fn signal_geometry() {
        let row_bounds = rect(420, 230, 1130, 340);
        let candidates = vec![
            SignalRowCandidate {
                node_index: 10,
                text: "authenticated first body".to_owned(),
                body_bounds: rect(500, 245, 960, 270),
            },
            SignalRowCandidate {
                node_index: 11,
                text: "unauthenticated wide body".to_owned(),
                body_bounds: rect(440, 272, 1120, 296),
            },
            SignalRowCandidate {
                node_index: 12,
                text: "authenticated second body".to_owned(),
                body_bounds: rect(500, 300, 980, 326),
            },
        ];

        let single = signal_paint_geometry(row_bounds, &candidates, &[12])
            .expect("one authenticated body rectangle should paint");
        assert_eq!(single.paint_bounds, rect(500, 300, 980, 326));
        assert_eq!(single.authenticated_node_indices, vec![12]);

        let paired = signal_paint_geometry(row_bounds, &candidates, &[10, 12])
            .expect("authenticated body rectangles should union");
        assert_eq!(paired.paint_bounds, rect(500, 245, 980, 326));
        assert_eq!(paired.authenticated_node_indices, vec![10, 12]);
        assert!(
            paired.paint_bounds.right < candidates[1].body_bounds.right,
            "unauthenticated body rectangles must not widen paint geometry"
        );

        assert_eq!(
            signal_paint_geometry(row_bounds, &candidates, &[11])
                .map(|geometry| geometry.paint_bounds),
            Ok(rect(440, 272, 1120, 296))
        );
        assert_eq!(
            signal_paint_geometry(row_bounds, &candidates, &[10, 10])
                .map(|geometry| geometry.paint_bounds),
            Err(SignalSelectorError::Invalid)
        );
        assert_eq!(
            signal_paint_geometry(row_bounds, &candidates, &[99])
                .map(|geometry| geometry.paint_bounds),
            Err(SignalSelectorError::Invalid)
        );
        assert_eq!(
            signal_paint_geometry(row_bounds, &candidates, &[])
                .map(|geometry| geometry.paint_bounds),
            Err(SignalSelectorError::Missing)
        );
    }
}
