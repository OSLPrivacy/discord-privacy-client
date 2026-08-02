//! Renderer-event bridge for native attachment progress.
//!
//! The job registry owns non-serializable source and key material. This module
//! is its only event boundary: it publishes copies of the registry's safe DTO
//! after each meaningful state transition, never the native job itself.

use crate::native_attachment_jobs::{
    NativeAttachmentFailure, NativeAttachmentJobDto, NativeAttachmentJobError,
    NativeAttachmentJobRegistry, NativeAttachmentSecrets, NativeAttachmentStage,
};
use std::collections::HashMap;

pub const NATIVE_ATTACHMENT_PROGRESS_EVENT: &str = "osl://attachment-progress";

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeAttachmentProgressEvent {
    /// Renderer-owned opaque conversation token, used only to select the
    /// matching local progress component. It is not a service identity.
    pub context_id: String,
    pub job: NativeAttachmentJobDto,
}

/// Deliberately tiny event port. The native state machine stays independent of
/// Tauri, while the desktop adapter can forward these safe values to the UI.
pub trait NativeAttachmentProgressSink {
    fn emit(&mut self, event: NativeAttachmentProgressEvent);
}

pub struct NativeAttachmentProgressBridge<S> {
    registry: NativeAttachmentJobRegistry,
    sink: S,
    last_emitted: HashMap<String, NativeAttachmentJobDto>,
}

impl<S> NativeAttachmentProgressBridge<S>
where
    S: NativeAttachmentProgressSink,
{
    pub fn new(sink: S) -> Self {
        Self {
            registry: NativeAttachmentJobRegistry::default(),
            sink,
            last_emitted: HashMap::new(),
        }
    }

    pub fn stage(
        &mut self,
        context_id: &str,
        filename: &str,
        media_type: &str,
        size: u64,
        secrets: NativeAttachmentSecrets,
        now_ms: u64,
    ) -> Result<NativeAttachmentJobDto, NativeAttachmentJobError> {
        let job = self
            .registry
            .stage(context_id, filename, media_type, size, secrets, now_ms)?;
        self.emit_changed(context_id, &job);
        Ok(job)
    }

    pub fn begin_protection(
        &mut self,
        context_id: &str,
        job_id: &str,
        now_ms: u64,
    ) -> Result<NativeAttachmentJobDto, NativeAttachmentJobError> {
        let job = self.registry.begin_protection(context_id, job_id, now_ms)?;
        self.emit_changed(context_id, &job);
        Ok(job)
    }

    pub fn advance(
        &mut self,
        context_id: &str,
        job_id: &str,
        next: NativeAttachmentStage,
        now_ms: u64,
    ) -> Result<NativeAttachmentJobDto, NativeAttachmentJobError> {
        let job = self.registry.advance(context_id, job_id, next, now_ms)?;
        self.emit_changed(context_id, &job);
        Ok(job)
    }

    pub fn report_progress(
        &mut self,
        context_id: &str,
        job_id: &str,
        reported_percent: u8,
        now_ms: u64,
    ) -> Result<NativeAttachmentJobDto, NativeAttachmentJobError> {
        let job = self
            .registry
            .report_progress(context_id, job_id, reported_percent, now_ms)?;
        self.emit_changed(context_id, &job);
        Ok(job)
    }

    pub fn fail(
        &mut self,
        context_id: &str,
        job_id: &str,
        failure: NativeAttachmentFailure,
    ) -> Result<NativeAttachmentJobDto, NativeAttachmentJobError> {
        let job = self.registry.fail(context_id, job_id, failure)?;
        self.emit_changed(context_id, &job);
        Ok(job)
    }

    pub fn cancel(
        &mut self,
        context_id: &str,
        job_id: &str,
    ) -> Result<NativeAttachmentJobDto, NativeAttachmentJobError> {
        let job = self.registry.cancel(context_id, job_id)?;
        self.emit_changed(context_id, &job);
        Ok(job)
    }

    pub fn retry(
        &mut self,
        context_id: &str,
        job_id: &str,
        now_ms: u64,
    ) -> Result<NativeAttachmentJobDto, NativeAttachmentJobError> {
        let job = self.registry.retry(context_id, job_id, now_ms)?;
        self.emit_changed(context_id, &job);
        Ok(job)
    }

    fn emit_changed(&mut self, context_id: &str, job: &NativeAttachmentJobDto) {
        if self.last_emitted.get(context_id) == Some(job) {
            return;
        }
        self.last_emitted.insert(context_id.to_owned(), job.clone());
        self.sink.emit(NativeAttachmentProgressEvent {
            context_id: context_id.to_owned(),
            job: job.clone(),
        });
    }
}

