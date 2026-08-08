//! Messenger's fill-in for the shared Scrub message reader.
//!
//! The browser adapter supplies rows it has already observed.  This module
//! neither opens a Messenger conversation nor reads browser rows for an
//! unticked place.  It keeps the common account acknowledgement boundary and
//! turns the rows for one selected place into a time-ordered Scrub result.

use std::collections::BTreeSet;

use crate::privacy_scan::{did_signed_in_account_send_message, MessageOwnerCheckInput};
use crate::services::read_messaging_risk_agreement;

pub const MESSENGER_SERVICE_ID: &str = "messenger";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessengerBrowserMessage {
    pub conversation_id: String,
    pub message_id: String,
    pub text: String,
    pub time: i64,
    pub author_id: String,
}

impl MessengerBrowserMessage {
    pub fn new(
        conversation_id: impl Into<String>,
        message_id: impl Into<String>,
        text: impl Into<String>,
        time: i64,
        author_id: impl Into<String>,
    ) -> Self {
        Self {
            conversation_id: conversation_id.into(),
            message_id: message_id.into(),
            text: text.into(),
            time,
            author_id: author_id.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SharedMessengerMessage {
    pub service_id: &'static str,
    pub account_id: String,
    pub place_id: String,
    pub message_id: String,
    pub text: String,
    pub time: i64,
    pub author_id: String,
    pub yours: bool,
}

/// Read the browser-observed messages for one selected Messenger place.
///
/// Both the account acknowledgement and the explicit place tick are privacy
/// boundaries.  In either unchecked state this returns before consuming
/// `browser_rows`, so callers can safely hand it a live browser iterator.
pub fn read_messenger_messages_for_scrub(
    owner_osl_user_id: &str,
    account_id: &str,
    place_id: &str,
    place_is_ticked: bool,
    signed_in_author_id: &str,
    browser_rows: impl IntoIterator<Item = MessengerBrowserMessage>,
) -> Result<Vec<SharedMessengerMessage>, String> {
    validate_text(owner_osl_user_id, "Messenger owner id")?;
    validate_text(account_id, "Messenger account id")?;
    validate_text(place_id, "Messenger place id")?;
    validate_text(signed_in_author_id, "Messenger signed-in author id")?;

    if !place_is_ticked
        || read_messaging_risk_agreement(owner_osl_user_id, MESSENGER_SERVICE_ID, account_id)?
            .is_none()
    {
        return Ok(Vec::new());
    }

    let mut seen = BTreeSet::new();
    let mut messages = browser_rows
        .into_iter()
        .filter(|row| row.conversation_id == place_id)
        .map(|row| {
            validate_text(&row.conversation_id, "Messenger conversation id")?;
            validate_text(&row.message_id, "Messenger message id")?;
            validate_text(&row.text, "Messenger message text")?;
            validate_text(&row.author_id, "Messenger message author id")?;
            if !seen.insert(row.message_id.clone()) {
                return Err("Messenger browser returned a duplicate message id".to_owned());
            }
            Ok(SharedMessengerMessage {
                service_id: MESSENGER_SERVICE_ID,
                account_id: account_id.to_owned(),
                place_id: place_id.to_owned(),
                message_id: row.message_id,
                text: row.text,
                time: row.time,
                // Keep attribution in the shared owner check so Messenger
                // does not grow a service-specific definition of "yours".
                yours: did_signed_in_account_send_message(MessageOwnerCheckInput {
                    signed_in_account_sender: Some(signed_in_author_id.to_owned()),
                    message_sender: Some(row.author_id.clone()),
                })
                .map_err(|error| error.to_string())?,
                author_id: row.author_id,
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

fn validate_text(value: &str, field: &str) -> Result<(), String> {
    if value.trim() == value
        && !value.is_empty()
        && value.len() <= 1024
        && !value.chars().any(char::is_control)
    {
        Ok(())
    } else {
        Err(format!("{field} is invalid"))
    }
}
