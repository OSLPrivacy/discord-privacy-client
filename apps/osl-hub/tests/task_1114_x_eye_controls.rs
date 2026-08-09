//! TASK 1114: the X open/closed-eye controls change only the marked row.

use osl_privacy_hub::x_eye_state::{
    XEyeState, XEyeStateStore, XReceivedFeed, XReceivingJobOutput, XReceivingJobRow, XRowKind,
};

const MARKER: &str = "TASK1114-X-MARKED";
const ORDINARY_X_CONTENT: &str = "ordinary X content TASK1114-X-MARKED";
const PROTECTED_TEXT: &str = "protected X text: marigold\nkeeps exact whitespace";

fn count_rows_showing(rows: &[osl_privacy_hub::x_eye_state::XEyeRowState], text: &str) -> usize {
    rows.iter()
        .filter(|row| row.displayed_text() == text)
        .count()
}

#[test]
fn task_1114_x_closed_and_open_eye_controls_switch_one_marked_fixture_row() {
    let output = XReceivingJobOutput {
        run_id: "task-1114-x-receiving-job-1".into(),
        rows: vec![XReceivingJobRow {
            kind: XRowKind::DirectMessage,
            marker: MARKER.into(),
            normal_text: ORDINARY_X_CONTENT.into(),
            protected_text: PROTECTED_TEXT.into(),
        }],
    };
    let feed = XReceivedFeed::recorded_by(&output);
    let mut state = XEyeStateStore::write_from_receiving_job(&output, &feed)
        .expect("fixture must come from the X receiving job");

    let closed = state.close_marked_row_eye(MARKER).expect("close X eye");
    assert_eq!(closed.eye_state, XEyeState::Normal);
    assert_eq!(closed.displayed_text(), ORDINARY_X_CONTENT);
    assert_eq!(count_rows_showing(state.rows(), ORDINARY_X_CONTENT), 1);
    assert_eq!(count_rows_showing(state.rows(), PROTECTED_TEXT), 0);
    println!(
        "TASK1114 closed_eye marked_fixture_rows={}",
        state.rows().len()
    );
    println!(
        "TASK1114 closed_eye ordinary_x_content_rows={}",
        count_rows_showing(state.rows(), ORDINARY_X_CONTENT)
    );
    println!(
        "TASK1114 closed_eye protected_text_rows={}",
        count_rows_showing(state.rows(), PROTECTED_TEXT)
    );

    let opened = state.open_marked_row_eye(MARKER).expect("open X eye");
    assert_eq!(opened.eye_state, XEyeState::Protected);
    assert_eq!(opened.displayed_text(), PROTECTED_TEXT);
    assert_eq!(count_rows_showing(state.rows(), ORDINARY_X_CONTENT), 0);
    assert_eq!(count_rows_showing(state.rows(), PROTECTED_TEXT), 1);
    println!(
        "TASK1114 open_eye marked_fixture_rows={}",
        state.rows().len()
    );
    println!(
        "TASK1114 open_eye ordinary_x_content_rows={}",
        count_rows_showing(state.rows(), ORDINARY_X_CONTENT)
    );
    println!(
        "TASK1114 open_eye protected_text_rows={}",
        count_rows_showing(state.rows(), PROTECTED_TEXT)
    );

    let closed_again = state
        .close_marked_row_eye(MARKER)
        .expect("close X eye again");
    assert_eq!(closed_again.eye_state, XEyeState::Normal);
    assert_eq!(closed_again.displayed_text(), ORDINARY_X_CONTENT);
    assert_eq!(count_rows_showing(state.rows(), ORDINARY_X_CONTENT), 1);
    assert_eq!(count_rows_showing(state.rows(), PROTECTED_TEXT), 0);
    println!(
        "TASK1114 closed_again ordinary_x_content_rows={}",
        count_rows_showing(state.rows(), ORDINARY_X_CONTENT)
    );
    println!(
        "TASK1114 closed_again protected_text_rows={}",
        count_rows_showing(state.rows(), PROTECTED_TEXT)
    );
}
