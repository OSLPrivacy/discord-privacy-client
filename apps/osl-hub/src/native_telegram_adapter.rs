//! Pure Telegram Desktop accessibility selectors.
//!
//! Telegram exposes each message as a `ListItem` whose direct children are
//! column sub-items.  This module deliberately selects only those direct
//! columns; it never walks a row's arbitrary descendant tree.

use crate::adapters::{Bounds, PaintConfidence, PaintTarget};

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
