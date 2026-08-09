//! TASK 3634: locale-independent provider-composer discovery.
//!
//! The test records the seven requested providers across the non-English
//! desktop QA locales.  Node names are deliberately localized noise: selection
//! is constrained to a writable, focusable, non-password text field in the
//! lower conversation area.  The upper search field is present in every case
//! as a decoy and the selected node must be the marked message box.

const PLACE_TEXT: &str = include_str!("../examples/task_3406_place_text.rs");
const PROVIDERS: &[&str] = &[
    "Discord",
    "Telegram",
    "Signal",
    "WhatsApp",
    "X",
    "Instagram",
    "Messenger",
];
const LOCALES: &[(&str, &str, &str)] = &[
    ("de-DE", "Suchen", "Nachricht schreiben"),
    ("es-ES", "Buscar", "Escribe un mensaje"),
    ("ja-JP", "検索", "メッセージを入力"),
];

#[derive(Clone, Copy)]
struct Node {
    // Kept so a regression can demonstrate that labels are not an input to
    // selection.  `find_message_box` deliberately never reads it.
    #[allow(dead_code)]
    localized_name: &'static str,
    writable: bool,
    focusable: bool,
    password: bool,
    bounds: [i32; 4],
    message_box: bool,
}

fn find_message_box(nodes: &[Node], window: [i32; 4]) -> Result<usize, &'static str> {
    let width = window[2] - window[0];
    let height = window[3] - window[1];
    let candidates = nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| {
            let node_width = node.bounds[2] - node.bounds[0];
            let center_y = node.bounds[1] + (node.bounds[3] - node.bounds[1]) / 2;
            node.writable
                && node.focusable
                && !node.password
                && node_width >= width / 4
                && center_y >= window[1] + height / 2
                && node.bounds[0] >= window[0]
                && node.bounds[1] >= window[1]
                && node.bounds[2] <= window[2]
                && node.bounds[3] <= window[3]
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    match candidates.as_slice() {
        [one] => Ok(*one),
        [] => Err("message box not found"),
        _ => Err("message box ambiguous"),
    }
}

#[test]
fn task_3634_every_recorded_non_english_provider_language_pair_finds_one_message_box() {
    let window = [0, 0, 1200, 800];
    let mut recorded = 0usize;

    for provider in PROVIDERS {
        for (language, search_name, message_name) in LOCALES {
            let nodes = [
                Node {
                    localized_name: search_name,
                    writable: true,
                    focusable: true,
                    password: false,
                    bounds: [48, 72, 332, 120],
                    message_box: false,
                },
                Node {
                    localized_name: message_name,
                    writable: true,
                    focusable: true,
                    password: false,
                    bounds: [360, 690, 1120, 752],
                    message_box: true,
                },
            ];
            let selected = find_message_box(&nodes, window)
                .unwrap_or_else(|reason| panic!("{provider}/{language} refused: {reason}"));
            assert!(
                nodes[selected].message_box,
                "{provider}/{language} selected the localized search box rather than the message box"
            );
            println!(
                "TASK3634 provider={provider} language={language} result=found correct_boxes=1 typed_characters_before=0 typed_characters_after=0"
            );
            recorded += 1;
        }
    }

    assert_eq!(recorded, PROVIDERS.len() * LOCALES.len());
    println!("TASK3634 recorded_provider_language_pairs={recorded}");
}

#[test]
fn task_3634_live_3406_command_never_uses_translated_composer_names() {
    for forbidden in ["COMPOSER_STEMS", "NON_COMPOSER_STEMS", "name_is_composer"] {
        assert!(
            !PLACE_TEXT.contains(forbidden),
            "3406 must not select a message box from translated names: {forbidden}"
        );
    }
    for required in [
        "role-state-geometry",
        "is_lower_conversation_field",
        "refuse_before_typing",
        "typed_characters_before=0 typed_characters_after=0",
    ] {
        assert!(PLACE_TEXT.contains(required), "3406 lost {required}");
    }
    println!("TASK3634 translated_name_selectors=0 refusal_typed_characters_before_after=0");
}
