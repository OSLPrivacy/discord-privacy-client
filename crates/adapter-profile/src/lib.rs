//! Signed adapter profile schema and fail-closed validation primitives.
//!
//! This crate is deliberately data-only at this stage. It defines the
//! reviewed profile document that later loader/envelope/contract units can
//! verify and select, but it does not wire any adapter into the runtime.

pub mod schema;

pub use schema::{
    canonical_profile_bytes, parse_profile_doc, ActionLevel, AdapterAuthority, AdapterSurface,
    BindingRequirement, Capability, CapabilityGrant, ProfileDoc, ProfileValidationError,
    SendOutcomeContract, ValidatedProfile, ValidationEvidence, PROFILE_DOC_VERSION,
};
