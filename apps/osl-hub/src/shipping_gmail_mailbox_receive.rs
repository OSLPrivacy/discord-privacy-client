//! Read-only Gmail Inbox transport for shipping shared-mail conversations.
//!
//! This is deliberately separate from the old seeded `SharedMailboxSnapshot`
//! helpers.  A shipping read has no saved mailbox rows: it lists a live Gmail
//! Inbox, filters the other participant's rows, and sends every selected body
//! through TASK 4359's single shared body reader.  The transport has no Gmail
//! mutation method, so it cannot mark, move, archive, or delete a message.

use std::collections::BTreeSet;
use std::fmt;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use reqwest::blocking::Client;
use serde_json::Value;
use url::Url;
use zeroize::Zeroizing;

use crate::services::shared_mailbox_thread_name;
use crate::shared_mail_body_reader::{
    read_listed_mailbox_message_words, ListedMailboxMessage, ListedMailboxWordsProvider,
    ProviderMessageWords,
};

const GMAIL_API_ROOT: &str = "https://gmail.googleapis.com/gmail/v1/users/me";
const GMAIL_INBOX_FOLDER: &str = "Inbox";

/// There is exactly one shipping Gmail reader.  Older seeded mailbox helpers
/// are development-only test seams and are intentionally not counted here.
pub const SHIPPING_GMAIL_READER_COUNT: usize = 1;

pub const fn shipping_gmail_reader_count() -> usize {
    SHIPPING_GMAIL_READER_COUNT
}

/// The production-user binding for one Gmail inbox.  It holds identifiers and
/// addresses only; OAuth credentials stay inside the transport.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GmailMailboxBinding {
    pub account_id: String,
    pub own_address: String,
    pub other_address: String,
}

impl GmailMailboxBinding {
    pub fn new(
        account_id: impl Into<String>,
        own_address: impl Into<String>,
        other_address: impl Into<String>,
    ) -> Result<Self, String> {
        let binding = Self {
            account_id: account_id.into(),
            own_address: own_address.into(),
            other_address: other_address.into(),
        };
        validate_text(&binding.account_id, "account id", 256)?;
        validate_email(&binding.own_address, "own address")?;
        validate_email(&binding.other_address, "other address")?;
        if binding
            .own_address
            .eq_ignore_ascii_case(&binding.other_address)
        {
            return Err("Gmail shipping mailbox needs two different addresses".to_owned());
        }
        Ok(binding)
    }
}

/// Metadata returned by Gmail's Inbox list plus its metadata representation.
/// `in_inbox` is retained even though the production transport requests the
/// INBOX label, so every caller rechecks the scope before exposing a row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GmailInboxMessageMetadata {
    pub provider_message_id: String,
    pub provider_thread_id: String,
    pub subject: String,
    pub sender: String,
    pub time: i64,
    pub in_inbox: bool,
}

/// Text returned by Gmail's full-message endpoint.  `server_token` is the
/// Gmail history id from that response, not a locally generated read marker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GmailMessageText {
    pub provider_message_id: String,
    pub text: String,
    pub server_token: String,
}

/// A returned cover message, copied from Gmail provider fields and the shared
/// body reader. `thread_name` is deterministic across either participant;
/// `provider_thread_id` remains the exact Gmail thread identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GmailCoverMessage {
    pub provider_sender: String,
    pub time: i64,
    pub text: String,
    pub provider_message_id: String,
    pub provider_thread_id: String,
    pub thread_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GmailShippingRead {
    pub inbox: Vec<GmailCoverMessage>,
}

