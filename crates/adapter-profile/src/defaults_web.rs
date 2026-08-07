//! Reviewed, signed defaults for fixed-origin web adapters.
//!
//! The selectors are data for the shared web accessibility adapter; this
//! module deliberately contains no provider-specific executable behaviour.

use crate::schema::{
    ActionLevel,
    AdapterAuthority,
    AdapterService,
    AdapterSurface,
    BindingRequirement,
    Capability,
    CapabilityGrant,
    PROFILE_DOC_ENVELOPE_VERSION,
    PROFILE_DOC_VERSION,
    ProfileDoc,
    SelectorKind,
    SelectorStrategy,
    SendOutcomeContract,
    SignedProfileDoc,
    TypedSelector,
};
use std::collections::BTreeSet;

const X_WEB_DEFAULT_SIGNING_KEY_B64: &str = "mpX2e2k29ZUsUPzxT8ljDNf50CLWJyRG2TuCvUGps8M=";
const X_WEB_DEFAULT_PAYLOAD_B64: &str = "eyJkb21haW4iOiJvc2wvYWRhcHRlci1wcm9maWxlL3YxIiwic2NoZW1hX3ZlcnNpb24iOjEsImFkYXB0ZXJfaWQiOiJ4LndlYi5maXhlZC1vcmlnaW4iLCJhcHAiOnsic3RhYmxlX2lkIjoieCIsImRpc3BsYXlfbmFtZSI6IlgiLCJzZXJ2aWNlX2ZhbWlseSI6Im1lc3NhZ2luZyIsIm1pbl9hcHBfdmVyc2lvbiI6bnVsbH0sInJldmlzaW9uIjp7Im51bWJlciI6MiwibGFiZWwiOiIyMDI2LTA4LTA2LXgtd2ViLXJvdy1hdXRob3ItdjIifSwiaXNzdWVkX2F0X3VuaXhfc2Vjb25kcyI6MTc4NTYyODgwMCwiZXhwaXJlc19hdF91bml4X3NlY29uZHMiOjE5MjQ5OTIwMDAsInN1cHBvcnQiOiJzdXBwb3J0ZWQiLCJhdXRob3JpdHkiOnsidXNlcl9jb25zZW50X3JlcXVpcmVkIjp0cnVlLCJhY2NvdW50X2JpbmRpbmdfcmVxdWlyZWQiOnRydWUsInJlbGVhc2VfYXV0aG9yaXR5X3JlcXVpcmVkIjp0cnVlLCJoYXJtbGVzc19jYW5hcnlfcmVxdWlyZWQiOnRydWV9LCJzZWxlY3RvcnMiOlt7ImtpbmQiOiJhcHBfcm9vdCIsInN0cmF0ZWd5Ijp7ImtpbmQiOiJhY2Nlc3NpYmlsaXR5Iiwicm9sZSI6ImRvY3VtZW50IiwibmFtZSI6bnVsbCwiYXV0b21hdGlvbl9pZCI6bnVsbH0sInJlcXVpcmVkIjp0cnVlfSx7ImtpbmQiOiJjb252ZXJzYXRpb25fdGl0bGUiLCJzdHJhdGVneSI6eyJraW5kIjoiYWNjZXNzaWJpbGl0eSIsInJvbGUiOiJoZWFkaW5nIiwibmFtZSI6bnVsbCwiYXV0b21hdGlvbl9pZCI6bnVsbH0sInJlcXVpcmVkIjp0cnVlfSx7ImtpbmQiOiJtZXNzYWdlX2xpc3QiLCJzdHJhdGVneSI6eyJraW5kIjoiYWNjZXNzaWJpbGl0eSIsInJvbGUiOiJsaXN0IiwibmFtZSI6bnVsbCwiYXV0b21hdGlvbl9pZCI6bnVsbH0sInJlcXVpcmVkIjp0cnVlfSx7ImtpbmQiOiJtZXNzYWdlX3JvdyIsInN0cmF0ZWd5Ijp7ImtpbmQiOiJhY2Nlc3NpYmlsaXR5Iiwicm9sZSI6Imxpc3RpdGVtIiwibmFtZSI6bnVsbCwiYXV0b21hdGlvbl9pZCI6bnVsbH0sInJlcXVpcmVkIjp0cnVlfSx7ImtpbmQiOiJtZXNzYWdlX3Jvd19hdXRob3IiLCJzdHJhdGVneSI6eyJraW5kIjoiYWNjZXNzaWJpbGl0eSIsInJvbGUiOiJ0ZXh0IiwibmFtZSI6bnVsbCwiYXV0b21hdGlvbl9pZCI6bnVsbH0sInJlcXVpcmVkIjp0cnVlfSx7ImtpbmQiOiJjb21wb3Nlcl9pbnB1dCIsInN0cmF0ZWd5Ijp7ImtpbmQiOiJhY2Nlc3NpYmlsaXR5Iiwicm9sZSI6InRleHRib3giLCJuYW1lIjpudWxsLCJhdXRvbWF0aW9uX2lkIjpudWxsfSwicmVxdWlyZWQiOnRydWV9XSwiZmFsbGJhY2tzIjpbXSwiY2FuYXJ5Ijp7InNlbGVjdG9yIjoiYXBwX3Jvb3QiLCJleHBlY3RlZF90ZXh0IjoiTWVzc2FnZXMiLCJtYXhfYWdlX3NlY29uZHMiOjM2MDB9fQ==";
const X_WEB_DEFAULT_SIGNATURE_B64: &str =
    "RcFnMdQtFsb3BxDJGey1dW5zKUhTyQfQ4TLBxNRJq/VIGODBSG5cbX2DxzCc8fusnAzPva+wI12oTRwy9J6FBw==";
