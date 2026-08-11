//! Read-only Microsoft Graph transport for Outlook on the web Inbox messages.
//!
//! This is the shipping Outlook-web route.  It deliberately does not share the
//! older page-marker or seeded-mailbox paths: a message body is obtained only
//! from Microsoft Graph's `GET /messages/{id}?$select=body` operation.  The
//! transport owns no mutation operation and retains no mailbox rows, so a
//! revoked credential cannot silently return a previous read.

use std::collections::BTreeSet;
use std::fmt;

use reqwest::blocking::Client;
use serde_json::Value;
use url::Url;
use zeroize::Zeroizing;

use crate::services::shared_mailbox_thread_name;
use crate::shared_mail_body_reader::{
    read_listed_mailbox_message_words, ListedMailboxMessage, ListedMailboxWordsProvider,
    ProviderMessageWords,
};

const GRAPH_ME_ROOT: &str = "https://graph.microsoft.com/v1.0/me/";
const OUTLOOK_INBOX_FOLDER: &str = "Inbox";

/// The one shipping Outlook-on-the-web reader.  Any old web-page route is not
/// a shipping reader and is intentionally excluded from this count.
pub const SHIPPING_OUTLOOK_WEB_READER_COUNT: usize = 1;

pub const fn shipping_outlook_web_reader_count() -> usize {
    SHIPPING_OUTLOOK_WEB_READER_COUNT
}

/// Identifiers for a production Outlook account. OAuth credentials remain in
/// [`MicrosoftGraphTransport`] and are not cached in this binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutlookWebMailboxBinding {
    pub account_id: String,
    pub own_address: String,
    pub other_address: String,
}

impl OutlookWebMailboxBinding {
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
            return Err("Outlook web shipping mailbox needs two different addresses".to_owned());
        }
        Ok(binding)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutlookWebInboxMessageMetadata {
    pub provider_message_id: String,
    pub provider_conversation_id: String,
    pub subject: String,
    pub sender: String,
    pub time: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutlookWebMessageText {
    pub provider_message_id: String,
    pub text: String,
    /// Graph's ETag is supplied by the service response for audit correlation.
    pub server_token: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutlookWebCoverMessage {
    pub provider_sender: String,
    pub time: i64,
    pub text: String,
    pub graph_message_id: String,
    pub conversation_id: String,
    pub thread_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutlookWebShippingRead {
    pub inbox: Vec<OutlookWebCoverMessage>,
}

/// A Microsoft sign-in challenge that must be completed by the account owner.
/// This is deliberately an outcome, not a browser-automation escape hatch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutlookGraphOAuthChallenge {
    Captcha,
    TwoFactor,
}

impl OutlookGraphOAuthChallenge {
    fn name(self) -> &'static str {
        match self {
            Self::Captcha => "CAPTCHA",
            Self::TwoFactor => "2FA",
        }
    }
}

/// Stop OAuth automation as soon as Microsoft requests a CAPTCHA or second
/// factor.  The caller must hand the login back to Liam; no credential,
/// challenge response, cookie, or cached message is substituted.
pub fn report_outlook_web_oauth_challenge(
    challenge: OutlookGraphOAuthChallenge,
) -> Result<(), String> {
    Err(format!(
        "Outlook web Graph OAuth requires Liam's login handoff for {}; automation stopped",
        challenge.name()
    ))
}

/// Read incoming messages from the other participant.  Missing, revoked, or
/// unavailable Graph transport fails closed with `Outlook web` named in the
/// error and never falls back to page marks, IMAP, or saved rows.
pub fn read_shipping_outlook_web_inbox(
    transport: Option<&mut MicrosoftGraphTransport>,
    binding: &OutlookWebMailboxBinding,
) -> Result<OutlookWebShippingRead, String> {
    let transport = transport.ok_or_else(|| {
        "Outlook web shipping Microsoft Graph transport is unavailable".to_owned()
    })?;
    let metadata = transport
        .list_inbox_metadata()
        .map_err(graph_transport_error)?;

    let mut seen = BTreeSet::new();
    let mut selected = Vec::new();
    for row in metadata {
        validate_metadata(&row)?;
        if !seen.insert(row.provider_message_id.clone()) {
            return Err("Outlook web Inbox returned a duplicate Graph message id".to_owned());
        }
        if sender_address(&row.sender)?.eq_ignore_ascii_case(&binding.other_address) {
            selected.push(row);
        }
    }

    let mut inbox = Vec::with_capacity(selected.len());
    for row in selected {
        let listed = ListedMailboxMessage {
            service_id: "outlook-web".to_owned(),
            account_id: binding.account_id.clone(),
            folder_id: OUTLOOK_INBOX_FOLDER.to_owned(),
            message_id: row.provider_message_id.clone(),
        };
        let mut body_provider = OutlookWebBodyProvider { transport };
        let words = read_listed_mailbox_message_words(&listed, &mut body_provider)
            .map_err(|error| format!("Outlook web shipping body read refused: {error}"))?;
        let thread_name =
            shared_mailbox_thread_name(&binding.own_address, &binding.other_address, &row.subject)
                .map_err(|error| format!("Outlook web shipping thread name refused: {error}"))?;
        inbox.push(OutlookWebCoverMessage {
            provider_sender: row.sender,
            time: row.time,
            text: words.words,
            graph_message_id: row.provider_message_id,
            conversation_id: row.provider_conversation_id,
            thread_name,
        });
    }
    inbox.sort_by(|left, right| {
        left.time
            .cmp(&right.time)
            .then_with(|| left.graph_message_id.cmp(&right.graph_message_id))
    });
    Ok(OutlookWebShippingRead { inbox })
}

struct OutlookWebBodyProvider<'a> {
    transport: &'a mut MicrosoftGraphTransport,
}

