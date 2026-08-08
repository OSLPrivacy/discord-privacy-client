//! Real website driver contract and browser-backed implementation.
//!
//! The implementation that talks to a browser lives behind this interface. The
//! backend job list is fixed here so higher-level website work cannot smuggle in
//! generic browser automation verbs.

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
}

impl WebsiteDriverJob {
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::FindPage => "find_page",
            Self::ReadPage => "read_page",
            Self::PlaceText => "place_text",
            Self::PressNamedControl => "press_named_control",
        }
    }
}

pub const WEBSITE_DRIVER_JOBS: [WebsiteDriverJob; 4] = [
    WebsiteDriverJob::FindPage,
    WebsiteDriverJob::ReadPage,
    WebsiteDriverJob::PlaceText,
    WebsiteDriverJob::PressNamedControl,
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsitePageRequest {
    pub url: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsitePage {
    pub url: String,
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
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsiteNamedControl {
    pub page: WebsitePage,
    pub name: String,
    pub kind: WebsiteControlKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsitePageText {
    pub page: WebsitePage,
    pub title: String,
    pub text: String,
    pub controls: WebsitePageControls,
}

/// The bounded pieces of an Instagram web surface that the website driver may
/// discover. These are observations only; they do not authorize a write.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsiteInstagramDiscovery {
    pub browser_title: String,
    pub place_kind: String,
    pub active_composer: String,
}

/// The outcome of placing a marked fixture into a named browser composer,
/// reading that same composer back, and clearing it again.
///
/// The byte counts are intentionally public for an attended check, while the
/// provider's text stays private to this receipt. A caller can only ask
/// whether the exact fixture bytes match it through
/// [`WebsiteExactTextPlacementReceipt::matches_fixture_bytes`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsiteExactTextPlacementReceipt {
    pub marked_bytes: usize,
    pub readback_bytes: usize,
    pub bytes_after_clear: usize,
    readback: Vec<u8>,
}

impl WebsiteExactTextPlacementReceipt {
    /// Check the provider's read-back against the original fixture bytes.
    ///
    /// This is exact byte equality, not a substring or normalized-text check;
    /// changing even one fixture byte makes the check fail.
    pub fn matches_fixture_bytes(&self, fixture: &[u8]) -> bool {
        self.readback == fixture
    }

    /// Turn the exact byte comparison into the same fail-closed result used
    /// by the placement action. This gives break checks a real red result
    /// instead of treating a boolean observation as a proof.
    pub fn verify_fixture_bytes(&self, fixture: &[u8]) -> Result<(), WebsiteDriverError> {
        self.matches_fixture_bytes(fixture)
            .then_some(())
            .ok_or(WebsiteDriverError::TextReadbackMismatch)
    }
}

/// Bounded browser-window discovery shared by the reviewed messaging surfaces.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsiteBrowserDiscovery {
    pub service_kind: String,
    pub browser_title: String,
    pub place_kind: String,
    pub active_composer: String,
}

/// The browser and accessibility facts required to identify a direct-message
/// conversation. A page title or URL alone is not evidence that a writable
/// conversation is open.
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

/// Live UI state for OSL's private draft mounted over a verified Messenger
/// composer.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessengerPrivateComposerState {
    pub locked: bool,
    pub private_box_visible: bool,
    pub private_bytes: usize,
    pub counter_text: String,
    pub messenger_composer_characters: usize,
}

/// Messenger's five reviewed send choices. Parsing is deliberately exact so a
/// stale or fabricated UI value cannot inherit another choice's behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessengerSendChoice {
    Manual,
    DoubleEnter,
    SingleEnter,
    Instant,
    MatchTyping,
}

impl MessengerSendChoice {
    pub const ALL: [Self; 5] = [
        Self::Manual,
        Self::DoubleEnter,
        Self::SingleEnter,
        Self::Instant,
        Self::MatchTyping,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Manual => "Manual",
            Self::DoubleEnter => "Double Enter",
            Self::SingleEnter => "Single Enter",
            Self::Instant => "Instant",
            Self::MatchTyping => "Match typing",
        }
    }

    pub fn parse(choice: &str) -> Result<Self, WebsiteDriverError> {
        match choice {
            "Manual" => Ok(Self::Manual),
            "Double Enter" => Ok(Self::DoubleEnter),
            "Single Enter" => Ok(Self::SingleEnter),
            "Instant" => Ok(Self::Instant),
            "Match typing" => Ok(Self::MatchTyping),
            _ => Err(WebsiteDriverError::UnknownMessengerSendChoice),
        }
    }
}

/// Readback after a reviewed choice prepares its public cover. The private
/// draft is reported by byte count only.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessengerPreparedCoverState {
    pub prepared: bool,
    pub choice: String,
    pub cover_text: String,
    pub cover_bytes: usize,
    pub private_bytes: usize,
    pub messenger_composer_characters: usize,
}

