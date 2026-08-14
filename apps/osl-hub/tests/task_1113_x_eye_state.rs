//! TASK 1113: X marked rows switch only between the received exact private
//! output and their normal carrier text.  A stand-in feed is a hard failure.

use osl_privacy_hub::x_eye_state::{
    XEyeState, XEyeStateStore, XReceivedFeed, XReceivingJobOutput, XReceivingJobRow, XRowKind,
};
use std::env;

const DM_MARKER: &str = "TASK1113-X-DM";
const POST_MARKER: &str = "TASK1113-X-POST";
const DM_NORMAL: &str = "ordinary X DM carrier TASK1113-X-DM";
const POST_NORMAL: &str = "ordinary X post carrier TASK1113-X-POST";
const DM_RECORDED_OUTPUT: &str = "DM receiver output: amber\n  keeps exact whitespace.";
const POST_RECORDED_OUTPUT: &str = "Post receiver output: cypress\tkeeps exact tab.";

fn x_receiving_job_recorded_output() -> XReceivingJobOutput {
    XReceivingJobOutput {
        run_id: "task-1113-x-receiving-job-1".into(),
        rows: vec![
            XReceivingJobRow {
                kind: XRowKind::DirectMessage,
                marker: DM_MARKER.into(),
                normal_text: DM_NORMAL.into(),
                protected_text: DM_RECORDED_OUTPUT.into(),
            },
            XReceivingJobRow {
                kind: XRowKind::PublicPost,
                marker: POST_MARKER.into(),
                normal_text: POST_NORMAL.into(),
                protected_text: POST_RECORDED_OUTPUT.into(),
            },
        ],
    }
}

fn feed_for(output: &XReceivingJobOutput) -> XReceivedFeed {
    if env::var_os("OSL_X_EYE_STATE_STANDIN_FEED").is_some() {
        // This is intentionally plausible-looking: only its run binding says
        // it is not the X receiving job that produced the recorded output.
        XReceivedFeed {
            receiving_job_run_id: "task-1113-stand-in-feed".into(),
            rows: output.rows.clone(),
        }
    } else {
        XReceivedFeed::recorded_by(output)
    }
}

#[test]
fn task_1113_x_dm_and_post_eye_command_uses_exact_receiving_job_output() {
    let output = x_receiving_job_recorded_output();
    let feed = feed_for(&output);
    let mut state = XEyeStateStore::write_from_receiving_job(&output, &feed)
        .expect("X eye state must refuse a stand-in feed");

    assert_eq!(
        state.rows().len(),
        2,
        "one marked X DM and one marked X post"
    );
    assert_eq!(state.rows()[0].kind, XRowKind::DirectMessage);
    assert_eq!(state.rows()[1].kind, XRowKind::PublicPost);

    let dm_protected = state
        .switch_marked_row(DM_MARKER, XEyeState::Protected)
        .expect("switch DM eye to protected");
    assert_eq!(dm_protected.displayed_text(), DM_RECORDED_OUTPUT);
    assert_eq!(dm_protected.protected_text, output.rows[0].protected_text);
    let dm_normal = state
        .switch_marked_row(DM_MARKER, XEyeState::Normal)
        .expect("switch DM eye to normal");
    assert_eq!(dm_normal.displayed_text(), DM_NORMAL);

    let post_protected = state
        .switch_marked_row(POST_MARKER, XEyeState::Protected)
        .expect("switch post eye to protected");
    assert_eq!(post_protected.displayed_text(), POST_RECORDED_OUTPUT);
    assert_eq!(post_protected.protected_text, output.rows[1].protected_text);
    let post_normal = state
        .switch_marked_row(POST_MARKER, XEyeState::Normal)
        .expect("switch post eye to normal");
    assert_eq!(post_normal.displayed_text(), POST_NORMAL);

    println!("TASK1113 marked_row_count={}", state.rows().len());
    println!("TASK1113 direct_message.normal={DM_NORMAL}");
    println!("TASK1113 direct_message.protected={DM_RECORDED_OUTPUT:?}");
    println!("TASK1113 direct_message.switches=protected,normal");
    println!("TASK1113 public_post.normal={POST_NORMAL}");
    println!("TASK1113 public_post.protected={POST_RECORDED_OUTPUT:?}");
    println!("TASK1113 public_post.switches=protected,normal");
    println!("TASK1113 receiving_job_run_id={}", output.run_id);
    println!("TASK1113 stand_in_feed=false");
}