impl ListedMailboxWordsProvider for OutlookWebBodyProvider<'_> {
    fn provider_route(&self) -> &str {
        "outlook-web"
    }

    fn fetch_listed_message_words(
        &mut self,
        folder_id: &str,
        provider_message_id: &str,
    ) -> Result<ProviderMessageWords, String> {
        if folder_id != OUTLOOK_INBOX_FOLDER {
            return Err("Outlook web shipping reader only reads Inbox".to_owned());
        }
        let text = self
            .transport
            .fetch_message_body(provider_message_id)
            .map_err(graph_transport_error)?;
        Ok(ProviderMessageWords {
            provider_message_id: text.provider_message_id,
            words: text.text,
            server_token: text.server_token,
            attachment_bytes_fetched: 0,
        })
    }
}

/// Live Microsoft Graph transport. It holds a consented OAuth bearer token,
/// performs only Graph GET requests, and exposes no state-changing method.
pub struct MicrosoftGraphTransport {
    client: Client,
    api_root: Url,
    bearer_token: Zeroizing<String>,
}

impl fmt::Debug for MicrosoftGraphTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MicrosoftGraphTransport")
            .field("api_root", &self.api_root)
            .field("bearer_token", &"<redacted>")
            .finish()
    }
}

impl MicrosoftGraphTransport {
    pub fn from_oauth_bearer_token(bearer_token: impl Into<String>) -> Result<Self, String> {
        Self::with_api_root(bearer_token, GRAPH_ME_ROOT)
    }

    /// Public only for a production-compatible local Graph HTTP server in the
    /// integration test. It still requires a bearer credential and remains
    /// GET-only; application code uses `from_oauth_bearer_token`.
    pub fn with_api_root(bearer_token: impl Into<String>, api_root: &str) -> Result<Self, String> {
        let bearer_token = bearer_token.into();
        if bearer_token.trim().is_empty() || bearer_token != bearer_token.trim() {
            return Err("Outlook web Graph OAuth credential is missing or invalid".to_owned());
        }
        let mut api_root =
            Url::parse(api_root).map_err(|_| "Outlook web Graph API root is invalid".to_owned())?;
        if api_root.scheme() != "https" && api_root.host_str() != Some("127.0.0.1") {
            return Err("Outlook web Graph API root must use HTTPS".to_owned());
        }
        if !api_root.path().ends_with('/') {
            let path = format!("{}/", api_root.path());
            api_root.set_path(&path);
        }
        let client = Client::builder()
            .https_only(api_root.scheme() == "https")
            .build()
            .map_err(|error| format!("Outlook web Graph HTTP client setup failed: {error}"))?;
        Ok(Self {
            client,
            api_root,
            bearer_token: Zeroizing::new(bearer_token),
        })
    }

