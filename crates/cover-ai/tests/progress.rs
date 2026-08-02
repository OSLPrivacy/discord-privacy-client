use cover_ai::progress::*;

#[derive(Default)] struct Events(Vec<GenerationEvent>);
impl ProgressSink for Events { fn emit(&mut self, event: GenerationEvent) { self.0.push(event); } }

#[test]
fn t13_tl6_stage_event_arrives_while_generation_is_in_flight() {
    let mut events = Events::default(); generation_started(&mut events); generation_stalled(&mut events);
    assert!(matches!(events.0.as_slice(), [GenerationEvent::Stage { stage: GenerationStage::Queued, .. }, GenerationEvent::Stage { stage: GenerationStage::Generating, .. }, GenerationEvent::Stage { stage: GenerationStage::Stalled, .. }]));
}

#[test]
fn t13_tl7_fallback_has_a_terminal_outcome_not_silence() {
    let mut events = Events::default(); generation_started(&mut events); generation_stalled(&mut events); generation_finished(&mut events, GenerationOutcome::FellBack);
    assert!(matches!(events.0.last(), Some(GenerationEvent::Terminal(GenerationOutcome::FellBack))));
}
