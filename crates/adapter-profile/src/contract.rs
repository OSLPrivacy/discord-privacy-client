//! Content-free adapter self-test verdicts.
//!
//! The shared contract carries only fixed labels and bounded counts. It has no
//! field for host text, account identifiers, handles, credentials, local paths,
//! or profile names. Absence of consent, binding, or authority is always a
//! refusal at the report level, never a degraded permission.

use serde::{de, Deserialize, Deserializer, Serialize};
use std::fmt;

pub const CONTRACT_VERSION: u16 = 1;
pub const MAX_SELF_TEST_CHECKS: usize = 32;
const REQUIRED_CHECKS: &[(Subsystem, Predicate, UnverifiedCause)] = &[
    (
        Subsystem::Composer,
        Predicate::ComposerDiscovery,
        UnverifiedCause::NotObserved,
    ),
    (
        Subsystem::Transcript,
        Predicate::TranscriptDiscovery,
        UnverifiedCause::NotObserved,
    ),
    (
        Subsystem::RowText,
        Predicate::RowTextExtraction,
        UnverifiedCause::NotObserved,
    ),
    (
        Subsystem::WriteProof,
        Predicate::WritePrefixProof,
        UnverifiedCause::MissingWriteProof,
    ),
    (
        Subsystem::Consent,
        Predicate::OperatorConsent,
        UnverifiedCause::MissingConsent,
    ),
    (
        Subsystem::Binding,
        Predicate::ScopeBinding,
        UnverifiedCause::MissingBinding,
    ),
    (
        Subsystem::Authority,
        Predicate::HostAuthority,
        UnverifiedCause::MissingAuthority,
    ),
];

/// The protected subsystem a self-test check covers.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Subsystem {
    Composer,
    Transcript,
    RowText,
    WriteProof,
    Consent,
    Binding,
    Authority,
}

impl Subsystem {
    pub fn label(self) -> &'static str {
        match self {
            Self::Composer => "subsystem_composer",
            Self::Transcript => "subsystem_transcript",
            Self::RowText => "subsystem_row_text",
            Self::WriteProof => "subsystem_write_proof",
            Self::Consent => "subsystem_consent",
            Self::Binding => "subsystem_binding",
            Self::Authority => "subsystem_authority",
        }
    }
}

/// The exact predicate a check attempted to prove.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Predicate {
    ProfileValidity,
    RouteAvailability,
    ComposerDiscovery,
    TranscriptDiscovery,
    RowTextExtraction,
    WritePrefixProof,
    OperatorConsent,
    ScopeBinding,
    HostAuthority,
}

impl Predicate {
    pub fn label(self) -> &'static str {
        match self {
            Self::ProfileValidity => "contract_profile_validity",
            Self::RouteAvailability => "contract_route_availability",
            Self::ComposerDiscovery => "contract_composer_discovery",
            Self::TranscriptDiscovery => "contract_transcript_discovery",
            Self::RowTextExtraction => "contract_row_text_extraction",
            Self::WritePrefixProof => "contract_write_prefix_proof",
            Self::OperatorConsent => "contract_operator_consent",
            Self::ScopeBinding => "contract_scope_binding",
            Self::HostAuthority => "contract_host_authority",
        }
    }
}

/// Why a predicate could not be verified.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UnverifiedCause {
    NotObserved,
    Ambiguous,
    Unsupported,
    TimedOut,
    InvalidProfile,
    BoundsExceeded,
    MissingWriteProof,
    MissingConsent,
    MissingBinding,
    MissingAuthority,
    OperatorRefused,
}

