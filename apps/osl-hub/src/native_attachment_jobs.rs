//! Native-only protected-media staging state.
//!
//! Serializable values in this module contain only an opaque job identifier,
//! sanitized display metadata, bounded caption/options, and coarse state. File
//! handles, paths, plaintext, upload capabilities, and keys remain in the
//! non-serializable secret half of each job and zeroize when replaced/removed.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fmt;
#[cfg(unix)]
use std::io::Read;
use zeroize::{Zeroize, Zeroizing};

pub const MAX_ATTACHMENT_SIZE: u64 = 512 * 1024 * 1024;
pub const MAX_CAPTION_BYTES: usize = 4_096;
pub const PROGRESS_THROTTLE_MS: u64 = 500;
pub const AUTHENTICATED_FIELDS: [&str; 4] = ["jobId", "metadata", "caption", "viewOnce"];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NativeAttachmentStage {
    Selected,
    Protecting,
    Uploading,
    Delivering,
    Sent,
    Failed,
    Cancelled,
}

impl NativeAttachmentStage {
    fn active(self) -> bool {
        matches!(self, Self::Protecting | Self::Uploading | Self::Delivering)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NativeAttachmentFailure {
    Offline,
    Protection,
    Upload,
    Delivery,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SafeAttachmentMetadata {
    pub filename: String,
    pub media_type: String,
    pub size: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeAttachmentJobDto {
    pub job_id: String,
    pub metadata: SafeAttachmentMetadata,
    pub caption: String,
    pub view_once: bool,
    pub stage: NativeAttachmentStage,
    pub progress: u8,
    pub retry_from: Option<NativeAttachmentStage>,
    pub failure: Option<NativeAttachmentFailure>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthenticatedAttachmentPlan {
    pub protocol: &'static str,
    pub job_id: String,
    pub metadata: SafeAttachmentMetadata,
    pub caption: String,
    pub view_once: bool,
    pub authenticated_fields: [&'static str; 4],
}

/// Native-only material. It deliberately implements neither `Serialize`,
/// `Debug`, nor `Clone`; both allocations zeroize on drop.
pub struct NativeAttachmentSecrets {
    source_reference: Zeroizing<Vec<u8>>,
    key_material: Zeroizing<Vec<u8>>,
}

impl NativeAttachmentSecrets {
    pub fn new(
        mut source_reference: Vec<u8>,
        mut key_material: Vec<u8>,
    ) -> Result<Self, NativeAttachmentJobError> {
        if source_reference.is_empty() || key_material.is_empty() {
            source_reference.zeroize();
            key_material.zeroize();
            return Err(NativeAttachmentJobError::MissingNativeSecret);
        }
        Ok(Self {
            source_reference: Zeroizing::new(source_reference),
            key_material: Zeroizing::new(key_material),
        })
    }

    /// Native transport integration seam. The callback cannot retain a borrow;
    /// ownership and drop-time zeroization stay with this job registry.
    pub fn with_native_material<T>(&self, use_material: impl FnOnce(&[u8], &[u8]) -> T) -> T {
        use_material(&self.source_reference, &self.key_material)
    }

    pub fn zeroize_now(&mut self) {
        self.source_reference.zeroize();
        self.key_material.zeroize();
    }
}

impl fmt::Debug for NativeAttachmentSecrets {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("NativeAttachmentSecrets([REDACTED])")
    }
}

struct NativeAttachmentJob {
    dto: NativeAttachmentJobDto,
    progress_updated_at_ms: u64,
    secrets: NativeAttachmentSecrets,
}

impl Drop for NativeAttachmentJob {
    fn drop(&mut self) {
        self.dto.caption.zeroize();
        self.secrets.zeroize_now();
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum NativeAttachmentJobError {
    InvalidContext,
    InvalidFilename,
    InvalidMediaType,
    InvalidSize,
    CaptionTooLong,
    CaptionContainsNul,
    MissingNativeSecret,
    EntropyUnavailable,
    JobNotFound,
    JobMismatch,
    InvalidTransition,
    ClockRegression,
}

impl fmt::Display for NativeAttachmentJobError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidContext => "invalid native attachment context",
            Self::InvalidFilename => "invalid attachment filename",
            Self::InvalidMediaType => "invalid attachment media type",
            Self::InvalidSize => "invalid attachment size",
            Self::CaptionTooLong => "attachment caption is too long",
            Self::CaptionContainsNul => "attachment caption contains a NUL",
            Self::MissingNativeSecret => "native attachment material is missing",
            Self::EntropyUnavailable => "operating-system entropy is unavailable",
            Self::JobNotFound => "native attachment job was not found",
            Self::JobMismatch => "opaque native attachment job identifier did not match",
            Self::InvalidTransition => "invalid native attachment state transition",
            Self::ClockRegression => "native attachment clock regressed",
        })
    }
}

impl std::error::Error for NativeAttachmentJobError {}

#[derive(Default)]
pub struct NativeAttachmentJobRegistry {
    jobs: HashMap<String, NativeAttachmentJob>,
}

impl NativeAttachmentJobRegistry {
    pub fn stage(
        &mut self,
        context_id: &str,
        filename: &str,
        media_type: &str,
        size: u64,
        secrets: NativeAttachmentSecrets,
        now_ms: u64,
    ) -> Result<NativeAttachmentJobDto, NativeAttachmentJobError> {
        validate_context(context_id)?;
        let metadata = SafeAttachmentMetadata {
            filename: sanitize_filename(filename)?,
            media_type: sanitize_media_type(media_type)?,
            size: validate_size(size)?,
        };
        let mut job_id = mint_opaque_job_id()?;
        for _ in 0..8 {
            if self.jobs.values().all(|job| job.dto.job_id != job_id) {
                break;
            }
            job_id = mint_opaque_job_id()?;
        }
        if self.jobs.values().any(|job| job.dto.job_id == job_id) {
            return Err(NativeAttachmentJobError::EntropyUnavailable);
        }
        let dto = NativeAttachmentJobDto {
            job_id,
            metadata,
            caption: String::new(),
            view_once: false,
            stage: NativeAttachmentStage::Selected,
            progress: 0,
            retry_from: None,
            failure: None,
        };
        // Replacement drops and zeroizes the previous context's native-only
        // material. A context can never have two staged files.
        self.jobs.insert(
            context_id.to_owned(),
            NativeAttachmentJob {
                dto: dto.clone(),
                progress_updated_at_ms: now_ms,
                secrets,
            },
        );
        Ok(dto)
    }

    pub fn snapshot(&self, context_id: &str) -> Option<NativeAttachmentJobDto> {
        self.jobs.get(context_id).map(|job| job.dto.clone())
    }

    pub fn set_caption(
        &mut self,
        context_id: &str,
        job_id: &str,
        caption: String,
    ) -> Result<NativeAttachmentJobDto, NativeAttachmentJobError> {
        validate_caption(&caption)?;
        let job = self.editable_job_mut(context_id, job_id)?;
        job.dto.caption.zeroize();
        job.dto.caption = caption;
        Ok(job.dto.clone())
    }

    pub fn set_view_once(
        &mut self,
        context_id: &str,
        job_id: &str,
        view_once: bool,
    ) -> Result<NativeAttachmentJobDto, NativeAttachmentJobError> {
        let job = self.editable_job_mut(context_id, job_id)?;
        job.dto.view_once = view_once;
        Ok(job.dto.clone())
    }

    pub fn authenticated_plan(
        &self,
        context_id: &str,
        job_id: &str,
    ) -> Result<AuthenticatedAttachmentPlan, NativeAttachmentJobError> {
        let job = self.matching_job(context_id, job_id)?;
        if !is_editable(job.dto.stage) {
            return Err(NativeAttachmentJobError::InvalidTransition);
        }
        validate_caption(&job.dto.caption)?;
        Ok(AuthenticatedAttachmentPlan {
            protocol: "osl-discord-media-v1",
            job_id: job.dto.job_id.clone(),
            metadata: job.dto.metadata.clone(),
            caption: job.dto.caption.clone(),
            view_once: job.dto.view_once,
            authenticated_fields: AUTHENTICATED_FIELDS,
        })
    }

    pub fn begin_protection(
        &mut self,
        context_id: &str,
        job_id: &str,
        now_ms: u64,
    ) -> Result<NativeAttachmentJobDto, NativeAttachmentJobError> {
        let job = self.editable_job_mut(context_id, job_id)?;
        job.dto.stage = NativeAttachmentStage::Protecting;
        job.dto.progress = 0;
        job.dto.retry_from = None;
        job.dto.failure = None;
        job.progress_updated_at_ms = now_ms;
        Ok(job.dto.clone())
    }

    pub fn advance(
        &mut self,
        context_id: &str,
        job_id: &str,
        next: NativeAttachmentStage,
        now_ms: u64,
    ) -> Result<NativeAttachmentJobDto, NativeAttachmentJobError> {
        let job = self.matching_job_mut(context_id, job_id)?;
        if now_ms < job.progress_updated_at_ms {
            return Err(NativeAttachmentJobError::ClockRegression);
        }
        let allowed = matches!(
            (job.dto.stage, next),
            (
                NativeAttachmentStage::Protecting,
                NativeAttachmentStage::Uploading
            ) | (
                NativeAttachmentStage::Uploading,
                NativeAttachmentStage::Delivering
            ) | (
                NativeAttachmentStage::Delivering,
                NativeAttachmentStage::Sent
            )
        );
        if !allowed {
            return Err(NativeAttachmentJobError::InvalidTransition);
        }
        job.dto.stage = next;
        job.dto.progress = if next == NativeAttachmentStage::Sent {
            100
        } else {
            0
        };
        job.dto.retry_from = None;
        job.dto.failure = None;
        job.progress_updated_at_ms = now_ms;
        Ok(job.dto.clone())
    }

    pub fn report_progress(
        &mut self,
        context_id: &str,
        job_id: &str,
        reported_percent: u8,
        now_ms: u64,
    ) -> Result<NativeAttachmentJobDto, NativeAttachmentJobError> {
        let job = self.matching_job_mut(context_id, job_id)?;
        if !job.dto.stage.active() {
            return Err(NativeAttachmentJobError::InvalidTransition);
        }
        if now_ms < job.progress_updated_at_ms {
            return Err(NativeAttachmentJobError::ClockRegression);
        }
        if now_ms - job.progress_updated_at_ms < PROGRESS_THROTTLE_MS {
            return Ok(job.dto.clone());
        }
        let next = coarse_progress(reported_percent);
        if next > job.dto.progress
            && (next < 100 || job.dto.stage == NativeAttachmentStage::Delivering)
        {
            job.dto.progress = next;
            job.progress_updated_at_ms = now_ms;
        }
        Ok(job.dto.clone())
    }

    pub fn fail(
        &mut self,
        context_id: &str,
        job_id: &str,
        failure: NativeAttachmentFailure,
    ) -> Result<NativeAttachmentJobDto, NativeAttachmentJobError> {
        let job = self.matching_job_mut(context_id, job_id)?;
        if !job.dto.stage.active() {
            return Err(NativeAttachmentJobError::InvalidTransition);
        }
        job.dto.retry_from = Some(job.dto.stage);
        job.dto.stage = NativeAttachmentStage::Failed;
        job.dto.failure = Some(failure);
        Ok(job.dto.clone())
    }

    pub fn cancel(
        &mut self,
        context_id: &str,
        job_id: &str,
    ) -> Result<NativeAttachmentJobDto, NativeAttachmentJobError> {
        let job = self.matching_job_mut(context_id, job_id)?;
        if matches!(
            job.dto.stage,
            NativeAttachmentStage::Sent | NativeAttachmentStage::Cancelled
        ) {
            return Err(NativeAttachmentJobError::InvalidTransition);
        }
        if job.dto.stage.active() {
            job.dto.retry_from = Some(job.dto.stage);
        }
        job.dto.stage = NativeAttachmentStage::Cancelled;
        job.dto.failure = None;
        Ok(job.dto.clone())
    }

    pub fn retry(
        &mut self,
        context_id: &str,
        job_id: &str,
        now_ms: u64,
    ) -> Result<NativeAttachmentJobDto, NativeAttachmentJobError> {
        let job = self.matching_job_mut(context_id, job_id)?;
        if !matches!(
            job.dto.stage,
            NativeAttachmentStage::Failed | NativeAttachmentStage::Cancelled
        ) {
            return Err(NativeAttachmentJobError::InvalidTransition);
        }
        job.dto.stage = NativeAttachmentStage::Protecting;
        job.dto.progress = 0;
        job.dto.retry_from = None;
        job.dto.failure = None;
        job.progress_updated_at_ms = now_ms;
        Ok(job.dto.clone())
    }

    pub fn with_native_material<T>(
        &self,
        context_id: &str,
        job_id: &str,
        use_material: impl FnOnce(&[u8], &[u8]) -> T,
    ) -> Result<T, NativeAttachmentJobError> {
        let job = self.matching_job(context_id, job_id)?;
        Ok(job.secrets.with_native_material(use_material))
    }

    pub fn remove(
        &mut self,
        context_id: &str,
        job_id: &str,
    ) -> Result<(), NativeAttachmentJobError> {
        let job = self.matching_job(context_id, job_id)?;
        if job.dto.stage.active() {
            return Err(NativeAttachmentJobError::InvalidTransition);
        }
        let mut removed = self
            .jobs
            .remove(context_id)
            .ok_or(NativeAttachmentJobError::JobNotFound)?;
        removed.dto.caption.zeroize();
        removed.secrets.zeroize_now();
        Ok(())
    }

    pub fn clear_context(&mut self, context_id: &str) {
        if let Some(mut removed) = self.jobs.remove(context_id) {
            removed.dto.caption.zeroize();
            removed.secrets.zeroize_now();
        }
    }

    fn matching_job(
        &self,
        context_id: &str,
        job_id: &str,
    ) -> Result<&NativeAttachmentJob, NativeAttachmentJobError> {
        let job = self
            .jobs
            .get(context_id)
            .ok_or(NativeAttachmentJobError::JobNotFound)?;
        if job.dto.job_id != job_id {
            return Err(NativeAttachmentJobError::JobMismatch);
        }
        Ok(job)
    }

    fn matching_job_mut(
        &mut self,
        context_id: &str,
        job_id: &str,
    ) -> Result<&mut NativeAttachmentJob, NativeAttachmentJobError> {
        let job = self
            .jobs
            .get_mut(context_id)
            .ok_or(NativeAttachmentJobError::JobNotFound)?;
        if job.dto.job_id != job_id {
            return Err(NativeAttachmentJobError::JobMismatch);
        }
        Ok(job)
    }

    fn editable_job_mut(
        &mut self,
        context_id: &str,
        job_id: &str,
    ) -> Result<&mut NativeAttachmentJob, NativeAttachmentJobError> {
        let job = self.matching_job_mut(context_id, job_id)?;
        if !is_editable(job.dto.stage) {
            return Err(NativeAttachmentJobError::InvalidTransition);
        }
        Ok(job)
    }
}

fn is_editable(stage: NativeAttachmentStage) -> bool {
    matches!(
        stage,
        NativeAttachmentStage::Selected
            | NativeAttachmentStage::Failed
            | NativeAttachmentStage::Cancelled
    )
}

fn validate_context(value: &str) -> Result<(), NativeAttachmentJobError> {
    if (8..=256).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':' | b'.'))
    {
        Ok(())
    } else {
        Err(NativeAttachmentJobError::InvalidContext)
    }
}

fn sanitize_filename(value: &str) -> Result<String, NativeAttachmentJobError> {
    let leaf = value.rsplit(['/', '\\']).next().unwrap_or_default();
    let mut output = String::new();
    let mut pending_space = false;
    for character in leaf.chars() {
        if character.is_control() {
            continue;
        }
        if character.is_whitespace() {
            pending_space = !output.is_empty();
            continue;
        }
        if pending_space && output.len() < 255 {
            output.push(' ');
        }
        pending_space = false;
        if output.len() + character.len_utf8() > 255 {
            break;
        }
        output.push(character);
    }
    if output.is_empty() || matches!(output.as_str(), "." | "..") {
        Err(NativeAttachmentJobError::InvalidFilename)
    } else {
        Ok(output)
    }
}

fn sanitize_media_type(value: &str) -> Result<String, NativeAttachmentJobError> {
    if value.len() > 127 || value.bytes().any(|byte| byte.is_ascii_uppercase()) {
        return Err(NativeAttachmentJobError::InvalidMediaType);
    }
    let mut parts = value.split('/');
    let major = parts.next().unwrap_or_default();
    let minor = parts.next().unwrap_or_default();
    if major.is_empty()
        || minor.is_empty()
        || parts.next().is_some()
        || !major.bytes().all(valid_media_type_byte)
        || !minor.bytes().all(valid_media_type_byte)
    {
        return Err(NativeAttachmentJobError::InvalidMediaType);
    }
    Ok(value.to_owned())
}

fn valid_media_type_byte(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"!#$&^_.+-".contains(&byte)
}

fn validate_size(size: u64) -> Result<u64, NativeAttachmentJobError> {
    if (1..=MAX_ATTACHMENT_SIZE).contains(&size) {
        Ok(size)
    } else {
        Err(NativeAttachmentJobError::InvalidSize)
    }
}

fn validate_caption(caption: &str) -> Result<(), NativeAttachmentJobError> {
    if caption.contains('\0') {
        Err(NativeAttachmentJobError::CaptionContainsNul)
    } else if caption.len() > MAX_CAPTION_BYTES {
        Err(NativeAttachmentJobError::CaptionTooLong)
    } else {
        Ok(())
    }
}

fn coarse_progress(reported: u8) -> u8 {
    match reported {
        100 => 100,
        75..=99 => 75,
        50..=74 => 50,
        25..=49 => 25,
        _ => 0,
    }
}

fn mint_opaque_job_id() -> Result<String, NativeAttachmentJobError> {
    let mut entropy = [0_u8; 32];
    fill_os_entropy(&mut entropy)?;
    let mut hasher = Sha256::new();
    hasher.update(b"osl-native-attachment-job-v1");
    hasher.update(&entropy);
    let digest = hasher.finalize();
    entropy.zeroize();
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut encoded, "{byte:02x}").expect("writing into a String cannot fail");
    }
    Ok(encoded)
}

