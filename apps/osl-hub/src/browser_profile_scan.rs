//! Browser-profile history scan consent boundary.
//!
//! This module owns the native profile-content gate. Listing returns profile
//! labels only; scanning requires an exact fresh one-shot grant and reads only a
//! bounded snapshot copied into OSL-owned scratch storage.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::native_apps::BrowserImportId;

pub const CONSENT_GRANT_TTL: Duration = Duration::from_secs(5 * 60);
pub const CONSENT_GRANT_MAX_PENDING: usize = 16;
pub const MAX_BROWSER_PROFILE_LABEL_BYTES: usize = 128;
pub const MAX_LISTED_BROWSER_PROFILES: usize = 128;
pub const MAX_HISTORY_SNAPSHOT_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_RAW_WAL_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_SNAPSHOT_ROWS: usize = 256;

const PLAINTEXT_STATE_FILE: &str = "browser-profile-scan-state.json";
const HISTORY_SOURCE_ACCOUNT: &str = "browser-history";
const HISTORY_SCOPE: &str = "history-footprint";

static NEXT_GRANT: AtomicU64 = AtomicU64::new(1);
static NEXT_SNAPSHOT: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserProfileDescriptor {
    pub browser_id: BrowserImportId,
    pub profile: String,
    pub display_name: String,
}

#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserProfileConsentGrant {
    pub grant_id: String,
    pub browser_id: BrowserImportId,
    pub profile: String,
    pub expires_at_unix_ms: u64,
}

#[derive(Clone)]
struct PendingConsentGrant {
    owner_sha256: [u8; 32],
    browser_id: BrowserImportId,
    profile: String,
    grant_id: String,
    expires_at_unix_ms: u64,
}

pub struct BrowserProfileScanState {
    pub snapshot_root: PathBuf,
    listed_profiles: Vec<BrowserProfileDescriptor>,
    grants: Vec<PendingConsentGrant>,
    firefox_login_decryption_consent: bool,
}

#[derive(Clone, Default)]
pub struct BrowserProfileRoots {
    pub chrome: Option<PathBuf>,
    pub edge: Option<PathBuf>,
    pub firefox: Option<PathBuf>,
    pub brave: Option<PathBuf>,
    pub opera: Option<PathBuf>,
}

#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserProfileObservation {
    pub service: String,
    pub site: String,
}

#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserProfileScanReceipt {
    pub browser_id: BrowserImportId,
    pub profile: String,
    pub source_account: &'static str,
    pub scope: &'static str,
    pub run_id: String,
    pub observation_count: usize,
    pub snapshot_deleted: bool,
}

pub enum BrowserProfileTransition {
    Lock,
    Burn,
    IdentitySwitch,
    Unlock,
    Startup,
    ProfilesListed,
    ScanCompleted,
}

pub struct SnapshotGuard {
    snapshot_dir: PathBuf,
    snapshot_path: PathBuf,
}

impl SnapshotGuard {
    pub fn path(&self) -> &Path {
        &self.snapshot_path
    }

    pub fn snapshot_dir(&self) -> &Path {
        &self.snapshot_dir
    }
}

impl Drop for SnapshotGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.snapshot_dir);
    }
}

impl BrowserProfileScanState {
    pub fn load(snapshot_root: impl AsRef<Path>) -> Result<Self, String> {
        let snapshot_root = snapshot_root.as_ref().to_path_buf();
        if snapshot_root.join(PLAINTEXT_STATE_FILE).exists() {
            return Err("Browser profile scan state must not be plaintext".to_owned());
        }
        std::fs::create_dir_all(&snapshot_root)
            .map_err(|_| "Browser profile snapshot storage is unavailable".to_owned())?;
        drop_stale_snapshots(&snapshot_root)?;
        Ok(Self {
            snapshot_root,
            listed_profiles: Vec::new(),
            grants: Vec::new(),
            firefox_login_decryption_consent: false,
        })
    }

    pub fn list_profiles(
        &mut self,
        roots: &BrowserProfileRoots,
    ) -> Result<Vec<BrowserProfileDescriptor>, String> {
        let mut profiles = Vec::new();
        for browser_id in supported_scan_browsers() {
            let Some(root) = roots.root_for(browser_id) else {
                continue;
            };
            for profile in list_profiles_for_browser(browser_id, root)? {
                if profiles.len() >= MAX_LISTED_BROWSER_PROFILES {
                    break;
                }
                profiles.push(profile);
            }
        }
        self.listed_profiles = profiles.clone();
        Ok(profiles)
    }

