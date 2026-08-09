//! TASK 1178: Messenger's closed/open-eye controls switch exactly the marked
//! fixture row between its ordinary carrier and its recorded protected text.

use osl_privacy_hub::messenger_delivery::{
    LiveMessengerReceivingJob, MessengerReceivingJob, MessengerTestMachine,
};
use osl_privacy_hub::messenger_eye_state::{
    MessengerRowStateWriter, MessengerVisibleRow, ReceivingJobMessengerRowFeed,
};

const SENDER_ACCOUNT: &str = "messenger-eye-sender-1178";
const RECEIVER_ACCOUNT: &str = "messenger-eye-receiver-1178";
const MARKED_COVER: &str = "[OSL-1178-MARKED] ordinary Messenger row";
const PROTECTED_TEXT: &str = "protected Messenger text: juniper-1178 🔒";

fn count_rows_showing(rows: &[MessengerVisibleRow], text: &str) -> usize {
    rows.iter().filter(|row| row.text == text).count()
}

fn print_counts(phase: &str, rows: &[MessengerVisibleRow]) {
    println!("TASK1178 {phase} marked_fixture_rows={}", rows.len());
    println!(
        "TASK1178 {phase} ordinary_messenger_rows={}",
        count_rows_showing(rows, MARKED_COVER)
    );
    println!(
        "TASK1178 {phase} protected_text_rows={}",
        count_rows_showing(rows, PROTECTED_TEXT)
    );
}

#[test]
fn task_1178_messenger_eye_controls_switch_exactly_the_marked_fixture_row_and_back() {
    let mut machine = MessengerTestMachine::default();
    machine.send_marked_cover(
        SENDER_ACCOUNT,
        RECEIVER_ACCOUNT,
        MARKED_COVER,
        Some(PROTECTED_TEXT),
    );
    assert_eq!(LiveMessengerReceivingJob.receive_pending(&mut machine), 1);

    let feed = ReceivingJobMessengerRowFeed;
    let mut rows = MessengerRowStateWriter::default();

    rows.close_marked_row_eye(&machine.receiver, &feed, MARKED_COVER)
        .expect("closed-eye control shows the ordinary Messenger row");
    assert_eq!(rows.rows().len(), 1);
    assert_eq!(count_rows_showing(rows.rows(), MARKED_COVER), 1);
    assert_eq!(count_rows_showing(rows.rows(), PROTECTED_TEXT), 0);
    print_counts("closed_eye", rows.rows());

    rows.open_marked_row_eye(&machine.receiver, &feed, MARKED_COVER)
        .expect("open-eye control shows the protected text");
    assert_eq!(rows.rows().len(), 1);
    assert_eq!(count_rows_showing(rows.rows(), MARKED_COVER), 0);
    assert_eq!(count_rows_showing(rows.rows(), PROTECTED_TEXT), 1);
    print_counts("open_eye", rows.rows());

    rows.close_marked_row_eye(&machine.receiver, &feed, MARKED_COVER)
        .expect("closed-eye control switches the same marked row back");
    assert_eq!(rows.rows().len(), 1);
    assert_eq!(count_rows_showing(rows.rows(), MARKED_COVER), 1);
    assert_eq!(count_rows_showing(rows.rows(), PROTECTED_TEXT), 0);
    print_counts("closed_again", rows.rows());
}
