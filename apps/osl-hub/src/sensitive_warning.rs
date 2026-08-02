//! Local, non-blocking consequence warning for an unencrypted composer send.
//!
//! The protected-send path must never call this module: its encrypted delivery
//! is not the exposure this warning describes.  This module retains no draft
//! text or finding; only the caller-owned draft id and policy choices cross
//! the decision boundary.

use crate::privacy_scan::{scan_local_messages, LocalMessageCandidate, PrivacyRiskCategory};
use sensitive_classify::{
    categories::{warning_category_for, DetectorCategory, WarningCategory},
    policy::{DraftId, WarningDecision, WarningPolicy},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UnencryptedSendDecision {
    SendWithoutWarning,
    Warn { categories: Vec<WarningCategory> },
}

/// Scan one unencrypted draft immediately before its send action.
///
/// A scan failure, disabled warning, muted category, or dismissed draft never
/// blocks sending.  The caller presents `Warn` and may proceed after the user
/// acknowledges it.  Encrypted sends have no entry point here by design.
pub fn before_unencrypted_send(
    draft: &DraftId,
    text: &str,
    policy: &WarningPolicy<WarningCategory>,
) -> UnencryptedSendDecision {
    let findings = scan_local_messages(vec![LocalMessageCandidate {
        service_id: "composer".to_owned(),
        account_id: "local".to_owned(),
        conversation_id: "active".to_owned(),
        message_locator: "draft".to_owned(),
        authored_by_self: true,
        created_at_unix_ms: None,
        text: text.to_owned(),
        attachments: Vec::new(),
    }]);

    let mut categories: Vec<_> = findings
        .findings
        .into_iter()
        .map(|finding| warning_category_for(map_category(finding.category)))
        .filter(|category| policy.decision_for(draft, category) == WarningDecision::Warn)
        .collect();
    categories.sort();
    categories.dedup();
    if categories.is_empty() {
        UnencryptedSendDecision::SendWithoutWarning
    } else {
        UnencryptedSendDecision::Warn { categories }
    }
}

fn map_category(category: PrivacyRiskCategory) -> DetectorCategory {
    match category {
        PrivacyRiskCategory::Credential => DetectorCategory::Credential,
        PrivacyRiskCategory::RecoveryMaterial => DetectorCategory::RecoveryMaterial,
        PrivacyRiskCategory::PaymentCard => DetectorCategory::PaymentCard,
        PrivacyRiskCategory::GovernmentIdentity => DetectorCategory::GovernmentIdentity,
        PrivacyRiskCategory::PreciseLocation => DetectorCategory::PreciseLocation,
        PrivacyRiskCategory::Profanity => DetectorCategory::Profanity,
        PrivacyRiskCategory::SexualContent => DetectorCategory::SexualContent,
        PrivacyRiskCategory::SensitiveHealth => DetectorCategory::SensitiveHealth,
        PrivacyRiskCategory::ControlledSubstances => DetectorCategory::ControlledSubstances,
        PrivacyRiskCategory::PotentiallyUnlawfulConduct => DetectorCategory::PotentiallyUnlawfulConduct,
        PrivacyRiskCategory::WorkSensitiveInformation => DetectorCategory::WorkSensitiveInformation,
    }
}
