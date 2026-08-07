//! Real website driver contract and browser-backed implementation.
//!
//! The implementation that talks to a browser lives behind this interface. The
//! backend job list is fixed here so higher-level website work cannot smuggle in
//! generic browser automation verbs.

use base64::Engine;
use core::fmt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebsiteDriverKind {
    RealBrowser,
    FakeTestBrowser,
}

impl WebsiteDriverKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RealBrowser => "realBrowser",
            Self::FakeTestBrowser => "fakeTestBrowser",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebsiteDriverJob {
    FindPage,
    ReadPage,
    PlaceText,
    PressNamedControl,
    ReadSelectedEmail,
    ReadSelectedEmailIdentity,
    ReadMailbox,
    ReadLiveRunProgress,
    SendEmailDraft,
}

impl WebsiteDriverJob {
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::FindPage => "find_page",
            Self::ReadPage => "read_page",
            Self::PlaceText => "place_text",
            Self::PressNamedControl => "press_named_control",
            Self::ReadSelectedEmail => "read_selected_email",
            Self::ReadSelectedEmailIdentity => "read_selected_email_identity",
            Self::ReadMailbox => "read_mailbox",
            Self::ReadLiveRunProgress => "read_live_run_progress",
            Self::SendEmailDraft => "send_email_draft",
        }
    }
}

