//! Mapping from the shipping local detector's categories to warning policy.
//!
//! This is intentionally not a detector. The adapter receives the category
//! selected by `privacy_scan::classify` and decides whether it belongs to the
//! consequence warning. Keeping text inspection out of this crate prevents a
//! second, divergent scanner from growing beside the working local detector.

/// Categories emitted by the existing local `privacy_scan` detector.
///
/// At the app boundary these are converted directly from
/// `privacy_scan::PrivacyRiskCategory`; no message text reaches this type.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DetectorCategory {
    Credential,
    RecoveryMaterial,
    PaymentCard,
    GovernmentIdentity,
    PreciseLocation,
    Profanity,
    SexualContent,
    SensitiveHealth,
    ControlledSubstances,
    PotentiallyUnlawfulConduct,
    WorkSensitiveInformation,
}

/// The detector family that produced a warning category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DetectorFamily {
    StructuredText,
    LexicalText,
}

/// A category that may be shown in the non-blocking exposure warning.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum WarningCategory {
    Credential,
    RecoveryMaterial,
    PaymentCard,
    GovernmentIdentity,
    PreciseLocation,
    Profanity,
    SexualContent,
    SensitiveHealth,
    ControlledSubstances,
    PotentiallyUnlawfulConduct,
    WorkSensitiveInformation,
}

impl WarningCategory {
    /// The policy family documented for this category.
    pub const fn detector_family(self) -> DetectorFamily {
        match self {
            Self::Credential
            | Self::RecoveryMaterial
            | Self::PaymentCard
            | Self::GovernmentIdentity
            | Self::PreciseLocation => DetectorFamily::StructuredText,
            Self::Profanity
            | Self::SexualContent
            | Self::SensitiveHealth
            | Self::ControlledSubstances
            | Self::PotentiallyUnlawfulConduct
            | Self::WorkSensitiveInformation => DetectorFamily::LexicalText,
        }
    }
}

/// Map a finding category from the existing detector into warning policy.
///
/// No detector is implemented here. An integration boundary must call this
/// after the app-local scanner has produced a category and before any warning
/// policy is considered.
pub const fn warning_category_for(category: DetectorCategory) -> WarningCategory {
    match category {
        DetectorCategory::Credential => WarningCategory::Credential,
        DetectorCategory::RecoveryMaterial => WarningCategory::RecoveryMaterial,
        DetectorCategory::PaymentCard => WarningCategory::PaymentCard,
        DetectorCategory::GovernmentIdentity => WarningCategory::GovernmentIdentity,
        DetectorCategory::PreciseLocation => WarningCategory::PreciseLocation,
        DetectorCategory::Profanity => WarningCategory::Profanity,
        DetectorCategory::SexualContent => WarningCategory::SexualContent,
        DetectorCategory::SensitiveHealth => WarningCategory::SensitiveHealth,
        DetectorCategory::ControlledSubstances => WarningCategory::ControlledSubstances,
        DetectorCategory::PotentiallyUnlawfulConduct => WarningCategory::PotentiallyUnlawfulConduct,
        DetectorCategory::WorkSensitiveInformation => WarningCategory::WorkSensitiveInformation,
    }
}

#[cfg(test)]
mod tests {
    use super::{warning_category_for, DetectorCategory, DetectorFamily, WarningCategory};

    #[test]
    fn every_existing_detector_category_has_a_warning_category() {
        let mappings = [
            (DetectorCategory::Credential, WarningCategory::Credential),
            (
                DetectorCategory::RecoveryMaterial,
                WarningCategory::RecoveryMaterial,
            ),
            (DetectorCategory::PaymentCard, WarningCategory::PaymentCard),
            (
                DetectorCategory::GovernmentIdentity,
                WarningCategory::GovernmentIdentity,
            ),
            (
                DetectorCategory::PreciseLocation,
                WarningCategory::PreciseLocation,
            ),
            (DetectorCategory::Profanity, WarningCategory::Profanity),
            (
                DetectorCategory::SexualContent,
                WarningCategory::SexualContent,
            ),
            (
                DetectorCategory::SensitiveHealth,
                WarningCategory::SensitiveHealth,
            ),
            (
                DetectorCategory::ControlledSubstances,
                WarningCategory::ControlledSubstances,
            ),
            (
                DetectorCategory::PotentiallyUnlawfulConduct,
                WarningCategory::PotentiallyUnlawfulConduct,
            ),
            (
                DetectorCategory::WorkSensitiveInformation,
                WarningCategory::WorkSensitiveInformation,
            ),
        ];

        for (detector, expected_warning) in mappings {
            assert_eq!(warning_category_for(detector), expected_warning);
        }
    }

    #[test]
    fn credential_findings_stay_on_the_structured_text_warning_path() {
        let warning = warning_category_for(DetectorCategory::Credential);
        assert_eq!(warning, WarningCategory::Credential);
        assert_eq!(warning.detector_family(), DetectorFamily::StructuredText);
    }
}
