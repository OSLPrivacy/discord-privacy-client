#[path = "../examples/task_3406_place_text.rs"]
mod shared_place_text;

use shared_place_text::{
    place_text_through_shared_job, SharedPlaceTextActions, SharedPlaceTextReceipt,
};

mod fixture {
    include!("fixtures/task_1033_marked_bytes.rs");
}

const TASK1034_TEXT: &str = "signal-text-1034";
const TASK1034_CHANGED_PLACED_BYTE: u8 = b'x';

#[derive(Default)]
struct SignalRichEditor {
    visible_text: String,
    editor_owned_text: String,
    shared_paste_calls: usize,
    change_before_read_back: bool,
    changed_read_back: Option<Vec<u8>>,
    task1034_events: Vec<&'static str>,
}

impl SignalRichEditor {
    fn changing_one_placed_byte_before_read_back() -> Self {
        Self {
            change_before_read_back: true,
            ..Self::default()
        }
    }

    /// Models ValuePattern-style mutation behind the rich editor's back: the
    /// pixels/readback change, but Signal's private message state does not.
    fn mutate_visible_value_only(&mut self, text: &str) {
        self.visible_text = text.to_owned();
    }

    fn send_button_available(&self) -> bool {
        !self.editor_owned_text.is_empty()
    }
}

impl SharedPlaceTextActions for SignalRichEditor {
    fn read_back_text(&mut self) -> Result<String, String> {
        if self.change_before_read_back && !self.visible_text.is_empty() {
            let mut changed = self.visible_text.as_bytes().to_vec();
            let index = changed
                .iter()
                .position(|byte| *byte != TASK1034_CHANGED_PLACED_BYTE)
                .expect("placed Signal text has a byte that can be changed to x");
            changed[index] = TASK1034_CHANGED_PLACED_BYTE;
            self.visible_text = String::from_utf8(changed.clone())
                .expect("changing one ASCII byte keeps the Signal read-back UTF-8");
            self.changed_read_back = Some(changed);
            self.task1034_events.push("changed-before-read-back");
            self.change_before_read_back = false;
        }
        Ok(self.visible_text.clone())
    }

    fn paste_text(&mut self, text: &str) -> Result<(), String> {
        self.shared_paste_calls += 1;
        self.visible_text = text.to_owned();
        self.editor_owned_text = text.to_owned();
        self.task1034_events.push("placed");
        Ok(())
    }

    fn editor_accepts_message(&mut self) -> Result<bool, String> {
        Ok(self.send_button_available())
    }
}

struct BehindTheEditorMutation(SignalRichEditor);

impl SharedPlaceTextActions for BehindTheEditorMutation {
    fn read_back_text(&mut self) -> Result<String, String> {
        Ok(self.0.visible_text.clone())
    }

    fn paste_text(&mut self, text: &str) -> Result<(), String> {
        self.0.mutate_visible_value_only(text);
        Ok(())
    }

    fn editor_accepts_message(&mut self) -> Result<bool, String> {
        Ok(self.0.send_button_available())
    }
}

fn concrete_signal_only_placement_count() -> usize {
    let signal_source = include_str!("../src/native_signal_adapter.rs");
    [
        "SignalComposerPlacementBackend",
        "SignalComposerProbeBackend",
        "SignalLivePlacementRequest",
        "SignalLivePlacementReceipt",
        "drive_signal_composer_placement",
        "probe_signal_composer_write_then_clear",
        "place_signal_desktop_carrier",
        "SignalCarrierPlacementRequest",
        "SignalCarrierPlacement",
        "place_signal_carrier",
        "signal_carrier_prefix_proof_sha256",
    ]
    .into_iter()
    .map(|identifier| signal_source.matches(identifier).count())
    .sum()
}

fn run_finish_line() -> (SharedPlaceTextReceipt, SignalRichEditor) {
    let mut signal = SignalRichEditor::default();

    signal.mutate_visible_value_only(fixture::MARKED_TEXT);
    assert_eq!(
        signal.visible_text.as_bytes(),
        fixture::MARKED_TEXT.as_bytes()
    );
    assert!(
        !signal.send_button_available(),
        "visible-only mutation must not satisfy Signal's editor-owned state"
    );
    signal.visible_text.clear();

    let receipt = place_text_through_shared_job(&mut signal, fixture::MARKED_TEXT)
        .expect("the shared place-text job drives Signal's editor-owned state");
    (receipt, signal)
}

