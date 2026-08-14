//! Receiver-bound protected/normal display state for Instagram items.
//!
//! The import path accepts no protected text. It locates an exact marked cover
//! in the receiver-owned Instagram inbox and invokes OSL's established receive
//! decrypt command itself. The direct eye command can then select normal cover
//! text or those receiver-produced protected words, but cannot substitute text.

use crate::instagram_direct_message::InstagramDirectMessageInbox;
use ipc::commands::cmd_osl_decrypt_message_v2;
use ipc::state::AppState;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const INSTAGRAM_RECEIVING_JOB_SOURCE: &str = "instagram_receiving_job";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstagramItemKind {
    Message,
    Post,
    Comment,
    Story,
}

impl InstagramItemKind {
    pub const ALL: [Self; 4] = [Self::Message, Self::Post, Self::Comment, Self::Story];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Message => "message",
            Self::Post => "post",
            Self::Comment => "comment",
            Self::Story => "story",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstagramEyeState {
    Normal,
    Protected,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct InstagramEyeItem {
    marker: String,
    kind: InstagramItemKind,
    normal_text: String,
    protected_text: String,
    eye_state: InstagramEyeState,
    receiving_job_index: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InstagramEye {
    items: BTreeMap<String, InstagramEyeItem>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstagramEyeView {
    pub marker: String,
    pub kind: InstagramItemKind,
    pub eye_state: InstagramEyeState,
    pub shown_text: String,
    pub receiving_job_index: usize,
    pub protected_text_source: &'static str,
}

impl InstagramEye {
    /// Import one exact marked cover from the Instagram receiving job and open
    /// its protected text through the established OSL receiver command.
    ///
    /// There is deliberately no normal-text or protected-text parameter.
    pub fn import_from_receiving_job(
        &mut self,
        receiver: &AppState,
        inbox: &InstagramDirectMessageInbox,
        marker: &str,
        kind: InstagramItemKind,
        channel_id: &str,
        sender_provider_id: &str,
    ) -> Result<InstagramEyeView, InstagramEyeStateError> {
        validate_nonempty(marker, "marker")?;
        validate_nonempty(channel_id, "channel_id")?;
        validate_nonempty(sender_provider_id, "sender_provider_id")?;
        if self.items.contains_key(marker) {
            return Err(InstagramEyeStateError::AlreadyImported(marker.to_owned()));
        }

        let mut matches = inbox
            .received_covers()
            .iter()
            .enumerate()
            .filter(|(_, delivery)| delivery.message_mark == marker);
        let (receiving_job_index, delivery) = matches
            .next()
            .ok_or_else(|| InstagramEyeStateError::NotReceived(marker.to_owned()))?;
        if matches.next().is_some() {
            return Err(InstagramEyeStateError::DuplicateReceivedMarker(
                marker.to_owned(),
            ));
        }

        let protected_text = cmd_osl_decrypt_message_v2(
            receiver,
            Some(delivery.message_mark.clone()),
            channel_id.to_owned(),
            sender_provider_id.to_owned(),
            delivery.cover.cover_text.clone(),
            None,
            None,
        )
        .map_err(InstagramEyeStateError::ReceiverOpenFailed)?;

        let item = InstagramEyeItem {
            marker: marker.to_owned(),
            kind,
            normal_text: delivery.cover.cover_text.clone(),
            protected_text,
            eye_state: InstagramEyeState::Normal,
            receiving_job_index: receiving_job_index + 1,
        };
        let view = view(&item);
        self.items.insert(marker.to_owned(), item);
        Ok(view)
    }

    pub fn item_count(&self) -> usize {
        self.items.len()
    }

    pub fn item_state(&self, marker: &str) -> Option<InstagramEyeState> {
        self.items.get(marker).map(|item| item.eye_state)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InstagramEyeStateCommand {
    marker: String,
    state: InstagramEyeState,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstagramEyeWriteResult {
    pub marker: String,
    pub kind: InstagramItemKind,
    pub before: InstagramEyeState,
    pub after: InstagramEyeState,
    pub changed: bool,
    pub shown_text: String,
    pub receiving_job_index: usize,
    pub protected_text_source: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstagramEyeStateError {
    InvalidField(&'static str),
    NotReceived(String),
    DuplicateReceivedMarker(String),
    AlreadyImported(String),
    UnknownMarker(String),
    ReceiverOpenFailed(String),
}

impl InstagramEyeStateError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidField(_) => "invalid_field",
            Self::NotReceived(_) => "not_received",
            Self::DuplicateReceivedMarker(_) => "duplicate_received_marker",
            Self::AlreadyImported(_) => "already_imported",
            Self::UnknownMarker(_) => "unknown_marker",
            Self::ReceiverOpenFailed(_) => "receiver_open_failed",
        }
    }
}

impl core::fmt::Display for InstagramEyeStateError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidField(field) => write!(f, "Instagram eye-state {field} is empty"),
            Self::NotReceived(marker) => write!(
                f,
                "Instagram receiving job did not deliver marked item {marker:?}"
            ),
            Self::DuplicateReceivedMarker(marker) => write!(
                f,
                "Instagram receiving job delivered marked item {marker:?} more than once"
            ),
            Self::AlreadyImported(marker) => {
                write!(f, "Instagram eye already imported marked item {marker:?}")
            }
            Self::UnknownMarker(marker) => {
                write!(f, "Instagram eye has no marked item {marker:?}")
            }
            Self::ReceiverOpenFailed(error) => {
                write!(
                    f,
                    "Instagram receiving job could not open protected text: {error}"
                )
            }
        }
    }
}

impl std::error::Error for InstagramEyeStateError {}

fn validate_nonempty(value: &str, field: &'static str) -> Result<(), InstagramEyeStateError> {
    if value.trim().is_empty() {
        Err(InstagramEyeStateError::InvalidField(field))
    } else {
        Ok(())
    }
}

fn view(item: &InstagramEyeItem) -> InstagramEyeView {
    let shown_text = match item.eye_state {
        InstagramEyeState::Normal => item.normal_text.clone(),
        InstagramEyeState::Protected => item.protected_text.clone(),
    };
    InstagramEyeView {
        marker: item.marker.clone(),
        kind: item.kind,
        eye_state: item.eye_state,
        shown_text,
        receiving_job_index: item.receiving_job_index,
        protected_text_source: INSTAGRAM_RECEIVING_JOB_SOURCE,
    }
}

fn write_state(
    eye: &mut InstagramEye,
    command: InstagramEyeStateCommand,
) -> Result<InstagramEyeWriteResult, InstagramEyeStateError> {
    validate_nonempty(&command.marker, "marker")?;
    let item = eye
        .items
        .get_mut(&command.marker)
        .ok_or_else(|| InstagramEyeStateError::UnknownMarker(command.marker.clone()))?;
    let before = item.eye_state;
    item.eye_state = command.state;
    let current = view(item);
    Ok(InstagramEyeWriteResult {
        marker: current.marker,
        kind: current.kind,
        before,
        after: current.eye_state,
        changed: before != current.eye_state,
        shown_text: current.shown_text,
        receiving_job_index: current.receiving_job_index,
        protected_text_source: current.protected_text_source,
    })
}

/// Direct JSON command used by the Instagram eye control.
///
/// Request shape: `{"marker":"...","state":"protected"}`. Unknown fields
/// are refused, so neither normal nor protected stand-in text can be smuggled
/// into this state-writing boundary.
pub fn run_instagram_eye_state_command(eye: &mut InstagramEye, request_json: &str) -> String {
    let command: InstagramEyeStateCommand = match serde_json::from_str(request_json) {
        Ok(command) => command,
        Err(error) => {
            return serde_json::json!({
                "ok": false,
                "command": "write-instagram-eye-state",
                "errorCode": "bad_request",
                "error": format!("Instagram eye-state request could not be read: {error}"),
            })
            .to_string();
        }
    };

    match write_state(eye, command) {
        Ok(result) => serde_json::json!({
            "ok": true,
            "command": "write-instagram-eye-state",
            "result": result,
        })
        .to_string(),
        Err(error) => serde_json::json!({
            "ok": false,
            "command": "write-instagram-eye-state",
            "errorCode": error.code(),
            "error": error.to_string(),
        })
        .to_string(),
    }
}
