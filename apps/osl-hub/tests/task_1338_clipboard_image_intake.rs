#![cfg(feature = "core")]

use osl_privacy_hub::native_attachment_jobs::NativeAttachmentStage;
use osl_privacy_hub::native_attachment_jobs_bridge::{
    NativeAttachmentProgressBridge, NativeAttachmentProgressEvent, NativeAttachmentProgressSink,
};
use std::cell::RefCell;
use std::rc::Rc;

const CONTEXT_ID: &str = "chat:task-1338";
const DIRECT_CLIPBOARD_COMMAND: &str = "stage_clipboard_image";
const TAURI_COMMAND: &str = "accept_osl_chat_clipboard_image_attachment";

#[derive(Clone, Default)]
struct RecordingSink {
    events: Rc<RefCell<Vec<NativeAttachmentProgressEvent>>>,
}

impl NativeAttachmentProgressSink for RecordingSink {
    fn emit(&mut self, event: NativeAttachmentProgressEvent) {
        self.events.borrow_mut().push(event);
    }
}

fn png_clipboard_bytes() -> Vec<u8> {
    b"\x89PNG\r\n\x1a\nTASK1338".to_vec()
}

fn stage_name(stage: NativeAttachmentStage) -> &'static str {
    match stage {
        NativeAttachmentStage::Selected => "selected",
        NativeAttachmentStage::Protecting => "protecting",
        NativeAttachmentStage::Uploading => "uploading",
        NativeAttachmentStage::Delivering => "delivering",
        NativeAttachmentStage::Sent => "sent",
        NativeAttachmentStage::Failed => "failed",
        NativeAttachmentStage::Cancelled => "cancelled",
    }
}

fn assert_shipping_command_registered_and_granted() {
    let main = include_str!("../src/main.rs");
    let handlers = include_str!("../src/hub_command_surface.rs");
    let permissions = include_str!("../permissions/hub.toml");
    let capability = include_str!("../capabilities/hub.json");
    let permission = "allow-accept-osl-chat-clipboard-image-attachment";

    assert!(main.contains(&format!("fn {TAURI_COMMAND}(")));
    assert!(handlers.contains(TAURI_COMMAND));
    assert!(permissions.contains(&format!("commands.allow = [\"{TAURI_COMMAND}\"]")));
    assert!(capability.contains(&format!("\"{permission}\"")));
}

#[test]
fn direct_clipboard_command_creates_one_image_attachment_card() {
    assert_shipping_command_registered_and_granted();

    let sink = RecordingSink::default();
    let events = sink.events.clone();
    let mut bridge = NativeAttachmentProgressBridge::new(sink);
    let card = bridge
        .stage_clipboard_image(CONTEXT_ID, "image/png", png_clipboard_bytes(), 1_338_000)
        .expect("direct clipboard image command creates an attachment card");
    let events = events.borrow();
    let image_attachment_cards = events
        .iter()
        .filter(|event| event.job.metadata.media_type.starts_with("image/"))
        .count();

    println!("TASK1338_TAURI_COMMAND={TAURI_COMMAND}");
    println!("TASK1338_DIRECT_CLIPBOARD_COMMAND={DIRECT_CLIPBOARD_COMMAND}");
    println!("TASK1338_ATTACHMENT_CARD_COUNT={}", events.len());
    println!("TASK1338_IMAGE_ATTACHMENT_CARD_COUNT={image_attachment_cards}");
    println!("TASK1338_CARD_CONTEXT_ID={}", events[0].context_id);
    println!("TASK1338_CARD_FILENAME={}", card.metadata.filename);
    println!("TASK1338_CARD_MEDIA_TYPE={}", card.metadata.media_type);
    println!("TASK1338_CARD_STAGE={}", stage_name(card.stage));

    assert_eq!(events.len(), 1);
    assert_eq!(image_attachment_cards, 1);
    assert_eq!(events[0].context_id, CONTEXT_ID);
    assert_eq!(events[0].job, card);
    assert_eq!(card.metadata.filename, "clipboard-image.png");
    assert_eq!(card.metadata.media_type, "image/png");
    assert_eq!(card.stage, NativeAttachmentStage::Selected);
}