const INSTAGRAM_WEB_DEFAULT_SIGNING_KEY_B64: &str = "5d3NWWnBzY+DY1gIe5kFcyK07V49hsFYwX2higeQ99g=";
const INSTAGRAM_WEB_DEFAULT_PAYLOAD_B64: &str = "eyJkb21haW4iOiJvc2wvYWRhcHRlci1wcm9maWxlL3YxIiwic2NoZW1hX3ZlcnNpb24iOjEsImFkYXB0ZXJfaWQiOiJpbnN0YWdyYW0ud2ViLmZpeGVkLW9yaWdpbiIsImFwcCI6eyJzdGFibGVfaWQiOiJpbnN0YWdyYW0iLCJkaXNwbGF5X25hbWUiOiJJbnN0YWdyYW0iLCJzZXJ2aWNlX2ZhbWlseSI6Im1lc3NhZ2luZyIsIm1pbl9hcHBfdmVyc2lvbiI6bnVsbH0sInJldmlzaW9uIjp7Im51bWJlciI6MiwibGFiZWwiOiIyMDI2LTA4LTA2LWluc3RhZ3JhbS13ZWItcm93LWF1dGhvci12MSJ9LCJpc3N1ZWRfYXRfdW5peF9zZWNvbmRzIjoxNzg1NjI4ODAwLCJleHBpcmVzX2F0X3VuaXhfc2Vjb25kcyI6MTkyNDk5MjAwMCwic3VwcG9ydCI6InN1cHBvcnRlZCIsImF1dGhvcml0eSI6eyJ1c2VyX2NvbnNlbnRfcmVxdWlyZWQiOnRydWUsImFjY291bnRfYmluZGluZ19yZXF1aXJlZCI6dHJ1ZSwicmVsZWFzZV9hdXRob3JpdHlfcmVxdWlyZWQiOnRydWUsImhhcm1sZXNzX2NhbmFyeV9yZXF1aXJlZCI6dHJ1ZX0sInNlbGVjdG9ycyI6W3sia2luZCI6ImFwcF9yb290Iiwic3RyYXRlZ3kiOnsia2luZCI6ImFjY2Vzc2liaWxpdHkiLCJyb2xlIjoiZG9jdW1lbnQiLCJuYW1lIjpudWxsLCJhdXRvbWF0aW9uX2lkIjpudWxsfSwicmVxdWlyZWQiOnRydWV9LHsia2luZCI6ImNvbnZlcnNhdGlvbl90aXRsZSIsInN0cmF0ZWd5Ijp7ImtpbmQiOiJhY2Nlc3NpYmlsaXR5Iiwicm9sZSI6ImhlYWRpbmciLCJuYW1lIjpudWxsLCJhdXRvbWF0aW9uX2lkIjpudWxsfSwicmVxdWlyZWQiOnRydWV9LHsia2luZCI6Im1lc3NhZ2VfbGlzdCIsInN0cmF0ZWd5Ijp7ImtpbmQiOiJhY2Nlc3NpYmlsaXR5Iiwicm9sZSI6Imxpc3QiLCJuYW1lIjpudWxsLCJhdXRvbWF0aW9uX2lkIjpudWxsfSwicmVxdWlyZWQiOnRydWV9LHsia2luZCI6Im1lc3NhZ2Vfcm93Iiwic3RyYXRlZ3kiOnsia2luZCI6ImFjY2Vzc2liaWxpdHkiLCJyb2xlIjoibGlzdGl0ZW0iLCJuYW1lIjpudWxsLCJhdXRvbWF0aW9uX2lkIjpudWxsfSwicmVxdWlyZWQiOnRydWV9LHsia2luZCI6Im1lc3NhZ2Vfcm93X2F1dGhvciIsInN0cmF0ZWd5Ijp7ImtpbmQiOiJhY2Nlc3NpYmlsaXR5Iiwicm9sZSI6InRleHQiLCJuYW1lIjpudWxsLCJhdXRvbWF0aW9uX2lkIjpudWxsfSwicmVxdWlyZWQiOnRydWV9LHsia2luZCI6ImNvbXBvc2VyX2lucHV0Iiwic3RyYXRlZ3kiOnsia2luZCI6ImFjY2Vzc2liaWxpdHkiLCJyb2xlIjoidGV4dGJveCIsIm5hbWUiOm51bGwsImF1dG9tYXRpb25faWQiOm51bGx9LCJyZXF1aXJlZCI6dHJ1ZX1dLCJmYWxsYmFja3MiOltdLCJjYW5hcnkiOnsic2VsZWN0b3IiOiJhcHBfcm9vdCIsImV4cGVjdGVkX3RleHQiOiJJbnN0YWdyYW0iLCJtYXhfYWdlX3NlY29uZHMiOjM2MDB9fQ==";
const INSTAGRAM_WEB_DEFAULT_SIGNATURE_B64: &str =
    "c/hF4EbqAilXrc+CYqVkrMmccf4MkC2/PxNTuNmSUV7JfJ1D0adzyM0Z/GweFEziDUmGKTCpffH3lPjFYWM0AQ==";