    pub fn grant_profile_consent(
        &mut self,
        owner: &str,
        browser_id: BrowserImportId,
        profile: &str,
        now_unix_ms: u64,
    ) -> Result<BrowserProfileConsentGrant, String> {
        if owner_sha256(owner).is_none() {
            return Err("An unlocked OSL owner is required".to_owned());
        }
        if !history_scan_supported(browser_id) {
            return Err("This browser profile cannot be scanned".to_owned());
        }
        if !valid_profile_label(profile) {
            return Err("The browser profile label is invalid".to_owned());
        }
        if !self
            .listed_profiles
            .iter()
            .any(|listed| listed.browser_id == browser_id && listed.profile == profile)
        {
            return Err("Fresh browser profile inventory is required".to_owned());
        }

        self.prune_expired_grants(now_unix_ms);
        if self.grants.len() >= CONSENT_GRANT_MAX_PENDING {
            self.grants.remove(0);
        }

        let expires_at_unix_ms = now_unix_ms.saturating_add(CONSENT_GRANT_TTL.as_millis() as u64);
        let grant_id = mint_grant_id(owner, browser_id, profile, now_unix_ms);
        self.grants.push(PendingConsentGrant {
            owner_sha256: owner_sha256(owner).expect("owner already validated"),
            browser_id,
            profile: profile.to_owned(),
            grant_id: grant_id.clone(),
            expires_at_unix_ms,
        });
        Ok(BrowserProfileConsentGrant {
            grant_id,
            browser_id,
            profile: profile.to_owned(),
            expires_at_unix_ms,
        })
    }

    pub fn consume_profile_consent(
        &mut self,
        owner: &str,
        browser_id: BrowserImportId,
        profile: &str,
        grant_id: &str,
        now_unix_ms: u64,
    ) -> Result<(), String> {
        self.consume_profile_consent_inner(owner, browser_id, profile, grant_id, now_unix_ms)
            .map(|_| ())
    }

    pub fn scan_consented_profile(
        &mut self,
        owner: &str,
        roots: &BrowserProfileRoots,
        browser_id: BrowserImportId,
        profile: &str,
        grant_id: &str,
        now_unix_ms: u64,
    ) -> Result<BrowserProfileScanReceipt, String> {
        let grant =
            self.consume_profile_consent_inner(owner, browser_id, profile, grant_id, now_unix_ms)?;
        let Some(root) = roots.root_for(grant.browser_id) else {
            return Err("The browser profile root is unavailable".to_owned());
        };
        let profile_dir = resolve_profile_dir(root, &grant.profile)?;
        scan_resolved_profile(
            &self.snapshot_root,
            grant.browser_id,
            &grant.profile,
            &profile_dir,
        )
    }

    pub fn apply_transition(&mut self, transition: BrowserProfileTransition) {
        if matches!(
            transition,
            BrowserProfileTransition::Lock
                | BrowserProfileTransition::Burn
                | BrowserProfileTransition::IdentitySwitch
        ) {
            self.revoke_all();
        }
    }

    pub fn revoke_all(&mut self) {
        self.grants.clear();
        self.listed_profiles.clear();
        self.firefox_login_decryption_consent = false;
    }

    pub fn set_firefox_login_decryption_consent(&mut self, enabled: bool) {
        self.firefox_login_decryption_consent = enabled;
    }

    pub fn firefox_login_decryption_consent(&self) -> bool {
        self.firefox_login_decryption_consent
    }

    fn consume_profile_consent_inner(
        &mut self,
        owner: &str,
        browser_id: BrowserImportId,
        profile: &str,
        grant_id: &str,
        now_unix_ms: u64,
    ) -> Result<PendingConsentGrant, String> {
        let owner_sha256 =
            owner_sha256(owner).ok_or_else(|| "An unlocked OSL owner is required".to_owned())?;
        if !history_scan_supported(browser_id) || !valid_profile_label(profile) {
            return Err("The browser profile grant does not match".to_owned());
        }
        let Some(index) = self.grants.iter().position(|grant| {
            grant.owner_sha256 == owner_sha256
                && grant.browser_id == browser_id
                && grant.profile == profile
                && grant.grant_id == grant_id
        }) else {
            return Err("A fresh browser profile consent grant is required".to_owned());
        };
        let grant = self.grants.remove(index);
        if grant.expires_at_unix_ms <= now_unix_ms {
            return Err("The browser profile consent grant has expired".to_owned());
        }
        Ok(grant)
    }

