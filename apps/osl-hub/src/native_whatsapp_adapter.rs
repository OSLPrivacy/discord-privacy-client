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
    pub store_package_family_name: Option<String>,
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

#[derive(Clone, PartialEq, Eq)]
pub enum WhatsAppLinkPreviewBlockReason {
    InvalidCarrier,
    ExactBodyUnavailable,
    PreviewFragmentsAreNotExactBody,
    MissingCarrierRowProof,
    AmbiguousCarrierRowProof,
}

#[derive(Clone, PartialEq, Eq)]
pub enum WhatsAppLinkPreviewCapability {
    SupportedWithoutPreview {
        row_id: String,
        carrier_sha256_label: String,
    },
    Blocked {
        reason: WhatsAppLinkPreviewBlockReason,
    },
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
                && node.store_package_family_name.as_deref()
                    == Some(crate::native_apps::whatsapp_store_package_family_name())
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

pub fn whatsapp_link_preview_capability(
    pair: &DiscoveredWhatsAppPair,
    carrier_payload: &str,
    rows_after_write: &[WhatsAppTranscriptRow],
) -> WhatsAppLinkPreviewCapability {
    if rows_after_write
        .iter()
        .filter(|row| {
            row.transcript_node_id == pair.transcript.node_id
                && !row.offscreen
                && row.bounds.map_or(false, WhatsAppBounds::is_positive)
        })
        .flat_map(|row| &row.fragments)
        .any(|fragment| {
            !canonical_whatsapp_body(&fragment.text).is_empty()
                && matches!(
                    fragment.role,
                    WhatsAppTextFragmentRole::LinkPreviewTitle
                        | WhatsAppTextFragmentRole::LinkPreviewDescription
                )
        })
    {
        return WhatsAppLinkPreviewCapability::Blocked {
            reason: WhatsAppLinkPreviewBlockReason::PreviewFragmentsAreNotExactBody,
        };
    }

    match place_whatsapp_carrier(pair, carrier_payload, rows_after_write) {
        Ok(placed) => WhatsAppLinkPreviewCapability::SupportedWithoutPreview {
            row_id: placed.row_proof.row_id,
            carrier_sha256_label: placed.row_proof.carrier_sha256_label,
        },
        Err(WhatsAppAdapterRefusal::InvalidCarrier) => WhatsAppLinkPreviewCapability::Blocked {
            reason: WhatsAppLinkPreviewBlockReason::InvalidCarrier,
        },
        Err(WhatsAppAdapterRefusal::MissingExactBodyCandidate)
        | Err(WhatsAppAdapterRefusal::BodyCandidateSupportBlocked) => {
            WhatsAppLinkPreviewCapability::Blocked {
                reason: WhatsAppLinkPreviewBlockReason::ExactBodyUnavailable,
            }
        }
        Err(WhatsAppAdapterRefusal::MissingCarrierRowProof) => {
            WhatsAppLinkPreviewCapability::Blocked {
                reason: WhatsAppLinkPreviewBlockReason::MissingCarrierRowProof,
            }
        }
        Err(WhatsAppAdapterRefusal::AmbiguousCarrierRowProof) => {
            WhatsAppLinkPreviewCapability::Blocked {
                reason: WhatsAppLinkPreviewBlockReason::AmbiguousCarrierRowProof,
            }
        }
        Err(_) => WhatsAppLinkPreviewCapability::Blocked {
            reason: WhatsAppLinkPreviewBlockReason::ExactBodyUnavailable,
        },
    }
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
            store_package_family_name: None,
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
            store_package_family_name: Some(
                crate::native_apps::whatsapp_store_package_family_name().to_owned(),
            ),
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

    fn exact_body_row_with_preview(
        row_id: &str,
        body: &str,
        preview: &str,
    ) -> WhatsAppTranscriptRow {
        let mut row = exact_body_row(row_id, body);
        row.fragments.push(WhatsAppTextFragment {
            role: WhatsAppTextFragmentRole::LinkPreviewTitle,
            text: preview.to_owned(),
        });
        row
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

        let mut unbound_root = root.clone();
        unbound_root.store_package_family_name = None;
        assert_eq!(
            trusted_whatsapp_content_root(&[unbound_root, bridge.clone(), content.clone()]),
            Err(WhatsAppAdapterRefusal::MissingExactAppRoot)
        );

        let mut spoofed_root = root.clone();
        spoofed_root.store_package_family_name =
            Some("5319275A.WhatsAppDesktop_attacker".to_owned());
        assert_eq!(
            trusted_whatsapp_content_root(&[spoofed_root, bridge.clone(), content.clone()]),
            Err(WhatsAppAdapterRefusal::MissingExactAppRoot)
        );

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

    #[test]
    fn whatsapp_link_preview_reports_blocked_for_preview_fragments_without_guessing() {
        let pair = discovered_pair();
        let plain_rows = vec![exact_body_row("carrier-row", "OSL1.WA.publiccover")];
        match whatsapp_link_preview_capability(&pair, "publiccover", &plain_rows) {
            WhatsAppLinkPreviewCapability::SupportedWithoutPreview {
                row_id,
                carrier_sha256_label,
            } => {
                assert_eq!(row_id, "carrier-row");
                assert!(carrier_sha256_label.starts_with("sha256:"));
            }
            WhatsAppLinkPreviewCapability::Blocked { .. } => {
                panic!("plain exact carrier row should remain supported")
            }
        }

        let preview_rows = vec![exact_body_row_with_preview(
            "carrier-row",
            "OSL1.WA.publiccover",
            "Example Domain",
        )];
        assert!(matches!(
            whatsapp_link_preview_capability(&pair, "publiccover", &preview_rows),
            WhatsAppLinkPreviewCapability::Blocked {
                reason: WhatsAppLinkPreviewBlockReason::PreviewFragmentsAreNotExactBody
            }
        ));

        let missing_proof = vec![exact_body_row("other-row", "OSL1.WA.different")];
        assert!(matches!(
            whatsapp_link_preview_capability(&pair, "publiccover", &missing_proof),
            WhatsAppLinkPreviewCapability::Blocked {
                reason: WhatsAppLinkPreviewBlockReason::MissingCarrierRowProof
            }
        ));

        assert!(matches!(
            whatsapp_link_preview_capability(&pair, "has spaces", &plain_rows),
            WhatsAppLinkPreviewCapability::Blocked {
                reason: WhatsAppLinkPreviewBlockReason::InvalidCarrier
            }
        ));
    }
}

/// Scan-only selector model kept separate from the WebView2 structural contract.
pub mod scan_selectors {
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

    pub fn extract_whatsapp_body_candidates(
        nodes: &[WhatsAppNode],
        row_index: usize,
        max_candidates: usize,
        max_text_bytes: usize,
    ) -> Result<Vec<WhatsAppBodyCandidate>, WhatsAppSelectorError> {
        let Some(row) = nodes.get(row_index) else {
            return Err(WhatsAppSelectorError::Missing);
        };
        if row.role != WhatsAppRole::Row
            || !row.visible
            || !row.bounds.valid()
            || max_candidates == 0
            || max_text_bytes == 0
        {
            return Err(WhatsAppSelectorError::Invalid);
        }

        let mut candidates = Vec::new();
        for index in descendants(nodes, row_index)? {
            let node = &nodes[index];
            match node.evidence {
                WhatsAppNodeEvidence::None => continue,
                WhatsAppNodeEvidence::AmbiguousBody => return Err(WhatsAppSelectorError::Unsupported),
                WhatsAppNodeEvidence::ExactBody => {
                    let Some(text) = node.text.as_ref() else {
                        return Err(WhatsAppSelectorError::Unsupported);
                    };
                    if node.role != WhatsAppRole::Text
                        || !node.visible
                        || !node.bounds.contained_by(row.bounds)
                        || !valid_candidate_text(text, max_text_bytes)
                    {
                        return Err(WhatsAppSelectorError::Invalid);
                    }
                    if candidates.len() >= max_candidates {
                        return Err(WhatsAppSelectorError::LimitExceeded);
                    }
                    candidates.push(WhatsAppBodyCandidate {
                        node_index: index,
                        text: text.clone(),
                        body_bounds: node.bounds,
                    });
                }
            }
        }

        if candidates.is_empty() {
            return Err(WhatsAppSelectorError::Unsupported);
        }
        Ok(candidates)
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
    mod scan_selector_tests {
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

        fn body_text(bounds: WhatsAppRect, text: &str) -> WhatsAppNode {
            let mut node = WhatsAppNode::structural(WhatsAppRole::Text, bounds);
            node.evidence = WhatsAppNodeEvidence::ExactBody;
            node.text = Some(text.to_owned());
            node
        }

        fn ambiguous_body(bounds: WhatsAppRect, text: &str) -> WhatsAppNode {
            let mut node = WhatsAppNode::structural(WhatsAppRole::Text, bounds);
            node.evidence = WhatsAppNodeEvidence::AmbiguousBody;
            node.text = Some(text.to_owned());
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

        #[test]
        fn whatsapp_body_candidates() {
            let mut row = WhatsAppNode::structural(WhatsAppRole::Row, rect(430, 210, 1210, 330));
            row.children = vec![1, 2, 3, 4];
            let mut nested = WhatsAppNode::structural(WhatsAppRole::Pane, rect(500, 276, 1000, 322));
            nested.children = vec![5];
            let nodes = vec![
                row,
                body_text(rect(510, 226, 970, 252), "first exact body"),
                {
                    let mut timestamp =
                        WhatsAppNode::structural(WhatsAppRole::Text, rect(1120, 254, 1180, 274));
                    timestamp.text = Some("10:42".to_owned());
                    timestamp
                },
                WhatsAppNode::structural(WhatsAppRole::Button, rect(470, 226, 498, 252)),
                nested,
                body_text(rect(510, 286, 990, 314), "second exact body"),
            ];

            let candidates = extract_whatsapp_body_candidates(&nodes, 0, 4, 128)
                .expect("exact WhatsApp body evidence should be extracted");
            assert_eq!(candidates.len(), 2);
            assert_eq!(candidates[0].node_index, 1);
            assert_eq!(candidates[0].text, "first exact body");
            assert_eq!(candidates[0].body_bounds, rect(510, 226, 970, 252));
            assert_eq!(candidates[1].node_index, 5);
            assert_eq!(candidates[1].text, "second exact body");

            let mut ambiguous = nodes.clone();
            ambiguous[1] = ambiguous_body(rect(510, 226, 970, 252), "looks like a body");
            assert_eq!(
                extract_whatsapp_body_candidates(&ambiguous, 0, 4, 128).map(|value| value.len()),
                Err(WhatsAppSelectorError::Unsupported)
            );

            let mut no_exact = nodes.clone();
            no_exact[1].evidence = WhatsAppNodeEvidence::None;
            no_exact[5].evidence = WhatsAppNodeEvidence::None;
            assert_eq!(
                extract_whatsapp_body_candidates(&no_exact, 0, 4, 128).map(|value| value.len()),
                Err(WhatsAppSelectorError::Unsupported)
            );

            let mut invalid = nodes.clone();
            invalid[5].text = Some("bad\u{0008}body".to_owned());
            assert_eq!(
                extract_whatsapp_body_candidates(&invalid, 0, 4, 128).map(|value| value.len()),
                Err(WhatsAppSelectorError::Invalid)
            );

            assert_eq!(
                extract_whatsapp_body_candidates(&nodes, 0, 1, 128).map(|value| value.len()),
                Err(WhatsAppSelectorError::LimitExceeded)
            );
        }
    }
}
