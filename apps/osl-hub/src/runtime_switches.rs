//! Test-only runtime switches that replace old build-selected QA shortcuts.
//!
//! These switches are deliberately narrow and named. They exist so one binary
//! can be started in either side of a former test-only branch without changing
//! Cargo features.

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
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
}

impl Default for ResolvedTestOnlyRunTimeSwitches {
    fn default() -> Self {
        Self {
            password_screen_access: PasswordScreenAccess::RequirePasswordScreen,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_password_screen_access_assignment() {
        let switches = read_test_only_runtime_switches([
            "password_screen_access=skip-password-screen-for-test",
        ])
        .unwrap();
        assert_eq!(
            switches.password_screen_access,
            PasswordScreenAccess::SkipPasswordScreenForTest
        );
        assert!(!switches.password_screen_gate_required());
    }
}