    fn prune_expired_grants(&mut self, now_unix_ms: u64) {
        self.grants
            .retain(|grant| grant.expires_at_unix_ms > now_unix_ms);
    }
}

impl BrowserProfileRoots {
    fn root_for(&self, browser_id: BrowserImportId) -> Option<&Path> {
        match browser_id {
            BrowserImportId::Chrome => self.chrome.as_deref(),
            BrowserImportId::Edge => self.edge.as_deref(),
            BrowserImportId::Firefox => self.firefox.as_deref(),
            BrowserImportId::Brave => self.brave.as_deref(),
            BrowserImportId::Opera => self.opera.as_deref(),
            BrowserImportId::DuckDuckGo => None,
        }
    }
}

pub fn scan_resolved_profile(
    snapshot_root: &Path,
    browser_id: BrowserImportId,
    profile: &str,
    profile_dir: &Path,
) -> Result<BrowserProfileScanReceipt, String> {
    scan_resolved_profile_with_commit(snapshot_root, browser_id, profile, profile_dir, |_| Ok(()))
}

fn scan_resolved_profile_with_commit(
    snapshot_root: &Path,
    browser_id: BrowserImportId,
    profile: &str,
    profile_dir: &Path,
    commit: impl FnOnce(&[BrowserProfileObservation]) -> Result<(), String>,
) -> Result<BrowserProfileScanReceipt, String> {
    if !history_scan_supported(browser_id) {
        return Err("This browser profile cannot be scanned".to_owned());
    }
    if !valid_profile_label(profile) {
        return Err("The browser profile label is invalid".to_owned());
    }
    let history_path = history_database_path(browser_id, profile_dir);
    let snapshot = copy_bounded_snapshot(&history_path, snapshot_root)?;
    let snapshot_dir = snapshot.snapshot_dir().to_owned();
    let observations = query_snapshot(snapshot.path())?;
    drop(snapshot);
    if snapshot_dir.exists() {
        return Err("The browser profile history snapshot could not be deleted".to_owned());
    }
    commit(&observations)?;
    Ok(BrowserProfileScanReceipt {
        browser_id,
        profile: profile.to_owned(),
        source_account: HISTORY_SOURCE_ACCOUNT,
        scope: HISTORY_SCOPE,
        run_id: receipt_run_id(browser_id, profile, &observations),
        observation_count: observations.len(),
        snapshot_deleted: true,
    })
}

pub fn query_snapshot(snapshot_path: &Path) -> Result<Vec<BrowserProfileObservation>, String> {
    let bytes = std::fs::read(snapshot_path)
        .map_err(|_| "The browser profile history snapshot could not be read".to_owned())?;
    if bytes.len() as u64 > MAX_HISTORY_SNAPSHOT_BYTES {
        return Err("The browser profile history snapshot is too large".to_owned());
    }
    let text = String::from_utf8_lossy(&bytes);
    let mut seen = BTreeSet::new();
    let mut observations = Vec::new();
    for line in text.lines() {
        if observations.len() >= MAX_SNAPSHOT_ROWS {
            break;
        }
        let Some(host) = url::Url::parse(line.trim())
            .ok()
            .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
        else {
            continue;
        };
        if seen.insert(host.clone()) {
            observations.push(BrowserProfileObservation {
                service: host.clone(),
                site: host,
            });
        }
    }
    Ok(observations)
}

pub fn copy_bounded_snapshot(
    history_path: &Path,
    snapshot_root: &Path,
) -> Result<SnapshotGuard, String> {
    copy_bounded_snapshot_with_hook(history_path, snapshot_root, || {})
}