#[test]
fn task_1033_marked_bytes_use_the_shared_job_and_enable_signals_send_button() {
    let shared_source = include_str!("../examples/task_3406_place_text.rs");
    assert!(shared_source.contains("pub fn place_text_through_shared_job("));
    assert!(shared_source.contains("super::place_text_through_shared_job("));
    assert!(shared_source.contains("SetClipboardData"));
    assert!(shared_source.contains("send_ctrl_v()?;"));
    assert!(shared_source.contains("TASK1033_SIGNAL_SEND_AFTER_AVAILABLE"));
    assert!(!shared_source.contains("SetValue"));

    let (receipt, signal) = run_finish_line();
    let signal_only_count = concrete_signal_only_placement_count();

    assert_eq!(signal.shared_paste_calls, 1);
    assert_eq!(
        signal.visible_text.as_bytes(),
        fixture::MARKED_TEXT.as_bytes()
    );
    assert_eq!(
        signal.editor_owned_text.as_bytes(),
        fixture::MARKED_TEXT.as_bytes()
    );
    assert_eq!(receipt.placed_bytes, fixture::MARKED_TEXT.len());
    assert_eq!(receipt.readback_bytes, fixture::MARKED_TEXT.len());
    assert!(receipt.readback_exact);
    assert!(receipt.editor_accepts_message);
    assert!(signal.send_button_available());
    assert_eq!(signal_only_count, 0);

    println!("TASK1033_MARKED_STRING={}", fixture::MARKED_TEXT);
    println!("TASK1033_MARKED_BYTES={}", receipt.placed_bytes);
    println!("TASK1033_READBACK_BYTES={}", receipt.readback_bytes);
    println!("TASK1033_READBACK_EXACT={}", receipt.readback_exact);
    println!(
        "TASK1033_SHARED_PLACE_JOB_CALLS={}",
        signal.shared_paste_calls
    );
    println!(
        "TASK1033_SIGNAL_SEND_BUTTON_AVAILABLE={}",
        signal.send_button_available()
    );
    println!("TASK1033_SIGNAL_ONLY_PLACING_CODE={signal_only_count}");
}

#[test]
fn task_1033_visible_only_mutation_goes_red_at_signal_readiness() {
    let mut mutant = BehindTheEditorMutation(SignalRichEditor::default());
    let error = place_text_through_shared_job(&mut mutant, fixture::MARKED_TEXT)
        .expect_err("behind-the-editor mutation must fail the Signal readiness proof");

    assert_eq!(
        mutant.0.visible_text.as_bytes(),
        fixture::MARKED_TEXT.as_bytes()
    );
    assert!(mutant.0.editor_owned_text.is_empty());
    assert!(!mutant.0.send_button_available());
    assert_eq!(
        error,
        "provider editor did not make its send control available"
    );
    println!(
        "TASK1033_MUTANT_VISIBLE_BYTES={}",
        mutant.0.visible_text.len()
    );
    println!("TASK1033_MUTANT_EDITOR_OWNED_BYTES=0");
    println!("TASK1033_MUTANT_SEND_BUTTON_AVAILABLE=false");
    println!("TASK1033_MUTANT_RESULT=red");
}

fn task1034_results(editor: &mut SignalRichEditor) -> Result<Vec<String>, String> {
    place_text_through_shared_job(editor, TASK1034_TEXT)?;
    Ok(vec![TASK1034_TEXT.to_owned()])
}

#[test]
fn task_1034_changed_placed_byte_is_refused_and_good_signal_result_is_unchanged() {
    let mut good_editor = SignalRichEditor::default();
    let good = task1034_results(&mut good_editor)
        .expect("good signal-text-1034 must pass exact Signal read-back");
    assert_eq!(good, [TASK1034_TEXT]);
    assert_eq!(good.len(), 1);

    let mut changed_editor = SignalRichEditor::changing_one_placed_byte_before_read_back();
    let refusal = task1034_results(&mut changed_editor)
        .expect_err("changed placed byte x must be refused by name");
    assert_eq!(
        refusal,
        "shared place-text read-back changed the marked bytes"
    );
    assert_eq!(
        changed_editor.task1034_events,
        ["placed", "changed-before-read-back"],
        "fault injection must occur after placement and before returned read-back"
    );
    assert_eq!(
        changed_editor.shared_paste_calls, 1,
        "the refused attempt places exactly once"
    );
    let changed = changed_editor
        .changed_read_back
        .as_deref()
        .expect("the placed Signal byte was changed before read-back");
    assert_eq!(changed, b"xignal-text-1034");
    assert_eq!(
        changed
            .iter()
            .zip(TASK1034_TEXT.as_bytes())
            .filter(|(actual, placed)| actual != placed)
            .count(),
        1,
        "fault injection must alter exactly one placed byte"
    );

    let mut restored_editor = SignalRichEditor::default();
    let restored = task1034_results(&mut restored_editor)
        .expect("restored signal-text-1034 must pass exact Signal read-back");
    assert_eq!(restored, good);
    assert_eq!(restored.len(), 1);

    println!(
        "TASK1034 good_text={TASK1034_TEXT} result_count={} result_name={}",
        good.len(),
        good[0]
    );
    println!(
        "TASK1034 changed_placed_byte={} changed_readback=xignal-text-1034 result=refused refusal_name={refusal}",
        char::from(TASK1034_CHANGED_PLACED_BYTE)
    );
    println!("TASK1034 mutation_order=placed,changed-before-read-back changed_byte_count=1");
    println!(
        "TASK1034 restored_text={TASK1034_TEXT} result_count={} result_name={}",
        restored.len(),
        restored[0]
    );
}