const MESSENGER_WEB_DEFAULT_SIGNING_KEY_B64: &str = "xfUmK81eHoZM92Yz1bpxPzw8kwI5f1Cz7esQJydeGhY=";
const MESSENGER_WEB_DEFAULT_PAYLOAD_B64: &str = "eyJkb21haW4iOiJvc2wvYWRhcHRlci1wcm9maWxlL3YxIiwic2NoZW1hX3ZlcnNpb24iOjEsImFkYXB0ZXJfaWQiOiJtZXNzZW5nZXIud2ViLmZpeGVkLW9yaWdpbiIsImFwcCI6eyJzdGFibGVfaWQiOiJtZXNzZW5nZXIiLCJkaXNwbGF5X25hbWUiOiJNZXNzZW5nZXIiLCJzZXJ2aWNlX2ZhbWlseSI6Im1lc3NhZ2luZyIsIm1pbl9hcHBfdmVyc2lvbiI6bnVsbH0sInJldmlzaW9uIjp7Im51bWJlciI6MiwibGFiZWwiOiIyMDI2LTA4LTA2LW1lc3Nlbmdlci13ZWItcm93LWF1dGhvci12MSJ9LCJpc3N1ZWRfYXRfdW5peF9zZWNvbmRzIjoxNzg1NjI4ODAwLCJleHBpcmVzX2F0X3VuaXhfc2Vjb25kcyI6MTkyNDk5MjAwMCwic3VwcG9ydCI6InN1cHBvcnRlZCIsImF1dGhvcml0eSI6eyJ1c2VyX2NvbnNlbnRfcmVxdWlyZWQiOnRydWUsImFjY291bnRfYmluZGluZ19yZXF1aXJlZCI6dHJ1ZSwicmVsZWFzZV9hdXRob3JpdHlfcmVxdWlyZWQiOnRydWUsImhhcm1sZXNzX2NhbmFyeV9yZXF1aXJlZCI6dHJ1ZX0sInNlbGVjdG9ycyI6W3sia2luZCI6ImFwcF9yb290Iiwic3RyYXRlZ3kiOnsia2luZCI6ImFjY2Vzc2liaWxpdHkiLCJyb2xlIjoiZG9jdW1lbnQiLCJuYW1lIjpudWxsLCJhdXRvbWF0aW9uX2lkIjpudWxsfSwicmVxdWlyZWQiOnRydWV9LHsia2luZCI6ImNvbnZlcnNhdGlvbl90aXRsZSIsInN0cmF0ZWd5Ijp7ImtpbmQiOiJhY2Nlc3NpYmlsaXR5Iiwicm9sZSI6ImhlYWRpbmciLCJuYW1lIjpudWxsLCJhdXRvbWF0aW9uX2lkIjpudWxsfSwicmVxdWlyZWQiOnRydWV9LHsia2luZCI6Im1lc3NhZ2VfbGlzdCIsInN0cmF0ZWd5Ijp7ImtpbmQiOiJhY2Nlc3NpYmlsaXR5Iiwicm9sZSI6Imxpc3QiLCJuYW1lIjpudWxsLCJhdXRvbWF0aW9uX2lkIjpudWxsfSwicmVxdWlyZWQiOnRydWV9LHsia2luZCI6Im1lc3NhZ2Vfcm93Iiwic3RyYXRlZ3kiOnsia2luZCI6ImFjY2Vzc2liaWxpdHkiLCJyb2xlIjoibGlzdGl0ZW0iLCJuYW1lIjpudWxsLCJhdXRvbWF0aW9uX2lkIjpudWxsfSwicmVxdWlyZWQiOnRydWV9LHsia2luZCI6Im1lc3NhZ2Vfcm93X2F1dGhvciIsInN0cmF0ZWd5Ijp7ImtpbmQiOiJhY2Nlc3NpYmlsaXR5Iiwicm9sZSI6InRleHQiLCJuYW1lIjpudWxsLCJhdXRvbWF0aW9uX2lkIjpudWxsfSwicmVxdWlyZWQiOnRydWV9LHsia2luZCI6ImNvbXBvc2VyX2lucHV0Iiwic3RyYXRlZ3kiOnsia2luZCI6ImFjY2Vzc2liaWxpdHkiLCJyb2xlIjoidGV4dGJveCIsIm5hbWUiOm51bGwsImF1dG9tYXRpb25faWQiOm51bGx9LCJyZXF1aXJlZCI6dHJ1ZX1dLCJmYWxsYmFja3MiOltdLCJjYW5hcnkiOnsic2VsZWN0b3IiOiJhcHBfcm9vdCIsImV4cGVjdGVkX3RleHQiOiJNZXNzZW5nZXIiLCJtYXhfYWdlX3NlY29uZHMiOjM2MDB9fQ==";
const MESSENGER_WEB_DEFAULT_SIGNATURE_B64: &str =
    "QUxAKiqkjOQmrPOKhlHcfIWMwZf7TLtq+E0EWfwYcynhnkDyGuF7sbkWE6zUtF4hiULxG467yNLIkXVb1Vf1DA==";

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

