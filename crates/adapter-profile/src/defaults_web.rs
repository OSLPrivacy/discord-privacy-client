//! Reviewed, signed defaults for fixed-origin web adapters.
//!
//! The selectors are data for the shared web accessibility adapter; this
//! module deliberately contains no provider-specific executable behaviour.

use crate::schema::{
    ActionLevel, AdapterAuthority, AdapterService, AdapterSurface, BindingRequirement, Capability,
    CapabilityGrant, ProfileDoc, SendOutcomeContract, SignedProfileDoc,
    PROFILE_DOC_ENVELOPE_VERSION, PROFILE_DOC_VERSION,
};
use std::collections::BTreeSet;

const X_WEB_DEFAULT_SIGNING_KEY_B64: &str = "my2s2WkkDdRIj4x0FR0PWxPfhotOc27nI5rmxlsMr40=";
const X_WEB_DEFAULT_PAYLOAD_B64: &str = "eyJkb21haW4iOiJvc2wvYWRhcHRlci1wcm9maWxlL3YxIiwic2NoZW1hX3ZlcnNpb24iOjEsImFkYXB0ZXJfaWQiOiJ4LndlYi5maXhlZC1vcmlnaW4iLCJhcHAiOnsic3RhYmxlX2lkIjoieCIsImRpc3BsYXlfbmFtZSI6IlgiLCJzZXJ2aWNlX2ZhbWlseSI6Im1lc3NhZ2luZyIsIm1pbl9hcHBfdmVyc2lvbiI6bnVsbH0sInJldmlzaW9uIjp7Im51bWJlciI6MSwibGFiZWwiOiIyMDI2LTA4LTAyLXgtd2ViLXJldmlld2VkLXYxIn0sImlzc3VlZF9hdF91bml4X3NlY29uZHMiOjE3ODU2Mjg4MDAsImV4cGlyZXNfYXRfdW5peF9zZWNvbmRzIjoxOTI0OTkyMDAwLCJzdXBwb3J0Ijoic3VwcG9ydGVkIiwiYXV0aG9yaXR5Ijp7InVzZXJfY29uc2VudF9yZXF1aXJlZCI6dHJ1ZSwiYWNjb3VudF9iaW5kaW5nX3JlcXVpcmVkIjp0cnVlLCJyZWxlYXNlX2F1dGhvcml0eV9yZXF1aXJlZCI6dHJ1ZSwiaGFybWxlc3NfY2FuYXJ5X3JlcXVpcmVkIjp0cnVlfSwic2VsZWN0b3JzIjpbeyJraW5kIjoiYXBwX3Jvb3QiLCJzdHJhdGVneSI6eyJraW5kIjoiYWNjZXNzaWJpbGl0eSIsInJvbGUiOiJkb2N1bWVudCIsIm5hbWUiOm51bGwsImF1dG9tYXRpb25faWQiOm51bGx9LCJyZXF1aXJlZCI6dHJ1ZX0seyJraW5kIjoiY29udmVyc2F0aW9uX3RpdGxlIiwic3RyYXRlZ3kiOnsia2luZCI6ImFjY2Vzc2liaWxpdHkiLCJyb2xlIjoiaGVhZGluZyIsIm5hbWUiOm51bGwsImF1dG9tYXRpb25faWQiOm51bGx9LCJyZXF1aXJlZCI6dHJ1ZX0seyJraW5kIjoibWVzc2FnZV9saXN0Iiwic3RyYXRlZ3kiOnsia2luZCI6ImFjY2Vzc2liaWxpdHkiLCJyb2xlIjoibGlzdCIsIm5hbWUiOm51bGwsImF1dG9tYXRpb25faWQiOm51bGx9LCJyZXF1aXJlZCI6dHJ1ZX0seyJraW5kIjoibWVzc2FnZV9yb3ciLCJzdHJhdGVneSI6eyJraW5kIjoiYWNjZXNzaWJpbGl0eSIsInJvbGUiOiJsaXN0aXRlbSIsIm5hbWUiOm51bGwsImF1dG9tYXRpb25faWQiOm51bGx9LCJyZXF1aXJlZCI6dHJ1ZX0seyJraW5kIjoiY29tcG9zZXJfaW5wdXQiLCJzdHJhdGVneSI6eyJraW5kIjoiYWNjZXNzaWJpbGl0eSIsInJvbGUiOiJ0ZXh0Ym94IiwibmFtZSI6bnVsbCwiYXV0b21hdGlvbl9pZCI6bnVsbH0sInJlcXVpcmVkIjp0cnVlfV0sImZhbGxiYWNrcyI6W10sImNhbmFyeSI6eyJzZWxlY3RvciI6ImFwcF9yb290IiwiZXhwZWN0ZWRfdGV4dCI6Ik1lc3NhZ2VzIiwibWF4X2FnZV9zZWNvbmRzIjozNjAwfX0=";
const X_WEB_DEFAULT_SIGNATURE_B64: &str = "LSsGOcJFXw+e0BhiTwcUW2+xqhyx+7XXsI9VkcTkaX+7378yjhPMc4X5hE9/zXk1looBKQ8T/SS7XSiQmzroDA==";

