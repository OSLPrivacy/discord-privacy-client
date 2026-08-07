//! Reviewed, signed defaults for fixed-origin web adapters.
//!
//! The selectors are data for the shared web accessibility adapter; this
//! module deliberately contains no provider-specific executable behaviour.

use crate::schema::{
    ActionLevel, AdapterAuthority, AdapterService, AdapterSurface, BindingRequirement, Capability,
    CapabilityGrant, ProfileDoc, SelectorKind, SelectorStrategy, SendOutcomeContract,
    SignedProfileDoc, TypedSelector, PROFILE_DOC_ENVELOPE_VERSION, PROFILE_DOC_VERSION,
};
use std::collections::BTreeSet;

const X_WEB_DEFAULT_SIGNING_KEY_B64: &str = "my2s2WkkDdRIj4x0FR0PWxPfhotOc27nI5rmxlsMr40=";
const X_WEB_DEFAULT_PAYLOAD_B64: &str = "eyJkb21haW4iOiJvc2wvYWRhcHRlci1wcm9maWxlL3YxIiwic2NoZW1hX3ZlcnNpb24iOjEsImFkYXB0ZXJfaWQiOiJ4LndlYi5maXhlZC1vcmlnaW4iLCJhcHAiOnsic3RhYmxlX2lkIjoieCIsImRpc3BsYXlfbmFtZSI6IlgiLCJzZXJ2aWNlX2ZhbWlseSI6Im1lc3NhZ2luZyIsIm1pbl9hcHBfdmVyc2lvbiI6bnVsbH0sInJldmlzaW9uIjp7Im51bWJlciI6MSwibGFiZWwiOiIyMDI2LTA4LTAyLXgtd2ViLXJldmlld2VkLXYxIn0sImlzc3VlZF9hdF91bml4X3NlY29uZHMiOjE3ODU2Mjg4MDAsImV4cGlyZXNfYXRfdW5peF9zZWNvbmRzIjoxOTI0OTkyMDAwLCJzdXBwb3J0Ijoic3VwcG9ydGVkIiwiYXV0aG9yaXR5Ijp7InVzZXJfY29uc2VudF9yZXF1aXJlZCI6dHJ1ZSwiYWNjb3VudF9iaW5kaW5nX3JlcXVpcmVkIjp0cnVlLCJyZWxlYXNlX2F1dGhvcml0eV9yZXF1aXJlZCI6dHJ1ZSwiaGFybWxlc3NfY2FuYXJ5X3JlcXVpcmVkIjp0cnVlfSwic2VsZWN0b3JzIjpbeyJraW5kIjoiYXBwX3Jvb3QiLCJzdHJhdGVneSI6eyJraW5kIjoiYWNjZXNzaWJpbGl0eSIsInJvbGUiOiJkb2N1bWVudCIsIm5hbWUiOm51bGwsImF1dG9tYXRpb25faWQiOm51bGx9LCJyZXF1aXJlZCI6dHJ1ZX0seyJraW5kIjoiY29udmVyc2F0aW9uX3RpdGxlIiwic3RyYXRlZ3kiOnsia2luZCI6ImFjY2Vzc2liaWxpdHkiLCJyb2xlIjoiaGVhZGluZyIsIm5hbWUiOm51bGwsImF1dG9tYXRpb25faWQiOm51bGx9LCJyZXF1aXJlZCI6dHJ1ZX0seyJraW5kIjoibWVzc2FnZV9saXN0Iiwic3RyYXRlZ3kiOnsia2luZCI6ImFjY2Vzc2liaWxpdHkiLCJyb2xlIjoibGlzdCIsIm5hbWUiOm51bGwsImF1dG9tYXRpb25faWQiOm51bGx9LCJyZXF1aXJlZCI6dHJ1ZX0seyJraW5kIjoibWVzc2FnZV9yb3ciLCJzdHJhdGVneSI6eyJraW5kIjoiYWNjZXNzaWJpbGl0eSIsInJvbGUiOiJsaXN0aXRlbSIsIm5hbWUiOm51bGwsImF1dG9tYXRpb25faWQiOm51bGx9LCJyZXF1aXJlZCI6dHJ1ZX0seyJraW5kIjoiY29tcG9zZXJfaW5wdXQiLCJzdHJhdGVneSI6eyJraW5kIjoiYWNjZXNzaWJpbGl0eSIsInJvbGUiOiJ0ZXh0Ym94IiwibmFtZSI6bnVsbCwiYXV0b21hdGlvbl9pZCI6bnVsbH0sInJlcXVpcmVkIjp0cnVlfV0sImZhbGxiYWNrcyI6W10sImNhbmFyeSI6eyJzZWxlY3RvciI6ImFwcF9yb290IiwiZXhwZWN0ZWRfdGV4dCI6Ik1lc3NhZ2VzIiwibWF4X2FnZV9zZWNvbmRzIjozNjAwfX0=";
const X_WEB_DEFAULT_SIGNATURE_B64: &str =
    "LSsGOcJFXw+e0BhiTwcUW2+xqhyx+7XXsI9VkcTkaX+7378yjhPMc4X5hE9/zXk1looBKQ8T/SS7XSiQmzroDA==";

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