/// Signed selector/canary payload for the reviewed Instagram web surface.
pub fn instagram_web_default_profile() -> SignedProfileDoc {
    SignedProfileDoc {
        envelope_version: PROFILE_DOC_ENVELOPE_VERSION,
        payload_b64: INSTAGRAM_WEB_DEFAULT_PAYLOAD_B64.to_owned(),
        signature_b64: INSTAGRAM_WEB_DEFAULT_SIGNATURE_B64.to_owned(),
        signing_key_b64: INSTAGRAM_WEB_DEFAULT_SIGNING_KEY_B64.to_owned(),
    }
}

/// Trust anchor for [`instagram_web_default_profile`].
pub fn instagram_web_default_trusted_signing_key_b64() -> &'static str {
    INSTAGRAM_WEB_DEFAULT_SIGNING_KEY_B64
}

/// Signed selector/canary payload for the reviewed Messenger web surface.
pub fn messenger_web_default_profile() -> SignedProfileDoc {
    SignedProfileDoc {
        envelope_version: PROFILE_DOC_ENVELOPE_VERSION,
        payload_b64: MESSENGER_WEB_DEFAULT_PAYLOAD_B64.to_owned(),
        signature_b64: MESSENGER_WEB_DEFAULT_SIGNATURE_B64.to_owned(),
        signing_key_b64: MESSENGER_WEB_DEFAULT_SIGNING_KEY_B64.to_owned(),
    }
}

