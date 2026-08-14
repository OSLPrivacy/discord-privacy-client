//! Signal's read-only open-screen Scrub message reader.

use std::collections::BTreeSet;

use crate::app_own_names::{AppOwnNameState, NamePublishedRow, RowWhoWroteIt};
use crate::models::ServiceKind;
use crate::privacy_scan::{did_signed_in_account_send_message, MessageOwnerCheckInput};
use crate::row_who_wrote_it::SharedRowWhoWroteIt;
use crate::services::{
    read_messaging_risk_agreement, ConversationPlaceKind, SharedConversationPlace,
};

pub const SIGNAL_SERVICE_ID: &str = "signal";
pub const SIGNAL_OPEN_SCREEN_MAX_MESSAGES: usize = 1_024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignalOpenScreenMessage {
    pub message_id: String,
    pub text: String,
    pub time: i64,
    pub sender_id: String,
    /// The name Signal publishes on this particular visible row, if it has
    /// one.  It is deliberately separate from `sender_id`: Signal does not
    /// expose a stable sender id through this read-only surface.
    pub published_name: Option<String>,
    /// The row's current position in the open transcript.  Ownership marks
    /// are bound to this observed position, never matched back by message
    /// words (which may legitimately repeat).
    pub screen_position: usize,
}

impl SignalOpenScreenMessage {
    pub fn new(
        message_id: impl Into<String>,
        text: impl Into<String>,
        time: i64,
        sender_id: impl Into<String>,
    ) -> Self {
        Self {
            message_id: message_id.into(),
            text: text.into(),
            time,
            sender_id: sender_id.into(),
            published_name: None,
            screen_position: 0,
        }
    }

