//! TASK 1115: removing a marked X row's protected key makes a later show
//! request fail closed without changing the one already-shown result.

use osl_privacy_hub::x_eye_state::{
    XEyeState, XEyeStateError, XEyeStateStore, XReceivedFeed, XReceivingJobOutput,
    XReceivingJobRow, XRowKind,
};

const ROW: &str = "x-row-1115";
const NORMAL_TEXT: &str = "ordinary X carrier for x-row-1115";
const PROTECTED_TEXT: &str = "x-row-1115";

fn shown_rows(state: &XEyeStateStore) -> Vec<String> {
    state
        .rows()
        .iter()
        .filter(|row| row.eye_state == XEyeState::Protected)
        .map(|row| row.displayed_text().to_owned())
        .collect()
}

#[test]
fn task_1115_removed_protected_key_refuses_show_and_preserves_one_result() {
    let output = XReceivingJobOutput {
        run_id: "task-1115-x-receiving-job".into(),
        rows: vec![XReceivingJobRow {
            kind: XRowKind::DirectMessage,
            marker: ROW.into(),
            normal_text: NORMAL_TEXT.into(),
            protected_text: PROTECTED_TEXT.into(),
        }],
    };
    let feed = XReceivedFeed::recorded_by(&output);
    let mut state = XEyeStateStore::write_from_receiving_job(&output, &feed)
        .expect("good protected X row must bind to its receiving job");

    state
        .open_marked_row_eye(ROW)
        .expect("present protected key must allow show");
    let good_shown = shown_rows(&state);
    assert_eq!(good_shown, vec![ROW.to_owned()]);
    println!(
        "TASK1115 good_protected_row={ROW} shown_result_count={} shown_results={}",
        good_shown.len(),
        good_shown.join(",")
    );

    state
        .remove_protected_key(ROW)
        .expect("remove the existing protected key");
    assert!(!state.has_protected_key(ROW));
    let refusal = state
        .open_marked_row_eye(ROW)
        .expect_err("removed protected key must refuse show");
    assert_eq!(refusal, XEyeStateError::ProtectedKeyRemoved(ROW.to_owned()));
    assert_eq!(refusal.refusal_name(), Some("removed"));
    println!(
        "TASK1115 changed_protected_key=removed refused_by_name={} refusal={refusal}",
        refusal.refusal_name().expect("named refusal")
    );

    let final_shown = shown_rows(&state);
    assert_eq!(final_shown, good_shown);
    assert_eq!(final_shown, vec![ROW.to_owned()]);
    println!(
        "TASK1115 final_protected_row={ROW} shown_result_count={} shown_results={} unchanged={}",
        final_shown.len(),
        final_shown.join(","),
        final_shown == good_shown
    );
}