fn copy_bounded_snapshot_with_hook(
    history_path: &Path,
    snapshot_root: &Path,
    before_copy: impl FnOnce(),
) -> Result<SnapshotGuard, String> {
    verify_plain_bounded_file(history_path, MAX_HISTORY_SNAPSHOT_BYTES)?;
    verify_raw_wal_bound(history_path)?;
    std::fs::create_dir_all(snapshot_root)
        .map_err(|_| "Browser profile snapshot storage is unavailable".to_owned())?;
    let snapshot_dir = snapshot_root.join(format!(
        "snapshot-{}-{}",
        std::process::id(),
        NEXT_SNAPSHOT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&snapshot_dir)
        .map_err(|_| "The browser profile history snapshot could not be created".to_owned())?;
    let snapshot_path = snapshot_dir.join("History");
    let guard = SnapshotGuard {
        snapshot_dir,
        snapshot_path,
    };
    before_copy();
    verify_raw_wal_bound(history_path)?;
    std::fs::copy(history_path, guard.path())
        .map_err(|_| "The browser profile history snapshot could not be copied".to_owned())?;
    verify_plain_bounded_file(guard.path(), MAX_HISTORY_SNAPSHOT_BYTES)?;
    verify_raw_wal_bound(history_path)?;
    Ok(guard)
}

pub fn verify_raw_wal_bound(history_path: &Path) -> Result<(), String> {
    let wal_path = wal_path(history_path);
    match std::fs::symlink_metadata(&wal_path) {
        Ok(metadata) => {
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err("The browser profile history WAL is not a plain file".to_owned());
            }
            if metadata.len() > MAX_RAW_WAL_BYTES {
                return Err("The browser profile history WAL is too large".to_owned());
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("The browser profile history WAL could not be verified".to_owned()),
    }
}

fn list_profiles_for_browser(
    browser_id: BrowserImportId,
    root: &Path,
) -> Result<Vec<BrowserProfileDescriptor>, String> {
    if !history_scan_supported(browser_id) {
        return Ok(Vec::new());
    }
    let root_meta = std::fs::symlink_metadata(root)
        .map_err(|_| "The browser profile root is unavailable".to_owned())?;
    if !root_meta.is_dir() || root_meta.file_type().is_symlink() {
        return Err("The browser profile root is not a plain directory".to_owned());
    }

    let mut profiles = Vec::new();
    let entries = std::fs::read_dir(root)
        .map_err(|_| "The browser profile root could not be listed".to_owned())?;
    for entry in entries.filter_map(Result::ok) {
        if profiles.len() >= MAX_LISTED_BROWSER_PROFILES {
            break;
        }
        let metadata = match entry.file_type() {
            Ok(file_type) if file_type.is_dir() && !file_type.is_symlink() => entry.metadata().ok(),
            _ => None,
        };
        if metadata.is_none() {
            continue;
        }
        let Some(label) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !valid_profile_label(&label) {
            continue;
        }
        profiles.push(BrowserProfileDescriptor {
            browser_id,
            profile: label.clone(),
            display_name: label,
        });
    }
    profiles.sort_by(|left, right| {
        browser_token(left.browser_id)
            .cmp(browser_token(right.browser_id))
            .then_with(|| left.profile.cmp(&right.profile))
    });
    Ok(profiles)
}

fn resolve_profile_dir(root: &Path, profile: &str) -> Result<PathBuf, String> {
    if !valid_profile_label(profile) {
        return Err("The browser profile label is invalid".to_owned());
    }
    let root = root
        .canonicalize()
        .map_err(|_| "The browser profile root could not be verified".to_owned())?;
    let profile_dir = root.join(profile);
    let metadata = std::fs::symlink_metadata(&profile_dir)
        .map_err(|_| "The browser profile is unavailable".to_owned())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("The browser profile is not a plain directory".to_owned());
    }
    let canonical = profile_dir
        .canonicalize()
        .map_err(|_| "The browser profile could not be verified".to_owned())?;
    if !canonical.starts_with(&root) {
        return Err("The browser profile escaped its browser root".to_owned());
    }
    Ok(canonical)
}

fn drop_stale_snapshots(snapshot_root: &Path) -> Result<(), String> {
    let entries = std::fs::read_dir(snapshot_root)
        .map_err(|_| "Browser profile snapshot storage is unavailable".to_owned())?;
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.starts_with("snapshot-") {
            let path = entry.path();
            if path.is_dir() {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            }
            .map_err(|_| "A stale browser profile snapshot could not be removed".to_owned())?;
        }
    }
    Ok(())
}

fn verify_plain_bounded_file(path: &Path, max_bytes: u64) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "The browser profile history database is unavailable".to_owned())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("The browser profile history database is not a plain file".to_owned());
    }
    if metadata.len() > max_bytes {
        return Err("The browser profile history database is too large".to_owned());
    }
    Ok(())
}

fn supported_scan_browsers() -> [BrowserImportId; 5] {
    [
        BrowserImportId::Chrome,
        BrowserImportId::Edge,
        BrowserImportId::Firefox,
        BrowserImportId::Brave,
        BrowserImportId::Opera,
    ]
}