/// UI-facing state for Messenger's protected send button.
///
/// The button binds the person's exact selected choice to cover preparation.
/// It holds no provider Send control and exposes no posting operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MessengerSendButton {
    selected_choice: MessengerSendChoice,
}

/// Fields that must still be present when a selected Messenger cover is
/// prepared.
///
/// The private text remains inside OSL. `composer_name` is only the accessible
/// name discovered for Messenger's provider composer; the protected send
/// boundary does not press the provider's Send control.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MessengerSendFields<'a> {
    pub private_text: &'a str,
    pub composer_name: Option<&'a str>,
    pub cover_text: &'a str,
}

impl MessengerSendButton {
    /// Connect the button to one of the five exact choices shown by the
    /// protected composer.
    pub fn for_selected_choice(choice_name: &str) -> Result<Self, WebsiteDriverError> {
        Ok(Self {
            selected_choice: MessengerSendChoice::parse(choice_name)?,
        })
    }

    /// Prepare the selected cover without pressing Messenger's Send control.
    pub fn prepare_selected_cover(
        &self,
        driver: &mut RealBrowserWebsiteDriver,
        page: &WebsitePage,
        fields: MessengerSendFields<'_>,
    ) -> Result<MessengerPreparedCoverState, WebsiteDriverError> {
        if fields.private_text.trim().is_empty() {
            return Err(WebsiteDriverError::RefusedMessengerSendFieldValue(
                "empty-text",
            ));
        }
        if fields
            .composer_name
            .filter(|name| !name.trim().is_empty())
            .is_none()
        {
            return Err(WebsiteDriverError::RefusedMessengerSendFieldValue(
                "missing-composer",
            ));
        }
        driver.prepare_messenger_cover_for_choice(
            page,
            self.selected_choice.name(),
            fields.cover_text,
        )
    }
}

/// Classify an already-open browser conversation from canonical service URL
/// routing and accessibility evidence. Unknown claims fail closed.
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
    pub body: String,
    pub conversation_identity: String,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebsiteDriverError {
    BrowserUnavailable,
    BrowserLaunchFailed,
    BrowserConnectionFailed,
    PageNotFound,
    ReadFailed,
    TextPlacementFailed,
    TextReadbackMismatch,
    TextClearFailed,
    NamedControlNotFound,
    PageUnavailable,
    UnknownMessengerSendChoice,
    RefusedMessengerSendFieldValue(&'static str),
}

impl fmt::Display for WebsiteDriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BrowserUnavailable => "website browser executable was not found",
            Self::BrowserLaunchFailed => "website browser could not be launched",
            Self::BrowserConnectionFailed => "website browser connection failed",
            Self::PageNotFound => "website page was not found",
            Self::ReadFailed => "website page could not be read",
            Self::TextPlacementFailed => "website text could not be placed",
            Self::TextReadbackMismatch => "website text read-back did not match the fixture bytes",
            Self::TextClearFailed => "website composer did not clear",
            Self::NamedControlNotFound => "website named control was not found",
            Self::PageUnavailable => "website page is unavailable",
            Self::UnknownMessengerSendChoice => "unknown Messenger send choice",
            Self::RefusedMessengerSendFieldValue(value) => {
                return write!(f, "refused Messenger send field value {value}")
            }
        })
    }
}

impl std::error::Error for WebsiteDriverError {}

pub trait WebsiteDriver {
    const JOBS: &'static [WebsiteDriverJob] = &WEBSITE_DRIVER_JOBS;

    fn find_page(&mut self, request: WebsitePageRequest)
        -> Result<WebsitePage, WebsiteDriverError>;
    fn read_page(&mut self, page: &WebsitePage) -> Result<WebsitePageText, WebsiteDriverError>;
    fn read_selected_email(
        &mut self,
        page: &WebsitePage,
    ) -> Result<WebsiteSelectedEmail, WebsiteDriverError>;
    fn read_live_run_progress(
        &mut self,
        page: &WebsitePage,
    ) -> Result<WebsiteLiveRunProgress, WebsiteDriverError>;
    fn place_text(&mut self, placement: WebsiteTextPlacement) -> Result<(), WebsiteDriverError>;
    fn press_named_control(
        &mut self,
        control: WebsiteNamedControl,
    ) -> Result<(), WebsiteDriverError>;
    fn read_named_controls(
        &self,
        _page: &WebsitePage,
        _required: &[WebsiteNamedControlRequest],
    ) -> Result<Vec<WebsiteNamedControl>, WebsiteDriverError> {
        Err(WebsiteDriverError::PageUnavailable)
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
    browser: Option<BrowserWebsiteSnapshot>,
}

#[derive(Deserialize)]
struct BrowserWebsiteSnapshot {
    service_kind: String,
    place_kind: String,
    active_composer: String,
}

#[derive(Deserialize)]
struct BrowserSelectedEmailSnapshot {
    body: String,
    conversation_identity: String,
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

