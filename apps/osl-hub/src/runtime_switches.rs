//! Test-only runtime switches that replace old build-selected QA shortcuts.
//!
//! These switches are deliberately narrow and named. They exist so one binary
//! can be started in either side of a former test-only branch without changing
//! Cargo features.

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
use serde::Serialize;

pub const TEST_ONLY_RUNTIME_SWITCH_LIST: &str = "osl-test-only-runtime-switches";
pub const TEST_ONLY_RUNTIME_SWITCH_ENV: &str = "OSL_TEST_ONLY_RUNTIME_SWITCHES";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum PasswordScreenAccess {
    RequirePasswordScreen,
    SkipPasswordScreenForTest,
}

impl PasswordScreenAccess {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RequirePasswordScreen => "require-password-screen",
            Self::SkipPasswordScreenForTest => "skip-password-screen-for-test",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ResolvedTestOnlyRunTimeSwitches {
    pub password_screen_access: PasswordScreenAccess,
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunTimeSwitchList {
    pub name: &'static str,
    pub switches: &'static [RunTimeSwitchSpec],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunTimeSwitchSpec {
    pub name: &'static str,
    pub default_value: &'static str,
    pub allowed_values: &'static [&'static str],
    pub behavior: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTestOnlyRunTimeSwitches {
    pub password_screen_access: &'static str,
    pub safe_sending: &'static str,
}

impl Default for ResolvedTestOnlyRunTimeSwitches {
    fn default() -> Self {
        Self {
            password_screen_access: PasswordScreenAccess::RequirePasswordScreen,
        resolve_test_only_runtime_switches(&[])
            .expect("built-in test-only runtime switch defaults must resolve")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TestOnlyRunTimeSwitchChoiceReport {
    pub password_screen_access: &'static str,
    pub safe_sending: &'static str,
    pub source: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunTimeSwitchError {
    UnknownSwitchList {
        list: String,
    },
    UnknownSwitch {
        list: &'static str,
        switch: String,
    },
    UnknownValue {
        list: &'static str,
        switch: &'static str,
        value: String,
        allowed_values: &'static [&'static str],
    },
    InvalidSwitchAssignment {
        assignment: String,
    },
}

impl std::fmt::Display for RunTimeSwitchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownSwitchList { list } => {
                write!(formatter, "unknown run-time switch list {list:?}")
            }
            Self::UnknownSwitch { list, switch } => {
                write!(formatter, "unknown switch {switch:?} in list {list:?}")
            }
            Self::UnknownValue {
                list,
                switch,
                value,
                allowed_values,
            } => write!(
                formatter,
                "unknown value {value:?} for switch {switch:?} in list {list:?}; allowed: {}",
                allowed_values.join(", ")
            ),
            Self::InvalidSwitchAssignment { assignment } => write!(
                formatter,
                "invalid switch assignment {assignment:?}; expected switch=value"
            ),
        }
    }
}

impl ResolvedTestOnlyRunTimeSwitches {
    pub fn password_screen_gate_required(self) -> bool {
        match self.password_screen_access {
            PasswordScreenAccess::RequirePasswordScreen => true,
            PasswordScreenAccess::SkipPasswordScreenForTest => false,
        }
    }
}

pub fn read_startup_test_only_runtime_switches() -> Result<ResolvedTestOnlyRunTimeSwitches, String>
{
    let assignments = std::env::var("OSL_TEST_ONLY_RUNTIME_SWITCHES").unwrap_or_default();
    read_test_only_runtime_switches(assignments.split(|ch: char| ch == ',' || ch.is_whitespace()))
impl std::error::Error for RunTimeSwitchError {}

pub const TEST_ONLY_RUNTIME_SWITCH_LIST_NAME: &str = "osl-test-only-runtime-switches";
pub const TEST_ONLY_RUNTIME_SWITCH_ENV: &str = "OSL_TEST_ONLY_RUNTIME_SWITCHES";
pub const RUNTIME_SWITCH_CHOICE_SOURCE: &str = "run-time switches";

pub const PASSWORD_SCREEN_ACCESS_SWITCH: &str = "password_screen_access";
pub const PASSWORD_SCREEN_ACCESS_REQUIRED: &str = "require-password-screen";
pub const PASSWORD_SCREEN_ACCESS_SKIP_FOR_TEST: &str = "skip-password-screen-for-test";

pub const SAFE_SENDING_SWITCH: &str = "safe_sending";
pub const SAFE_SENDING_LIVE_AUTHORITY_REQUIRED: &str = "live-send-requires-authority";
pub const SAFE_SENDING_DRY_RUN_FOR_TEST: &str = "dry-run-send-for-test";

const TEST_ONLY_RUNTIME_SWITCHES: &[RunTimeSwitchSpec] = &[
    RunTimeSwitchSpec {
        name: PASSWORD_SCREEN_ACCESS_SWITCH,
        default_value: PASSWORD_SCREEN_ACCESS_REQUIRED,
        allowed_values: &[
            PASSWORD_SCREEN_ACCESS_REQUIRED,
            PASSWORD_SCREEN_ACCESS_SKIP_FOR_TEST,
        ],
        behavior: "password-screen access",
    },
    RunTimeSwitchSpec {
        name: SAFE_SENDING_SWITCH,
        default_value: SAFE_SENDING_LIVE_AUTHORITY_REQUIRED,
        allowed_values: &[
            SAFE_SENDING_LIVE_AUTHORITY_REQUIRED,
            SAFE_SENDING_DRY_RUN_FOR_TEST,
        ],

    fn from_str(value: &str) -> Option<Self> {
        match value {
            "require-password-screen" => Some(Self::RequirePasswordScreen),
            "skip-password-screen-for-test" => Some(Self::SkipPasswordScreenForTest),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum SafeSending {
    LiveSendRequiresAuthority,
    DryRunSendForTest,
}

impl SafeSending {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LiveSendRequiresAuthority => "live-send-requires-authority",
            Self::DryRunSendForTest => "dry-run-send-for-test",
        }
    }

    fn from_str(value: &str) -> Option<Self> {
        match value {
            "live-send-requires-authority" => Some(Self::LiveSendRequiresAuthority),
            "dry-run-send-for-test" => Some(Self::DryRunSendForTest),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
pub struct ResolvedTestOnlyRunTimeSwitches {
    pub password_screen_access: PasswordScreenAccess,
    pub safe_sending: SafeSending,
}

impl Default for PasswordScreenAccess {
    fn default() -> Self {
        Self::RequirePasswordScreen
    }
}

impl Default for SafeSending {
    fn default() -> Self {
        Self::LiveSendRequiresAuthority
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RunTimeSwitchDescriptor {
    pub name: &'static str,
    pub default: &'static str,
    pub allowed: &'static [&'static str],
    pub behavior: &'static str,
}

pub const TEST_ONLY_RUNTIME_SWITCHES: &[RunTimeSwitchDescriptor] = &[
    RunTimeSwitchDescriptor {
        name: "password_screen_access",
        default: "require-password-screen",
        allowed: &["require-password-screen", "skip-password-screen-for-test"],
        behavior: "password-screen access",
    },
    RunTimeSwitchDescriptor {
        name: "safe_sending",
        default: "live-send-requires-authority",
        allowed: &["live-send-requires-authority", "dry-run-send-for-test"],
        behavior: "safe sending",
    },
];

pub const TEST_ONLY_RUNTIME_SWITCH_LIST: RunTimeSwitchList = RunTimeSwitchList {
    name: TEST_ONLY_RUNTIME_SWITCH_LIST_NAME,
    switches: TEST_ONLY_RUNTIME_SWITCHES,
};

pub fn test_only_runtime_switch_list() -> &'static RunTimeSwitchList {
    &TEST_ONLY_RUNTIME_SWITCH_LIST
}

pub fn resolve_test_only_runtime_switches(
    overrides: &[(&str, &str)],
) -> Result<ResolvedTestOnlyRunTimeSwitches, RunTimeSwitchError> {
    let password_screen_access = value_or_default(PASSWORD_SCREEN_ACCESS_SWITCH, overrides)?;
    let safe_sending = value_or_default(SAFE_SENDING_SWITCH, overrides)?;

    for (switch, _) in overrides {
        if switch_spec(switch).is_none() {
            return Err(RunTimeSwitchError::UnknownSwitch {
                list: TEST_ONLY_RUNTIME_SWITCH_LIST_NAME,
                switch: (*switch).to_owned(),
            });
        }
    }

    Ok(ResolvedTestOnlyRunTimeSwitches {
        password_screen_access,
        safe_sending,
    })
}

pub fn read_test_only_runtime_switches<'a>(
    assignments: impl IntoIterator<Item = &'a str>,
) -> Result<ResolvedTestOnlyRunTimeSwitches, String> {
    let mut resolved = ResolvedTestOnlyRunTimeSwitches::default();
    for assignment in assignments {
        let assignment = assignment.trim();
        if assignment.is_empty() {
            continue;
        }
        let Some((name, value)) = assignment.split_once('=') else {
            return Err(format!("runtime switch must be name=value: {assignment}"));
        };
        match name {
            "password_screen_access" => {
                resolved.password_screen_access = match value {
                    "require-password-screen" => PasswordScreenAccess::RequirePasswordScreen,
                    "skip-password-screen-for-test" => {
                        PasswordScreenAccess::SkipPasswordScreenForTest
                    }
                    _ => {
                        return Err(format!(
                            "unknown password_screen_access runtime switch value: {value}"
                        ));
                    }
                };
            }
            _ => return Err(format!("unknown test-only runtime switch: {name}")),
        }
    }
    Ok(resolved)
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OldTestOnlyBuildChoiceReport {
    pub name: &'static str,
    pub value: &'static str,
    pub source: &'static str,
}

pub fn old_test_only_build_choice_reports(
    switches: ResolvedTestOnlyRunTimeSwitches,
) -> Vec<OldTestOnlyBuildChoiceReport> {
    vec![
        OldTestOnlyBuildChoiceReport {
            name: "password_screen_access",
            value: switches.password_screen_access.as_str(),
            source: "run-time switches",
        },
        OldTestOnlyBuildChoiceReport {
            name: "safe_sending",
            value: switches.safe_sending.as_str(),
            source: "run-time switches",
        },
    ]
}

pub fn read_startup_test_only_runtime_switches() -> Result<ResolvedTestOnlyRunTimeSwitches, String>
{
    let raw = std::env::var(TEST_ONLY_RUNTIME_SWITCH_ENV).unwrap_or_default();
    read_startup_test_only_runtime_switches_from_assignments(split_assignments(&raw))
}

pub fn read_startup_test_only_runtime_switches_from_assignments<I, S>(
    assignments: I,
) -> Result<ResolvedTestOnlyRunTimeSwitches, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    read_test_only_runtime_switches(assignments)
}

pub fn read_test_only_runtime_switches<I, S>(
    assignments: I,
) -> Result<ResolvedTestOnlyRunTimeSwitches, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut switches = ResolvedTestOnlyRunTimeSwitches::default();
    for assignment in assignments {
        let assignment = assignment.as_ref().trim();
        if assignment.is_empty() {
            continue;
        }
        let (name, value) = assignment.split_once('=').ok_or_else(|| {
            format!(
                "switch assignment \"{assignment}\" in list \"{TEST_ONLY_RUNTIME_SWITCH_LIST}\" must be name=value"
            )
        })?;
        match name {
            "password_screen_access" => {
                switches.password_screen_access = PasswordScreenAccess::from_str(value)
                    .ok_or_else(|| {
                        unknown_value_error(name, value, PasswordScreenAccess::default().as_str())
                    })?;
            }
            "safe_sending" => {
                switches.safe_sending = SafeSending::from_str(value).ok_or_else(|| {
                    unknown_value_error(name, value, SafeSending::default().as_str())
                })?;
            }
            _ => {
                return Err(format!(
                    "unknown switch \"{name}\" in list \"{TEST_ONLY_RUNTIME_SWITCH_LIST}\""
                ));
            }
        }
    }
    Ok(switches)
}

fn unknown_value_error(name: &str, value: &str, default: &str) -> String {
    let allowed = TEST_ONLY_RUNTIME_SWITCHES
        .iter()
        .find(|switch| switch.name == name)
        .map(|switch| switch.allowed.join(", "))
        .unwrap_or_else(|| default.to_owned());
    format!(
        "unknown value \"{value}\" for switch \"{name}\" in list \"{TEST_ONLY_RUNTIME_SWITCH_LIST}\"; allowed: {allowed}"
    )
}

fn split_assignments(raw: &str) -> impl Iterator<Item = &str> {
    raw.split(|byte: char| byte == ',' || byte.is_ascii_whitespace())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_password_screen_access_assignment() {
        let switches = read_test_only_runtime_switches([
            "password_screen_access=skip-password-screen-for-test",
    fn reader_defaults_to_shipping_choices() {
        let switches = read_test_only_runtime_switches(std::iter::empty::<&str>()).unwrap();
        assert_eq!(
            switches.password_screen_access,
            PasswordScreenAccess::RequirePasswordScreen
        );
        assert_eq!(
            switches.safe_sending,
            SafeSending::LiveSendRequiresAuthority
        );
    }

    #[test]
    fn reader_accepts_every_declared_override() {
        let switches = read_test_only_runtime_switches([
            "password_screen_access=skip-password-screen-for-test",
            "safe_sending=dry-run-send-for-test",
        ])
        .unwrap();
        assert_eq!(
            switches.password_screen_access,
            PasswordScreenAccess::SkipPasswordScreenForTest
        );
        assert!(!switches.password_screen_gate_required());
    }
}
) -> Result<ResolvedTestOnlyRunTimeSwitches, RunTimeSwitchError> {
    let mut overrides = Vec::new();
    for assignment in assignments {
        let Some((switch, value)) = assignment.split_once('=') else {
            return Err(RunTimeSwitchError::InvalidSwitchAssignment {
                assignment: assignment.to_owned(),
            });
        };
        if switch.is_empty() || value.is_empty() {
            return Err(RunTimeSwitchError::InvalidSwitchAssignment {
                assignment: assignment.to_owned(),
            });
        }
        overrides.push((switch, value));
    }

    resolve_test_only_runtime_switches(&overrides)
}

pub fn read_startup_test_only_runtime_switches(
) -> Result<ResolvedTestOnlyRunTimeSwitches, RunTimeSwitchError> {
    match std::env::var(TEST_ONLY_RUNTIME_SWITCH_ENV) {
        Ok(raw) => read_startup_test_only_runtime_switches_from_assignments(
            raw.split(',')
                .map(str::trim)
                .filter(|assignment| !assignment.is_empty()),
        ),
        Err(std::env::VarError::NotPresent) => {
            read_startup_test_only_runtime_switches_from_assignments([])
        }
        Err(std::env::VarError::NotUnicode(raw)) => {
            Err(RunTimeSwitchError::InvalidSwitchAssignment {
                assignment: raw.to_string_lossy().into_owned(),
            })
        }
    }
}

pub fn read_startup_test_only_runtime_switches_from_assignments<'a>(
    assignments: impl IntoIterator<Item = &'a str>,
) -> Result<ResolvedTestOnlyRunTimeSwitches, RunTimeSwitchError> {
    read_test_only_runtime_switches(assignments)
}

pub fn test_only_runtime_switch_choice_report(
    switches: &ResolvedTestOnlyRunTimeSwitches,
) -> TestOnlyRunTimeSwitchChoiceReport {
    TestOnlyRunTimeSwitchChoiceReport {
        password_screen_access: switches.password_screen_access,
        safe_sending: switches.safe_sending,
        source: RUNTIME_SWITCH_CHOICE_SOURCE,
    }
}

pub fn validate_test_only_runtime_switch_value(
    switch: &str,
    value: &str,
) -> Result<&'static str, RunTimeSwitchError> {
    let spec = switch_spec(switch).ok_or_else(|| RunTimeSwitchError::UnknownSwitch {
        list: TEST_ONLY_RUNTIME_SWITCH_LIST_NAME,
        switch: switch.to_owned(),
    })?;
    spec.allowed_values
        .iter()
        .copied()
        .find(|allowed| *allowed == value)
        .ok_or_else(|| RunTimeSwitchError::UnknownValue {
            list: TEST_ONLY_RUNTIME_SWITCH_LIST_NAME,
            switch: spec.name,
            value: value.to_owned(),
            allowed_values: spec.allowed_values,
        })
}

pub fn validate_runtime_switch_list_name(list: &str) -> Result<(), RunTimeSwitchError> {
    if list == TEST_ONLY_RUNTIME_SWITCH_LIST_NAME {
        Ok(())
    } else {
        Err(RunTimeSwitchError::UnknownSwitchList {
            list: list.to_owned(),
        })
    }
}

fn value_or_default(
    switch: &'static str,
    overrides: &[(&str, &str)],
) -> Result<&'static str, RunTimeSwitchError> {
    match overrides
        .iter()
        .rfind(|(candidate, _)| *candidate == switch)
    {
        Some((_, value)) => validate_test_only_runtime_switch_value(switch, value),
        None => Ok(switch_spec(switch)
            .expect("built-in switch must have a spec")
            .default_value),
    }
}

fn switch_spec(switch: &str) -> Option<&'static RunTimeSwitchSpec> {
    TEST_ONLY_RUNTIME_SWITCHES
        .iter()
        .find(|spec| spec.name == switch)
}
        assert_eq!(switches.safe_sending, SafeSending::DryRunSendForTest);
    }

    #[test]
    fn reader_rejects_unknown_names_and_values() {
        let bad_name = read_test_only_runtime_switches(["bad_switch_name=on"]).unwrap_err();
        assert!(bad_name.contains("unknown switch \"bad_switch_name\""));
        let bad_value = read_test_only_runtime_switches(["safe_sending=unsafe-send"]).unwrap_err();
        assert!(bad_value.contains("unknown value \"unsafe-send\""));
    }
}