impl UnverifiedCause {
    pub fn label(self) -> &'static str {
        match self {
            Self::NotObserved => "cause_not_observed",
            Self::Ambiguous => "cause_ambiguous",
            Self::Unsupported => "cause_unsupported",
            Self::TimedOut => "cause_timed_out",
            Self::InvalidProfile => "cause_invalid_profile",
            Self::BoundsExceeded => "cause_bounds_exceeded",
            Self::MissingWriteProof => "cause_missing_write_proof",
            Self::MissingConsent => "cause_missing_consent",
            Self::MissingBinding => "cause_missing_binding",
            Self::MissingAuthority => "cause_missing_authority",
            Self::OperatorRefused => "cause_operator_refused",
        }
    }

    pub fn requires_refusal(self) -> bool {
        matches!(
            self,
            Self::MissingConsent
                | Self::MissingBinding
                | Self::MissingAuthority
                | Self::OperatorRefused
        )
    }
}

/// One measured self-test check, in report order.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckOutcome {
    pub subsystem: Subsystem,
    pub predicate: Predicate,
    pub passed: bool,
    pub cause: Option<UnverifiedCause>,
    pub observed_count: u32,
    pub required_count: u32,
}

impl CheckOutcome {
    pub fn passed(subsystem: Subsystem, predicate: Predicate) -> Self {
        Self {
            subsystem,
            predicate,
            passed: true,
            cause: None,
            observed_count: 0,
            required_count: 0,
        }
    }

    pub fn failed(
        subsystem: Subsystem,
        predicate: Predicate,
        cause: UnverifiedCause,
        observed_count: u32,
        required_count: u32,
    ) -> Self {
        Self {
            subsystem,
            predicate,
            passed: false,
            cause: Some(cause),
            observed_count,
            required_count,
        }
    }

    pub fn label(self) -> &'static str {
        if self.passed {
            "check_passed"
        } else {
            "check_failed"
        }
    }
}

/// Overall self-test verdict.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ContractVerdict {
    /// Every required check passed.
    Verified,
    /// At least one required check passed and at least one non-authority check
    /// failed. Callers may keep unrelated, independently proven read-only
    /// behavior, but not the failed subsystem.
    Degraded {
        failed_subsystem: Subsystem,
        failed_predicate: Predicate,
        cause: UnverifiedCause,
    },
    /// The protected path is refused. This is mandatory when no check passed or
    /// when consent, binding, or authority is absent.
    Refused {
        failed_subsystem: Subsystem,
        failed_predicate: Predicate,
        cause: UnverifiedCause,
    },
}

impl ContractVerdict {
    pub fn label(self) -> &'static str {
        match self {
            Self::Verified => "contract_verified",
            Self::Degraded { .. } => "contract_degraded",
            Self::Refused { .. } => "contract_refused",
        }
    }

    pub fn permits_protected_path(self) -> bool {
        matches!(self, Self::Verified)
    }
}

/// A bounded, content-free report safe to persist in local diagnostic trails.
#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfTestReport {
    version: u16,
    verdict: ContractVerdict,
    checks: Vec<CheckOutcome>,
}

impl<'de> Deserialize<'de> for SelfTestReport {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct RawSelfTestReport {
            version: u16,
            verdict: ContractVerdict,
            checks: Vec<CheckOutcome>,
        }

        let raw = RawSelfTestReport::deserialize(deserializer)?;
        let report = Self {
            version: raw.version,
            verdict: raw.verdict,
            checks: raw.checks,
        };
        report.validate().map_err(de::Error::custom)?;
        Ok(report)
    }
}

impl fmt::Debug for SelfTestReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SelfTestReport")
            .field("version", &self.version)
            .field("verdict_label", &self.verdict.label())
            .field("check_count", &self.checks.len())
            .field("failed_predicates", &self.failed_predicate_labels())
            .finish()
    }
}

impl SelfTestReport {
    pub fn from_checks(checks: Vec<CheckOutcome>) -> Result<Self, ReportError> {
        validate_checks(&checks)?;
        let verdict = classify(&checks);
        Ok(Self {
            version: CONTRACT_VERSION,
            verdict,
            checks,
        })
    }