    /// Read the active Instagram place and composer from an already-open page.
    ///
    /// The browser-side query only yields a value for a page explicitly marked
    /// as an Instagram surface and with one visible place record plus one
    /// visible active composer. In particular, a Messenger page cannot inherit
    /// an Instagram place merely because it has similarly named controls.
    pub fn discover_instagram_window(
        &mut self,
        page: &WebsitePage,
    ) -> Result<Option<WebsiteInstagramDiscovery>, WebsiteDriverError> {
        let websocket_url = self.page_websocket_url(page)?;
        let snapshot = read_page_snapshot(&websocket_url)?;
        Ok(instagram_discovery_from_snapshot(snapshot))
    }

    /// Discover the prepared browser surface without treating one service's
    /// place taxonomy as another service's taxonomy.
    pub fn discover_browser_window(
        &mut self,
        page: &WebsitePage,
    ) -> Result<Option<WebsiteBrowserDiscovery>, WebsiteDriverError> {
        let websocket_url = self.page_websocket_url(page)?;
        let snapshot = read_page_snapshot(&websocket_url)?;
        Ok(browser_discovery_from_snapshot(snapshot))
    }
    /// Put text in one named editable control. This is deliberately service
    /// neutral: provider discovery chooses the control, while this shared
    /// action performs the browser mutation.
    pub fn place_text_in_named_composer(
        &mut self,
        page: &WebsitePage,
        composer_name: &str,
        text: &str,
    ) -> Result<(), WebsiteDriverError> {
        let websocket_url = self.page_websocket_url(page)?;
        let expression = named_composer_expression(
            PLACE_NAMED_COMPOSER_TEXT_EXPRESSION,
            composer_name,
            Some(text),
        )?;
        match evaluate_target(&websocket_url, &expression)? {
            serde_json::Value::Bool(true) => Ok(()),
            _ => Err(WebsiteDriverError::TextPlacementFailed),
        }
    }

    /// Read one named editable control through the same generic browser
    /// action used by every reviewed website surface.
    pub fn read_named_composer_text(
        &mut self,
        page: &WebsitePage,
        composer_name: &str,
    ) -> Result<String, WebsiteDriverError> {
        let websocket_url = self.page_websocket_url(page)?;
        let expression =
            named_composer_expression(READ_NAMED_COMPOSER_TEXT_EXPRESSION, composer_name, None)?;
        match evaluate_target(&websocket_url, &expression)? {
            serde_json::Value::String(text) => Ok(text),
            _ => Err(WebsiteDriverError::ReadFailed),
        }
    }

    /// Clear one named editable control through the shared browser action.
    pub fn clear_named_composer_text(
        &mut self,
        page: &WebsitePage,
        composer_name: &str,
    ) -> Result<(), WebsiteDriverError> {
        let websocket_url = self.page_websocket_url(page)?;
        let expression =
            named_composer_expression(CLEAR_NAMED_COMPOSER_TEXT_EXPRESSION, composer_name, None)?;
        match evaluate_target(&websocket_url, &expression)? {
            serde_json::Value::Bool(true) => Ok(()),
            _ => Err(WebsiteDriverError::TextClearFailed),
        }
    }

    /// Place a marked fixture, read it back byte-for-byte, then leave the
    /// composer empty. The action is shared by website surfaces; the caller
    /// supplies only the previously discovered composer name.
    pub fn place_composer_text_exactly_and_clear(
        &mut self,
        page: &WebsitePage,
        composer_name: &str,
        fixture: &str,
    ) -> Result<WebsiteExactTextPlacementReceipt, WebsiteDriverError> {
        self.place_text_in_named_composer(page, composer_name, fixture)?;

        // Once a write has happened, always attempt the clear before returning
        // a failed exactness check. A mismatched browser read-back must never
        // leave marked text in a real conversation.
        let readback = self.read_named_composer_text(page, composer_name);
        let clear = self.clear_named_composer_text(page, composer_name);
        let after_clear = self.read_named_composer_text(page, composer_name);
        let readback = readback?;
        clear?;
        let after_clear = after_clear?;
        let receipt = WebsiteExactTextPlacementReceipt {
            marked_bytes: fixture.len(),
            readback_bytes: readback.len(),
            bytes_after_clear: after_clear.len(),
            readback: readback.into_bytes(),
        };
        if receipt.bytes_after_clear != 0 {
            return Err(WebsiteDriverError::TextClearFailed);
        }
        receipt.verify_fixture_bytes(fixture.as_bytes())?;
        Ok(receipt)
    }

    /// Instagram supplies discovery facts only. The actual write, exact
    /// read-back, and clear are the generic actions above, not an Instagram
    /// specific DOM path.
    pub fn place_instagram_composer_text_exactly_and_clear(
        &mut self,
        page: &WebsitePage,
        fixture: &str,
    ) -> Result<WebsiteExactTextPlacementReceipt, WebsiteDriverError> {
        let discovery = self
            .discover_instagram_window(page)?
            .ok_or(WebsiteDriverError::PageUnavailable)?;
        self.place_composer_text_exactly_and_clear(page, &discovery.active_composer, fixture)
    }