/// Signed selector/canary payload for the first reviewed web surface.
pub fn x_web_default_profile() -> SignedProfileDoc {
    SignedProfileDoc {
        envelope_version: PROFILE_DOC_ENVELOPE_VERSION,
        payload_b64: X_WEB_DEFAULT_PAYLOAD_B64.to_owned(),
        signature_b64: X_WEB_DEFAULT_SIGNATURE_B64.to_owned(),
        signing_key_b64: X_WEB_DEFAULT_SIGNING_KEY_B64.to_owned(),
    }
}

/// Trust anchor for [`x_web_default_profile`].
pub fn x_web_default_trusted_signing_key_b64() -> &'static str {
    X_WEB_DEFAULT_SIGNING_KEY_B64
}

/// Capability grants paired with the signed web payload.
///
/// This default is intentionally L2-only until the live X proof has earned
/// the L3 grants. Consumers derive the layer from these grants; no service-id
/// match is allowed to widen it.
pub fn x_web_default_capability_profile() -> ProfileDoc {
    ProfileDoc {
        version: PROFILE_DOC_VERSION,
        profile_id: "x-web-fixed-origin-reviewed-v1".into(),
        service: AdapterService::X,
        profile_sequence: 1,
        rollback_floor: 1,
        issued_at_unix_seconds: 1_785_628_800,
        expires_at_unix_seconds: 1_924_992_000,
        min_client_version: "0.1.0".into(),
        surfaces: vec![AdapterSurface::FixedOfficialWebOrigin],
        capabilities: vec![CapabilityGrant {
            capability: Capability::PlaceProtectedPayload,
            action_level: ActionLevel::UserAssistedAction,
            consent_required: true,
            binding_required: BindingRequirement::all(),
            authority: AdapterAuthority::ReviewedLocalAdapter,
        }],
        send_outcome: SendOutcomeContract {
            reports_sent: true,
            reports_not_sent: true,
            reports_unknown: true,
            auto_retries_unknown: false,
        },
    }
}

/// Returns exactly the reviewed grants in a structurally valid profile.
///
/// This is intentionally profile-driven: callers can use the returned set to
/// derive L2/L3 without a provider-specific branch.
pub fn capabilities_from_profile(profile: &ProfileDoc) -> BTreeSet<Capability> {
    profile
        .validate_structure()
        .map(|validated| {
            validated
                .doc()
                .capabilities
                .iter()
                .map(|grant| grant.capability)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::verify_profile_doc;

    const NOW: u64 = 1_800_000_000;

    #[test]
    fn web_w2_default_profile_verifies_and_derives_l2_from_grants() {
        let signed = x_web_default_profile();
        let payload = verify_profile_doc(
            &signed,
            x_web_default_trusted_signing_key_b64(),
            NOW,
        )
        .expect("compiled-in web payload must verify");
        payload
            .validate_for_use(NOW)
            .expect("compiled-in web payload must be usable");

        let grants = capabilities_from_profile(&x_web_default_capability_profile());
        assert!(grants.contains(&Capability::PlaceProtectedPayload));
        assert!(!grants.contains(&Capability::SendProtectedPayload));
        assert!(!grants.contains(&Capability::VerifySendOutcome));
    }
}
