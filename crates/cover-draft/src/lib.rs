//! Local, model-agnostic cover-draft orchestration.
//!
//! This crate deliberately has no networking, filesystem, logging, platform,
//! transport, or send APIs. Callers must supply explicitly authorized context,
//! and generation only creates an editable draft that requires a separate,
//! scope-bound approval before it can be consumed once.

use sha2::{Digest, Sha256};
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use thiserror::Error;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

const BINDING_DOMAIN: &[u8] = b"osl-cover-draft-scope-v1";
const DRAFT_DOMAIN: &[u8] = b"osl-cover-draft-content-v1";
const MODEL_PACK_DOMAIN: &[u8] = b"osl-cover-draft-model-pack-v1";

pub const HARD_MAX_CONTEXT_ENTRIES: usize = 64;
pub const HARD_MAX_CONTEXT_BYTES: usize = 256 * 1024;
pub const HARD_MAX_ENTRY_BYTES: usize = 32 * 1024;
pub const HARD_MAX_OUTPUT_BYTES: usize = 8 * 1024;
pub const HARD_MAX_DEADLINE: Duration = Duration::from_secs(30);
pub const HARD_MAX_DRAFT_LIFETIME: Duration = Duration::from_secs(10 * 60);
pub const HARD_MAX_SCOPE_FIELD_BYTES: usize = 512;
pub const HARD_MAX_MODEL_ID_BYTES: usize = 128;
pub const HARD_MIN_MODEL_SIGNATURE_BYTES: usize = 32;
pub const HARD_MAX_MODEL_SIGNATURE_BYTES: usize = 4 * 1024;
pub const HARD_MAX_MODEL_ARTIFACT_BYTES: u64 = 8 * 1024 * 1024 * 1024;
pub const HARD_MAX_MODEL_WORKING_SET_BYTES: u64 = 4 * 1024 * 1024 * 1024;

/// Caller-controlled bounds, additionally constrained by immutable hard caps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub max_context_entries: usize,
    pub max_context_bytes: usize,
    pub max_entry_bytes: usize,
    pub max_output_bytes: usize,
    pub max_generation_time: Duration,
    pub draft_lifetime: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_context_entries: 24,
            max_context_bytes: 64 * 1024,
            max_entry_bytes: 8 * 1024,
            max_output_bytes: 2 * 1024,
            max_generation_time: Duration::from_secs(8),
            draft_lifetime: Duration::from_secs(5 * 60),
        }
    }
}

impl Limits {
    pub fn validate(self) -> Result<Self, Error> {
        let valid = self.max_context_entries > 0
            && self.max_context_entries <= HARD_MAX_CONTEXT_ENTRIES
            && self.max_context_bytes > 0
            && self.max_context_bytes <= HARD_MAX_CONTEXT_BYTES
            && self.max_entry_bytes > 0
            && self.max_entry_bytes <= HARD_MAX_ENTRY_BYTES
            && self.max_entry_bytes <= self.max_context_bytes
            && self.max_output_bytes > 0
            && self.max_output_bytes <= HARD_MAX_OUTPUT_BYTES
            && !self.max_generation_time.is_zero()
            && self.max_generation_time <= HARD_MAX_DEADLINE
            && !self.draft_lifetime.is_zero()
            && self.draft_lifetime <= HARD_MAX_DRAFT_LIFETIME;
        if valid {
            Ok(self)
        } else {
            Err(Error::InvalidLimits)
        }
    }
}

/// Sensitive text that has been deliberately authorized by the caller for one
/// generation request. There is no constructor for implicit history access.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct AuthorizedContextEntry {
    text: String,
}

impl AuthorizedContextEntry {
    pub fn authorize(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }

    fn as_str(&self) -> &str {
        &self.text
    }
}

/// The exact local account/conversation/recipient destination. Raw identifiers
/// are hashed immediately and are never retained in a draft session.
pub struct DraftScope<'a> {
    pub account: &'a [u8],
    pub conversation: &'a [u8],
    pub recipient: &'a [u8],
}

impl DraftScope<'_> {
    pub fn binding_hash(&self) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(BINDING_DOMAIN);
        hash_field(&mut hash, self.account);
        hash_field(&mut hash, self.conversation);
        hash_field(&mut hash, self.recipient);
        hash.finalize().into()
    }
}

fn hash_field(hash: &mut Sha256, value: &[u8]) {
    hash.update((value.len() as u64).to_be_bytes());
    hash.update(value);
}

/// Cancellation is cooperative during synchronous inference and checked both
/// before and after the model call. Implementations must check it between
/// decode steps. A model that blocks or ignores this token cannot be forcibly
/// stopped by this crate; callers must put untrusted inference behind their own
/// process or thread watchdog and terminate that worker at the deadline.
#[derive(Default)]
pub struct CancellationToken(AtomicBool);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Read-only input exposed to a local model. It contains no scope identifiers,
/// keys, transport tokens, credentials, or send capability.
pub struct ModelInput<'a> {
    context: &'a [AuthorizedContextEntry],
    pub max_output_bytes: usize,
}

