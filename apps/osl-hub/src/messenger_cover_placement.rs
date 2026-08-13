//! Messenger cover placement is deliberately separate from a live control send.
//!
//! A control may be sent once, but every later cover-placement attempt is only
//! an empty/read/place/read/clear transaction.  If Messenger closes at any
//! named step, the transaction stops before another send boundary.

use std::collections::BTreeMap;

pub const MESSENGER_COVER_PLACEMENT_STEPS: [&str; 4] =
    ["empty-readback", "marked-paste", "exact-readback", "clear"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessengerCoverPlacementReceipt {
    pub cover_bytes: usize,
    pub messenger_sent_count: usize,
}

/// The small, provider-owned seam required for the placement transaction.
/// `non_typing_fields` excludes the composer: only the composer is allowed to
/// change while a cover is being placed and cleared.
pub trait MessengerCoverPlacementTarget {
    /// Records the currently executing named boundary before its liveness
    /// check. Implementations may use this for a provider-death observer.
    fn begin_cover_placement_step(&mut self, step: &'static str);
    fn messenger_is_live(&self) -> bool;
    fn read_composer(&self) -> Result<String, String>;
    fn place_cover(&mut self, cover: &str) -> Result<(), String>;
    fn clear_composer(&mut self) -> Result<(), String>;
    fn non_typing_fields(&self) -> BTreeMap<String, String>;
    fn messenger_sent_count(&self) -> usize;
}

pub fn place_messenger_cover(
    target: &mut impl MessengerCoverPlacementTarget,
    cover: &str,
) -> Result<MessengerCoverPlacementReceipt, String> {
    if cover.is_empty() {
        return Err("Messenger cover must not be empty".to_owned());
    }
    let before_fields = target.non_typing_fields();
    require_live(target, "empty-readback")?;
    if !target.read_composer()?.is_empty() {
        return Err("Messenger composer was not empty before cover placement".to_owned());
    }

    target.place_cover(cover)?;
    require_live(target, "marked-paste")?;
    require_unchanged_fields(target, &before_fields)?;

    require_live(target, "exact-readback")?;
    if target.read_composer()?.as_bytes() != cover.as_bytes() {
        return Err("Messenger cover read-back was not exact".to_owned());
    }

    require_live(target, "clear")?;
    target.clear_composer()?;
    if !target.read_composer()?.is_empty() {
        return Err("Messenger cover clear was not exact".to_owned());
    }
    require_unchanged_fields(target, &before_fields)?;
    Ok(MessengerCoverPlacementReceipt {
        cover_bytes: cover.len(),
        messenger_sent_count: target.messenger_sent_count(),
    })
}

fn require_live(
    target: &mut impl MessengerCoverPlacementTarget,
    step: &'static str,
) -> Result<(), String> {
    target.begin_cover_placement_step(step);
    if target.messenger_is_live() {
        Ok(())
    } else {
        Err(format!(
            "Messenger closed during {step}. Your message was not sent anywhere. Retry placement in Messenger."
        ))
    }
}

fn require_unchanged_fields(
    target: &impl MessengerCoverPlacementTarget,
    before: &BTreeMap<String, String>,
) -> Result<(), String> {
    if target.non_typing_fields() == *before {
        Ok(())
    } else {
        Err("Messenger changed a non-typing field during cover placement".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRIVATE_DRAFT: &str = "TASK3592 private draft: café 🔒";
    const CONTROL_MARK: &str = "TASK3592-LIVE-CONTROL";

    struct Attempt {
        closes_at: &'static str,
        step: &'static str,
        composer: String,
        private_draft: String,
        receiver_marks: Vec<String>,
        sent_count: usize,
        fields: BTreeMap<String, String>,
    }

    impl Attempt {
        fn fresh(closes_at: &'static str) -> Self {
            Self {
                closes_at,
                step: "before-placement",
                composer: String::new(),
                private_draft: PRIVATE_DRAFT.to_owned(),
                receiver_marks: vec![CONTROL_MARK.to_owned()],
                sent_count: 1,
                fields: BTreeMap::from([
                    ("conversation".to_owned(), "Ada Lovelace".to_owned()),
                    ("read_receipts".to_owned(), "on".to_owned()),
                    ("search".to_owned(), String::new()),
                    ("theme".to_owned(), "default".to_owned()),
                ]),
            }
        }
    }

    impl MessengerCoverPlacementTarget for Attempt {
        fn begin_cover_placement_step(&mut self, step: &'static str) {
            self.step = step;
        }
        fn messenger_is_live(&self) -> bool {
            self.closes_at != self.step
        }
        fn read_composer(&self) -> Result<String, String> {
            Ok(self.composer.clone())
        }
        fn place_cover(&mut self, cover: &str) -> Result<(), String> {
            self.composer = cover.to_owned();
            Ok(())
        }
        fn clear_composer(&mut self) -> Result<(), String> {
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
    fn task_3592_closing_every_named_step_preserves_one_control() {
        let mut control = Attempt::fresh("never-closes");
        control.receiver_marks.clear();
        control.sent_count = 0;
        let receiver_before = control.receiver_marks.len();
        control.receiver_marks.push(CONTROL_MARK.to_owned());
        control.sent_count += 1;
        assert_eq!(receiver_before, 0);
        assert_eq!(control.receiver_marks.len(), 1);
        assert_eq!(control.sent_count, 1);
        println!(
            "TASK3592 control_receiver_before={receiver_before} control_receiver_after={} control_mark_count={} messenger_sent_count={}",
            control.receiver_marks.len(),
            control.receiver_marks.len(),
            control.sent_count,
        );
        for step in MESSENGER_COVER_PLACEMENT_STEPS {
            let mut attempt = Attempt::fresh(step);
            let fields_before = attempt.non_typing_fields();
            let error = place_messenger_cover(&mut attempt, "TASK3592-COVER")
                .expect_err("a closed Messenger must refuse placement");
            assert_eq!(attempt.receiver_marks, [CONTROL_MARK]);
            assert_eq!(attempt.sent_count, 1);
            assert_eq!(attempt.private_draft, PRIVATE_DRAFT);
            assert_eq!(attempt.non_typing_fields(), fields_before);
            assert!(error.contains("Retry placement in Messenger."));
            println!(
                "TASK3592 step={step} receiver_mark_count={} messenger_sent_count={} private_draft_exact={} fields_unchanged={} retry_text={error}",
                attempt.receiver_marks.len(),
                attempt.sent_count,
                attempt.private_draft == PRIVATE_DRAFT,
                attempt.non_typing_fields() == fields_before,
            );
        }
    }
}