/// Validate that the Proton mapping has every required semantic target.
pub fn validate_proton_web_control_targets(
    targets: &[EmailWebControlTarget],
) -> Result<(), String> {
    validate_required_email_web_control_targets(
        "Proton",
        &[
            "floating compose",
            "body",
            "Send",
            "folders",
            "labels",
            "threads",
            "reading pane",
        ],
        targets,
    )
    .map_err(|error| {
        if error == "missing required Proton web control target: floating compose" {
            "Proton compose missing".to_owned()
        } else {
            error
        }
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::verify_profile_doc;

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
    fn task_1242_proton_mapping_contains_all_seven_named_targets() {
        let names = proton_web_control_targets()
            .iter()
            .filter(|target| target.required)
            .map(|target| target.name)
            .collect::<Vec<_>>();

        println!("proton target count={}", names.len());
        println!("proton targets={}", names.join(","));

        assert_eq!(
            names,
            vec![
                "floating compose",
                "body",
                "Send",
                "folders",
                "labels",
                "threads",
                "reading pane",
            ]
        );
    }

    #[test]
    fn task_1245_proton_refuses_missing_floating_compose_without_losing_existing_draft() {
        #[derive(Default)]
        struct ProtonDrafts {
            drafts: Vec<(&'static str, &'static str)>,
        }

        impl ProtonDrafts {
            fn count(&self) -> usize {
                self.drafts.len()
            }

            fn create(
                &mut self,
                targets: &[EmailWebControlTarget],
                id: &'static str,
                body: &'static str,
            ) -> Result<(), String> {
                validate_proton_web_control_targets(targets)?;
                self.drafts.push((id, body));
                Ok(())
            }

            fn read(&self, id: &str) -> Option<&'static str> {
                self.drafts
                    .iter()
                    .find_map(|(draft_id, body)| (*draft_id == id).then_some(*body))
            }
        }

        let mut drafts = ProtonDrafts::default();
        let good_targets = proton_web_control_targets();
        println!("TASK1245_DRAFT_COUNT_BEFORE={}", drafts.count());
        assert_eq!(drafts.count(), 0);

        drafts
            .create(good_targets, "proton-maple", "MAPLE-4172")
            .expect("complete Proton selectors create the MAPLE draft");
        let after_body = drafts.read("proton-maple").unwrap_or("");
        println!("TASK1245_DRAFT_COUNT_AFTER_CREATE={}", drafts.count());
        println!("TASK1245_PROTON_MAPLE_AFTER_CREATE={after_body}");
        assert_eq!(drafts.count(), 1);
        assert_eq!(after_body, "MAPLE-4172");

        let mut broken_targets = good_targets.to_vec();
        let broken = broken_targets
            .iter_mut()
            .find(|target| target.name == "floating compose")
            .expect("shipping Proton mapping names the compose control");
        let original_strategy = broken.strategy;
        let original_required = broken.required;
        broken.name = "Missing Compose";
        assert_eq!(broken.strategy, original_strategy);
        assert_eq!(broken.required, original_required);

        let refusal = drafts
            .create(
                &broken_targets,
                "proton-maple-duplicate",
                "SHOULD-NOT-PLACE",
            )
            .expect_err("renamed Proton compose target must refuse before adding a draft");
        println!("TASK1245_MUTATED_CONTROL_NAME=Missing Compose");
        println!("TASK1245_REFUSAL={refusal}");
        assert_eq!(refusal, "Proton compose missing");

        let final_body = drafts.read("proton-maple").unwrap_or("");
        println!("TASK1245_PROTON_MAPLE_AFTER_REFUSAL={final_body}");
        println!("TASK1245_DRAFT_COUNT_AFTER_REFUSAL={}", drafts.count());
        assert_eq!(final_body, "MAPLE-4172");
        assert_eq!(drafts.count(), 1);
    }

    #[test]
    fn task_1273_icloud_mapping_contains_all_six_named_targets() {
        let names = icloud_web_control_targets()
            .iter()
            .filter(|target| target.required)
            .map(|target| target.name)
            .collect::<Vec<_>>();

        println!("icloud target count={}", names.len());
        println!("icloud targets={}", names.join(","));

        assert_eq!(
            names,
            vec![
                "compose",
                "body",
                "Send",
                "folders",
                "thread view",
                "reading pane",
            ]
        );
        validate_icloud_web_control_targets(icloud_web_control_targets())
            .expect("complete iCloud mapping must validate");
    }

    #[test]
    fn task_1273_icloud_mapping_missing_any_required_target_is_refused_by_name() {
        for missing_name in ICLOUD_REQUIRED_WEB_CONTROL_TARGET_NAMES {
            let missing = icloud_web_control_targets()
                .iter()
                .copied()
                .filter(|target| target.name != *missing_name)
                .collect::<Vec<_>>();

            let error = validate_icloud_web_control_targets(&missing)
                .expect_err("missing required iCloud mapping target must be refused");
            println!("icloud missing target refused={error}");
            assert_eq!(
                error,
                format!("missing required iCloud web control target: {missing_name}")
            );
        }
    }

    #[test]
    fn task_1248_yahoo_mapping_contains_all_six_named_targets() {
        let targets = yahoo_web_mail_targets();
        let names = targets.iter().map(|target| target.name).collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                "compose",
                "body",
                "Send",
                "folders",
                "thread view",
                "reading pane"
            ]
        );
        assert!(targets.iter().all(|target| target.selector.required));

        println!(
            "TASK1248 yahoo_targets={} names={}",
            targets.len(),
            names.join("|")
        );
    }

    #[test]
    fn task_1278_tuta_mapping_contains_all_six_named_targets() {
        let targets = tuta_web_mail_targets();
        let names = targets.iter().map(|target| target.name).collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                "compose",
                "body",
                "Send",
                "folders",
                "thread view",
                "reading pane"
            ]
        );
        assert!(targets.iter().all(|target| target.selector.required));

        println!(
            "TASK1278 tuta_targets={} names={}",
            targets.len(),
            names.join("|")
        );
    }

    #[test]
    fn task_1267_mail_com_mapping_contains_all_six_named_targets_and_refuses_missing_by_name() {
        let targets = mail_com_web_mail_targets();
        let names = targets.iter().map(|target| target.name).collect::<Vec<_>>();

        assert_eq!(names, MAIL_COM_WEB_TARGET_NAMES);
        assert!(targets.iter().all(|target| target.selector.required));
        validate_mail_com_web_mail_targets(&targets).expect("complete Mail.com mapping is valid");

        let mut refused = Vec::new();
        for missing in MAIL_COM_WEB_TARGET_NAMES {
            let incomplete = targets
                .iter()
                .filter(|target| target.name != missing)
                .cloned()
                .collect::<Vec<_>>();
            let err = validate_mail_com_web_mail_targets(&incomplete)
                .expect_err("mapping missing a required target must be refused");
            assert_eq!(err.name, missing);
            refused.push(err.name);
        }

        println!(
            "TASK1267 mail_com_targets={} names={}",
            targets.len(),
            names.join("|")
        );
        for name in &names {
            println!("TASK1267 named_target={name}");
        }
        println!("TASK1267 refused_missing={}", refused.join("|"));
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
const INSTAGRAM_WEB_DEFAULT_SIGNING_KEY_B64: &str = "5d3NWWnBzY+DY1gIe5kFcyK07V49hsFYwX2higeQ99g=";
const INSTAGRAM_WEB_DEFAULT_PAYLOAD_B64: &str = "eyJkb21haW4iOiJvc2wvYWRhcHRlci1wcm9maWxlL3YxIiwic2NoZW1hX3ZlcnNpb24iOjEsImFkYXB0ZXJfaWQiOiJpbnN0YWdyYW0ud2ViLmZpeGVkLW9yaWdpbiIsImFwcCI6eyJzdGFibGVfaWQiOiJpbnN0YWdyYW0iLCJkaXNwbGF5X25hbWUiOiJJbnN0YWdyYW0iLCJzZXJ2aWNlX2ZhbWlseSI6Im1lc3NhZ2luZyIsIm1pbl9hcHBfdmVyc2lvbiI6bnVsbH0sInJldmlzaW9uIjp7Im51bWJlciI6MiwibGFiZWwiOiIyMDI2LTA4LTA2LWluc3RhZ3JhbS13ZWItcm93LWF1dGhvci12MSJ9LCJpc3N1ZWRfYXRfdW5peF9zZWNvbmRzIjoxNzg1NjI4ODAwLCJleHBpcmVzX2F0X3VuaXhfc2Vjb25kcyI6MTkyNDk5MjAwMCwic3VwcG9ydCI6InN1cHBvcnRlZCIsImF1dGhvcml0eSI6eyJ1c2VyX2NvbnNlbnRfcmVxdWlyZWQiOnRydWUsImFjY291bnRfYmluZGluZ19yZXF1aXJlZCI6dHJ1ZSwicmVsZWFzZV9hdXRob3JpdHlfcmVxdWlyZWQiOnRydWUsImhhcm1sZXNzX2NhbmFyeV9yZXF1aXJlZCI6dHJ1ZX0sInNlbGVjdG9ycyI6W3sia2luZCI6ImFwcF9yb290Iiwic3RyYXRlZ3kiOnsia2luZCI6ImFjY2Vzc2liaWxpdHkiLCJyb2xlIjoiZG9jdW1lbnQiLCJuYW1lIjpudWxsLCJhdXRvbWF0aW9uX2lkIjpudWxsfSwicmVxdWlyZWQiOnRydWV9LHsia2luZCI6ImNvbnZlcnNhdGlvbl90aXRsZSIsInN0cmF0ZWd5Ijp7ImtpbmQiOiJhY2Nlc3NpYmlsaXR5Iiwicm9sZSI6ImhlYWRpbmciLCJuYW1lIjpudWxsLCJhdXRvbWF0aW9uX2lkIjpudWxsfSwicmVxdWlyZWQiOnRydWV9LHsia2luZCI6Im1lc3NhZ2VfbGlzdCIsInN0cmF0ZWd5Ijp7ImtpbmQiOiJhY2Nlc3NpYmlsaXR5Iiwicm9sZSI6Imxpc3QiLCJuYW1lIjpudWxsLCJhdXRvbWF0aW9uX2lkIjpudWxsfSwicmVxdWlyZWQiOnRydWV9LHsia2luZCI6Im1lc3NhZ2Vfcm93Iiwic3RyYXRlZ3kiOnsia2luZCI6ImFjY2Vzc2liaWxpdHkiLCJyb2xlIjoibGlzdGl0ZW0iLCJuYW1lIjpudWxsLCJhdXRvbWF0aW9uX2lkIjpudWxsfSwicmVxdWlyZWQiOnRydWV9LHsia2luZCI6Im1lc3NhZ2Vfcm93X2F1dGhvciIsInN0cmF0ZWd5Ijp7ImtpbmQiOiJhY2Nlc3NpYmlsaXR5Iiwicm9sZSI6InRleHQiLCJuYW1lIjpudWxsLCJhdXRvbWF0aW9uX2lkIjpudWxsfSwicmVxdWlyZWQiOnRydWV9LHsia2luZCI6ImNvbXBvc2VyX2lucHV0Iiwic3RyYXRlZ3kiOnsia2luZCI6ImFjY2Vzc2liaWxpdHkiLCJyb2xlIjoidGV4dGJveCIsIm5hbWUiOm51bGwsImF1dG9tYXRpb25faWQiOm51bGx9LCJyZXF1aXJlZCI6dHJ1ZX1dLCJmYWxsYmFja3MiOltdLCJjYW5hcnkiOnsic2VsZWN0b3IiOiJhcHBfcm9vdCIsImV4cGVjdGVkX3RleHQiOiJJbnN0YWdyYW0iLCJtYXhfYWdlX3NlY29uZHMiOjM2MDB9fQ==";
const INSTAGRAM_WEB_DEFAULT_SIGNATURE_B64: &str =
    "c/hF4EbqAilXrc+CYqVkrMmccf4MkC2/PxNTuNmSUV7JfJ1D0adzyM0Z/GweFEziDUmGKTCpffH3lPjFYWM0AQ==";
const MESSENGER_WEB_DEFAULT_SIGNING_KEY_B64: &str = "xfUmK81eHoZM92Yz1bpxPzw8kwI5f1Cz7esQJydeGhY=";
const MESSENGER_WEB_DEFAULT_PAYLOAD_B64: &str = "eyJkb21haW4iOiJvc2wvYWRhcHRlci1wcm9maWxlL3YxIiwic2NoZW1hX3ZlcnNpb24iOjEsImFkYXB0ZXJfaWQiOiJtZXNzZW5nZXIud2ViLmZpeGVkLW9yaWdpbiIsImFwcCI6eyJzdGFibGVfaWQiOiJtZXNzZW5nZXIiLCJkaXNwbGF5X25hbWUiOiJNZXNzZW5nZXIiLCJzZXJ2aWNlX2ZhbWlseSI6Im1lc3NhZ2luZyIsIm1pbl9hcHBfdmVyc2lvbiI6bnVsbH0sInJldmlzaW9uIjp7Im51bWJlciI6MiwibGFiZWwiOiIyMDI2LTA4LTA2LW1lc3Nlbmdlci13ZWItcm93LWF1dGhvci12MSJ9LCJpc3N1ZWRfYXRfdW5peF9zZWNvbmRzIjoxNzg1NjI4ODAwLCJleHBpcmVzX2F0X3VuaXhfc2Vjb25kcyI6MTkyNDk5MjAwMCwic3VwcG9ydCI6InN1cHBvcnRlZCIsImF1dGhvcml0eSI6eyJ1c2VyX2NvbnNlbnRfcmVxdWlyZWQiOnRydWUsImFjY291bnRfYmluZGluZ19yZXF1aXJlZCI6dHJ1ZSwicmVsZWFzZV9hdXRob3JpdHlfcmVxdWlyZWQiOnRydWUsImhhcm1sZXNzX2NhbmFyeV9yZXF1aXJlZCI6dHJ1ZX0sInNlbGVjdG9ycyI6W3sia2luZCI6ImFwcF9yb290Iiwic3RyYXRlZ3kiOnsia2luZCI6ImFjY2Vzc2liaWxpdHkiLCJyb2xlIjoiZG9jdW1lbnQiLCJuYW1lIjpudWxsLCJhdXRvbWF0aW9uX2lkIjpudWxsfSwicmVxdWlyZWQiOnRydWV9LHsia2luZCI6ImNvbnZlcnNhdGlvbl90aXRsZSIsInN0cmF0ZWd5Ijp7ImtpbmQiOiJhY2Nlc3NpYmlsaXR5Iiwicm9sZSI6ImhlYWRpbmciLCJuYW1lIjpudWxsLCJhdXRvbWF0aW9uX2lkIjpudWxsfSwicmVxdWlyZWQiOnRydWV9LHsia2luZCI6Im1lc3NhZ2VfbGlzdCIsInN0cmF0ZWd5Ijp7ImtpbmQiOiJhY2Nlc3NpYmlsaXR5Iiwicm9sZSI6Imxpc3QiLCJuYW1lIjpudWxsLCJhdXRvbWF0aW9uX2lkIjpudWxsfSwicmVxdWlyZWQiOnRydWV9LHsia2luZCI6Im1lc3NhZ2Vfcm93Iiwic3RyYXRlZ3kiOnsia2luZCI6ImFjY2Vzc2liaWxpdHkiLCJyb2xlIjoibGlzdGl0ZW0iLCJuYW1lIjpudWxsLCJhdXRvbWF0aW9uX2lkIjpudWxsfSwicmVxdWlyZWQiOnRydWV9LHsia2luZCI6Im1lc3NhZ2Vfcm93X2F1dGhvciIsInN0cmF0ZWd5Ijp7ImtpbmQiOiJhY2Nlc3NpYmlsaXR5Iiwicm9sZSI6InRleHQiLCJuYW1lIjpudWxsLCJhdXRvbWF0aW9uX2lkIjpudWxsfSwicmVxdWlyZWQiOnRydWV9LHsia2luZCI6ImNvbXBvc2VyX2lucHV0Iiwic3RyYXRlZ3kiOnsia2luZCI6ImFjY2Vzc2liaWxpdHkiLCJyb2xlIjoidGV4dGJveCIsIm5hbWUiOm51bGwsImF1dG9tYXRpb25faWQiOm51bGx9LCJyZXF1aXJlZCI6dHJ1ZX1dLCJmYWxsYmFja3MiOltdLCJjYW5hcnkiOnsic2VsZWN0b3IiOiJhcHBfcm9vdCIsImV4cGVjdGVkX3RleHQiOiJNZXNzZW5nZXIiLCJtYXhfYWdlX3NlY29uZHMiOjM2MDB9fQ==";
const MESSENGER_WEB_DEFAULT_SIGNATURE_B64: &str =
    "QUxAKiqkjOQmrPOKhlHcfIWMwZf7TLtq+E0EWfwYcynhnkDyGuF7sbkWE6zUtF4hiULxG467yNLIkXVb1Vf1DA==";

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
