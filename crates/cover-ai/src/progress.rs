//! Generation lifecycle signal consumed by the UI bridge.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenerationStage { Queued, Generating, Stalled }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenerationOutcome { Succeeded, FellBack, Failed }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenerationEvent {
    Stage { stage: GenerationStage, progress_percent: u8 },
    Terminal(GenerationOutcome),
}

pub trait ProgressSink { fn emit(&mut self, event: GenerationEvent); }

/// Emit an ordered, non-flashing lifecycle. Call this only for background
/// generation; a pool hit intentionally produces no events.
pub fn generation_started(sink: &mut impl ProgressSink) {
    sink.emit(GenerationEvent::Stage { stage: GenerationStage::Queued, progress_percent: 0 });
    sink.emit(GenerationEvent::Stage { stage: GenerationStage::Generating, progress_percent: 10 });
}
pub fn generation_stalled(sink: &mut impl ProgressSink) {
    sink.emit(GenerationEvent::Stage { stage: GenerationStage::Stalled, progress_percent: 90 });
}
pub fn generation_finished(sink: &mut impl ProgressSink, outcome: GenerationOutcome) { sink.emit(GenerationEvent::Terminal(outcome)); }