impl ModelInput<'_> {
    pub fn context(&self) -> impl ExactSizeIterator<Item = &str> {
        self.context.iter().map(AuthorizedContextEntry::as_str)
    }
}

pub struct GenerationControl<'a> {
    pub cancellation: &'a CancellationToken,
    pub deadline: Instant,
}

impl GenerationControl<'_> {
    pub fn should_stop(&self) -> bool {
        self.cancellation.is_cancelled() || Instant::now() >= self.deadline
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ModelError {
    #[error("local cover model is unavailable")]
    Unavailable,
    #[error("local cover generation was cancelled")]
    Cancelled,
    #[error("local cover generation exceeded its deadline")]
    DeadlineExceeded,
    #[error("local cover generation failed")]
    Failed,
}

/// Signed model-pack metadata supplied before artifact loading.
pub struct ModelPackMetadata {
    pub model_id: String,
    pub version_digest: [u8; 32],
    pub artifact_digest: [u8; 32],
    pub artifact_size: u64,
    pub max_working_set_bytes: u64,
    pub signature: Vec<u8>,
}

/// Digest and size independently observed by the caller while reading the
/// artifact. This crate performs no filesystem access itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObservedArtifact {
    pub digest: [u8; 32],
    pub size: u64,
}

/// Caller-owned signature implementation. It receives only a domain-separated
/// canonical metadata digest and the bounded signature bytes.
pub trait ModelPackSignatureVerifier {
    fn verify(&self, metadata_digest: &[u8; 32], signature: &[u8]) -> bool;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ModelPackError {
    #[error("model-pack metadata exceeds a hard bound")]
    InvalidMetadata,
    #[error("observed model artifact does not match signed metadata")]
    ArtifactMismatch,
    #[error("model-pack signature is invalid")]
    InvalidSignature,
}

/// Verified model identity. Its fields are private and it has no unchecked
/// constructor, so inference provenance cannot be self-reported by an adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustedModelPack {
    model_id: String,
    version_digest: [u8; 32],
    artifact_digest: [u8; 32],
    artifact_size: u64,
    max_working_set_bytes: u64,
    metadata_digest: [u8; 32],
}

impl TrustedModelPack {
    pub fn verify(
        metadata: ModelPackMetadata,
        observed: ObservedArtifact,
        verifier: &dyn ModelPackSignatureVerifier,
    ) -> Result<Self, ModelPackError> {
        validate_model_metadata(&metadata)?;
        if observed.digest != metadata.artifact_digest || observed.size != metadata.artifact_size {
            return Err(ModelPackError::ArtifactMismatch);
        }
        let metadata_digest = canonical_model_metadata_digest(&metadata);
        if !verifier.verify(&metadata_digest, &metadata.signature) {
            return Err(ModelPackError::InvalidSignature);
        }
        Ok(Self {
            model_id: metadata.model_id,
            version_digest: metadata.version_digest,
            artifact_digest: metadata.artifact_digest,
            artifact_size: metadata.artifact_size,
            max_working_set_bytes: metadata.max_working_set_bytes,
            metadata_digest,
        })
    }

    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    pub fn version_digest(&self) -> [u8; 32] {
        self.version_digest
    }

    pub fn artifact_digest(&self) -> [u8; 32] {
        self.artifact_digest
    }

    pub fn artifact_size(&self) -> u64 {
        self.artifact_size
    }

    pub fn max_working_set_bytes(&self) -> u64 {
        self.max_working_set_bytes
    }

    pub fn metadata_digest(&self) -> [u8; 32] {
        self.metadata_digest
    }
}

fn validate_model_metadata(metadata: &ModelPackMetadata) -> Result<(), ModelPackError> {
    let id = metadata.model_id.as_bytes();
    let id_valid = !id.is_empty()
        && id.len() <= HARD_MAX_MODEL_ID_BYTES
        && id[0].is_ascii_alphanumeric()
        && id
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-._/".contains(byte));
    let signature_valid = metadata.signature.len() >= HARD_MIN_MODEL_SIGNATURE_BYTES
        && metadata.signature.len() <= HARD_MAX_MODEL_SIGNATURE_BYTES;
    let sizes_valid = metadata.artifact_size > 0
        && metadata.artifact_size <= HARD_MAX_MODEL_ARTIFACT_BYTES
        && metadata.max_working_set_bytes > 0
        && metadata.max_working_set_bytes <= HARD_MAX_MODEL_WORKING_SET_BYTES;
    let digests_valid = metadata.version_digest != [0; 32] && metadata.artifact_digest != [0; 32];
    if id_valid && signature_valid && sizes_valid && digests_valid {
        Ok(())
    } else {
        Err(ModelPackError::InvalidMetadata)
    }
}

