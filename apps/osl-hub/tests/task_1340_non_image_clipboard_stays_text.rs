#![cfg(feature = "core")]

//! TASK 1340 - non-image clipboard content stays text.
//!
//! A paste can only become an attachment or a message by going through one of
//! the two shipping clipboard-intake surfaces:
//!
//! * `broker::cmd_clipboard_image_attachment_tray` -- behind the
//!   `intake_osl_chat_clipboard_image` command; it is the thing that mints a
//!   `peer-` message id and puts a record in the send tray.
//! * `NativeAttachmentJobRegistry::stage_clipboard_image` -- behind the
//!   `accept_osl_chat_clipboard_image_attachment` command (TASK 1338); it is
//!   the thing that emits the `osl://attachment-progress` card the composer
//!   renders before Send.
//!
//! This check pastes every payload in a fixture at BOTH surfaces and requires
//! that the non-image ones come back refused, having produced no attachment
//! card, no tray record, no minted id, and no change to their own bytes -- the
//! pasted text is still exactly the text that was copied.
//!
//! The payload list is a fixture, not a literal, so the check can be aimed
//! elsewhere with `TASK_1340_CLIPBOARD_FIXTURE=<path>`. A fixture carrying no
//! non-image clipboard content fails the check instead of passing vacuously:
//! a green run over images alone would say nothing about non-image content.
//!
//! # Not executed on this branch
//!
//! `osl-hub` does not compile on `lane/d`: `broker.rs`, `security.rs`,
//! `main.rs` and ~20 other modules carry interleaved merge damage (two lane
//! versions of the same function spliced together), which predates TASK 1340
//! and is unrelated to the clipboard. `cargo check -p osl-hub
//! --no-default-features --features core --lib` fails at the parser, so this
//! target has never been built or run. The finish line was measured instead by
//! the renderer-side check
//! `apps/osl-hub-ui/src/task_1340_non_image_clipboard_stays_text.test.ts`,
//! which drives the real composer over the same fixture. This file is the
//! same check one layer down, ready for the first build after that damage is
//! repaired.

use osl_privacy_hub::broker;
use osl_privacy_hub::native_attachment_jobs::{
    NativeAttachmentJobError, NativeAttachmentJobRegistry, NativeAttachmentStage,
};
use osl_privacy_hub::native_attachment_jobs_bridge::{
    NativeAttachmentProgressBridge, NativeAttachmentProgressEvent, NativeAttachmentProgressSink,
};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

const CONTEXT_ID: &str = "chat:task-1340";
const FIXTURE_ENV: &str = "TASK_1340_CLIPBOARD_FIXTURE";
/// Same fixture the runnable renderer-side check reads, so the two cannot drift.
const DEFAULT_FIXTURE: &str = "../osl-hub-ui/src/fixtures/task_1340_clipboard_payloads.json";
/// The exact user-facing refusal `cmd_clipboard_image_attachment_tray` returns.
const TRAY_REFUSAL: &str = "OSL could not read the pasted clipboard image";
/// The shipping command that carries composer text. Text belongs to this
/// command, never to either clipboard-image command.
const TEXT_COMMAND: &str = "prepare_osl_chat_text";
const IMAGE_COMMANDS: [&str; 2] = [
    "accept_osl_chat_clipboard_image_attachment",
    "intake_osl_chat_clipboard_image",
];

#[derive(Clone, Default)]
struct RecordingSink {
    events: Rc<RefCell<Vec<NativeAttachmentProgressEvent>>>,
}

impl NativeAttachmentProgressSink for RecordingSink {
    fn emit(&mut self, event: NativeAttachmentProgressEvent) {
        self.events.borrow_mut().push(event);
    }
}

#[derive(Clone, Debug)]
struct ClipboardPayload {
    name: String,
    kind: String,
    media_type: String,
    bytes: Vec<u8>,
    /// `Some` only for `kind == "text"`: the exact string that was copied.
    text: Option<String>,
}

