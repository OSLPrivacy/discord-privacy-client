//! Signal allowed-place commands and bilateral verification state.
//!
//! Signal allowances are directional: allowing `alice -> bob` must never
//! imply that `bob -> alice` is allowed.  The verification tick is therefore
//! represented as an optional value and exists only while both independently
//! stored directions are present.

use crate::allowed_places::{
    add_allowed_place_record, is_allowed_place_record, remove_allowed_place_record,
    AllowedPlaceRecord,
};
use crate::auto_whitelist_rules::SignalWhitelistKind;
use serde::{Deserialize, Serialize};
use std::path::Path;

const SIGNAL_APP: &str = "signal";
const MAX_SIGNAL_PLACE_ID_BYTES: usize = 512;

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalAllowanceKind {
    DirectMessage,
    GroupChat,
}

impl SignalAllowanceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DirectMessage => "direct_message",
            Self::GroupChat => "group_chat",
        }
    }

    fn allowed_place_kind(self) -> SignalWhitelistKind {
        match self {
            Self::DirectMessage => SignalWhitelistKind::DirectMessage,
            Self::GroupChat => SignalWhitelistKind::GroupChat,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalAllowanceAction {
    Add,
    Remove,
}

impl SignalAllowanceAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Remove => "remove",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalAllowanceCommandReceipt {
    pub action: SignalAllowanceAction,
    pub kind: SignalAllowanceKind,
    pub stable_id: String,
    pub changed: bool,
    pub allowed_after: bool,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalVerificationTick {
    pub kind: SignalAllowanceKind,
    pub first_account: String,
    pub second_account: String,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalAllowanceDirectionState {
    pub kind: SignalAllowanceKind,
    pub first_to_second_allowed: bool,
    pub second_to_first_allowed: bool,
    pub saved_directions: u8,
    pub state: String,
    /// Omitted from serialized command responses unless both directions exist.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification_tick: Option<SignalVerificationTick>,
}

/// Run one direct, durable Signal allowance mutation.
pub fn run_signal_allowance_command(
    app_data_dir: impl AsRef<Path>,
    action: SignalAllowanceAction,
    kind: SignalAllowanceKind,
    source_account: impl AsRef<str>,
    destination: impl AsRef<str>,
) -> Result<SignalAllowanceCommandReceipt, String> {
    let record = signal_allowance_record(kind, source_account.as_ref(), destination.as_ref())?;
    let changed = match action {
        SignalAllowanceAction::Add => {
            add_allowed_place_record(app_data_dir.as_ref(), &record)
                .map_err(|error| format!("OSL: Signal allowance add failed: {error}"))?;
            true
        }
        SignalAllowanceAction::Remove => {
            remove_allowed_place_record(app_data_dir.as_ref(), &record.stable_id)
                .map_err(|error| format!("OSL: Signal allowance remove failed: {error}"))?
        }
    };
    let allowed_after = is_allowed_place_record(app_data_dir.as_ref(), &record)
        .map_err(|error| format!("OSL: Signal allowance read-back failed: {error}"))?;

    Ok(SignalAllowanceCommandReceipt {
        action,
        kind,
        stable_id: record.stable_id,
        changed,
        allowed_after,
    })
}

/// Read the reciprocal Signal allowance state and issue a tick only for two-way state.
pub fn signal_allowance_direction_state(
    app_data_dir: impl AsRef<Path>,
    kind: SignalAllowanceKind,
    first_account: impl AsRef<str>,
    second_account: impl AsRef<str>,
) -> Result<SignalAllowanceDirectionState, String> {
    let first_account = first_account.as_ref();
    let second_account = second_account.as_ref();
    if first_account == second_account {
        return Err("OSL: Signal allowance accounts must be different".to_owned());
    }
    let first_to_second = signal_allowance_record(kind, first_account, second_account)?;
    let second_to_first = signal_allowance_record(kind, second_account, first_account)?;
    let first_to_second_allowed = is_allowed_place_record(app_data_dir.as_ref(), &first_to_second)
        .map_err(|error| format!("OSL: Signal allowance direction check failed: {error}"))?;
    let second_to_first_allowed = is_allowed_place_record(app_data_dir.as_ref(), &second_to_first)
        .map_err(|error| format!("OSL: Signal allowance direction check failed: {error}"))?;
    let saved_directions = u8::from(first_to_second_allowed) + u8::from(second_to_first_allowed);
    let state = match saved_directions {
        2 => "two-way",
        1 => "one-way",
        _ => "none",
    };
    let verification_tick = (saved_directions == 2).then(|| SignalVerificationTick {
        kind,
        first_account: first_account.to_owned(),
        second_account: second_account.to_owned(),
    });

    Ok(SignalAllowanceDirectionState {
        kind,
        first_to_second_allowed,
        second_to_first_allowed,
        saved_directions,
        state: state.to_owned(),
        verification_tick,
    })
}

fn signal_allowance_record(
    kind: SignalAllowanceKind,
    source_account: &str,
    destination: &str,
) -> Result<AllowedPlaceRecord, String> {
    validate_signal_place_id(source_account, "source account")?;
    validate_signal_place_id(destination, "destination")?;
    let record = AllowedPlaceRecord::signal(source_account, kind.allowed_place_kind(), destination);
    debug_assert_eq!(record.app, SIGNAL_APP);
    Ok(record)
}

fn validate_signal_place_id(value: &str, label: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > MAX_SIGNAL_PLACE_ID_BYTES
        || value.contains(['\0', ':'])
        || value.chars().any(char::is_whitespace)
    {
        return Err(format!("OSL: Signal allowance {label} is invalid"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsupported_or_ambiguous_ids_before_storage() {
        for invalid in ["", "two words", "colon:value", "\0"] {
            assert!(signal_allowance_record(
                SignalAllowanceKind::DirectMessage,
                "account-a",
                invalid,
            )
            .is_err());
        }
    }

    #[test]
    fn kind_names_match_the_shared_signal_allowed_place_contract() {
        assert_eq!(
            SignalAllowanceKind::DirectMessage.as_str(),
            "direct_message"
        );
        assert_eq!(SignalAllowanceKind::GroupChat.as_str(), "group_chat");
        assert_eq!(SIGNAL_APP, "signal");
    }
}
