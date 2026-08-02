//! Provider-neutral native-surface adapter ABI.
//!
//! The host owns window claiming and profile verification.  Adapters receive a
//! claimed generation and may only operate on that generation; this keeps a
//! provider-specific accessibility implementation from becoming a second
//! window-discovery authority.

use std::collections::BTreeSet;

pub mod discord;
pub mod signal;
pub mod telegram;

pub const ADAPTER_ABI_VERSION: u32 = 1;

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlacementAuthorization {
    scope_binding_hash: String,
}

impl PlacementAuthorization {
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

pub(crate) fn is_send_evidence_admissible(evidence: &BindingEvidence) -> bool {
    !matches!(evidence, BindingEvidence::Pixel)
}