impl ClipboardPayload {
    fn is_image(&self) -> bool {
        self.kind == "image"
    }
}

fn fixture_path() -> PathBuf {
    match std::env::var(FIXTURE_ENV) {
        Ok(value) if !value.trim().is_empty() => PathBuf::from(value),
        _ => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DEFAULT_FIXTURE),
    }
}

fn decode_hex(name: &str, hex: &str) -> Vec<u8> {
    assert!(
        hex.len() % 2 == 0,
        "clipboard payload {name}: bytes_hex must have an even length"
    );
    (0..hex.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&hex[index..index + 2], 16)
                .unwrap_or_else(|_| panic!("clipboard payload {name}: bytes_hex is not hex"))
        })
        .collect()
}

fn load_payloads(path: &PathBuf) -> Vec<ClipboardPayload> {
    let raw = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("clipboard fixture {} is unreadable: {error}", path.display()));
    let document: serde_json::Value = serde_json::from_str(&raw)
        .unwrap_or_else(|error| panic!("clipboard fixture {} is not JSON: {error}", path.display()));
    let entries = document
        .get("payloads")
        .and_then(serde_json::Value::as_array)
        .unwrap_or_else(|| {
            panic!(
                "clipboard fixture {} has no `payloads` array",
                path.display()
            )
        });

    entries
        .iter()
        .map(|entry| {
            let field = |key: &str| -> String {
                entry
                    .get(key)
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_else(|| {
                        panic!(
                            "clipboard fixture {} has a payload without a `{key}` string",
                            path.display()
                        )
                    })
                    .to_owned()
            };
            let name = field("name");
            let kind = field("kind");
            assert!(
                matches!(kind.as_str(), "text" | "object" | "image"),
                "clipboard payload {name}: kind must be text, object or image (got {kind})"
            );
            let media_type = field("media_type");
            let text = entry
                .get("text")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned);
            let bytes = match (&text, entry.get("bytes_hex").and_then(serde_json::Value::as_str)) {
                (Some(text), None) => text.as_bytes().to_vec(),
                (None, Some(hex)) => decode_hex(&name, hex),
                _ => panic!(
                    "clipboard payload {name}: give exactly one of `text` or `bytes_hex`"
                ),
            };
            assert!(
                !bytes.is_empty(),
                "clipboard payload {name}: an empty clipboard payload is not a paste"
            );
            if kind == "text" {
                assert!(
                    text.is_some(),
                    "clipboard payload {name}: a text payload must carry its `text`"
                );
            }
            ClipboardPayload {
                name,
                kind,
                media_type,
                bytes,
                text,
            }
        })
        .collect()
}

/// The anti-vacuity gate. A run that never pasted anything non-image cannot
/// have shown that non-image clipboard content stays text, so it fails here
/// rather than reporting green.
fn require_non_image_clipboard_content(path: &PathBuf, payloads: &[ClipboardPayload]) {
    let text_payloads = payloads.iter().filter(|p| p.kind == "text").count();
    let object_payloads = payloads.iter().filter(|p| p.kind == "object").count();

    println!("TASK1340_FIXTURE={}", path.display());
    println!("TASK1340_FIXTURE_PAYLOADS={}", payloads.len());
    println!("TASK1340_FIXTURE_TEXT_PAYLOADS={text_payloads}");
    println!("TASK1340_FIXTURE_UNSUPPORTED_OBJECT_PAYLOADS={object_payloads}");

    assert!(
        text_payloads >= 1,
        "clipboard fixture {} carries no plain-text payload: a check that never pasted text \
         cannot show that text stays text",
        path.display()
    );
    assert!(
        object_payloads >= 1,
        "clipboard fixture {} carries no unsupported clipboard object: a check that never \
         pasted one cannot show it creates no attachment or message",
        path.display()
    );
    for payload in payloads.iter().filter(|p| !p.is_image()) {
        assert!(
            !payload.media_type.starts_with("image/"),
            "clipboard payload {} claims kind {} but carries an image media type {}",
            payload.name,
            payload.kind,
            payload.media_type
        );
    }
}

