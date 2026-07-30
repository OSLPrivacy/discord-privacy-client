//! Pure structural contracts for WhatsApp's native WebView2 surface.
//!
//! The live Windows accessibility side is allowed to collect only bounded
//! metadata. This module keeps the trust, discovery, and placement decisions
//! deterministic and testable without touching the remote process.

use sha2::{Digest, Sha256};

pub const WHATSAPP_ROOT_WINDOW_CLASS: &str = "WinUIDesktopWin32WindowClass";
pub const WHATSAPP_CARRIER_PREFIX: &str = "OSL1.WA.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WhatsAppProcessKind {
    StoreAppRoot,
    WebView2Child,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WhatsAppControlType {
    Window,
    Pane,
    Document,
    Edit,
    List,
    ListItem,
    Text,
    Button,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WhatsAppBounds {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl WhatsAppBounds {
    fn is_positive(self) -> bool {
        self.width > 0 && self.height > 0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WhatsAppStructuralNode {
    pub structural_id: String,
    pub parent_structural_id: Option<String>,
    pub process_kind: WhatsAppProcessKind,
    pub window_class: Option<String>,
    pub control_type: WhatsAppControlType,
    pub automation_id: Option<String>,
    pub class_name: Option<String>,
    pub framework_id: Option<String>,
    pub runtime_hash: Option<String>,
    pub bounds: Option<WhatsAppBounds>,
    pub enabled: bool,
    pub offscreen: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustedWhatsAppContentRoot {
    pub app_root_id: String,
    pub webview_ancestor_id: String,
    pub content_root_id: String,
    pub content_runtime_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WhatsAppComposerBinding {
    pub node_id: String,
    pub runtime_hash: String,
    pub bounds: WhatsAppBounds,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WhatsAppTranscriptBinding {
    pub node_id: String,
    pub runtime_hash: String,
    pub bounds: WhatsAppBounds,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredWhatsAppPair {
    pub content_root: TrustedWhatsAppContentRoot,
    pub composer: WhatsAppComposerBinding,
    pub transcript: WhatsAppTranscriptBinding,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WhatsAppTextFragmentRole {
    ExactBody,
    SenderLabel,
    Timestamp,
    Reaction,
    LinkPreviewTitle,
    LinkPreviewDescription,
    Summary,
    Unknown,
}

#[derive(Clone, PartialEq, Eq)]
pub struct WhatsAppTextFragment {
    pub role: WhatsAppTextFragmentRole,
    pub text: String,
}

#[derive(Clone, PartialEq, Eq)]
pub struct WhatsAppTranscriptRow {
    pub row_id: String,
    pub transcript_node_id: String,
    pub runtime_hash: String,
    pub bounds: Option<WhatsAppBounds>,
    pub offscreen: bool,
    pub fragments: Vec<WhatsAppTextFragment>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct WhatsAppBodyCandidate {
    pub row_id: String,
    pub row_runtime_hash: String,
    pub body: String,
    pub bounds: WhatsAppBounds,
}

#[derive(Clone, PartialEq, Eq)]
pub struct WhatsAppCarrierRowProof {
    pub row_id: String,
    pub row_runtime_hash: String,
    pub carrier_sha256_label: String,
    pub bounds: WhatsAppBounds,
}

#[derive(Clone, PartialEq, Eq)]
pub struct PlacedWhatsAppCarrier {
    pub composer_node_id: String,
    pub composer_runtime_hash: String,
    pub carrier: String,
    pub row_proof: WhatsAppCarrierRowProof,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WhatsAppAdapterRefusal {
    MissingExactAppRoot,
    AmbiguousAppRoot,
    MissingWebView2ContentRoot,
    AmbiguousWebView2ContentRoot,
    MissingComposer,
    AmbiguousComposer,
    MissingTranscript,
    AmbiguousTranscript,
    MissingExactBodyCandidate,
    BodyCandidateSupportBlocked,
    InvalidCarrier,
    MissingCarrierRowProof,
    AmbiguousCarrierRowProof,
}

pub fn trusted_whatsapp_content_root(
    nodes: &[WhatsAppStructuralNode],
) -> Result<TrustedWhatsAppContentRoot, WhatsAppAdapterRefusal> {
    let roots: Vec<&WhatsAppStructuralNode> = nodes
        .iter()
        .filter(|node| {
            node.process_kind == WhatsAppProcessKind::StoreAppRoot
                && node.parent_structural_id.is_none()
                && node.window_class.as_deref() == Some(WHATSAPP_ROOT_WINDOW_CLASS)
                && node.control_type == WhatsAppControlType::Window
                && node.enabled
                && !node.offscreen
                && node.bounds.map_or(false, WhatsAppBounds::is_positive)
        })
        .collect();
    let app_root = match roots.as_slice() {
        [root] => *root,
        [] => return Err(WhatsAppAdapterRefusal::MissingExactAppRoot),
        _ => return Err(WhatsAppAdapterRefusal::AmbiguousAppRoot),
    };

    let content_roots: Vec<TrustedWhatsAppContentRoot> = nodes
        .iter()
        .filter_map(|node| {
            let webview_ancestor_id = trusted_webview_ancestor_id(nodes, app_root, node)?;
            if !is_content_root_candidate(node) {
                return None;
            }
            Some(TrustedWhatsAppContentRoot {
                app_root_id: app_root.structural_id.clone(),
                webview_ancestor_id,
                content_root_id: node.structural_id.clone(),
                content_runtime_hash: node.runtime_hash.clone()?,
            })
        })
        .collect();

    match content_roots.as_slice() {
        [content_root] => Ok(content_root.clone()),
        [] => Err(WhatsAppAdapterRefusal::MissingWebView2ContentRoot),
        _ => Err(WhatsAppAdapterRefusal::AmbiguousWebView2ContentRoot),
    }
}

pub fn discover_whatsapp_pair(
    nodes: &[WhatsAppStructuralNode],
) -> Result<DiscoveredWhatsAppPair, WhatsAppAdapterRefusal> {
    let content_root = trusted_whatsapp_content_root(nodes)?;
    let composer_candidates: Vec<WhatsAppComposerBinding> = nodes
        .iter()
        .filter(|node| descendant_of(nodes, node, &content_root.content_root_id))
        .filter_map(composer_binding)
        .collect();
    let transcript_candidates: Vec<WhatsAppTranscriptBinding> = nodes
        .iter()
        .filter(|node| descendant_of(nodes, node, &content_root.content_root_id))
        .filter(|node| node.structural_id != content_root.content_root_id)
        .filter_map(transcript_binding)
        .collect();

    let composer = match composer_candidates.as_slice() {
        [composer] => composer.clone(),
        [] => return Err(WhatsAppAdapterRefusal::MissingComposer),
        _ => return Err(WhatsAppAdapterRefusal::AmbiguousComposer),
    };
    let transcript = match transcript_candidates.as_slice() {
        [transcript] => transcript.clone(),
        [] => return Err(WhatsAppAdapterRefusal::MissingTranscript),
        _ => return Err(WhatsAppAdapterRefusal::AmbiguousTranscript),
    };

    Ok(DiscoveredWhatsAppPair {
        content_root,
        composer,
        transcript,
    })
}

pub fn extract_whatsapp_body_candidates(
    pair: &DiscoveredWhatsAppPair,
    rows: &[WhatsAppTranscriptRow],
) -> Result<Vec<WhatsAppBodyCandidate>, WhatsAppAdapterRefusal> {
    let mut candidates = Vec::new();
    for row in rows.iter().filter(|row| {
        row.transcript_node_id == pair.transcript.node_id
            && !row.offscreen
            && row.bounds.map_or(false, WhatsAppBounds::is_positive)
    }) {
        let mut row_has_blocking_text = false;
        for fragment in &row.fragments {
            let text = canonical_whatsapp_body(&fragment.text);
            if text.is_empty() {
                continue;
            }
            match fragment.role {
                WhatsAppTextFragmentRole::ExactBody => {
                    candidates.push(WhatsAppBodyCandidate {
                        row_id: row.row_id.clone(),
                        row_runtime_hash: row.runtime_hash.clone(),
                        body: text,
                        bounds: row.bounds.expect("row bounds were checked above"),
                    });
                }
                WhatsAppTextFragmentRole::SenderLabel
                | WhatsAppTextFragmentRole::Timestamp
                | WhatsAppTextFragmentRole::Reaction => {}
                WhatsAppTextFragmentRole::LinkPreviewTitle
                | WhatsAppTextFragmentRole::LinkPreviewDescription
                | WhatsAppTextFragmentRole::Summary
                | WhatsAppTextFragmentRole::Unknown => {
                    row_has_blocking_text = true;
                }
            }
        }
        if row_has_blocking_text {
            return Err(WhatsAppAdapterRefusal::BodyCandidateSupportBlocked);
        }
    }
    if candidates.is_empty() {
        return Err(WhatsAppAdapterRefusal::MissingExactBodyCandidate);
    }
    Ok(candidates)
}

pub fn place_whatsapp_carrier(
    pair: &DiscoveredWhatsAppPair,
    carrier_payload: &str,
    rows_after_write: &[WhatsAppTranscriptRow],
) -> Result<PlacedWhatsAppCarrier, WhatsAppAdapterRefusal> {
    let carrier = prefixed_whatsapp_carrier(carrier_payload)?;
    let matching_candidates: Vec<WhatsAppBodyCandidate> =
        extract_whatsapp_body_candidates(pair, rows_after_write)?
            .into_iter()
            .filter(|candidate| candidate.body == carrier)
            .collect();
    let proof_candidate = match matching_candidates.as_slice() {
        [candidate] => candidate,
        [] => return Err(WhatsAppAdapterRefusal::MissingCarrierRowProof),
        _ => return Err(WhatsAppAdapterRefusal::AmbiguousCarrierRowProof),
    };

    Ok(PlacedWhatsAppCarrier {
        composer_node_id: pair.composer.node_id.clone(),
        composer_runtime_hash: pair.composer.runtime_hash.clone(),
        carrier: carrier.clone(),
        row_proof: WhatsAppCarrierRowProof {
            row_id: proof_candidate.row_id.clone(),
            row_runtime_hash: proof_candidate.row_runtime_hash.clone(),
            carrier_sha256_label: carrier_sha256_label(&carrier),
            bounds: proof_candidate.bounds,
        },
    })
}

fn trusted_webview_ancestor_id(
    nodes: &[WhatsAppStructuralNode],
    app_root: &WhatsAppStructuralNode,
    node: &WhatsAppStructuralNode,
) -> Option<String> {
    let mut cursor = Some(node);
    let mut webview_ancestor_id = None;
    let mut depth = 0usize;
    while let Some(current) = cursor {
        if depth > 32 {
            return None;
        }
        if current.structural_id == app_root.structural_id {
            return webview_ancestor_id;
        }
        if is_webview2_lineage_node(current) {
            webview_ancestor_id = Some(current.structural_id.clone());
        }
        cursor = current
            .parent_structural_id
            .as_deref()
            .and_then(|parent_id| {
                nodes
                    .iter()
                    .find(|candidate| candidate.structural_id == parent_id)
            });
        depth += 1;
    }
    None
}

fn descendant_of(
    nodes: &[WhatsAppStructuralNode],
    node: &WhatsAppStructuralNode,
    ancestor_id: &str,
) -> bool {
    let mut cursor = Some(node);
    let mut depth = 0usize;
    while let Some(current) = cursor {
        if depth > 32 {
            return false;
        }
        if current.structural_id == ancestor_id {
            return true;
        }
        cursor = current
            .parent_structural_id
            .as_deref()
            .and_then(|parent_id| {
                nodes
                    .iter()
                    .find(|candidate| candidate.structural_id == parent_id)
            });
        depth += 1;
    }
    false
}

fn composer_binding(node: &WhatsAppStructuralNode) -> Option<WhatsAppComposerBinding> {
    if node.control_type != WhatsAppControlType::Edit || !visible_structural_node(node) {
        return None;
    }
    let has_composer_hint = token_contains(&node.automation_id, &["composer", "input", "message"])
        || token_contains(
            &node.class_name,
            &["composer", "input", "editable", "textbox"],
        );
    if !has_composer_hint {
        return None;
    }
    Some(WhatsAppComposerBinding {
        node_id: node.structural_id.clone(),
        runtime_hash: node.runtime_hash.clone()?,
        bounds: node.bounds?,
    })
}

fn transcript_binding(node: &WhatsAppStructuralNode) -> Option<WhatsAppTranscriptBinding> {
    if !matches!(
        node.control_type,
        WhatsAppControlType::List | WhatsAppControlType::Document
    ) || !visible_structural_node(node)
    {
        return None;
    }
    let has_transcript_hint = token_contains(
        &node.automation_id,
        &["message-list", "messages", "conversation", "transcript"],
    ) || token_contains(
        &node.class_name,
        &["message-list", "conversation", "transcript"],
    );
    if !has_transcript_hint {
        return None;
    }
    Some(WhatsAppTranscriptBinding {
        node_id: node.structural_id.clone(),
        runtime_hash: node.runtime_hash.clone()?,
        bounds: node.bounds?,
    })
}

fn is_content_root_candidate(node: &WhatsAppStructuralNode) -> bool {
    visible_structural_node(node)
        && matches!(
            node.control_type,
            WhatsAppControlType::Document | WhatsAppControlType::List
        )
        && is_webview2_lineage_node(node)
}

fn visible_structural_node(node: &WhatsAppStructuralNode) -> bool {
    node.enabled
        && !node.offscreen
        && node.bounds.map_or(false, WhatsAppBounds::is_positive)
        && node.runtime_hash.is_some()
}

fn is_webview2_lineage_node(node: &WhatsAppStructuralNode) -> bool {
    if node.process_kind == WhatsAppProcessKind::WebView2Child {
        return true;
    }
    node.class_name.as_deref().map_or(false, |class_name| {
        class_name.contains("Chrome_WidgetWin") || class_name.contains("WebView2")
    }) || node.framework_id.as_deref() == Some("Chrome")
}

fn token_contains(token: &Option<String>, needles: &[&str]) -> bool {
    token.as_deref().map_or(false, |value| {
        let value = value.to_ascii_lowercase();
        needles.iter().any(|needle| value.contains(needle))
    })
}

fn canonical_whatsapp_body(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim_matches(|value: char| value.is_whitespace() && value != '\n')
        .to_owned()
}

fn prefixed_whatsapp_carrier(payload: &str) -> Result<String, WhatsAppAdapterRefusal> {
    let payload = payload.trim();
    if payload.is_empty()
        || payload.len() > 4096
        || payload.chars().any(|value| value.is_control())
        || payload.contains(char::is_whitespace)
    {
        return Err(WhatsAppAdapterRefusal::InvalidCarrier);
    }
    Ok(format!("{WHATSAPP_CARRIER_PREFIX}{payload}"))
}

fn carrier_sha256_label(carrier: &str) -> String {
    let digest = Sha256::digest(carrier.as_bytes());
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        encoded.push_str(&format!("{byte:02x}"));
    }
    format!("sha256:{encoded}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(
        id: &str,
        parent: Option<&str>,
        control_type: WhatsAppControlType,
    ) -> WhatsAppStructuralNode {
        WhatsAppStructuralNode {
            structural_id: id.to_owned(),
            parent_structural_id: parent.map(str::to_owned),
            process_kind: WhatsAppProcessKind::Other,
            window_class: None,
            control_type,
            automation_id: None,
            class_name: None,
            framework_id: None,
            runtime_hash: Some(format!("runtime-{id}")),
            bounds: Some(WhatsAppBounds {
                x: 1,
                y: 1,
                width: 10,
                height: 10,
            }),
            enabled: true,
            offscreen: false,
        }
    }

    fn app_root() -> WhatsAppStructuralNode {
        WhatsAppStructuralNode {
            structural_id: "app-root".to_owned(),
            parent_structural_id: None,
            process_kind: WhatsAppProcessKind::StoreAppRoot,
            window_class: Some(WHATSAPP_ROOT_WINDOW_CLASS.to_owned()),
            control_type: WhatsAppControlType::Window,
            automation_id: None,
            class_name: None,
            framework_id: None,
            runtime_hash: Some("runtime-app-root".to_owned()),
            bounds: Some(WhatsAppBounds {
                x: 0,
                y: 0,
                width: 800,
                height: 600,
            }),
            enabled: true,
            offscreen: false,
        }
    }

    fn webview(id: &str, parent: &str) -> WhatsAppStructuralNode {
        let mut node = node(id, Some(parent), WhatsAppControlType::Pane);
        node.process_kind = WhatsAppProcessKind::WebView2Child;
        node.class_name = Some("Chrome_WidgetWin_0".to_owned());
        node.framework_id = Some("Chrome".to_owned());
        node
    }

    fn trusted_nodes_with_pair() -> Vec<WhatsAppStructuralNode> {
        let root = app_root();
        let bridge = webview("webview", "app-root");
        let mut content = webview("content", "webview");
        content.control_type = WhatsAppControlType::Document;
        content.runtime_hash = Some("runtime-content".to_owned());
        let mut transcript = node("transcript", Some("content"), WhatsAppControlType::List);
        transcript.automation_id = Some("message-list".to_owned());
        transcript.runtime_hash = Some("runtime-transcript".to_owned());
        let mut composer = node("composer", Some("content"), WhatsAppControlType::Edit);
        composer.automation_id = Some("main-composer-input".to_owned());
        composer.runtime_hash = Some("runtime-composer".to_owned());
        vec![root, bridge, content, transcript, composer]
    }

    fn discovered_pair() -> DiscoveredWhatsAppPair {
        discover_whatsapp_pair(&trusted_nodes_with_pair()).expect("pair fixture should be exact")
    }

    fn row(row_id: &str, role: WhatsAppTextFragmentRole, text: &str) -> WhatsAppTranscriptRow {
        WhatsAppTranscriptRow {
            row_id: row_id.to_owned(),
            transcript_node_id: "transcript".to_owned(),
            runtime_hash: format!("runtime-{row_id}"),
            bounds: Some(WhatsAppBounds {
                x: 5,
                y: 5,
                width: 50,
                height: 20,
            }),
            offscreen: false,
            fragments: vec![WhatsAppTextFragment {
                role,
                text: text.to_owned(),
            }],
        }
    }

    fn exact_body_row(row_id: &str, text: &str) -> WhatsAppTranscriptRow {
        row(row_id, WhatsAppTextFragmentRole::ExactBody, text)
    }

    #[test]
    fn whatsapp_content_root_trusts_only_webview2_descendant_of_exact_whatsapp_root() {
        let root = app_root();
        let bridge = webview("webview", "app-root");
        let mut content = webview("content", "webview");
        content.control_type = WhatsAppControlType::Document;
        content.runtime_hash = Some("runtime-content".to_owned());
        let trusted =
            trusted_whatsapp_content_root(&[root.clone(), bridge.clone(), content.clone()])
                .expect("one WebView2 descendant content root should be trusted");
        assert_eq!(trusted.app_root_id, "app-root");
        assert_eq!(trusted.webview_ancestor_id, "webview");
        assert_eq!(trusted.content_root_id, "content");
        assert_eq!(trusted.content_runtime_hash, "runtime-content");

        let mut not_webview = node(
            "plain-document",
            Some("app-root"),
            WhatsAppControlType::Document,
        );
        not_webview.runtime_hash = Some("runtime-plain".to_owned());
        assert_eq!(
            trusted_whatsapp_content_root(&[root.clone(), not_webview]),
            Err(WhatsAppAdapterRefusal::MissingWebView2ContentRoot)
        );

        let mut sibling = webview("sibling-content", "foreign-root");
        sibling.control_type = WhatsAppControlType::Document;
        assert_eq!(
            trusted_whatsapp_content_root(&[
                root.clone(),
                bridge.clone(),
                content.clone(),
                sibling
            ]),
            Ok(trusted)
        );

        let mut second_content = webview("second-content", "webview");
        second_content.control_type = WhatsAppControlType::Document;
        assert_eq!(
            trusted_whatsapp_content_root(&[root, bridge, content, second_content]),
            Err(WhatsAppAdapterRefusal::AmbiguousWebView2ContentRoot)
        );
    }

    #[test]
    fn whatsapp_pair_discovers_composer_and_transcript_only_as_one_pair() {
        let nodes = trusted_nodes_with_pair();
        let pair = discover_whatsapp_pair(&nodes).expect("exact pair should be discovered");
        assert_eq!(pair.content_root.content_root_id, "content");
        assert_eq!(pair.composer.node_id, "composer");
        assert_eq!(pair.composer.runtime_hash, "runtime-composer");
        assert_eq!(pair.transcript.node_id, "transcript");
        assert_eq!(pair.transcript.runtime_hash, "runtime-transcript");

        let mut missing_transcript = nodes.clone();
        missing_transcript.retain(|node| node.structural_id != "transcript");
        assert_eq!(
            discover_whatsapp_pair(&missing_transcript),
            Err(WhatsAppAdapterRefusal::MissingTranscript)
        );

        let mut missing_composer = nodes.clone();
        missing_composer.retain(|node| node.structural_id != "composer");
        assert_eq!(
            discover_whatsapp_pair(&missing_composer),
            Err(WhatsAppAdapterRefusal::MissingComposer)
        );

        let mut ambiguous_composer = nodes.clone();
        let mut second = node(
            "second-composer",
            Some("content"),
            WhatsAppControlType::Edit,
        );
        second.automation_id = Some("composer-input".to_owned());
        ambiguous_composer.push(second);
        assert_eq!(
            discover_whatsapp_pair(&ambiguous_composer),
            Err(WhatsAppAdapterRefusal::AmbiguousComposer)
        );
    }

    #[test]
    fn whatsapp_body_candidates_extract_only_exact_row_bodies_or_block() {
        let pair = discovered_pair();
        let rows = vec![
            row("timestamp", WhatsAppTextFragmentRole::Timestamp, "10:41"),
            row(
                "body",
                WhatsAppTextFragmentRole::ExactBody,
                "  hello\r\nworld  ",
            ),
        ];
        let candidates = extract_whatsapp_body_candidates(&pair, &rows)
            .expect("one exact body row should produce a body candidate");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].row_id, "body");
        assert_eq!(candidates[0].row_runtime_hash, "runtime-body");
        assert_eq!(candidates[0].body, "hello\nworld");

        let preview = vec![row(
            "preview",
            WhatsAppTextFragmentRole::LinkPreviewTitle,
            "Example Domain",
        )];
        assert!(matches!(
            extract_whatsapp_body_candidates(&pair, &preview),
            Err(WhatsAppAdapterRefusal::BodyCandidateSupportBlocked)
        ));

        let unknown = vec![row(
            "unknown",
            WhatsAppTextFragmentRole::Unknown,
            "maybe body",
        )];
        assert!(matches!(
            extract_whatsapp_body_candidates(&pair, &unknown),
            Err(WhatsAppAdapterRefusal::BodyCandidateSupportBlocked)
        ));

        let metadata_only = vec![row(
            "timestamp",
            WhatsAppTextFragmentRole::Timestamp,
            "10:42",
        )];
        assert!(matches!(
            extract_whatsapp_body_candidates(&pair, &metadata_only),
            Err(WhatsAppAdapterRefusal::MissingExactBodyCandidate)
        ));
    }

    #[test]
    fn whatsapp_carrier_writes_prefixed_carrier_only_with_row_proof() {
        let pair = discovered_pair();
        let proof_rows = vec![exact_body_row("carrier-row", "OSL1.WA.publiccover")];
        let placed = place_whatsapp_carrier(&pair, "publiccover", &proof_rows)
            .expect("matching exact body row should prove placement");
        assert_eq!(placed.composer_node_id, "composer");
        assert_eq!(placed.composer_runtime_hash, "runtime-composer");
        assert_eq!(placed.carrier, "OSL1.WA.publiccover");
        assert_eq!(placed.row_proof.row_id, "carrier-row");
        assert_eq!(placed.row_proof.row_runtime_hash, "runtime-carrier-row");
        assert!(placed.row_proof.carrier_sha256_label.starts_with("sha256:"));

        let unprefixed_row = vec![exact_body_row("unprefixed", "publiccover")];
        assert!(matches!(
            place_whatsapp_carrier(&pair, "publiccover", &unprefixed_row),
            Err(WhatsAppAdapterRefusal::MissingCarrierRowProof)
        ));

        let preview_row = vec![row(
            "preview",
            WhatsAppTextFragmentRole::LinkPreviewTitle,
            "OSL1.WA.publiccover",
        )];
        assert!(matches!(
            place_whatsapp_carrier(&pair, "publiccover", &preview_row),
            Err(WhatsAppAdapterRefusal::BodyCandidateSupportBlocked)
        ));

        let duplicate_rows = vec![
            exact_body_row("carrier-row-a", "OSL1.WA.publiccover"),
            exact_body_row("carrier-row-b", "OSL1.WA.publiccover"),
        ];
        assert!(matches!(
            place_whatsapp_carrier(&pair, "publiccover", &duplicate_rows),
            Err(WhatsAppAdapterRefusal::AmbiguousCarrierRowProof)
        ));

        assert!(matches!(
            place_whatsapp_carrier(&pair, "has spaces", &[]),
            Err(WhatsAppAdapterRefusal::InvalidCarrier)
        ));
    }
}