#[cfg(feature = "desktop")]
pub struct TauriAttachmentProgressSink {
    app: tauri::AppHandle,
    target: String,
}

#[cfg(feature = "desktop")]
impl TauriAttachmentProgressSink {
    /// Progress is addressed to a trusted local webview label. Callers must
    /// never pass a service-hosted child window here.
    pub fn for_trusted_window(app: tauri::AppHandle, target: impl Into<String>) -> Self {
        Self {
            app,
            target: target.into(),
        }
    }
}

#[cfg(feature = "desktop")]
impl NativeAttachmentProgressSink for TauriAttachmentProgressSink {
    fn emit(&mut self, event: NativeAttachmentProgressEvent) {
        use tauri::Emitter;

        let _ = self
            .app
            .emit_to(&self.target, NATIVE_ATTACHMENT_PROGRESS_EVENT, event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingSink(Vec<NativeAttachmentProgressEvent>);

    impl NativeAttachmentProgressSink for RecordingSink {
        fn emit(&mut self, event: NativeAttachmentProgressEvent) {
            self.0.push(event);
        }
    }

    fn secrets() -> NativeAttachmentSecrets {
        NativeAttachmentSecrets::new(vec![7; 16], vec![8; 32]).unwrap()
    }

    #[test]
    fn emits_every_intermediate_attachment_stage_in_order() {
        let mut bridge = NativeAttachmentProgressBridge::new(RecordingSink::default());
        let selected = bridge
            .stage(
                "chat:opaque-42",
                "photo.png",
                "image/png",
                2_048,
                secrets(),
                1_000,
            )
            .unwrap();
        bridge
            .begin_protection("chat:opaque-42", &selected.job_id, 1_500)
            .unwrap();
        bridge
            .report_progress("chat:opaque-42", &selected.job_id, 30, 2_000)
            .unwrap();
        bridge
            .advance(
                "chat:opaque-42",
                &selected.job_id,
                NativeAttachmentStage::Uploading,
                2_500,
            )
            .unwrap();
        bridge
            .advance(
                "chat:opaque-42",
                &selected.job_id,
                NativeAttachmentStage::Delivering,
                3_000,
            )
            .unwrap();
        bridge
            .advance(
                "chat:opaque-42",
                &selected.job_id,
                NativeAttachmentStage::Sent,
                3_500,
            )
            .unwrap();

        let events = bridge.sink.0;
        assert_eq!(events.len(), 6);
        assert!(events
            .iter()
            .all(|event| event.context_id == "chat:opaque-42"));
        assert_eq!(
            events
                .iter()
                .map(|event| event.job.stage)
                .collect::<Vec<_>>(),
            vec![
                NativeAttachmentStage::Selected,
                NativeAttachmentStage::Protecting,
                NativeAttachmentStage::Protecting,
                NativeAttachmentStage::Uploading,
                NativeAttachmentStage::Delivering,
                NativeAttachmentStage::Sent,
            ]
        );
        assert_eq!(events[2].job.progress, 25);
        assert_eq!(events[5].job.progress, 100);
    }

    #[test]
    fn suppresses_a_throttled_duplicate_snapshot() {
        let mut bridge = NativeAttachmentProgressBridge::new(RecordingSink::default());
        let selected = bridge
            .stage(
                "chat:opaque-42",
                "photo.png",
                "image/png",
                2_048,
                secrets(),
                1_000,
            )
            .unwrap();
        bridge
            .begin_protection("chat:opaque-42", &selected.job_id, 1_500)
            .unwrap();
        bridge
            .report_progress("chat:opaque-42", &selected.job_id, 40, 1_700)
            .unwrap();

        assert_eq!(bridge.sink.0.len(), 2);
    }
}
