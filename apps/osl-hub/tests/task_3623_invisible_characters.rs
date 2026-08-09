//! TASK 3623: invisible Unicode must either survive the shared native
//! placement route byte-for-byte in both the marked message and its name, or
//! be refused before that route creates either record.  This uses the same
//! eight supported private-message paths as TASK 3621 and calls the shared
//! 3406 job directly so it exercises the actual empty-before, guarded-paste,
//! exact-readback, and clear protocol.

#[path = "../examples/task_3406_place_text.rs"]
mod shared_place_text;

use shared_place_text::{
    place_read_back_and_clear_guarded, PlacementWindowGuard, PlacementWindowState,
    SharedTextActions,
};

#[derive(Clone, Copy)]
struct MessagePath {
    app: &'static str,
    kind: &'static str,
}

const SUPPORTED_PRIVATE_MESSAGE_PATHS: [MessagePath; 8] = [
    MessagePath {
        app: "discord",
        kind: "direct-message",
    },
    MessagePath {
        app: "telegram",
        kind: "direct-message",
    },
    MessagePath {
        app: "signal",
        kind: "direct-message",
    },
    MessagePath {
        app: "whatsapp",
        kind: "direct-message",
    },
    MessagePath {
        app: "x",
        kind: "direct-message",
    },
    MessagePath {
        app: "instagram",
        kind: "direct-message",
    },
    MessagePath {
        app: "messenger",
        kind: "direct-message",
    },
    MessagePath {
        app: "email",
        kind: "private-message",
    },
];

#[derive(Clone, Copy)]
struct InvisibleCase {
    label: &'static str,
    character: char,
    code_point: &'static str,
}

const INVISIBLE_CASES: [InvisibleCase; 4] = [
    InvisibleCase {
        label: "zero-width-space",
        character: '\u{200B}',
        code_point: "U+200B",
    },
    InvisibleCase {
        label: "byte-order-mark",
        character: '\u{FEFF}',
        code_point: "U+FEFF",
    },
    InvisibleCase {
        label: "left-to-right-mark",
        character: '\u{200E}',
        code_point: "U+200E",
    },
    InvisibleCase {
        label: "right-to-left-mark",
        character: '\u{200F}',
        code_point: "U+200F",
    },
];

#[derive(Clone, Debug, Eq, PartialEq)]
struct DeliveredMessage {
    name: String,
    text: String,
}

struct PrivateMessageActions {
    composer: String,
    marked_name: String,
    delivered_messages: Vec<DeliveredMessage>,
    place_calls: usize,
}

impl PrivateMessageActions {
    fn new(marked_name: String) -> Self {
        Self {
            composer: String::new(),
            marked_name,
            delivered_messages: Vec::new(),
            place_calls: 0,
        }
    }

    fn message_count(&self) -> usize {
        self.delivered_messages.len()
    }

    fn name_count(&self) -> usize {
        self.delivered_messages.len()
    }
}

impl SharedTextActions for PrivateMessageActions {
    fn read_back_text(&mut self) -> Result<String, String> {
        Ok(self.composer.clone())
    }

    fn place_text(&mut self, text: &str) -> Result<(), String> {
        self.place_calls += 1;
        self.composer.clear();
        self.composer.push_str(text);
        self.delivered_messages.push(DeliveredMessage {
            name: self.marked_name.clone(),
            text: text.to_owned(),
        });
        Ok(())
    }

    fn clear_text(&mut self) -> Result<(), String> {
        self.composer.clear();
        Ok(())
    }
}

struct FixedWindowGuard {
    state: PlacementWindowState,
}

impl FixedWindowGuard {
    const fn ready() -> Self {
        Self {
            state: PlacementWindowState {
                app_has_focus: true,
                app_is_covered: false,
                app_is_minimized: false,
                app_display_available: true,
            },
        }
    }

    const fn refusing() -> Self {
        Self {
            state: PlacementWindowState {
                app_has_focus: false,
                app_is_covered: false,
                app_is_minimized: false,
                app_display_available: true,
            },
        }
    }
}

impl PlacementWindowGuard for FixedWindowGuard {
    fn state_before_place(&mut self) -> Result<PlacementWindowState, String> {
        Ok(self.state)
    }
}

fn mark(field: &str, case: InvisibleCase, app: &str) -> String {
    format!("TASK3623-{field}-{}-{}-{app}", case.label, case.character)
}

fn code_points(value: &str) -> String {
    value
        .chars()
        .map(|character| format!("U+{:04X}", character as u32))
        .collect::<Vec<_>>()
        .join(",")
}

