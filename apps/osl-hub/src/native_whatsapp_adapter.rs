//! Pure structural contracts for WhatsApp's native WebView2 surface.
//!
//! The live Windows accessibility side is allowed to collect only bounded
//! metadata. This module keeps the trust, discovery, and placement decisions
//! deterministic and testable without touching the remote process.

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
pub enum WhatsAppAdapterRefusal {
    MissingExactAppRoot,
    AmbiguousAppRoot,
    MissingWebView2ContentRoot,
    AmbiguousWebView2ContentRoot,
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

fn is_content_root_candidate(node: &WhatsAppStructuralNode) -> bool {
    node.enabled
        && !node.offscreen
        && node.bounds.map_or(false, WhatsAppBounds::is_positive)
        && node.runtime_hash.is_some()
        && matches!(
            node.control_type,
            WhatsAppControlType::Document | WhatsAppControlType::List
        )
        && is_webview2_lineage_node(node)
}

fn is_webview2_lineage_node(node: &WhatsAppStructuralNode) -> bool {
    if node.process_kind == WhatsAppProcessKind::WebView2Child {
        return true;
    }
    node.class_name.as_deref().map_or(false, |class_name| {
        class_name.contains("Chrome_WidgetWin") || class_name.contains("WebView2")
    }) || node.framework_id.as_deref() == Some("Chrome")
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
}