fn canonical_model_metadata_digest(metadata: &ModelPackMetadata) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(MODEL_PACK_DOMAIN);
    hash_field(&mut hash, metadata.model_id.as_bytes());
    hash_field(&mut hash, &metadata.version_digest);
    hash_field(&mut hash, &metadata.artifact_digest);
    hash.update(metadata.artifact_size.to_be_bytes());
    hash.update(metadata.max_working_set_bytes.to_be_bytes());
    hash.finalize().into()
}

/// A local inference adapter. The trait intentionally cannot send, persist, or
/// retrieve context. A usable adapter must expose previously verified pack
/// metadata rather than asserting its own identity.
pub trait LocalCoverModel {
    fn trusted_model_pack(&self) -> Option<&TrustedModelPack>;
    fn generate(
        &mut self,
        input: ModelInput<'_>,
        control: GenerationControl<'_>,
    ) -> Result<Zeroizing<String>, ModelError>;
}

/// Truthful fail-closed adapter used when no trusted local model is installed.
pub struct UnavailableModel;

impl LocalCoverModel for UnavailableModel {
    fn trusted_model_pack(&self) -> Option<&TrustedModelPack> {
        None
    }

    fn generate(
        &mut self,
        _input: ModelInput<'_>,
        _control: GenerationControl<'_>,
    ) -> Result<Zeroizing<String>, ModelError> {
        Err(ModelError::Unavailable)
    }
}

