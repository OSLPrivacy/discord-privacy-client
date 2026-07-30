//! Pure Signal Desktop accessibility selectors.
//!
//! This module is intentionally scan-only. It contains no process launch,
//! keyboard, pointer, focus, value-pattern, database, credential, or network
//! capability. Localized names and placeholder text are modeled only so tests
//! can prove selectors do not depend on them.

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

        let mut ambiguous = renamed;
        ambiguous.push(editable(rect(480, 740, 1130, 825), "another locale"));
        assert_eq!(
            discover_signal_composer(&ambiguous, window),
            Err(SignalSelectorError::Ambiguous)
        );
    }
}
