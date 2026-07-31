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

fn required_contract_check_outcomes() -> Vec<CheckOutcome> {
    REQUIRED_CHECKS
        .iter()
        .map(|(subsystem, predicate, _)| CheckOutcome::passed(*subsystem, *predicate))
        .collect()
}

pub const REQUIRED_SELF_TEST_PROBES: [SelfTestProbe; 7] = [
    SelfTestProbe::new(Subsystem::Composer, Predicate::ComposerDiscovery),
    SelfTestProbe::new(Subsystem::Transcript, Predicate::TranscriptDiscovery),
    SelfTestProbe::new(Subsystem::RowText, Predicate::RowTextExtraction),
    SelfTestProbe::new(Subsystem::WriteProof, Predicate::WritePrefixProof),
    SelfTestProbe::new(Subsystem::Consent, Predicate::OperatorConsent),
    SelfTestProbe::new(Subsystem::Binding, Predicate::ScopeBinding),
    SelfTestProbe::new(Subsystem::Authority, Predicate::HostAuthority),
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct SelfTestProbe {
    pub subsystem: Subsystem,
    pub predicate: Predicate,
}

impl SelfTestProbe {
    pub const fn new(subsystem: Subsystem, predicate: Predicate) -> Self {
        Self {
            subsystem,
            predicate,
        }
    }
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
#[serde(deny_unknown_fields)]
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
    // rename_all on the enum renames VARIANTS, not the fields inside them, so these
    // serialized as failed_subsystem/failed_predicate while the contract fixture and
    // every consumer expect camelCase. Rename per variant.
    #[serde(rename_all = "camelCase")]
    Degraded {
        failed_subsystem: Subsystem,
        failed_predicate: Predicate,
        cause: UnverifiedCause,
    },
    /// The protected path is refused. This is mandatory when no check passed or
    /// when consent, binding, or authority is absent.
    #[serde(rename_all = "camelCase")]
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
        #[serde(deny_unknown_fields)]
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
    pub fn run_contract_self_test(
        mut probe: impl FnMut(SelfTestProbe) -> CheckOutcome,
    ) -> Result<Self, ReportError> {
        let mut checks = Vec::with_capacity(REQUIRED_SELF_TEST_PROBES.len());
        for expected in REQUIRED_SELF_TEST_PROBES {
            let check = probe(expected);
            if check.subsystem != expected.subsystem || check.predicate != expected.predicate {
                return Err(ReportError::ProbeMismatch {
                    expected_subsystem: expected.subsystem,
                    expected_predicate: expected.predicate,
                    got_subsystem: check.subsystem,
                    got_predicate: check.predicate,
                });
            }
            checks.push(check);
        }
        Self::from_checks(checks)
    }

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

/// Run the local profile contract self-test without provider telemetry.
///
/// The report is deliberately made from the shared fixed predicate vocabulary:
/// no host text, account identifiers, handles, credentials, paths, profile
/// names, or adapter-local diagnostic strings can enter the returned shape.
pub fn run_contract_self_test() -> Result<SelfTestReport, ReportError> {
    SelfTestReport::from_checks(required_contract_check_outcomes())
}

pub fn run_local_fixed_label_profile_self_test() -> Result<SelfTestReport, ReportError> {
    run_contract_self_test()
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
    ProbeMismatch {
        expected_subsystem: Subsystem,
        expected_predicate: Predicate,
        got_subsystem: Subsystem,
        got_predicate: Predicate,
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
            Self::ProbeMismatch {
                expected_subsystem,
                expected_predicate,
                got_subsystem,
                got_predicate,
            } => write!(
                f,
                "self-test probe {} / {} returned {} / {}",
                expected_subsystem.label(),
                expected_predicate.label(),
                got_subsystem.label(),
                got_predicate.label()
            ),
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
    use serde_json::Value;

    fn required_checks() -> Vec<CheckOutcome> {
        required_contract_check_outcomes()
    }

    fn collect_json_keys<'a>(value: &'a Value, keys: &mut Vec<&'a str>) {
        match value {
            Value::Object(object) => {
                for (key, child) in object {
                    keys.push(key.as_str());
                    collect_json_keys(child, keys);
                }
            }
            Value::Array(values) => {
                for child in values {
                    collect_json_keys(child, keys);
                }
            }
            _ => {}
        }
    }

    fn collect_json_strings<'a>(value: &'a Value, strings: &mut Vec<&'a str>) {
        match value {
            Value::String(string) => strings.push(string.as_str()),
            Value::Object(object) => {
                for child in object.values() {
                    collect_json_strings(child, strings);
                }
            }
            Value::Array(values) => {
                for child in values {
                    collect_json_strings(child, strings);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn contract() {
        assert_eq!(
            REQUIRED_SELF_TEST_PROBES,
            [
                SelfTestProbe::new(Subsystem::Composer, Predicate::ComposerDiscovery),
                SelfTestProbe::new(Subsystem::Transcript, Predicate::TranscriptDiscovery),
                SelfTestProbe::new(Subsystem::RowText, Predicate::RowTextExtraction),
                SelfTestProbe::new(Subsystem::WriteProof, Predicate::WritePrefixProof),
                SelfTestProbe::new(Subsystem::Consent, Predicate::OperatorConsent),
                SelfTestProbe::new(Subsystem::Binding, Predicate::ScopeBinding),
                SelfTestProbe::new(Subsystem::Authority, Predicate::HostAuthority),
            ]
        );
        let expected_required_checks: &[(Subsystem, Predicate, UnverifiedCause)] = &[
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
        assert_eq!(REQUIRED_CHECKS, expected_required_checks);

        let verified = SelfTestReport::from_checks(required_checks()).unwrap();
        assert_eq!(verified.verdict(), ContractVerdict::Verified);
        assert_eq!(verified.verdict().label(), "contract_verified");
        assert!(verified.verdict().permits_protected_path());
        assert_eq!(
            serde_json::to_value(verified.verdict()).unwrap(),
            Value::String("verified".to_owned())
        );
        assert_eq!(
            serde_json::from_value::<ContractVerdict>(Value::String("verified".to_owned()))
                .unwrap(),
            ContractVerdict::Verified
        );

        let mut degraded_checks = required_checks();
        degraded_checks[1] = CheckOutcome::failed(
            Subsystem::Transcript,
            Predicate::TranscriptDiscovery,
            UnverifiedCause::Ambiguous,
            2,
            1,
        );
        let degraded = SelfTestReport::from_checks(degraded_checks).unwrap();
        assert_eq!(
            degraded.verdict(),
            ContractVerdict::Degraded {
                failed_subsystem: Subsystem::Transcript,
                failed_predicate: Predicate::TranscriptDiscovery,
                cause: UnverifiedCause::Ambiguous,
            }
        );
        assert_eq!(degraded.verdict().label(), "contract_degraded");
        assert!(!degraded.verdict().permits_protected_path());
        assert_eq!(
            serde_json::to_value(degraded.verdict()).unwrap(),
            serde_json::json!({
                "degraded": {
                    "failedSubsystem": "transcript",
                    "failedPredicate": "transcriptDiscovery",
                    "cause": "ambiguous"
                }
            })
        );
        assert_eq!(
            serde_json::from_value::<ContractVerdict>(serde_json::json!({
                "degraded": {
                    "failedSubsystem": "transcript",
                    "failedPredicate": "transcriptDiscovery",
                    "cause": "ambiguous"
                }
            }))
            .unwrap(),
            degraded.verdict()
        );

        let mut refused_checks = required_checks();
        refused_checks[5] = CheckOutcome::failed(
            Subsystem::Binding,
            Predicate::ScopeBinding,
            UnverifiedCause::MissingBinding,
            0,
            1,
        );
        let refused = SelfTestReport::from_checks(refused_checks).unwrap();
        assert_eq!(
            refused.verdict(),
            ContractVerdict::Refused {
                failed_subsystem: Subsystem::Binding,
                failed_predicate: Predicate::ScopeBinding,
                cause: UnverifiedCause::MissingBinding,
            }
        );
        assert_eq!(refused.verdict().label(), "contract_refused");
        assert!(!refused.verdict().permits_protected_path());
        assert_eq!(
            serde_json::to_value(refused.verdict()).unwrap(),
            serde_json::json!({
                "refused": {
                    "failedSubsystem": "binding",
                    "failedPredicate": "scopeBinding",
                    "cause": "missingBinding"
                }
            })
        );
        assert_eq!(
            serde_json::from_value::<ContractVerdict>(serde_json::json!({
                "refused": {
                    "failedSubsystem": "binding",
                    "failedPredicate": "scopeBinding",
                    "cause": "missingBinding"
                }
            }))
            .unwrap(),
            refused.verdict()
        );

        assert_eq!(
            [
                Subsystem::Composer.label(),
                Subsystem::Transcript.label(),
                Subsystem::RowText.label(),
                Subsystem::WriteProof.label(),
                Subsystem::Consent.label(),
                Subsystem::Binding.label(),
                Subsystem::Authority.label(),
            ],
            [
                "subsystem_composer",
                "subsystem_transcript",
                "subsystem_row_text",
                "subsystem_write_proof",
                "subsystem_consent",
                "subsystem_binding",
                "subsystem_authority",
            ]
        );
        assert_eq!(
            [
                Predicate::ProfileValidity.label(),
                Predicate::RouteAvailability.label(),
                Predicate::ComposerDiscovery.label(),
                Predicate::TranscriptDiscovery.label(),
                Predicate::RowTextExtraction.label(),
                Predicate::WritePrefixProof.label(),
                Predicate::OperatorConsent.label(),
                Predicate::ScopeBinding.label(),
                Predicate::HostAuthority.label(),
            ],
            [
                "contract_profile_validity",
                "contract_route_availability",
                "contract_composer_discovery",
                "contract_transcript_discovery",
                "contract_row_text_extraction",
                "contract_write_prefix_proof",
                "contract_operator_consent",
                "contract_scope_binding",
                "contract_host_authority",
            ]
        );
        assert_eq!(
            [
                UnverifiedCause::NotObserved.label(),
                UnverifiedCause::Ambiguous.label(),
                UnverifiedCause::Unsupported.label(),
                UnverifiedCause::TimedOut.label(),
                UnverifiedCause::InvalidProfile.label(),
                UnverifiedCause::BoundsExceeded.label(),
                UnverifiedCause::MissingWriteProof.label(),
                UnverifiedCause::MissingConsent.label(),
                UnverifiedCause::MissingBinding.label(),
                UnverifiedCause::MissingAuthority.label(),
                UnverifiedCause::OperatorRefused.label(),
            ],
            [
                "cause_not_observed",
                "cause_ambiguous",
                "cause_unsupported",
                "cause_timed_out",
                "cause_invalid_profile",
                "cause_bounds_exceeded",
                "cause_missing_write_proof",
                "cause_missing_consent",
                "cause_missing_binding",
                "cause_missing_authority",
                "cause_operator_refused",
            ]
        );

        let missing_binding_checks = required_checks()
            .into_iter()
            .filter(|check| check.predicate != Predicate::ScopeBinding)
            .collect();
        let missing_binding = SelfTestReport::from_checks(missing_binding_checks).unwrap();
        assert_eq!(
            missing_binding.verdict(),
            ContractVerdict::Refused {
                failed_subsystem: Subsystem::Binding,
                failed_predicate: Predicate::ScopeBinding,
                cause: UnverifiedCause::MissingBinding,
            }
        );
        assert!(!missing_binding.verdict().permits_protected_path());

        assert!(!UnverifiedCause::NotObserved.requires_refusal());
        assert!(!UnverifiedCause::Ambiguous.requires_refusal());
        assert!(!UnverifiedCause::Unsupported.requires_refusal());
        assert!(!UnverifiedCause::TimedOut.requires_refusal());
        assert!(!UnverifiedCause::InvalidProfile.requires_refusal());
        assert!(!UnverifiedCause::BoundsExceeded.requires_refusal());
        assert!(!UnverifiedCause::MissingWriteProof.requires_refusal());
        assert!(UnverifiedCause::MissingConsent.requires_refusal());
        assert!(UnverifiedCause::MissingBinding.requires_refusal());
        assert!(UnverifiedCause::MissingAuthority.requires_refusal());
        assert!(UnverifiedCause::OperatorRefused.requires_refusal());
    }

    mod contract {
        use super::*;

        #[test]
        fn run_contract_self_test() {
            let report = super::super::run_contract_self_test().unwrap();

            assert_eq!(report.version(), CONTRACT_VERSION);
            assert_eq!(report.verdict(), ContractVerdict::Verified);
            assert!(report.verdict().permits_protected_path());
            assert_eq!(report.checks().len(), REQUIRED_CHECKS.len());
            assert_eq!(
                report
                    .checks()
                    .iter()
                    .map(|check| (
                        check.subsystem.label(),
                        check.predicate.label(),
                        check.label()
                    ))
                    .collect::<Vec<_>>(),
                vec![
                    (
                        "subsystem_composer",
                        "contract_composer_discovery",
                        "check_passed",
                    ),
                    (
                        "subsystem_transcript",
                        "contract_transcript_discovery",
                        "check_passed",
                    ),
                    (
                        "subsystem_row_text",
                        "contract_row_text_extraction",
                        "check_passed",
                    ),
                    (
                        "subsystem_write_proof",
                        "contract_write_prefix_proof",
                        "check_passed",
                    ),
                    (
                        "subsystem_consent",
                        "contract_operator_consent",
                        "check_passed",
                    ),
                    (
                        "subsystem_binding",
                        "contract_scope_binding",
                        "check_passed",
                    ),
                    (
                        "subsystem_authority",
                        "contract_host_authority",
                        "check_passed",
                    ),
                ]
            );
            assert!(report.checks().iter().all(|check| check.cause.is_none()
                && check.observed_count == 0
                && check.required_count == 0));

            let json = serde_json::to_value(&report).unwrap();
            let object = json.as_object().expect("report object");
            let mut report_keys = object.keys().map(String::as_str).collect::<Vec<_>>();
            report_keys.sort_unstable();
            assert_eq!(report_keys, vec!["checks", "verdict", "version"]);
            assert_eq!(
                object.get("verdict"),
                Some(&Value::String("verified".to_owned()))
            );
            let checks = object
                .get("checks")
                .and_then(Value::as_array)
                .expect("checks");
            assert!(checks.iter().all(|check| {
                check.as_object().is_some_and(|fields| {
                    let mut keys = fields.keys().map(String::as_str).collect::<Vec<_>>();
                    keys.sort_unstable();
                    keys == vec![
                        "cause",
                        "observedCount",
                        "passed",
                        "predicate",
                        "requiredCount",
                        "subsystem",
                    ]
                })
            }));
        }

        #[test]
        fn run_the_profile_self_test_as_a_local_fixed_label_contract_without_telemetry() {
            let report = run_local_fixed_label_profile_self_test().unwrap();
            let shared_contract = super::super::run_contract_self_test().unwrap();

            assert_eq!(report, shared_contract);
            assert_eq!(report.version(), CONTRACT_VERSION);
            assert_eq!(report.verdict(), ContractVerdict::Verified);
            assert!(report.verdict().permits_protected_path());
            assert_eq!(report.checks().len(), REQUIRED_SELF_TEST_PROBES.len());

            for (check, expected) in report.checks().iter().zip(REQUIRED_SELF_TEST_PROBES) {
                assert_eq!(check.subsystem, expected.subsystem);
                assert_eq!(check.predicate, expected.predicate);
                assert_eq!(check.label(), "check_passed");
                assert!(check.cause.is_none());
                assert_eq!(check.observed_count, 0);
                assert_eq!(check.required_count, 0);
            }

            let json = serde_json::to_value(&report).unwrap();
            let object = json.as_object().expect("report object");
            let mut report_keys = object.keys().map(String::as_str).collect::<Vec<_>>();
            report_keys.sort_unstable();
            assert_eq!(report_keys, vec!["checks", "verdict", "version"]);
            assert_eq!(
                object.get("verdict"),
                Some(&Value::String("verified".to_owned()))
            );
            let mut telemetry_report = json.clone();
            telemetry_report["providerTelemetry"] = Value::String("host-text".to_owned());
            assert!(
                serde_json::from_value::<SelfTestReport>(telemetry_report).is_err(),
                "profile self-test reports must refuse adapter telemetry fields"
            );
            let mut nested_telemetry_report = json.clone();
            nested_telemetry_report["checks"][0]["hostText"] =
                Value::String("private account text".to_owned());
            assert!(
                serde_json::from_value::<SelfTestReport>(nested_telemetry_report).is_err(),
                "profile self-test checks must refuse adapter telemetry fields"
            );

            let checks = object
                .get("checks")
                .and_then(Value::as_array)
                .expect("checks");
            let mut fixed_string_values =
                vec![object.get("verdict").and_then(Value::as_str).unwrap()];
            let mut serialized_probes = Vec::new();
            for check in checks {
                let fields = check.as_object().expect("check object");
                let mut keys = fields.keys().map(String::as_str).collect::<Vec<_>>();
                keys.sort_unstable();
                assert_eq!(
                    keys,
                    vec![
                        "cause",
                        "observedCount",
                        "passed",
                        "predicate",
                        "requiredCount",
                        "subsystem",
                    ]
                );
                assert_eq!(fields.get("cause"), Some(&Value::Null));
                assert_eq!(fields.get("observedCount"), Some(&Value::from(0)));
                assert_eq!(fields.get("passed"), Some(&Value::Bool(true)));
                assert_eq!(fields.get("requiredCount"), Some(&Value::from(0)));
                let subsystem = fields.get("subsystem").and_then(Value::as_str).unwrap();
                let predicate = fields.get("predicate").and_then(Value::as_str).unwrap();
                fixed_string_values.push(subsystem);
                fixed_string_values.push(predicate);
                serialized_probes.push((subsystem, predicate));
            }
            assert_eq!(
                serialized_probes,
                vec![
                    ("composer", "composerDiscovery"),
                    ("transcript", "transcriptDiscovery"),
                    ("rowText", "rowTextExtraction"),
                    ("writeProof", "writePrefixProof"),
                    ("consent", "operatorConsent"),
                    ("binding", "scopeBinding"),
                    ("authority", "hostAuthority"),
                ]
            );
            assert_eq!(
                fixed_string_values,
                vec![
                    "verified",
                    "composer",
                    "composerDiscovery",
                    "transcript",
                    "transcriptDiscovery",
                    "rowText",
                    "rowTextExtraction",
                    "writeProof",
                    "writePrefixProof",
                    "consent",
                    "operatorConsent",
                    "binding",
                    "scopeBinding",
                    "authority",
                    "hostAuthority",
                ]
            );
            assert!(
                !fixed_string_values.iter().any(|value| value.contains('@')
                    || value.contains('/')
                    || value.contains("token")
                    || value.contains("secret")),
                "local contract self-test must expose only fixed labels"
            );

            // Clone for the round trip: `json` is borrowed again below to collect keys,
            // and from_value takes ownership.
            let round_tripped: SelfTestReport = serde_json::from_value(json.clone()).unwrap();
            assert_eq!(round_tripped, report);
            round_tripped.validate().unwrap();

            let mut keys = Vec::new();
            collect_json_keys(&json, &mut keys);
            for forbidden in [
                "hostText",
                "accountIdentifier",
                "accountHandle",
                "credential",
                "localPath",
                "profileName",
                "telemetry",
            ] {
                assert!(
                    !keys.contains(&forbidden),
                    "self-test report carried telemetry-like key {forbidden}"
                );
            }

            let mut strings = Vec::new();
            collect_json_strings(&json, &mut strings);
            let allowed_strings = [
                "verified",
                "composer",
                "transcript",
                "rowText",
                "writeProof",
                "consent",
                "binding",
                "authority",
                "composerDiscovery",
                "transcriptDiscovery",
                "rowTextExtraction",
                "writePrefixProof",
                "operatorConsent",
                "scopeBinding",
                "hostAuthority",
            ];
            assert!(
                strings
                    .iter()
                    .all(|string| allowed_strings.contains(string)),
                "self-test report carried a non-contract string: {strings:?}"
            );
        }
    }

    #[test]
    fn run_contract_self_test_with_required_probes() {
        let mut visited = Vec::new();
        let report = SelfTestReport::run_contract_self_test(|probe| {
            visited.push(probe);
            if probe.subsystem == Subsystem::Transcript {
                CheckOutcome::failed(
                    probe.subsystem,
                    probe.predicate,
                    UnverifiedCause::TimedOut,
                    0,
                    1,
                )
            } else {
                CheckOutcome::passed(probe.subsystem, probe.predicate)
            }
        })
        .unwrap();

        assert_eq!(visited.as_slice(), REQUIRED_SELF_TEST_PROBES.as_slice());
        assert_eq!(
            report.verdict(),
            ContractVerdict::Degraded {
                failed_subsystem: Subsystem::Transcript,
                failed_predicate: Predicate::TranscriptDiscovery,
                cause: UnverifiedCause::TimedOut,
            }
        );
        assert_eq!(report.checks().len(), REQUIRED_SELF_TEST_PROBES.len());

        let mismatch = SelfTestReport::run_contract_self_test(|probe| {
            if probe.subsystem == Subsystem::Composer {
                CheckOutcome::passed(Subsystem::Authority, Predicate::HostAuthority)
            } else {
                CheckOutcome::passed(probe.subsystem, probe.predicate)
            }
        })
        .unwrap_err();
        assert_eq!(
            mismatch,
            ReportError::ProbeMismatch {
                expected_subsystem: Subsystem::Composer,
                expected_predicate: Predicate::ComposerDiscovery,
                got_subsystem: Subsystem::Authority,
                got_predicate: Predicate::HostAuthority,
            }
        );
    }

    #[test]
    fn permits_protected_path() {
        assert!(ContractVerdict::Verified.permits_protected_path());

        let degraded = ContractVerdict::Degraded {
            failed_subsystem: Subsystem::Transcript,
            failed_predicate: Predicate::TranscriptDiscovery,
            cause: UnverifiedCause::Ambiguous,
        };
        assert!(!degraded.permits_protected_path());

        let refused = ContractVerdict::Refused {
            failed_subsystem: Subsystem::Authority,
            failed_predicate: Predicate::HostAuthority,
            cause: UnverifiedCause::MissingAuthority,
        };
        assert!(!refused.permits_protected_path());
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

        // Refusal is the invariant, and the REASON must be the one that mandates it.
        // These two checks fail with NotObserved and MissingWriteProof, neither of
        // which requires refusal on its own; the report also omits the consent and
        // binding checks entirely, and THOSE absences do mandate it. Naming Composer
        // here would report an ordinary observation miss while silently outranking a
        // missing-consent refusal.
        assert!(
            matches!(
                report.verdict(),
                ContractVerdict::Refused {
                    cause: UnverifiedCause::MissingConsent
                        | UnverifiedCause::MissingBinding
                        | UnverifiedCause::MissingAuthority
                        | UnverifiedCause::OperatorRefused,
                    ..
                }
            ),
            "no successful check must refuse, citing a refusal-mandating cause: {:?}",
            report.verdict()
        );
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