fn history_scan_supported(browser_id: BrowserImportId) -> bool {
    !matches!(browser_id, BrowserImportId::DuckDuckGo)
}

fn history_database_path(browser_id: BrowserImportId, profile_dir: &Path) -> PathBuf {
    match browser_id {
        BrowserImportId::Firefox => profile_dir.join("places.sqlite"),
        BrowserImportId::Chrome
        | BrowserImportId::Edge
        | BrowserImportId::Brave
        | BrowserImportId::Opera
        | BrowserImportId::DuckDuckGo => profile_dir.join("History"),
    }
}

fn valid_profile_label(profile: &str) -> bool {
    !profile.is_empty()
        && profile.len() <= MAX_BROWSER_PROFILE_LABEL_BYTES
        && profile != "."
        && profile != ".."
        && !profile.contains(['/', '\\', ':', '\0'])
        && profile
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b' ' | b'.' | b'_' | b'-'))
}

fn owner_sha256(owner: &str) -> Option<[u8; 32]> {
    if owner.is_empty() || owner.len() > 256 || owner.contains(['\0', '/', '\\']) {
        return None;
    }
    let mut digest = Sha256::new();
    digest.update(b"OSL/browser-profile-owner/v1");
    digest.update(owner.as_bytes());
    Some(digest.finalize().into())
}

fn browser_token(browser_id: BrowserImportId) -> &'static str {
    match browser_id {
        BrowserImportId::Chrome => "chrome",
        BrowserImportId::Edge => "edge",
        BrowserImportId::Firefox => "firefox",
        BrowserImportId::Brave => "brave",
        BrowserImportId::Opera => "opera",
        BrowserImportId::DuckDuckGo => "duckduckgo",
    }
}

