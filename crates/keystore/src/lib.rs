//! Scaffolding placeholder. Implementation pending design-doc review.
//!
//! v1: TPM 2.0 seal/unseal of identity-key blobs via Windows TBS API
//! (windows crate). Fallback to Windows Credential Manager via the
//! keyring crate when TPM unavailable. The TPM is used to wrap opaque
//! blobs; X25519 and ML-KEM-768 are not native TPM operations.
