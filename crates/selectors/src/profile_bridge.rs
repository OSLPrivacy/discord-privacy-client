//! Bridge signed selector hot-updates into the frozen adapter-profile payload.
//!
//! A selector update is a signed `SignedProfileDoc`, not a parallel selector
//! schema. Both compiled-in and fetched documents are verified by
//! `adapter-profile` and return the same `ProfilePayload` value.

use adapter_profile::{verify_profile_doc, ProfileError, ProfilePayload, SignedProfileDoc};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProfileBridgeError {
    #[error("hot-update profile document is not valid JSON")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Profile(#[from] ProfileError),
}

/// Verify a compiled-in signed profile and return the payload consumed by an
/// adapter. This is intentionally the same verification path as updates.
pub fn compiled_in_profile_payload(
    document: &SignedProfileDoc,
    trusted_signing_key_b64: &str,
    now_unix_seconds: u64,
) -> Result<ProfilePayload, ProfileBridgeError> {
    Ok(verify_profile_doc(
        document,
        trusted_signing_key_b64,
        now_unix_seconds,
    )?)
}

/// Decode and verify a fetched signed profile document. The returned payload
/// has no hot-update-only fields, so adapters receive the exact same shape as
/// they do for a compiled-in profile.
pub fn fetched_profile_payload(
    manifest_json: &[u8],
    trusted_signing_key_b64: &str,
    now_unix_seconds: u64,
) -> Result<ProfilePayload, ProfileBridgeError> {
    let document = serde_json::from_slice(manifest_json)?;
    compiled_in_profile_payload(&document, trusted_signing_key_b64, now_unix_seconds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapter_profile::{
        canonical_profile_payload_bytes, signal_default_profile,
        signal_default_trusted_signing_key_b64,
    };

    const NOW: u64 = 1_800_000_000;

    #[test]
    fn fetched_and_compiled_profiles_produce_identical_payload_bytes() {
        let compiled_document = signal_default_profile();
        let compiled = compiled_in_profile_payload(
            &compiled_document,
            signal_default_trusted_signing_key_b64(),
            NOW,
        )
        .expect("compiled profile must verify");
        let fetched_manifest =
            serde_json::to_vec(&compiled_document).expect("profile document must serialize");
        let fetched = fetched_profile_payload(
            &fetched_manifest,
            signal_default_trusted_signing_key_b64(),
            NOW,
        )
        .expect("fetched profile must verify");

        assert_eq!(
            canonical_profile_payload_bytes(&compiled).expect("compiled payload serializes"),
            canonical_profile_payload_bytes(&fetched).expect("fetched payload serializes"),
        );
    }
}
