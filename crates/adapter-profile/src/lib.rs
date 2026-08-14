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
pub mod visible_subject;

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
    capabilities_from_profile, icloud_fake_page_fixture, icloud_web_control_targets,
    instagram_web_default_profile,
    instagram_web_default_trusted_signing_key_b64, mail_com_web_mail_targets,
    proton_web_control_targets, tuta_web_mail_targets, validate_icloud_web_control_targets,
    validate_mail_com_web_mail_targets, validate_yahoo_web_mail_targets,
    x_web_default_capability_profile, x_web_default_profile, x_web_default_trusted_signing_key_b64,
    yahoo_web_mail_targets, EmailWebControlStrategy, EmailWebControlTarget, MailComWebTarget,
    IcloudFakePageControl, IcloudFakePageFixture, IcloudFakePageSnapshot, MissingMailComWebTarget,
    MissingYahooWebTarget, TutaWebTarget, YahooFakePageCounts,
    YahooFakePageFixture, YahooWebTarget, MAIL_COM_WEB_TARGET_NAMES, OSL_YAHOO_1249_COVER_MESSAGE,
    ICLOUD_1274_MARKED_WORDS, YAHOO_FAKE_PAGE_CONTROL_NAMES, YAHOO_WEB_TARGET_NAMES,
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
    canonical_profile_payload_bytes, selector_kind_count, sign_profile_doc, verify_profile_doc,
    AppDescriptor, AuthorityRequirements, FallbackCondition, FallbackStrategy, HarmlessCanary,
    ProfileError, ProfilePayload, ProfileRevision, SelectorKind, SelectorStrategy,
    SignedProfileDoc, SupportLevel, TypedSelector, ALL_SELECTOR_KINDS, PROFILE_DOC_DOMAIN,
    PROFILE_DOC_ENVELOPE_VERSION, PROFILE_DOC_SCHEMA_VERSION, TASK_4073_SELECTOR_KIND_COUNT_BEFORE,
};
pub use score::{
    score, CandidateEvidence, CandidateSelection, CandidateSurface, EvidenceSignal, ScoredCandidate,
};
pub use trust::{
    verify_signed_profile, ShippedAnchorKey, TrustError, DISCORD_PROFILE_ROLLBACK_FLOOR,
    SHIPPED_ANCHOR_KEYS,
};
pub use visible_subject::{
    visible_subject_is_private_fact, SubjectProvider, VisibleSubjectError,
    VisibleSubjectSendLedger, EMAIL_SUBJECT_PROVIDERS, PRIVATE_SUBJECT_PROVIDERS,
};