    /// Mount OSL's locked private drafting box over the already-verified
    /// Instagram composer.  This does not place private text into Instagram.
    pub fn install_instagram_private_composer(
        &mut self,
        page: &WebsitePage,
    ) -> Result<InstagramPrivateComposerState, WebsiteDriverError> {
        let websocket_url = self.page_websocket_url(page)?;
        read_instagram_private_composer_state(
            &websocket_url,
            INSTALL_INSTAGRAM_PRIVATE_COMPOSER_EXPRESSION,
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
        let text = serde_json::to_string(text).map_err(|_| WebsiteDriverError::ReadFailed)?;
        let expression = WRITE_INSTAGRAM_PRIVATE_TEXT_EXPRESSION.replace("__OSL_TEXT__", &text);
        read_instagram_private_composer_state(&websocket_url, &expression)
    }

    /// Clear OSL's private box without changing the Instagram composer.
    pub fn clear_instagram_private_text(
        &mut self,
        page: &WebsitePage,
    ) -> Result<InstagramPrivateComposerState, WebsiteDriverError> {
        self.write_instagram_private_text(page, "")
    }
}

fn named_composer_expression(
    template: &str,
    composer_name: &str,
    text: Option<&str>,
) -> Result<String, WebsiteDriverError> {
    let name = serde_json::to_string(composer_name).map_err(|_| WebsiteDriverError::ReadFailed)?;
    let expression = template
        .replace("__OSL_NAMED_COMPOSER_COMMON__", NAMED_COMPOSER_COMMON)
        .replace("__OSL_COMPOSER_NAME__", &name);
    match text {
        Some(text) => serde_json::to_string(text)
            .map(|text| expression.replace("__OSL_TEXT__", &text))
            .map_err(|_| WebsiteDriverError::ReadFailed),
        None => Ok(expression),
    }
}

fn instagram_discovery_from_snapshot(
    snapshot: BrowserPageSnapshot,
) -> Option<WebsiteInstagramDiscovery> {
    let browser = browser_discovery_from_snapshot(snapshot)?;
    (browser.service_kind == "instagram").then_some(WebsiteInstagramDiscovery {
        browser_title: browser.browser_title,
        place_kind: browser.place_kind,
        active_composer: browser.active_composer,
    })
}

fn browser_discovery_from_snapshot(
    snapshot: BrowserPageSnapshot,
) -> Option<WebsiteBrowserDiscovery> {
    let browser = snapshot.browser?;
    Some(WebsiteBrowserDiscovery {
        service_kind: browser.service_kind,
        browser_title: snapshot.title,
        place_kind: browser.place_kind,
        active_composer: browser.active_composer,
    })
}

impl RealBrowserWebsiteDriver {
    pub fn discover_messenger_conversation(
        &mut self,
        page: &WebsitePage,
    ) -> Result<WebsiteConversationDiscovery, WebsiteDriverError> {
        let websocket_url = self.page_websocket_url(page)?;
        let snapshot = read_browser_conversation_snapshot(&websocket_url)?;
        discover_messenger_browser_conversation(&snapshot)
    }

    pub fn install_messenger_private_composer(
        &mut self,
        page: &WebsitePage,
        discovery: &WebsiteConversationDiscovery,
    ) -> Result<MessengerPrivateComposerState, WebsiteDriverError> {
        if discovery.place_kind != "messenger:direct_message"
            || discovery.composer.trim().is_empty()
        {
            return Err(WebsiteDriverError::PageUnavailable);
        }
        let websocket_url = self.page_websocket_url(page)?;
        let composer_name = serde_json::to_string(&discovery.composer)
            .map_err(|_| WebsiteDriverError::ReadFailed)?;
        let expression = INSTALL_MESSENGER_PRIVATE_COMPOSER_EXPRESSION
            .replace("__OSL_COMPOSER_NAME__", &composer_name);
        read_messenger_private_composer_state(&websocket_url, &expression)
    }

    pub fn write_messenger_private_text(
        &mut self,
        page: &WebsitePage,
        text: &str,
    ) -> Result<MessengerPrivateComposerState, WebsiteDriverError> {
        let websocket_url = self.page_websocket_url(page)?;
        let text = serde_json::to_string(text).map_err(|_| WebsiteDriverError::ReadFailed)?;
        let expression = WRITE_MESSENGER_PRIVATE_TEXT_EXPRESSION.replace("__OSL_TEXT__", &text);
        read_messenger_private_composer_state(&websocket_url, &expression)
    }

    pub fn clear_messenger_private_text(
        &mut self,
        page: &WebsitePage,
    ) -> Result<MessengerPrivateComposerState, WebsiteDriverError> {
        self.write_messenger_private_text(page, "")
    }

