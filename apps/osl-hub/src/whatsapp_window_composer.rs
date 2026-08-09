//! Hermetic WhatsApp active-window, direct-conversation, and typing-box finder.
//!
//! The fixture command exercises the same structural selectors used by the
//! native adapter.  It deliberately releases no result for a group, login, or
//! search surface: a writable `Edit` alone is never a WhatsApp typing box.

use crate::native_whatsapp_adapter::{
    discover_whatsapp_pair, WhatsAppAdapterRefusal, WhatsAppBounds, WhatsAppControlType,
    WhatsAppProcessKind, WhatsAppStructuralNode, WHATSAPP_ROOT_WINDOW_CLASS,
};

const WHATSAPP_PACKAGE_FAMILY: &str = "5319275A.WhatsAppDesktop_cv1g1gvanyjgm";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WhatsAppConversationKind {
    DirectMessage,
    Group,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActiveWhatsAppWindow {
    pub title: String,
    pub active: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActiveWhatsAppConversation {
    pub title: String,
    pub kind: WhatsAppConversationKind,
    pub selected: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FoundWhatsAppDirectMessage {
    pub window: ActiveWhatsAppWindow,
    pub conversation: ActiveWhatsAppConversation,
    pub typing_box_name: String,
}

#[derive(Clone, Debug)]
pub struct WhatsAppFinderSnapshot {
    pub window: ActiveWhatsAppWindow,
    pub conversation: ActiveWhatsAppConversation,
    pub nodes: Vec<WhatsAppStructuralNode>,
}

/// Finds all three controls only when the active WhatsApp surface is a selected
/// one-to-one conversation with exactly one adapter-approved composer.
pub fn find_active_whatsapp_direct_message(
    snapshot: &WhatsAppFinderSnapshot,
) -> Result<FoundWhatsAppDirectMessage, WhatsAppAdapterRefusal> {
    if !snapshot.window.active || snapshot.window.title != "WhatsApp" {
        return Err(WhatsAppAdapterRefusal::MissingExactAppRoot);
    }
    if !snapshot.conversation.selected
        || snapshot.conversation.kind != WhatsAppConversationKind::DirectMessage
    {
        return Err(WhatsAppAdapterRefusal::MissingTranscript);
    }
    let pair = discover_whatsapp_pair(&snapshot.nodes)?;
    let typing_box = snapshot
        .nodes
        .iter()
        .find(|node| node.structural_id == pair.composer.node_id)
        .and_then(|node| node.automation_id.as_deref())
        .ok_or(WhatsAppAdapterRefusal::MissingComposer)?;
    Ok(FoundWhatsAppDirectMessage {
        window: snapshot.window.clone(),
        conversation: snapshot.conversation.clone(),
        typing_box_name: typing_box.to_owned(),
    })
}

pub fn render_prepared_whatsapp_fixture(value: &str) -> Result<String, String> {
    let snapshot = match value {
        "whatsapp-direct" => prepared_direct_snapshot(),
        "whatsapp-signed-out" => prepared_signed_out_snapshot(),
        _ => {
            return Err(
                "usage: whatsapp-window-composer <whatsapp-direct|whatsapp-signed-out>".to_owned(),
            )
        }
    };
    let found = find_active_whatsapp_direct_message(&snapshot)
        .map_err(|error| format!("WhatsApp direct-message finder refused: {error:?}"))?;
    Ok(format!(
        "TASK1060_WINDOW={}\nTASK1060_CONVERSATION={}\nTASK1060_TYPING_BOX={}\nTASK1060_FOUND_COUNT=3\n",
        found.window.title, found.conversation.title, found.typing_box_name
    ))
}

fn node(
    id: &str,
    parent: Option<&str>,
    process_kind: WhatsAppProcessKind,
    control_type: WhatsAppControlType,
    automation_id: Option<&str>,
) -> WhatsAppStructuralNode {
    WhatsAppStructuralNode {
        structural_id: id.to_owned(),
        parent_structural_id: parent.map(str::to_owned),
        process_kind,
        store_package_family_name: None,
        window_class: None,
        control_type,
        automation_id: automation_id.map(str::to_owned),
        class_name: None,
        framework_id: None,
        runtime_hash: Some(format!("runtime-{id}")),
        bounds: Some(WhatsAppBounds {
            x: 0,
            y: 0,
            width: 1280,
            height: 720,
        }),
        enabled: true,
        offscreen: false,
    }
}

fn prepared_direct_snapshot() -> WhatsAppFinderSnapshot {
    let mut root = node(
        "app-root",
        None,
        WhatsAppProcessKind::StoreAppRoot,
        WhatsAppControlType::Window,
        None,
    );
    root.store_package_family_name = Some(WHATSAPP_PACKAGE_FAMILY.to_owned());
    root.window_class = Some(WHATSAPP_ROOT_WINDOW_CLASS.to_owned());
    let webview = node(
        "webview",
        Some("app-root"),
        WhatsAppProcessKind::WebView2Child,
        WhatsAppControlType::Pane,
        None,
    );
    let content = node(
        "content",
        Some("webview"),
        WhatsAppProcessKind::WebView2Child,
        WhatsAppControlType::Document,
        None,
    );
    let transcript = node(
        "conversation",
        Some("content"),
        WhatsAppProcessKind::Other,
        WhatsAppControlType::List,
        Some("message-list"),
    );
    let composer = node(
        "typing-box",
        Some("content"),
        WhatsAppProcessKind::Other,
        WhatsAppControlType::Edit,
        Some("Type a message"),
    );
    WhatsAppFinderSnapshot {
        window: ActiveWhatsAppWindow {
            title: "WhatsApp".to_owned(),
            active: true,
        },
        conversation: ActiveWhatsAppConversation {
            title: "OSL QA Peer".to_owned(),
            kind: WhatsAppConversationKind::DirectMessage,
            selected: true,
        },
        nodes: vec![root, webview, content, transcript, composer],
    }
}

fn prepared_signed_out_snapshot() -> WhatsAppFinderSnapshot {
    let mut snapshot = prepared_direct_snapshot();
    snapshot.conversation.selected = false;
    snapshot
        .nodes
        .retain(|node| node.structural_id != "conversation");
    let composer = snapshot
        .nodes
        .iter_mut()
        .find(|node| node.structural_id == "typing-box")
        .expect("prepared typing box exists");
    composer.automation_id = Some("Phone number".to_owned());
    snapshot
}
