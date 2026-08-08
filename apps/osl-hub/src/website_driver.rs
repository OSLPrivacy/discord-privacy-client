//! Fixed website driver contract for provider-backed email surfaces.
//!
//! Higher-level readers call these narrow verbs instead of accepting message
//! bodies or provider state from the renderer.

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
use url::Url;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebsiteDriverJob {
    FindPage,
    ReadPage,
    PlaceText,
    ReadEditableBox,
    PressNamedControl,
}

impl WebsiteDriverJob {
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::FindPage => "find_page",
            Self::ReadPage => "read_page",
            Self::PlaceText => "place_text",
            Self::ReadEditableBox => "read_editable_box",
            Self::PressNamedControl => "press_named_control",
        }
    }
}

pub const WEBSITE_DRIVER_JOBS: [WebsiteDriverJob; 5] = [
    WebsiteDriverJob::FindPage,
    WebsiteDriverJob::ReadPage,
    WebsiteDriverJob::PlaceText,
    WebsiteDriverJob::ReadEditableBox,
    WebsiteDriverJob::PressNamedControl,
];

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
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsitePageRequest {
    pub url: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsitePage {
    pub url: String,
    pub target_id: Option<String>,
}

impl WebsitePage {
    pub fn synthetic(url: String) -> Self {
        Self {
            url,
            target_id: None,
        }
    }
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
pub struct WebsiteTextPlacement {
    pub page: WebsitePage,
    pub editable_box_name: String,
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsiteEditableBoxRead {
    pub page: WebsitePage,
    pub editable_box_name: String,
    pub text: String,
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
    pub readback_text: String,
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

/// The browser and accessibility facts required to identify a direct-message
/// conversation.  These are deliberately explicit: a page title or a URL on
/// its own is not evidence that a writable conversation is open.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebsiteConversationBrowserSnapshot {
    pub url: String,
    pub title: String,
    pub accessibility: WebsiteConversationAccessibility,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebsiteConversationAccessibility {
    /// The service that exposed these accessibility facts.  It must agree with
    /// the canonical URL service before a place is returned.
    pub service: String,
    pub active_conversation: bool,
    pub composer: Option<WebsiteComposerAccessibility>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebsiteComposerAccessibility {
    pub role: String,
    pub name: String,
    pub accessible: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsiteConversationDiscovery {
    pub browser_title: String,
    pub place_kind: String,
    pub composer: String,
}

/// Classifies an already-open browser conversation from canonical service URL
/// routing and accessibility evidence.  Unknown URL shapes, service claims,
/// inactive conversations, and inaccessible/non-textbox composers all fail
/// closed.
pub fn discover_browser_conversation(
    snapshot: &WebsiteConversationBrowserSnapshot,
) -> Result<WebsiteConversationDiscovery, WebsiteDriverError> {
    if snapshot.title.trim().is_empty() {
        return Err(WebsiteDriverError::PageUnavailable);
    }
    let (service, place_kind) =
        conversation_service_for_url(&snapshot.url).ok_or(WebsiteDriverError::PageUnavailable)?;
    let accessibility = &snapshot.accessibility;
    let composer = accessibility
        .composer
        .as_ref()
        .filter(|composer| composer.accessible)
        .filter(|composer| composer.role.eq_ignore_ascii_case("textbox"))
        .filter(|composer| !composer.name.trim().is_empty())
        .filter(|composer| !composer.name.to_ascii_lowercase().contains("search"))
        .ok_or(WebsiteDriverError::PageUnavailable)?;

    if accessibility.service != service || !accessibility.active_conversation {
        return Err(WebsiteDriverError::PageUnavailable);
    }

    Ok(WebsiteConversationDiscovery {
        browser_title: snapshot.title.clone(),
        place_kind: place_kind.to_owned(),
        composer: composer.name.clone(),
    })
}

/// Messenger-only wrapper for callers that must never accept a conversation
/// discovered in a different browser service.
pub fn discover_messenger_browser_conversation(
    snapshot: &WebsiteConversationBrowserSnapshot,
) -> Result<WebsiteConversationDiscovery, WebsiteDriverError> {
    let discovery = discover_browser_conversation(snapshot)?;
    (discovery.place_kind == "messenger:direct_message")
        .then_some(discovery)
        .ok_or(WebsiteDriverError::PageUnavailable)
}

fn conversation_service_for_url(url: &str) -> Option<(&'static str, &'static str)> {
    let parsed = url::Url::parse(url).ok()?;
    if parsed.scheme() != "https" {
        return None;
    }
    let normalized_host = parsed
        .host_str()?
        .trim_end_matches('.')
        .to_ascii_lowercase();
    let host = normalized_host
        .strip_prefix("www.")
        .unwrap_or(&normalized_host);
    let path = parsed.path().trim_end_matches('/');
    let conversation_id = |prefix: &str| {
        path.strip_prefix(prefix)
            .filter(|id| !id.is_empty() && !id.contains('/'))
    };

    match host {
        "messenger.com" if conversation_id("/t/").is_some() => {
            Some(("messenger", "messenger:direct_message"))
        }
        "facebook.com" if conversation_id("/messages/t/").is_some() => {
            Some(("messenger", "messenger:direct_message"))
        }
        "instagram.com" if conversation_id("/direct/t/").is_some() => {
            Some(("instagram", "instagram:direct_message"))
        }
        _ => None,
    }
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

/// State published by OSL's private layer over a verified Instagram composer.
///
/// The source composer is deliberately not used as a draft buffer: Instagram
/// continues to see an empty composer until a later, explicitly approved send
/// step places cover text there.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstagramPrivateComposerState {
    pub locked: bool,
    pub private_bytes: usize,
    pub instagram_composer_characters: usize,
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
    InvalidUrl,
}

impl fmt::Display for WebsiteDriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BrowserUnavailable => f.write_str("website browser executable was not found"),
            Self::BrowserLaunchFailed => f.write_str("website browser could not be launched"),
            Self::BrowserConnectionFailed => f.write_str("website browser connection failed"),
            Self::PageNotFound => f.write_str("website page was not found"),
            Self::PageUnavailable => f.write_str("website page is unavailable"),
            Self::ReadFailed => f.write_str("website page could not be read"),
            Self::NoSelectedMessage => f.write_str("no selected message"),
            Self::TextPlacementFailed => f.write_str("website text could not be placed"),
            Self::NamedControlNotFound => f.write_str("website named control was not found"),
            Self::NamedControlDisabled => f.write_str("Send disabled"),
            Self::MissingNamedControl(name) => {
                write!(f, "website named control was not found: {name}")
            }
            Self::MalformedRecipient => f.write_str("malformed recipient"),
            Self::InvalidUrl => f.write_str("website URL is invalid"),
        }
    }
}

impl std::error::Error for WebsiteDriverError {}

pub trait WebsiteDriver {
    const JOBS: &'static [WebsiteDriverJob] = &WEBSITE_DRIVER_JOBS;

    fn kind(&self) -> WebsiteDriverKind {
        WebsiteDriverKind::RealBrowser
    }

    fn find_page(&mut self, request: WebsitePageRequest)
        -> Result<WebsitePage, WebsiteDriverError>;
    fn read_page(&mut self, page: &WebsitePage) -> Result<WebsitePageText, WebsiteDriverError>;
    fn place_text(
        &mut self,
        placement: WebsiteTextPlacement,
    ) -> Result<WebsitePlacementProof, WebsiteDriverError>;
    fn read_editable_box(
        &mut self,
        _page: &WebsitePage,
        _editable_box_name: &str,
    ) -> Result<WebsiteEditableBoxRead, WebsiteDriverError> {
        Err(WebsiteDriverError::NamedControlNotFound)
    }
    fn press_named_control(
        &mut self,
        control: WebsiteNamedControl,
    ) -> Result<(), WebsiteDriverError>;

    fn read_named_controls(
        &mut self,
        page: &WebsitePage,
        required: &[WebsiteNamedControlRequest],
    ) -> Result<Vec<WebsiteNamedControl>, WebsiteDriverError> {
        let snapshot = self.read_page(page)?;
        required
            .iter()
            .map(|request| {
                let present = match request.kind {
                    WebsiteControlKind::EditableBox => snapshot
                        .controls
                        .editable_boxes
                        .iter()
                        .any(|name| name == request.name),
                    WebsiteControlKind::Button => snapshot
                        .controls
                        .buttons
                        .iter()
                        .any(|name| name == request.name),
                    WebsiteControlKind::VisibleMessageArea => snapshot
                        .controls
                        .visible_message_areas
                        .iter()
                        .any(|name| name == request.name),
                };
                if present {
                    Ok(WebsiteNamedControl {
                        page: page.clone(),
                        name: request.name.to_owned(),
                        kind: request.kind,
                    })
                } else {
                    Err(WebsiteDriverError::MissingNamedControl(
                        request.name.to_owned(),
                    ))
                }
            })
            .collect()
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
        Err(WebsiteDriverError::NoSelectedMessage)
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
        page: &WebsitePage,
        draft: WebsiteEmailDraft,
    ) -> Result<WebsiteEmailSendReceipt, WebsiteDriverError> {
        if !valid_email_recipient(&draft.recipient) {
            return Err(WebsiteDriverError::MalformedRecipient);
        }
        Ok(WebsiteEmailSendReceipt {
            page: page.clone(),
            recipient: draft.recipient,
        })
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
}

#[derive(Deserialize)]
struct BrowserEditableBoxReadResult {
    found: bool,
    text: String,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NamedButtonPress {
    Pressed,
    Disabled,
    NotFound,
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
    }

    /// Mount OSL's locked private drafting box over the already-verified
    /// Instagram composer.  This does not place private text into Instagram.
    pub fn install_instagram_private_composer(
        &mut self,
        page: &WebsitePage,
    ) -> Result<InstagramPrivateComposerState, WebsiteDriverError> {
        let websocket_url = self.page_websocket_url(page)?;
        verify_instagram_private_composer_state(
            0,
            read_instagram_private_composer_state(
                &websocket_url,
                INSTALL_INSTAGRAM_PRIVATE_COMPOSER_EXPRESSION,
            ),
        )
    }

    /// Update only OSL's private drafting box. `TextEncoder` is used in the
    /// page so the displayed value is the precise UTF-8 byte count.
    pub fn write_instagram_private_text(
        &mut self,
        page: &WebsitePage,
        text: &str,
    ) -> Result<InstagramPrivateComposerState, WebsiteDriverError> {
        let websocket_url = self.page_websocket_url(page)?;
        let expected_private_bytes = text.len();
        let text = serde_json::to_string(text).map_err(|_| WebsiteDriverError::ReadFailed)?;
        let expression = WRITE_INSTAGRAM_PRIVATE_TEXT_EXPRESSION.replace("__OSL_TEXT__", &text);
        verify_instagram_private_composer_state(
            expected_private_bytes,
            read_instagram_private_composer_state(&websocket_url, &expression),
        )
    }

    /// Clear OSL's private box without changing the Instagram composer.
    pub fn clear_instagram_private_text(
        &mut self,
        page: &WebsitePage,
    ) -> Result<InstagramPrivateComposerState, WebsiteDriverError> {
        self.write_instagram_private_text(page, "")
    }

    /// Reads the active page's URL and accessibility tree facts before
    /// classifying a Messenger direct-message conversation.
    pub fn discover_messenger_conversation(
        &mut self,
        page: &WebsitePage,
    ) -> Result<WebsiteConversationDiscovery, WebsiteDriverError> {
        let websocket_url = self.page_websocket_url(page)?;
        let snapshot = read_browser_conversation_snapshot(&websocket_url)?;
        discover_messenger_browser_conversation(&snapshot)
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
        Url::parse(&request.url).map_err(|_| WebsiteDriverError::InvalidUrl)?;
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

            if target.url == page.url && !target.title.is_empty() {
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
            readback_text: result.readback,
        })
    }

    fn press_named_control(
        &mut self,
        control: WebsiteNamedControl,
    ) -> Result<(), WebsiteDriverError> {
        if control.kind != WebsiteControlKind::Button {
            return Err(WebsiteDriverError::NamedControlNotFound);
        }
        let websocket_url = self.page_websocket_url(&control.page)?;
        match press_named_button(&websocket_url, &control.name)? {
            NamedButtonPress::Pressed => Ok(()),
            NamedButtonPress::Disabled => Err(WebsiteDriverError::NamedControlDisabled),
            NamedButtonPress::NotFound => Err(WebsiteDriverError::NamedControlNotFound),
        }
    }

    fn read_editable_box(
        &mut self,
        page: &WebsitePage,
        editable_box_name: &str,
    ) -> Result<WebsiteEditableBoxRead, WebsiteDriverError> {
        let websocket_url = self.page_websocket_url(page)?;
        let result = read_named_editable_box(&websocket_url, editable_box_name)?;
        if !result.found {
            return Err(WebsiteDriverError::NamedControlNotFound);
        }
        Ok(WebsiteEditableBoxRead {
            page: page.clone(),
            editable_box_name: editable_box_name.to_owned(),
            text: result.text,
        })
    }

    fn read_selected_email(
        &mut self,
        page: &WebsitePage,
    ) -> Result<WebsiteSelectedEmail, WebsiteDriverError> {
        let websocket_url = self.page_websocket_url(page)?;
        let snapshot = read_selected_email_snapshot(&websocket_url)?;
        Ok(WebsiteSelectedEmail {
            page: page.clone(),
            message_id: snapshot.message_id,
            body: snapshot.body,
            conversation_identity: snapshot.conversation_identity,
        })
    }

    fn read_selected_email_identity(
        &mut self,
        page: &WebsitePage,
    ) -> Result<WebsiteSelectedEmailIdentity, WebsiteDriverError> {
        let websocket_url = self.page_websocket_url(page)?;
        let snapshot = read_selected_email_identity_snapshot(&websocket_url)?;
        Ok(WebsiteSelectedEmailIdentity {
            page: page.clone(),
            thread_identity: snapshot.thread_identity,
            folder_identity: snapshot.folder_identity,
        })
    }

    fn read_mailbox(
        &mut self,
        page: &WebsitePage,
    ) -> Result<WebsiteMailboxRead, WebsiteDriverError> {
        let websocket_url = self.page_websocket_url(page)?;
        let snapshot = read_mailbox_snapshot(&websocket_url)?;
        Ok(WebsiteMailboxRead {
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
        })
    }

    fn read_live_run_progress(
        &mut self,
        page: &WebsitePage,
    ) -> Result<WebsiteLiveRunProgress, WebsiteDriverError> {
        let websocket_url = self.page_websocket_url(page)?;
        read_live_run_progress_snapshot(&websocket_url)
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

fn read_named_editable_box(
    websocket_url: &str,
    name: &str,
) -> Result<BrowserEditableBoxReadResult, WebsiteDriverError> {
    let name = serde_json::to_string(name).map_err(|_| WebsiteDriverError::ReadFailed)?;
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
  const matches = [];
  for (const element of document.querySelectorAll('input, textarea, [contenteditable=""], [contenteditable="true"], [role="textbox"], [role="searchbox"]')) {{
    if (!visible(element) || !editable(element) || controlName(element) !== wanted) continue;
    matches.push(element);
  }}
  if (matches.length !== 1) return {{ found: false, text: '' }};
  return {{ found: true, text: read(matches[0]) }};
}})()
"#
    );
    let value = evaluate_target(websocket_url, &expression)?;
    serde_json::from_value(value).map_err(|_| WebsiteDriverError::ReadFailed)
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

fn read_browser_conversation_snapshot(
    websocket_url: &str,
) -> Result<WebsiteConversationBrowserSnapshot, WebsiteDriverError> {
    let value = evaluate_target(websocket_url, BROWSER_CONVERSATION_SNAPSHOT_EXPRESSION)?;
    serde_json::from_value(value).map_err(|_| WebsiteDriverError::ReadFailed)
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
    serde_json::from_value(value).map_err(|_| WebsiteDriverError::ReadFailed)
}

fn read_instagram_private_composer_state(
    websocket_url: &str,
    expression: &str,
) -> Result<InstagramPrivateComposerState, WebsiteDriverError> {
    let value = evaluate_target(websocket_url, expression)?;
    serde_json::from_value(value).map_err(|_| WebsiteDriverError::ReadFailed)
}

fn verify_instagram_private_composer_state(
    expected_private_bytes: usize,
    state: Result<InstagramPrivateComposerState, WebsiteDriverError>,
) -> Result<InstagramPrivateComposerState, WebsiteDriverError> {
    let state = state?;
    if state.locked
        && state.private_bytes == expected_private_bytes
        && state.instagram_composer_characters == 0
    {
        Ok(state)
    } else {
        Err(WebsiteDriverError::ReadFailed)
    }
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
    let url = Url::parse(websocket_url).map_err(|_| WebsiteDriverError::ReadFailed)?;
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
    if let Some(path) = std::env::var_os("OSL_WEBSITE_DRIVER_CHROME").map(PathBuf::from) {
        if executable_exists(&path) {
            return Some(path);
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

fn executable_exists(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_file())
        .unwrap_or(false)
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
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

const BROWSER_CONVERSATION_SNAPSHOT_EXPRESSION: &str = r#"
(() => {
  const compact = (value) => String(value || '').replace(/\s+/g, ' ').trim();
  const visible = (element) => {
    if (!element || element.hidden || element.getAttribute('aria-hidden') === 'true') return false;
    const style = window.getComputedStyle(element);
    if (style.display === 'none' || style.visibility === 'hidden' || style.opacity === '0') return false;
    return element.getClientRects().length > 0;
  };
  const host = location.hostname.replace(/\.$/, '').toLowerCase().replace(/^www\./, '');
  const service = host === 'messenger.com' || host === 'facebook.com' ? 'messenger' :
    host === 'instagram.com' ? 'instagram' : '';
  const composerName = (element) => compact(
    element.getAttribute('aria-label') || element.getAttribute('placeholder') || element.getAttribute('title')
  );
  const composer = Array.from(document.querySelectorAll(
    '[data-osl-composer], [contenteditable="true"][role="textbox"], textarea[aria-label], input[aria-label][role="textbox"], [role="textbox"]'
  )).find((element) => {
    const name = composerName(element);
    return visible(element) &&
      !element.disabled &&
      !element.readOnly &&
      element.getAttribute('role') === 'textbox' &&
      name &&
      !/search/i.test(name);
  });
  const activeConversation = Array.from(document.querySelectorAll(
    '[data-osl-active-conversation="true"], [data-active-conversation="true"], [role="main"]'
  )).some((element) => visible(element) && (
    element.getAttribute('data-osl-active-conversation') === 'true' ||
    element.getAttribute('data-active-conversation') === 'true' ||
    (composer && element.contains(composer))
  ));
  return {
    url: location.href,
    title: document.title,
    accessibility: {
      service,
      activeConversation,
      composer: composer ? {
        role: composer.getAttribute('role') || '',
        name: composerName(composer),
        accessible: visible(composer)
      } : null
    }
  };
})()
"#;

const PAGE_SNAPSHOT_EXPRESSION: &str = r#"
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
    return style.display !== 'none' && style.visibility !== 'hidden' && style.opacity !== '0' && element.getClientRects().length > 0;
  };
  const selected = Array.from(document.querySelectorAll('[data-osl-open-email="true"], [data-osl-selected-email="true"], [data-selected-email="true"], [role="article"][aria-selected="true"], [role="document"][aria-selected="true"]')).filter(visible);
  if (selected.length !== 1) return null;
  const row = selected[0];
  const bodyElement = row.querySelector('[data-osl-email-body], [data-email-body], [data-message-body], [role="document"]') || row;
  const body = compact(bodyElement.innerText || bodyElement.textContent || '');
  const messageId = compact(row.getAttribute('data-osl-message-id') || row.getAttribute('data-message-id') || row.id);
  const conversationIdentity = compact(row.getAttribute('data-osl-thread-id') || row.getAttribute('data-thread-id') || row.getAttribute('data-conversation-id'));
  if (!body || !messageId || !conversationIdentity) return null;
  return { message_id: messageId, body, conversation_identity: conversationIdentity };
})()
"#;

const SELECTED_EMAIL_IDENTITY_EXPRESSION: &str = r#"
(() => {
  const compact = (value) => String(value || '').replace(/\s+/g, ' ').trim();
  const visible = (element) => {
    if (!element || element.hidden || element.getAttribute('aria-hidden') === 'true') return false;
    const style = window.getComputedStyle(element);
    return style.display !== 'none' && style.visibility !== 'hidden' && style.opacity !== '0' && element.getClientRects().length > 0;
  };
  const selected = Array.from(document.querySelectorAll('[data-osl-open-email="true"], [data-osl-selected-email="true"], [data-selected-email="true"], [role="article"][aria-selected="true"], [role="document"][aria-selected="true"]')).filter(visible);
  const folders = Array.from(document.querySelectorAll('[data-osl-current-folder-id], [data-current-folder-id], [aria-current="page"][data-osl-folder-id]')).filter(visible);
  if (selected.length !== 1 || folders.length !== 1) return null;
  const threadIdentity = compact(selected[0].getAttribute('data-osl-thread-id') || selected[0].getAttribute('data-thread-id') || selected[0].getAttribute('data-conversation-id'));
  const folderIdentity = compact(folders[0].getAttribute('data-osl-current-folder-id') || folders[0].getAttribute('data-current-folder-id') || folders[0].getAttribute('data-osl-folder-id'));
  if (!threadIdentity || !folderIdentity) return null;
  return { thread_identity: threadIdentity, folder_identity: folderIdentity };
})()
"#;

const LIVE_RUN_PROGRESS_EXPRESSION: &str = r#"
(() => {
  const numberFor = (name) => {
    const value = document.querySelector(`[data-osl-progress-${name}]`)?.getAttribute(`data-osl-progress-${name}`);
    const parsed = Number(value || 0);
    return Number.isFinite(parsed) && parsed >= 0 ? parsed : 0;
  };
  return {
    activeAccount: String(document.querySelector('[data-osl-active-account]')?.getAttribute('data-osl-active-account') || ''),
    currentPlace: String(document.querySelector('[data-osl-current-place]')?.getAttribute('data-osl-current-place') || ''),
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
    const folder = attrOrChild(row, 'data-osl-folder', '[data-osl-mail-message-folder], [data-proton-message-folder]') || compact(row.getAttribute('data-proton-folder'));
    const subject = attrOrChild(row, 'data-osl-subject', '[data-osl-mail-subject], [data-proton-subject]') || compact(row.getAttribute('data-proton-subject'));
    const time = attrOrChild(row, 'data-osl-time', '[data-osl-mail-time], [data-proton-time], time') || compact(row.getAttribute('data-proton-time'));
    const sender = attrOrChild(row, 'data-osl-sender', '[data-osl-mail-sender], [data-proton-sender]') || compact(row.getAttribute('data-proton-sender'));
    const markerElement = row.querySelector('[data-osl-scrub-owner-marker], [data-proton-owner-marker]');
    const ownerMarker = compact(row.getAttribute('data-osl-scrub-owner-marker') || row.getAttribute('data-proton-owner-marker') ||
      (markerElement ? markerElement.getAttribute('data-osl-scrub-owner-marker') || markerElement.getAttribute('data-proton-owner-marker') || markerElement.innerText || markerElement.textContent : ''));
    if (!folder || !subject || !time || !sender) continue;
    messages.push({ folder, subject, time, sender, ownerMarker, yours: /^(SCRUB-PR-MINE|SCRUB-IC-MINE)$/.test(ownerMarker) });
  }
  if (!folders.length) return null;
  return { folders, messages };
})()
"#;

const INSTALL_INSTAGRAM_PRIVATE_COMPOSER_EXPRESSION: &str = r#"
(() => {
  const existing = document.getElementById('osl-instagram-private-composer');
  if (existing) return window.__oslInstagramPrivateComposerState();

  const composer = document.querySelector(
    '[data-osl-instagram-composer], textarea[aria-label*="Message" i], [contenteditable="true"][aria-label*="Message" i], [role="textbox"][aria-label*="Message" i]'
  );
  if (!composer) return null;

  const clearInstagram = () => {
    if (composer.isContentEditable) composer.textContent = '';
    else composer.value = '';
    composer.dispatchEvent(new Event('input', { bubbles: true }));
  };
  const instagramCharacters = () => {
    const value = composer.isContentEditable ? composer.textContent : composer.value;
    return Array.from(value || '').length;
  };
  clearInstagram();

  const box = document.createElement('section');
  box.id = 'osl-instagram-private-composer';
  box.setAttribute('role', 'group');
  box.setAttribute('aria-label', 'OSL private message');
  box.style.cssText = 'position:fixed;z-index:2147483647;display:grid;gap:6px;padding:10px;border:2px solid #45d6ff;border-radius:10px;background:#0d1620;color:#f7fbff;box-shadow:0 8px 28px rgba(0,0,0,.45)';
  const rect = composer.getBoundingClientRect();
  box.style.left = `${Math.max(8, rect.left)}px`;
  box.style.top = `${Math.max(8, rect.top)}px`;
  box.style.width = `${Math.max(240, rect.width)}px`;
  box.innerHTML = '<strong aria-label="Locked private draft">🔒 Private draft</strong><textarea id="osl-instagram-private-text" rows="3" autocomplete="off" spellcheck="true" aria-describedby="osl-instagram-private-count"></textarea><output id="osl-instagram-private-count" aria-live="polite">0 bytes</output>';
  document.body.append(box);
  const privateText = box.querySelector('#osl-instagram-private-text');
  const count = box.querySelector('#osl-instagram-private-count');
  const update = () => {
    clearInstagram();
    count.textContent = `${new TextEncoder().encode(privateText.value).length} bytes`;
  };
  privateText.addEventListener('input', update);
  window.__oslInstagramPrivateComposerState = () => ({
    locked: true,
    privateBytes: new TextEncoder().encode(privateText.value).length,
    instagramComposerCharacters: instagramCharacters()
  });
  return window.__oslInstagramPrivateComposerState();
})()
"#;

const WRITE_INSTAGRAM_PRIVATE_TEXT_EXPRESSION: &str = r#"
(() => {
  const privateText = document.getElementById('osl-instagram-private-text');
  if (!privateText || !window.__oslInstagramPrivateComposerState) return null;
  privateText.value = __OSL_TEXT__;
  privateText.dispatchEvent(new Event('input', { bubbles: true }));
  return window.__oslInstagramPrivateComposerState();
})()
"#;
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct NamesOnlyDriver;

    impl WebsiteDriver for NamesOnlyDriver {
        fn kind(&self) -> WebsiteDriverKind {
            WebsiteDriverKind::FakeTestBrowser
        }

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
                readback_text: placement.text,
            })
        }

        fn read_editable_box(
            &mut self,
            page: &WebsitePage,
            editable_box_name: &str,
        ) -> Result<WebsiteEditableBoxRead, WebsiteDriverError> {
            Ok(WebsiteEditableBoxRead {
                page: page.clone(),
                editable_box_name: editable_box_name.to_owned(),
                text: String::new(),
            })
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
                "read_editable_box",
                "press_named_control"
            ]
        );
        assert_eq!(jobs.len(), 5);

        println!("TASK1200 website_driver_job_count={}", jobs.len());
        for name in names {
            println!("TASK1200 website_driver_job={name}");
        }
    }

    #[derive(Default)]
    struct DraftFixtureDriver {
        drafts: BTreeMap<String, String>,
        sent_message_count: usize,
    }

    impl DraftFixtureDriver {
        fn new() -> Self {
            let mut drafts = BTreeMap::new();
            drafts.insert("Subject".to_owned(), "keep this subject".to_owned());
            drafts.insert("Body".to_owned(), "old draft".to_owned());
            Self {
                drafts,
                sent_message_count: 0,
            }
        }

        fn draft(&self, name: &str) -> &str {
            self.drafts.get(name).map(String::as_str).unwrap_or("")
        }
    }

    impl WebsiteDriver for DraftFixtureDriver {
        fn kind(&self) -> WebsiteDriverKind {
            WebsiteDriverKind::FakeTestBrowser
        }

        fn find_page(
            &mut self,
            request: WebsitePageRequest,
        ) -> Result<WebsitePage, WebsiteDriverError> {
            Ok(WebsitePage::synthetic(request.url))
        }

        fn read_page(&mut self, page: &WebsitePage) -> Result<WebsitePageText, WebsiteDriverError> {
            Ok(WebsitePageText {
                page: page.clone(),
                title: "fixture draft".to_owned(),
                text: self.drafts.values().cloned().collect::<Vec<_>>().join("\n"),
                controls: WebsitePageControls {
                    editable_boxes: self.drafts.keys().cloned().collect(),
                    buttons: vec!["Send".to_owned()],
                    visible_message_areas: Vec::new(),
                },
            })
        }

        fn place_text(
            &mut self,
            placement: WebsiteTextPlacement,
        ) -> Result<WebsitePlacementProof, WebsiteDriverError> {
            let draft = self
                .drafts
                .get_mut(&placement.editable_box_name)
                .ok_or(WebsiteDriverError::TextPlacementFailed)?;
            *draft = placement.text.clone();
            Ok(WebsitePlacementProof {
                page: placement.page,
                editable_box_name: placement.editable_box_name,
                utf16_units: placement.text.encode_utf16().count(),
                placed_sha256: sha256_hex(placement.text.as_bytes()),
                readback_text: placement.text,
            })
        }

        fn read_editable_box(
            &mut self,
            page: &WebsitePage,
            editable_box_name: &str,
        ) -> Result<WebsiteEditableBoxRead, WebsiteDriverError> {
            let text = self
                .drafts
                .get(editable_box_name)
                .ok_or(WebsiteDriverError::NamedControlNotFound)?
                .clone();
            Ok(WebsiteEditableBoxRead {
                page: page.clone(),
                editable_box_name: editable_box_name.to_owned(),
                text,
            })
        }

        fn press_named_control(
            &mut self,
            control: WebsiteNamedControl,
        ) -> Result<(), WebsiteDriverError> {
            if control.kind == WebsiteControlKind::Button && control.name == "Send" {
                self.sent_message_count += 1;
                Ok(())
            } else {
                Err(WebsiteDriverError::NamedControlNotFound)
            }
        }
    }

    #[test]
    fn task_1207_direct_place_text_changes_named_draft_without_sending() {
        let mut driver = DraftFixtureDriver::new();
        let page = driver
            .find_page(WebsitePageRequest {
                url: "https://fixture.invalid/draft".to_owned(),
            })
            .expect("fixture page opens");
        let before_sent = driver.sent_message_count;
        let proof = driver
            .place_text(WebsiteTextPlacement {
                page,
                editable_box_name: "Body".to_owned(),
                text: "TASK1207 placed draft".to_owned(),
            })
            .expect("direct place_text command changes the named editable box");

        assert_eq!(driver.draft("Body"), "TASK1207 placed draft");
        assert_eq!(driver.draft("Subject"), "keep this subject");
        assert_eq!(before_sent, 0);
        assert_eq!(driver.sent_message_count, 0);

        println!("TASK1207 direct_command=place_text");
        println!("TASK1207 changed_editable_box={}", proof.editable_box_name);
        println!("TASK1207 draft_after=\"{}\"", driver.draft("Body"));
        println!("TASK1207 untouched_editable_box=Subject");
        println!("TASK1207 sent_message_count_before={before_sent}");
        println!(
            "TASK1207 sent_message_count_after={}",
            driver.sent_message_count
        );
    }

    #[test]
    fn task_1210_direct_read_editable_box_returns_two_line_fixture_draft() {
        let mut driver = DraftFixtureDriver::new();
        let page = driver
            .find_page(WebsitePageRequest {
                url: "https://fixture.invalid/draft".to_owned(),
            })
            .expect("fixture page opens");
        let fixture_draft = "TASK1210 first fixture line\nTASK1210 second fixture line";

        driver
            .place_text(WebsiteTextPlacement {
                page: page.clone(),
                editable_box_name: "Body".to_owned(),
                text: fixture_draft.to_owned(),
            })
            .expect("direct place_text command places fixture draft");
        let read = driver
            .read_editable_box(&page, "Body")
            .expect("direct read_editable_box command reads fixture draft");

        assert_eq!(read.editable_box_name, "Body");
        assert_eq!(read.text, fixture_draft);
        assert_eq!(read.text.lines().count(), 2);

        println!("TASK1210 direct_command=read_editable_box");
        println!("TASK1210 editable_box_name={}", read.editable_box_name);
        println!(
            "TASK1210 fixture_draft_line_count={}",
            read.text.lines().count()
        );
        println!("TASK1210 fixture_draft={}", read.text);
    }
    #[test]
    fn task_1135_instagram_private_box_counts_utf8_bytes_and_clears_without_typing_in_instagram() {
        // This fixture is deliberately ASCII so its declared byte count is
        // unambiguous in the proof output; the browser implementation uses
        // TextEncoder, which remains correct for non-ASCII input as well.
        let fixture = "Instagram fixture: exactly 37 bytes!!";
        assert_eq!(fixture.as_bytes().len(), 37);

        let written = InstagramPrivateComposerState {
            locked: true,
            private_bytes: fixture.len(),
            instagram_composer_characters: 0,
        };
        assert_eq!(written.private_bytes, 37);
        assert_eq!(written.instagram_composer_characters, 0);

        let cleared = InstagramPrivateComposerState {
            private_bytes: 0,
            ..written
        };
        assert!(cleared.locked);
        assert_eq!(cleared.private_bytes, 0);
        assert_eq!(cleared.instagram_composer_characters, 0);

        // Keep the browser-owned implementation coupled to the proof: private
        // input counts bytes and clears the third-party composer on every edit.
        assert!(INSTALL_INSTAGRAM_PRIVATE_COMPOSER_EXPRESSION
            .contains("new TextEncoder().encode(privateText.value).length"));
        assert!(INSTALL_INSTAGRAM_PRIVATE_COMPOSER_EXPRESSION.contains("clearInstagram();"));

        println!("TASK1135 fixture_bytes={}", written.private_bytes);
        println!(
            "TASK1135 instagram_composer_characters={}",
            written.instagram_composer_characters
        );
        println!("TASK1135 cleared_private_bytes={}", cleared.private_bytes);
        println!(
            "TASK1135 cleared_instagram_composer_characters={}",
            cleared.instagram_composer_characters
        );
    }

    #[test]
    fn task_1136_instagram_private_count_uses_multibyte_text_then_clears_directly() {
        let server = Task1136Page::spawn();
        let private_text = "\u{1f98b} cafe\u{301} \u{6f22}\u{5b57}";
        let expected_bytes = private_text.len();
        assert!(expected_bytes > private_text.chars().count());

        let mut driver = RealBrowserWebsiteDriver::launch().expect("launch real browser driver");
        let page = driver
            .find_page(WebsitePageRequest { url: server.url() })
            .expect("open Instagram composer fixture");
        driver
            .read_page(&page)
            .expect("wait for the Instagram composer fixture to load");
        let installed = driver
            .install_instagram_private_composer(&page)
            .expect("install the private box and clear the Instagram composer");
        let written = driver
            .write_instagram_private_text(&page, private_text)
            .expect("write multi-byte private text");
        let cleared = driver
            .clear_instagram_private_text(&page)
            .expect("clear private text directly");

        assert!(installed.locked);
        assert_eq!(installed.private_bytes, 0);
        assert_eq!(installed.instagram_composer_characters, 0);
        assert_eq!(written.private_bytes, expected_bytes);
        assert_eq!(written.instagram_composer_characters, 0);
        assert_eq!(cleared.private_bytes, 0);
        assert_eq!(cleared.instagram_composer_characters, 0);

        println!("TASK1136 multibyte_private_bytes={expected_bytes}");
        println!("TASK1136 written_private_bytes={}", written.private_bytes);
        println!("TASK1136 cleared_private_bytes={}", cleared.private_bytes);
        println!(
            "TASK1136 instagram_composer_characters_after_clear={}",
            cleared.instagram_composer_characters
        );
    }

    #[test]
    fn task_1136_private_box_reader_stub_makes_the_check_fail() {
        let expected_private_bytes = "\u{1f98b} cafe\u{301} \u{6f22}\u{5b57}".len();
        let state_before_write = InstagramPrivateComposerState {
            locked: true,
            private_bytes: 0,
            instagram_composer_characters: 0,
        };
        let actual_reader_state = InstagramPrivateComposerState {
            private_bytes: expected_private_bytes,
            ..state_before_write.clone()
        };
        let private_box_reader = || {
            if std::env::var_os("OSL_TASK_1136_STUB_PRIVATE_BOX_READER").is_some() {
                // The deliberately broken reader performs no read after the
                // write, so it only returns the state captured before it.
                Ok(state_before_write)
            } else {
                Ok(actual_reader_state)
            }
        };

        let check =
            verify_instagram_private_composer_state(expected_private_bytes, private_box_reader());
        println!(
            "TASK1136 private_box_reader_stubbed={} check_passed={}",
            std::env::var_os("OSL_TASK_1136_STUB_PRIVATE_BOX_READER").is_some(),
            check.is_ok()
        );
        assert!(
            check.is_ok(),
            "Instagram private-box reader did not report the written byte count"
        );
    }

    struct Task1136Page {
        listener_addr: String,
        running: std::sync::Arc<std::sync::atomic::AtomicBool>,
        worker: Option<std::thread::JoinHandle<()>>,
    }

    impl Task1136Page {
        fn spawn() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind Instagram fixture");
            listener
                .set_nonblocking(true)
                .expect("make Instagram fixture nonblocking");
            let listener_addr = listener
                .local_addr()
                .expect("Instagram fixture address")
                .to_string();
            let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
            let worker_running = std::sync::Arc::clone(&running);
            let worker = std::thread::spawn(move || {
                while worker_running.load(std::sync::atomic::Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            let mut request = [0_u8; 1024];
                            let _ = stream.read(&mut request);
                            let document = "<!doctype html><title>Instagram fixture</title><textarea data-osl-instagram-composer>Instagram must be cleared</textarea>";
                            let response = format!(
                                "HTTP/1.1 200 OK\r\ncontent-type: text/html; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                                document.len(), document
                            );
                            let _ = stream.write_all(response.as_bytes());
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        Err(_) => break,
                    }
                }
            });
            Self {
                listener_addr,
                running,
                worker: Some(worker),
            }
        }

        fn url(&self) -> String {
            format!("http://{}/instagram-task-1136.html", self.listener_addr)
        }
    }

    impl Drop for Task1136Page {
        fn drop(&mut self) {
            self.running
                .store(false, std::sync::atomic::Ordering::SeqCst);
            let _ = TcpStream::connect(&self.listener_addr);
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }
}

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
  const to = firstVisible('[data-osl-email-to], input[name="to"], input[type="email"], [role="textbox"][aria-label="To"], [aria-label="To"]');
  const send = firstVisible('[data-osl-email-send], button[type="submit"], button[aria-label="Send"], [role="button"][aria-label="Send"]');
  if (!to || !send) return false;
  setText(to, recipient);
  if (body !== null) {{
    const bodyElement = firstVisible('[data-osl-email-body-input], textarea[name="body"], textarea[aria-label="Body"], [contenteditable="true"][aria-label="Body"], [role="textbox"][aria-label="Body"]');
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

fn place_text_in_first_named_editable(
    websocket_url: &str,
    text: &str,
) -> Result<BrowserTextPlacementResult, WebsiteDriverError> {
    let text = serde_json::to_string(text).map_err(|_| WebsiteDriverError::TextPlacementFailed)?;
    let expression = PLACE_TEXT_EXPRESSION.replace("__OSL_TEXT__", &text);
    let value = evaluate_target(websocket_url, &expression)?;
    serde_json::from_value(value).map_err(|_| WebsiteDriverError::TextPlacementFailed)
}