/// Read cover messages received in the production Gmail Inbox from the other
/// participant. Passing no transport, a revoked credential, or an unavailable
/// Gmail transport fails closed with `Gmail` named in the error; this function
/// never falls back to cached or seeded mailbox rows.
pub fn read_shipping_gmail_inbox(
    transport: Option<&mut GmailApiTransport>,
    binding: &GmailMailboxBinding,
) -> Result<GmailShippingRead, String> {
    let transport =
        transport.ok_or_else(|| "Gmail shipping transport is unavailable".to_owned())?;
    let metadata = transport
        .list_inbox_metadata()
        .map_err(gmail_transport_error)?;

    let mut seen = BTreeSet::new();
    let mut selected = Vec::new();
    for row in metadata {
        validate_metadata(&row)?;
        if !seen.insert(row.provider_message_id.clone()) {
            return Err("Gmail Inbox returned a duplicate provider message id".to_owned());
        }
        if row.in_inbox && sender_address(&row.sender)?.eq_ignore_ascii_case(&binding.other_address)
        {
            selected.push(row);
        }
    }

    let mut inbox = Vec::with_capacity(selected.len());
    for row in selected {
        let listed = ListedMailboxMessage {
            service_id: "gmail".to_owned(),
            account_id: binding.account_id.clone(),
            folder_id: GMAIL_INBOX_FOLDER.to_owned(),
            message_id: row.provider_message_id.clone(),
        };
        let mut body_provider = GmailBodyProvider { transport };
        let words = read_listed_mailbox_message_words(&listed, &mut body_provider)
            .map_err(|error| format!("Gmail shipping body read refused: {error}"))?;
        let thread_name =
            shared_mailbox_thread_name(&binding.own_address, &binding.other_address, &row.subject)
                .map_err(|error| format!("Gmail shipping thread name refused: {error}"))?;
        inbox.push(GmailCoverMessage {
            provider_sender: row.sender,
            time: row.time,
            text: words.words,
            provider_message_id: row.provider_message_id,
            provider_thread_id: row.provider_thread_id,
            thread_name,
        });
    }
    inbox.sort_by(|left, right| {
        left.time
            .cmp(&right.time)
            .then_with(|| left.provider_message_id.cmp(&right.provider_message_id))
    });
    Ok(GmailShippingRead { inbox })
}

struct GmailBodyProvider<'a> {
    transport: &'a mut GmailApiTransport,
}

impl ListedMailboxWordsProvider for GmailBodyProvider<'_> {
    fn provider_route(&self) -> &str {
        "gmail"
    }

    fn fetch_listed_message_words(
        &mut self,
        folder_id: &str,
        provider_message_id: &str,
    ) -> Result<ProviderMessageWords, String> {
        if folder_id != GMAIL_INBOX_FOLDER {
            return Err("Gmail shipping reader only reads Inbox".to_owned());
        }
        let text = self
            .transport
            .fetch_message_text(provider_message_id)
            .map_err(gmail_transport_error)?;
        Ok(ProviderMessageWords {
            provider_message_id: text.provider_message_id,
            words: text.text,
            server_token: text.server_token,
            attachment_bytes_fetched: 0,
        })
    }
}

/// Live Gmail REST transport. It uses OAuth bearer authentication and only
/// GETs Gmail's list/metadata/full-message endpoints. It is intentionally not
/// constructible from a fixture or a saved mailbox snapshot.
pub struct GmailApiTransport {
    client: Client,
    api_root: Url,
    bearer_token: Zeroizing<String>,
}

impl fmt::Debug for GmailApiTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GmailApiTransport")
            .field("api_root", &self.api_root)
            .field("bearer_token", &"<redacted>")
            .finish()
    }
}

impl GmailApiTransport {
    pub fn from_oauth_bearer_token(bearer_token: impl Into<String>) -> Result<Self, String> {
        Self::with_api_root(bearer_token, GMAIL_API_ROOT)
    }

    /// Kept public for a production-compatible local HTTP test server; callers
    /// still need an OAuth bearer token and all operations remain GET-only.
    pub fn with_api_root(bearer_token: impl Into<String>, api_root: &str) -> Result<Self, String> {
        let bearer_token = bearer_token.into();
        if bearer_token.trim().is_empty() || bearer_token != bearer_token.trim() {
            return Err("Gmail OAuth credential is missing or invalid".to_owned());
        }
        let mut api_root =
            Url::parse(api_root).map_err(|_| "Gmail API root is invalid".to_owned())?;
        if api_root.scheme() != "https" && api_root.host_str() != Some("127.0.0.1") {
            return Err("Gmail API root must use HTTPS".to_owned());
        }
        if !api_root.path().ends_with('/') {
            let path = format!("{}/", api_root.path());
            api_root.set_path(&path);
        }
        let client = Client::builder()
            .https_only(api_root.scheme() == "https")
            .build()
            .map_err(|error| format!("Gmail HTTP client setup failed: {error}"))?;
        Ok(Self {
            client,
            api_root,
            bearer_token: Zeroizing::new(bearer_token),
        })
    }

