use cover_ai::fallback::{
    select_carrier, CarrierCapabilities, CarrierSource, UserVisibleTransition,
};
use cover_ai::progress::{generation_finished, GenerationEvent, GenerationOutcome, ProgressSink};

#[derive(Default)]
struct Events(Vec<GenerationEvent>);
impl ProgressSink for Events {
    fn emit(&mut self, event: GenerationEvent) {
        self.0.push(event);
    }
}

#[test]
fn t13_tl7_timeout_or_missing_model_completes_with_visible_word_bank_fallback() {
    let decision = select_carrier(CarrierCapabilities {
        ai_model_available: false,
        word_bank_selection_available: true,
    });
    assert_eq!(
        decision.source,
        CarrierSource::WordBankSelectedReadableCover
    );
    assert_eq!(
        decision.transitions,
        vec![UserVisibleTransition::AiToWordBankSelected]
    );
    let mut events = Events::default();
    generation_finished(&mut events, GenerationOutcome::FellBack);
    assert_eq!(
        events.0,
        vec![GenerationEvent::Terminal(GenerationOutcome::FellBack)]
    );
}
