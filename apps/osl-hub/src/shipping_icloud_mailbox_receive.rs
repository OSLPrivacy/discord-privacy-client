//! Read-only iCloud Inbox reader. Apple two-factor login is deliberately not
//! automated: its caller supplies an owner-created app-specific password.

use crate::services::shared_mailbox_thread_name;
use rustls::{pki_types::ServerName, ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use std::{collections::BTreeSet, fmt, net::TcpStream, sync::Arc};
use zeroize::Zeroizing;

pub const ICLOUD_IMAP_HOST: &str = "imap.mail.me.com";
pub const ICLOUD_IMAP_PORT: u16 = 993;
pub const SHIPPING_ICLOUD_READER_COUNT: usize = 1;
pub const fn shipping_icloud_reader_count() -> usize {
    SHIPPING_ICLOUD_READER_COUNT
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IcloudMailboxBinding {
    pub account_id: String,
    pub own_address: String,
    pub other_address: String,
}
impl IcloudMailboxBinding {
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
        text(&binding.account_id, "account id", 256)?;
        email(&binding.own_address, "own address")?;
        email(&binding.other_address, "other address")?;
        if binding
            .own_address
            .eq_ignore_ascii_case(&binding.other_address)
        {
            return Err("iCloud shipping mailbox needs two different addresses".into());
        }
        Ok(binding)
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IcloudInboxMetadata {
    pub provider_uid: u32,
    pub subject: String,
    pub sender: String,
    pub time: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IcloudCoverMessage {
    pub provider_sender: String,
    pub time: i64,
    pub text: String,
    pub provider_uid: u32,
    pub thread_name: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IcloudShippingRead {
    pub inbox: Vec<IcloudCoverMessage>,
}

/// Read-only provider seam. It has no cache or mutation method; fixtures are
/// supplied only by tests, while [`IcloudImapTransport`] is the shipping route.
pub trait IcloudInboxTransport {
    fn list_inbox(&mut self) -> Result<Vec<IcloudInboxMetadata>, String>;
    fn fetch_inbox_text(&mut self, provider_uid: u32) -> Result<String, String>;
}

pub fn read_shipping_icloud_inbox(
    transport: Option<&mut dyn IcloudInboxTransport>,
    binding: &IcloudMailboxBinding,
) -> Result<IcloudShippingRead, String> {
    let transport =
        transport.ok_or_else(|| "iCloud shipping transport is unavailable".to_owned())?;
    let mut seen = BTreeSet::new();
    let mut inbox = Vec::new();
    for row in transport.list_inbox().map_err(icloud_error)? {
        validate_row(&row)?;
        if !seen.insert(row.provider_uid) {
            return Err("iCloud Inbox returned a duplicate provider UID".into());
        }
        if sender_address(&row.sender)?.eq_ignore_ascii_case(&binding.other_address) {
            let body = transport
                .fetch_inbox_text(row.provider_uid)
                .map_err(icloud_error)?;
            if body.len() > 256 * 1024
                || body
                    .chars()
                    .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
            {
                return Err("iCloud provider message text is invalid".into());
            }
            let thread_name = shared_mailbox_thread_name(
                &binding.own_address,
                &binding.other_address,
                &row.subject,
            )
            .map_err(|e| format!("iCloud shipping thread name refused: {e}"))?;
            inbox.push(IcloudCoverMessage {
                provider_sender: row.sender,
                time: row.time,
                text: body,
                provider_uid: row.provider_uid,
                thread_name,
            });
        }
    }
    inbox.sort_by(|a, b| {
        a.time
            .cmp(&b.time)
            .then_with(|| a.provider_uid.cmp(&b.provider_uid))
    });
    Ok(IcloudShippingRead { inbox })
}

/// Concrete implicit-TLS IMAP transport for Apple's shipping endpoint.
pub struct IcloudImapTransport {
    username: String,
    password: Zeroizing<String>,
}
impl fmt::Debug for IcloudImapTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IcloudImapTransport")
            .field("username", &"<redacted>")
            .field("password", &"<redacted>")
            .finish()
    }
}
impl IcloudImapTransport {
    pub fn with_app_specific_password(
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Result<Self, String> {
        let username = username.into();
        let password = password.into();
        email(&username, "app-password username")?;
        if password.trim().is_empty()
            || password != password.trim()
            || password.len() > 4096
            || password.chars().any(char::is_control)
        {
            return Err("iCloud app-specific password is missing or invalid".into());
        }
        Ok(Self {
            username,
            password: Zeroizing::new(password),
        })
    }
    fn session(&self) -> Result<imap::Session<StreamOwned<ClientConnection, TcpStream>>, String> {
        let roots = RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        let config = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let name = ServerName::try_from(ICLOUD_IMAP_HOST.to_owned())
            .map_err(|_| "iCloud IMAP hostname is invalid".to_owned())?;
        let connection = ClientConnection::new(Arc::new(config), name)
            .map_err(|e| format!("iCloud TLS setup failed: {e}"))?;
        let socket = TcpStream::connect((ICLOUD_IMAP_HOST, ICLOUD_IMAP_PORT))
            .map_err(|e| format!("iCloud IMAP connection failed: {e}"))?;
        let mut client = imap::Client::new(StreamOwned::new(connection, socket));
        client
            .read_greeting()
            .map_err(|e| format!("iCloud IMAP greeting failed: {e}"))?;
        client
            .login(&self.username, self.password.as_str())
            .map_err(|(e, _)| format!("iCloud IMAP authentication failed: {e}"))
    }
}
impl IcloudInboxTransport for IcloudImapTransport {
    fn list_inbox(&mut self) -> Result<Vec<IcloudInboxMetadata>, String> {
        let mut session = self.session()?;
        session
            .examine("INBOX")
            .map_err(|e| format!("iCloud IMAP Inbox examine failed: {e}"))?;
        let fetched = session
            .uid_fetch(
                "1:*",
                "(UID INTERNALDATE BODY.PEEK[HEADER.FIELDS (FROM SUBJECT)])",
            )
            .map_err(|e| format!("iCloud IMAP Inbox list failed: {e}"))?;
        let mut rows = Vec::new();
        for fetch in &fetched {
            let uid = fetch
                .uid
                .ok_or_else(|| "iCloud IMAP Inbox row lacks UID".to_owned())?;
            let time = fetch
                .internal_date()
                .ok_or_else(|| "iCloud IMAP Inbox row lacks INTERNALDATE".to_owned())?
                .timestamp();
            let bytes = fetch
                .header()
                .ok_or_else(|| "iCloud IMAP Inbox row lacks headers".to_owned())?;
            let headers = std::str::from_utf8(bytes)
                .map_err(|_| "iCloud IMAP headers are not UTF-8".to_owned())?;
            rows.push(IcloudInboxMetadata {
                provider_uid: uid,
                sender: header(headers, "From")?,
                subject: header(headers, "Subject")?,
                time,
            });
        }
        let _ = session.logout();
        Ok(rows)
    }
    fn fetch_inbox_text(&mut self, uid: u32) -> Result<String, String> {
        let mut session = self.session()?;
        session
            .examine("INBOX")
            .map_err(|e| format!("iCloud IMAP Inbox examine failed: {e}"))?;
        let fetched = session
            .uid_fetch(uid.to_string(), "BODY.PEEK[TEXT]")
            .map_err(|e| format!("iCloud IMAP message read failed: {e}"))?;
        let bytes = fetched
            .iter()
            .next()
            .and_then(|row| row.text())
            .ok_or_else(|| "iCloud IMAP message text is missing".to_owned())?;
        let text = String::from_utf8(bytes.to_vec())
            .map_err(|_| "iCloud IMAP message text is not UTF-8".to_owned())?;
        let _ = session.logout();
        Ok(text)
    }
}
fn icloud_error(error: String) -> String {
    if error.contains("iCloud") {
        error
    } else {
        format!("iCloud shipping transport failed: {error}")
    }
}
fn text(value: &str, label: &str, max: usize) -> Result<(), String> {
    if value.is_empty()
        || value.trim() != value
        || value.len() > max
        || value.chars().any(char::is_control)
    {
        Err(format!("iCloud {label} is invalid"))
    } else {
        Ok(())
    }
}
fn email(value: &str, label: &str) -> Result<(), String> {
    text(value, label, 254)?;
    let (local, domain) = value
        .rsplit_once('@')
        .ok_or_else(|| format!("iCloud {label} is invalid"))?;
    if local.is_empty()
        || domain.is_empty()
        || !domain.contains('.')
        || value.matches('@').count() != 1
    {
        Err(format!("iCloud {label} is invalid"))
    } else {
        Ok(())
    }
}
fn sender_address(value: &str) -> Result<String, String> {
    let value = value.trim();
    let address = match (value.rfind('<'), value.rfind('>')) {
        (Some(a), Some(b)) if a < b && b == value.len() - 1 => &value[a + 1..b],
        (None, None) => value,
        _ => return Err("iCloud provider sender is invalid".into()),
    };
    email(address.trim(), "provider sender")?;
    Ok(address.trim().to_ascii_lowercase())
}
fn validate_row(row: &IcloudInboxMetadata) -> Result<(), String> {
    if row.provider_uid == 0 || row.time <= 0 {
        return Err("iCloud provider Inbox row is invalid".into());
    }
    text(&row.subject, "message subject", 512)?;
    text(&row.sender, "provider sender", 254)?;
    email(&sender_address(&row.sender)?, "provider sender")
}
fn header(headers: &str, wanted: &str) -> Result<String, String> {
    let h = headers.replace("\r\n\t", " ").replace("\r\n ", " ");
    h.lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(name, _)| name.eq_ignore_ascii_case(wanted))
                .map(|(_, value)| value.trim().to_owned())
        })
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("iCloud IMAP headers lack {wanted}"))
}