pub struct DraftRequest<'a> {
    pub scope: DraftScope<'a>,
    /// Hash of the exact canonical private message this cover draft represents.
    pub canonical_message_hash: [u8; 32],
    pub authorized_context: Vec<AuthorizedContextEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Provenance {
    pub model_id: String,
    pub model_version_digest: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DraftState {
    Prepared,
    Approved,
    Consumed,
    Cancelled,
    Expired,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Error {
    #[error("cover-draft limits are invalid")]
    InvalidLimits,
    #[error("authorized context is empty")]
    EmptyContext,
    #[error("draft scope or canonical message binding is invalid")]
    InvalidBinding,
    #[error("authorized context exceeds its configured bounds")]
    ContextTooLarge,
    #[error("cover model is unavailable")]
    ModelUnavailable,
    #[error("generation was cancelled")]
    Cancelled,
    #[error("generation deadline was exceeded")]
    DeadlineExceeded,
    #[error("cover model failed")]
    ModelFailed,
    #[error("model produced an empty draft")]
    EmptyOutput,
    #[error("model output exceeds its configured bound")]
    OutputTooLarge,
    #[error("model output contains a reserved OSL token marker")]
    ReservedMarker,
    #[error("model output contains unsafe invisible control characters")]
    UnsafeOutput,
    #[error("draft is not in a state that permits this operation")]
    InvalidState,
    #[error("draft scope or canonical message does not match")]
    BindingMismatch,
    #[error("draft has expired")]
    Expired,
}

impl From<ModelError> for Error {
    fn from(value: ModelError) -> Self {
        match value {
            ModelError::Unavailable => Self::ModelUnavailable,
            ModelError::Cancelled => Self::Cancelled,
            ModelError::DeadlineExceeded => Self::DeadlineExceeded,
            ModelError::Failed => Self::ModelFailed,
        }
    }
}

pub struct CoverDraftEngine {
    limits: Limits,
}

impl CoverDraftEngine {
    pub fn new(limits: Limits) -> Result<Self, Error> {
        Ok(Self {
            limits: limits.validate()?,
        })
    }

    pub fn prepare<M: LocalCoverModel>(
        &self,
        model: &mut M,
        request: DraftRequest<'_>,
        cancellation: &CancellationToken,
    ) -> Result<DraftSession, Error> {
        validate_binding(&request.scope, request.canonical_message_hash)?;
        self.validate_context(&request.authorized_context)?;
        if cancellation.is_cancelled() {
            return Err(Error::Cancelled);
        }

        let started = Instant::now();
        let deadline = started + self.limits.max_generation_time;
        let trusted_pack = model.trusted_model_pack().ok_or(Error::ModelUnavailable)?;
        let model_id = trusted_pack.model_id().to_owned();
        let version_digest = trusted_pack.version_digest();
        let mut output = model.generate(
            ModelInput {
                context: &request.authorized_context,
                max_output_bytes: self.limits.max_output_bytes,
            },
            GenerationControl {
                cancellation,
                deadline,
            },
        )?;

        if cancellation.is_cancelled() {
            output.zeroize();
            return Err(Error::Cancelled);
        }
        if Instant::now() >= deadline {
            output.zeroize();
            return Err(Error::DeadlineExceeded);
        }
        validate_output(&output, self.limits.max_output_bytes)?;
        let content_hash = draft_hash(&output);

        Ok(DraftSession {
            draft: output,
            scope_binding_hash: request.scope.binding_hash(),
            canonical_message_hash: request.canonical_message_hash,
            content_hash,
            approved_content_hash: None,
            provenance: Provenance {
                model_id,
                model_version_digest: version_digest,
            },
            state: DraftState::Prepared,
            expires_at: started + self.limits.draft_lifetime,
            max_output_bytes: self.limits.max_output_bytes,
        })
    }

    fn validate_context(&self, context: &[AuthorizedContextEntry]) -> Result<(), Error> {
        if context.is_empty() {
            return Err(Error::EmptyContext);
        }
        if context.len() > self.limits.max_context_entries {
            return Err(Error::ContextTooLarge);
        }
        let mut total = 0usize;
        for entry in context {
            let bytes = entry.text.len();
            if entry.text.trim().is_empty() || bytes > self.limits.max_entry_bytes {
                return Err(Error::ContextTooLarge);
            }
            total = total.checked_add(bytes).ok_or(Error::ContextTooLarge)?;
            if total > self.limits.max_context_bytes {
                return Err(Error::ContextTooLarge);
            }
        }
        Ok(())
    }
}

pub struct DraftSession {
    draft: Zeroizing<String>,
    scope_binding_hash: [u8; 32],
    canonical_message_hash: [u8; 32],
    content_hash: [u8; 32],
    approved_content_hash: Option<[u8; 32]>,
    provenance: Provenance,
    state: DraftState,
    expires_at: Instant,
    max_output_bytes: usize,
}

impl fmt::Debug for DraftSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DraftSession")
            .field("state", &self.state)
            .field("provenance", &self.provenance)
            .field("content", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

impl DraftSession {
    pub fn state(&mut self, now: Instant) -> DraftState {
        self.expire_if_needed(now);
        self.state
    }

    pub fn draft(&mut self, now: Instant) -> Result<&str, Error> {
        self.ensure_live(now)?;
        Ok(&self.draft)
    }

    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }

    pub fn binding_hash(&self) -> [u8; 32] {
        self.scope_binding_hash
    }

    /// Editing always returns the lifecycle to Prepared and invalidates any
    /// prior approval, even when the replacement happens to be identical.
    pub fn edit(&mut self, replacement: impl Into<String>, now: Instant) -> Result<(), Error> {
        self.ensure_live(now)?;
        if !matches!(self.state, DraftState::Prepared | DraftState::Approved) {
            return Err(Error::InvalidState);
        }
        let replacement = Zeroizing::new(replacement.into());
        validate_output(&replacement, self.max_output_bytes)?;
        self.draft.zeroize();
        self.draft = replacement;
        self.content_hash = draft_hash(&self.draft);
        self.approved_content_hash = None;
        self.state = DraftState::Prepared;
        Ok(())
    }

    pub fn approve(
        &mut self,
        scope: &DraftScope<'_>,
        canonical_message_hash: [u8; 32],
        now: Instant,
    ) -> Result<(), Error> {
        self.ensure_live(now)?;
        if self.state != DraftState::Prepared {
            return Err(Error::InvalidState);
        }
        self.ensure_binding(scope, canonical_message_hash)?;
        self.approved_content_hash = Some(self.content_hash);
        self.state = DraftState::Approved;
        Ok(())
    }

    /// Consumes the approved text exactly once. This returns content only; the
    /// type contains no transport handle, destination, or send authority.
    pub fn consume(
        &mut self,
        scope: &DraftScope<'_>,
        canonical_message_hash: [u8; 32],
        now: Instant,
    ) -> Result<ConsumedDraft, Error> {
        self.ensure_live(now)?;
        if self.state != DraftState::Approved
            || self.approved_content_hash != Some(self.content_hash)
        {
            return Err(Error::InvalidState);
        }
        self.ensure_binding(scope, canonical_message_hash)?;
        let text = Zeroizing::new(std::mem::take(&mut *self.draft));
        self.approved_content_hash = None;
        self.state = DraftState::Consumed;
        Ok(ConsumedDraft {
            text,
            provenance: self.provenance.clone(),
        })
    }

    pub fn cancel(&mut self, now: Instant) -> Result<(), Error> {
        self.ensure_live(now)?;
        if !matches!(self.state, DraftState::Prepared | DraftState::Approved) {
            return Err(Error::InvalidState);
        }
        self.draft.zeroize();
        self.approved_content_hash = None;
        self.state = DraftState::Cancelled;
        Ok(())
    }

    fn ensure_binding(
        &self,
        scope: &DraftScope<'_>,
        canonical_message_hash: [u8; 32],
    ) -> Result<(), Error> {
        if scope.binding_hash() != self.scope_binding_hash
            || canonical_message_hash != self.canonical_message_hash
        {
            return Err(Error::BindingMismatch);
        }
        Ok(())
    }

    fn ensure_live(&mut self, now: Instant) -> Result<(), Error> {
        self.expire_if_needed(now);
        if self.state == DraftState::Expired {
            Err(Error::Expired)
        } else {
            Ok(())
        }
    }

    fn expire_if_needed(&mut self, now: Instant) {
        if now >= self.expires_at
            && matches!(self.state, DraftState::Prepared | DraftState::Approved)
        {
            self.draft.zeroize();
            self.approved_content_hash = None;
            self.state = DraftState::Expired;
        }
    }
}

/// Approved cover content after one-time consumption. It is zeroized on drop
/// and deliberately has no destination or method capable of sending it.
pub struct ConsumedDraft {
    text: Zeroizing<String>,
    provenance: Provenance,
}

impl fmt::Debug for ConsumedDraft {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConsumedDraft")
            .field("provenance", &self.provenance)
            .field("content", &"[REDACTED]")
            .finish()
    }
}

impl ConsumedDraft {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }
}