pub const WEBSITE_DRIVER_JOBS: [WebsiteDriverJob; 9] = [
    WebsiteDriverJob::FindPage,
    WebsiteDriverJob::ReadPage,
    WebsiteDriverJob::PlaceText,
    WebsiteDriverJob::PressNamedControl,
    WebsiteDriverJob::ReadSelectedEmail,
    WebsiteDriverJob::ReadSelectedEmailIdentity,
    WebsiteDriverJob::ReadMailbox,
    WebsiteDriverJob::ReadLiveRunProgress,
    WebsiteDriverJob::SendEmailDraft,
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsitePageRequest {
    pub url: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsitePage {
    pub url: String,
}

impl WebsitePage {
    pub fn synthetic(url: String) -> Self {
        Self { url }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsiteTextPlacement {
    pub page: WebsitePage,
    pub text: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebsiteControlKind {
    EditableBox,
    Button,
    VisibleMessageArea,
}

impl WebsiteControlKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EditableBox => "editableBox",
            Self::Button => "button",
            Self::VisibleMessageArea => "visibleMessageArea",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WebsiteNamedControlRequest {
    pub name: &'static str,
    pub kind: WebsiteControlKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsiteNamedControl {
    pub page: WebsitePage,
    pub name: String,
    pub kind: WebsiteControlKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsiteSendCommand {
    pub placement: WebsiteTextPlacement,
    pub send_control_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsitePlacementProof {
    pub page: WebsitePage,
    pub editable_box_name: String,
    pub utf16_units: usize,
    pub placed_sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsiteSendReceipt {
    pub placement_proof: WebsitePlacementProof,
    pub send_control_name: String,
    pub send_pressed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsitePageText {
    pub page: WebsitePage,
    pub title: String,
    pub text: String,
    pub controls: WebsitePageControls,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsiteSelectedEmail {
    pub page: WebsitePage,
    pub message_id: String,
    pub body: String,
    pub conversation_identity: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsiteSelectedEmailIdentity {
    pub page: WebsitePage,
    pub thread_identity: String,
    pub folder_identity: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsiteEmailDraft {
    pub recipient: String,
    pub body: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsiteEmailSendReceipt {
    pub page: WebsitePage,
    pub recipient: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsiteMailboxMessage {
    pub folder: String,
    pub subject: String,
    pub time: String,
    pub sender: String,
    pub owner_marker: String,
    pub yours: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsiteMailboxRead {
    pub page: WebsitePage,
    pub folders: Vec<String>,
    pub messages: Vec<WebsiteMailboxMessage>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebsiteLiveRunProgress {
    pub active_account: String,
    pub current_place: String,
    pub messages_checked: usize,
    pub matches: usize,
    pub scrolls: usize,
    pub waits: usize,
    pub changes: usize,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
pub struct WebsitePageControls {
    pub editable_boxes: Vec<String>,
    pub buttons: Vec<String>,
    pub visible_message_areas: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WebsiteDriverError {
    BrowserUnavailable,
    BrowserLaunchFailed,
    BrowserConnectionFailed,
    PageNotFound,
    PageUnavailable,
    ReadFailed,
    NoSelectedMessage,
    TextPlacementFailed,
    NamedControlNotFound,
    NamedControlDisabled,
    MissingNamedControl(String),
    MalformedRecipient,
}

impl fmt::Display for WebsiteDriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BrowserUnavailable => f.write_str("website browser executable was not found"),
            Self::BrowserLaunchFailed => f.write_str("website browser could not be launched"),
            Self::BrowserConnectionFailed => f.write_str("website browser connection failed"),
            Self::PageNotFound => f.write_str("website page was not found"),
            Self::PageUnavailable => f.write_str("website page was unavailable"),
            Self::ReadFailed => f.write_str("website page could not be read"),
            Self::NoSelectedMessage => f.write_str("no selected message"),
            Self::TextPlacementFailed => f.write_str("website text could not be placed"),
            Self::NamedControlNotFound => f.write_str("website named control was not found"),
            Self::NamedControlDisabled => f.write_str("Send disabled"),
            Self::MissingNamedControl(name) => write!(f, "missing named website control: {name}"),
            Self::MalformedRecipient => f.write_str("malformed recipient"),
        }
    }
}

impl std::error::Error for WebsiteDriverError {}

pub trait WebsiteDriver {
    const JOBS: &'static [WebsiteDriverJob] = &WEBSITE_DRIVER_JOBS;

    fn kind(&self) -> WebsiteDriverKind {
        WebsiteDriverKind::FakeTestBrowser
    }

    fn find_page(&mut self, request: WebsitePageRequest)
        -> Result<WebsitePage, WebsiteDriverError>;

    fn read_page(&mut self, _page: &WebsitePage) -> Result<WebsitePageText, WebsiteDriverError> {
        Err(WebsiteDriverError::ReadFailed)
    }

    fn read_named_controls(
        &mut self,
        page: &WebsitePage,
        required: &[WebsiteNamedControlRequest],
    ) -> Result<Vec<WebsiteNamedControl>, WebsiteDriverError> {
        let controls = self.read_page(page)?.controls;
        let mut found = Vec::new();
        for request in required {
            let present = match request.kind {
                WebsiteControlKind::EditableBox => controls.editable_boxes.iter(),
                WebsiteControlKind::Button => controls.buttons.iter(),
                WebsiteControlKind::VisibleMessageArea => controls.visible_message_areas.iter(),
            }
            .any(|name| name == request.name);
            if !present {
                return Err(WebsiteDriverError::MissingNamedControl(
                    request.name.to_owned(),
                ));
            }
            found.push(WebsiteNamedControl {
                page: page.clone(),
                name: request.name.to_owned(),
                kind: request.kind,
            });
        }
        Ok(found)
    }

    fn place_text(
        &mut self,
        _placement: WebsiteTextPlacement,
    ) -> Result<WebsitePlacementProof, WebsiteDriverError> {
        Err(WebsiteDriverError::TextPlacementFailed)
    }

    fn press_named_control(
        &mut self,
        _control: WebsiteNamedControl,
    ) -> Result<(), WebsiteDriverError> {
        Err(WebsiteDriverError::NamedControlNotFound)
    }

    fn send_after_successful_placement(
        &mut self,
        command: WebsiteSendCommand,
    ) -> Result<WebsiteSendReceipt, WebsiteDriverError> {
        let page = command.placement.page.clone();
        let send_control_name = command.send_control_name;
        let placement_proof = self.place_text(command.placement)?;
        self.press_named_control(WebsiteNamedControl {
            page,
            name: send_control_name.clone(),
            kind: WebsiteControlKind::Button,
        })?;
        Ok(WebsiteSendReceipt {
            placement_proof,
            send_control_name,
            send_pressed: true,
        })
    }

    fn read_selected_email(
        &mut self,
        _page: &WebsitePage,
    ) -> Result<WebsiteSelectedEmail, WebsiteDriverError> {
        Err(WebsiteDriverError::NoSelectedMessage)
    }

    fn read_selected_email_identity(
        &mut self,
        _page: &WebsitePage,
    ) -> Result<WebsiteSelectedEmailIdentity, WebsiteDriverError> {
        Err(WebsiteDriverError::ReadFailed)
    }

    fn read_mailbox(
        &mut self,
        _page: &WebsitePage,
    ) -> Result<WebsiteMailboxRead, WebsiteDriverError> {
        Err(WebsiteDriverError::ReadFailed)
    }

    fn read_live_run_progress(
        &mut self,
        _page: &WebsitePage,
    ) -> Result<WebsiteLiveRunProgress, WebsiteDriverError> {
        Err(WebsiteDriverError::ReadFailed)
    }

    fn send_email_draft(
        &mut self,
        _page: &WebsitePage,
        _draft: WebsiteEmailDraft,
    ) -> Result<WebsiteEmailSendReceipt, WebsiteDriverError> {
        Err(WebsiteDriverError::ReadFailed)
    }
}

pub struct RealBrowserWebsiteDriver {
    browser: Child,
    browser_executable: PathBuf,
    profile_dir: PathBuf,
    devtools_base_url: String,
    client: reqwest::blocking::Client,
}

#[derive(Deserialize)]
struct DevtoolsTarget {
    id: String,
    #[serde(default)]
    #[allow(dead_code)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(rename = "type")]
    target_type: String,
    #[serde(rename = "webSocketDebuggerUrl")]
    web_socket_debugger_url: Option<String>,
}

#[derive(Deserialize)]
struct BrowserPageSnapshot {
    title: String,
    text: String,
    controls: WebsitePageControls,
}

#[derive(Deserialize)]
struct BrowserTextPlacementResult {
    placed: bool,
    readback: String,
    editable_name: String,
}

#[derive(Deserialize)]
struct BrowserSelectedEmailSnapshot {
    message_id: String,
    body: String,
    conversation_identity: String,
}

#[derive(Deserialize)]
struct BrowserSelectedEmailIdentitySnapshot {
    thread_identity: String,
    folder_identity: String,
}

#[derive(Deserialize)]
struct BrowserMailboxReadSnapshot {
    folders: Vec<String>,
    messages: Vec<BrowserMailboxMessageSnapshot>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrowserMailboxMessageSnapshot {
    folder: String,
    subject: String,
    time: String,
    sender: String,
    owner_marker: String,
    yours: bool,
}

impl RealBrowserWebsiteDriver {
    pub fn launch() -> Result<Self, WebsiteDriverError> {
        let browser_executable =
            discover_browser_executable().ok_or(WebsiteDriverError::BrowserUnavailable)?;
        Self::launch_with_executable(browser_executable)
    }

    pub fn browser_executable(&self) -> &Path {
        &self.browser_executable
    }

    fn launch_with_executable(browser_executable: PathBuf) -> Result<Self, WebsiteDriverError> {
        let port = reserve_loopback_port()?;
        let profile_dir = make_profile_dir()?;
        let devtools_base_url = format!("http://127.0.0.1:{port}");
        let mut browser = Command::new(&browser_executable)
            .arg("--headless=new")
            .arg("--no-sandbox")
            .arg("--disable-background-networking")
            .arg("--disable-dev-shm-usage")
            .arg("--disable-extensions")
            .arg("--disable-gpu")
            .arg("--disable-sync")
            .arg("--no-default-browser-check")
            .arg("--no-first-run")
            .arg(format!("--remote-debugging-port={port}"))
            .arg(format!("--user-data-dir={}", profile_dir.display()))
            .arg("about:blank")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| WebsiteDriverError::BrowserLaunchFailed)?;
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_millis(750))
            .build()
            .map_err(|_| WebsiteDriverError::BrowserConnectionFailed)?;

        if wait_for_devtools(&client, &devtools_base_url, Duration::from_secs(10)).is_err() {
            let _ = browser.kill();
            let _ = browser.wait();
            let _ = fs::remove_dir_all(&profile_dir);
            return Err(WebsiteDriverError::BrowserConnectionFailed);
        }

        Ok(Self {
            browser,
            browser_executable,
            profile_dir,
            devtools_base_url,
            client,
        })
    }

    fn devtools_targets(&self) -> Result<Vec<DevtoolsTarget>, WebsiteDriverError> {
        self.client
            .get(format!("{}/json/list", self.devtools_base_url))
            .send()
            .and_then(|response| response.error_for_status())
            .map_err(|_| WebsiteDriverError::BrowserConnectionFailed)?
            .json::<Vec<DevtoolsTarget>>()
            .map_err(|_| WebsiteDriverError::ReadFailed)
    }

    fn page_websocket_url(&self, page: &WebsitePage) -> Result<String, WebsiteDriverError> {
        self.devtools_targets()?
            .into_iter()
            .find(|target| target.target_type == "page" && target.url == page.url)
            .and_then(|target| target.web_socket_debugger_url)
            .ok_or(WebsiteDriverError::ReadFailed)
    }
}

impl WebsiteDriver for RealBrowserWebsiteDriver {
    fn kind(&self) -> WebsiteDriverKind {
        WebsiteDriverKind::RealBrowser
    }

    fn find_page(
        &mut self,
        request: WebsitePageRequest,
    ) -> Result<WebsitePage, WebsiteDriverError> {
        let encoded_url =
            url::form_urlencoded::byte_serialize(request.url.as_bytes()).collect::<String>();
        let target = self
            .client
            .put(format!("{}/json/new?{encoded_url}", self.devtools_base_url))
            .send()
            .and_then(|response| response.error_for_status())
            .map_err(|_| WebsiteDriverError::PageNotFound)?
            .json::<DevtoolsTarget>()
            .map_err(|_| WebsiteDriverError::PageNotFound)?;
        if target.id.is_empty() {
            return Err(WebsiteDriverError::PageNotFound);
        }
        Ok(WebsitePage { url: request.url })
    }

    fn read_page(&mut self, page: &WebsitePage) -> Result<WebsitePageText, WebsiteDriverError> {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            if let Ok(websocket_url) = self.page_websocket_url(page) {
                let snapshot = read_page_snapshot(&websocket_url)?;
                if !snapshot.title.is_empty() {
                    return Ok(WebsitePageText {
                        page: page.clone(),
                        title: snapshot.title,
                        text: snapshot.text,
                        controls: snapshot.controls,
                    });
                }
            }

            if std::time::Instant::now() >= deadline {
                return Err(WebsiteDriverError::ReadFailed);
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    fn place_text(
        &mut self,
        placement: WebsiteTextPlacement,
    ) -> Result<WebsitePlacementProof, WebsiteDriverError> {
        let websocket_url = self.page_websocket_url(&placement.page)?;
        let result = place_text_in_first_named_editable(&websocket_url, &placement.text)?;
        if !result.placed || result.readback != placement.text {
            return Err(WebsiteDriverError::TextPlacementFailed);
        }
        Ok(WebsitePlacementProof {
            page: placement.page,
            editable_box_name: result.editable_name,
            utf16_units: placement.text.encode_utf16().count(),
            placed_sha256: sha256_hex(placement.text.as_bytes()),
        })
    }

    fn press_named_control(
        &mut self,
        control: WebsiteNamedControl,
    ) -> Result<(), WebsiteDriverError> {
        let websocket_url = self.page_websocket_url(&control.page)?;
        match press_named_button(&websocket_url, &control.name)? {
            NamedButtonPress::Pressed => Ok(()),
            NamedButtonPress::Disabled => Err(WebsiteDriverError::NamedControlDisabled),
            NamedButtonPress::NotFound => Err(WebsiteDriverError::NamedControlNotFound),
        }
    }

    fn read_selected_email(
        &mut self,
        page: &WebsitePage,
    ) -> Result<WebsiteSelectedEmail, WebsiteDriverError> {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            if let Ok(websocket_url) = self.page_websocket_url(page) {
                if let Ok(snapshot) = read_selected_email_snapshot(&websocket_url) {
                    return Ok(WebsiteSelectedEmail {
                        page: page.clone(),
                        message_id: snapshot.message_id,
                        body: snapshot.body,
                        conversation_identity: snapshot.conversation_identity,
                    });
                }
            }

            if std::time::Instant::now() >= deadline {
                return Err(WebsiteDriverError::NoSelectedMessage);
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    fn read_selected_email_identity(
        &mut self,
        page: &WebsitePage,
    ) -> Result<WebsiteSelectedEmailIdentity, WebsiteDriverError> {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            if let Ok(websocket_url) = self.page_websocket_url(page) {
                if let Ok(snapshot) = read_selected_email_identity_snapshot(&websocket_url) {
                    return Ok(WebsiteSelectedEmailIdentity {
                        page: page.clone(),
                        thread_identity: snapshot.thread_identity,
                        folder_identity: snapshot.folder_identity,
                    });
                }
            }

            if std::time::Instant::now() >= deadline {
                return Err(WebsiteDriverError::ReadFailed);
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    fn read_mailbox(
        &mut self,
        page: &WebsitePage,
    ) -> Result<WebsiteMailboxRead, WebsiteDriverError> {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            if let Ok(websocket_url) = self.page_websocket_url(page) {
                if let Ok(snapshot) = read_mailbox_snapshot(&websocket_url) {
                    return Ok(WebsiteMailboxRead {
                        page: page.clone(),
                        folders: snapshot.folders,
                        messages: snapshot
                            .messages
                            .into_iter()
                            .map(|message| WebsiteMailboxMessage {
                                folder: message.folder,
                                subject: message.subject,
                                time: message.time,
                                sender: message.sender,
                                owner_marker: message.owner_marker,
                                yours: message.yours,
                            })
                            .collect(),
                    });
                }
            }

            if std::time::Instant::now() >= deadline {
                return Err(WebsiteDriverError::ReadFailed);
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    fn read_live_run_progress(
        &mut self,
        page: &WebsitePage,
    ) -> Result<WebsiteLiveRunProgress, WebsiteDriverError> {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            if let Ok(websocket_url) = self.page_websocket_url(page) {
                if let Ok(progress) = read_live_run_progress_snapshot(&websocket_url) {
                    return Ok(progress);
                }
            }

            if std::time::Instant::now() >= deadline {
                return Err(WebsiteDriverError::ReadFailed);
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    fn send_email_draft(
        &mut self,
        page: &WebsitePage,
        draft: WebsiteEmailDraft,
    ) -> Result<WebsiteEmailSendReceipt, WebsiteDriverError> {
        let recipient_is_valid = valid_email_recipient(&draft.recipient);
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            if let Ok(websocket_url) = self.page_websocket_url(page) {
                if let Ok(sent) = evaluate_target(
                    &websocket_url,
                    &email_send_expression(
                        &draft.recipient,
                        draft.body.as_deref(),
                        recipient_is_valid,
                    ),
                ) {
                    if sent.as_bool() == Some(true) {
                        break;
                    }
                }
            }
            if std::time::Instant::now() >= deadline {
                return Err(WebsiteDriverError::ReadFailed);
            }
            thread::sleep(Duration::from_millis(100));
        }
        if !recipient_is_valid {
            return Err(WebsiteDriverError::MalformedRecipient);
        }
        Ok(WebsiteEmailSendReceipt {
            page: page.clone(),
            recipient: draft.recipient,
        })
    }
}

impl Drop for RealBrowserWebsiteDriver {
    fn drop(&mut self) {
        let _ = self.browser.kill();
        let _ = self.browser.wait();
        let _ = fs::remove_dir_all(&self.profile_dir);
    }
}

fn wait_for_devtools(
    client: &reqwest::blocking::Client,
    base_url: &str,
    timeout: Duration,
) -> Result<(), WebsiteDriverError> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if client
            .get(format!("{base_url}/json/version"))
            .send()
            .and_then(|response| response.error_for_status())
            .is_ok()
        {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(WebsiteDriverError::BrowserConnectionFailed);
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn read_page_snapshot(websocket_url: &str) -> Result<BrowserPageSnapshot, WebsiteDriverError> {
    let value = evaluate_target(websocket_url, PAGE_SNAPSHOT_EXPRESSION)?;
    serde_json::from_value(value).map_err(|_| WebsiteDriverError::ReadFailed)
}

fn place_text_in_first_named_editable(
    websocket_url: &str,
    text: &str,
) -> Result<BrowserTextPlacementResult, WebsiteDriverError> {
    let text = serde_json::to_string(text).map_err(|_| WebsiteDriverError::TextPlacementFailed)?;
    let expression = PLACE_TEXT_EXPRESSION.replace("__OSL_TEXT__", &text);
    let value = evaluate_target(websocket_url, &expression)?;
    serde_json::from_value(value).map_err(|_| WebsiteDriverError::TextPlacementFailed)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NamedButtonPress {
    Pressed,
    Disabled,
    NotFound,
}

fn press_named_button(
    websocket_url: &str,
    name: &str,
) -> Result<NamedButtonPress, WebsiteDriverError> {
    let name = serde_json::to_string(name).map_err(|_| WebsiteDriverError::NamedControlNotFound)?;
    let expression = CLICK_NAMED_CONTROL_EXPRESSION.replace("__OSL_CONTROL_NAME__", &name);
    match evaluate_target(websocket_url, &expression)?.as_str() {
        Some("pressed") => Ok(NamedButtonPress::Pressed),
        Some("disabled") => Ok(NamedButtonPress::Disabled),
        Some("not_found") => Ok(NamedButtonPress::NotFound),
        _ => Err(WebsiteDriverError::ReadFailed),
    }
}

fn read_selected_email_snapshot(
    websocket_url: &str,
) -> Result<BrowserSelectedEmailSnapshot, WebsiteDriverError> {
    let value = evaluate_target(websocket_url, SELECTED_EMAIL_EXPRESSION)?;
    serde_json::from_value(value).map_err(|_| WebsiteDriverError::ReadFailed)
}

fn read_selected_email_identity_snapshot(
    websocket_url: &str,
) -> Result<BrowserSelectedEmailIdentitySnapshot, WebsiteDriverError> {
    let value = evaluate_target(websocket_url, SELECTED_EMAIL_IDENTITY_EXPRESSION)?;
    serde_json::from_value(value).map_err(|_| WebsiteDriverError::ReadFailed)
}

fn read_mailbox_snapshot(
    websocket_url: &str,
) -> Result<BrowserMailboxReadSnapshot, WebsiteDriverError> {
    let value = evaluate_target(websocket_url, MAILBOX_READ_EXPRESSION)?;
    serde_json::from_value(value).map_err(|_| WebsiteDriverError::ReadFailed)
}

fn read_live_run_progress_snapshot(
    websocket_url: &str,
) -> Result<WebsiteLiveRunProgress, WebsiteDriverError> {
    let value = evaluate_target(websocket_url, LIVE_RUN_PROGRESS_EXPRESSION)?;
    serde_json::from_value(value).map_err(|_| WebsiteDriverError::ReadFailed)
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn evaluate_target(
    websocket_url: &str,
    expression: &str,
) -> Result<serde_json::Value, WebsiteDriverError> {
    let mut socket = open_devtools_websocket(websocket_url)?;
    let request = serde_json::json!({
        "id": 1,
        "method": "Runtime.evaluate",
        "params": {
            "expression": expression,
            "returnByValue": true,
            "awaitPromise": true
        }
    })
    .to_string();
    write_websocket_text_frame(&mut socket, request.as_bytes())?;

    loop {
        let message = read_websocket_text_message(&mut socket)?;
        let response: serde_json::Value =
            serde_json::from_slice(&message).map_err(|_| WebsiteDriverError::ReadFailed)?;
        if response.get("id").and_then(serde_json::Value::as_u64) != Some(1) {
            continue;
        }
        if response.get("exceptionDetails").is_some() {
            return Err(WebsiteDriverError::ReadFailed);
        }
        return response
            .get("result")
            .and_then(|result| result.get("result"))
            .and_then(|result| result.get("value"))
            .cloned()
            .ok_or(WebsiteDriverError::ReadFailed);
    }
}

fn open_devtools_websocket(websocket_url: &str) -> Result<TcpStream, WebsiteDriverError> {
    let url = url::Url::parse(websocket_url).map_err(|_| WebsiteDriverError::ReadFailed)?;
    if url.scheme() != "ws" {
        return Err(WebsiteDriverError::ReadFailed);
    }
    let host = url.host_str().ok_or(WebsiteDriverError::ReadFailed)?;
    let port = url
        .port_or_known_default()
        .ok_or(WebsiteDriverError::ReadFailed)?;
    let path = match url.query() {
        Some(query) => format!("{}?{query}", url.path()),
        None => url.path().to_owned(),
    };
    let mut stream = TcpStream::connect((host, port))
        .map_err(|_| WebsiteDriverError::BrowserConnectionFailed)?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|_| WebsiteDriverError::BrowserConnectionFailed)?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|_| WebsiteDriverError::BrowserConnectionFailed)?;

    let key_bytes = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| WebsiteDriverError::BrowserConnectionFailed)?
        .as_nanos()
        .to_le_bytes();
    let key = base64::engine::general_purpose::STANDARD.encode(key_bytes);
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|_| WebsiteDriverError::BrowserConnectionFailed)?;

    let mut response = Vec::new();
    let mut byte = [0_u8; 1];
    while !response.ends_with(b"\r\n\r\n") {
        stream
            .read_exact(&mut byte)
            .map_err(|_| WebsiteDriverError::BrowserConnectionFailed)?;
        response.push(byte[0]);
        if response.len() > 8192 {
            return Err(WebsiteDriverError::BrowserConnectionFailed);
        }
    }
    let response_text =
        std::str::from_utf8(&response).map_err(|_| WebsiteDriverError::BrowserConnectionFailed)?;
    if !response_text.starts_with("HTTP/1.1 101") && !response_text.starts_with("HTTP/1.0 101") {
        return Err(WebsiteDriverError::BrowserConnectionFailed);
    }

    Ok(stream)
}

fn write_websocket_text_frame(
    stream: &mut TcpStream,
    payload: &[u8],
) -> Result<(), WebsiteDriverError> {
    let mut frame = Vec::new();
    frame.push(0x81);
    if payload.len() <= 125 {
        frame.push(0x80 | payload.len() as u8);
    } else if payload.len() <= u16::MAX as usize {
        frame.push(0x80 | 126);
        frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    } else {
        return Err(WebsiteDriverError::ReadFailed);
    }

    let mask = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| WebsiteDriverError::ReadFailed)?
        .as_nanos()
        .to_le_bytes();
    let mask = [mask[0], mask[1], mask[2], mask[3]];
    frame.extend_from_slice(&mask);
    frame.extend(
        payload
            .iter()
            .enumerate()
            .map(|(index, byte)| byte ^ mask[index % 4]),
    );
    stream
        .write_all(&frame)
        .map_err(|_| WebsiteDriverError::BrowserConnectionFailed)
}

fn read_websocket_text_message(stream: &mut TcpStream) -> Result<Vec<u8>, WebsiteDriverError> {
    let mut message = Vec::new();
    loop {
        let mut header = [0_u8; 2];
        stream
            .read_exact(&mut header)
            .map_err(|_| WebsiteDriverError::BrowserConnectionFailed)?;
        let final_fragment = header[0] & 0x80 != 0;
        let opcode = header[0] & 0x0f;
        let masked = header[1] & 0x80 != 0;
        let mut length = u64::from(header[1] & 0x7f);
        if length == 126 {
            let mut extended = [0_u8; 2];
            stream
                .read_exact(&mut extended)
                .map_err(|_| WebsiteDriverError::BrowserConnectionFailed)?;
            length = u64::from(u16::from_be_bytes(extended));
        } else if length == 127 {
            let mut extended = [0_u8; 8];
            stream
                .read_exact(&mut extended)
                .map_err(|_| WebsiteDriverError::BrowserConnectionFailed)?;
            length = u64::from_be_bytes(extended);
        }
        if length > 1_000_000 {
            return Err(WebsiteDriverError::ReadFailed);
        }

        let mut mask = [0_u8; 4];
        if masked {
            stream
                .read_exact(&mut mask)
                .map_err(|_| WebsiteDriverError::BrowserConnectionFailed)?;
        }
        let mut payload = vec![0_u8; length as usize];
        stream
            .read_exact(&mut payload)
            .map_err(|_| WebsiteDriverError::BrowserConnectionFailed)?;
        if masked {
            for (index, byte) in payload.iter_mut().enumerate() {
                *byte ^= mask[index % 4];
            }
        }

        match opcode {
            0x1 | 0x0 => message.extend_from_slice(&payload),
            0x8 => return Err(WebsiteDriverError::BrowserConnectionFailed),
            0x9 | 0xa => continue,
            _ => return Err(WebsiteDriverError::ReadFailed),
        }

        if final_fragment {
            return Ok(message);
        }
    }
}

const PAGE_SNAPSHOT_EXPRESSION: &str = r#"
(() => {
  const compact = (value) => String(value || '').replace(/\s+/g, ' ').trim();
  const visible = (element) => {
    if (!element || element.hidden || element.getAttribute('aria-hidden') === 'true') return false;
    const style = window.getComputedStyle(element);
    if (style.display === 'none' || style.visibility === 'hidden' || style.visibility === 'collapse' || style.opacity === '0') return false;
    return element.getClientRects().length > 0;
  };
  const labelledBy = (element) => compact(
    (element.getAttribute('aria-labelledby') || '')
      .split(/\s+/)
      .map((id) => document.getElementById(id))
      .filter(Boolean)
      .map((label) => label.innerText || label.textContent || '')
      .join(' ')
  );
  const controlName = (element) => {
    const candidates = [
      element.getAttribute('aria-label'),
      labelledBy(element),
      element.getAttribute('title'),
      element.getAttribute('placeholder'),
      element.value,
      element.innerText || element.textContent,
      element.getAttribute('name'),
      element.id
    ];
    for (const candidate of candidates) {
      const name = compact(candidate);
      if (name) return name;
    }
    return '';
  };
  const uniqueNames = (selector, predicate) => {
    const names = [];
    const seen = new Set();
    for (const element of document.querySelectorAll(selector)) {
      if (!visible(element) || (predicate && !predicate(element))) continue;
      const name = controlName(element);
      if (!name || seen.has(name)) continue;
      seen.add(name);
      names.push(name);
    }
    return names;
  };
  const editable = (element) => {
    if (element.disabled || element.readOnly || element.getAttribute('aria-disabled') === 'true') return false;
    if (element.isContentEditable) return true;
    const tag = element.tagName.toLowerCase();
    if (tag === 'textarea') return true;
    if (tag !== 'input') return element.getAttribute('role') === 'textbox' || element.getAttribute('role') === 'searchbox';
    const type = (element.getAttribute('type') || 'text').toLowerCase();
    return !['button', 'checkbox', 'color', 'file', 'hidden', 'image', 'radio', 'range', 'reset', 'submit'].includes(type);
  };
  return {
    title: document.title,
    text: compact(document.body ? document.body.innerText : ''),
    controls: {
      editable_boxes: uniqueNames('input, textarea, [contenteditable=""], [contenteditable="true"], [role="textbox"], [role="searchbox"]', editable),
      buttons: uniqueNames('button, input[type="button"], input[type="submit"], input[type="reset"], [role="button"]'),
      visible_message_areas: uniqueNames('[role="log"], [role="feed"], [aria-live], [data-osl-message-area]')
    }
  };
})()
"#;

const PLACE_TEXT_EXPRESSION: &str = r#"
(() => {
  const text = __OSL_TEXT__;
  const compact = (value) => String(value || '').replace(/\s+/g, ' ').trim();
  const visible = (element) => {
    if (!element || element.hidden || element.getAttribute('aria-hidden') === 'true') return false;
    const style = window.getComputedStyle(element);
    if (style.display === 'none' || style.visibility === 'hidden' || style.visibility === 'collapse' || style.opacity === '0') return false;
    return element.getClientRects().length > 0;
  };
  const labelledBy = (element) => compact(
    (element.getAttribute('aria-labelledby') || '')
      .split(/\s+/)
      .map((id) => document.getElementById(id))
      .filter(Boolean)
      .map((label) => label.innerText || label.textContent || '')
      .join(' ')
  );
  const controlName = (element) => {
    const candidates = [
      element.getAttribute('aria-label'),
      labelledBy(element),
      element.getAttribute('title'),
      element.getAttribute('placeholder'),
      element.getAttribute('name'),
      element.id
    ];
    for (const candidate of candidates) {
      const name = compact(candidate);
      if (name) return name;
    }
    return '';
  };
  const editable = (element) => {
    if (element.disabled || element.readOnly || element.getAttribute('aria-disabled') === 'true') return false;
    if (element.isContentEditable) return true;
    const tag = element.tagName.toLowerCase();
    if (tag === 'textarea') return true;
    if (tag !== 'input') return element.getAttribute('role') === 'textbox' || element.getAttribute('role') === 'searchbox';
    const type = (element.getAttribute('type') || 'text').toLowerCase();
    return !['button', 'checkbox', 'color', 'file', 'hidden', 'image', 'radio', 'range', 'reset', 'submit'].includes(type);
  };
  const read = (element) => element.isContentEditable ? (element.innerText || element.textContent || '') : String(element.value || '');
  const write = (element) => {
    element.focus();
    if (element.isContentEditable) {
      element.textContent = text;
    } else {
      element.value = text;
    }
    element.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText', data: text }));
    element.dispatchEvent(new Event('change', { bubbles: true }));
    return read(element);
  };
  const matches = [];
  for (const element of document.querySelectorAll('input, textarea, [contenteditable=""], [contenteditable="true"], [role="textbox"], [role="searchbox"]')) {
    if (!visible(element) || !editable(element)) continue;
    matches.push(element);
  }
  if (matches.length !== 1) return { placed: false, readback: '', editable_name: '' };
  const readback = write(matches[0]);
  return { placed: readback === text, readback, editable_name: controlName(matches[0]) };
})()
"#;

const CLICK_NAMED_CONTROL_EXPRESSION: &str = r#"
(() => {
  const wanted = __OSL_CONTROL_NAME__;
  const compact = (value) => String(value || '').replace(/\s+/g, ' ').trim();
  const visible = (element) => {
    if (!element || element.hidden || element.getAttribute('aria-hidden') === 'true') return false;
    const style = window.getComputedStyle(element);
    if (style.display === 'none' || style.visibility === 'hidden' || style.visibility === 'collapse' || style.opacity === '0') return false;
    return element.getClientRects().length > 0;
  };
  const enabled = (element) => !element.disabled && element.getAttribute('aria-disabled') !== 'true';
  const labelledBy = (element) => compact(
    (element.getAttribute('aria-labelledby') || '')
      .split(/\s+/)
      .map((id) => document.getElementById(id))
      .filter(Boolean)
      .map((label) => label.innerText || label.textContent || '')
      .join(' ')
  );
  const controlName = (element) => {
    const candidates = [
      element.getAttribute('aria-label'),
      labelledBy(element),
      element.getAttribute('title'),
      element.getAttribute('value'),
      element.value,
      element.innerText || element.textContent,
      element.getAttribute('name'),
      element.id
    ];
    for (const candidate of candidates) {
      const name = compact(candidate);
      if (name) return name;
    }
    return '';
  };
  const matches = [];
  const disabled = [];
  for (const element of document.querySelectorAll('button, input[type="button"], input[type="submit"], input[type="reset"], [role="button"]')) {
    if (!visible(element) || controlName(element) !== wanted) continue;
    if (enabled(element)) {
      matches.push(element);
    } else {
      disabled.push(element);
    }
  }
  if (matches.length === 0 && disabled.length > 0) return 'disabled';
  if (matches.length !== 1) return 'not_found';
  matches[0].click();
  return 'pressed';
})()
"#;

const SELECTED_EMAIL_EXPRESSION: &str = r#"
(() => {
  const compact = (value) => String(value || '').replace(/\s+/g, ' ').trim();
  const visible = (element) => {
    if (!element || element.hidden || element.getAttribute('aria-hidden') === 'true') return false;
    const style = window.getComputedStyle(element);
    if (style.display === 'none' || style.visibility === 'hidden' || style.opacity === '0') return false;
    return element.getClientRects().length > 0;
  };
  const firstVisible = (root, selector) => {
    for (const element of root.querySelectorAll(selector)) {
      if (visible(element)) return element;
    }
    return null;
  };
  const selected = firstVisible(document, [
    '[data-osl-open-email="true"]',
    '[data-osl-selected-email="true"]',
    '[data-selected-email="true"]',
    '[role="article"][aria-selected="true"]',
    '[role="document"][aria-selected="true"]',
    '[aria-current="true"][data-osl-email]'
  ].join(', '));
  if (!selected) return null;

  const identityAttrs = [
    'data-osl-thread-id',
    'data-thread-id',
    'data-osl-conversation-id',
    'data-conversation-id',
    'data-message-thread-id',
    'data-email-thread-id'
  ];
  let conversationIdentity = '';
  for (let current = selected; current && !conversationIdentity; current = current.parentElement) {
    for (const attr of identityAttrs) {
      conversationIdentity = compact(current.getAttribute(attr));
      if (conversationIdentity) break;
    }
  }

  const messageId =
    compact(selected.getAttribute('data-osl-message-id')) ||
    compact(selected.getAttribute('data-message-id')) ||
    compact(selected.id) ||
    conversationIdentity;
  const bodyElement =
    firstVisible(selected, '[data-osl-email-body], [data-email-body], [data-message-body], [role="document"]') ||
    selected;
  const body = compact(bodyElement.innerText || bodyElement.textContent || '');
  if (!body || !conversationIdentity || !messageId) return null;
  return {
    message_id: messageId,
    body,
    conversation_identity: conversationIdentity
  };
})()
"#;

const SELECTED_EMAIL_IDENTITY_EXPRESSION: &str = r#"
(() => {
  const compact = (value) => String(value || '').replace(/\s+/g, ' ').trim();
  const visible = (element) => {
    if (!element || element.hidden || element.getAttribute('aria-hidden') === 'true') return false;
    const style = window.getComputedStyle(element);
    if (style.display === 'none' || style.visibility === 'hidden' || style.opacity === '0') return false;
    return element.getClientRects().length > 0;
  };
  const matchingVisible = (selector) =>
    Array.from(document.querySelectorAll(selector)).filter(visible);
  const readAncestorAttribute = (element, attrs) => {
    for (let current = element; current; current = current.parentElement) {
      for (const attr of attrs) {
        const value = compact(current.getAttribute(attr));
        if (value) return value;
      }
    }
    return '';
  };

  const selected = matchingVisible([
    '[data-osl-open-email="true"]',
    '[data-osl-selected-email="true"]',
    '[data-selected-email="true"]',
    '[role="article"][aria-selected="true"]',
    '[role="document"][aria-selected="true"]',
    '[aria-current="true"][data-osl-email]'
  ].join(', '));
  if (selected.length !== 1) return null;

  const folders = matchingVisible([
    '[data-osl-current-folder-id]',
    '[data-osl-current-label-id]',
    '[data-current-folder-id]',
    '[data-current-label-id]',
    '[aria-current="page"][data-osl-folder-id]',
    '[aria-current="page"][data-osl-label-id]'
  ].join(', '));
  if (folders.length !== 1) return null;

  const threadIdentity = readAncestorAttribute(selected[0], [
    'data-osl-thread-id',
    'data-thread-id',
    'data-osl-conversation-id',
    'data-conversation-id',
    'data-message-thread-id',
    'data-email-thread-id'
  ]);
  const folderIdentity =
    compact(folders[0].getAttribute('data-osl-current-folder-id')) ||
    compact(folders[0].getAttribute('data-osl-current-label-id')) ||
    compact(folders[0].getAttribute('data-current-folder-id')) ||
    compact(folders[0].getAttribute('data-current-label-id')) ||
    compact(folders[0].getAttribute('data-osl-folder-id')) ||
    compact(folders[0].getAttribute('data-osl-label-id'));

  if (!threadIdentity || !folderIdentity) return null;
  return {
    thread_identity: threadIdentity,
    folder_identity: folderIdentity
  };
})()
"#;

const LIVE_RUN_PROGRESS_EXPRESSION: &str = r#"
(() => {
  const compact = (value) => String(value || '').replace(/\s+/g, ' ').trim();
  const root = document.querySelector('[data-osl-live-run-progress]');
  if (!root) return null;
  const valueFor = (name) => {
    const fromRoot = root.getAttribute(`data-osl-${name}`);
    if (fromRoot !== null) return fromRoot;
    const element = document.querySelector(`[data-osl-${name}]`);
    return element ? element.getAttribute(`data-osl-${name}`) : '';
  };
  const numberFor = (name) => {
    const parsed = Number.parseInt(valueFor(name), 10);
    return Number.isFinite(parsed) && parsed >= 0 ? parsed : 0;
  };
  return {
    activeAccount: compact(valueFor('active-account')),
    currentPlace: compact(valueFor('current-place')),
    messagesChecked: numberFor('messages-checked'),
    matches: numberFor('matches'),
    scrolls: numberFor('scrolls'),
    waits: numberFor('waits'),
    changes: numberFor('changes')
  };
})()
"#;

const MAILBOX_READ_EXPRESSION: &str = r#"
(() => {
  const compact = (value) => String(value || '').replace(/\s+/g, ' ').trim();
  const visible = (element) => {
    if (!element || element.hidden || element.getAttribute('aria-hidden') === 'true') return false;
    const style = window.getComputedStyle(element);
    if (style.display === 'none' || style.visibility === 'hidden' || style.opacity === '0') return false;
    return element.getClientRects().length > 0;
  };
  const attrOrChild = (root, attr, selector) => {
    const direct = compact(root.getAttribute(attr));
    if (direct) return direct;
    const element = root.querySelector(selector);
    if (!element) return '';
    return compact(element.getAttribute(attr) || element.innerText || element.textContent);
  };
  const folders = [];
  const seenFolders = new Set();
  for (const element of document.querySelectorAll('[data-osl-mail-folder], [data-proton-folder]')) {
    if (!visible(element)) continue;
    const folder = compact(element.getAttribute('data-osl-mail-folder') || element.getAttribute('data-proton-folder') || element.innerText || element.textContent);
    if (!folder || seenFolders.has(folder)) continue;
    seenFolders.add(folder);
    folders.push(folder);
  }
  const messages = [];
  for (const row of document.querySelectorAll('[data-osl-mail-message], [data-proton-message-row]')) {
    if (!visible(row)) continue;
    const folder = attrOrChild(row, 'data-osl-folder', '[data-osl-mail-message-folder], [data-proton-message-folder]') ||
      compact(row.getAttribute('data-proton-folder'));
    const subject = attrOrChild(row, 'data-osl-subject', '[data-osl-mail-subject], [data-proton-subject]') ||
      compact(row.getAttribute('data-proton-subject'));
    const time = attrOrChild(row, 'data-osl-time', '[data-osl-mail-time], [data-proton-time], time') ||
      compact(row.getAttribute('data-proton-time'));
    const sender = attrOrChild(row, 'data-osl-sender', '[data-osl-mail-sender], [data-proton-sender]') ||
      compact(row.getAttribute('data-proton-sender'));
    const markerElement = row.querySelector('[data-osl-scrub-owner-marker], [data-proton-owner-marker]');
    const ownerMarker = compact(row.getAttribute('data-osl-scrub-owner-marker') || row.getAttribute('data-proton-owner-marker') ||
      (markerElement ? markerElement.getAttribute('data-osl-scrub-owner-marker') || markerElement.getAttribute('data-proton-owner-marker') || markerElement.innerText || markerElement.textContent : ''));
    if (!folder || !subject || !time || !sender) continue;
    messages.push({
      folder,
      subject,
      time,
      sender,
      ownerMarker,
      yours: /^(SCRUB-PR-MINE|SCRUB-IC-MINE)$/.test(ownerMarker)
    });
  }
  if (!folders.length) return null;
  return { folders, messages };
})()
"#;

fn email_send_expression(recipient: &str, body: Option<&str>, should_send: bool) -> String {
    let recipient = serde_json::to_string(recipient).unwrap_or_else(|_| "\"\"".to_owned());
    let body = body
        .map(|body| serde_json::to_string(body).unwrap_or_else(|_| "\"\"".to_owned()))
        .unwrap_or_else(|| "null".to_owned());
    let should_send = if should_send { "true" } else { "false" };
    format!(
        r#"
(async () => {{
  const recipient = {recipient};
  const body = {body};
  const shouldSend = {should_send};
  const visible = (element) => {{
    if (!element || element.disabled || element.hidden || element.getAttribute('aria-hidden') === 'true') return false;
    const style = window.getComputedStyle(element);
    return style.display !== 'none' && style.visibility !== 'hidden' && style.opacity !== '0' && element.getClientRects().length > 0;
  }};
  const firstVisible = (selector) => Array.from(document.querySelectorAll(selector)).find(visible);
  const setText = (element, value) => {{
    element.focus();
    if (element.isContentEditable) {{
      element.textContent = value;
    }} else {{
      element.value = value;
    }}
    element.dispatchEvent(new InputEvent('input', {{ bubbles: true, inputType: 'insertText', data: value }}));
    element.dispatchEvent(new Event('change', {{ bubbles: true }}));
  }};

  const to = firstVisible([
    '[data-osl-email-to]',
    'input[name="to"]',
    'input[type="email"]',
    '[role="textbox"][aria-label="To"]',
    '[aria-label="To"]'
  ].join(', '));
  const send = firstVisible([
    '[data-osl-email-send]',
    'button[type="submit"]',
    'button[aria-label="Send"]',
    '[role="button"][aria-label="Send"]'
  ].join(', '));
  if (!to || !send) return false;
  setText(to, recipient);

  if (body !== null) {{
    const bodyElement = firstVisible([
      '[data-osl-email-body-input]',
      'textarea[name="body"]',
      'textarea[aria-label="Body"]',
      '[contenteditable="true"][aria-label="Body"]',
      '[role="textbox"][aria-label="Body"]'
    ].join(', '));
    if (!bodyElement) return false;
    setText(bodyElement, body);
  }}

  if (!shouldSend) return true;
  send.click();
  if (window.__oslLastSendPromise && typeof window.__oslLastSendPromise.then === 'function') {{
    await window.__oslLastSendPromise;
  }} else {{
    await new Promise((resolve) => setTimeout(resolve, 100));
  }}
  return true;
}})()
"#
    )
}

fn valid_email_recipient(recipient: &str) -> bool {
    if recipient.len() > 254
        || recipient
            .chars()
            .any(|ch| ch.is_control() || ch.is_whitespace())
    {
        return false;
    }
    let Some((local, domain)) = recipient.split_once('@') else {
        return false;
    };
    if local.is_empty() || domain.is_empty() || domain.ends_with('.') || !domain.contains('.') {
        return false;
    }
    if domain.contains('@') || local.contains('@') {
        return false;
    }
    domain.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    })
}

fn reserve_loopback_port() -> Result<u16, WebsiteDriverError> {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .map_err(|_| WebsiteDriverError::BrowserLaunchFailed)
}

fn make_profile_dir() -> Result<PathBuf, WebsiteDriverError> {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| WebsiteDriverError::BrowserLaunchFailed)?;
    let path = std::env::temp_dir().join(format!(
        "osl-website-driver-{}-{}",
        std::process::id(),
        since_epoch.as_nanos()
    ));
    fs::create_dir_all(&path).map_err(|_| WebsiteDriverError::BrowserLaunchFailed)?;
    Ok(path)
}

fn discover_browser_executable() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("OSL_WEBSITE_DRIVER_BROWSER") {
        let candidate = PathBuf::from(path);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    if let Ok(path) = std::env::var("OSL_WEBSITE_DRIVER_CHROME") {
        let candidate = PathBuf::from(path);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    for name in [
        "chromium",
        "chromium-browser",
        "google-chrome",
        "google-chrome-stable",
    ] {
        if let Some(path) = find_on_path(name) {
            return Some(path);
        }
    }

    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    let playwright_root = home.join(".cache").join("ms-playwright");
    let mut candidates = fs::read_dir(playwright_root)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path().join("chrome-linux64").join("chrome"))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    candidates.sort();
    candidates.pop()
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}