#[cfg(target_os = "windows")]
fn fill_os_entropy(destination: &mut [u8]) -> Result<(), NativeAttachmentJobError> {
    use windows_sys::Win32::Security::Cryptography::{
        BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG,
    };
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            destination.as_mut_ptr(),
            destination.len() as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status == 0 {
        Ok(())
    } else {
        Err(NativeAttachmentJobError::EntropyUnavailable)
    }
}

#[cfg(unix)]
fn fill_os_entropy(destination: &mut [u8]) -> Result<(), NativeAttachmentJobError> {
    std::fs::File::open("/dev/urandom")
        .and_then(|mut source| source.read_exact(destination))
        .map_err(|_| NativeAttachmentJobError::EntropyUnavailable)
}

#[cfg(not(any(unix, target_os = "windows")))]
fn fill_os_entropy(_destination: &mut [u8]) -> Result<(), NativeAttachmentJobError> {
    Err(NativeAttachmentJobError::EntropyUnavailable)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTEXT: &str = "discord:conversation:opaque-123";

    fn secrets(marker: u8) -> NativeAttachmentSecrets {
        NativeAttachmentSecrets::new(vec![marker; 16], vec![marker.wrapping_add(1); 32]).unwrap()
    }

    fn staged(registry: &mut NativeAttachmentJobRegistry) -> NativeAttachmentJobDto {
        registry
            .stage(
                CONTEXT,
                "C:\\Users\\liam\\ private  photo.png",
                "image/png",
                2_048,
                secrets(7),
                1_000,
            )
            .unwrap()
    }

    #[test]
    fn stages_only_safe_dto_metadata_with_an_opaque_identifier() {
        let mut registry = NativeAttachmentJobRegistry::default();
        let dto = staged(&mut registry);
        assert_eq!(dto.metadata.filename, "private photo.png");
        assert_eq!(dto.metadata.media_type, "image/png");
        assert_eq!(dto.metadata.size, 2_048);
        assert_eq!(dto.job_id.len(), 64);
        assert!(dto.job_id.bytes().all(|byte| byte.is_ascii_hexdigit()));

        let serialized = serde_json::to_string(&dto).unwrap();
        assert!(!serialized.contains("Users"));
        assert!(!serialized.contains("sourceReference"));
        assert!(!serialized.contains("keyMaterial"));
        assert!(!serialized.contains("base64"));
        assert!(!serialized.contains("bytes"));
    }

    #[test]
    fn enforces_one_job_per_context_and_mints_unrelated_ids() {
        let mut registry = NativeAttachmentJobRegistry::default();
        let first = staged(&mut registry);
        let second = registry
            .stage(CONTEXT, "second.png", "image/png", 9, secrets(9), 2_000)
            .unwrap();
        assert_ne!(first.job_id, second.job_id);
        assert_eq!(registry.snapshot(CONTEXT), Some(second));
        assert_eq!(
            registry.authenticated_plan(CONTEXT, &first.job_id),
            Err(NativeAttachmentJobError::JobMismatch)
        );
    }

    #[test]
    fn bounds_caption_and_declares_every_field_that_transport_must_authenticate() {
        let mut registry = NativeAttachmentJobRegistry::default();
        let dto = staged(&mut registry);
        let updated = registry
            .set_caption(CONTEXT, &dto.job_id, "private caption".to_owned())
            .unwrap();
        let updated = registry
            .set_view_once(CONTEXT, &updated.job_id, true)
            .unwrap();
        let plan = registry
            .authenticated_plan(CONTEXT, &updated.job_id)
            .unwrap();
        assert_eq!(plan.caption, "private caption");
        assert!(plan.view_once);
        assert_eq!(
            plan.authenticated_fields,
            ["jobId", "metadata", "caption", "viewOnce"]
        );
        assert_eq!(plan.protocol, "osl-discord-media-v1");
        assert_eq!(
            registry.set_caption(CONTEXT, &updated.job_id, "🙂".repeat(MAX_CAPTION_BYTES)),
            Err(NativeAttachmentJobError::CaptionTooLong)
        );
    }

    #[test]
    fn enforces_ordered_truthful_stages_and_coarse_throttled_progress() {
        let mut registry = NativeAttachmentJobRegistry::default();
        let dto = staged(&mut registry);
        assert_eq!(
            registry.advance(
                CONTEXT,
                &dto.job_id,
                NativeAttachmentStage::Uploading,
                1_500
            ),
            Err(NativeAttachmentJobError::InvalidTransition)
        );
        let protecting = registry
            .begin_protection(CONTEXT, &dto.job_id, 2_000)
            .unwrap();
        assert_eq!(
            registry
                .report_progress(CONTEXT, &dto.job_id, 49, 2_200)
                .unwrap()
                .progress,
            0
        );
        assert_eq!(
            registry
                .report_progress(CONTEXT, &dto.job_id, 49, 2_500)
                .unwrap()
                .progress,
            25
        );
        assert_eq!(
            registry
                .report_progress(CONTEXT, &dto.job_id, 10, 3_000)
                .unwrap()
                .progress,
            25
        );
        assert_eq!(
            registry
                .report_progress(CONTEXT, &dto.job_id, 100, 3_000)
                .unwrap()
                .progress,
            25
        );
        assert_eq!(protecting.stage, NativeAttachmentStage::Protecting);
        registry
            .advance(
                CONTEXT,
                &dto.job_id,
                NativeAttachmentStage::Uploading,
                3_500,
            )
            .unwrap();
        registry
            .advance(
                CONTEXT,
                &dto.job_id,
                NativeAttachmentStage::Delivering,
                4_000,
            )
            .unwrap();
        assert_eq!(
            registry
                .report_progress(CONTEXT, &dto.job_id, 100, 4_500)
                .unwrap()
                .progress,
            100
        );
        assert_eq!(
            registry
                .advance(CONTEXT, &dto.job_id, NativeAttachmentStage::Sent, 5_000)
                .unwrap()
                .stage,
            NativeAttachmentStage::Sent
        );
    }

    #[test]
    fn cancellation_failure_and_retry_preserve_safe_user_choices_and_native_material() {
        let mut registry = NativeAttachmentJobRegistry::default();
        let dto = staged(&mut registry);
        registry
            .set_caption(CONTEXT, &dto.job_id, "keep".to_owned())
            .unwrap();
        registry.set_view_once(CONTEXT, &dto.job_id, true).unwrap();
        registry
            .begin_protection(CONTEXT, &dto.job_id, 2_000)
            .unwrap();
        registry
            .advance(
                CONTEXT,
                &dto.job_id,
                NativeAttachmentStage::Uploading,
                2_500,
            )
            .unwrap();
        let failed = registry
            .fail(CONTEXT, &dto.job_id, NativeAttachmentFailure::Offline)
            .unwrap();
        assert_eq!(failed.retry_from, Some(NativeAttachmentStage::Uploading));
        assert_eq!(failed.caption, "keep");
        assert!(failed.view_once);
        let retried = registry.retry(CONTEXT, &dto.job_id, 3_000).unwrap();
        assert_eq!(retried.stage, NativeAttachmentStage::Protecting);
        assert_eq!(
            registry
                .with_native_material(CONTEXT, &dto.job_id, |source, key| (
                    source.len(),
                    key.len()
                ))
                .unwrap(),
            (16, 32)
        );
        let cancelled = registry.cancel(CONTEXT, &dto.job_id).unwrap();
        assert_eq!(
            cancelled.retry_from,
            Some(NativeAttachmentStage::Protecting)
        );
        registry.remove(CONTEXT, &dto.job_id).unwrap();
        assert!(registry.snapshot(CONTEXT).is_none());
    }

    #[test]
    fn rejects_unsafe_metadata_and_redacts_secret_debug_output() {
        assert_eq!(
            sanitize_filename(".."),
            Err(NativeAttachmentJobError::InvalidFilename)
        );
        assert_eq!(
            sanitize_media_type("IMAGE/PNG"),
            Err(NativeAttachmentJobError::InvalidMediaType)
        );
        assert_eq!(validate_size(0), Err(NativeAttachmentJobError::InvalidSize));
        assert_eq!(
            format!("{:?}", secrets(1)),
            "NativeAttachmentSecrets([REDACTED])"
        );
    }
}