fn draft_hash(text: &str) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(DRAFT_DOMAIN);
    hash_field(&mut hash, text.as_bytes());
    hash.finalize().into()
}

fn validate_binding(scope: &DraftScope<'_>, canonical_message_hash: [u8; 32]) -> Result<(), Error> {
    let valid_field = |value: &[u8]| {
        !value.is_empty()
            && value.len() <= HARD_MAX_SCOPE_FIELD_BYTES
            && !value.iter().all(|byte| *byte == 0)
    };
    if canonical_message_hash == [0; 32]
        || !valid_field(scope.account)
        || !valid_field(scope.conversation)
        || !valid_field(scope.recipient)
    {
        return Err(Error::InvalidBinding);
    }
    Ok(())
}

fn validate_output(output: &str, max_output_bytes: usize) -> Result<(), Error> {
    if output.trim().is_empty() {
        return Err(Error::EmptyOutput);
    }
    if output.len() > max_output_bytes {
        return Err(Error::OutputTooLarge);
    }
    if output.chars().any(|character| {
        (character.is_control() && character != '\n' && character != '\t')
            || matches!(
                character,
                '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}'
            )
    }) {
        return Err(Error::UnsafeOutput);
    }
    // These are the real, case-sensitive wire prefixes. Lowercase lookalikes
    // are ordinary prose; exact protocol markers must never be model output.
    if output.contains("DPC0::") || output.contains("DPC1::") {
        return Err(Error::ReservedMarker);
    }
    const RESERVED: [&[u8]; 6] = [
        b"osl1:",
        b"osl2:",
        b"osl3:",
        b"osl://",
        b"[[osl:",
        b"<osl_token",
    ];
    if RESERVED
        .iter()
        .any(|marker| contains_ascii_case_insensitive(output.as_bytes(), marker))
    {
        return Err(Error::ReservedMarker);
    }
    Ok(())
}

