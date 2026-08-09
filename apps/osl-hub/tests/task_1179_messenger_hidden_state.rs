//! TASK 1179: removing a Messenger row's protected key makes a later show
//! request fail closed without changing the protected row already on screen.

use osl_privacy_hub::messenger_delivery::{
    LiveMessengerReceivingJob, MessengerReceivingJob, MessengerReceivingJobOutput,
    MessengerTestAccount, MessengerTestMachine,
};
use osl_privacy_hub::messenger_eye_state::{
    MessengerRowFeed, MessengerRowStateWriter, MessengerVisibleRow,
};

const SENDER_ACCOUNT: &str = "messenger-hidden-sender-1179";
const RECEIVER_ACCOUNT: &str = "messenger-hidden-receiver-1179";
const MARKED_COVER: &str = "[OSL-1179-MARKED] ordinary Messenger carrier";
const PROTECTED_ROW: &str = "messenger-row-1179";
const PROTECTED_KEY: &str = "messenger-protected-key-1179";
const REMOVED_KEY_REFUSAL: &str =
    "OSL: Messenger show refused: protected key removed for messenger-row-1179";

struct RemovableProtectedKeyFeed {
    recorded: MessengerReceivingJobOutput,
    protected_key: Option<String>,
}

impl RemovableProtectedKeyFeed {
    fn recorded_by(receiver: &MessengerTestAccount, marked_cover: &str) -> Self {
        Self {
            recorded: receiver
                .receiving_job_output_for_marked_cover(marked_cover)
                .expect("Messenger receiving job recorded the protected row")
                .clone(),
            protected_key: Some(PROTECTED_KEY.to_owned()),
        }
    }

    fn remove_protected_key(&mut self) -> String {
        self.protected_key
            .take()
            .expect("fixture has one protected key to remove")
    }
}

impl MessengerRowFeed for RemovableProtectedKeyFeed {
    fn recorded_output(
        &self,
        _receiver: &MessengerTestAccount,
        marked_cover: &str,
    ) -> Result<MessengerReceivingJobOutput, String> {
        if self.protected_key.is_none() {
            return Err(REMOVED_KEY_REFUSAL.to_owned());
        }
        if self.recorded.marked_cover.as_bytes() != marked_cover.as_bytes() {
            return Err("OSL: Messenger show refused: marked row changed".to_owned());
        }
        Ok(self.recorded.clone())
    }
}

fn count_shown(rows: &[MessengerVisibleRow], text: &str) -> usize {
    rows.iter().filter(|row| row.text == text).count()
}

#[test]
fn task_1179_removed_protected_key_refuses_show_and_preserves_messenger_row() {
    let mut machine = MessengerTestMachine::default();
    machine.send_marked_cover(
        SENDER_ACCOUNT,
        RECEIVER_ACCOUNT,
        MARKED_COVER,
        Some(PROTECTED_ROW),
    );
    assert_eq!(LiveMessengerReceivingJob.receive_pending(&mut machine), 1);

    let mut feed = RemovableProtectedKeyFeed::recorded_by(&machine.receiver, MARKED_COVER);
    let mut writer = MessengerRowStateWriter::default();

    let shown = writer
        .open_marked_row_eye(&machine.receiver, &feed, MARKED_COVER)
        .expect("good protected Messenger row is shown");
    assert_eq!(shown.text, PROTECTED_ROW);
    assert_eq!(writer.rows().len(), 1);
    assert_eq!(count_shown(writer.rows(), PROTECTED_ROW), 1);
    let shown_before_removed_key = writer.rows().to_vec();
    println!("TASK1179_GOOD_PROTECTED_ROW={}", shown.text);
    println!(
        "TASK1179_GOOD_SHOWN_RESULT_COUNT={}",
        count_shown(writer.rows(), PROTECTED_ROW)
    );

    let removed_key = feed.remove_protected_key();
    assert_eq!(removed_key, PROTECTED_KEY);
    let refusal = writer
        .open_marked_row_eye(&machine.receiver, &feed, MARKED_COVER)
        .expect_err("removed protected key must refuse the Messenger show request");
    assert_eq!(refusal, REMOVED_KEY_REFUSAL);
    println!("TASK1179_CHANGED=protected key removed");
    println!("TASK1179_REMOVED_KEY={removed_key}");
    println!("TASK1179_REFUSAL={refusal}");

    assert_eq!(writer.rows(), shown_before_removed_key);
    assert_eq!(writer.rows().len(), 1);
    assert_eq!(count_shown(writer.rows(), PROTECTED_ROW), 1);
    println!("TASK1179_PRESERVED_PROTECTED_ROW={PROTECTED_ROW}");
    println!(
        "TASK1179_PRESERVED_SHOWN_RESULT_COUNT={}",
        count_shown(writer.rows(), PROTECTED_ROW)
    );
}
