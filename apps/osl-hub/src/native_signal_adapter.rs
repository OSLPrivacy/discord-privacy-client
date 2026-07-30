//! Pure Signal Desktop accessibility selectors.
//!
//! This module is intentionally scan-only. It contains no process launch,
//! keyboard, pointer, focus, value-pattern, database, credential, or network
//! capability. Localized names and placeholder text are modeled only so tests
//! can prove selectors do not depend on them.

use sha2::{Digest, Sha256};

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

pub fn discover_signal_composer(
    nodes: &[SignalNode],
    window_bounds: SignalRect,
) -> Result<usize, SignalSelectorError> {
    let matches = nodes
        .iter()
        .enumerate()
        .filter_map(|(index, node)| signal_composer_candidate(node, window_bounds).then_some(index))
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [index] => Ok(*index),
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
    let right_pane_left = window_bounds.left.saturating_add(window_bounds.width() / 3);
    let lower_band_top = window_bounds
        .top
        .saturating_add(window_bounds.height() * 3 / 5);
    node.role == SignalRole::EditableText
        && node.visible
        && node.enabled
        && node.focusable
        && node.editable
        && !node.read_only
        && node.bounds.contained_by(window_bounds)
        && node.bounds.left >= right_pane_left
        && node.bounds.top >= lower_band_top
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
    fn signal_composer() {
        let window = rect(0, 0, 1200, 900);
        let nodes = vec![
            editable(rect(20, 30, 340, 72), "Nach Signal suchen"),
            editable(rect(460, 735, 1120, 820), "localized placeholder A"),
            {
                let mut node = editable(rect(460, 620, 1120, 680), "localized placeholder B");
                node.read_only = true;
                node
            },
            SignalNode::structural(SignalRole::Button, rect(1080, 735, 1160, 820)),
        ];

        assert_eq!(discover_signal_composer(&nodes, window), Ok(1));

        let mut renamed = nodes.clone();
        renamed[1].localized_name = Some("Escribe un mensaje".to_owned());
        assert_eq!(discover_signal_composer(&renamed, window), Ok(1));

        let mut unnamed = renamed.clone();
        for node in &mut unnamed {
            node.localized_name = None;
        }
        assert_eq!(discover_signal_composer(&unnamed, window), Ok(1));

        let mut ambiguous = renamed;
        ambiguous.push(editable(rect(480, 740, 1130, 825), "another locale"));
        assert_eq!(
            discover_signal_composer(&ambiguous, window),
            Err(SignalSelectorError::Ambiguous)
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
            candidates
                .iter()
                .all(|candidate| candidate.node_index != 2),
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