fn contains_ascii_case_insensitive(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestVerifier(bool);

    impl ModelPackSignatureVerifier for TestVerifier {
        fn verify(&self, _metadata_digest: &[u8; 32], _signature: &[u8]) -> bool {
            self.0
        }
    }

    fn metadata() -> ModelPackMetadata {
        ModelPackMetadata {
            model_id: "local-test-model".to_owned(),
            version_digest: [7; 32],
            artifact_digest: [8; 32],
            artifact_size: 1024,
            max_working_set_bytes: 4096,
            signature: vec![5; 64],
        }
    }

    fn trusted_pack() -> TrustedModelPack {
        TrustedModelPack::verify(
            metadata(),
            ObservedArtifact {
                digest: [8; 32],
                size: 1024,
            },
            &TestVerifier(true),
        )
        .unwrap()
    }

    struct FixedModel {
        output: &'static str,
        calls: usize,
        pack: TrustedModelPack,
    }

    impl LocalCoverModel for FixedModel {
        fn trusted_model_pack(&self) -> Option<&TrustedModelPack> {
            Some(&self.pack)
        }

        fn generate(
            &mut self,
            input: ModelInput<'_>,
            control: GenerationControl<'_>,
        ) -> Result<Zeroizing<String>, ModelError> {
            self.calls += 1;
            assert!(!control.should_stop());
            assert!(input.context().count() > 0);
            Ok(Zeroizing::new(self.output.to_owned()))
        }
    }

    fn scope() -> DraftScope<'static> {
        DraftScope {
            account: b"account-a",
            conversation: b"dm-17",
            recipient: b"device-b",
        }
    }

    fn request(text: &str) -> DraftRequest<'static> {
        DraftRequest {
            scope: scope(),
            canonical_message_hash: [9; 32],
            authorized_context: vec![AuthorizedContextEntry::authorize(text)],
        }
    }

    fn engine() -> CoverDraftEngine {
        CoverDraftEngine::new(Limits::default()).unwrap()
    }

    fn prepared(output: &'static str) -> DraftSession {
        let mut model = FixedModel {
            output,
            calls: 0,
            pack: trusted_pack(),
        };
        engine()
            .prepare(
                &mut model,
                request("authorized context"),
                &CancellationToken::default(),
            )
            .unwrap()
    }

    #[test]
    fn prepares_with_content_free_provenance() {
        let mut session = prepared("Want to grab coffee later?");
        assert_eq!(session.state(Instant::now()), DraftState::Prepared);
        assert_eq!(session.provenance().model_id, "local-test-model");
        assert_eq!(session.provenance().model_version_digest, [7; 32]);
        assert_eq!(
            session.draft(Instant::now()).unwrap(),
            "Want to grab coffee later?"
        );
    }

    #[test]
    fn invalid_model_pack_signature_is_rejected() {
        assert_eq!(
            TrustedModelPack::verify(
                metadata(),
                ObservedArtifact {
                    digest: [8; 32],
                    size: 1024,
                },
                &TestVerifier(false),
            ),
            Err(ModelPackError::InvalidSignature)
        );
    }

    #[test]
    fn observed_artifact_must_match_signed_metadata() {
        assert_eq!(
            TrustedModelPack::verify(
                metadata(),
                ObservedArtifact {
                    digest: [6; 32],
                    size: 1024,
                },
                &TestVerifier(true),
            ),
            Err(ModelPackError::ArtifactMismatch)
        );
        assert_eq!(
            TrustedModelPack::verify(
                metadata(),
                ObservedArtifact {
                    digest: [8; 32],
                    size: 1025,
                },
                &TestVerifier(true),
            ),
            Err(ModelPackError::ArtifactMismatch)
        );
    }

    #[test]
    fn model_pack_metadata_hard_bounds_fail_closed() {
        let mut invalid_id = metadata();
        invalid_id.model_id = "x".repeat(HARD_MAX_MODEL_ID_BYTES + 1);
        assert_eq!(
            TrustedModelPack::verify(
                invalid_id,
                ObservedArtifact {
                    digest: [8; 32],
                    size: 1024,
                },
                &TestVerifier(true),
            ),
            Err(ModelPackError::InvalidMetadata)
        );

        let mut invalid_signature = metadata();
        invalid_signature.signature = vec![5; HARD_MIN_MODEL_SIGNATURE_BYTES - 1];
        assert_eq!(
            TrustedModelPack::verify(
                invalid_signature,
                ObservedArtifact {
                    digest: [8; 32],
                    size: 1024,
                },
                &TestVerifier(true),
            ),
            Err(ModelPackError::InvalidMetadata)
        );

        let mut invalid_size = metadata();
        invalid_size.artifact_size = HARD_MAX_MODEL_ARTIFACT_BYTES + 1;
        assert_eq!(
            TrustedModelPack::verify(
                invalid_size,
                ObservedArtifact {
                    digest: [8; 32],
                    size: HARD_MAX_MODEL_ARTIFACT_BYTES + 1,
                },
                &TestVerifier(true),
            ),
            Err(ModelPackError::InvalidMetadata)
        );

        let mut invalid_working_set = metadata();
        invalid_working_set.max_working_set_bytes = HARD_MAX_MODEL_WORKING_SET_BYTES + 1;
        assert_eq!(
            TrustedModelPack::verify(
                invalid_working_set,
                ObservedArtifact {
                    digest: [8; 32],
                    size: 1024,
                },
                &TestVerifier(true),
            ),
            Err(ModelPackError::InvalidMetadata)
        );

        let mut invalid_digest = metadata();
        invalid_digest.version_digest = [0; 32];
        assert_eq!(
            TrustedModelPack::verify(
                invalid_digest,
                ObservedArtifact {
                    digest: [8; 32],
                    size: 1024,
                },
                &TestVerifier(true),
            ),
            Err(ModelPackError::InvalidMetadata)
        );
    }

    #[test]
    fn trusted_pack_drives_provenance() {
        let pack = trusted_pack();
        assert_eq!(pack.model_id(), "local-test-model");
        assert_eq!(pack.version_digest(), [7; 32]);
        assert_eq!(pack.artifact_digest(), [8; 32]);
        assert_eq!(pack.artifact_size(), 1024);
        assert_eq!(pack.max_working_set_bytes(), 4096);
        assert_ne!(pack.metadata_digest(), [0; 32]);

        let session = prepared("trusted draft");
        assert_eq!(session.provenance().model_id, pack.model_id());
        assert_eq!(
            session.provenance().model_version_digest,
            pack.version_digest()
        );
    }

    #[test]
    fn hard_and_configurable_limits_fail_closed() {
        let invalid = Limits {
            max_context_entries: HARD_MAX_CONTEXT_ENTRIES + 1,
            ..Limits::default()
        };
        assert!(matches!(
            CoverDraftEngine::new(invalid),
            Err(Error::InvalidLimits)
        ));

        let limits = Limits {
            max_context_entries: 1,
            max_context_bytes: 4,
            max_entry_bytes: 4,
            ..Limits::default()
        };
        let bounded_engine = CoverDraftEngine::new(limits).unwrap();
        let mut model = FixedModel {
            output: "draft",
            calls: 0,
            pack: trusted_pack(),
        };
        let err = bounded_engine
            .prepare(&mut model, request("12345"), &CancellationToken::default())
            .unwrap_err();
        assert_eq!(err, Error::ContextTooLarge);
        assert_eq!(model.calls, 0);
    }

    #[test]
    fn empty_context_never_reaches_model() {
        let mut model = FixedModel {
            output: "draft",
            calls: 0,
            pack: trusted_pack(),
        };
        let mut request = request("unused");
        request.authorized_context.clear();
        assert_eq!(
            engine()
                .prepare(&mut model, request, &CancellationToken::default())
                .unwrap_err(),
            Error::EmptyContext
        );
        assert_eq!(model.calls, 0);
    }

    #[test]
    fn unavailable_model_never_pretends_to_generate_ai() {
        let err = engine()
            .prepare(
                &mut UnavailableModel,
                request("authorized context"),
                &CancellationToken::default(),
            )
            .unwrap_err();
        assert_eq!(err, Error::ModelUnavailable);
    }

    #[test]
    fn pre_cancelled_generation_never_calls_model() {
        let token = CancellationToken::default();
        token.cancel();
        let mut model = FixedModel {
            output: "draft",
            calls: 0,
            pack: trusted_pack(),
        };
        assert_eq!(
            engine()
                .prepare(&mut model, request("context"), &token)
                .unwrap_err(),
            Error::Cancelled
        );
        assert_eq!(model.calls, 0);
    }

    struct CancellingModel {
        pack: TrustedModelPack,
    }

    impl LocalCoverModel for CancellingModel {
        fn trusted_model_pack(&self) -> Option<&TrustedModelPack> {
            Some(&self.pack)
        }
        fn generate(
            &mut self,
            _input: ModelInput<'_>,
            control: GenerationControl<'_>,
        ) -> Result<Zeroizing<String>, ModelError> {
            control.cancellation.cancel();
            Ok(Zeroizing::new("must be discarded".to_owned()))
        }
    }

    #[test]
    fn cancellation_after_model_return_discards_output() {
        assert_eq!(
            engine()
                .prepare(
                    &mut CancellingModel {
                        pack: trusted_pack(),
                    },
                    request("context"),
                    &CancellationToken::default()
                )
                .unwrap_err(),
            Error::Cancelled
        );
    }

    struct LateModel {
        pack: TrustedModelPack,
    }

    impl LocalCoverModel for LateModel {
        fn trusted_model_pack(&self) -> Option<&TrustedModelPack> {
            Some(&self.pack)
        }
        fn generate(
            &mut self,
            _input: ModelInput<'_>,
            _control: GenerationControl<'_>,
        ) -> Result<Zeroizing<String>, ModelError> {
            std::thread::sleep(Duration::from_millis(5));
            Ok(Zeroizing::new("late output must be discarded".to_owned()))
        }
    }

    #[test]
    fn model_cannot_return_output_after_deadline() {
        let limits = Limits {
            max_generation_time: Duration::from_millis(1),
            ..Limits::default()
        };
        let deadline_engine = CoverDraftEngine::new(limits).unwrap();
        assert_eq!(
            deadline_engine
                .prepare(
                    &mut LateModel {
                        pack: trusted_pack(),
                    },
                    request("context"),
                    &CancellationToken::default()
                )
                .unwrap_err(),
            Error::DeadlineExceeded
        );
    }

    #[test]
    fn output_bounds_and_reserved_markers_are_rejected() {
        let limits = Limits {
            max_output_bytes: 5,
            ..Limits::default()
        };
        let bounded_engine = CoverDraftEngine::new(limits).unwrap();
        let mut long = FixedModel {
            output: "123456",
            calls: 0,
            pack: trusted_pack(),
        };
        assert_eq!(
            bounded_engine
                .prepare(&mut long, request("context"), &CancellationToken::default())
                .unwrap_err(),
            Error::OutputTooLarge
        );

        let mut marker = FixedModel {
            output: "hello OSL2:payload",
            calls: 0,
            pack: trusted_pack(),
        };
        assert_eq!(
            engine()
                .prepare(
                    &mut marker,
                    request("context"),
                    &CancellationToken::default()
                )
                .unwrap_err(),
            Error::ReservedMarker
        );

        for output in ["DPC0::ciphertext", "prefix DPC1::ciphertext"] {
            let mut marker = FixedModel {
                output,
                calls: 0,
                pack: trusted_pack(),
            };
            assert_eq!(
                engine()
                    .prepare(
                        &mut marker,
                        request("context"),
                        &CancellationToken::default()
                    )
                    .unwrap_err(),
                Error::ReservedMarker
            );
        }

        // DPC wire markers are deliberately case-sensitive.
        let mut lowercase = FixedModel {
            output: "ordinary dpc0:: prose",
            calls: 0,
            pack: trusted_pack(),
        };
        assert!(engine()
            .prepare(
                &mut lowercase,
                request("context"),
                &CancellationToken::default()
            )
            .is_ok());
    }

    #[test]
    fn approval_requires_exact_scope_and_message() {
        let now = Instant::now();
        let mut session = prepared("draft");
        let wrong_scope = DraftScope {
            account: b"account-a",
            conversation: b"different-dm",
            recipient: b"device-b",
        };
        assert_eq!(
            session.approve(&wrong_scope, [9; 32], now),
            Err(Error::BindingMismatch)
        );
        assert_eq!(
            session.approve(&scope(), [8; 32], now),
            Err(Error::BindingMismatch)
        );
        session.approve(&scope(), [9; 32], now).unwrap();
        assert_eq!(session.state(now), DraftState::Approved);
    }

    #[test]
    fn edits_invalidate_approval() {
        let now = Instant::now();
        let mut session = prepared("first draft");
        session.approve(&scope(), [9; 32], now).unwrap();
        session.edit("edited draft", now).unwrap();
        assert_eq!(session.state(now), DraftState::Prepared);
        assert_eq!(
            session.consume(&scope(), [9; 32], now).unwrap_err(),
            Error::InvalidState
        );
        session.approve(&scope(), [9; 32], now).unwrap();
        assert_eq!(
            session.consume(&scope(), [9; 32], now).unwrap().text(),
            "edited draft"
        );
    }

    #[test]
    fn consumption_is_single_use_and_replay_safe() {
        let now = Instant::now();
        let mut session = prepared("one use");
        session.approve(&scope(), [9; 32], now).unwrap();
        let consumed = session.consume(&scope(), [9; 32], now).unwrap();
        assert_eq!(consumed.text(), "one use");
        assert_eq!(
            session.consume(&scope(), [9; 32], now).unwrap_err(),
            Error::InvalidState
        );
        assert_eq!(session.state(now), DraftState::Consumed);
    }

    #[test]
    fn expiry_zeroizes_and_blocks_approval() {
        let limits = Limits {
            draft_lifetime: Duration::from_millis(1),
            ..Limits::default()
        };
        let engine = CoverDraftEngine::new(limits).unwrap();
        let mut model = FixedModel {
            output: "short lived",
            calls: 0,
            pack: trusted_pack(),
        };
        let mut session = engine
            .prepare(
                &mut model,
                request("context"),
                &CancellationToken::default(),
            )
            .unwrap();
        let late = Instant::now() + Duration::from_secs(1);
        assert_eq!(
            session.approve(&scope(), [9; 32], late),
            Err(Error::Expired)
        );
        assert_eq!(session.state(late), DraftState::Expired);
    }

    #[test]
    fn explicit_cancel_blocks_later_use() {
        let now = Instant::now();
        let mut session = prepared("cancel me");
        session.cancel(now).unwrap();
        assert_eq!(session.state(now), DraftState::Cancelled);
        assert_eq!(
            session.approve(&scope(), [9; 32], now),
            Err(Error::InvalidState)
        );
    }

    #[test]
    fn scope_hash_is_length_delimited() {
        let one = DraftScope {
            account: b"ab",
            conversation: b"c",
            recipient: b"d",
        };
        let two = DraftScope {
            account: b"1",
            conversation: b"bc",
            recipient: b"d",
        };
        assert_ne!(one.binding_hash(), two.binding_hash());
    }

    #[test]
    fn invalid_bindings_and_invisible_output_fail_before_approval() {
        let mut model = FixedModel {
            output: "draft",
            calls: 0,
            pack: trusted_pack(),
        };
        let invalid = DraftRequest {
            scope: DraftScope {
                account: b"",
                conversation: b"dm",
                recipient: b"peer",
            },
            canonical_message_hash: [9; 32],
            authorized_context: vec![AuthorizedContextEntry::authorize("context")],
        };
        assert_eq!(
            engine()
                .prepare(&mut model, invalid, &CancellationToken::default())
                .unwrap_err(),
            Error::InvalidBinding
        );
        assert_eq!(model.calls, 0);

        let mut invisible = FixedModel {
            output: "looks\u{202E}safe",
            calls: 0,
            pack: trusted_pack(),
        };
        assert_eq!(
            engine()
                .prepare(
                    &mut invisible,
                    request("context"),
                    &CancellationToken::default()
                )
                .unwrap_err(),
            Error::UnsafeOutput
        );
    }
}
