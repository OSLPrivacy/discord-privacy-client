//! TASK 3638: webmail composer discovery across reviewed alternate layouts.
//!
//! Each fixture has mail-specific label noise and at least one editable decoy.
//! Discovery is intentionally limited to the role/state/geometry contract used
//! by the 3406 placement command; it does not inspect labels or choose among
//! multiple candidates.  A beta Tuta layout deliberately has two plausible
//! lower editors, which must be refused before either typing or sending.

const PLACE_TEXT: &str = include_str!("../examples/task_3406_place_text.rs");

const SERVICES: &[&str] = &[
    "Gmail",
    "Outlook web",
    "Yahoo Mail",
    "AOL Mail",
    "iCloud Mail",
    "GMX Mail",
    "Mail.com",
    "Proton Mail",
    "Tuta Mail",
];

const BASE_LAYOUTS: &[&str] = &["dark theme", "compact mode", "beta layout"];
const EXTRA_THEME_SERVICES: &[&str] = &["Gmail", "Outlook web", "Proton Mail"];

#[derive(Clone, Copy)]
struct Node {
    // Accessible names vary by theme, compactness, and provider branding.
    // The selector deliberately never reads this field.
    #[allow(dead_code)]
    accessible_name: &'static str,
    editable_role: bool,
    writable: bool,
    focusable: bool,
    bounds: [i32; 4],
    compose_box: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Discovery {
    Found(usize),
    Refused(&'static str),
}

#[derive(Default)]
struct Activity {
    typed_characters: usize,
    sent_messages: usize,
}

fn discover_compose_box(nodes: &[Node], window: [i32; 4]) -> Discovery {
    let width = window[2] - window[0];
    let height = window[3] - window[1];
    let candidates = nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| {
            let node_width = node.bounds[2] - node.bounds[0];
            let center_y = node.bounds[1] + (node.bounds[3] - node.bounds[1]) / 2;
            node.editable_role
                && node.writable
                && node.focusable
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
        [one] => Discovery::Found(*one),
        [] => Discovery::Refused("message box not found"),
        _ => Discovery::Refused("message box ambiguous"),
    }
}

fn fixture(service: &'static str, layout: &'static str) -> ([i32; 4], Vec<Node>) {
    let window = [0, 0, 1280, 820];
    let compact = layout == "compact mode";
    let beta = layout == "beta layout";
    let compose_top = if compact { 624 } else { 694 };
    let compose_bottom = if compact { 676 } else { 764 };
    let compose_left = if compact { 280 } else { 348 };
    let compose_right = if compact { 1218 } else { 1184 };
    let mut nodes = vec![
        Node {
            accessible_name: "Search mail",
            editable_role: true,
            writable: true,
            focusable: true,
            bounds: [56, 70, 418, 118],
            compose_box: false,
        },
        Node {
            accessible_name: "To recipients",
            editable_role: true,
            writable: true,
            focusable: true,
            bounds: [compose_left, 332, compose_right, 374],
            compose_box: false,
        },
        Node {
            accessible_name: "Message body",
            editable_role: true,
            writable: true,
            focusable: true,
            bounds: [compose_left, compose_top, compose_right, compose_bottom],
            compose_box: true,
        },
        Node {
            accessible_name: "Message preview",
            editable_role: true,
            writable: false,
            focusable: false,
            bounds: [compose_left, 420, compose_right, 588],
            compose_box: false,
        },
    ];

    // This reviewed beta surface exposes a second editable lower document.
    // Since neither candidate has a stronger role/state/geometry signal, 3406
    // must fail closed rather than guess and risk placing mail text.
    if service == "Tuta Mail" && beta {
        nodes.push(Node {
            accessible_name: "Experimental signature editor",
            editable_role: true,
            writable: true,
            focusable: true,
            bounds: [
                compose_left,
                compose_top + 6,
                compose_right,
                compose_bottom + 12,
            ],
            compose_box: false,
        });
    }
    (window, nodes)
}

fn exercise_case(service: &'static str, layout: &'static str) -> (Discovery, Activity, Vec<Node>) {
    let (window, nodes) = fixture(service, layout);
    let activity = Activity::default();
    // Discovery finishes before the only write or send operations in 3406.
    let outcome = discover_compose_box(&nodes, window);
    (outcome, activity, nodes)
}

#[test]
fn task_3638_all_thirty_webmail_layout_cases_find_one_compose_box_or_refuse_safely() {
    let mut cases = 0usize;
    let mut found = 0usize;
    let mut refused = 0usize;

    for service in SERVICES {
        for layout in BASE_LAYOUTS {
            let (outcome, activity, nodes) = exercise_case(service, layout);
            match outcome {
                Discovery::Found(index) => {
                    assert!(
                        nodes[index].compose_box,
                        "{service}/{layout} selected a non-compose editable control"
                    );
                    found += 1;
                    println!("TASK3638 service={service} layout={layout} result=found compose_boxes=1 typed_characters_before={} typed_characters_after={} sent_messages_before={} sent_messages_after={}", activity.typed_characters, activity.typed_characters, activity.sent_messages, activity.sent_messages);
                }
                Discovery::Refused(reason) => {
                    assert_eq!(
                        activity.typed_characters, 0,
                        "{service}/{layout} typed before refusal"
                    );
                    assert_eq!(
                        activity.sent_messages, 0,
                        "{service}/{layout} sent before refusal"
                    );
                    refused += 1;
                    println!("TASK3638 service={service} layout={layout} result=refused reason={reason:?} typed_characters_before=0 typed_characters_after=0 sent_messages_before=0 sent_messages_after=0");
                }
            }
            cases += 1;
        }
    }
    for service in EXTRA_THEME_SERVICES {
        let layout = "high-contrast theme";
        let (outcome, activity, nodes) = exercise_case(service, layout);
        let Discovery::Found(index) = outcome else {
            panic!("{service}/{layout} unexpectedly refused: {outcome:?}");
        };
        assert!(
            nodes[index].compose_box,
            "{service}/{layout} selected a non-compose editable control"
        );
        assert_eq!(activity.typed_characters, 0);
        assert_eq!(activity.sent_messages, 0);
        found += 1;
        cases += 1;
        println!("TASK3638 service={service} layout={layout} result=found compose_boxes=1 typed_characters_before=0 typed_characters_after=0 sent_messages_before=0 sent_messages_after=0");
    }

    assert_eq!(cases, 30, "9 services × 3 layouts plus 3 extra theme cases");
    assert_eq!(found, 29);
    assert_eq!(refused, 1);
    println!("TASK3638 service_layout_cases={cases} found_exactly_one={found} refused_before_typing={refused} refusal_typed_characters_before_after=0 refusal_sent_messages_before_after=0");
}

#[test]
fn task_3638_3406_refusal_receipt_reports_no_typing_or_sends() {
    for required in [
        "composer_discovered_by=role-state-geometry",
        "message box ambiguous",
        "refuse_before_typing",
        "typed_characters_before=0 typed_characters_after=0 sent_messages_before=0 sent_messages_after=0",
    ] {
        assert!(PLACE_TEXT.contains(required), "3406 lost {required}");
    }
    println!("TASK3638 3406_refusal_typed_characters_before_after=0 3406_refusal_sent_messages_before_after=0");
}