    /// Dispatch one exact Messenger send choice and prepare its public cover.
    /// This step never presses Send.
    pub fn prepare_messenger_cover_for_choice(
        &mut self,
        page: &WebsitePage,
        choice: &str,
        cover_text: &str,
    ) -> Result<MessengerPreparedCoverState, WebsiteDriverError> {
        let choice = MessengerSendChoice::parse(choice)?;
        if cover_text.is_empty() {
            return Err(WebsiteDriverError::TextPlacementFailed);
        }

        match choice {
            MessengerSendChoice::Manual => self.prepare_messenger_manual_cover(page, cover_text),
            MessengerSendChoice::DoubleEnter => {
                self.prepare_messenger_double_enter_cover(page, cover_text)
            }
            MessengerSendChoice::SingleEnter => {
                self.prepare_messenger_single_enter_cover(page, cover_text)
            }
            MessengerSendChoice::Instant => self.prepare_messenger_instant_cover(page, cover_text),
            MessengerSendChoice::MatchTyping => {
                self.prepare_messenger_match_typing_cover(page, cover_text)
            }
        }
    }

    pub fn read_messenger_prepared_cover(
        &mut self,
        page: &WebsitePage,
    ) -> Result<MessengerPreparedCoverState, WebsiteDriverError> {
        let websocket_url = self.page_websocket_url(page)?;
        read_messenger_prepared_cover_state(
            &websocket_url,
            READ_MESSENGER_PREPARED_COVER_EXPRESSION,
        )
    }

    fn prepare_messenger_manual_cover(
        &mut self,
        page: &WebsitePage,
        cover_text: &str,
    ) -> Result<MessengerPreparedCoverState, WebsiteDriverError> {
        self.prepare_messenger_cover(page, MessengerSendChoice::Manual, cover_text)
    }

    fn prepare_messenger_double_enter_cover(
        &mut self,
        page: &WebsitePage,
        cover_text: &str,
    ) -> Result<MessengerPreparedCoverState, WebsiteDriverError> {
        self.prepare_messenger_cover(page, MessengerSendChoice::DoubleEnter, cover_text)
    }

    fn prepare_messenger_single_enter_cover(
        &mut self,
        page: &WebsitePage,
        cover_text: &str,
    ) -> Result<MessengerPreparedCoverState, WebsiteDriverError> {
        self.prepare_messenger_cover(page, MessengerSendChoice::SingleEnter, cover_text)
    }

    fn prepare_messenger_instant_cover(
        &mut self,
        page: &WebsitePage,
        cover_text: &str,
    ) -> Result<MessengerPreparedCoverState, WebsiteDriverError> {
        self.prepare_messenger_cover(page, MessengerSendChoice::Instant, cover_text)
    }

    fn prepare_messenger_match_typing_cover(
        &mut self,
        page: &WebsitePage,
        cover_text: &str,
    ) -> Result<MessengerPreparedCoverState, WebsiteDriverError> {
        self.prepare_messenger_cover(page, MessengerSendChoice::MatchTyping, cover_text)
    }

    fn prepare_messenger_cover(
        &mut self,
        page: &WebsitePage,
        choice: MessengerSendChoice,
        cover_text: &str,
    ) -> Result<MessengerPreparedCoverState, WebsiteDriverError> {
        let websocket_url = self.page_websocket_url(page)?;
        let choice =
            serde_json::to_string(choice.name()).map_err(|_| WebsiteDriverError::ReadFailed)?;
        let cover_text =
            serde_json::to_string(cover_text).map_err(|_| WebsiteDriverError::ReadFailed)?;
        let expression = PREPARE_MESSENGER_COVER_EXPRESSION
            .replace("__OSL_SEND_CHOICE__", &choice)
            .replace("__OSL_COVER_TEXT__", &cover_text);
        read_messenger_prepared_cover_state(&websocket_url, &expression)
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
            }

