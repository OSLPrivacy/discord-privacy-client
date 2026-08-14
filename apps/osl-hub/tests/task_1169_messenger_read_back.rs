#[path = "../examples/task_3406_place_text.rs"]
mod shared_place_text;

use shared_place_text::{place_read_back_and_clear, SharedTextActions};

const TEXT: &str = "messenger-text-1169";
const CHANGED_PLACED_BYTE: u8 = b'x';
const REFUSAL_NAME: &str = "readback did not equal placed mark";

#[derive(Default)]
struct MessengerComposerActions {
    composer: String,
    change_before_read_back: bool,
    changed_read_back: Option<Vec<u8>>,
}

impl MessengerComposerActions {
    fn changing_one_placed_byte_before_read_back() -> Self {
        Self {
            change_before_read_back: true,
            ..Self::default()
        }
    }
}

impl SharedTextActions for MessengerComposerActions {
    fn read_back_text(&mut self) -> Result<String, String> {
        if self.change_before_read_back && !self.composer.is_empty() {
            self.change_before_read_back = false;
            let mut changed = self.composer.as_bytes().to_vec();
            let index = changed
                .iter()
                .position(|byte| *byte != CHANGED_PLACED_BYTE)
                .ok_or_else(|| "placed text has no byte that can be changed to x".to_owned())?;
            changed[index] = CHANGED_PLACED_BYTE;
            self.composer = String::from_utf8(changed.clone())
                .map_err(|_| "changed Messenger read-back was not UTF-8".to_owned())?;
            self.changed_read_back = Some(changed);
        }
        Ok(self.composer.clone())
    }

    fn place_text(&mut self, text: &str) -> Result<(), String> {
        self.composer.clear();
        self.composer.push_str(text);
        Ok(())
    }

    fn clear_text(&mut self) -> Result<(), String> {
        self.composer.clear();
        Ok(())
    }
}

fn results(actions: &mut MessengerComposerActions) -> Result<Vec<String>, String> {
    place_read_back_and_clear(actions, TEXT).map(|_| vec![TEXT.to_owned()])
}

#[test]
fn task_1169_changed_placed_byte_is_refused_and_good_result_is_unchanged() {
    let mut baseline_actions = MessengerComposerActions::default();
    let good = results(&mut baseline_actions).expect("good Messenger text must pass read-back");
    assert_eq!(good, [TEXT]);

    let mut changed_actions = MessengerComposerActions::changing_one_placed_byte_before_read_back();
    let refusal =
        results(&mut changed_actions).expect_err("changed placed byte x must be refused by name");
    assert!(
        refusal.contains(REFUSAL_NAME),
        "changed placed byte x had wrong refusal: {refusal}"
    );
    assert_eq!(
        changed_actions.changed_read_back.as_deref(),
        Some(&b"xessenger-text-1169"[..])
    );
    assert_eq!(
        changed_actions
            .changed_read_back
            .as_deref()
            .unwrap()
            .iter()
            .zip(TEXT.as_bytes())
            .filter(|(actual, expected)| actual != expected)
            .count(),
        1,
        "fault injection must alter exactly one placed byte"
    );

    let mut restored_actions = MessengerComposerActions::default();
    let restored =
        results(&mut restored_actions).expect("restored Messenger text must pass read-back");
    assert_eq!(restored, good);

    println!(
        "TASK1169 text={TEXT} result_count={} result_name={}",
        good.len(),
        good[0]
    );
    println!(
        "TASK1169 changed_placed_byte={} changed_readback=xessenger-text-1169 result=refused refusal_name={REFUSAL_NAME}",
        char::from(CHANGED_PLACED_BYTE)
    );
    println!(
        "TASK1169 restored_text={TEXT} result_count={} result_name={}",
        restored.len(),
        restored[0]
    );
}
