#[path = "../examples/task_3406_place_text.rs"]
mod shared_place_text;

use shared_place_text::{
    place_text_through_shared_job, SharedPlaceTextActions, SharedPlaceTextReceipt,
};

mod fixture {
    include!("fixtures/task_1033_marked_bytes.rs");
}

#[derive(Default)]
struct SignalRichEditor {
    visible_text: String,
    editor_owned_text: String,
    shared_paste_calls: usize,
}

impl SignalRichEditor {
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
        Ok(self.visible_text.clone())
    }

    fn paste_text(&mut self, text: &str) -> Result<(), String> {
        self.shared_paste_calls += 1;
        self.visible_text = text.to_owned();
        self.editor_owned_text = text.to_owned();
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
