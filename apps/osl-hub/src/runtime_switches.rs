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