#[test]
fn task_3623_every_supported_message_path_preserves_invisible_message_and_name_code_points_or_refuses_before_placement(
) {
    let mut exact_round_trips = 0usize;
    let mut refusal_cases = 0usize;
    let mut refusal_messages_before = 0usize;
    let mut refusal_messages_after = 0usize;
    let mut refusal_names_before = 0usize;
    let mut refusal_names_after = 0usize;

    for path in SUPPORTED_PRIVATE_MESSAGE_PATHS {
        for case in INVISIBLE_CASES {
            let marked_name = mark("name", case, path.app);
            let marked_message = mark("message", case, path.app);
            assert!(marked_name.contains(case.character));
            assert!(marked_message.contains(case.character));

            let mut delivered = PrivateMessageActions::new(marked_name.clone());
            let mut ready = FixedWindowGuard::ready();
            let receipt =
                place_read_back_and_clear_guarded(&mut delivered, &mut ready, &marked_message)
                    .unwrap_or_else(|error| {
                        panic!(
                            "TASK3623 app={} path={} case={} did not round-trip exactly: {error}",
                            path.app, path.kind, case.code_point
                        )
                    });

            assert_eq!(receipt.placed_bytes, marked_message.len());
            assert_eq!(receipt.readback_bytes, marked_message.len());
            assert_eq!(receipt.clear_bytes, 0);
            assert_eq!(delivered.message_count(), 1);
            assert_eq!(delivered.name_count(), 1);
            assert_eq!(
                delivered.delivered_messages,
                [DeliveredMessage {
                    name: marked_name.clone(),
                    text: marked_message.clone(),
                }],
                "TASK3623 app={} path={} case={} message/name changed during placement",
                path.app,
                path.kind,
                case.code_point
            );
            println!(
                "TASK3623_ROUND_TRIP app={} path={} case={} message_code_points={} name_code_points={} messages=1 names=1",
                path.app,
                path.kind,
                case.code_point,
                code_points(&marked_message),
                code_points(&marked_name),
            );
            exact_round_trips += 1;

            let mut refused = PrivateMessageActions::new(marked_name);
            let messages_before = refused.message_count();
            let names_before = refused.name_count();
            let mut refusing = FixedWindowGuard::refusing();
            let refusal =
                place_read_back_and_clear_guarded(&mut refused, &mut refusing, &marked_message)
                    .expect_err("focus loss must refuse before creating a message or name");
            let messages_after = refused.message_count();
            let names_after = refused.name_count();
            assert!(
                refusal.contains("focus changed"),
                "TASK3623 app={} path={} case={} unclear refusal: {refusal}",
                path.app,
                path.kind,
                case.code_point
            );
            assert_eq!(refused.place_calls, 0);
            assert_eq!(messages_before, messages_after);
            assert_eq!(names_before, names_after);
            println!(
                "TASK3623_REFUSED app={} path={} case={} refusal={:?} messages_before={} messages_after={} names_before={} names_after={}",
                path.app,
                path.kind,
                case.code_point,
                refusal,
                messages_before,
                messages_after,
                names_before,
                names_after,
            );
            refusal_cases += 1;
            refusal_messages_before += messages_before;
            refusal_messages_after += messages_after;
            refusal_names_before += names_before;
            refusal_names_after += names_after;
        }
    }

    println!("TASK3623_PATHS={}", SUPPORTED_PRIVATE_MESSAGE_PATHS.len());
    println!("TASK3623_INVISIBLE_CASES={}", INVISIBLE_CASES.len());
    println!("TASK3623_EXACT_ROUND_TRIPS={exact_round_trips}");
    println!("TASK3623_REFUSAL_CASES={refusal_cases}");
    println!("TASK3623_REFUSAL_MESSAGES_BEFORE={refusal_messages_before}");
    println!("TASK3623_REFUSAL_MESSAGES_AFTER={refusal_messages_after}");
    println!("TASK3623_REFUSAL_NAMES_BEFORE={refusal_names_before}");
    println!("TASK3623_REFUSAL_NAMES_AFTER={refusal_names_after}");

    let total_cases = SUPPORTED_PRIVATE_MESSAGE_PATHS.len() * INVISIBLE_CASES.len();
    assert_eq!(total_cases, 32);
    assert_eq!(exact_round_trips, total_cases);
    assert_eq!(refusal_cases, total_cases);
    assert_eq!(refusal_messages_before, 0);
    assert_eq!(refusal_messages_after, 0);
    assert_eq!(refusal_names_before, 0);
    assert_eq!(refusal_names_after, 0);
}