/// Text and unsupported objects are routed by the text command, never by either
/// clipboard-image command: neither image intake table admits a non-image media
/// type. Read out of the shipping sources, the same way TASK 1338 proves its
/// command is registered.
fn assert_text_is_not_a_clipboard_image_command(payloads: &[ClipboardPayload]) {
    let broker_source = include_str!("../src/broker.rs");
    let registry_source = include_str!("../src/native_attachment_jobs.rs");
    let command_surface = include_str!("../src/hub_command_surface.rs");

    assert!(
        command_surface.contains(TEXT_COMMAND),
        "the shipping text command {TEXT_COMMAND} must exist for pasted text to stay text"
    );
    for command in IMAGE_COMMANDS {
        assert!(
            command_surface.contains(command),
            "clipboard image command {command} must be registered"
        );
    }
    for payload in payloads.iter().filter(|p| !p.is_image()) {
        let admitted = format!("\"{}\" =>", payload.media_type);
        assert!(
            !broker_source.contains(&admitted),
            "{} must not be an accepted clipboard-image media type in broker.rs",
            payload.media_type
        );
        assert!(
            !registry_source.contains(&admitted),
            "{} must not be an accepted clipboard-image media type in native_attachment_jobs.rs",
            payload.media_type
        );
    }
    println!("TASK1340_TEXT_COMMAND={TEXT_COMMAND}");
    println!("TASK1340_IMAGE_COMMANDS={}", IMAGE_COMMANDS.join(","));
}

#[test]
fn non_image_clipboard_content_stays_text_and_creates_nothing() {
    let path = fixture_path();
    let payloads = load_payloads(&path);
    require_non_image_clipboard_content(&path, &payloads);
    assert_text_is_not_a_clipboard_image_command(&payloads);

    let sink = RecordingSink::default();
    let events = sink.events.clone();
    let mut bridge = NativeAttachmentProgressBridge::new(sink);
    let mut registry = NativeAttachmentJobRegistry::default();

    let mut pasted = 0_usize;
    let mut tray_records = 0_usize;
    let mut minted_ids = 0_usize;
    let mut text_unchanged = 0_usize;
    let mut now_ms = 1_340_000_u64;

    for payload in payloads.iter().filter(|p| !p.is_image()) {
        pasted += 1;
        now_ms += 1;
        let original = payload.bytes.clone();

        // Surface 1: the tray/message side. It is the only path that mints a
        // peer message id for a pasted payload.
        match broker::cmd_clipboard_image_attachment_tray(&payload.bytes, &payload.media_type, false)
        {
            Ok(result) => {
                tray_records += result.records.len();
                minted_ids += result.records.len();
                panic!(
                    "pasting {} ({}) built {} tray record(s): a non-image paste must create no \
                     attachment and no message",
                    payload.name,
                    payload.media_type,
                    result.tray_count
                );
            }
            Err(error) => assert_eq!(
                error, TRAY_REFUSAL,
                "pasting {} ({}) must be refused with the user-facing clipboard refusal",
                payload.name, payload.media_type
            ),
        }

        // Surface 2: the attachment-card side, driven through the same bridge
        // the renderer listens to.
        let bridged = bridge.stage_clipboard_image(
            CONTEXT_ID,
            &payload.media_type,
            payload.bytes.clone(),
            now_ms,
        );
        assert_eq!(
            bridged.err(),
            Some(NativeAttachmentJobError::InvalidMediaType),
            "pasting {} ({}) must not stage an attachment card",
            payload.name,
            payload.media_type
        );

        let staged =
            registry.stage_clipboard_image(CONTEXT_ID, &payload.media_type, payload.bytes.clone(), now_ms);
        assert_eq!(
            staged.err(),
            Some(NativeAttachmentJobError::InvalidMediaType),
            "pasting {} ({}) must not create a job",
            payload.name,
            payload.media_type
        );
        assert!(
            registry.snapshot(CONTEXT_ID).is_none(),
            "pasting {} left a staged attachment job behind",
            payload.name
        );
        assert!(
            registry.query_tray_records(CONTEXT_ID).is_empty(),
            "pasting {} left a send-tray record behind",
            payload.name
        );

        // The payload itself is untouched by the refused intake.
        assert_eq!(
            payload.bytes, original,
            "pasting {} rewrote the clipboard bytes",
            payload.name
        );
        if let Some(text) = payload.text.as_ref() {
            let still_text = std::str::from_utf8(&payload.bytes).unwrap_or_else(|error| {
                panic!("pasted text {} stopped being text: {error}", payload.name)
            });
            assert_eq!(
                still_text, text,
                "pasted text {} did not stay byte-for-byte the copied text",
                payload.name
            );
            text_unchanged += 1;
            println!(
                "TASK1340_TEXT_STAYS_TEXT name={} media_type={} bytes={} text={still_text}",
                payload.name,
                payload.media_type,
                payload.bytes.len()
            );
        } else {
            println!(
                "TASK1340_UNSUPPORTED_OBJECT name={} media_type={} bytes={} attachment=none message=none",
                payload.name,
                payload.media_type,
                payload.bytes.len()
            );
        }
    }

    let cards = events.borrow().len();
    println!("TASK1340_NON_IMAGE_PASTES={pasted}");
    println!("TASK1340_ATTACHMENT_CARDS={cards}");
    println!("TASK1340_TRAY_RECORDS={tray_records}");
    println!("TASK1340_MINTED_MESSAGE_IDS={minted_ids}");
    println!("TASK1340_TEXT_PAYLOADS_UNCHANGED={text_unchanged}");

    assert_eq!(cards, 0, "a non-image paste must show no attachment card");
    assert_eq!(tray_records, 0, "a non-image paste must add no tray record");
    assert_eq!(minted_ids, 0, "a non-image paste must mint no message id");
    assert_eq!(
        text_unchanged,
        payloads.iter().filter(|p| p.kind == "text").count(),
        "every text payload must come back as the same text"
    );
}