    pub fn with_published_name_and_position(
        mut self,
        published_name: Option<String>,
        screen_position: usize,
    ) -> Self {
        self.published_name = published_name;
        self.screen_position = screen_position;
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignalOpenScreenSnapshot {
    pub place_id: String,
    pub messages: Vec<SignalOpenScreenMessage>,
}

impl SignalOpenScreenSnapshot {
    pub fn new(
        place_id: impl Into<String>,
        messages: impl IntoIterator<Item = SignalOpenScreenMessage>,
    ) -> Self {
        Self {
            place_id: place_id.into(),
            messages: messages.into_iter().collect(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SignalScreenReadAction {
    ReadOpenScreen,
    ScrollOneScreen,
    KeyPress { key: String },
}

pub trait SignalOpenScreenSource {
    fn read_open_screen(&mut self) -> Result<SignalOpenScreenSnapshot, String>;
    fn action_log(&self) -> &[SignalScreenReadAction];
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SharedSignalMessage {
    pub service_id: &'static str,
    pub account_id: String,
    pub place_id: String,
    pub message_id: String,
    pub text: String,
    pub time: i64,
    pub sender_id: String,
    pub yours: bool,
    pub published_name: Option<String>,
    pub screen_position: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignalMessageRead {
    pub messages: Vec<SharedSignalMessage>,
    pub action_log: Vec<SignalScreenReadAction>,
}

/// The shipping answer for Signal's three-state who-wrote-it question.
///
/// Signal Desktop publishes a person name on group rows, but deliberately
/// does not publish one on one-to-one rows.  A missing name is therefore an
/// explicit `NotPublishedByApp` answer, not an inference from bubble side or
/// message text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignalPublishedNameRead {
    pub messages: Vec<SharedSignalMessage>,
    pub marks: Vec<SignalPublishedNameMark>,
    pub refused: usize,
    pub refusal: Option<String>,
    pub action_log: Vec<SignalScreenReadAction>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignalPublishedNameMark {
    pub screen_position: usize,
    pub message_id: String,
    pub who_wrote_it: SharedRowWhoWroteIt,
}

/// The reason intentionally names the unavailable provider field.  This is
/// what a fresh Signal one-to-one must return for every row.
pub const SIGNAL_UNAVAILABLE_AUTHOR_REASON: &str =
    "Signal does not publish a per-row author in one-to-one conversations";

impl SignalMessageRead {
    pub fn key_press_count(&self) -> usize {
        self.action_log
            .iter()
            .filter(|action| matches!(action, SignalScreenReadAction::KeyPress { .. }))
            .count()
    }
}

pub fn read_signal_messages_for_scrub(
    owner_osl_user_id: &str,
    account_id: &str,
    selected_place: &SharedConversationPlace,
    signed_in_sender_id: &str,
    source: &mut dyn SignalOpenScreenSource,
) -> Result<SignalMessageRead, String> {
    validate_text(owner_osl_user_id, "Signal owner id", 1_024, false)?;
    validate_text(account_id, "Signal account id", 1_024, false)?;
    validate_text(
        signed_in_sender_id,
        "Signal signed-in sender id",
        1_024,
        false,
    )?;
    validate_selected_place(selected_place, account_id)?;

    if read_messaging_risk_agreement(owner_osl_user_id, SIGNAL_SERVICE_ID, account_id)?.is_none() {
        return Ok(SignalMessageRead {
            messages: Vec::new(),
            action_log: Vec::new(),
        });
    }

    let action_log_start = source.action_log().len();
    let snapshot = source.read_open_screen()?;
    let action_log = source
        .action_log()
        .get(action_log_start..)
        .ok_or_else(|| "Signal screen action log moved backwards".to_owned())?
        .to_vec();
    if action_log != [SignalScreenReadAction::ReadOpenScreen] {
        return Err(
            "Signal message read performed an action other than reading the open screen".to_owned(),
        );
    }
    if snapshot.place_id != selected_place.place_id {
        return Err("Signal open screen does not match the selected Scrub place".to_owned());
    }
    validate_text(
        &snapshot.place_id,
        "Signal open-screen place id",
        1_024,
        false,
    )?;
    if snapshot.messages.len() > SIGNAL_OPEN_SCREEN_MAX_MESSAGES {
        return Err("Signal open screen exceeded the message limit".to_owned());
    }

    let mut seen = BTreeSet::new();
    let mut messages = snapshot
        .messages
        .into_iter()
        .map(|row| {
            validate_text(&row.message_id, "Signal message id", 1_024, false)?;
            validate_text(&row.text, "Signal message text", 8_192, true)?;
            validate_text(&row.sender_id, "Signal message sender id", 1_024, false)?;
            if !seen.insert(row.message_id.clone()) {
                return Err("Signal open screen returned a duplicate message id".to_owned());
            }
            let yours = did_signed_in_account_send_message(MessageOwnerCheckInput {
                signed_in_account_sender: Some(signed_in_sender_id.to_owned()),
                message_sender: Some(row.sender_id.clone()),
            })
            .map_err(|error| error.to_string())?;
            Ok(SharedSignalMessage {
                service_id: SIGNAL_SERVICE_ID,
                account_id: account_id.to_owned(),
                place_id: snapshot.place_id.clone(),
                message_id: row.message_id,
                text: row.text,
                time: row.time,
                sender_id: row.sender_id,
                yours,
                published_name: row.published_name,
                screen_position: row.screen_position,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    messages.sort_by(|left, right| {
        left.time
            .cmp(&right.time)
            .then_with(|| left.message_id.cmp(&right.message_id))
    });
    Ok(SignalMessageRead {
        messages,
        action_log,
    })
}

/// Read the already-open Signal screen and answer who wrote each row using
/// only the row's published group name and the person's confirmed own-name
/// list (TASK 4072).  The marks are position-bound; identical message words
/// cannot move an ownership answer to a different current row.
pub fn read_signal_messages_with_published_names_for_scrub(
    owner_osl_user_id: &str,
    account_id: &str,
    selected_place: &SharedConversationPlace,
    signed_in_sender_id: &str,
    own_names: &AppOwnNameState,
    source: &mut dyn SignalOpenScreenSource,
) -> Result<SignalPublishedNameRead, String> {
    let read = read_signal_messages_for_scrub(
        owner_osl_user_id,
        account_id,
        selected_place,
        signed_in_sender_id,
        source,
    )?;
    let name_rows = read
        .messages
        .iter()
        .map(|message| NamePublishedRow {
            row_id: message.message_id.clone(),
            published_name: message.published_name.clone(),
        })
        .collect::<Vec<_>>();
    let answer = own_names.read_name_published_rows(
        owner_osl_user_id,
        ServiceKind::Signal,
        account_id,
        &name_rows,
    )?;
    let is_direct = matches!(
        selected_place.place_kind,
        ConversationPlaceKind::DirectMessage
    );
    let refused = answer.refused;
    let refusal = if is_direct && refused > 0 {
        Some(format!(
            "OSL: refused {refused} rows because {SIGNAL_UNAVAILABLE_AUTHOR_REASON}"
        ))
    } else {
        answer.refusal
    };
    let marks = answer
        .marks
        .into_iter()
        .zip(read.messages.iter())
        .map(|(mark, message)| SignalPublishedNameMark {
            screen_position: message.screen_position,
            message_id: mark.row_id,
            who_wrote_it: match mark.answer {
                RowWhoWroteIt::Yours => SharedRowWhoWroteIt::Yours,
                RowWhoWroteIt::Theirs => SharedRowWhoWroteIt::Theirs,
                RowWhoWroteIt::NotPublishedByApp => SharedRowWhoWroteIt::NotPublishedByApp,
            },
        })
        .collect();
    Ok(SignalPublishedNameRead {
        messages: read.messages,
        marks,
        refused,
        refusal,
        action_log: read.action_log,
    })
}

fn validate_selected_place(
    place: &SharedConversationPlace,
    account_id: &str,
) -> Result<(), String> {
    if place.service_id != SIGNAL_SERVICE_ID
        || place.account_id != account_id
        || !matches!(
            place.place_kind,
            ConversationPlaceKind::DirectMessage
                | ConversationPlaceKind::Group
                | ConversationPlaceKind::NoteToSelf
        )
    {
        return Err("selected Scrub place is not a Signal conversation".to_owned());
    }
    validate_text(&place.place_id, "Signal selected place id", 1_024, false)?;
    validate_text(&place.label, "Signal selected place label", 1_024, false)
}

fn validate_text(
    value: &str,
    field: &str,
    max_bytes: usize,
    allow_line_breaks: bool,
) -> Result<(), String> {
    let invalid_control = value.chars().any(|character| {
        character.is_control() && !(allow_line_breaks && matches!(character, '\n' | '\r' | '\t'))
    });
    if value.trim() == value && !value.is_empty() && value.len() <= max_bytes && !invalid_control {
        Ok(())
    } else {
        Err(format!("{field} is invalid"))
    }
}