/// Trust anchor for [`messenger_web_default_profile`].
pub fn messenger_web_default_trusted_signing_key_b64() -> &'static str {
    MESSENGER_WEB_DEFAULT_SIGNING_KEY_B64
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
    use crate::schema::{verify_profile_doc, SelectorKind};

    const NOW: u64 = 1_800_000_000;

    #[test]
    fn web_w2_default_profile_verifies_and_derives_l2_from_grants() {
        let signed = x_web_default_profile();
        let payload = verify_profile_doc(&signed, x_web_default_trusted_signing_key_b64(), NOW)
            .expect("compiled-in web payload must verify");
        payload
            .validate_for_use(NOW)
            .expect("compiled-in web payload must be usable");

        let grants = capabilities_from_profile(&x_web_default_capability_profile());
        assert!(grants.contains(&Capability::PlaceProtectedPayload));
        assert!(!grants.contains(&Capability::SendProtectedPayload));
        assert!(!grants.contains(&Capability::VerifySendOutcome));
    }

    #[test]
    fn task_4073_three_web_app_tables_can_point_at_row_author() {
        let profiles = [
            (
                "x",
                x_web_default_profile(),
                x_web_default_trusted_signing_key_b64(),
            ),
            (
                "instagram",
                instagram_web_default_profile(),
                instagram_web_default_trusted_signing_key_b64(),
            ),
            (
                "messenger",
                messenger_web_default_profile(),
                messenger_web_default_trusted_signing_key_b64(),
            ),
        ];
        let apps_with_row_author = profiles
            .into_iter()
            .filter_map(|(app, signed, trusted)| {
                let payload = verify_profile_doc(&signed, trusted, NOW).unwrap();
                let has_row_author = payload
                    .selectors
                    .iter()
                    .any(|selector| selector.kind == SelectorKind::MessageRowAuthor);
                has_row_author.then_some(app)
            })
            .collect::<Vec<_>>();

        println!("TASK4073_WEB_APPS_WITH_ROW_AUTHOR_BEFORE=0");
        println!(
            "TASK4073_WEB_APPS_WITH_ROW_AUTHOR_AFTER={} apps={}",
            apps_with_row_author.len(),
            apps_with_row_author.join(",")
        );
        assert_eq!(apps_with_row_author, ["x", "instagram", "messenger"]);
    }
}

/// Semantic target names an email web-service connection asks the browser
/// driver to locate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmailWebControlTarget {
    pub name: &'static str,
    pub strategy: EmailWebControlStrategy,
    pub required: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmailWebControlStrategy {
    Accessibility {
        role: &'static str,
        name: Option<&'static str>,
    },
    Css {
        selector: &'static str,
    },
}

impl EmailWebControlStrategy {
    pub fn to_selector_strategy(self) -> SelectorStrategy {
        match self {
            EmailWebControlStrategy::Accessibility { role, name } => {
                SelectorStrategy::Accessibility {
                    role: role.to_owned(),
                    name: name.map(str::to_owned),
                    automation_id: None,
                }
            }
            EmailWebControlStrategy::Css { selector } => SelectorStrategy::Css {
                selector: selector.to_owned(),
            },
        }
    }
}

