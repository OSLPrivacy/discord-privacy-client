use osl_privacy_hub::native_signal_adapter::SignalNode;
use osl_privacy_hub::signal_surface_finder::{
    find_signal_open_direct_message, task_1031_open_direct_message_fixture, SignalFinderError,
    SignalSurfaceMatch, SignalWindowCandidate,
};

const BOX_ID: &str = "signal-box-1032";
const MESSAGE_BOX_RESULT: &str = "Signal message box";

fn find_named_state(
    state: &str,
    windows: &[SignalWindowCandidate],
    nodes: &[SignalNode],
) -> Result<SignalSurfaceMatch, SignalFinderError> {
    find_signal_open_direct_message(windows, nodes).map_err(|error| {
        println!("TASK1032 state={state} refused_by_name={error:?}");
        error
    })
}

fn one_signal_message_box(
    windows: &[SignalWindowCandidate],
    nodes: &[SignalNode],
) -> Vec<(&'static str, SignalSurfaceMatch)> {
    find_signal_open_direct_message(windows, nodes)
        .map(|found| vec![(MESSAGE_BOX_RESULT, found)])
        .expect("signal-box-1032 is an open Signal direct-message surface")
}

#[test]
fn task_1032_refuses_closed_and_search_focused_then_finds_the_same_good_box() {
    let (good_windows, good_nodes) = task_1031_open_direct_message_fixture();

    let good = one_signal_message_box(&good_windows, &good_nodes);
    assert_eq!(
        good.len(),
        1,
        "signal-box-1032 must yield exactly one result"
    );
    assert_eq!(good[0].0, MESSAGE_BOX_RESULT);
    println!(
        "TASK1032 good_box={BOX_ID} result_count={} result_name={}",
        good.len(),
        good[0].0
    );

    let closed_windows = Vec::new();
    let closed_error = find_named_state("closed", &closed_windows, &good_nodes)
        .expect_err("closed Signal must be refused");
    assert_eq!(closed_error, SignalFinderError::ActiveWindowMissing);

    let mut search_nodes = good_nodes.clone();
    let search_field = search_nodes
        .get_mut(good[0].1.typing_box_node_index)
        .expect("good fixture typing box exists");
    assert!(search_field.focusable && search_field.editable && !search_field.read_only);
    search_field.localized_name = Some("Search messages".to_owned());
    let search_error = find_named_state("search-focused", &good_windows, &search_nodes)
        .expect_err("focused Signal search field must be refused by accessible name");
    assert_eq!(search_error, SignalFinderError::TypingBoxMissing);

    let restored = one_signal_message_box(&good_windows, &good_nodes);
    assert_eq!(restored, good, "good box must return the same exact result");
    println!(
        "TASK1032 restored_box={BOX_ID} result_count={} result_name={}",
        restored.len(),
        restored[0].0
    );
}
