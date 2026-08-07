//! Fixed website driver contract for provider-backed email surfaces.
//!
//! Higher-level readers call these narrow verbs instead of accepting message
//! bodies or provider state from the renderer.

use core::fmt;
//! Real website driver contract and browser-backed implementation.
//!
//! The implementation that talks to a browser lives behind this interface. The
//! backend job list is fixed here so higher-level website work cannot smuggle in
//! generic browser automation verbs.
//! Real website driver contract and browser-backed open email identity read.

use base64::Engine;
use core::fmt;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use base64::Engine;
use core::fmt;
use serde::{Deserialize, Serialize};
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
pub enum WebsiteDriverJob {
    FindPage,
    ReadPage,
    PlaceText,
    PressNamedControl,
    ReadSelectedEmailIdentity,
    SendEmailDraft,
}

impl WebsiteDriverJob {
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::FindPage => "find_page",
            Self::ReadPage => "read_page",
            Self::PlaceText => "place_text",
            Self::PressNamedControl => "press_named_control",
//! Real browser-backed website driver.
//!
//! This driver is intentionally small: it starts a real Chromium-family
//! browser with an isolated profile, opens a target through Chrome DevTools
//! HTTP, and reads page metadata back from that browser. Tests may define their
//! own fakes, but production-facing app code should construct this driver when
//! it needs website automation evidence.

use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use url::Url;

const DRIVER_START_TIMEOUT: Duration = Duration::from_secs(10);
const PAGE_READ_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum WebsiteDriverKind {
    RealBrowser,
    FakeTestBrowser,
}

impl WebsiteDriverKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RealBrowser => "realBrowser",
            Self::FakeTestBrowser => "fakeTestBrowser",
            Self::ReadSelectedEmailIdentity => "read_selected_email_identity",
            Self::SendEmailDraft => "send_email_draft",
        }
    }
}