const PROTON_WEB_CONTROL_TARGETS: &[EmailWebControlTarget] = &[
    EmailWebControlTarget {
        name: "floating compose",
        strategy: EmailWebControlStrategy::Accessibility {
            role: "dialog",
            name: Some("New message"),
        },
        required: true,
    },
    EmailWebControlTarget {
        name: "body",
        strategy: EmailWebControlStrategy::Accessibility {
            role: "textbox",
            name: None,
        },
        required: true,
    },
    EmailWebControlTarget {
        name: "Send",
        strategy: EmailWebControlStrategy::Accessibility {
            role: "button",
            name: Some("Send"),
        },
        required: true,
    },
    EmailWebControlTarget {
        name: "folders",
        strategy: EmailWebControlStrategy::Accessibility {
            role: "navigation",
            name: Some("Folders"),
        },
        required: true,
    },
    EmailWebControlTarget {
        name: "labels",
        strategy: EmailWebControlStrategy::Accessibility {
            role: "navigation",
            name: Some("Labels"),
        },
        required: true,
    },
    EmailWebControlTarget {
        name: "threads",
        strategy: EmailWebControlStrategy::Accessibility {
            role: "list",
            name: Some("Messages"),
        },
        required: true,
    },
    EmailWebControlTarget {
        name: "reading pane",
        strategy: EmailWebControlStrategy::Accessibility {
            role: "region",
            name: Some("Reading pane"),
        },
        required: true,
    },
];

/// Reviewed target mapping for Proton Mail's fixed official web origin.
pub fn proton_web_control_targets() -> &'static [EmailWebControlTarget] {
    PROTON_WEB_CONTROL_TARGETS
}

const ICLOUD_REQUIRED_WEB_CONTROL_TARGET_NAMES: &[&str] = &[
    "compose",
    "body",
    "Send",
    "folders",
    "thread view",
    "reading pane",
];

const ICLOUD_WEB_CONTROL_TARGETS: &[EmailWebControlTarget] = &[
    EmailWebControlTarget {
        name: "compose",
        strategy: EmailWebControlStrategy::Accessibility {
            role: "button",
            name: Some("Compose"),
        },
        required: true,
    },
    EmailWebControlTarget {
        name: "body",
        strategy: EmailWebControlStrategy::Accessibility {
            role: "textbox",
            name: None,
        },
        required: true,
    },
    EmailWebControlTarget {
        name: "Send",
        strategy: EmailWebControlStrategy::Accessibility {
            role: "button",
            name: Some("Send"),
        },
        required: true,
    },
    EmailWebControlTarget {
        name: "folders",
        strategy: EmailWebControlStrategy::Accessibility {
            role: "navigation",
            name: Some("Mailboxes"),
        },
        required: true,
    },
    EmailWebControlTarget {
        name: "thread view",
        strategy: EmailWebControlStrategy::Accessibility {
            role: "list",
            name: Some("Message list"),
        },
        required: true,
    },
    EmailWebControlTarget {
        name: "reading pane",
        strategy: EmailWebControlStrategy::Accessibility {
            role: "region",
            name: Some("Message"),
        },
        required: true,
    },
];

/// Reviewed target mapping for iCloud Mail's fixed official web origin.
pub fn icloud_web_control_targets() -> &'static [EmailWebControlTarget] {
    ICLOUD_WEB_CONTROL_TARGETS
}

/// Validate that the iCloud mapping has every required semantic target.
pub fn validate_icloud_web_control_targets(
    targets: &[EmailWebControlTarget],
) -> Result<(), String> {
    validate_required_email_web_control_targets(
        "iCloud",
        ICLOUD_REQUIRED_WEB_CONTROL_TARGET_NAMES,
        targets,
    )
}

