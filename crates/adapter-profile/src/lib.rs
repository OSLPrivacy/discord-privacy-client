//! Signed adapter profile schema and fail-closed validation primitives.
//!
//! This crate is deliberately data-only at this stage. It defines the
//! reviewed profile document that later loader/envelope/contract units can
//! verify and select, but it does not wire any adapter into the runtime.

pub mod contract;
pub mod defaults;
pub mod defaults_web;
pub mod envelope;
pub mod loader;
pub mod schema;
pub mod score;
pub mod trust;

pub use contract::{
    run_contract_self_test, CheckOutcome, ContractVerdict, Predicate, ReportError, SelfTestProbe,
    SelfTestReport, Subsystem, UnverifiedCause, CONTRACT_VERSION, MAX_SELF_TEST_CHECKS,
    REQUIRED_SELF_TEST_PROBES,
};
pub use defaults::{
    signal_default_profile, signal_default_trusted_signing_key_b64, whatsapp_default_profile,
    whatsapp_default_trusted_signing_key_b64, SIGNAL_DESKTOP_NATIVE_APP_ROOT_ROLE,
    SIGNAL_DESKTOP_NATIVE_PRIMARY_WINDOW_CLASS, SIGNAL_DESKTOP_NATIVE_WINDOW_TITLE,
};
pub use defaults_web::{
    capabilities_from_profile, x_web_default_capability_profile, x_web_default_profile,
    x_web_default_trusted_signing_key_b64,
};
pub use envelope::{
    canonical_profile_digest, EnvelopeError, SignedProfile, ADAPTER_PROFILE_ENVELOPE_DOMAIN,
    ADAPTER_PROFILE_ENVELOPE_VERSION, ED25519_PUBLIC_KEY_LEN, ED25519_SIGNATURE_LEN,
    MAX_PROFILE_BYTES, MAX_SIGNER_KEY_ID_BYTES, SHA256_DIGEST_LEN,
};
pub use loader::{
    load_trusted_profile, load_trusted_profile_from_wire_json, LoadedProfile, LoaderError,
};

pub use schema::{
    canonical_profile_bytes, parse_profile_doc, ActionLevel, AdapterAuthority, AdapterService,
    AdapterSurface, BindingRequirement, Capability, CapabilityGrant, ProfileDoc,
    ProfileValidationError, SendOutcomeContract, ValidatedProfile, ValidationEvidence,
    PROFILE_DOC_VERSION,
};
pub use schema::{
    canonical_profile_payload_bytes, sign_profile_doc, verify_profile_doc, AppDescriptor,
    AuthorityRequirements, FallbackCondition, FallbackStrategy, HarmlessCanary, ProfileError,
    ProfilePayload, ProfileRevision, SelectorKind, SelectorStrategy, SignedProfileDoc,
    SupportLevel, TypedSelector, PROFILE_DOC_DOMAIN, PROFILE_DOC_ENVELOPE_VERSION,
    PROFILE_DOC_SCHEMA_VERSION,
};
pub use score::{
    score, CandidateEvidence, CandidateSelection, CandidateSurface, EvidenceSignal, ScoredCandidate,
};
pub use trust::{
    verify_signed_profile, ShippedAnchorKey, TrustError, DISCORD_PROFILE_ROLLBACK_FLOOR,
    SHIPPED_ANCHOR_KEYS,
};
