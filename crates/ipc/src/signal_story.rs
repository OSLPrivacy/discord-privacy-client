//! Signal Story controls gated by the exact selected audience in OSL's
//! durable allowed-place store.

use crate::allowed_places::{is_allowed_place_record, AllowedPlaceRecord};
use crate::auto_whitelist_rules::SignalWhitelistKind;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub const SIGNAL_STORY_ALLOWED_PLACE_KIND: &str = "story";
pub const MAX_SIGNAL_STORY_AUDIENCE_MEMBERS: usize = 128;
pub const SIGNAL_STORY_CONTROL_NAMES: [&str; 7] = [
    "lock",
    "private box",
    "count",
    "timer",
    "eye",
    "view once",
    "burn",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignalStoryAudienceInput {
    pub account: String,
    pub selected_audience: Vec<String>,
    pub stories_enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalStoryControls {
    pub status: &'static str,
    pub available: bool,
    pub stories_enabled: bool,
    pub allowed_audience: bool,
    pub audience_count: usize,
    pub control_names: Vec<&'static str>,
    pub unallowed_audience: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalStoryProtectionReceipt {
    pub status: &'static str,
    pub audience_count: usize,
    pub control_names: Vec<&'static str>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignalStoryCommandResult {
    pub exit_code: i32,
    pub stdout: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SignalStoryCommandError {
    ok: bool,
    command: &'static str,
    error: String,
}

/// Inspect all seven Task 1055 controls. Every selected member must have the
/// exact `signal:<account>:story:<member>` record; app, account, kind, and
/// stable ID are all checked by the shared allowed-place reader.
pub fn inspect_signal_story_controls(
    app_data_dir: impl AsRef<Path>,
    input: &SignalStoryAudienceInput,
) -> Result<SignalStoryControls, String> {
    validate_input(input)?;
    let mut unallowed_audience = Vec::new();
    for member in &input.selected_audience {
        let record = signal_story_audience_record(&input.account, member)?;
        let allowed = is_allowed_place_record(app_data_dir.as_ref(), &record)
            .map_err(|error| format!("OSL: Signal story audience check failed: {error}"))?;
        if !allowed {
            unallowed_audience.push(member.clone());
        }
    }
    let allowed_audience = unallowed_audience.is_empty();
    let available = input.stories_enabled && allowed_audience;

    Ok(SignalStoryControls {
        status: if available {
            "available"
        } else {
            "unavailable"
        },
        available,
        stories_enabled: input.stories_enabled,
        allowed_audience,
        audience_count: input.selected_audience.len(),
        control_names: if available {
            SIGNAL_STORY_CONTROL_NAMES.to_vec()
        } else {
            Vec::new()
        },
        unallowed_audience,
    })
}

/// Re-read the durable allowed list at the direct invocation boundary. A UI
/// availability result is never accepted as authority.
pub fn invoke_signal_story_protection(
    app_data_dir: impl AsRef<Path>,
    input: &SignalStoryAudienceInput,
) -> Result<SignalStoryProtectionReceipt, String> {
    let controls = inspect_signal_story_controls(app_data_dir, input)?;
    if !controls.stories_enabled {
        return Err("Signal Stories are disabled".to_owned());
    }
    if let Some(member) = controls.unallowed_audience.first() {
        return Err(format!(
            "Signal story audience member is not allowed: {member}"
        ));
    }
    if !controls.available {
        return Err("Signal story controls are unavailable".to_owned());
    }

    Ok(SignalStoryProtectionReceipt {
        status: "available",
        audience_count: controls.audience_count,
        control_names: controls.control_names,
    })
}

/// Headless direct command used by automation and negative checks.
pub fn run_signal_story_command<I>(args: I) -> SignalStoryCommandResult
where
    I: IntoIterator<Item = OsString>,
{
    let args = args
        .into_iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let rendered = parse_command_input(&args)
        .and_then(|(store, input)| invoke_signal_story_protection(store, &input));
    match rendered {
        Ok(receipt) => SignalStoryCommandResult {
            exit_code: 0,
            stdout: format_json_line(&receipt),
        },
        Err(error) => SignalStoryCommandResult {
            exit_code: 2,
            stdout: format_json_line(&SignalStoryCommandError {
                ok: false,
                command: "signalStoryProtect",
                error,
            }),
        },
    }
}

pub fn signal_story_audience_record(
    account: &str,
    member: &str,
) -> Result<AllowedPlaceRecord, String> {
    validate_component(account, "Signal story account is invalid")?;
    validate_component(member, "Signal story audience member is invalid")?;
    Ok(AllowedPlaceRecord::signal(
        account,
        SignalWhitelistKind::Story,
        member,
    ))
}

fn validate_input(input: &SignalStoryAudienceInput) -> Result<(), String> {
    validate_component(&input.account, "Signal story account is invalid")?;
    if input.selected_audience.is_empty()
        || input.selected_audience.len() > MAX_SIGNAL_STORY_AUDIENCE_MEMBERS
    {
        return Err("Signal story selected audience is invalid".to_owned());
    }
    let mut unique = BTreeSet::new();
    for member in &input.selected_audience {
        validate_component(member, "Signal story audience member is invalid")?;
        if member == &input.account || !unique.insert(member) {
            return Err("Signal story selected audience is invalid".to_owned());
        }
    }
    Ok(())
}

fn validate_component(value: &str, message: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 512
        || value.contains(['\0', ',', ':'])
        || value.chars().any(char::is_whitespace)
    {
        return Err(message.to_owned());
    }
    Ok(())
}

fn parse_command_input(args: &[String]) -> Result<(PathBuf, SignalStoryAudienceInput), String> {
    let mut values = BTreeMap::new();
    let mut index = 0;
    while index < args.len() {
        let key = args
            .get(index)
            .ok_or_else(|| "missing Signal story command argument".to_owned())?;
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("missing value for {key}"))?;
        if !key.starts_with("--") {
            return Err(format!("unexpected argument {key}"));
        }
        values.insert(key.trim_start_matches("--").to_owned(), value.clone());
        index += 2;
    }
    let required = |key: &str| {
        values
            .get(key)
            .cloned()
            .ok_or_else(|| format!("missing --{key}"))
    };
    let store = PathBuf::from(required("store")?);
    let selected_audience = required("audience")?
        .split(',')
        .map(str::to_owned)
        .collect();
    Ok((
        store,
        SignalStoryAudienceInput {
            account: required("account")?,
            selected_audience,
            stories_enabled: true,
        },
    ))
}

fn format_json_line<T: Serialize>(value: &T) -> String {
    format!(
        "{}\n",
        serde_json::to_string(value).expect("Signal story command response serializes")
    )
}