fn mint_grant_id(
    owner: &str,
    browser_id: BrowserImportId,
    profile: &str,
    now_unix_ms: u64,
) -> String {
    let mut digest = Sha256::new();
    digest.update(b"OSL/browser-profile-consent-grant/v1");
    digest.update(owner.as_bytes());
    digest.update(browser_token(browser_id).as_bytes());
    digest.update(profile.as_bytes());
    digest.update(now_unix_ms.to_le_bytes());
    digest.update(NEXT_GRANT.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    let digest = digest.finalize();
    hex32(&digest)
}

fn receipt_run_id(
    browser_id: BrowserImportId,
    profile: &str,
    observations: &[BrowserProfileObservation],
) -> String {
    let mut digest = Sha256::new();
    digest.update(b"OSL/browser-profile-history-run/v1");
    digest.update(browser_token(browser_id).as_bytes());
    digest.update(profile.as_bytes());
    for observation in observations {
        digest.update(observation.service.as_bytes());
        digest.update(observation.site.as_bytes());
    }
    let digest = digest.finalize();
    hex32(&digest)
}

fn wal_path(history_path: &Path) -> PathBuf {
    let mut value: OsString = history_path.as_os_str().to_owned();
    value.push("-wal");
    PathBuf::from(value)
}

fn hex32(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(64);
    for byte in bytes.iter().take(32) {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[allow(dead_code)]
fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use serde_json::json;

    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        std::env::temp_dir().join(format!(
            "osl-browser-profile-scan-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn state(label: &str) -> BrowserProfileScanState {
        BrowserProfileScanState::load(temp_root(label)).expect("test state should load")
    }

    fn roots_with_chrome_profile(label: &str, profile: &str) -> (PathBuf, BrowserProfileRoots) {
        let root = temp_root(label);
        std::fs::create_dir_all(root.join(profile)).unwrap();
        (
            root.clone(),
            BrowserProfileRoots {
                chrome: Some(root),
                ..BrowserProfileRoots::default()
            },
        )
    }

    fn inventory_with_profile(
        label: &str,
        profile: &str,
    ) -> (BrowserProfileScanState, PathBuf, BrowserProfileRoots) {
        let (root, roots) = roots_with_chrome_profile(label, profile);
        let mut state = state(label);
        let profiles = state.list_profiles(&roots).unwrap();
        assert!(profiles.iter().any(
            |listed| listed.browser_id == BrowserImportId::Chrome && listed.profile == profile
        ));
        (state, root, roots)
    }

    #[test]
    fn browser_profile_consent_models_are_reconciled_default_deny() {
        let mut state = state("default-deny");
        assert!(!state.firefox_login_decryption_consent());
        assert!(state
            .grant_profile_consent("owner-a", BrowserImportId::Chrome, "Default", 1000)
            .is_err());
        assert!(state
            .scan_consented_profile(
                "owner-a",
                &BrowserProfileRoots::default(),
                BrowserImportId::Chrome,
                "Default",
                "missing-grant",
                1000,
            )
            .is_err());

        let descriptor = BrowserProfileDescriptor {
            browser_id: BrowserImportId::Chrome,
            profile: "Default".to_owned(),
            display_name: "Default".to_owned(),
        };
        let json = serde_json::to_value(descriptor).unwrap();
        assert_eq!(json["browserId"], "chrome");
        assert!(json.get("id").is_none());
        assert!(json.get("installed").is_none());
        assert!(json.get("opened").is_none());
    }

    #[test]
    fn browser_profile_descriptor_reports_only_browser_profile_and_display_name() {
        let descriptor = BrowserProfileDescriptor {
            browser_id: BrowserImportId::Firefox,
            profile: "Profiles.default-release".to_owned(),
            display_name: "Default Release".to_owned(),
        };
        let json = serde_json::to_value(descriptor).unwrap();
        assert_eq!(
            json,
            json!({
                "browserId": "firefox",
                "profile": "Profiles.default-release",
                "displayName": "Default Release"
            })
        );
    }

    #[test]
    fn browser_profile_consent_grant_is_single_use_and_bounded() {
        let (mut state, _root, _roots) = inventory_with_profile("grant-bounded", "Default");
        let grant = state
            .grant_profile_consent("owner-a", BrowserImportId::Chrome, "Default", 10_000)
            .unwrap();
        assert_eq!(grant.expires_at_unix_ms, 10_000 + 300_000);
        assert!(state
            .consume_profile_consent(
                "owner-a",
                BrowserImportId::Chrome,
                "Default",
                &grant.grant_id,
                10_001,
            )
            .is_ok());
        assert!(state
            .consume_profile_consent(
                "owner-a",
                BrowserImportId::Chrome,
                "Default",
                &grant.grant_id,
                10_002,
            )
            .is_err());

        for offset in 0..(CONSENT_GRANT_MAX_PENDING + 4) {
            let grant = state
                .grant_profile_consent(
                    "owner-a",
                    BrowserImportId::Chrome,
                    "Default",
                    20_000 + offset as u64,
                )
                .unwrap();
            assert_eq!(grant.browser_id, BrowserImportId::Chrome);
        }
        assert_eq!(state.grants.len(), CONSENT_GRANT_MAX_PENDING);
    }

    #[test]
    fn browser_profile_scan_state_has_snapshot_profiles_grants_and_consent_fields() {
        let snapshot_root = temp_root("state-fields");
        let state = BrowserProfileScanState::load(&snapshot_root).unwrap();
        assert_eq!(state.snapshot_root, snapshot_root);
        assert!(state.listed_profiles.is_empty());
        assert!(state.grants.is_empty());
        assert!(!state.firefox_login_decryption_consent);
    }

    #[test]
    fn browser_profile_scan_consumes_exact_one_shot_consent() {
        let (mut state, _root, _roots) = inventory_with_profile("scan-consumes", "Default");
        let grant = state
            .grant_profile_consent("owner-a", BrowserImportId::Chrome, "Default", 50_000)
            .unwrap();
        assert!(state
            .scan_consented_profile(
                "owner-a",
                &BrowserProfileRoots::default(),
                BrowserImportId::Chrome,
                "Default",
                &grant.grant_id,
                50_001,
            )
            .is_err());
        assert!(
            state.grants.is_empty(),
            "a scan attempt that presents the exact grant must consume it even when the profile root is unavailable"
        );
        assert!(state
            .consume_profile_consent(
                "owner-a",
                BrowserImportId::Chrome,
                "Default",
                &grant.grant_id,
                50_002,
            )
            .is_err());
    }

    #[test]
    fn browser_profile_scan_state_load_drops_stale_snapshots_and_refuses_plaintext() {
        let root = temp_root("load-cleanup");
        let stale = root.join("snapshot-leftover");
        std::fs::create_dir_all(&stale).unwrap();
        std::fs::write(stale.join("History"), b"https://example.test/").unwrap();
        BrowserProfileScanState::load(&root).unwrap();
        assert!(!stale.exists());

        std::fs::write(root.join(PLAINTEXT_STATE_FILE), b"{}").unwrap();
        assert!(BrowserProfileScanState::load(&root).is_err());
        assert!(root.join(PLAINTEXT_STATE_FILE).exists());
    }

    #[test]
    fn list_profiles_enumerates_candidates_without_reading_history_or_logins() {
        let (root, roots) = roots_with_chrome_profile("list-no-content", "Profile 1");
        std::fs::create_dir(root.join("Profile 1").join("History")).unwrap();
        std::fs::create_dir(root.join("Profile 1").join("Login Data")).unwrap();
        std::fs::create_dir_all(root.join("bad:name")).unwrap();
        let mut state = state("list-no-content");
        let profiles = state.list_profiles(&roots).unwrap();
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].browser_id, BrowserImportId::Chrome);
        assert_eq!(profiles[0].profile, "Profile 1");
        assert_eq!(profiles[0].display_name, "Profile 1");
    }

    #[test]
    fn revoke_all_fires_on_lock_burn_and_identity_switch_transitions() {
        for transition in [
            BrowserProfileTransition::Lock,
            BrowserProfileTransition::Burn,
            BrowserProfileTransition::IdentitySwitch,
        ] {
            let (mut state, _root, _roots) =
                inventory_with_profile("revoke-transitions", "Default");
            state.set_firefox_login_decryption_consent(true);
            state
                .grant_profile_consent("owner-a", BrowserImportId::Chrome, "Default", 1)
                .unwrap();
            state.apply_transition(transition);
            assert!(state.grants.is_empty());
            assert!(state.listed_profiles.is_empty());
            assert!(!state.firefox_login_decryption_consent());
        }

        for transition in [
            BrowserProfileTransition::Unlock,
            BrowserProfileTransition::Startup,
            BrowserProfileTransition::ProfilesListed,
            BrowserProfileTransition::ScanCompleted,
        ] {
            let (mut state, _root, _roots) =
                inventory_with_profile("non-revoke-transitions", "Default");
            state.set_firefox_login_decryption_consent(true);
            state
                .grant_profile_consent("owner-a", BrowserImportId::Chrome, "Default", 1)
                .unwrap();
            state.apply_transition(transition);
            assert_eq!(state.grants.len(), 1);
            assert_eq!(state.listed_profiles.len(), 1);
            assert!(state.firefox_login_decryption_consent());
        }
    }

    #[test]
    fn profile_consent_grants_are_single_use_and_consumed_once() {
        let (mut state, _root, _roots) = inventory_with_profile("consumed-once", "Default");
        let grant = state
            .grant_profile_consent("owner-a", BrowserImportId::Chrome, "Default", 100)
            .unwrap();
        assert!(state
            .consume_profile_consent(
                "owner-b",
                BrowserImportId::Chrome,
                "Default",
                &grant.grant_id,
                101,
            )
            .is_err());
        assert!(state
            .consume_profile_consent(
                "owner-a",
                BrowserImportId::Chrome,
                "Default",
                &grant.grant_id,
                101,
            )
            .is_ok());
        assert!(state
            .consume_profile_consent(
                "owner-a",
                BrowserImportId::Chrome,
                "Default",
                &grant.grant_id,
                101,
            )
            .is_err());

        let expired = state
            .grant_profile_consent("owner-a", BrowserImportId::Chrome, "Default", 200)
            .unwrap();
        assert!(state
            .consume_profile_consent(
                "owner-a",
                BrowserImportId::Chrome,
                "Default",
                &expired.grant_id,
                expired.expires_at_unix_ms,
            )
            .is_err());
        assert!(
            state.grants.is_empty(),
            "an expired consent grant must be removed when a scan attempts to spend it"
        );
    }

    #[test]
    fn snapshot_guard_drops_and_deletes_history_snapshot() {
        let dir = temp_root("snapshot-guard");
        std::fs::create_dir_all(&dir).unwrap();
        let snapshot_dir = dir.join("snapshot-test");
        let snapshot_path = snapshot_dir.join("History");
        std::fs::create_dir_all(&snapshot_dir).unwrap();
        std::fs::write(&snapshot_path, b"https://example.test/").unwrap();
        {
            let guard = SnapshotGuard {
                snapshot_dir: snapshot_dir.clone(),
                snapshot_path,
            };
            assert!(guard.path().exists());
        }
        assert!(!snapshot_dir.exists());
    }

    #[test]
    fn copy_bounded_snapshot_rechecks_raw_wal_bound() {
        let root = temp_root("wal-recheck");
        let profile = root.join("Default");
        let snapshot_root = root.join("snapshots");
        std::fs::create_dir_all(&profile).unwrap();
        let history = profile.join("History");
        std::fs::write(&history, b"https://example.test/").unwrap();
        let wal = wal_path(&history);
        std::fs::write(&wal, vec![0u8; 16]).unwrap();

        let result = copy_bounded_snapshot_with_hook(&history, &snapshot_root, || {
            std::fs::write(&wal, vec![0u8; (MAX_RAW_WAL_BYTES + 1) as usize]).unwrap();
        });
        assert!(result.is_err());
        assert!(!snapshot_root
            .read_dir()
            .map(|mut entries| entries.any(|entry| entry.is_ok()))
            .unwrap_or(false));
    }

    #[test]
    fn scan_consented_profile_requires_exact_unconsumed_grant() {
        let (mut state, root, roots) = inventory_with_profile("exact-grant", "Default");
        std::fs::write(
            root.join("Default").join("History"),
            b"https://example.test/",
        )
        .unwrap();
        let grant = state
            .grant_profile_consent("owner-a", BrowserImportId::Chrome, "Default", 1_000)
            .unwrap();

        assert!(state
            .scan_consented_profile(
                "owner-b",
                &roots,
                BrowserImportId::Chrome,
                "Default",
                &grant.grant_id,
                1_001,
            )
            .is_err());
        assert!(state
            .scan_consented_profile(
                "owner-a",
                &roots,
                BrowserImportId::Firefox,
                "Default",
                &grant.grant_id,
                1_001,
            )
            .is_err());
        assert!(state
            .scan_consented_profile(
                "owner-a",
                &roots,
                BrowserImportId::Chrome,
                "Profile 2",
                &grant.grant_id,
                1_001,
            )
            .is_err());
        assert_eq!(
            state.grants.len(),
            1,
            "wrong owner, browser, or profile must not consume the matching grant"
        );
        let receipt = state
            .scan_consented_profile(
                "owner-a",
                &roots,
                BrowserImportId::Chrome,
                "Default",
                &grant.grant_id,
                1_001,
            )
            .unwrap();
        assert_eq!(receipt.observation_count, 1);
        assert!(
            state.grants.is_empty(),
            "the successful exact scan must consume the matching grant"
        );
        assert!(state
            .scan_consented_profile(
                "owner-a",
                &roots,
                BrowserImportId::Chrome,
                "Default",
                &grant.grant_id,
                1_002,
            )
            .is_err());
    }

    #[test]
    fn scan_resolved_profile_reads_only_bounded_snapshot_rows() {
        let root = temp_root("resolved-scan");
        let profile_dir = root.join("Default");
        let snapshot_root = root.join("snapshots");
        std::fs::create_dir_all(&profile_dir).unwrap();
        let history = profile_dir.join("History");
        let mut rows = Vec::new();
        for index in 0..(MAX_SNAPSHOT_ROWS + 20) {
            rows.push(format!("https://service{index}.example/path"));
        }
        let original = rows.join("\n");
        std::fs::write(&history, original.as_bytes()).unwrap();

        let receipt = scan_resolved_profile_with_commit(
            &snapshot_root,
            BrowserImportId::Chrome,
            "Default",
            &profile_dir,
            |observations| {
                assert_eq!(observations.len(), MAX_SNAPSHOT_ROWS);
                assert_eq!(observations[0].site, "service0.example");
                assert!(!snapshot_root
                    .read_dir()
                    .map(|mut entries| entries.any(|entry| entry.is_ok()))
                    .unwrap_or(false));
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(receipt.observation_count, MAX_SNAPSHOT_ROWS);
        assert!(receipt.snapshot_deleted);
        assert_eq!(std::fs::read(&history).unwrap(), original.as_bytes());
    }

    #[test]
    fn firefox_login_decryption_consent_is_separate_default_off_switch() {
        let (mut state, _root, _roots) = inventory_with_profile("firefox-login-consent", "Default");
        assert!(!state.firefox_login_decryption_consent());
        state
            .grant_profile_consent("owner-a", BrowserImportId::Chrome, "Default", 1)
            .unwrap();
        assert!(!state.firefox_login_decryption_consent());
        state.set_firefox_login_decryption_consent(true);
        assert!(state.firefox_login_decryption_consent());
        assert_eq!(state.grants.len(), 1);
        state.revoke_all();
        assert!(!state.firefox_login_decryption_consent());
        assert!(state.grants.is_empty());
    }
}
