use osl_privacy_hub::native_signal_adapter::{SignalNode, SignalRect, SignalRole};
use osl_privacy_hub::signal_surface_finder::{
    find_signal_open_direct_message, task_1031_open_direct_message_fixture, SignalFinderError,
    SIGNAL_PROCESS_IMAGE, SIGNAL_WINDOW_CLASS, SIGNAL_WINDOW_TITLE,
};

#[test]
fn task_1031_finds_one_window_conversation_and_typing_box_in_open_direct_message() {
    let (windows, nodes) = task_1031_open_direct_message_fixture();
    let found = find_signal_open_direct_message(&windows, &nodes).expect("open direct message");

    println!("active_window_count=1 index={}", found.active_window_index);
    println!(
        "conversation_count=1 index={}",
        found.conversation_node_index
    );
    println!("typing_box_count=1 index={}", found.typing_box_node_index);
    assert_eq!(found.active_window_index, 0);
    assert_eq!(found.conversation_node_index, 1);
    assert_eq!(found.typing_box_node_index, 2);
}

#[test]
fn task_1031_refuses_a_search_box_and_a_non_foreground_signal_window() {
    let (mut windows, mut nodes) = task_1031_open_direct_message_fixture();
    windows[0].foreground = false;
    assert_eq!(
        find_signal_open_direct_message(&windows, &nodes),
        Err(SignalFinderError::ActiveWindowMissing)
    );

    windows[0].foreground = true;
    nodes[2].localized_name = Some("Search messages".to_owned());
    assert_eq!(
        find_signal_open_direct_message(&windows, &nodes),
        Err(SignalFinderError::TypingBoxMissing)
    );
}

#[test]
fn task_1031_requires_exact_signal_window_identity() {
    let (mut windows, nodes) = task_1031_open_direct_message_fixture();
    for mutation in [
        ("Other.exe", SIGNAL_WINDOW_CLASS, SIGNAL_WINDOW_TITLE),
        (SIGNAL_PROCESS_IMAGE, "OtherClass", SIGNAL_WINDOW_TITLE),
        (SIGNAL_PROCESS_IMAGE, SIGNAL_WINDOW_CLASS, "Signal Beta"),
    ] {
        windows[0].process_image = mutation.0.to_owned();
        windows[0].class_name = mutation.1.to_owned();
        windows[0].title = mutation.2.to_owned();
        assert_eq!(
            find_signal_open_direct_message(&windows, &nodes),
            Err(SignalFinderError::ActiveWindowMissing)
        );
    }

    let mut second_conversation = SignalNode::structural(
        SignalRole::List,
        SignalRect {
            left: 430,
            top: 100,
            right: 1130,
            bottom: 700,
        },
    );
    second_conversation.visible = true;
    let mut ambiguous_nodes = nodes;
    ambiguous_nodes.push(second_conversation);
    let (exact_windows, _) = task_1031_open_direct_message_fixture();
    assert_eq!(
        find_signal_open_direct_message(&exact_windows, &ambiguous_nodes),
        Err(SignalFinderError::ConversationAmbiguous)
    );
}