    pub fn version(&self) -> u16 {
        self.version
    }

    pub fn verdict(&self) -> ContractVerdict {
        self.verdict
    }

    pub fn checks(&self) -> &[CheckOutcome] {
        &self.checks
    }

    pub fn failed_predicates(&self) -> Vec<Predicate> {
        self.checks
            .iter()
            .filter(|check| !check.passed)
            .map(|check| check.predicate)
            .collect()
    }

    pub fn failed_predicate_labels(&self) -> Vec<&'static str> {
        self.failed_predicates()
            .into_iter()
            .map(Predicate::label)
            .collect()
    }

    pub fn validate(&self) -> Result<(), ReportError> {
        if self.version != CONTRACT_VERSION {
            return Err(ReportError::UnsupportedVersion {
                got: self.version,
                expected: CONTRACT_VERSION,
            });
        }
        validate_checks(&self.checks)?;
        let expected = classify(&self.checks);
        if self.verdict != expected {
            return Err(ReportError::VerdictMismatch {
                got: self.verdict.label(),
                expected: expected.label(),
            });
        }
        Ok(())
    }
}

/// Structural refusal for malformed reports. These variants contain no secret
/// material and their display strings include only fixed labels or counts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReportError {
    EmptyChecks,
    TooManyChecks {
        len: usize,
        max: usize,
    },
    PassedCheckHasCause {
        predicate: Predicate,
    },
    FailedCheckMissingCause {
        predicate: Predicate,
    },
    UnsupportedVersion {
        got: u16,
        expected: u16,
    },
    VerdictMismatch {
        got: &'static str,
        expected: &'static str,
    },
}

impl fmt::Display for ReportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyChecks => write!(f, "self-test report has no checks"),
            Self::TooManyChecks { len, max } => {
                write!(
                    f,
                    "self-test report has {len} checks, over the {max}-check bound"
                )
            }
            Self::PassedCheckHasCause { predicate } => write!(
                f,
                "passed self-test check carried a failure cause for {}",
                predicate.label()
            ),
            Self::FailedCheckMissingCause { predicate } => write!(
                f,
                "failed self-test check omitted a failure cause for {}",
                predicate.label()
            ),
            Self::UnsupportedVersion { got, expected } => {
                write!(
                    f,
                    "unsupported self-test report version {got}, expected {expected}"
                )
            }
            Self::VerdictMismatch { got, expected } => {
                write!(f, "self-test verdict {got} did not match {expected}")
            }
        }
    }
}

impl std::error::Error for ReportError {}

fn validate_checks(checks: &[CheckOutcome]) -> Result<(), ReportError> {
    if checks.is_empty() {
        return Err(ReportError::EmptyChecks);
    }
    if checks.len() > MAX_SELF_TEST_CHECKS {
        return Err(ReportError::TooManyChecks {
            len: checks.len(),
            max: MAX_SELF_TEST_CHECKS,
        });
    }
    for check in checks {
        match (check.passed, check.cause) {
            (true, Some(_)) => {
                return Err(ReportError::PassedCheckHasCause {
                    predicate: check.predicate,
                })
            }
            (false, None) => {
                return Err(ReportError::FailedCheckMissingCause {
                    predicate: check.predicate,
                })
            }
            _ => {}
        }
    }
    Ok(())
}