    fn get_json(&self, path: &str, query: &[(&str, &str)]) -> Result<Value, String> {
        let url = self
            .api_root
            .join(path)
            .map_err(|_| "Gmail API path is invalid".to_owned())?;
        let response = self
            .client
            .get(url)
            .bearer_auth(self.bearer_token.as_str())
            .query(query)
            .send()
            .map_err(|error| format!("Gmail API request failed: {error}"))?;
        if !response.status().is_success() {
            return Err(format!(
                "Gmail API credential or transport rejected read: HTTP {}",
                response.status()
            ));
        }
        response
            .json::<Value>()
            .map_err(|error| format!("Gmail API returned invalid JSON: {error}"))
    }

    fn message_metadata(
        &self,
        message_id: &str,
        thread_id: &str,
    ) -> Result<GmailInboxMessageMetadata, String> {
        let json = self.get_json(
            &format!("messages/{message_id}"),
            &[
                ("format", "metadata"),
                ("metadataHeaders", "From"),
                ("metadataHeaders", "Subject"),
            ],
        )?;
        let provider_message_id = json_string(&json, "id")?;
        if provider_message_id != message_id {
            return Err("Gmail API returned a different message id".to_owned());
        }
        let provider_thread_id = json
            .get("threadId")
            .and_then(Value::as_str)
            .unwrap_or(thread_id)
            .to_owned();
        let headers = json
            .pointer("/payload/headers")
            .and_then(Value::as_array)
            .ok_or_else(|| "Gmail API message metadata lacks headers".to_owned())?;
        let sender = gmail_header(headers, "From")?;
        let subject = gmail_header(headers, "Subject")?;
        let millis = json_string(&json, "internalDate").and_then(|value| {
            value
                .parse::<i64>()
                .map_err(|_| "Gmail API internal date is invalid".to_owned())
        })?;
        Ok(GmailInboxMessageMetadata {
            provider_message_id,
            provider_thread_id,
            subject,
            sender,
            time: millis / 1000,
            in_inbox: true,
        })
    }
}

impl GmailApiTransport {
    fn list_inbox_metadata(&mut self) -> Result<Vec<GmailInboxMessageMetadata>, String> {
        let mut metadata = Vec::new();
        let mut page_token = None::<String>;
        loop {
            let mut query = vec![("labelIds", "INBOX"), ("maxResults", "100")];
            if let Some(token) = page_token.as_deref() {
                query.push(("pageToken", token));
            }
            let json = self.get_json("messages", &query)?;
            if let Some(messages) = json.get("messages").and_then(Value::as_array) {
                for message in messages {
                    let message_id = json_string(message, "id")?;
                    let thread_id = json_string(message, "threadId")?;
                    metadata.push(self.message_metadata(&message_id, &thread_id)?);
                }
            }
            page_token = json
                .get("nextPageToken")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            if page_token.is_none() {
                break;
            }
        }
        Ok(metadata)
    }

    fn fetch_message_text(
        &mut self,
        provider_message_id: &str,
    ) -> Result<GmailMessageText, String> {
        validate_text(provider_message_id, "provider message id", 180)?;
        let json = self.get_json(
            &format!("messages/{provider_message_id}"),
            &[("format", "full")],
        )?;
        let returned_id = json_string(&json, "id")?;
        if returned_id != provider_message_id {
            return Err("Gmail API returned a different message id".to_owned());
        }
        let payload = json
            .get("payload")
            .ok_or_else(|| "Gmail API full message lacks payload".to_owned())?;
        Ok(GmailMessageText {
            provider_message_id: returned_id,
            text: gmail_payload_text(payload)?,
            server_token: json_string(&json, "historyId")?,
        })
    }
}