    fn get_json(&self, path: &str, query: &[(&str, &str)]) -> Result<(Value, String), String> {
        let url = self
            .api_root
            .join(path)
            .map_err(|_| "Outlook web Graph API path is invalid".to_owned())?;
        let response = self
            .client
            .get(url)
            .bearer_auth(self.bearer_token.as_str())
            // This means a selected Graph body is text, never page DOM/HTML.
            .header("Prefer", "outlook.body-content-type=\"text\"")
            .query(query)
            .send()
            .map_err(|error| format!("Outlook web Graph API request failed: {error}"))?;
        if !response.status().is_success() {
            return Err(format!(
                "Outlook web Graph credential or transport rejected read: HTTP {}",
                response.status()
            ));
        }
        let server_token = response
            .headers()
            .get("etag")
            .and_then(|value| value.to_str().ok())
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| "graph-response-without-etag".to_owned());
        let json = response
            .json::<Value>()
            .map_err(|error| format!("Outlook web Graph API returned invalid JSON: {error}"))?;
        Ok((json, server_token))
    }

    fn list_inbox_metadata(&mut self) -> Result<Vec<OutlookWebInboxMessageMetadata>, String> {
        let mut metadata = Vec::new();
        let mut next_path = Some("mailFolders/Inbox/messages".to_owned());
        let mut first_page = true;
        while let Some(path) = next_path.take() {
            let query = if first_page {
                vec![
                    ("$select", "id,conversationId,subject,from,receivedDateTime"),
                    ("$top", "100"),
                ]
            } else {
                Vec::new()
            };
            first_page = false;
            let (json, _) = self.get_json(&path, &query)?;
            let rows = json
                .get("value")
                .and_then(Value::as_array)
                .ok_or_else(|| "Outlook web Graph Inbox response lacks value".to_owned())?;
            for row in rows {
                metadata.push(graph_metadata(row)?);
            }
            next_path = json
                .get("@odata.nextLink")
                .and_then(Value::as_str)
                .map(graph_next_path)
                .transpose()?;
        }
        Ok(metadata)
    }

    fn fetch_message_body(
        &mut self,
        provider_message_id: &str,
    ) -> Result<OutlookWebMessageText, String> {
        validate_text(provider_message_id, "Graph message id", 180)?;
        // This is the only body route: Microsoft Graph GET /messages/{id}?$select=body.
        let (json, header_token) =
            self.get_json(&format!("messages/{provider_message_id}?$select=body"), &[])?;
        let returned_id = json_string(&json, "id")?;
        if returned_id != provider_message_id {
            return Err("Outlook web Graph returned a different message id".to_owned());
        }
        let content_type = json
            .pointer("/body/contentType")
            .and_then(Value::as_str)
            .ok_or_else(|| "Outlook web Graph message body lacks content type".to_owned())?;
        if !content_type.eq_ignore_ascii_case("text") {
            return Err("Outlook web Graph message body was not returned as text".to_owned());
        }
        let text = json
            .pointer("/body/content")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .ok_or_else(|| "Outlook web Graph message body lacks content".to_owned())?;
        let server_token = json
            .get("@odata.etag")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or(header_token);
        Ok(OutlookWebMessageText {
            provider_message_id: returned_id,
            text,
            server_token,
        })
    }
}

fn graph_transport_error(error: String) -> String {
    if error.contains("Outlook web") {
        error
    } else {
        format!("Outlook web shipping Graph transport failed: {error}")
    }
}

fn graph_metadata(json: &Value) -> Result<OutlookWebInboxMessageMetadata, String> {
    let sender_name = json
        .pointer("/from/emailAddress/name")
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty());
    let sender_address = json
        .pointer("/from/emailAddress/address")
        .and_then(Value::as_str)
        .ok_or_else(|| "Outlook web Graph message lacks sender address".to_owned())?;
    validate_email(sender_address, "provider sender")?;
    let sender = sender_name
        .map(|name| format!("{name} <{sender_address}>"))
        .unwrap_or_else(|| sender_address.to_owned());
    Ok(OutlookWebInboxMessageMetadata {
        provider_message_id: json_string(json, "id")?,
        provider_conversation_id: json_string(json, "conversationId")?,
        subject: json_string(json, "subject")?,
        sender,
        time: graph_datetime_unix(json_string(json, "receivedDateTime")?.as_str())?,
    })
}

fn graph_next_path(next_link: &str) -> Result<String, String> {
    let url = Url::parse(next_link)
        .map_err(|_| "Outlook web Graph next page link is invalid".to_owned())?;
    if url.scheme() != "https" && url.host_str() != Some("127.0.0.1") {
        return Err("Outlook web Graph next page link must use HTTPS".to_owned());
    }
    // `Url::join` keeps an absolute URL absolute, so a Graph-issued nextLink
    // reaches precisely the page Graph authorized rather than being joined
    // underneath the current `/me/` prefix a second time.
    Ok(url.into())
}

