//! WhatsApp's fill-in for the shared Scrub message reader.
//!
//! The browser adapter supplies rows it has already observed. This module
//! neither opens a WhatsApp conversation nor reads browser rows for an
//! unticked place. It keeps the common account acknowledgement boundary and
//! turns rows for one selected place into a time-ordered Scrub result.

use std::collections::BTreeSet;

use crate::privacy_scan::{did_signed_in_account_send_message, MessageOwnerCheckInput};
use crate::services::read_messaging_risk_agreement;
use crate::shared_conversation_scroll::{SharedConversationScrollablePlace, SharedPlaceMessage};

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

/// A read-only WhatsApp conversation viewport for the shared paged reader.
///
/// The viewport is constructed only from the consent- and place-tick-gated,
/// chronological result above. Scrolling therefore cannot consume more browser
/// rows or change the source conversation.
pub struct WhatsAppConversationPagePlace {
    messages: Vec<SharedWhatsAppMessage>,
    current_page: usize,
    page_size: usize,
    one_screen_scrolls: usize,
    stop_when_reading_page: Option<usize>,
    stop_request_callback: Option<Box<dyn Fn() -> Result<(), String>>>,
}

impl std::fmt::Debug for WhatsAppConversationPagePlace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WhatsAppConversationPagePlace")
            .field("message_count", &self.messages.len())
            .field("current_page", &self.current_page)
            .field("page_size", &self.page_size)
            .field("one_screen_scrolls", &self.one_screen_scrolls)
            .field("stop_when_reading_page", &self.stop_when_reading_page)
            .finish()
    }
}

/// Construct a bounded, one-screen-at-a-time WhatsApp reader for an explicitly
/// selected Scrub place. The privacy checks occur before `browser_rows` is
/// consumed, through [`read_whatsapp_messages_for_scrub`].
pub fn whatsapp_conversation_page_place_for_scrub(
    owner_osl_user_id: &str,
    account_id: &str,
    place_id: &str,
    place_is_ticked: bool,
    signed_in_author_id: &str,
    browser_rows: impl IntoIterator<Item = WhatsAppBrowserMessage>,
    page_size: usize,
) -> Result<WhatsAppConversationPagePlace, String> {
    if page_size == 0 {
        return Err("WhatsApp message page size must be at least one".to_owned());
    }
    Ok(WhatsAppConversationPagePlace {
        messages: read_whatsapp_messages_for_scrub(
            owner_osl_user_id,
            account_id,
            place_id,
            place_is_ticked,
            signed_in_author_id,
            browser_rows,
        )?,
        current_page: 0,
        page_size,
        one_screen_scrolls: 0,
        stop_when_reading_page: None,
        stop_request_callback: None,
    })
}

impl WhatsAppConversationPagePlace {
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    pub fn current_page_number(&self) -> usize {
        self.current_page.saturating_add(1)
    }

    pub fn one_screen_scroll_count(&self) -> usize {
        self.one_screen_scrolls
    }

    /// Arrange for a stop request while the nominated visible page is read.
    /// The shared gate observes it after the whole page is read, so no partial
    /// page leaks into the stop result and no next-page scroll occurs.
    pub fn request_stop_when_reading_page(
        &mut self,
        page_number: usize,
        callback: impl Fn() -> Result<(), String> + 'static,
    ) {
        self.stop_when_reading_page = Some(page_number);
        self.stop_request_callback = Some(Box::new(callback));
    }
}

impl SharedConversationScrollablePlace for WhatsAppConversationPagePlace {
    fn read_current_screen(&self) -> Result<Vec<SharedPlaceMessage>, String> {
        if self.stop_when_reading_page == Some(self.current_page_number()) {
            if let Some(callback) = &self.stop_request_callback {
                callback()?;
            }
        }
        let start = self.current_page.saturating_mul(self.page_size);
        let end = start
            .saturating_add(self.page_size)
            .min(self.messages.len());
        Ok(self.messages[start..end]
            .iter()
            .map(|message| SharedPlaceMessage::new(&message.message_id, &message.text))
            .collect())
    }

    fn scroll_one_screen(&mut self) -> Result<bool, String> {
        let next_start = (self.current_page + 1).saturating_mul(self.page_size);
        if next_start >= self.messages.len() {
            return Ok(false);
        }
        self.current_page += 1;
        self.one_screen_scrolls += 1;
        Ok(true)
    }
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