fn gmail_transport_error(error: String) -> String {
    if error.contains("Gmail") {
        error
    } else {
        format!("Gmail shipping transport failed: {error}")
    }
}

fn validate_metadata(row: &GmailInboxMessageMetadata) -> Result<(), String> {
    validate_text(&row.provider_message_id, "provider message id", 180)?;
    validate_text(&row.provider_thread_id, "provider thread id", 180)?;
    validate_text(&row.subject, "message subject", 512)?;
    validate_text(&row.sender, "provider sender", 254)?;
    validate_email(&sender_address(&row.sender)?, "provider sender")?;
    if row.time <= 0 {
        return Err("Gmail provider time is invalid".to_owned());
    }
    Ok(())
}

fn validate_text(value: &str, label: &str, limit: usize) -> Result<(), String> {
    if value.is_empty()
        || value.trim() != value
        || value.len() > limit
        || value.chars().any(char::is_control)
    {
        return Err(format!("Gmail {label} is invalid"));
    }
    Ok(())
}

fn validate_email(value: &str, label: &str) -> Result<(), String> {
    validate_text(value, label, 254)?;
    let (local, domain) = value
        .rsplit_once('@')
        .ok_or_else(|| format!("Gmail {label} is invalid"))?;
    if local.is_empty()
        || domain.is_empty()
        || !domain.contains('.')
        || value.matches('@').count() != 1
    {
        return Err(format!("Gmail {label} is invalid"));
    }
    Ok(())
}

fn sender_address(value: &str) -> Result<String, String> {
    let value = value.trim();
    let address = match (value.rfind('<'), value.rfind('>')) {
        (Some(start), Some(end)) if start < end && end == value.len() - 1 => &value[start + 1..end],
        (None, None) => value,
        _ => return Err("Gmail provider sender is invalid".to_owned()),
    };
    validate_email(address.trim(), "provider sender")?;
    Ok(address.trim().to_ascii_lowercase())
}

fn json_string(json: &Value, field: &str) -> Result<String, String> {
    json.get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("Gmail API response lacks {field}"))
}

fn gmail_header(headers: &[Value], name: &str) -> Result<String, String> {
    headers
        .iter()
        .find(|header| {
            header
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|candidate| candidate.eq_ignore_ascii_case(name))
        })
        .and_then(|header| header.get("value"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("Gmail API message lacks {name} header"))
}

fn gmail_payload_text(payload: &Value) -> Result<String, String> {
    fn visit(payload: &Value, fallback: &mut Option<String>) -> Result<Option<String>, String> {
        let mime = payload
            .get("mimeType")
            .and_then(Value::as_str)
            .unwrap_or("");
        if mime.eq_ignore_ascii_case("text/plain") {
            return gmail_payload_data(payload).map(Some);
        }
        if mime.eq_ignore_ascii_case("text/html") && fallback.is_none() {
            *fallback = Some(gmail_payload_data(payload)?);
        }
        if let Some(parts) = payload.get("parts").and_then(Value::as_array) {
            for part in parts {
                if let Some(text) = visit(part, fallback)? {
                    return Ok(Some(text));
                }
            }
        }
        Ok(None)
    }

    let mut html_fallback = None;
    visit(payload, &mut html_fallback)?
        .or(html_fallback)
        .ok_or_else(|| {
            "Gmail message has no inline text body; attachment reads are refused".to_owned()
        })
}

fn gmail_payload_data(payload: &Value) -> Result<String, String> {
    let data = payload
        .pointer("/body/data")
        .and_then(Value::as_str)
        .ok_or_else(|| "Gmail message body is not inline text".to_owned())?;
    let bytes = URL_SAFE_NO_PAD
        .decode(data)
        .map_err(|_| "Gmail message body encoding is invalid".to_owned())?;
    String::from_utf8(bytes).map_err(|_| "Gmail message text is not UTF-8".to_owned())
}
