//! WhatsApp's fill-in for the shared Scrub message reader.
//!
//! The browser adapter supplies rows it has already observed. This module
//! neither opens a WhatsApp conversation nor reads browser rows for an
//! unticked place. It keeps the common account acknowledgement boundary and
//! turns rows for one selected place into a time-ordered Scrub result.

use std::collections::BTreeSet;

use crate::privacy_scan::{did_signed_in_account_send_message, MessageOwnerCheckInput};
use crate::services::read_messaging_risk_agreement;

pub const WHATSAPP_SERVICE_ID: &str = "whatsapp";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WhatsAppBrowserMessage {
    pub conversation_id: String,
    pub message_id: String,
    pub text: String,
    pub time: i64,
    pub author_id: String,
}

impl WhatsAppBrowserMessage {
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
pub struct SharedWhatsAppMessage {
    pub service_id: &'static str,
    pub account_id: String,
    pub place_id: String,
    pub message_id: String,
    pub text: String,
    pub time: i64,
    pub author_id: String,
    pub yours: bool,
}

/// Read the browser-observed messages for one selected WhatsApp place.
///
/// Both the account acknowledgement and the explicit place tick are privacy
/// boundaries. In either unchecked state this returns before consuming
/// `browser_rows`, so callers can safely hand it a live browser iterator.
pub fn read_whatsapp_messages_for_scrub(
    owner_osl_user_id: &str,
    account_id: &str,
    place_id: &str,
    place_is_ticked: bool,
    signed_in_author_id: &str,
    browser_rows: impl IntoIterator<Item = WhatsAppBrowserMessage>,
) -> Result<Vec<SharedWhatsAppMessage>, String> {
    validate_text(owner_osl_user_id, "WhatsApp owner id")?;
    validate_text(account_id, "WhatsApp account id")?;
    validate_text(place_id, "WhatsApp place id")?;
    validate_text(signed_in_author_id, "WhatsApp signed-in author id")?;

    if !place_is_ticked
        || read_messaging_risk_agreement(owner_osl_user_id, WHATSAPP_SERVICE_ID, account_id)?
            .is_none()
    {
        return Ok(Vec::new());
    }

    let mut seen = BTreeSet::new();
    let mut messages = browser_rows
        .into_iter()
        .filter(|row| row.conversation_id == place_id)
        .map(|row| {
            validate_text(&row.conversation_id, "WhatsApp conversation id")?;
            validate_text(&row.message_id, "WhatsApp message id")?;
            validate_text(&row.text, "WhatsApp message text")?;
            validate_text(&row.author_id, "WhatsApp message author id")?;
            if !seen.insert(row.message_id.clone()) {
                return Err("WhatsApp browser returned a duplicate message id".to_owned());
            }
            Ok(SharedWhatsAppMessage {
                service_id: WHATSAPP_SERVICE_ID,
                account_id: account_id.to_owned(),
                place_id: place_id.to_owned(),
                message_id: row.message_id,
                text: row.text,
                time: row.time,
                // Keep attribution in the shared owner check so WhatsApp
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
