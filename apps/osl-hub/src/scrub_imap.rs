//! Narrow, deletion-only IMAP transport for AutoScrub.
//!
//! Secrets are accepted only by the configuration command, verified against
//! the server, and then written to the operating-system credential store.
//! Runtime operations receive an account id and a fresh authentication epoch;
//! they never accept or return credentials.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use zeroize::{Zeroize, Zeroizing};

const KEYRING_SERVICE: &str = "org.open-source-labs.hub.scrub-imap";
const DEFAULT_IMAP_PORT: u16 = 993;
const COMMAND_INTERVAL: Duration = Duration::from_secs(1);
const AUTH_EPOCH_LIFETIME: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ImapAuthKind {
    AppPassword,
    OauthBearer,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigureImapRequest {
    pub account_id: String,
    pub host: String,
    pub port: Option<u16>,
    pub username: String,
    pub auth: ImapAuthInput,
    pub default_mailbox: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImapAuthInput {
    pub kind: ImapAuthKind,
    pub secret: String,
}

impl Drop for ImapAuthInput {
    fn drop(&mut self) {
        self.secret.zeroize();
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImapAccountRequest {
    pub account_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImapItemRequest {
    pub account_id: String,
    pub expected_auth_epoch: String,
    pub mailbox: String,
    pub message_id: String,
    pub since_date_unix_ms: Option<u64>,
    /// Required for delete, ignored by enumerate/inspect/verify.
    pub expected_content_fingerprint: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImapCapability {
    pub configured: bool,
    pub live_confirmed: bool,
    pub auth_epoch: Option<String>,
    pub detail: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImapEnumeration {
    pub findings: Vec<ImapTransportFinding>,
    pub auth_epoch: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImapTransportFinding {
    pub uid: u32,
    pub mailbox: String,
    pub message_id: String,
    pub authored_by_self: bool,
    pub content_fingerprint: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImapInspection {
    pub state: ImapPresence,
    pub authored_by_self: bool,
    pub content_fingerprint: Option<String>,
    pub auth_epoch: String,
    pub schema_version: &'static str,
    pub retractable: bool,
    pub detail: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ImapPresence {
    Present,
    Absent,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImapDeleteResult {
    pub accepted: bool,
    pub auth_epoch: String,
    pub detail: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImapVerification {
    pub outcome: ImapVerificationOutcome,
    pub auth_epoch: String,
    pub detail: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub enum ImapVerificationOutcome {
    #[serde(rename = "confirmed-deleted")]
    ConfirmedDeleted,
    #[serde(rename = "confirmed-not-deleted")]
    ConfirmedNotDeleted,
    #[serde(rename = "UNKNOWN")]
    Unknown,
}

/// Read-only metadata surfaced by the local IMAP live-test harness.
pub struct ImapLiveMessage {
    pub uid: u32,
    pub internal_date: String,
    pub from: String,
    pub subject: String,
    pub message_id: Option<String>,
    pub authored_by_self: bool,
    pub content_fingerprint: Option<String>,
    identity_headers: Vec<u8>,
}

/// A single inspected item that is ready for the harness to preview.
pub struct ImapLivePreparedDelete {
    pub uid: u32,
    pub message_id: String,
    request: ImapItemRequest,
}

pub struct ImapLiveDeleteExecution {
    pub delete_result: Result<ImapDeleteResult, String>,
    pub verification: ImapVerification,
}

/// Credential-in-memory facade used only by the local live-test binary.
///
/// It delegates connection, authentication, inspection, deletion, scoped
/// expunge, and readback to the production transport implementation. It never
/// persists the supplied app password.
pub struct ImapLiveTestTransport {
    state: ScrubImapState,
    account_id: String,
    auth_epoch: String,
    secret: Zeroizing<String>,
}

#[derive(Clone)]
struct AccountConfig {
    host: String,
    port: u16,
    username: String,
    auth_kind: ImapAuthKind,
    default_mailbox: String,
    auth_epoch: String,
    auth_issued_at: Instant,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredAccount {
    host: String,
    port: u16,
    username: String,
    auth_kind: ImapAuthKind,
    default_mailbox: String,
    secret: String,
}

impl Drop for StoredAccount {
    fn drop(&mut self) {
        self.secret.zeroize();
    }
}

#[derive(Default)]
pub struct ScrubImapState {
    // Intentionally process-local. A restart always clears live confirmation.
    accounts: Mutex<HashMap<String, AccountConfig>>,
}

trait MailboxSession {
    fn select(&mut self, mailbox: &str) -> Result<(), TransportFailure>;
    fn search(
        &mut self,
        message_id: &str,
        since_date: Option<&str>,
    ) -> Result<Vec<u32>, TransportFailure>;
    fn fetch_identity_headers(&mut self, uid: u32) -> Result<Vec<u8>, TransportFailure>;
    fn supports_uid_expunge(&mut self) -> Result<bool, TransportFailure>;
    fn mark_deleted(&mut self, uid: u32) -> Result<(), TransportFailure>;
    fn expunge_uid(&mut self, uid: u32) -> Result<(), TransportFailure>;
    fn recent_messages(&mut self, _limit: usize) -> Result<Vec<ImapLiveMessage>, TransportFailure> {
        Err(TransportFailure::Ambiguous)
    }
}

trait SessionConnector {
    fn connect(
        &self,
        config: &AccountConfig,
        secret: &str,
    ) -> Result<Box<dyn MailboxSession>, TransportFailure>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransportFailure {
    Authentication,
    RateLimited,
    TlsOrConnection,
    Server,
    Ambiguous,
}

impl TransportFailure {
    fn public_detail(self) -> &'static str {
        match self {
            Self::Authentication => "IMAP authentication changed or expired",
            Self::RateLimited => "IMAP server rate-limited the request; no automatic retry",
            Self::TlsOrConnection => "IMAP TLS connection was unavailable",
            Self::Server => "IMAP server rejected or could not complete the request",
            Self::Ambiguous => "IMAP readback was ambiguous",
        }
    }
}

struct RealConnector;

struct OAuthBearer<'a> {
    username: &'a str,
    token: &'a str,
}

impl imap::Authenticator for OAuthBearer<'_> {
    type Response = Vec<u8>;

    fn process(&self, _challenge: &[u8]) -> Self::Response {
        format!(
            "user={}\u{1}auth=Bearer {}\u{1}\u{1}",
            self.username, self.token
        )
        .into_bytes()
    }
}

impl SessionConnector for RealConnector {
    fn connect(
        &self,
        config: &AccountConfig,
        secret: &str,
    ) -> Result<Box<dyn MailboxSession>, TransportFailure> {
        let client = imap::ClientBuilder::new(&config.host, config.port)
            .connect()
            .map_err(classify_imap_error)?;
        let session = match config.auth_kind {
            ImapAuthKind::AppPassword => client
                .login(&config.username, secret)
                .map_err(|(error, _)| classify_imap_error(error))?,
            ImapAuthKind::OauthBearer => client
                .authenticate(
                    "XOAUTH2",
                    &OAuthBearer {
                        username: &config.username,
                        token: secret,
                    },
                )
                .map_err(|(error, _)| classify_imap_error(error))?,
        };
        Ok(Box::new(RealMailbox {
            session,
            // Authentication is itself a server command. Pace SELECT and all
            // subsequent commands from it instead of bursting after login.
            last_command: Some(Instant::now()),
        }))
    }
}

struct RealMailbox {
    session: imap::Session<imap::Connection>,
    last_command: Option<Instant>,
}

impl RealMailbox {
    fn pace(&mut self) {
        if let Some(last) = self.last_command {
            let elapsed = last.elapsed();
            if elapsed < COMMAND_INTERVAL {
                std::thread::sleep(COMMAND_INTERVAL - elapsed);
            }
        }
        self.last_command = Some(Instant::now());
    }
}

impl MailboxSession for RealMailbox {
    fn select(&mut self, mailbox: &str) -> Result<(), TransportFailure> {
        validate_mailbox(mailbox)?;
        self.pace();
        self.session
            .select(mailbox)
            .map(|_| ())
            .map_err(classify_imap_error)
    }

    fn search(
        &mut self,
        message_id: &str,
        since_date: Option<&str>,
    ) -> Result<Vec<u32>, TransportFailure> {
        validate_message_id(message_id)?;
        if let Some(date) = since_date {
            validate_imap_date(date)?;
        }
        let query = match since_date {
            Some(date) => format!("HEADER Message-ID \"{message_id}\" SINCE {date}"),
            None => format!("HEADER Message-ID \"{message_id}\""),
        };
        self.pace();
        let mut uids: Vec<u32> = self
            .session
            .uid_search(query)
            .map_err(classify_imap_error)?
            .into_iter()
            .collect();
        uids.sort_unstable();
        Ok(uids)
    }

    fn mark_deleted(&mut self, uid: u32) -> Result<(), TransportFailure> {
        self.pace();
        self.session
            .uid_store(uid.to_string(), "+FLAGS.SILENT (\\Deleted)")
            .map(|_| ())
            .map_err(classify_imap_error)
    }

    fn fetch_identity_headers(&mut self, uid: u32) -> Result<Vec<u8>, TransportFailure> {
        self.pace();
        let fetches = self
            .session
            .uid_fetch(
                uid.to_string(),
                "BODY.PEEK[HEADER.FIELDS (MESSAGE-ID FROM)]",
            )
            .map_err(classify_imap_error)?;
        let fetch = fetches.iter().next().ok_or(TransportFailure::Ambiguous)?;
        fetch
            .body()
            .map(ToOwned::to_owned)
            .ok_or(TransportFailure::Ambiguous)
    }

    fn supports_uid_expunge(&mut self) -> Result<bool, TransportFailure> {
        self.pace();
        self.session
            .capabilities()
            .map(|capabilities| capabilities.has_str("UIDPLUS"))
            .map_err(classify_imap_error)
    }

    fn expunge_uid(&mut self, uid: u32) -> Result<(), TransportFailure> {
        self.pace();
        self.session
            .uid_expunge(uid.to_string())
            .map(|_| ())
            .map_err(classify_imap_error)
    }

    fn recent_messages(&mut self, limit: usize) -> Result<Vec<ImapLiveMessage>, TransportFailure> {
        self.pace();
        let mut uids: Vec<u32> = self
            .session
            .uid_search("ALL")
            .map_err(classify_imap_error)?
            .into_iter()
            .collect();
        uids.sort_unstable_by(|left, right| right.cmp(left));
        uids.truncate(limit);
        if uids.is_empty() {
            return Ok(Vec::new());
        }

        let sequence = uids
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        self.pace();
        let fetches = self
            .session
            .uid_fetch(
                sequence,
                "(UID INTERNALDATE BODY.PEEK[HEADER.FIELDS (MESSAGE-ID FROM SUBJECT)])",
            )
            .map_err(classify_imap_error)?;
        let mut messages = Vec::with_capacity(fetches.len());
        for fetch in fetches.iter() {
            let uid = fetch.uid.ok_or(TransportFailure::Ambiguous)?;
            let headers = fetch
                .body()
                .map(ToOwned::to_owned)
                .ok_or(TransportFailure::Ambiguous)?;
            use mailparse::MailHeaderMap;
            let (parsed, _) =
                mailparse::parse_headers(&headers).map_err(|_| TransportFailure::Ambiguous)?;
            messages.push(ImapLiveMessage {
                uid,
                internal_date: fetch
                    .internal_date()
                    .map(|date| date.to_rfc3339())
                    .unwrap_or_else(|| "unknown".to_owned()),
                from: parsed
                    .get_first_value("From")
                    .unwrap_or_else(|| "unknown".to_owned()),
                subject: parsed
                    .get_first_value("Subject")
                    .unwrap_or_else(|| "(no subject)".to_owned()),
                message_id: parsed
                    .get_first_value("Message-ID")
                    .map(|value| value.trim().to_owned())
                    .filter(|value| !value.is_empty()),
                authored_by_self: false,
                content_fingerprint: None,
                identity_headers: headers,
            });
        }
        messages.sort_unstable_by(|left, right| right.uid.cmp(&left.uid));
        Ok(messages)
    }
}

fn classify_imap_error(error: imap::Error) -> TransportFailure {
    // Do not return the provider's text to IPC: it can contain identifiers or
    // authentication details. Classification only controls fail-closed UI copy.
    let lower = error.to_string().to_ascii_lowercase();
    if lower.contains("auth") || lower.contains("login") || lower.contains("credential") {
        TransportFailure::Authentication
    } else if lower.contains("rate") || lower.contains("too many") || lower.contains("try again") {
        TransportFailure::RateLimited
    } else if lower.contains("tls")
        || lower.contains("certificate")
        || lower.contains("connection")
        || lower.contains("timed out")
    {
        TransportFailure::TlsOrConnection
    } else {
        TransportFailure::Server
    }
}

fn validate_account_field(value: &str) -> Result<(), TransportFailure> {
    if value.is_empty() || value.len() > 256 || value.chars().any(|c| c.is_control()) {
        return Err(TransportFailure::Ambiguous);
    }
    Ok(())
}

fn validate_host(host: &str) -> Result<(), TransportFailure> {
    validate_account_field(host)?;
    if host.len() > 253
        || !host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
    {
        return Err(TransportFailure::Ambiguous);
    }
    Ok(())
}

fn validate_mailbox(mailbox: &str) -> Result<(), TransportFailure> {
    validate_account_field(mailbox)
}

fn validate_message_id(message_id: &str) -> Result<(), TransportFailure> {
    validate_account_field(message_id)?;
    if message_id.contains(['"', '\\']) {
        return Err(TransportFailure::Ambiguous);
    }
    Ok(())
}

fn validate_imap_date(date: &str) -> Result<(), TransportFailure> {
    let bytes = date.as_bytes();
    let valid = (bytes.len() == 10 || bytes.len() == 11)
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'-')
        && bytes.iter().filter(|b| **b == b'-').count() == 2;
    if !valid {
        return Err(TransportFailure::Ambiguous);
    }
    Ok(())
}

fn request_since_date(request: &ImapItemRequest) -> Result<Option<String>, TransportFailure> {
    let Some(unix_ms) = request.since_date_unix_ms else {
        return Ok(None);
    };
    let seconds = i64::try_from(unix_ms / 1_000).map_err(|_| TransportFailure::Ambiguous)?;
    let date = chrono::DateTime::from_timestamp(seconds, 0)
        .ok_or(TransportFailure::Ambiguous)?
        .format("%d-%b-%Y")
        .to_string();
    validate_imap_date(&date)?;
    Ok(Some(date))
}

fn keyring_entry(account_id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, account_id)
        .map_err(|_| "OS credential storage is unavailable".to_owned())
}

fn load_stored_account(account_id: &str) -> Result<StoredAccount, String> {
    let serialized = Zeroizing::new(
        keyring_entry(account_id)?
            .get_password()
            .map_err(|_| "Stored IMAP authentication is unavailable".to_owned())?,
    );
    serde_json::from_str(serialized.as_str())
        .map_err(|_| "Stored IMAP account configuration is invalid".to_owned())
}

fn next_auth_epoch(account_id: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut digest = Sha256::new();
    digest.update(account_id.as_bytes());
    digest.update(nanos.to_le_bytes());
    digest.update(count.to_le_bytes());
    format!("imap-{:x}", digest.finalize())
}

fn finding_from_headers(
    config: &AccountConfig,
    mailbox: &str,
    uid: u32,
    requested_message_id: &str,
    headers: &[u8],
) -> Result<ImapTransportFinding, TransportFailure> {
    use mailparse::MailHeaderMap;

    let (parsed, _) = mailparse::parse_headers(headers).map_err(|_| TransportFailure::Ambiguous)?;
    let message_id = parsed
        .get_first_value("Message-ID")
        .ok_or(TransportFailure::Ambiguous)?;
    if message_id.trim() != requested_message_id.trim() {
        return Err(TransportFailure::Ambiguous);
    }
    let authored_by_self = parsed
        .get_first_value("From")
        .and_then(|from| mailparse::addrparse(&from).ok())
        .map(|addresses| {
            addresses.iter().any(|address| match address {
                mailparse::MailAddr::Single(info) => {
                    info.addr.eq_ignore_ascii_case(&config.username)
                }
                mailparse::MailAddr::Group(group) => group
                    .addrs
                    .iter()
                    .any(|info| info.addr.eq_ignore_ascii_case(&config.username)),
            })
        })
        .unwrap_or(false);
    let mut digest = Sha256::new();
    digest.update(b"osl-imap-finding-v1\0");
    digest.update(mailbox.as_bytes());
    digest.update([0]);
    digest.update(uid.to_le_bytes());
    digest.update(message_id.trim().as_bytes());
    digest.update([0]);
    digest.update(headers);
    Ok(ImapTransportFinding {
        uid,
        mailbox: mailbox.to_owned(),
        message_id: message_id.trim().to_owned(),
        authored_by_self,
        content_fingerprint: format!("sha256:{:x}", digest.finalize()),
    })
}

impl ScrubImapState {
    fn configure_with(
        &self,
        mut request: ConfigureImapRequest,
        connector: &dyn SessionConnector,
        persist_secret: bool,
    ) -> Result<ImapCapability, String> {
        validate_account_field(&request.account_id).map_err(|e| e.public_detail())?;
        validate_host(&request.host).map_err(|e| e.public_detail())?;
        validate_account_field(&request.username).map_err(|e| e.public_detail())?;
        let mailbox = request
            .default_mailbox
            .unwrap_or_else(|| "INBOX".to_owned());
        validate_mailbox(&mailbox).map_err(|e| e.public_detail())?;
        if request.auth.secret.is_empty()
            || request.auth.secret.len() > 16_384
            || request.auth.secret.chars().any(|c| c.is_control())
        {
            return Err("IMAP authentication secret is invalid".to_owned());
        }

        let secret = Zeroizing::new(std::mem::take(&mut request.auth.secret));
        let mut config = AccountConfig {
            host: request.host,
            port: request.port.unwrap_or(DEFAULT_IMAP_PORT),
            username: request.username,
            auth_kind: request.auth.kind,
            default_mailbox: mailbox,
            auth_epoch: String::new(),
            auth_issued_at: Instant::now(),
        };
        let mut session = connector
            .connect(&config, secret.as_str())
            .map_err(|e| e.public_detail().to_owned())?;
        session
            .select(&config.default_mailbox)
            .map_err(|e| e.public_detail().to_owned())?;

        if persist_secret {
            let entry = keyring_entry(&request.account_id)?;
            let stored_account = StoredAccount {
                host: config.host.clone(),
                port: config.port,
                username: config.username.clone(),
                auth_kind: config.auth_kind,
                default_mailbox: config.default_mailbox.clone(),
                secret: secret.to_string(),
            };
            let serialized = Zeroizing::new(
                serde_json::to_string(&stored_account)
                    .map_err(|_| "IMAP account configuration could not be protected".to_owned())?,
            );
            entry
                .set_password(serialized.as_str())
                .map_err(|_| "OS credential storage rejected IMAP authentication".to_owned())?;
            let stored = Zeroizing::new(
                entry
                    .get_password()
                    .map_err(|_| "OS credential storage readback failed".to_owned())?,
            );
            if stored.as_str() != serialized.as_str() {
                let _ = entry.delete_credential();
                return Err("OS credential storage readback failed".to_owned());
            }
        }

        config.auth_epoch = next_auth_epoch(&request.account_id);
        config.auth_issued_at = Instant::now();
        let response = ImapCapability {
            configured: true,
            live_confirmed: true,
            auth_epoch: Some(config.auth_epoch.clone()),
            detail: "TLS, authentication, and mailbox selection confirmed in this session"
                .to_owned(),
        };
        self.accounts
            .lock()
            .map_err(|_| "IMAP account state is unavailable".to_owned())?
            .insert(request.account_id, config);
        Ok(response)
    }

    fn config_for_epoch(
        &self,
        account_id: &str,
        expected_auth_epoch: &str,
    ) -> Result<AccountConfig, String> {
        let accounts = self
            .accounts
            .lock()
            .map_err(|_| "IMAP account state is unavailable".to_owned())?;
        let config = accounts
            .get(account_id)
            .ok_or_else(|| "IMAP transport is not live-confirmed in this session".to_owned())?;
        if config.auth_epoch != expected_auth_epoch
            || config.auth_issued_at.elapsed() > AUTH_EPOCH_LIFETIME
        {
            return Err("Fresh IMAP re-authentication is required".to_owned());
        }
        Ok(config.clone())
    }

    fn connect_for_item(
        &self,
        request: &ImapItemRequest,
        connector: &dyn SessionConnector,
        secret_override: Option<&str>,
    ) -> Result<(AccountConfig, Box<dyn MailboxSession>), String> {
        validate_mailbox(&request.mailbox).map_err(|e| e.public_detail())?;
        validate_message_id(&request.message_id).map_err(|e| e.public_detail())?;
        request_since_date(request).map_err(|e| e.public_detail())?;
        let config = self.config_for_epoch(&request.account_id, &request.expected_auth_epoch)?;
        let stored;
        let secret = match secret_override {
            Some(value) => value,
            None => {
                let account = load_stored_account(&request.account_id)?;
                stored = Zeroizing::new(account.secret.clone());
                stored.as_str()
            }
        };
        let mut session = connector
            .connect(&config, secret)
            .map_err(|e| e.public_detail().to_owned())?;
        session
            .select(&request.mailbox)
            .map_err(|e| e.public_detail().to_owned())?;
        Ok((config, session))
    }
}

pub fn configure(
    state: &ScrubImapState,
    request: ConfigureImapRequest,
) -> Result<ImapCapability, String> {
    state.configure_with(request, &RealConnector, true)
}

pub fn capability(state: &ScrubImapState, account_id: &str) -> ImapCapability {
    let config = state
        .accounts
        .lock()
        .ok()
        .and_then(|accounts| accounts.get(account_id).cloned());
    match config {
        Some(config) => ImapCapability {
            configured: true,
            live_confirmed: true,
            auth_epoch: Some(config.auth_epoch),
            detail: "IMAP was live-confirmed in this process session".to_owned(),
        },
        None => ImapCapability {
            configured: load_stored_account(account_id).is_ok(),
            live_confirmed: false,
            auth_epoch: None,
            detail: "Fresh IMAP authentication is required in this session".to_owned(),
        },
    }
}

pub fn reauthenticate(state: &ScrubImapState, account_id: &str) -> Result<ImapCapability, String> {
    let in_memory = state
        .accounts
        .lock()
        .map_err(|_| "IMAP account state is unavailable".to_owned())?
        .get(account_id)
        .cloned();
    let stored = load_stored_account(account_id)?;
    let mut config = in_memory.unwrap_or(AccountConfig {
        host: stored.host.clone(),
        port: stored.port,
        username: stored.username.clone(),
        auth_kind: stored.auth_kind,
        default_mailbox: stored.default_mailbox.clone(),
        auth_epoch: String::new(),
        auth_issued_at: Instant::now(),
    });
    let secret = Zeroizing::new(stored.secret.clone());
    let mut session = RealConnector
        .connect(&config, secret.as_str())
        .map_err(|e| e.public_detail().to_owned())?;
    session
        .select(&config.default_mailbox)
        .map_err(|e| e.public_detail().to_owned())?;
    config.auth_epoch = next_auth_epoch(account_id);
    config.auth_issued_at = Instant::now();
    state
        .accounts
        .lock()
        .map_err(|_| "IMAP account state is unavailable".to_owned())?
        .insert(account_id.to_owned(), config.clone());
    Ok(ImapCapability {
        configured: true,
        live_confirmed: true,
        auth_epoch: Some(config.auth_epoch),
        detail: "Fresh TLS, authentication, and mailbox selection confirmed".to_owned(),
    })
}

fn enumerate_with(
    state: &ScrubImapState,
    request: &ImapItemRequest,
    connector: &dyn SessionConnector,
    secret_override: Option<&str>,
) -> Result<ImapEnumeration, String> {
    let (config, mut session) = state.connect_for_item(request, connector, secret_override)?;
    let since_date = request_since_date(request).map_err(|e| e.public_detail().to_owned())?;
    let uids = session
        .search(&request.message_id, since_date.as_deref())
        .map_err(|e| e.public_detail().to_owned())?;
    let mut findings = Vec::with_capacity(uids.len());
    for uid in uids {
        let headers = session
            .fetch_identity_headers(uid)
            .map_err(|e| e.public_detail().to_owned())?;
        findings.push(
            finding_from_headers(
                &config,
                &request.mailbox,
                uid,
                &request.message_id,
                &headers,
            )
            .map_err(|e| e.public_detail().to_owned())?,
        );
    }
    Ok(ImapEnumeration {
        findings,
        auth_epoch: config.auth_epoch,
    })
}

pub fn enumerate(
    state: &ScrubImapState,
    request: &ImapItemRequest,
) -> Result<ImapEnumeration, String> {
    enumerate_with(state, request, &RealConnector, None)
}

fn inspect_with(
    state: &ScrubImapState,
    request: &ImapItemRequest,
    connector: &dyn SessionConnector,
    secret_override: Option<&str>,
) -> Result<ImapInspection, String> {
    let result = enumerate_with(state, request, connector, secret_override)?;
    let (state_value, uid, authored_by_self, fingerprint) = match result.findings.as_slice() {
        [] => (ImapPresence::Absent, None, false, None),
        [finding] => (
            ImapPresence::Present,
            Some(finding.uid),
            finding.authored_by_self,
            Some(finding.content_fingerprint.clone()),
        ),
        _ => return Err(TransportFailure::Ambiguous.public_detail().to_owned()),
    };
    Ok(ImapInspection {
        state: state_value,
        authored_by_self,
        content_fingerprint: fingerprint,
        auth_epoch: result.auth_epoch,
        schema_version: "imap-v1",
        retractable: true,
        detail: match uid {
            Some(_) => "IMAP identity headers were read from the server".to_owned(),
            None => "IMAP Message-ID/date search found no message".to_owned(),
        },
    })
}

pub fn inspect(
    state: &ScrubImapState,
    request: &ImapItemRequest,
) -> Result<ImapInspection, String> {
    inspect_with(state, request, &RealConnector, None)
}

fn delete_with(
    state: &ScrubImapState,
    request: &ImapItemRequest,
    connector: &dyn SessionConnector,
    secret_override: Option<&str>,
) -> Result<ImapDeleteResult, String> {
    let (config, mut session) = state.connect_for_item(request, connector, secret_override)?;
    let since_date = request_since_date(request).map_err(|e| e.public_detail().to_owned())?;
    let uids = session
        .search(&request.message_id, since_date.as_deref())
        .map_err(|e| e.public_detail().to_owned())?;
    let uid = match uids.as_slice() {
        [uid] => *uid,
        [] => {
            return Ok(ImapDeleteResult {
                accepted: false,
                auth_epoch: config.auth_epoch,
                detail: "Message was not present at delete time".to_owned(),
            })
        }
        _ => return Err(TransportFailure::Ambiguous.public_detail().to_owned()),
    };
    let headers = session
        .fetch_identity_headers(uid)
        .map_err(|e| e.public_detail().to_owned())?;
    let finding = finding_from_headers(
        &config,
        &request.mailbox,
        uid,
        &request.message_id,
        &headers,
    )
    .map_err(|e| e.public_detail().to_owned())?;
    if !finding.authored_by_self {
        return Ok(ImapDeleteResult {
            accepted: false,
            auth_epoch: config.auth_epoch,
            detail: "Delete refused because From did not match the configured account".to_owned(),
        });
    }
    if request.expected_content_fingerprint.as_deref() != Some(finding.content_fingerprint.as_str())
    {
        return Ok(ImapDeleteResult {
            accepted: false,
            auth_epoch: config.auth_epoch,
            detail: "Delete refused because the inspected message fingerprint changed".to_owned(),
        });
    }
    if !session
        .supports_uid_expunge()
        .map_err(|e| e.public_detail().to_owned())?
    {
        return Ok(ImapDeleteResult {
            accepted: false,
            auth_epoch: config.auth_epoch,
            detail: "Delete refused because the server does not advertise scoped UID EXPUNGE"
                .to_owned(),
        });
    }
    session
        .mark_deleted(uid)
        .map_err(|e| e.public_detail().to_owned())?;
    session
        .expunge_uid(uid)
        .map_err(|e| e.public_detail().to_owned())?;
    Ok(ImapDeleteResult {
        accepted: true,
        auth_epoch: config.auth_epoch,
        detail: "Server accepted Deleted flag and EXPUNGE; readback still required".to_owned(),
    })
}

pub fn delete(
    state: &ScrubImapState,
    request: &ImapItemRequest,
) -> Result<ImapDeleteResult, String> {
    delete_with(state, request, &RealConnector, None)
}

fn verify_with(
    state: &ScrubImapState,
    request: &ImapItemRequest,
    connector: &dyn SessionConnector,
    secret_override: Option<&str>,
) -> ImapVerification {
    let epoch = request.expected_auth_epoch.clone();
    match inspect_with(state, request, connector, secret_override) {
        Ok(inspection) if inspection.state == ImapPresence::Absent => ImapVerification {
            outcome: ImapVerificationOutcome::ConfirmedDeleted,
            auth_epoch: inspection.auth_epoch,
            detail: "IMAP Message-ID/date readback found no message".to_owned(),
        },
        Ok(inspection) => ImapVerification {
            outcome: ImapVerificationOutcome::ConfirmedNotDeleted,
            auth_epoch: inspection.auth_epoch,
            detail: "IMAP Message-ID/date readback still found the message".to_owned(),
        },
        Err(detail) => ImapVerification {
            outcome: ImapVerificationOutcome::Unknown,
            auth_epoch: epoch,
            detail,
        },
    }
}

pub fn verify(state: &ScrubImapState, request: &ImapItemRequest) -> ImapVerification {
    verify_with(state, request, &RealConnector, None)
}

impl ImapLiveTestTransport {
    pub fn connect(
        host: String,
        port: u16,
        username: String,
        app_password: String,
    ) -> Result<Self, String> {
        let state = ScrubImapState::default();
        let account_id = "imap-livetest".to_owned();
        let secret = Zeroizing::new(app_password);
        let capability = state.configure_with(
            ConfigureImapRequest {
                account_id: account_id.clone(),
                host,
                port: Some(port),
                username,
                auth: ImapAuthInput {
                    kind: ImapAuthKind::AppPassword,
                    secret: secret.to_string(),
                },
                default_mailbox: Some("INBOX".to_owned()),
            },
            &RealConnector,
            false,
        )?;
        let auth_epoch = capability
            .auth_epoch
            .ok_or_else(|| "IMAP authentication did not produce a live epoch".to_owned())?;
        Ok(Self {
            state,
            account_id,
            auth_epoch,
            secret,
        })
    }

    pub fn recent_messages(&self, limit: usize) -> Result<Vec<ImapLiveMessage>, String> {
        let config = self
            .state
            .config_for_epoch(&self.account_id, &self.auth_epoch)?;
        let mut session = RealConnector
            .connect(&config, self.secret.as_str())
            .map_err(|error| error.public_detail().to_owned())?;
        session
            .select("INBOX")
            .map_err(|error| error.public_detail().to_owned())?;
        let mut messages = session
            .recent_messages(limit)
            .map_err(|error| error.public_detail().to_owned())?;
        for message in &mut messages {
            let Some(message_id) = message.message_id.as_deref() else {
                continue;
            };
            let finding = finding_from_headers(
                &config,
                "INBOX",
                message.uid,
                message_id,
                &message.identity_headers,
            )
            .map_err(|error| error.public_detail().to_owned())?;
            message.authored_by_self = finding.authored_by_self;
            message.content_fingerprint = Some(finding.content_fingerprint);
        }
        Ok(messages)
    }

    pub fn prepare_delete(
        &self,
        message: &ImapLiveMessage,
    ) -> Result<ImapLivePreparedDelete, String> {
        let message_id = message.message_id.as_deref().ok_or_else(|| {
            "Delete refused because the searched message has no Message-ID".to_owned()
        })?;
        let searched_fingerprint = message.content_fingerprint.as_deref().ok_or_else(|| {
            "Delete refused because the searched message has no identity fingerprint".to_owned()
        })?;
        let mut request = ImapItemRequest {
            account_id: self.account_id.clone(),
            expected_auth_epoch: self.auth_epoch.clone(),
            mailbox: "INBOX".to_owned(),
            message_id: message_id.to_owned(),
            since_date_unix_ms: None,
            expected_content_fingerprint: None,
        };
        let inspection = inspect_with(
            &self.state,
            &request,
            &RealConnector,
            Some(self.secret.as_str()),
        )?;
        if inspection.state != ImapPresence::Present {
            return Err(
                "Delete refused because the searched message is no longer present".to_owned(),
            );
        }
        if !inspection.authored_by_self {
            return Err(
                "Delete refused because From did not match the configured account".to_owned(),
            );
        }
        let inspected_fingerprint = inspection.content_fingerprint.ok_or_else(|| {
            "Delete refused because inspection produced no content fingerprint".to_owned()
        })?;
        if inspected_fingerprint != searched_fingerprint {
            return Err(
                "Delete refused because the message changed after the prior search".to_owned(),
            );
        }
        request.expected_content_fingerprint = Some(inspected_fingerprint);
        Ok(ImapLivePreparedDelete {
            uid: message.uid,
            message_id: message_id.to_owned(),
            request,
        })
    }

    pub fn delete_prepared(&self, prepared: &ImapLivePreparedDelete) -> ImapLiveDeleteExecution {
        let delete_result = delete_with(
            &self.state,
            &prepared.request,
            &RealConnector,
            Some(self.secret.as_str()),
        );
        let verification = verify_with(
            &self.state,
            &prepared.request,
            &RealConnector,
            Some(self.secret.as_str()),
        );
        ImapLiveDeleteExecution {
            delete_result,
            verification,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct FakeMailboxState {
        present: HashSet<u32>,
        search_failure: Option<TransportFailure>,
        retain_after_expunge: bool,
        selected: Vec<String>,
        marked: Vec<u32>,
        headers: Option<Vec<u8>>,
        uidplus: bool,
    }

    struct FakeConnector(Arc<Mutex<FakeMailboxState>>);

    struct FakeMailbox(Arc<Mutex<FakeMailboxState>>);

    impl SessionConnector for FakeConnector {
        fn connect(
            &self,
            _config: &AccountConfig,
            _secret: &str,
        ) -> Result<Box<dyn MailboxSession>, TransportFailure> {
            Ok(Box::new(FakeMailbox(self.0.clone())))
        }
    }

    impl MailboxSession for FakeMailbox {
        fn select(&mut self, mailbox: &str) -> Result<(), TransportFailure> {
            self.0.lock().unwrap().selected.push(mailbox.to_owned());
            Ok(())
        }

        fn search(
            &mut self,
            _message_id: &str,
            _since_date: Option<&str>,
        ) -> Result<Vec<u32>, TransportFailure> {
            let state = self.0.lock().unwrap();
            if let Some(error) = state.search_failure {
                return Err(error);
            }
            Ok(state.present.iter().copied().collect())
        }

        fn fetch_identity_headers(&mut self, _uid: u32) -> Result<Vec<u8>, TransportFailure> {
            Ok(self.0.lock().unwrap().headers.clone().unwrap_or_else(|| {
                b"Message-ID: <message@example.test>\r\nFrom: self@example.test\r\n\r\n".to_vec()
            }))
        }

        fn supports_uid_expunge(&mut self) -> Result<bool, TransportFailure> {
            Ok(self.0.lock().unwrap().uidplus)
        }

        fn mark_deleted(&mut self, uid: u32) -> Result<(), TransportFailure> {
            self.0.lock().unwrap().marked.push(uid);
            Ok(())
        }

        fn expunge_uid(&mut self, uid: u32) -> Result<(), TransportFailure> {
            let mut state = self.0.lock().unwrap();
            if !state.retain_after_expunge {
                state.present.remove(&uid);
            }
            Ok(())
        }
    }

    fn configured(fake: Arc<Mutex<FakeMailboxState>>) -> (ScrubImapState, FakeConnector, String) {
        let state = ScrubImapState::default();
        let connector = FakeConnector(fake);
        let capability = state
            .configure_with(
                ConfigureImapRequest {
                    account_id: "mail".to_owned(),
                    host: "imap.example.test".to_owned(),
                    port: Some(993),
                    username: "self@example.test".to_owned(),
                    auth: ImapAuthInput {
                        kind: ImapAuthKind::AppPassword,
                        secret: "test-only-secret".to_owned(),
                    },
                    default_mailbox: Some("Sent".to_owned()),
                },
                &connector,
                false,
            )
            .unwrap();
        (state, connector, capability.auth_epoch.unwrap())
    }

    fn item(epoch: String) -> ImapItemRequest {
        ImapItemRequest {
            account_id: "mail".to_owned(),
            expected_auth_epoch: epoch,
            mailbox: "Sent".to_owned(),
            message_id: "<message@example.test>".to_owned(),
            since_date_unix_ms: Some(1_774_051_200_000),
            expected_content_fingerprint: None,
        }
    }

    fn authorize_delete(
        state: &ScrubImapState,
        connector: &FakeConnector,
        request: &mut ImapItemRequest,
    ) {
        let inspection = inspect_with(state, request, connector, Some("secret")).unwrap();
        assert!(inspection.authored_by_self);
        request.expected_content_fingerprint = inspection.content_fingerprint;
    }

    #[test]
    fn delete_then_readback_confirms_deleted() {
        let fake = Arc::new(Mutex::new(FakeMailboxState {
            present: HashSet::from([7]),
            uidplus: true,
            ..Default::default()
        }));
        let (state, connector, epoch) = configured(fake.clone());
        let mut request = item(epoch);
        authorize_delete(&state, &connector, &mut request);

        let deleted = delete_with(&state, &request, &connector, Some("secret")).unwrap();
        assert!(deleted.accepted);
        assert_eq!(
            verify_with(&state, &request, &connector, Some("secret")).outcome,
            ImapVerificationOutcome::ConfirmedDeleted
        );
        assert_eq!(fake.lock().unwrap().marked, vec![7]);
    }

    #[test]
    fn server_acceptance_without_removal_confirms_not_deleted() {
        let fake = Arc::new(Mutex::new(FakeMailboxState {
            present: HashSet::from([8]),
            retain_after_expunge: true,
            uidplus: true,
            ..Default::default()
        }));
        let (state, connector, epoch) = configured(fake);
        let mut request = item(epoch);
        authorize_delete(&state, &connector, &mut request);

        assert!(
            delete_with(&state, &request, &connector, Some("secret"))
                .unwrap()
                .accepted
        );
        assert_eq!(
            verify_with(&state, &request, &connector, Some("secret")).outcome,
            ImapVerificationOutcome::ConfirmedNotDeleted
        );
    }

    #[test]
    fn readback_failure_is_unknown() {
        let fake = Arc::new(Mutex::new(FakeMailboxState {
            search_failure: Some(TransportFailure::TlsOrConnection),
            ..Default::default()
        }));
        let (state, connector, epoch) = configured(fake);
        let result = verify_with(&state, &item(epoch), &connector, Some("secret"));
        assert_eq!(result.outcome, ImapVerificationOutcome::Unknown);
        assert!(result.detail.contains("TLS"));
    }

    #[test]
    fn ambiguity_never_mutates_and_verifies_unknown() {
        let fake = Arc::new(Mutex::new(FakeMailboxState {
            present: HashSet::from([1, 2]),
            ..Default::default()
        }));
        let (state, connector, epoch) = configured(fake.clone());
        let request = item(epoch);
        assert!(delete_with(&state, &request, &connector, Some("secret")).is_err());
        assert!(fake.lock().unwrap().marked.is_empty());
        assert_eq!(
            verify_with(&state, &request, &connector, Some("secret")).outcome,
            ImapVerificationOutcome::Unknown
        );
    }

    #[test]
    fn stale_auth_epoch_fails_before_connecting() {
        let fake = Arc::new(Mutex::new(FakeMailboxState {
            present: HashSet::from([4]),
            ..Default::default()
        }));
        let (state, connector, _epoch) = configured(fake.clone());
        let request = item("stale".to_owned());
        assert!(delete_with(&state, &request, &connector, Some("secret")).is_err());
        assert_eq!(fake.lock().unwrap().selected, vec!["Sent"]);
    }

    #[test]
    fn fresh_process_is_never_live_confirmed() {
        let capability = capability(&ScrubImapState::default(), "mail");
        assert!(!capability.live_confirmed);
        assert!(!capability.configured);
    }

    #[test]
    fn nested_auth_and_numeric_date_contract_deserialize() {
        let configured: ConfigureImapRequest = serde_json::from_value(serde_json::json!({
            "accountId": "mail",
            "host": "imap.example.test",
            "username": "self@example.test",
            "auth": { "kind": "appPassword", "secret": "secret" }
        }))
        .unwrap();
        assert_eq!(configured.auth.kind, ImapAuthKind::AppPassword);
        let item: ImapItemRequest = serde_json::from_value(serde_json::json!({
            "accountId": "mail",
            "expectedAuthEpoch": "epoch",
            "mailbox": "Sent",
            "messageId": "<message@example.test>",
            "sinceDateUnixMs": 1774051200000_u64
        }))
        .unwrap();
        assert_eq!(item.since_date_unix_ms, Some(1_774_051_200_000));
    }

    #[test]
    fn delete_refuses_non_self_message_and_missing_uidplus() {
        let fake = Arc::new(Mutex::new(FakeMailboxState {
            present: HashSet::from([9]),
            headers: Some(
                b"Message-ID: <message@example.test>\r\nFrom: other@example.test\r\n\r\n".to_vec(),
            ),
            uidplus: true,
            ..Default::default()
        }));
        let (state, connector, epoch) = configured(fake.clone());
        let mut request = item(epoch);
        let inspection = inspect_with(&state, &request, &connector, Some("secret")).unwrap();
        assert!(!inspection.authored_by_self);
        request.expected_content_fingerprint = inspection.content_fingerprint;
        let result = delete_with(&state, &request, &connector, Some("secret")).unwrap();
        assert!(!result.accepted);
        assert!(fake.lock().unwrap().marked.is_empty());

        let fake = Arc::new(Mutex::new(FakeMailboxState {
            present: HashSet::from([10]),
            uidplus: false,
            ..Default::default()
        }));
        let (state, connector, epoch) = configured(fake.clone());
        let mut request = item(epoch);
        authorize_delete(&state, &connector, &mut request);
        let result = delete_with(&state, &request, &connector, Some("secret")).unwrap();
        assert!(!result.accepted);
        assert!(fake.lock().unwrap().marked.is_empty());
    }
}
