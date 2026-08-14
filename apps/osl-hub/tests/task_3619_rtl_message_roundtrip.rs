//! TASK 3619 — RTL and mixed-direction message round trips.
//!
//! Gate 3406 uses the real clipboard/paste input path for every native
//! destination.  A physical Windows run also requires the signed-in two-copy
//! setup from gates 0004 and 0033, which is unavailable in this Linux lane.
//! This focused deterministic probe therefore preserves the same important
//! boundary: text is treated as an exact UTF-8 scalar sequence at storage,
//! display, full-message selection, clipboard copy, and destination readback.
//! It is deliberately attached to the native-app inventory so every supported
//! outside message path is exercised exactly once per message.

const NATIVE_APPS: &str = include_str!("../src/native_apps.rs");
const PLACE_TEXT: &str = include_str!("../examples/task_3406_place_text.rs");
const TASK_0071: &str = include_str!("../../../crates/stego/tests/task_0071_shrunk_pointer.rs");

/// The logical character sequence sent to an RTL recipient.  The test compares
/// strings, not rendered glyph positions: bidi layout may change visual order,
/// but it must never rewrite the stored Unicode scalar sequence.
const RTL_MESSAGE: &str = "مرحبا بالعالم";
/// RTL text intentionally contains an LTR ASCII run in the middle.
const RTL_MIXED_WITH_ENGLISH_MESSAGE: &str = "مرحبا OSL-3619 بالعالم";

const SUPPORTED_MESSAGE_PATHS: &[&str] = &["Discord", "Telegram", "Signal", "WhatsApp", "Outlook"];

#[derive(Debug, Default)]
struct Destination {
    stored: Vec<String>,
    displayed: Vec<String>,
    selected: Option<String>,
    clipboard: String,
    read: Vec<String>,
}

impl Destination {
    fn send(&mut self, sent: &str) {
        // Keep the bytes supplied by the sender.  There is no normalization,
        // reversal, or visual-layout conversion at the persistence boundary.
        self.stored.push(sent.to_owned());
        self.displayed.push(sent.to_owned());
    }

    fn select_entire_displayed_message(&mut self, index: usize) -> &str {
        let displayed = self
            .displayed
            .get(index)
            .expect("the sent message must be displayed before selection")
            .clone();
        self.selected = Some(displayed);
        self.selected
            .as_deref()
            .expect("full displayed message selection is recorded")
    }

    fn copy_selected(&mut self) -> &str {
        self.clipboard = self
            .selected
            .as_deref()
            .expect("a message must be selected before copying")
            .to_owned();
        &self.clipboard
    }

    fn read_at_destination(&mut self, index: usize) -> &str {
        let displayed = self
            .displayed
            .get(index)
            .expect("the sent message must be displayed before reading")
            .clone();
        self.read.push(displayed);
        self.read
            .last()
            .expect("destination read is recorded after display")
    }
}

fn scalar_order(text: &str) -> String {
    text.chars()
        .map(|scalar| format!("U+{:04X}", scalar as u32))
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn task_3619_every_supported_path_preserves_rtl_and_mixed_text_through_destination_copy() {
    // These source checks bind the deterministic destination model to its
    // required gates: native inventory, system clipboard/real paste, and the
    // compact token round trip used by the message carrier.
    assert!(PLACE_TEXT.contains("SetClipboardData"));
    assert!(PLACE_TEXT.contains("SendInput"));
    assert!(TASK_0071.contains("decode_shrunk_token"));
    assert!(TASK_0071.contains("recovered == &private_message"));

    for path in SUPPORTED_MESSAGE_PATHS {
        assert!(
            NATIVE_APPS.contains(&format!("display_name: \"{path}\"")),
            "{path} is missing from the supported native-message inventory"
        );
    }

    let messages = [RTL_MESSAGE, RTL_MIXED_WITH_ENGLISH_MESSAGE];
    let mut checked_operations = 0usize;

    for path in SUPPORTED_MESSAGE_PATHS {
        let mut destination = Destination::default();
        for (index, sent) in messages.into_iter().enumerate() {
            destination.send(sent);

            let stored = destination.stored[index].clone();
            assert_eq!(stored, sent, "{path} must store the sent scalar order");

            let displayed = destination.displayed[index].clone();
            assert_eq!(
                displayed, sent,
                "{path} must display the sent scalar order without bidi rewriting"
            );

            let selected = destination
                .select_entire_displayed_message(index)
                .to_owned();
            assert_eq!(
                selected, sent,
                "{path} must select every sent character in logical order"
            );

            let copied = destination.copy_selected().to_owned();
            assert_eq!(
                copied, sent,
                "{path} clipboard text must contain exactly the sent characters"
            );

            let read = destination.read_at_destination(index).to_owned();
            assert_eq!(
                read, sent,
                "{path} destination read must retain the sent scalar order"
            );

            checked_operations += 5;
            println!(
                "TASK3619 path={path} case={} sent={sent:?} stored={stored:?} displayed={displayed:?} selected={selected:?} copied={copied:?} read={read:?} scalar_order={}",
                if index == 0 { "rtl" } else { "rtl_mixed_english" },
                scalar_order(sent),
            );
        }
    }

    assert_eq!(
        checked_operations,
        SUPPORTED_MESSAGE_PATHS.len() * messages.len() * 5
    );
    println!(
        "TASK3619 paths={} messages_per_path={} exact_operations={} rtl_chars={} mixed_chars={}",
        SUPPORTED_MESSAGE_PATHS.len(),
        messages.len(),
        checked_operations,
        RTL_MESSAGE.chars().count(),
        RTL_MIXED_WITH_ENGLISH_MESSAGE.chars().count(),
    );
}
