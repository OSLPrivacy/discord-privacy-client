//! Provider-neutral native-surface adapter ABI.
//!
//! The host owns window claiming and profile verification.  Adapters receive a
//! claimed generation and may only operate on that generation; this keeps a
//! provider-specific accessibility implementation from becoming a second
//! window-discovery authority.

use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

#[cfg(not(feature = "task-1263-only"))]
pub mod discord;
#[cfg(not(feature = "task-1263-only"))]
pub mod signal;
#[cfg(not(feature = "task-1263-only"))]
pub mod telegram;
#[cfg(not(feature = "task-1263-only"))]
pub mod whatsapp;

pub const ADAPTER_ABI_VERSION: u32 = 1;
pub const SUPPORTED_MESSAGE_BOX_PROVIDER_COUNT: usize = 14;
pub const SUPPORTED_MESSAGE_BOX_PROVIDERS: [&str; SUPPORTED_MESSAGE_BOX_PROVIDER_COUNT] = [
    "discord",
    "signal",
    "telegram",
    "whatsapp",
    "instagram",
    "snapchat",
    "x",
    "messenger",
    "gmail",
    "outlook",
    "proton",
    "yahoo",
    "aol",
    "icloud",
];

pub type AdapterAppId = adapter_profile::AdapterService;
pub type SurfaceKind = adapter_profile::AdapterSurface;
pub type CapabilitySet = BTreeSet<adapter_profile::Capability>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum A11yTree {
    Msaa,
    Uia,
    Both,
    WebAx,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BindingEvidence {
    Accessibility {
        tree: A11yTree,
    },
    Win32Structural,
    UserConfirmedVisualBinding {
        ceremony_id: String,
        confirmed_at_ms: u64,
    },
    Pixel,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SurfaceTarget {
    pub app: AdapterAppId,
    pub surface: SurfaceKind,
    pub generation: u64,
}

/// An adapter-private node handle.  It is deliberately numeric so provider
/// labels and values cannot escape across the ABI by accident.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NodeRef(pub(crate) u64);

impl NodeRef {
    /// Creates an opaque node reference from a host-owned accessibility handle.
    pub fn for_claimed_node(handle: u64) -> Self {
        Self(handle)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SurfaceBinding {
    pub app: AdapterAppId,
    pub generation: u64,
    pub evidence: BindingEvidence,
    pub composer: NodeRef,
    pub transcript: Option<NodeRef>,
    pub bounds: Bounds,
    pub bound_at_ms: u64,
    /// Opaque, host-derived scope-binding hash; never provider identity text.
    pub(crate) scope_binding_hash: String,
}

impl SurfaceBinding {
    /// Constructs a binding from the host's already-claimed native surface.
    ///
    /// The scope value is deliberately opaque: adapters may compare it with an
    /// authorization but must never derive it from provider text.
    pub fn for_claimed_surface(
        app: AdapterAppId,
        generation: u64,
        evidence: BindingEvidence,
        composer: NodeRef,
        transcript: Option<NodeRef>,
        bounds: Bounds,
        bound_at_ms: u64,
        scope_binding_hash: impl Into<String>,
    ) -> Self {
        Self {
            app,
            generation,
            evidence,
            composer,
            transcript,
            bounds,
            bound_at_ms,
            scope_binding_hash: scope_binding_hash.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SurfaceState {
    pub composer_text_sha256: String,
    pub composer_is_empty: bool,
    pub composer_is_password_field: bool,
    pub focused: bool,
    pub occluded: bool,
    pub read_was_complete: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DestinationStatus {
    Attested,
    Unattested,
    Changed,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DestinationIdentity {
    pub status: DestinationStatus,
    pub account_digest: String,
    pub conversation_digest: String,
    pub recipients_digest: String,
    pub scope_binding_hash: String,
    pub evidence: BindingEvidence,
    pub attested_at_ms: u64,
    pub ttl_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Carrier(pub String);

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct MessageBoxFingerprint(String);

impl MessageBoxFingerprint {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderMessageBoxFingerprint {
    pub provider_id: &'static str,
    pub fingerprint: MessageBoxFingerprint,
}

pub fn record_provider_message_box_fingerprints() -> Vec<ProviderMessageBoxFingerprint> {
    SUPPORTED_MESSAGE_BOX_PROVIDERS
        .iter()
        .copied()
        .map(|provider_id| ProviderMessageBoxFingerprint {
            provider_id,
            fingerprint: message_box_fingerprint_for_provider(provider_id)
                .expect("supported provider has a message-box fingerprint"),
        })
        .collect()
}

pub fn message_box_fingerprint_for_provider(provider_id: &str) -> Option<MessageBoxFingerprint> {
    let canonical = canonical_message_box_provider_id(provider_id)?;
    Some(MessageBoxFingerprint(stable_message_box_fingerprint(
        canonical,
    )))
}

fn message_box_fingerprint_for_app(app: AdapterAppId) -> Option<MessageBoxFingerprint> {
    message_box_fingerprint_for_provider(match app {
        AdapterAppId::Discord => "discord",
        AdapterAppId::Signal => "signal",
        AdapterAppId::Telegram => "telegram",
        AdapterAppId::Whatsapp => "whatsapp",
        AdapterAppId::Instagram => "instagram",
        AdapterAppId::Snapchat => "snapchat",
        AdapterAppId::X => "x",
        AdapterAppId::Messenger => "messenger",
        AdapterAppId::Gmail => "gmail",
        AdapterAppId::Outlook => "outlook",
        AdapterAppId::Proton => "proton",
        AdapterAppId::Yahoo => "yahoo",
        AdapterAppId::Aol => "aol",
    })
}

fn canonical_message_box_provider_id(provider_id: &str) -> Option<&'static str> {
    let normalized = provider_id.trim().to_ascii_lowercase();
    let canonical = match normalized.as_str() {
        value => value,
    };
    SUPPORTED_MESSAGE_BOX_PROVIDERS
        .iter()
        .copied()
        .find(|candidate| *candidate == canonical)
}

fn stable_message_box_fingerprint(provider_id: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"OSL/provider-message-box-fingerprint/v1");
    digest.update([0]);
    digest.update(provider_id.as_bytes());
    hex_digest_bytes(&digest.finalize())
}

fn hex_digest_bytes(digest: impl AsRef<[u8]>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes = digest.as_ref();
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[usize::from(byte >> 4)] as char);
        encoded.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    encoded
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlacementAuthorization {
    scope_binding_hash: String,
    message_box_fingerprint: MessageBoxFingerprint,
}

impl PlacementAuthorization {
    /// The host passes its already-derived scope-binding hash.
    pub fn for_scope(scope_binding_hash: impl Into<String>) -> Self {
        Self {
            scope_binding_hash: scope_binding_hash.into(),
            message_box_fingerprint: MessageBoxFingerprint(String::new()),
        }
    }

    pub fn for_scope_and_provider(
        scope_binding_hash: impl Into<String>,
        provider_id: &str,
    ) -> Result<Self, String> {
        let fingerprint = message_box_fingerprint_for_provider(provider_id)
            .ok_or_else(|| "message-box provider is unsupported".to_owned())?;
        Ok(Self {
            scope_binding_hash: scope_binding_hash.into(),
            message_box_fingerprint: fingerprint,
        })
    }

    /// Returns the opaque host-derived binding for same-scope verification.
    pub(crate) fn scope_binding_hash(&self) -> &str {
        &self.scope_binding_hash
    }

    pub(crate) fn message_box_matches(&self, app: AdapterAppId) -> bool {
        message_box_fingerprint_for_app(app)
            .map(|fingerprint| fingerprint == self.message_box_fingerprint)
            .unwrap_or(false)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SendAuthorization {
    scope_binding_hash: String,
}

impl SendAuthorization {
    /// The host passes its already-derived scope-binding hash.
    pub fn for_scope(scope_binding_hash: impl Into<String>) -> Self {
        Self {
            scope_binding_hash: scope_binding_hash.into(),
        }
    }

    /// Returns the opaque host-derived binding for same-scope verification.
    pub(crate) fn scope_binding_hash(&self) -> &str {
        &self.scope_binding_hash
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlacementStatus {
    Placed,
    NotPlaced,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlacementReceipt {
    pub status: PlacementStatus,
    pub placed_sha256: Option<String>,
    pub elapsed_ms: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SendOutcome {
    Sent,
    NotSent,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SendReceipt {
    pub outcome: SendOutcome,
    pub elapsed_ms: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaintConfidence {
    Exact,
    Approximate,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaintTarget {
    pub carrier_sha256: String,
    pub rect: Bounds,
    pub clipped_by: Option<Bounds>,
    pub confidence: PaintConfidence,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdapterRefusal {
    ProfileNotUsable,
    ProfileExpired,
    CanaryMismatch,
    WindowGone,
    GenerationStale,
    NotFocused,
    Occluded,
    ComposerNotFound,
    ComposerAmbiguous,
    PasswordField,
    TranscriptNotFound,
    ReadIncomplete,
    DestinationUnattested,
    DestinationChanged,
    AuthorizationRejected,
    CapabilityNotGranted,
    AccessibilityUnavailable,
    PlatformUnsupported,
    Timeout,
}

pub trait SurfaceAdapter: Send + Sync {
    fn abi_version(&self) -> u32;
    fn app(&self) -> AdapterAppId;
    fn surface(&self) -> SurfaceKind;
    fn capabilities(&self, now_unix_seconds: u64) -> CapabilitySet;
    fn locate(&self, target: &SurfaceTarget) -> Result<SurfaceBinding, AdapterRefusal>;
    fn read_state(&self, binding: &SurfaceBinding) -> Result<SurfaceState, AdapterRefusal>;
    fn destination(&self, binding: &SurfaceBinding) -> Result<DestinationIdentity, AdapterRefusal>;
    fn place(
        &self,
        binding: &SurfaceBinding,
        authorization: &PlacementAuthorization,
        carrier: &Carrier,
    ) -> PlacementReceipt;
    fn commit(
        &self,
        binding: &SurfaceBinding,
        authorization: &SendAuthorization,
        placed: &PlacementReceipt,
    ) -> SendReceipt;
    fn paint_targets(&self, binding: &SurfaceBinding) -> Result<Vec<PaintTarget>, AdapterRefusal>;
}

pub(crate) fn same_scope(actual: &str, expected: &str) -> bool {
    !actual.is_empty() && actual == expected
}

pub(crate) fn same_scope_and_message_box(
    binding: &SurfaceBinding,
    authorization: &PlacementAuthorization,
) -> bool {
    same_scope(
        &binding.scope_binding_hash,
        authorization.scope_binding_hash(),
    ) && authorization.message_box_matches(binding.app.clone())
}

/// Whether binding evidence is strong enough to authorize an L3 send.
/// Pixel observations may guide an overlay but never authorize plaintext
/// delivery; every native adapter applies this predicate before committing.
pub fn is_send_evidence_admissible(evidence: &BindingEvidence) -> bool {
    !matches!(evidence, BindingEvidence::Pixel)
}