/// Positive control: the very same surfaces, sink and fixture loader do show a
/// card and a tray record for an image paste. Without this, "zero cards" above
/// could just mean the check never reaches the intake at all.
#[test]
fn an_image_paste_from_the_same_fixture_still_creates_one_card() {
    let path = fixture_path();
    let payloads = load_payloads(&path);
    let images: Vec<&ClipboardPayload> = payloads.iter().filter(|p| p.is_image()).collect();
    assert!(
        !images.is_empty(),
        "clipboard fixture {} carries no image payload, so the check cannot show it can say yes",
        path.display()
    );

    let mut now_ms = 1_341_000_u64;
    for image in images {
        now_ms += 1;
        let sink = RecordingSink::default();
        let events = sink.events.clone();
        let mut bridge = NativeAttachmentProgressBridge::new(sink);
        let card = bridge
            .stage_clipboard_image(CONTEXT_ID, &image.media_type, image.bytes.clone(), now_ms)
            .unwrap_or_else(|error| panic!("image paste {} was refused: {error}", image.name));
        let tray =
            broker::cmd_clipboard_image_attachment_tray(&image.bytes, &image.media_type, false)
                .unwrap_or_else(|error| panic!("image paste {} built no tray record: {error}", image.name));

        println!(
            "TASK1340_IMAGE_CONTROL name={} media_type={} cards={} tray_count={} filename={} stage={:?}",
            image.name,
            image.media_type,
            events.borrow().len(),
            tray.tray_count,
            card.metadata.filename,
            card.stage
        );

        assert_eq!(events.borrow().len(), 1);
        assert_eq!(tray.tray_count, 1);
        assert_eq!(card.stage, NativeAttachmentStage::Selected);
        assert_eq!(card.metadata.media_type, image.media_type);
    }
}
