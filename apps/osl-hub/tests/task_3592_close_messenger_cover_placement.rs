use osl_privacy_hub::messenger_cover_placement::{
    place_messenger_cover, MessengerCoverPlacementTarget, MESSENGER_COVER_PLACEMENT_STEPS,
};
use std::collections::BTreeMap;

const PRIVATE_DRAFT: &str = "TASK3592 private draft: café 🔒";
const CONTROL_MARK: &str = "TASK3592-LIVE-CONTROL";
const COVER_MARK: &str = "TASK3592-COVER-MARK";

struct MessengerAttempt {
    closes_at: Option<&'static str>,
    current_step: &'static str,
    composer: String,
    private_draft: String,
    receiver_marks: Vec<String>,
    sent_count: usize,
    fields: BTreeMap<String, String>,
}

impl MessengerAttempt {
    fn fresh(closes_at: Option<&'static str>) -> Self {
        Self {
            closes_at,
            current_step: "before-placement",
            composer: String::new(),
            private_draft: PRIVATE_DRAFT.to_owned(),
            receiver_marks: Vec::new(),
            sent_count: 0,
            fields: BTreeMap::from([
                ("conversation".to_owned(), "Ada Lovelace".to_owned()),
                ("read_receipts".to_owned(), "on".to_owned()),
                ("search".to_owned(), String::new()),
                ("theme".to_owned(), "default".to_owned()),
            ]),
        }
    }

    fn send_one_live_control(&mut self) {
        self.receiver_marks.push(CONTROL_MARK.to_owned());
        self.sent_count += 1;
    }

    fn step(&mut self, step: &'static str) {
        self.current_step = step;
    }
}

impl MessengerCoverPlacementTarget for MessengerAttempt {
    fn begin_cover_placement_step(&mut self, step: &'static str) {
        self.step(step);
    }

    fn messenger_is_live(&self) -> bool {
        self.closes_at != Some(self.current_step)
    }

    fn read_composer(&self) -> Result<String, String> {
        Ok(self.composer.clone())
    }

    fn place_cover(&mut self, cover: &str) -> Result<(), String> {
        self.step("marked-paste");
        self.composer = cover.to_owned();
        Ok(())
    }

    fn clear_composer(&mut self) -> Result<(), String> {
        self.step("clear");
        self.composer.clear();
        Ok(())
    }

    fn non_typing_fields(&self) -> BTreeMap<String, String> {
        self.fields.clone()
    }

    fn messenger_sent_count(&self) -> usize {
        self.sent_count
    }
}

#[test]
fn task_3592_one_live_control_survives_every_messenger_cover_close() {
    let mut control = MessengerAttempt::fresh(None);
    let control_before = control.receiver_marks.len();
    control.send_one_live_control();
    assert_eq!(control_before, 0);
    assert_eq!(control.receiver_marks.len(), 1);
    assert_eq!(control.receiver_marks, [CONTROL_MARK]);
    assert_eq!(control.sent_count, 1);
    println!(
        "TASK3592 control_receiver_before={} control_receiver_after={} control_mark_count={} messenger_sent_count={}",
        control_before, control.receiver_marks.len(), control.receiver_marks.len(), control.sent_count
    );

    for step in MESSENGER_COVER_PLACEMENT_STEPS {
        let mut attempt = MessengerAttempt::fresh(Some(step));
        // A fresh marked attempt starts with the one already-sent control and
        // no additional send permission.
        attempt.receiver_marks = control.receiver_marks.clone();
        attempt.sent_count = control.sent_count;
        let fields_before = attempt.non_typing_fields();
        let error = place_messenger_cover(&mut attempt, COVER_MARK)
            .expect_err("closing Messenger at each named cover step must refuse");
        assert!(error.contains("Retry placement in Messenger."));
        assert_eq!(attempt.receiver_marks, [CONTROL_MARK]);
        assert_eq!(attempt.sent_count, 1);
        assert_eq!(attempt.private_draft, PRIVATE_DRAFT);
        assert_eq!(attempt.non_typing_fields(), fields_before);
        println!(
            "TASK3592 close_step={} receiver_mark_count={} messenger_sent_count={} private_draft_exact={} non_typing_fields_unchanged={} retry_text={:?}",
            step,
            attempt.receiver_marks.len(),
            attempt.sent_count,
            attempt.private_draft == PRIVATE_DRAFT,
            attempt.non_typing_fields() == fields_before,
            error
        );
    }
}