fn graph_datetime_unix(value: &str) -> Result<i64, String> {
    let value = value
        .strip_suffix('Z')
        .ok_or_else(|| "Outlook web Graph receivedDateTime must be UTC RFC3339".to_owned())?;
    let (date, time) = value
        .split_once('T')
        .ok_or_else(|| "Outlook web Graph receivedDateTime is invalid".to_owned())?;
    let mut date_parts = date.split('-');
    let year = parse_number(date_parts.next(), "receivedDateTime")?;
    let month = parse_number(date_parts.next(), "receivedDateTime")?;
    let day = parse_number(date_parts.next(), "receivedDateTime")?;
    if date_parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err("Outlook web Graph receivedDateTime is invalid".to_owned());
    }
    let time = time.split('.').next().unwrap_or(time);
    let mut time_parts = time.split(':');
    let hour = parse_number(time_parts.next(), "receivedDateTime")?;
    let minute = parse_number(time_parts.next(), "receivedDateTime")?;
    let second = parse_number(time_parts.next(), "receivedDateTime")?;
    if time_parts.next().is_some() || hour > 23 || minute > 59 || second > 59 {
        return Err("Outlook web Graph receivedDateTime is invalid".to_owned());
    }
    // Howard Hinnant's civil-date conversion, with 1970-01-01 as day zero.
    let adjusted_year = year - if month <= 2 { 1 } else { 0 };
    let era = if adjusted_year >= 0 {
        adjusted_year
    } else {
        adjusted_year - 399
    } / 400;
    let yoe = adjusted_year - era * 400;
    let month_prime = i64::from(month) + if month > 2 { -3 } else { 9 };
    let doy = (153 * month_prime + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Ok(days * 86_400 + hour * 3_600 + minute * 60 + second)
}

fn parse_number(value: Option<&str>, label: &str) -> Result<i64, String> {
    value
        .filter(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        .ok_or_else(|| format!("Outlook web Graph {label} is invalid"))?
        .parse::<i64>()
        .map_err(|_| format!("Outlook web Graph {label} is invalid"))
}

fn validate_metadata(row: &OutlookWebInboxMessageMetadata) -> Result<(), String> {
    validate_text(&row.provider_message_id, "Graph message id", 180)?;
    validate_text(&row.provider_conversation_id, "Graph conversation id", 180)?;
    validate_text(&row.subject, "message subject", 512)?;
    validate_text(&row.sender, "provider sender", 254)?;
    validate_email(&sender_address(&row.sender)?, "provider sender")?;
    if row.time <= 0 {
        return Err("Outlook web Graph provider time is invalid".to_owned());
    }
    Ok(())
}

fn validate_text(value: &str, label: &str, limit: usize) -> Result<(), String> {
    if value.is_empty()
        || value.trim() != value
        || value.len() > limit
        || value.chars().any(char::is_control)
    {
        return Err(format!("Outlook web {label} is invalid"));
    }
    Ok(())
}

fn validate_email(value: &str, label: &str) -> Result<(), String> {
    validate_text(value, label, 254)?;
    let (local, domain) = value
        .rsplit_once('@')
        .ok_or_else(|| format!("Outlook web {label} is invalid"))?;
    if local.is_empty()
        || domain.is_empty()
        || !domain.contains('.')
        || value.matches('@').count() != 1
    {
        return Err(format!("Outlook web {label} is invalid"));
    }
    Ok(())
}

fn sender_address(value: &str) -> Result<String, String> {
    let value = value.trim();
    let address = match (value.rfind('<'), value.rfind('>')) {
        (Some(start), Some(end)) if start < end && end == value.len() - 1 => &value[start + 1..end],
        (None, None) => value,
        _ => return Err("Outlook web provider sender is invalid".to_owned()),
    };
    validate_email(address.trim(), "provider sender")?;
    Ok(address.trim().to_ascii_lowercase())
}

fn json_string(json: &Value, field: &str) -> Result<String, String> {
    json.get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("Outlook web Graph response lacks {field}"))
}

#[cfg(test)]
mod tests {
    use super::graph_datetime_unix;

    #[test]
    fn graph_datetime_uses_utc_rfc3339() {
        assert_eq!(
            graph_datetime_unix("2026-08-11T00:00:00Z"),
            Ok(1_786_406_400)
        );
        assert!(graph_datetime_unix("2026-08-11T00:00:00+00:00").is_err());
    }
}
