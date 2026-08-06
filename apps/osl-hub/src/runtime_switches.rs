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
