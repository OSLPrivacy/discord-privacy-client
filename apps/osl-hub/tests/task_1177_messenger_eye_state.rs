//! TASK 1177: a direct command switches one marked Messenger row between its
//! normal carrier and the exact protected text recorded by the receiving job.

#[path = "../src/messenger_delivery.rs"]
mod messenger_delivery;
#[path = "../src/messenger_eye_state.rs"]
mod messenger_eye_state;

use messenger_delivery::{
    LiveMessengerReceivingJob, MessengerReceivingJob, MessengerReceivingJobOutput,
    MessengerTestAccount, MessengerTestMachine,
};
use messenger_eye_state::{
    MessengerRowEyeState, MessengerRowFeed, MessengerRowStateWriter, ReceivingJobMessengerRowFeed,
};
use std::env;

const SENDER_ACCOUNT: &str = "messenger-eye-sender-1177";
const RECEIVER_ACCOUNT: &str = "messenger-eye-receiver-1177";
const MARKED_COVER: &str = "[OSL-1177] Meet beside the old cedar after lunch.";
const RECEIVING_JOB_PROTECTED_TEXT: &str =
    "Messenger receiving job output exactly: copper-sparrow-1177 🔒";
const FEED_ENV: &str = "OSL_TASK_1177_ROW_FEED";

struct StandInMessengerRowFeed;

impl MessengerRowFeed for StandInMessengerRowFeed {
    fn recorded_output(
        &self,
        _receiver: &MessengerTestAccount,
        marked_cover: &str,
    ) -> Result<MessengerReceivingJobOutput, String> {
        Ok(MessengerReceivingJobOutput {
            sender_account: SENDER_ACCOUNT.to_owned(),
            receiver_account: RECEIVER_ACCOUNT.to_owned(),
            marked_cover: marked_cover.to_owned(),
            protected_text: "stand-in feed private text (must be refused)".to_owned(),
        })
    }
}

#[test]
fn task_1177_direct_command_switches_the_marked_messenger_row_using_receiver_output() {
    let mut machine = MessengerTestMachine::default();
    machine.send_marked_cover(
        SENDER_ACCOUNT,
        RECEIVER_ACCOUNT,
        MARKED_COVER,
        Some(RECEIVING_JOB_PROTECTED_TEXT),
    );
    let delivered = LiveMessengerReceivingJob.receive_pending(&mut machine);
    assert_eq!(delivered, 1);

    let recorded = machine
        .receiver
        .receiving_job_output_for_marked_cover(MARKED_COVER)
        .expect("live Messenger receiving job records the marked output")
        .clone();
    assert_eq!(recorded.protected_text, RECEIVING_JOB_PROTECTED_TEXT);

    let stand_in = env::var(FEED_ENV).ok().as_deref() == Some("stand-in");
    let feed: Box<dyn MessengerRowFeed> = if stand_in {
        Box::new(StandInMessengerRowFeed)
    } else {
        Box::new(ReceivingJobMessengerRowFeed)
    };
    let mut writer = MessengerRowStateWriter::default();

    let normal_before = writer
        .write_marked_row_state(
            &machine.receiver,
            feed.as_ref(),
            MARKED_COVER,
            MessengerRowEyeState::Normal,
        )
        .expect("direct command writes normal Messenger row state");
    assert_eq!(normal_before.state, MessengerRowEyeState::Normal);
    assert_eq!(normal_before.text.as_bytes(), MARKED_COVER.as_bytes());

    let protected = writer
        .write_marked_row_state(
            &machine.receiver,
            feed.as_ref(),
            MARKED_COVER,
            MessengerRowEyeState::Protected,
        )
        .expect("direct command writes protected Messenger row state");
    assert_eq!(protected.state, MessengerRowEyeState::Protected);
    assert_eq!(
        protected.text,
        recorded.protected_text,
        "TASK1177_STAND_IN_FEED_REFUSED protected row text must equal Messenger receiving job recorded output exactly; stand_in={stand_in}"
    );

    let normal_after = writer
        .write_marked_row_state(
            &machine.receiver,
            feed.as_ref(),
            MARKED_COVER,
            MessengerRowEyeState::Normal,
        )
        .expect("direct command switches protected Messenger row back to normal");
    assert_eq!(normal_after.state, MessengerRowEyeState::Normal);
    assert_eq!(normal_after.text.as_bytes(), MARKED_COVER.as_bytes());
    assert_eq!(writer.rows().len(), 1);
    assert_eq!(writer.row(MARKED_COVER), Some(&normal_after));

    println!("TASK1177_DIRECT_COMMAND=write_marked_row_state");
    println!(
        "TASK1177_STATE_SEQUENCE={}->{}->{}",
        normal_before.state.name(),
        protected.state.name(),
        normal_after.state.name()
    );
    println!("TASK1177_MARKED_ROW_COUNT={}", writer.rows().len());
    println!("TASK1177_NORMAL_TEXT={}", normal_after.text);
    println!(
        "TASK1177_RECEIVING_JOB_RECORDED_OUTPUT={}",
        recorded.protected_text
    );
    println!("TASK1177_PROTECTED_TEXT={}", protected.text);
    println!(
        "TASK1177_PROTECTED_TEXT_EXACT_MATCH={}",
        protected.text.as_bytes() == recorded.protected_text.as_bytes()
    );
    println!("TASK1177_STAND_IN_FEED={stand_in}");
}
