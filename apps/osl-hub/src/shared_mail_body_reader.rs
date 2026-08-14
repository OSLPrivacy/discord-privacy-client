//! The one shipping boundary that reads the words of a listed mail message.
//!
//! Providers may list a message through `services::read_shared_mailbox_messages`,
//! but they may not release its body themselves.  Every provider adapts its
//! selected message to [`ListedMailboxMessage`] and calls
//! [`read_listed_mailbox_message_words`].  The provider port intentionally has
//! no attachment operation: a body read must never fetch attachment bytes.

use crate::services::SharedMailboxMessageSummary;

/// The independently enumerated shipping provider routes.  Outlook web and
/// Outlook desktop are distinct installed routes and must not be collapsed.
pub const SHIPPING_MAIL_BODY_PROVIDER_ROUTES: [&str; 10] = [
    "gmail",
    "outlook-web",
    "outlook-desktop",
    "proton",
    "tuta",
    "yahoo",
    "aol",
    "gmx",
    "maildotcom",
    "icloud",
];

/// A message selected by the shared mailbox listing from TASK 4120.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ListedMailboxMessage {
    pub service_id: String,
    pub account_id: String,
    pub folder_id: String,
    pub message_id: String,
}

impl ListedMailboxMessage {
    pub fn from_shared_listing(message: &SharedMailboxMessageSummary) -> Self {
        Self {
            service_id: message.service_id.clone(),
            account_id: message.account_id.clone(),
            folder_id: message.folder_id.clone(),
            message_id: message.message_id.clone(),
        }
    }
}

/// The narrow text-only answer a provider may give the shared reader.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ProviderMessageWords {
    /// The opaque identifier returned by the provider, never a locally made
    /// substitute.  It must exactly match the identifier that was listed.
    pub provider_message_id: String,
    /// The provider's exact rendered/plain message words.
    pub words: String,
    /// A server-produced per-read token, retained for audit correlation.
    pub server_token: String,
    /// Must be zero.  This is reported by the provider text-only operation so
    /// an implementation cannot quietly fetch a MIME attachment to get words.
    pub attachment_bytes_fetched: u64,
}

/// A provider-specific adapter.  There is deliberately no attachment method.
pub trait ListedMailboxWordsProvider {
    fn provider_route(&self) -> &str;

    /// Fetch only the message text for the listed provider folder/id pair.
    fn fetch_listed_message_words(
        &mut self,
        folder_id: &str,
        provider_message_id: &str,
    ) -> Result<ProviderMessageWords, String>;
}

/// The provider-neutral result exposed to the rest of the shipping read path.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SharedMailBodyWords {
    pub service_id: String,
    pub account_id: String,
    pub folder_id: String,
    pub provider_message_id: String,
    pub words: String,
    pub server_token: String,
    pub attachment_bytes_fetched: u64,
}

fn checked_provider_route(route: &str) -> Result<(), String> {
    if SHIPPING_MAIL_BODY_PROVIDER_ROUTES.contains(&route) {
        Ok(())
    } else {
        Err(format!(
            "mail body reader has no shipping provider route {route}"
        ))
    }
}

fn validate_listed_message(message: &ListedMailboxMessage) -> Result<(), String> {
    for (value, label, limit) in [
        (&message.service_id, "service", 32),
        (&message.account_id, "account", 256),
        (&message.folder_id, "folder", 128),
        (&message.message_id, "message id", 180),
    ] {
        if value.is_empty()
            || value.trim() != value
            || value.len() > limit
            || value.chars().any(char::is_control)
        {
            return Err(format!("listed mailbox {label} is invalid"));
        }
    }
    checked_provider_route(&message.service_id)
}

fn validate_words(words: &str) -> Result<(), String> {
    if words.len() > 256 * 1024
        || words
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        Err("provider message words are invalid".to_owned())
    } else {
        Ok(())
    }
}

/// Read the exact words of one message that the shared mailbox has already
/// listed.  This is the sole shipping body-reading part; all ten provider
/// adapters enter here with their provider route and provider-generated id.
pub fn read_listed_mailbox_message_words(
    listed: &ListedMailboxMessage,
    provider: &mut dyn ListedMailboxWordsProvider,
) -> Result<SharedMailBodyWords, String> {
    validate_listed_message(listed)?;
    let route = provider.provider_route().to_owned();
    checked_provider_route(&route)?;
    if route != listed.service_id {
        return Err(format!(
            "mail body reader provider route mismatch: listed {}, provider {route}",
            listed.service_id
        ));
    }

    let answer = provider.fetch_listed_message_words(&listed.folder_id, &listed.message_id)?;
    if answer.provider_message_id != listed.message_id {
        return Err(format!("{route} returned a different provider message id"));
    }
    if answer.server_token.is_empty() || answer.server_token.chars().any(char::is_control) {
        return Err(format!(
            "{route} did not return a server-produced read token"
        ));
    }
    if answer.attachment_bytes_fetched != 0 {
        return Err(format!(
            "{route} fetched {} attachment bytes while reading mail words",
            answer.attachment_bytes_fetched
        ));
    }
    validate_words(&answer.words)?;

    Ok(SharedMailBodyWords {
        service_id: listed.service_id.clone(),
        account_id: listed.account_id.clone(),
        folder_id: listed.folder_id.clone(),
        provider_message_id: answer.provider_message_id,
        words: answer.words,
        server_token: answer.server_token,
        attachment_bytes_fetched: answer.attachment_bytes_fetched,
    })
}

/// Reject an empty, incomplete, duplicate, or reordered candidate inventory
/// with the affected provider named in the error.  The exact ordering binds
/// the reader inventory to the independently reviewed shipping route order.
pub fn verify_shipping_mail_body_provider_inventory(routes: &[&str]) -> Result<(), String> {
    if routes.is_empty() {
        return Err("mail body provider inventory is empty".to_owned());
    }
    for expected in SHIPPING_MAIL_BODY_PROVIDER_ROUTES {
        let count = routes.iter().filter(|route| **route == expected).count();
        if count == 0 {
            return Err(format!(
                "mail body provider inventory is missing {expected}"
            ));
        }
        if count != 1 {
            return Err(format!(
                "mail body provider inventory duplicates {expected}"
            ));
        }
    }
    if routes.len() != SHIPPING_MAIL_BODY_PROVIDER_ROUTES.len() {
        let unexpected = routes
            .iter()
            .find(|route| !SHIPPING_MAIL_BODY_PROVIDER_ROUTES.contains(route))
            .copied()
            .unwrap_or("route count");
        return Err(format!(
            "mail body provider inventory has unexpected {unexpected}"
        ));
    }
    if routes != SHIPPING_MAIL_BODY_PROVIDER_ROUTES {
        return Err(
            "mail body provider inventory no longer matches shipping route order".to_owned(),
        );
    }
    Ok(())
}
