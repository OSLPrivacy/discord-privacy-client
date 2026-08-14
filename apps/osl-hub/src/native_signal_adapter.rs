//! Signal Desktop accessibility selectors.
//!
//! The structural selectors are pure. Text placement deliberately does not live
//! here: Signal's rich editor must be driven through the provider-neutral shared
//! place-text job so the editor updates its own private message state. Localized
//! names and placeholder text are modeled only so tests can prove selectors do
//! not depend on them.

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

fn valid_candidate_text(value: &str, max_text_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_text_bytes
        && !value
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
}

#[cfg(test)]
mod tests {
    use super::*;

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