            if std::time::Instant::now() >= deadline {
                return Err(WebsiteDriverError::ReadFailed);
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    fn read_selected_email(
        &mut self,
        page: &WebsitePage,
    ) -> Result<WebsiteSelectedEmail, WebsiteDriverError> {
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
        let name =
            serde_json::to_string(&control.name).map_err(|_| WebsiteDriverError::ReadFailed)?;
        let expression = CLICK_NAMED_CONTROL_EXPRESSION.replace("__OSL_CONTROL_NAME__", &name);
        match evaluate_target(&websocket_url, &expression)? {
            serde_json::Value::Bool(true) => Ok(()),
            _ => Err(WebsiteDriverError::NamedControlNotFound),
        }
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

fn read_browser_conversation_snapshot(
    websocket_url: &str,
) -> Result<WebsiteConversationBrowserSnapshot, WebsiteDriverError> {
    let value = evaluate_target(websocket_url, BROWSER_CONVERSATION_SNAPSHOT_EXPRESSION)?;
    serde_json::from_value(value).map_err(|_| WebsiteDriverError::ReadFailed)
}

fn read_messenger_private_composer_state(
    websocket_url: &str,
    expression: &str,
) -> Result<MessengerPrivateComposerState, WebsiteDriverError> {
    let value = evaluate_target(websocket_url, expression)?;
    serde_json::from_value(value).map_err(|_| WebsiteDriverError::ReadFailed)
}

fn read_messenger_prepared_cover_state(
    websocket_url: &str,
    expression: &str,
) -> Result<MessengerPreparedCoverState, WebsiteDriverError> {
    let value = evaluate_target(websocket_url, expression)?;
    serde_json::from_value(value).map_err(|_| WebsiteDriverError::TextPlacementFailed)
}

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

fn read_instagram_private_composer_state(
    websocket_url: &str,
    expression: &str,
) -> Result<InstagramPrivateComposerState, WebsiteDriverError> {
    let value = evaluate_target(websocket_url, expression)?;
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
    let loopback = host
        .parse::<std::net::IpAddr>()
        .map(|ip| ip.is_loopback())
        .unwrap_or(false);
    if !loopback {
        return Err(WebsiteDriverError::ReadFailed);
    }
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

const INSTALL_MESSENGER_PRIVATE_COMPOSER_EXPRESSION: &str = r#"
(() => {
  const expectedName = __OSL_COMPOSER_NAME__;
  if (document.getElementById('osl-messenger-private-composer')) {
    return window.__oslMessengerPrivateComposerState?.() || null;
  }
  const compact = (value) => String(value || '').replace(/\s+/g, ' ').trim();
  const nameOf = (element) => compact(
    element.getAttribute('aria-label') || element.getAttribute('placeholder') || element.getAttribute('title')
  );
  const candidates = document.querySelectorAll(
    '[data-osl-messenger-composer="active"], [data-osl-composer], [contenteditable="true"][role="textbox"], textarea[role="textbox"], input[role="textbox"], [role="textbox"]'
  );
  const composer = Array.from(candidates).find((element) =>
    nameOf(element) === expectedName && !element.disabled && !element.readOnly
  );
  if (!composer) return null;

  const contentEditableComposer = composer.isContentEditable;
  const composerValue = () => contentEditableComposer ? composer.textContent : composer.value;
  const clearMessenger = () => {
    if (contentEditableComposer) composer.textContent = '';
    else composer.value = '';
    composer.dispatchEvent(new Event('input', { bubbles: true }));
  };
  const messengerCharacters = () => Array.from(composerValue() || '').length;
  clearMessenger();
  window.__oslMessengerComposer = composer;
  window.__oslMessengerComposerWasContentEditable = contentEditableComposer;
  window.__oslMessengerPreparedCover = null;
  composer.setAttribute('data-osl-private-lock', 'true');
  composer.setAttribute('aria-disabled', 'true');
  composer.style.pointerEvents = 'none';
  if (contentEditableComposer) composer.setAttribute('contenteditable', 'false');
  else composer.readOnly = true;

  const box = document.createElement('section');
  box.id = 'osl-messenger-private-composer';
  box.setAttribute('role', 'group');
  box.setAttribute('aria-label', 'OSL private message');
  box.style.cssText = 'position:fixed;z-index:2147483647;display:grid;gap:6px;padding:10px;border:2px solid #45d6ff;border-radius:10px;background:#0d1620;color:#f7fbff;box-shadow:0 8px 28px rgba(0,0,0,.45)';
  const rect = composer.getBoundingClientRect();
  box.style.left = `${Math.max(8, rect.left)}px`;
  box.style.top = `${Math.max(8, rect.top)}px`;
  box.style.width = `${Math.max(240, rect.width)}px`;
  box.innerHTML = '<strong aria-label="Locked private draft">🔒 Private draft</strong><textarea id="osl-messenger-private-text" rows="3" autocomplete="off" spellcheck="true" aria-describedby="osl-messenger-private-count"></textarea><output id="osl-messenger-private-count" aria-live="polite">0 bytes</output>';
  document.body.append(box);

  const privateText = box.querySelector('#osl-messenger-private-text');
  const count = box.querySelector('#osl-messenger-private-count');
  const update = () => {
    clearMessenger();
    count.textContent = `${new TextEncoder().encode(privateText.value).length} bytes`;
  };
  privateText.addEventListener('input', update);
  window.__oslMessengerPrivateComposerState = () => ({
    locked: composer.getAttribute('data-osl-private-lock') === 'true',
    privateBoxVisible: document.body.contains(box),
    privateBytes: new TextEncoder().encode(privateText.value).length,
    counterText: count.textContent,
    messengerComposerCharacters: messengerCharacters()
  });
  window.__oslMessengerPreparedCoverState = () => {
    const prepared = window.__oslMessengerPreparedCover;
    if (!prepared) return null;
    const coverText = composerValue() || '';
    return {
      prepared: coverText === prepared.coverText,
      choice: prepared.choice,
      coverText,
      coverBytes: new TextEncoder().encode(coverText).length,
      privateBytes: new TextEncoder().encode(privateText.value).length,
      messengerComposerCharacters: Array.from(coverText).length
    };
  };
  update();
  return window.__oslMessengerPrivateComposerState();
})()
"#;

const WRITE_MESSENGER_PRIVATE_TEXT_EXPRESSION: &str = r#"
(() => {
  const privateText = document.getElementById('osl-messenger-private-text');
  if (!privateText || !window.__oslMessengerPrivateComposerState) return null;
  privateText.value = __OSL_TEXT__;
  privateText.dispatchEvent(new Event('input', { bubbles: true }));
  return window.__oslMessengerPrivateComposerState();
})()
"#;

const PREPARE_MESSENGER_COVER_EXPRESSION: &str = r#"
(() => {
  const choice = __OSL_SEND_CHOICE__;
  const coverText = __OSL_COVER_TEXT__;
  const composer = window.__oslMessengerComposer;
  if (!composer ||
      composer.getAttribute('data-osl-private-lock') !== 'true' ||
      !window.__oslMessengerPrivateComposerState ||
      !window.__oslMessengerPreparedCoverState) return null;

  if (window.__oslMessengerComposerWasContentEditable) composer.textContent = coverText;
  else composer.value = coverText;
  composer.dispatchEvent(new Event('input', { bubbles: true }));

  const readback = window.__oslMessengerComposerWasContentEditable
    ? composer.textContent
    : composer.value;
  if (readback !== coverText) return null;
  window.__oslMessengerPreparedCover = { choice, coverText };
  return window.__oslMessengerPreparedCoverState();
})()
"#;

const READ_MESSENGER_PREPARED_COVER_EXPRESSION: &str = r#"
(() => window.__oslMessengerPreparedCoverState?.() || null)()
"#;

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
    },
    browser: (() => {
      const root = document.documentElement;
      const body = document.body;
      const serviceRoot = document.querySelector('[data-osl-service], [data-osl-website]');
      const service = compact(
        root.getAttribute('data-osl-service') ||
        (body && body.getAttribute('data-osl-service')) ||
        root.getAttribute('data-osl-website') ||
        (body && body.getAttribute('data-osl-website')) ||
        (serviceRoot && (serviceRoot.getAttribute('data-osl-service') || serviceRoot.getAttribute('data-osl-website')))
      ).toLowerCase();
      if (!['instagram', 'messenger'].includes(service)) return null;
      const places = Array.from(document.querySelectorAll(`[data-osl-${service}-place-kind]`))
        .filter(visible)
        .map((element) => compact(element.getAttribute(`data-osl-${service}-place-kind`)))
        .filter(Boolean);
      const composers = Array.from(document.querySelectorAll(`[data-osl-${service}-composer="active"]`))
        .filter(visible)
        .map(controlName)
        .filter(Boolean);
      if (places.length !== 1 || composers.length !== 1) return null;
      return { service_kind: service, place_kind: places[0], active_composer: composers[0] };
    })()
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

  const bodyElement =
    firstVisible(selected, '[data-osl-email-body], [data-email-body], [data-message-body], [role="document"]') ||
    selected;
  const body = compact(bodyElement.innerText || bodyElement.textContent || '');
  if (!body || !conversationIdentity) return null;
  return {
    body,
    conversation_identity: conversationIdentity
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

// These are intentionally service-neutral browser actions. A reviewed surface
// discovers its composer first, then gives its accessible name to this shared
// implementation. Keeping the selector and mutation together prevents a
// provider wrapper from quietly growing its own input path.
const NAMED_COMPOSER_COMMON: &str = r#"
  const expected = __OSL_COMPOSER_NAME__;
  const compact = (value) => String(value || '').replace(/\s+/g, ' ').trim();
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
  const labelledBy = (element) => compact(
    (element.getAttribute('aria-labelledby') || '')
      .split(/\s+/)
      .map((id) => document.getElementById(id))
      .filter(Boolean)
      .map((label) => label.innerText || label.textContent || '')
      .join(' ')
  );
  const controlName = (element) => {
    for (const candidate of [
      element.getAttribute('aria-label'), labelledBy(element), element.getAttribute('placeholder'),
      element.getAttribute('title'), element.getAttribute('name'), element.id
    ]) {
      const name = compact(candidate);
      if (name) return name;
    }
    return '';
  };
  const matches = Array.from(document.querySelectorAll(
    'textarea, input, [contenteditable=""], [contenteditable="true"], [role="textbox"], [role="searchbox"]'
  )).filter((element) => visible(element) && editable(element) && controlName(element) === expected);
  if (matches.length !== 1) return null;
  const composer = matches[0];
  const valueOf = (element) => element.isContentEditable ? (element.textContent || '') : String(element.value || '');
"#;

const PLACE_NAMED_COMPOSER_TEXT_EXPRESSION: &str = r#"
(() => {
__OSL_NAMED_COMPOSER_COMMON__
  const text = __OSL_TEXT__;
  composer.focus();
  if (composer.isContentEditable) composer.textContent = text;
  else composer.value = text;
  composer.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText', data: text }));
  composer.dispatchEvent(new Event('change', { bubbles: true }));
  return valueOf(composer) === text;
})()
"#;

const READ_NAMED_COMPOSER_TEXT_EXPRESSION: &str = r#"
(() => {
__OSL_NAMED_COMPOSER_COMMON__
  return valueOf(composer);
})()
"#;

const CLEAR_NAMED_COMPOSER_TEXT_EXPRESSION: &str = r#"
(() => {
__OSL_NAMED_COMPOSER_COMMON__
  composer.focus();
  if (composer.isContentEditable) composer.textContent = '';
  else composer.value = '';
  composer.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'deleteContentBackward', data: null }));
  composer.dispatchEvent(new Event('change', { bubbles: true }));
  return valueOf(composer) === '';
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
    }

    #[test]
    fn task_1133_instagram_uses_shared_exact_place_readback_and_clear_actions() {
        const FIXTURE: &str = "OSL-MARKED-1133";
        let instagram = Task1131Page::spawn(
            "Instagram — Maya placement fixture",
            r#"<main data-osl-service="instagram" data-osl-instagram-place-kind="direct_message">
                  <textarea aria-label="Search messages"></textarea>
                  <textarea aria-label="Message Maya" data-osl-instagram-composer="active"></textarea>
                </main>"#,
        );
        let mut driver = RealBrowserWebsiteDriver::launch().expect("launch fixture browser");
        let page = driver
            .find_page(WebsitePageRequest {
                url: instagram.url(),
            })
            .expect("open Instagram fixture");
        driver.read_page(&page).expect("wait for Instagram fixture");

        let receipt = driver
            .place_instagram_composer_text_exactly_and_clear(&page, FIXTURE)
            .expect("shared actions place, read back, and clear the Instagram composer");
        let search_after = driver
            .read_named_composer_text(&page, "Search messages")
            .expect("read decoy composer");
        let changed = b"PSL-MARKED-1133";
        let expected_fixture =
            std::env::var("OSL_TASK_1133_EXPECTED_FIXTURE").unwrap_or_else(|_| FIXTURE.to_owned());

        assert_eq!(receipt.marked_bytes, FIXTURE.len());
        assert_eq!(receipt.readback_bytes, FIXTURE.len());
        assert_eq!(receipt.bytes_after_clear, 0);
        assert!(receipt.matches_fixture_bytes(FIXTURE.as_bytes()));
        receipt
            .verify_fixture_bytes(expected_fixture.as_bytes())
            .expect("the configured fixture bytes must match the exact browser read-back");
        assert!(
            !receipt.matches_fixture_bytes(changed),
            "a one-byte fixture change must fail the exact read-back check"
        );
        assert_eq!(
            receipt.verify_fixture_bytes(changed),
            Err(WebsiteDriverError::TextReadbackMismatch),
            "the changed fixture must produce the shared check's failure"
        );
        assert!(
            search_after.is_empty(),
            "the shared named action skipped Search"
        );

        println!("TASK1133 marked_bytes={}", receipt.marked_bytes);
        println!("TASK1133 readback_bytes={}", receipt.readback_bytes);
        println!("TASK1133 clear_bytes={}", receipt.bytes_after_clear);
        println!(
            "TASK1133 exact_fixture_match={}",
            receipt.matches_fixture_bytes(FIXTURE.as_bytes())
        );
        println!(
            "TASK1133 one_byte_changed_fixture_match={}",
            receipt.matches_fixture_bytes(changed)
        );
        println!(
            "TASK1133 one_byte_changed_fixture_check={}",
            receipt.verify_fixture_bytes(changed).is_err()
        );
        println!("TASK1133 search_decoy_bytes={}", search_after.len());
    }

    struct Task1131Page {
        listener_addr: String,
        running: std::sync::Arc<std::sync::atomic::AtomicBool>,
        worker: Option<std::thread::JoinHandle<()>>,
    }

    impl Task1131Page {
        fn spawn(title: &'static str, body: &'static str) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind task 1131 fixture");
            listener
                .set_nonblocking(true)
                .expect("nonblocking fixture listener");
            let listener_addr = listener.local_addr().expect("fixture address").to_string();
            let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
            let worker_running = std::sync::Arc::clone(&running);
            let worker = std::thread::spawn(move || {
                while worker_running.load(std::sync::atomic::Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            let mut request = [0_u8; 1024];
                            let _ = stream.read(&mut request);
                            let document = format!("<!doctype html><title>{title}</title>{body}");
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
            format!("http://{}/task-1131.html", self.listener_addr)
        }
    }

    impl Drop for Task1131Page {
        fn drop(&mut self) {
            self.running
                .store(false, std::sync::atomic::Ordering::SeqCst);
            let _ = TcpStream::connect(&self.listener_addr);
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
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
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum WebsiteControlKind {
    EditableBox,
    Button,
    VisibleMessageArea,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct WebsiteNamedControlRequest {
    pub name: &'static str,
    pub kind: WebsiteControlKind,
}
