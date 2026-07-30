//! Pure WhatsApp Desktop accessibility selectors.
//!
//! This module is scan-only. It models the structural facts OSL needs before it
//! can claim WhatsApp support: one editable composer paired with one transcript
//! surface, and exact message-body nodes only when the platform exposes them as
//! such. Localized labels and placeholder text are deliberately not selector
//! inputs.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WhatsAppRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl WhatsAppRect {
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
pub enum WhatsAppRole {
    Window,
    Pane,
    List,
    Document,
    Row,
    Text,
    EditableText,
    Button,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WhatsAppNodeEvidence {
    None,
    ExactBody,
    AmbiguousBody,
}

#[derive(Clone)]
pub struct WhatsAppNode {
    pub role: WhatsAppRole,
    pub evidence: WhatsAppNodeEvidence,
    pub bounds: WhatsAppRect,
    pub visible: bool,
    pub enabled: bool,
    pub focusable: bool,
    pub editable: bool,
    pub read_only: bool,
    pub localized_name: Option<String>,
    pub text: Option<String>,
    pub children: Vec<usize>,
}

impl WhatsAppNode {
    pub fn structural(role: WhatsAppRole, bounds: WhatsAppRect) -> Self {
        Self {
            role,
            evidence: WhatsAppNodeEvidence::None,
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
pub enum WhatsAppSelectorError {
    Missing,
    Ambiguous,
    Invalid,
    LimitExceeded,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WhatsAppPair {
    pub composer_index: usize,
    pub transcript_index: usize,
}

#[derive(Clone, Eq, PartialEq)]
pub struct WhatsAppBodyCandidate {
    pub node_index: usize,
    pub text: String,
    pub body_bounds: WhatsAppRect,
}

pub fn discover_whatsapp_pair(
    nodes: &[WhatsAppNode],
    window_bounds: WhatsAppRect,
) -> Result<WhatsAppPair, WhatsAppSelectorError> {
    if !window_bounds.valid() {
        return Err(WhatsAppSelectorError::Invalid);
    }

    let mut pairs = Vec::new();
    for (composer_index, composer) in nodes.iter().enumerate() {
        if !whatsapp_composer_candidate(composer, window_bounds) {
            continue;
        }
        for (transcript_index, transcript) in nodes.iter().enumerate() {
            if whatsapp_transcript_candidate(transcript, composer, window_bounds) {
                pairs.push(WhatsAppPair {
                    composer_index,
                    transcript_index,
                });
            }
        }
    }

    match pairs.as_slice() {
        [pair] => Ok(*pair),
        [] => Err(WhatsAppSelectorError::Missing),
        _ => Err(WhatsAppSelectorError::Ambiguous),
    }
}

fn whatsapp_composer_candidate(node: &WhatsAppNode, window_bounds: WhatsAppRect) -> bool {
    let conversation_left = window_bounds.left.saturating_add(window_bounds.width() / 3);
    let composer_band_top = window_bounds
        .top
        .saturating_add(window_bounds.height() * 2 / 3);
    node.role == WhatsAppRole::EditableText
        && node.visible
        && node.enabled
        && node.focusable
        && node.editable
        && !node.read_only
        && node.bounds.contained_by(window_bounds)
        && node.bounds.left >= conversation_left
        && node.bounds.top >= composer_band_top
        && node.bounds.width() >= 240
        && (24..=180).contains(&node.bounds.height())
}

fn whatsapp_transcript_candidate(
    node: &WhatsAppNode,
    composer: &WhatsAppNode,
    window_bounds: WhatsAppRect,
) -> bool {
    let required_overlap = composer.bounds.width().saturating_mul(3) / 5;
    matches!(node.role, WhatsAppRole::List | WhatsAppRole::Document)
        && node.visible
        && node.bounds.contained_by(window_bounds)
        && node.bounds.bottom <= composer.bounds.top
        && node.bounds.height() >= window_bounds.height() / 3
        && node.bounds.horizontal_overlap(composer.bounds) >= required_overlap
}

fn descendants(nodes: &[WhatsAppNode], root: usize) -> Result<Vec<usize>, WhatsAppSelectorError> {
    let Some(root_node) = nodes.get(root) else {
        return Err(WhatsAppSelectorError::Missing);
    };
    let mut result = Vec::new();
    let mut queue = std::collections::VecDeque::from(root_node.children.clone());
    while let Some(index) = queue.pop_front() {
        let Some(node) = nodes.get(index) else {
            return Err(WhatsAppSelectorError::Invalid);
        };
        if result.len() >= nodes.len() {
            return Err(WhatsAppSelectorError::Invalid);
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

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> WhatsAppRect {
        WhatsAppRect {
            left,
            top,
            right,
            bottom,
        }
    }

    fn editable(bounds: WhatsAppRect, localized_name: &str) -> WhatsAppNode {
        let mut node = WhatsAppNode::structural(WhatsAppRole::EditableText, bounds);
        node.focusable = true;
        node.editable = true;
        node.read_only = false;
        node.localized_name = Some(localized_name.to_owned());
        node
    }

    fn transcript(bounds: WhatsAppRect, localized_name: &str) -> WhatsAppNode {
        let mut node = WhatsAppNode::structural(WhatsAppRole::List, bounds);
        node.localized_name = Some(localized_name.to_owned());
        node
    }

    #[test]
    fn whatsapp_pair() {
        let window = rect(0, 0, 1280, 900);
        let nodes = vec![
            transcript(rect(0, 85, 360, 840), "Chats"),
            transcript(rect(430, 86, 1210, 720), "Messages"),
            editable(rect(455, 748, 1180, 812), "Type a message"),
            editable(rect(16, 130, 320, 174), "Search or start new chat"),
            WhatsAppNode::structural(WhatsAppRole::Button, rect(1188, 748, 1236, 812)),
        ];

        assert_eq!(
            discover_whatsapp_pair(&nodes, window),
            Ok(WhatsAppPair {
                composer_index: 2,
                transcript_index: 1,
            })
        );

        let mut localized = nodes.clone();
        localized[1].localized_name = Some("Historial de mensajes".to_owned());
        localized[2].localized_name = Some("Escribe un mensaje".to_owned());
        assert_eq!(
            discover_whatsapp_pair(&localized, window),
            Ok(WhatsAppPair {
                composer_index: 2,
                transcript_index: 1,
            })
        );

        let mut ambiguous = localized;
        ambiguous.push(transcript(
            rect(440, 96, 1202, 710),
            "second matching transcript",
        ));
        assert_eq!(
            discover_whatsapp_pair(&ambiguous, window),
            Err(WhatsAppSelectorError::Ambiguous)
        );

        let mut missing = nodes;
        missing[2].read_only = true;
        assert_eq!(
            discover_whatsapp_pair(&missing, window),
            Err(WhatsAppSelectorError::Missing)
        );
    }
}
