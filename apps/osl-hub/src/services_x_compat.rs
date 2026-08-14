//! X browser-reader compatibility API recovered from the real X reader lane.
//!
//! The implementations use the shared risk-agreement and validation boundary
//! in the parent module, just as the other provider readers do.

use super::*;
use serde::{Deserialize, Serialize};

/// A place observed by the read-only X browser accessibility reader. X public
/// places are limited to posts and replies owned by the signed-in account.
#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum XBrowserPlaceKind {
    DirectMessage,
    GroupDirectMessage,
    OwnPostOrReply,
}

impl XBrowserPlaceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DirectMessage => "direct_message",
            Self::GroupDirectMessage => "group_chat",
            Self::OwnPostOrReply => "public_post",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct XBrowserPlace {
    pub place_id: String,
    pub label: String,
    pub kind: XBrowserPlaceKind,
}

impl XBrowserPlace {
    pub fn new(
        place_id: impl Into<String>,
        label: impl Into<String>,
        kind: XBrowserPlaceKind,
    ) -> Self {
        Self {
            place_id: place_id.into(),
            label: label.into(),
            kind,
        }
    }
}

/// The narrow, read-only record returned by the X browser machine. It holds
/// observed place metadata and message rows, never browser session material.
#[derive(Debug, Clone, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct XBrowserMachine {
    pub places: Vec<XBrowserPlace>,
    pub messages: Vec<XBrowserMessage>,
}

impl XBrowserMachine {
    pub fn new(places: impl IntoIterator<Item = XBrowserPlace>) -> Self {
        Self {
            places: places.into_iter().collect(),
            messages: Vec::new(),
        }
    }

    pub fn with_messages(mut self, messages: impl IntoIterator<Item = XBrowserMessage>) -> Self {
        self.messages = messages.into_iter().collect();
        self
    }
}

/// One direct-message or own-public-post row observed by the X browser reader.
#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct XBrowserMessage {
    pub place_id: String,
    pub message_id: String,
    pub text: String,
    pub time: i64,
    pub yours: bool,
}

impl XBrowserMessage {
    pub fn new(
        place_id: impl Into<String>,
        message_id: impl Into<String>,
        text: impl Into<String>,
        time: i64,
        yours: bool,
    ) -> Self {
        Self {
            place_id: place_id.into(),
            message_id: message_id.into(),
            text: text.into(),
            time,
            yours,
        }
    }
}

/// Map currently observed X browser places through the shared account-specific
/// reader. The risk-agreement check happens before any browser place is
/// released, so an unticked account sees no provider data.
pub fn read_x_shared_places(
    owner_osl_user_id: &str,
    account_id: &str,
    browser_machine: &XBrowserMachine,
) -> Result<Vec<SharedConversationPlace>, String> {
    let places = browser_machine.places.iter().map(|place| match place.kind {
        XBrowserPlaceKind::DirectMessage => {
            ConversationPlaceCandidate::direct_message(&place.place_id, &place.label)
        }
        XBrowserPlaceKind::GroupDirectMessage => {
            ConversationPlaceCandidate::group_chat(&place.place_id, &place.label)
        }
        XBrowserPlaceKind::OwnPostOrReply => {
            ConversationPlaceCandidate::public_post(&place.place_id, &place.label)
        }
    });
    read_shared_conversation_places(owner_osl_user_id, "x", account_id, places)
}

/// Read messages from one owner-selected X direct-message or own-public-post
/// place. The account-level risk agreement is checked before any row is
/// filtered or released.
pub fn read_x_shared_messages(
    owner_osl_user_id: &str,
    account_id: &str,
    place_id: &str,
    browser_machine: &XBrowserMachine,
) -> Result<Vec<SharedConversationMessage>, String> {
    validate_owner_osl_user_id(owner_osl_user_id)?;
    validate_messaging_risk_account_id(account_id)?;
    validate_conversation_message_place_id(place_id)?;
    if read_messaging_risk_agreement(owner_osl_user_id, "x", account_id)?.is_none() {
        return Ok(Vec::new());
    }

    let mut messages = browser_machine
        .messages
        .iter()
        .filter(|message| message.place_id == place_id)
        .map(|message| {
            validate_x_browser_message(message)?;
            Ok(SharedConversationMessage {
                service_id: "x".to_owned(),
                account_id: account_id.to_owned(),
                place_id: message.place_id.clone(),
                message_id: message.message_id.clone(),
                text: message.text.clone(),
                time: message.time,
                yours: message.yours,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    messages.sort_by(|left, right| {
        left.time
            .cmp(&right.time)
            .then_with(|| left.message_id.cmp(&right.message_id))
    });
    Ok(messages)
}

fn validate_x_browser_message(message: &XBrowserMessage) -> Result<(), String> {
    validate_conversation_message_place_id(&message.place_id)?;
    validate_conversation_place_text(&message.message_id, "X message id")?;
    if message.text.trim() != message.text
        || message.text.len() > 8_192
        || message
            .text
            .chars()
            .any(|character| character.is_control() && character != '\n')
    {
        return Err("X message text is invalid".to_owned());
    }
    Ok(())
}
