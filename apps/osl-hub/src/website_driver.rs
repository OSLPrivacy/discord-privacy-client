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
        }
    }
}

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

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum WebsiteDriverError {
    BrowserUnavailable,
    BrowserLaunchFailed(String),
    DevToolsUnavailable(String),
    PageUnavailable,
    InvalidUrl,
}

pub trait WebsiteDriver {
    fn kind(&self) -> WebsiteDriverKind;
    fn find_page(&mut self, url: &Url) -> Result<WebsitePage, WebsiteDriverError>;
    fn read_page(&self, page: &WebsitePage) -> Result<WebsitePageSnapshot, WebsiteDriverError>;
}

pub struct RealBrowserWebsiteDriver {
    child: Child,
    profile_dir: PathBuf,
    devtools_base: String,
    client: reqwest::blocking::Client,
    executable: PathBuf,
}

impl RealBrowserWebsiteDriver {
    pub fn launch() -> Result<Self, WebsiteDriverError> {
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
    }
}

impl Drop for RealBrowserWebsiteDriver {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.profile_dir);
    }
}

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