fn classify(checks: &[CheckOutcome]) -> ContractVerdict {
    let first_missing_required = REQUIRED_CHECKS
        .iter()
        .find(|(_, predicate, _)| !checks.iter().any(|check| check.predicate == *predicate));
    let missing_refusal = REQUIRED_CHECKS.iter().find(|(_, predicate, cause)| {
        !checks.iter().any(|check| check.predicate == *predicate) && cause.requires_refusal()
    });
    if checks.iter().all(|check| check.passed) && first_missing_required.is_none() {
        return ContractVerdict::Verified;
    }

    let first_failure = checks.iter().find(|check| !check.passed);
    let refusal_failure = checks.iter().find(|check| {
        !check.passed
            && check
                .cause
                .expect("validated failed checks always carry a cause")
                .requires_refusal()
    });

    if let Some((subsystem, predicate, cause)) = missing_refusal.copied() {
        return ContractVerdict::Refused {
            failed_subsystem: subsystem,
            failed_predicate: predicate,
            cause,
        };
    }

    if let Some(refusal_failure) = refusal_failure {
        let cause = refusal_failure
            .cause
            .expect("validated failed checks always carry a cause");
        return ContractVerdict::Refused {
            failed_subsystem: refusal_failure.subsystem,
            failed_predicate: refusal_failure.predicate,
            cause,
        };
    }

    let (failed_subsystem, failed_predicate, cause) = if let Some(first_failure) = first_failure {
        (
            first_failure.subsystem,
            first_failure.predicate,
            first_failure
                .cause
                .expect("validated failed checks always carry a cause"),
        )
    } else {
        first_missing_required
            .copied()
            .expect("non-verified reports have a failed or missing required check")
    };
    if checks.iter().all(|check| !check.passed) {
        return ContractVerdict::Refused {
            failed_subsystem,
            failed_predicate,
            cause,
        };
    }

    ContractVerdict::Degraded {
        failed_subsystem,
        failed_predicate,
        cause,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn required_checks() -> Vec<CheckOutcome> {
        vec![
            CheckOutcome::passed(Subsystem::Composer, Predicate::ComposerDiscovery),
            CheckOutcome::passed(Subsystem::Transcript, Predicate::TranscriptDiscovery),
            CheckOutcome::passed(Subsystem::RowText, Predicate::RowTextExtraction),
            CheckOutcome::passed(Subsystem::WriteProof, Predicate::WritePrefixProof),
            CheckOutcome::passed(Subsystem::Consent, Predicate::OperatorConsent),
            CheckOutcome::passed(Subsystem::Binding, Predicate::ScopeBinding),
            CheckOutcome::passed(Subsystem::Authority, Predicate::HostAuthority),
        ]
    }

    #[test]
    fn contract_report_all_passed_is_verified() {
        let report = SelfTestReport::from_checks(required_checks()).unwrap();

        assert_eq!(report.version(), CONTRACT_VERSION);
        assert_eq!(report.verdict(), ContractVerdict::Verified);
        assert!(report.verdict().permits_protected_path());
        assert!(report.failed_predicates().is_empty());
        report.validate().unwrap();
    }

    #[test]
    fn contract_report_non_authority_failure_degrades() {
        let mut checks = required_checks();
        checks[1] = CheckOutcome::failed(
            Subsystem::Transcript,
            Predicate::TranscriptDiscovery,
            UnverifiedCause::Ambiguous,
            2,
            1,
        );

        let report = SelfTestReport::from_checks(checks).unwrap();

        assert_eq!(
            report.verdict(),
            ContractVerdict::Degraded {
                failed_subsystem: Subsystem::Transcript,
                failed_predicate: Predicate::TranscriptDiscovery,
                cause: UnverifiedCause::Ambiguous,
            }
        );
        assert!(!report.verdict().permits_protected_path());
        assert_eq!(
            report.failed_predicate_labels(),
            vec!["contract_transcript_discovery"]
        );
    }

    #[test]
    fn missing_required_non_authority_check_does_not_verify() {
        let checks = required_checks()
            .into_iter()
            .filter(|check| check.predicate != Predicate::TranscriptDiscovery)
            .collect();

        let report = SelfTestReport::from_checks(checks).unwrap();

        assert_eq!(
            report.verdict(),
            ContractVerdict::Degraded {
                failed_subsystem: Subsystem::Transcript,
                failed_predicate: Predicate::TranscriptDiscovery,
                cause: UnverifiedCause::NotObserved,
            }
        );
        assert!(!report.verdict().permits_protected_path());
    }

    #[test]
    fn missing_consent_check_refuses_even_when_present_checks_passed() {
        let checks = required_checks()
            .into_iter()
            .filter(|check| check.predicate != Predicate::OperatorConsent)
            .collect();

        let report = SelfTestReport::from_checks(checks).unwrap();

        assert_eq!(
            report.verdict(),
            ContractVerdict::Refused {
                failed_subsystem: Subsystem::Consent,
                failed_predicate: Predicate::OperatorConsent,
                cause: UnverifiedCause::MissingConsent,
            }
        );
        assert!(!report.verdict().permits_protected_path());
    }

    #[test]
    fn missing_authority_dominates_missing_non_authority_check() {
        let checks = required_checks()
            .into_iter()
            .filter(|check| {
                check.predicate != Predicate::TranscriptDiscovery
                    && check.predicate != Predicate::HostAuthority
            })
            .collect();

        let report = SelfTestReport::from_checks(checks).unwrap();

        assert_eq!(
            report.verdict(),
            ContractVerdict::Refused {
                failed_subsystem: Subsystem::Authority,
                failed_predicate: Predicate::HostAuthority,
                cause: UnverifiedCause::MissingAuthority,
            }
        );
        assert!(!report.verdict().permits_protected_path());
    }

    #[test]
    fn missing_consent_refuses_even_when_other_checks_passed() {
        let mut checks = required_checks();
        checks[4] = CheckOutcome::failed(
            Subsystem::Consent,
            Predicate::OperatorConsent,
            UnverifiedCause::MissingConsent,
            0,
            1,
        );

        let report = SelfTestReport::from_checks(checks).unwrap();

        assert_eq!(
            report.verdict(),
            ContractVerdict::Refused {
                failed_subsystem: Subsystem::Consent,
                failed_predicate: Predicate::OperatorConsent,
                cause: UnverifiedCause::MissingConsent,
            }
        );
    }

    #[test]
    fn missing_binding_refuses_even_when_other_checks_passed() {
        let mut checks = required_checks();
        checks[5] = CheckOutcome::failed(
            Subsystem::Binding,
            Predicate::ScopeBinding,
            UnverifiedCause::MissingBinding,
            0,
            1,
        );

        let report = SelfTestReport::from_checks(checks).unwrap();

        assert!(matches!(
            report.verdict(),
            ContractVerdict::Refused {
                failed_subsystem: Subsystem::Binding,
                failed_predicate: Predicate::ScopeBinding,
                cause: UnverifiedCause::MissingBinding,
            }
        ));
    }

    #[test]
    fn missing_authority_refuses_even_when_other_checks_passed() {
        let mut checks = required_checks();
        checks[6] = CheckOutcome::failed(
            Subsystem::Authority,
            Predicate::HostAuthority,
            UnverifiedCause::MissingAuthority,
            0,
            1,
        );

        let report = SelfTestReport::from_checks(checks).unwrap();

        assert!(matches!(
            report.verdict(),
            ContractVerdict::Refused {
                failed_subsystem: Subsystem::Authority,
                failed_predicate: Predicate::HostAuthority,
                cause: UnverifiedCause::MissingAuthority,
            }
        ));
    }

    #[test]
    fn missing_authority_refusal_dominates_earlier_degraded_failure() {
        let mut checks = required_checks();
        checks[1] = CheckOutcome::failed(
            Subsystem::Transcript,
            Predicate::TranscriptDiscovery,
            UnverifiedCause::Ambiguous,
            2,
            1,
        );
        checks[6] = CheckOutcome::failed(
            Subsystem::Authority,
            Predicate::HostAuthority,
            UnverifiedCause::MissingAuthority,
            0,
            1,
        );

        let report = SelfTestReport::from_checks(checks).unwrap();

        assert_eq!(
            report.verdict(),
            ContractVerdict::Refused {
                failed_subsystem: Subsystem::Authority,
                failed_predicate: Predicate::HostAuthority,
                cause: UnverifiedCause::MissingAuthority,
            }
        );
    }

    #[test]
    fn no_successful_check_refuses() {
        let report = SelfTestReport::from_checks(vec![
            CheckOutcome::failed(
                Subsystem::Composer,
                Predicate::ComposerDiscovery,
                UnverifiedCause::NotObserved,
                0,
                1,
            ),
            CheckOutcome::failed(
                Subsystem::WriteProof,
                Predicate::WritePrefixProof,
                UnverifiedCause::MissingWriteProof,
                0,
                1,
            ),
        ])
        .unwrap();

        assert!(matches!(
            report.verdict(),
            ContractVerdict::Refused {
                failed_subsystem: Subsystem::Composer,
                failed_predicate: Predicate::ComposerDiscovery,
                cause: UnverifiedCause::NotObserved,
            }
        ));
    }

    #[test]
    fn malformed_report_shape_is_refused() {
        assert_eq!(
            SelfTestReport::from_checks(Vec::new()).unwrap_err(),
            ReportError::EmptyChecks
        );

        let mut too_many = Vec::new();
        for _ in 0..=MAX_SELF_TEST_CHECKS {
            too_many.push(CheckOutcome::passed(
                Subsystem::Composer,
                Predicate::ComposerDiscovery,
            ));
        }
        assert_eq!(
            SelfTestReport::from_checks(too_many).unwrap_err(),
            ReportError::TooManyChecks {
                len: MAX_SELF_TEST_CHECKS + 1,
                max: MAX_SELF_TEST_CHECKS,
            }
        );
    }

    #[test]
    fn failed_check_must_name_a_cause() {
        let check = CheckOutcome {
            subsystem: Subsystem::Composer,
            predicate: Predicate::ComposerDiscovery,
            passed: false,
            cause: None,
            observed_count: 0,
            required_count: 1,
        };

        assert_eq!(
            SelfTestReport::from_checks(vec![check]).unwrap_err(),
            ReportError::FailedCheckMissingCause {
                predicate: Predicate::ComposerDiscovery,
            }
        );
    }

    #[test]
    fn passed_check_must_not_smuggle_a_failure_cause() {
        let check = CheckOutcome {
            subsystem: Subsystem::Composer,
            predicate: Predicate::ComposerDiscovery,
            passed: true,
            cause: Some(UnverifiedCause::MissingAuthority),
            observed_count: 1,
            required_count: 1,
        };

        assert_eq!(
            SelfTestReport::from_checks(vec![check]).unwrap_err(),
            ReportError::PassedCheckHasCause {
                predicate: Predicate::ComposerDiscovery,
            }
        );
    }

    #[test]
    fn deserialized_report_must_match_its_checks() {
        let json = r#"{
            "version": 1,
            "verdict": "verified",
            "checks": [{
                "subsystem": "authority",
                "predicate": "hostAuthority",
                "passed": false,
                "cause": "missingAuthority",
                "observedCount": 0,
                "requiredCount": 1
            }]
        }"#;

        let error = serde_json::from_str::<SelfTestReport>(json).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("self-test verdict contract_verified did not match contract_refused"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn debug_report_prints_labels_and_counts_only() {
        let mut checks = required_checks();
        checks[6] = CheckOutcome::failed(
            Subsystem::Authority,
            Predicate::HostAuthority,
            UnverifiedCause::MissingAuthority,
            0,
            1,
        );
        let report = SelfTestReport::from_checks(checks).unwrap();

        let debug = format!("{report:?}");

        assert!(debug.contains("SelfTestReport"));
        assert!(debug.contains("contract_refused"));
        assert!(debug.contains("contract_host_authority"));
        assert!(!debug.contains("observed_count"));
        assert!(!debug.contains("required_count"));
    }
}
