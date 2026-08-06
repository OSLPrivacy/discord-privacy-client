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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsiteTextPlacement {
    pub page: WebsitePage,
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsiteNamedControl {
    pub page: WebsitePage,
    pub name: String,
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebsiteDriverError {
    BrowserUnavailable,
    BrowserLaunchFailed,
    BrowserConnectionFailed,
    PageNotFound,
    ReadFailed,
    TextPlacementFailed,
    NamedControlNotFound,
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
            Self::NamedControlNotFound => "website named control was not found",
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
    }
}
