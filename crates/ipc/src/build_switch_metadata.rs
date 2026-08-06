use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::sync::atomic::Ordering;

pub const ONE_BUILD_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const RUNTIME_SWITCH_NAMES: &[&str] = &["sender_keys_enabled", "rn_wire_in_enabled"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeSwitchRecord {
    pub name: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuildSwitchTestMetadata {
    pub one_build_version: String,
    pub runtime_switches: Vec<RuntimeSwitchRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchListValidationError {
    pub missing: Vec<String>,
    pub unknown: Vec<String>,
}

impl SwitchListValidationError {
    pub fn is_empty(&self) -> bool {
        self.missing.is_empty() && self.unknown.is_empty()
    }
}

impl std::fmt::Display for SwitchListValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts = Vec::new();
        if !self.missing.is_empty() {
            parts.push(format!(
                "missing runtime switch: {}",
                self.missing.join(", ")
            ));
        }
        if !self.unknown.is_empty() {
            parts.push(format!(
                "unknown runtime switch: {}",
                self.unknown.join(", ")
            ));
        }
        write!(f, "{}", parts.join("; "))
    }
}

pub fn cmd_osl_build_switch_test_metadata(state: &AppState) -> BuildSwitchTestMetadata {
    BuildSwitchTestMetadata {
        one_build_version: ONE_BUILD_VERSION.to_owned(),
        runtime_switches: vec![
            RuntimeSwitchRecord {
                name: "sender_keys_enabled".to_owned(),
                enabled: state.sender_keys_enabled.load(Ordering::Acquire),
            },
            RuntimeSwitchRecord {
                name: "rn_wire_in_enabled".to_owned(),
                enabled: state.rn_wire_in_enabled(),
            },
        ],
    }
}

pub fn validate_runtime_switch_list(
    provided_switches: impl IntoIterator<Item = String>,
) -> Result<(), SwitchListValidationError> {
    let expected: BTreeSet<String> = RUNTIME_SWITCH_NAMES
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    let provided: BTreeSet<String> = provided_switches.into_iter().collect();
    let error = SwitchListValidationError {
        missing: expected.difference(&provided).cloned().collect(),
        unknown: provided.difference(&expected).cloned().collect(),
    };
    if error.is_empty() {
        Ok(())
    } else {
        Err(error)
    }
}

pub fn format_build_switch_metadata(metadata: &BuildSwitchTestMetadata) -> String {
    let mut lines = vec![
        format!("one_build_version={}", metadata.one_build_version),
        format!("runtime_switches={}", metadata.runtime_switches.len()),
    ];
    lines.extend(
        metadata
            .runtime_switches
            .iter()
            .map(|switch| format!("runtime_switch: {}={}", switch.name, switch.enabled)),
    );
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_records_version_and_every_runtime_switch() {
        let state = AppState::new();

        let metadata = cmd_osl_build_switch_test_metadata(&state);

        assert_eq!(metadata.one_build_version, "0.0.1");
        assert_eq!(
            metadata.runtime_switches,
            vec![
                RuntimeSwitchRecord {
                    name: "sender_keys_enabled".to_owned(),
                    enabled: true,
                },
                RuntimeSwitchRecord {
                    name: "rn_wire_in_enabled".to_owned(),
                    enabled: false,
                },
            ]
        );
    }

    #[test]
    fn runtime_switch_list_validation_fails_when_a_switch_is_omitted() {
        let error = validate_runtime_switch_list(["sender_keys_enabled".to_owned()])
            .expect_err("omitting rn_wire_in_enabled must fail");

        assert_eq!(error.missing, vec!["rn_wire_in_enabled"]);
        assert!(error.unknown.is_empty());
    }
}