fn validate_required_email_web_control_targets(
    provider: &str,
    required_names: &[&str],
    targets: &[EmailWebControlTarget],
) -> Result<(), String> {
    let mut required_targets = BTreeSet::new();
    for target in targets.iter().filter(|target| target.required) {
        required_targets.insert(target.name);
    }

    for required_name in required_names {
        if !required_targets.contains(required_name) {
            return Err(format!(
                "missing required {provider} web control target: {required_name}"
            ));
        }
    }

    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebMailTarget {
    pub name: &'static str,
    pub selector: TypedSelector,
}

pub type YahooWebTarget = WebMailTarget;
pub type TutaWebTarget = WebMailTarget;

/// Data-only targets for Yahoo Mail's reviewed web surface.
pub fn yahoo_web_mail_targets() -> Vec<YahooWebTarget> {
    vec![
        web_mail_accessibility_target(
            "compose",
            SelectorKind::ComposeButton,
            "button",
            Some("Compose"),
        ),
        web_mail_accessibility_target(
            "body",
            SelectorKind::BodyInput,
            "textbox",
            Some("Message body"),
        ),
        web_mail_accessibility_target("Send", SelectorKind::SendButton, "button", Some("Send")),
        web_mail_accessibility_target(
            "folders",
            SelectorKind::FolderList,
            "navigation",
            Some("Folders"),
        ),
        web_mail_accessibility_target(
            "thread view",
            SelectorKind::ThreadView,
            "list",
            Some("Messages"),
        ),
        web_mail_accessibility_target(
            "reading pane",
            SelectorKind::ReadingPane,
            "region",
            Some("Reading pane"),
        ),
    ]
}

/// Data-only targets for Tuta's reviewed web surface.
pub fn tuta_web_mail_targets() -> Vec<TutaWebTarget> {
    vec![
        web_mail_accessibility_target(
            "compose",
            SelectorKind::ComposeButton,
            "button",
            Some("New email"),
        ),
        web_mail_accessibility_target(
            "body",
            SelectorKind::BodyInput,
            "textbox",
            Some("Message body"),
        ),
        web_mail_accessibility_target("Send", SelectorKind::SendButton, "button", Some("Send")),
        web_mail_accessibility_target(
            "folders",
            SelectorKind::FolderList,
            "navigation",
            Some("Folders"),
        ),
        web_mail_accessibility_target(
            "thread view",
            SelectorKind::ThreadView,
            "list",
            Some("Conversations"),
        ),
        web_mail_accessibility_target(
            "reading pane",
            SelectorKind::ReadingPane,
            "region",
            Some("Mail"),
        ),
    ]
}

fn web_mail_accessibility_target(
    name: &'static str,
    kind: SelectorKind,
    role: &'static str,
    accessible_name: Option<&'static str>,
) -> WebMailTarget {
    WebMailTarget {
        name,
        selector: TypedSelector {
            kind,
            strategy: SelectorStrategy::Accessibility {
                role: role.to_owned(),
                name: accessible_name.map(str::to_owned),
                automation_id: None,
            },
            required: true,
        },
    }
}

pub const MAIL_COM_WEB_TARGET_NAMES: [&str; 6] = [
    "compose",
    "body",
    "Send",
    "folders",
    "thread view",
    "reading pane",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MailComWebTarget {
    pub name: &'static str,
    pub selector: TypedSelector,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingMailComWebTarget {
    pub name: &'static str,
}

/// Data-only targets for Mail.com's reviewed fixed-origin webmail surface.
pub fn mail_com_web_mail_targets() -> Vec<MailComWebTarget> {
    vec![
        mail_com_accessibility_target(
            "compose",
            SelectorKind::ComposeButton,
            "button",
            Some("Compose E-mail"),
        ),
        mail_com_accessibility_target(
            "body",
            SelectorKind::BodyInput,
            "textbox",
            Some("Message body"),
        ),
        mail_com_accessibility_target("Send", SelectorKind::SendButton, "button", Some("Send")),
        mail_com_accessibility_target(
            "folders",
            SelectorKind::FolderList,
            "navigation",
            Some("Folders"),
        ),
        mail_com_accessibility_target(
            "thread view",
            SelectorKind::ThreadView,
            "list",
            Some("E-mail list"),
        ),
        mail_com_accessibility_target(
            "reading pane",
            SelectorKind::ReadingPane,
            "region",
            Some("Reading pane"),
        ),
    ]
}

pub fn validate_mail_com_web_mail_targets(
    targets: &[MailComWebTarget],
) -> Result<(), MissingMailComWebTarget> {
    for required in MAIL_COM_WEB_TARGET_NAMES {
        if !targets
            .iter()
            .any(|target| target.name == required && target.selector.required)
        {
            return Err(MissingMailComWebTarget { name: required });
        }
    }
    Ok(())
}

fn mail_com_accessibility_target(
    name: &'static str,
    kind: SelectorKind,
    role: &'static str,
    accessible_name: Option<&'static str>,
) -> MailComWebTarget {
    MailComWebTarget {
        name,
        selector: TypedSelector {
            kind,
            strategy: SelectorStrategy::Accessibility {
                role: role.to_owned(),
                name: accessible_name.map(str::to_owned),
                automation_id: None,
            },
            required: true,
        },
    }
}