pub const WEBSITE_DRIVER_JOBS: [WebsiteDriverJob; 4] = [
    WebsiteDriverJob::FindPage,
    WebsiteDriverJob::ReadPage,
    WebsiteDriverJob::PlaceText,
    WebsiteDriverJob::PressNamedControl,
pub const WEBSITE_DRIVER_JOBS: [WebsiteDriverJob; 3] = [
    WebsiteDriverJob::FindPage,
    WebsiteDriverJob::ReadSelectedEmailIdentity,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsiteSelectedEmail {
    pub page: WebsitePage,
    pub message_id: String,
    pub body: String,
    pub conversation_identity: String,
    target_id: Option<String>,
    target_id: Option<String>,
}

impl WebsitePage {
    pub fn synthetic(url: String) -> Self {
        Self {
            url,
            target_id: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsiteTextPlacement {
    pub page: WebsitePage,
    pub editable_box_name: String,
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsiteNamedControl {
    pub page: WebsitePage,
    pub name: String,
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
    pub body: String,
    pub conversation_identity: String,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebsiteDriverError {
    PageNotFound,
    ReadFailed,
    NoSelectedMessage,
    BrowserUnavailable,
    BrowserLaunchFailed,
    BrowserConnectionFailed,
    PageNotFound,
    ReadFailed,
    TextPlacementFailed,
    NamedControlNotFound,
    NamedControlDisabled,
    MalformedRecipient,
}

impl fmt::Display for WebsiteDriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::PageNotFound => "website page was not found",
            Self::ReadFailed => "website page could not be read",
            Self::NoSelectedMessage => "no selected message",
            Self::BrowserUnavailable => "website browser executable was not found",
            Self::BrowserLaunchFailed => "website browser could not be launched",
            Self::BrowserConnectionFailed => "website browser connection failed",
            Self::PageNotFound => "website page was not found",
            Self::ReadFailed => "website page could not be read",
            Self::TextPlacementFailed => "website text could not be placed",
            Self::NamedControlNotFound => "website named control was not found",
            Self::NamedControlDisabled => "Send disabled",
            Self::MalformedRecipient => "malformed recipient",
        })
    }
}

impl std::error::Error for WebsiteDriverError {}

pub trait WebsiteDriver {
    fn find_page(&mut self, request: WebsitePageRequest)
        -> Result<WebsitePage, WebsiteDriverError>;

    fn read_selected_email(
        &mut self,
        page: &WebsitePage,
    ) -> Result<WebsiteSelectedEmail, WebsiteDriverError>;
    const JOBS: &'static [WebsiteDriverJob] = &WEBSITE_DRIVER_JOBS;

    fn find_page(&mut self, request: WebsitePageRequest)
        -> Result<WebsitePage, WebsiteDriverError>;
    fn read_page(&mut self, page: &WebsitePage) -> Result<WebsitePageText, WebsiteDriverError>;
    fn place_text(
        &mut self,
        placement: WebsiteTextPlacement,
    ) -> Result<WebsitePlacementProof, WebsiteDriverError>;
    fn read_selected_email(
        &mut self,
        page: &WebsitePage,
    ) -> Result<WebsiteSelectedEmail, WebsiteDriverError>;
    fn read_mailbox(
        &mut self,
        page: &WebsitePage,
    ) -> Result<WebsiteMailboxRead, WebsiteDriverError>;
    fn read_live_run_progress(
        &mut self,
        page: &WebsitePage,
    ) -> Result<WebsiteLiveRunProgress, WebsiteDriverError>;
    fn place_text(&mut self, placement: WebsiteTextPlacement) -> Result<(), WebsiteDriverError>;
    fn press_named_control(
        &mut self,
        control: WebsiteNamedControl,
    ) -> Result<(), WebsiteDriverError>;
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
        })?;
        Ok(WebsiteSendReceipt {
            placement_proof,
            send_control_name,
            send_pressed: true,
        })
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WebsitePage {
    pub target_id: String,
    pub url: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WebsitePageSnapshot {
    pub title: String,
    pub url: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
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

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct WebsiteNamedControlRequest {
    pub name: &'static str,
    pub kind: WebsiteControlKind,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WebsiteNamedControl {
    pub name: String,
    pub kind: WebsiteControlKind,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum WebsiteDriverError {
    BrowserUnavailable,
    BrowserLaunchFailed(String),
    DevToolsUnavailable(String),
    PageUnavailable,
    MissingNamedControl(String),
    InvalidUrl,
}

pub trait WebsiteDriver {
    fn kind(&self) -> WebsiteDriverKind;
    fn find_page(&mut self, url: &Url) -> Result<WebsitePage, WebsiteDriverError>;
    fn read_page(&self, page: &WebsitePage) -> Result<WebsitePageSnapshot, WebsiteDriverError>;
    fn read_named_controls(
        &self,
        _page: &WebsitePage,
        _required: &[WebsiteNamedControlRequest],
    ) -> Result<Vec<WebsiteNamedControl>, WebsiteDriverError> {
        Err(WebsiteDriverError::PageUnavailable)
    }

    fn read_selected_email_identity(
        &mut self,
        page: &WebsitePage,
    ) -> Result<WebsiteSelectedEmailIdentity, WebsiteDriverError>;

    fn send_email_draft(
        &mut self,
        page: &WebsitePage,
        draft: WebsiteEmailDraft,
    ) -> Result<WebsiteEmailSendReceipt, WebsiteDriverError>;
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
    title: String,
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
    child: Child,
    profile_dir: PathBuf,
    devtools_base: String,
    client: reqwest::blocking::Client,
    executable: PathBuf,
struct BrowserSelectedEmailSnapshot {
    body: String,
    conversation_identity: String,
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
struct BrowserSelectedEmailIdentitySnapshot {
    thread_identity: String,
    folder_identity: String,
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
            .arg("--disable-background-networking")
            .arg("--disable-dev-shm-usage")
            .arg("--disable-extensions")
            .arg("--disable-gpu")
            .arg("--disable-sync")
            .arg("--no-default-browser-check")
            .arg("--no-first-run")
            .arg(format!("--remote-debugging-port={port}"))
            .arg(format!("--user-data-dir={}", profile_dir.display()))
        let executable = find_browser_executable().ok_or(WebsiteDriverError::BrowserUnavailable)?;
        Self::launch_with_executable(executable)
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }

    fn launch_with_executable(executable: PathBuf) -> Result<Self, WebsiteDriverError> {
        let port = reserve_loopback_port()?;
        let profile_dir = unique_profile_dir();
        fs::create_dir_all(&profile_dir).map_err(|error| {
            WebsiteDriverError::BrowserLaunchFailed(format!(
                "profile directory could not be created: {error}"
            ))
        })?;

        let mut child = Command::new(&executable)
            .arg("--headless=new")
            .arg("--no-sandbox")
            .arg("--disable-gpu")
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .arg(format!("--user-data-dir={}", profile_dir.display()))
            .arg(format!("--remote-debugging-port={port}"))
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
        let target_id = page
            .target_id
            .as_ref()
            .ok_or(WebsiteDriverError::ReadFailed)?;
        self.devtools_targets()?
            .into_iter()
            .find(|target| target.target_type == "page" && &target.id == target_id)
            .and_then(|target| target.web_socket_debugger_url)
            .ok_or(WebsiteDriverError::ReadFailed)
            .map_err(|error| WebsiteDriverError::BrowserLaunchFailed(error.to_string()))?;

        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .map_err(|error| WebsiteDriverError::DevToolsUnavailable(error.to_string()))?;
        let devtools_base = format!("http://127.0.0.1:{port}");
        let deadline = Instant::now() + DRIVER_START_TIMEOUT;
        while Instant::now() < deadline {
            if child.try_wait().ok().flatten().is_some() {
                let _ = fs::remove_dir_all(&profile_dir);
                return Err(WebsiteDriverError::BrowserLaunchFailed(
                    "browser exited before DevTools became available".to_owned(),
                ));
            }
            if client
                .get(format!("{devtools_base}/json/version"))
                .send()
                .and_then(|response| response.error_for_status())
                .is_ok()
            {
                return Ok(Self {
                    child,
                    profile_dir,
                    devtools_base,
                    client,
                    executable,
                });
            }
            thread::sleep(Duration::from_millis(50));
        }

        let _ = child.kill();
        let _ = child.wait();
        let _ = fs::remove_dir_all(&profile_dir);
        Err(WebsiteDriverError::DevToolsUnavailable(
            "timed out waiting for browser DevTools".to_owned(),
        ))
    }
}

impl WebsiteDriver for RealBrowserWebsiteDriver {
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

        Ok(WebsitePage {
            url: request.url,
            target_id: Some(target.id),
        })
    }

    fn read_page(&mut self, page: &WebsitePage) -> Result<WebsitePageText, WebsiteDriverError> {
    fn read_selected_email_identity(
        &mut self,
        page: &WebsitePage,
    ) -> Result<WebsiteSelectedEmailIdentity, WebsiteDriverError> {
        let target_id = page
            .target_id
            .as_ref()
            .ok_or(WebsiteDriverError::ReadFailed)?;
        let deadline = std::time::Instant::now() + Duration::from_secs(10);

        loop {
            let target = self
                .devtools_targets()?
                .into_iter()
                .find(|target| target.target_type == "page" && &target.id == target_id)
                .ok_or(WebsiteDriverError::ReadFailed)?;

            if !target.title.is_empty() {
                let websocket_url = target
                    .web_socket_debugger_url
                    .ok_or(WebsiteDriverError::ReadFailed)?;
                let snapshot = read_page_snapshot(&websocket_url)?;
                return Ok(WebsitePageText {
                    page: page.clone(),
                    title: snapshot.title,
                    text: snapshot.text,
                    controls: snapshot.controls,
                });
            if let Some(websocket_url) = target.web_socket_debugger_url {
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

    fn place_text(
        &mut self,
        placement: WebsiteTextPlacement,
    ) -> Result<WebsitePlacementProof, WebsiteDriverError> {
        let websocket_url = self.page_websocket_url(&placement.page)?;
        let result = place_text_in_named_editable(
            &websocket_url,
            &placement.editable_box_name,
            &placement.text,
        )?;
        if !result.placed || result.readback != placement.text {
            return Err(WebsiteDriverError::TextPlacementFailed);
        }
        Ok(WebsitePlacementProof {
            page: placement.page,
            editable_box_name: placement.editable_box_name,
            utf16_units: placement.text.encode_utf16().count(),
            placed_sha256: sha256_hex(placement.text.as_bytes()),
        })
    fn read_selected_email(
        &mut self,
        page: &WebsitePage,
    ) -> Result<WebsiteSelectedEmail, WebsiteDriverError> {
    fn send_email_draft(
        &mut self,
        page: &WebsitePage,
        draft: WebsiteEmailDraft,
    ) -> Result<WebsiteEmailSendReceipt, WebsiteDriverError> {
        let recipient_is_valid = valid_email_recipient(&draft.recipient);
        let target_id = page
            .target_id
            .as_ref()
            .ok_or(WebsiteDriverError::ReadFailed)?;
        let deadline = std::time::Instant::now() + Duration::from_secs(10);

        loop {
            let target = self
                .devtools_targets()?
                .into_iter()
                .find(|target| target.target_type == "page" && &target.id == target_id)
                .ok_or(WebsiteDriverError::ReadFailed)?;

            if !target.title.is_empty() {
                let websocket_url = target
                    .web_socket_debugger_url
                    .ok_or(WebsiteDriverError::ReadFailed)?;
                if let Ok(snapshot) = read_selected_email_snapshot(&websocket_url) {
                    return Ok(WebsiteSelectedEmail {
                        page: page.clone(),
                        body: snapshot.body,
                        conversation_identity: snapshot.conversation_identity,
                    });
                }
            }

            if let Some(websocket_url) = target.web_socket_debugger_url {
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

    fn place_text(&mut self, placement: WebsiteTextPlacement) -> Result<(), WebsiteDriverError> {
        let websocket_url = self.page_websocket_url(&placement.page)?;
        let text =
            serde_json::to_string(&placement.text).map_err(|_| WebsiteDriverError::ReadFailed)?;
        let expression = PLACE_TEXT_EXPRESSION.replace("__OSL_TEXT__", &text);
        match evaluate_target(&websocket_url, &expression)? {
            serde_json::Value::Bool(true) => Ok(()),
            _ => Err(WebsiteDriverError::TextPlacementFailed),
        }
    }

    fn press_named_control(
        &mut self,
        control: WebsiteNamedControl,
    ) -> Result<(), WebsiteDriverError> {
        let websocket_url = self.page_websocket_url(&control.page)?;
        let pressed = press_named_button(&websocket_url, &control.name)?;
        pressed
            .then_some(())
            .ok_or(WebsiteDriverError::NamedControlNotFound)
        match press_named_button(&websocket_url, &control.name)? {
            NamedButtonPress::Pressed => Ok(()),
            NamedButtonPress::Disabled => Err(WebsiteDriverError::NamedControlDisabled),
            NamedButtonPress::NotFound => Err(WebsiteDriverError::NamedControlNotFound),
        }
    fn kind(&self) -> WebsiteDriverKind {
        WebsiteDriverKind::RealBrowser
    }

    fn find_page(&mut self, url: &Url) -> Result<WebsitePage, WebsiteDriverError> {
        let encoded: String =
            url::form_urlencoded::byte_serialize(url.as_str().as_bytes()).collect();
        let target: DevToolsTarget = self
            .client
            .put(format!("{}/json/new?{encoded}", self.devtools_base))
            .send()
            .and_then(|response| response.error_for_status())
            .map_err(|error| WebsiteDriverError::DevToolsUnavailable(error.to_string()))?
            .json()
            .map_err(|error| WebsiteDriverError::DevToolsUnavailable(error.to_string()))?;
        if target.id.is_empty() {
            return Err(WebsiteDriverError::PageUnavailable);
        }
        Ok(WebsitePage {
            target_id: target.id,
            url: url.to_string(),
        })
    }

    fn read_page(&self, page: &WebsitePage) -> Result<WebsitePageSnapshot, WebsiteDriverError> {
        let deadline = Instant::now() + PAGE_READ_TIMEOUT;
        let placeholder_title = Url::parse(&page.url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned));
        while Instant::now() < deadline {
            let targets: Vec<DevToolsTarget> = self
                .client
                .get(format!("{}/json/list", self.devtools_base))
                .send()
                .and_then(|response| response.error_for_status())
                .map_err(|error| WebsiteDriverError::DevToolsUnavailable(error.to_string()))?
                .json()
                .map_err(|error| WebsiteDriverError::DevToolsUnavailable(error.to_string()))?;
            if let Some(target) = targets.iter().find(|target| target.id == page.target_id) {
                if target.url == page.url
                    && !target.title.is_empty()
                    && Some(target.title.as_str()) != placeholder_title.as_deref()
                {
                    return Ok(WebsitePageSnapshot {
                        title: target.title.clone(),
                        url: target.url.clone(),
                    });
                }
            }
            thread::sleep(Duration::from_millis(50));
        }
        Err(WebsiteDriverError::PageUnavailable)
        let name =
            serde_json::to_string(&control.name).map_err(|_| WebsiteDriverError::ReadFailed)?;
        let expression = CLICK_NAMED_CONTROL_EXPRESSION.replace("__OSL_CONTROL_NAME__", &name);
        match evaluate_target(&websocket_url, &expression)? {
            serde_json::Value::Bool(true) => Ok(()),
            _ => Err(WebsiteDriverError::NamedControlNotFound),
        }
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
        let _ = self.child.kill();
        let _ = self.child.wait();
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

fn place_text_in_named_editable(
    websocket_url: &str,
    name: &str,
    text: &str,
) -> Result<BrowserTextPlacementResult, WebsiteDriverError> {
    let name = serde_json::to_string(name).map_err(|_| WebsiteDriverError::TextPlacementFailed)?;
    let text = serde_json::to_string(text).map_err(|_| WebsiteDriverError::TextPlacementFailed)?;
    let expression = format!(
        r#"
(() => {{
  const wanted = {name};
  const text = {text};
  const compact = (value) => String(value || '').replace(/\s+/g, ' ').trim();
  const visible = (element) => {{
    if (!element || element.hidden || element.getAttribute('aria-hidden') === 'true') return false;
    const style = window.getComputedStyle(element);
    if (style.display === 'none' || style.visibility === 'hidden' || style.visibility === 'collapse' || style.opacity === '0') return false;
    return element.getClientRects().length > 0;
  }};
  const enabled = (element) => !element.disabled && !element.readOnly && element.getAttribute('aria-disabled') !== 'true';
  const labelledBy = (element) => compact(
    (element.getAttribute('aria-labelledby') || '')
      .split(/\s+/)
      .map((id) => document.getElementById(id))
      .filter(Boolean)
      .map((label) => label.innerText || label.textContent || '')
      .join(' ')
  );
  const controlName = (element) => {{
    const candidates = [
      element.getAttribute('aria-label'),
      labelledBy(element),
      element.getAttribute('title'),
      element.getAttribute('placeholder'),
      element.getAttribute('name'),
      element.id
    ];
    for (const candidate of candidates) {{
      const name = compact(candidate);
      if (name) return name;
    }}
    return '';
  }};
  const editable = (element) => {{
    if (!enabled(element)) return false;
    if (element.isContentEditable) return true;
    const tag = element.tagName.toLowerCase();
    if (tag === 'textarea') return true;
    if (tag !== 'input') return element.getAttribute('role') === 'textbox' || element.getAttribute('role') === 'searchbox';
    const type = (element.getAttribute('type') || 'text').toLowerCase();
    return !['button', 'checkbox', 'color', 'file', 'hidden', 'image', 'radio', 'range', 'reset', 'submit'].includes(type);
  }};
  const read = (element) => element.isContentEditable ? (element.innerText || element.textContent || '') : String(element.value || '');
  const write = (element) => {{
    element.focus();
    if (element.isContentEditable) {{
      element.textContent = text;
    }} else {{
      element.value = text;
    }}
    element.dispatchEvent(new InputEvent('input', {{ bubbles: true, inputType: 'insertText', data: text }}));
    element.dispatchEvent(new Event('change', {{ bubbles: true }}));
    return read(element);
  }};
  const matches = [];
  for (const element of document.querySelectorAll('input, textarea, [contenteditable=""], [contenteditable="true"], [role="textbox"], [role="searchbox"]')) {{
    if (!visible(element) || !editable(element) || controlName(element) !== wanted) continue;
    matches.push(element);
  }}
  if (matches.length !== 1) return {{ placed: false, readback: '' }};
  const readback = write(matches[0]);
  return {{ placed: readback === text, readback }};
}})()
"#
    );
    let value = evaluate_target(websocket_url, &expression)?;
    serde_json::from_value(value).map_err(|_| WebsiteDriverError::TextPlacementFailed)
}

fn press_named_button(websocket_url: &str, name: &str) -> Result<bool, WebsiteDriverError> {
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
    let expression = format!(
        r#"
(() => {{
  const wanted = {name};
  const compact = (value) => String(value || '').replace(/\s+/g, ' ').trim();
  const visible = (element) => {{
    if (!element || element.hidden || element.getAttribute('aria-hidden') === 'true') return false;
    const style = window.getComputedStyle(element);
    if (style.display === 'none' || style.visibility === 'hidden' || style.visibility === 'collapse' || style.opacity === '0') return false;
    return element.getClientRects().length > 0;
  }};
  const enabled = (element) => !element.disabled && element.getAttribute('aria-disabled') !== 'true';
  const labelledBy = (element) => compact(
    (element.getAttribute('aria-labelledby') || '')
      .split(/\s+/)
      .map((id) => document.getElementById(id))
      .filter(Boolean)
      .map((label) => label.innerText || label.textContent || '')
      .join(' ')
  );
  const controlName = (element) => {{
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
    for (const candidate of candidates) {{
      const name = compact(candidate);
      if (name) return name;
    }}
    return '';
  }};
  const matches = [];
  for (const element of document.querySelectorAll('button, input[type="button"], input[type="submit"], input[type="reset"], [role="button"]')) {{
    if (!visible(element) || !enabled(element) || controlName(element) !== wanted) continue;
    matches.push(element);
  }}
  if (matches.length !== 1) return false;
  matches[0].click();
  return true;
}})()
"#
    );
    evaluate_target(websocket_url, &expression)?
        .as_bool()
        .ok_or(WebsiteDriverError::ReadFailed)
  const disabled = [];
  for (const element of document.querySelectorAll('button, input[type="button"], input[type="submit"], input[type="reset"], [role="button"]')) {{
    if (!visible(element) || controlName(element) !== wanted) continue;
    if (enabled(element)) {{
      matches.push(element);
    }} else {{
      disabled.push(element);
    }}
  }}
  if (matches.length === 0 && disabled.length > 0) return 'disabled';
  if (matches.length !== 1) return 'not_found';
  matches[0].click();
  return 'pressed';
}})()
"#
    );
    match evaluate_target(websocket_url, &expression)?.as_str() {
        Some("pressed") => Ok(NamedButtonPress::Pressed),
        Some("disabled") => Ok(NamedButtonPress::Disabled),
        Some("not_found") => Ok(NamedButtonPress::NotFound),
        _ => Err(WebsiteDriverError::ReadFailed),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
fn read_selected_email_snapshot(
    websocket_url: &str,
) -> Result<BrowserSelectedEmailSnapshot, WebsiteDriverError> {
    let value = evaluate_target(websocket_url, SELECTED_EMAIL_EXPRESSION)?;
    serde_json::from_value(value).map_err(|_| WebsiteDriverError::ReadFailed)
}

fn read_live_run_progress_snapshot(
    websocket_url: &str,
) -> Result<WebsiteLiveRunProgress, WebsiteDriverError> {
    let value = evaluate_target(websocket_url, LIVE_RUN_PROGRESS_EXPRESSION)?;
    serde_json::from_value(value).map_err(|_| WebsiteDriverError::ReadFailed)
}

fn read_mailbox_snapshot(
    websocket_url: &str,
) -> Result<BrowserMailboxReadSnapshot, WebsiteDriverError> {
    let value = evaluate_target(websocket_url, MAILBOX_READ_EXPRESSION)?;
fn read_selected_email_identity_snapshot(
    websocket_url: &str,
) -> Result<BrowserSelectedEmailIdentitySnapshot, WebsiteDriverError> {
    let value = evaluate_target(websocket_url, SELECTED_EMAIL_IDENTITY_EXPRESSION)?;
    serde_json::from_value(value).map_err(|_| WebsiteDriverError::ReadFailed)
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
const SELECTED_EMAIL_IDENTITY_EXPRESSION: &str = r#"
(() => {
  const compact = (value) => String(value || '').replace(/\s+/g, ' ').trim();
  const visible = (element) => {
    if (!element || element.hidden || element.getAttribute('aria-hidden') === 'true') return false;
    const style = window.getComputedStyle(element);
    if (style.display === 'none' || style.visibility === 'hidden' || style.opacity === '0') return false;
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
    if (element.disabled || element.readOnly) return false;
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
  if (!selected) return null;

  const identityAttrs = [
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
  ];
  let conversationIdentity = '';
  for (let current = selected; current && !conversationIdentity; current = current.parentElement) {
    for (const attr of identityAttrs) {
      conversationIdentity = compact(current.getAttribute(attr));
      if (conversationIdentity) break;
    }
  }

  const bodyElement =
    firstVisible(selected, '[data-osl-email-body], [data-email-body], [data-message-body], [role="document"]') ||
    selected;
  const body = compact(bodyElement.innerText || bodyElement.textContent || '');
  if (!body || !conversationIdentity) return null;
  return {
    body,
    conversation_identity: conversationIdentity
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

const PLACE_TEXT_EXPRESSION: &str = r#"
(() => {
  const text = __OSL_TEXT__;
  const visible = (element) => {
    if (!element || element.hidden || element.getAttribute('aria-hidden') === 'true') return false;
    const style = window.getComputedStyle(element);
    if (style.display === 'none' || style.visibility === 'hidden' || style.opacity === '0') return false;
    return element.getClientRects().length > 0;
  };
  const editable = (element) => {
    if (element.disabled || element.readOnly) return false;
    if (element.isContentEditable) return true;
    const tag = element.tagName.toLowerCase();
    if (tag === 'textarea') return true;
    if (tag !== 'input') return element.getAttribute('role') === 'textbox' || element.getAttribute('role') === 'searchbox';
    const type = (element.getAttribute('type') || 'text').toLowerCase();
    return !['button', 'checkbox', 'color', 'file', 'hidden', 'image', 'radio', 'range', 'reset', 'submit'].includes(type);
  };
  const candidates = document.querySelectorAll('textarea, input, [contenteditable=""], [contenteditable="true"], [role="textbox"], [role="searchbox"]');
  for (const element of candidates) {
    if (!visible(element) || !editable(element)) continue;
    element.focus();
    if (element.isContentEditable) {
      element.textContent = text;
    } else {
      element.value = text;
    }
    element.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText', data: text }));
    element.dispatchEvent(new Event('change', { bubbles: true }));
    return true;
  }
  return false;
})()
"#;

const CLICK_NAMED_CONTROL_EXPRESSION: &str = r#"
(() => {
  const expected = __OSL_CONTROL_NAME__;
  const compact = (value) => String(value || '').replace(/\s+/g, ' ').trim();
  const visible = (element) => {
    if (!element || element.hidden || element.getAttribute('aria-hidden') === 'true') return false;
    const style = window.getComputedStyle(element);
    if (style.display === 'none' || style.visibility === 'hidden' || style.opacity === '0') return false;
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
  for (const element of document.querySelectorAll('button, input[type="button"], input[type="submit"], input[type="reset"], [role="button"]')) {
    if (!visible(element) || controlName(element) !== expected) continue;
    element.click();
    return true;
  }
  return false;
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

    for name in [
        "chromium",
        "chromium-browser",
        "google-chrome",
        "google-chrome-stable",
    ] {
        if let Some(path) = find_on_path(name) {
#[derive(Debug, Deserialize)]
struct DevToolsTarget {
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
}

fn reserve_loopback_port() -> Result<u16, WebsiteDriverError> {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .map_err(|error| WebsiteDriverError::BrowserLaunchFailed(error.to_string()))
}

fn unique_profile_dir() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "osl-real-website-driver-{}-{nonce:x}",
        std::process::id()
    ))
}

fn find_browser_executable() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("OSL_WEBSITE_DRIVER_CHROME").map(PathBuf::from) {
        if executable_exists(&path) {
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

#[cfg(test)]
mod tests {
    use super::*;

    struct NamesOnlyDriver;

    impl WebsiteDriver for NamesOnlyDriver {
        fn find_page(
            &mut self,
            request: WebsitePageRequest,
        ) -> Result<WebsitePage, WebsiteDriverError> {
            Ok(WebsitePage {
                url: request.url,
                target_id: None,
            })
        }

        fn read_page(&mut self, page: &WebsitePage) -> Result<WebsitePageText, WebsiteDriverError> {
            Ok(WebsitePageText {
                page: page.clone(),
                title: "visible page title".to_owned(),
                text: "visible page text".to_owned(),
                controls: WebsitePageControls {
                    editable_boxes: vec!["Compose".to_owned()],
                    buttons: vec!["Send".to_owned()],
                    visible_message_areas: vec!["Reading pane".to_owned()],
                },
            })
        }

        fn place_text(
            &mut self,
            placement: WebsiteTextPlacement,
        ) -> Result<WebsitePlacementProof, WebsiteDriverError> {
            Ok(WebsitePlacementProof {
                page: placement.page,
                editable_box_name: placement.editable_box_name,
                utf16_units: placement.text.encode_utf16().count(),
                placed_sha256: sha256_hex(placement.text.as_bytes()),
            })
        fn read_selected_email(
            &mut self,
            page: &WebsitePage,
        ) -> Result<WebsiteSelectedEmail, WebsiteDriverError> {
            Ok(WebsiteSelectedEmail {
                page: page.clone(),
                body: "selected email body".to_owned(),
                conversation_identity: "stable-thread-identity".to_owned(),
            })
        }

        fn read_live_run_progress(
            &mut self,
            _page: &WebsitePage,
        ) -> Result<WebsiteLiveRunProgress, WebsiteDriverError> {
            Ok(WebsiteLiveRunProgress {
                active_account: "fixture@example.invalid".to_owned(),
                current_place: "Inbox".to_owned(),
                messages_checked: 1,
                matches: 1,
                scrolls: 0,
                waits: 0,
                changes: 1,
            })
        }

        fn read_mailbox(
            &mut self,
            page: &WebsitePage,
        ) -> Result<WebsiteMailboxRead, WebsiteDriverError> {
            Ok(WebsiteMailboxRead {
                page: page.clone(),
                folders: vec!["Inbox".to_owned(), "Sent".to_owned()],
                messages: vec![WebsiteMailboxMessage {
                    folder: "Sent".to_owned(),
                    subject: "fixture".to_owned(),
                    time: "2026-08-06 09:00".to_owned(),
                    sender: "fixture@example.invalid".to_owned(),
                    owner_marker: "SCRUB-PR-MINE".to_owned(),
                    yours: true,
                }],
            })
        }

        fn place_text(
            &mut self,
            _placement: WebsiteTextPlacement,
        ) -> Result<(), WebsiteDriverError> {
            Ok(())
        }

        fn press_named_control(
            &mut self,
            _control: WebsiteNamedControl,
        ) -> Result<(), WebsiteDriverError> {
            Ok(())
        }
    }

    #[test]
    fn task_1200_driver_interface_lists_exactly_four_real_website_jobs() {
        let jobs = <NamesOnlyDriver as WebsiteDriver>::JOBS;
        let names = jobs.iter().map(|job| job.wire_name()).collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                "find_page",
                "read_page",
                "place_text",
                "press_named_control"
            ]
        );
        assert_eq!(jobs.len(), 4);

        println!("TASK1200 website_driver_job_count={}", jobs.len());
        for name in names {
            println!("TASK1200 website_driver_job={name}");
        }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut candidates = Vec::new();
    if let Some(home) = home {
        candidates.push(home.join(".cache/ms-playwright/chromium-1234/chrome-linux64/chrome"));
    }
    candidates.extend(
        [
            "/usr/bin/google-chrome",
            "/usr/bin/google-chrome-stable",
            "/usr/bin/chromium",
            "/usr/bin/chromium-browser",
            "/snap/bin/chromium",
        ]
        .into_iter()
        .map(PathBuf::from),
    );
    candidates.into_iter().find(|path| executable_exists(path))
}

fn executable_exists(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_file())
        .unwrap_or(false)
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) struct FakeWebsiteDriver;

#[cfg(test)]
impl WebsiteDriver for FakeWebsiteDriver {
    fn kind(&self) -> WebsiteDriverKind {
        WebsiteDriverKind::FakeTestBrowser
    }

    fn find_page(&mut self, url: &Url) -> Result<WebsitePage, WebsiteDriverError> {
        Ok(WebsitePage {
            target_id: "fake-target".to_owned(),
            url: url.to_string(),
        })
    }

    fn read_page(&self, page: &WebsitePage) -> Result<WebsitePageSnapshot, WebsiteDriverError> {
        Ok(WebsitePageSnapshot {
            title: "fake test browser".to_owned(),
            url: page.url.clone(),
        })
    }
}
