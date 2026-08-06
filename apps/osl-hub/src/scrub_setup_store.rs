use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

const SCRUB_SETUP_VERSION: u8 = 1;
const MAX_SCRUB_SETUP_BYTES: u64 = 8 * 1024;
const MAX_SCAN_ACCOUNTS: usize = 32;
const MAX_ACCOUNT_ID_BYTES: usize = 128;

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScrubSetupSchedule {
    Daily,
    Weekly,
    Monthly,
}

impl ScrubSetupSchedule {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScrubSetupNoticeSetting {
    BeforeScan,
    AfterScan,
    Quiet,
}

impl ScrubSetupNoticeSetting {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BeforeScan => "before_scan",
            Self::AfterScan => "after_scan",
            Self::Quiet => "quiet",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScrubSetupCommand {
    #[serde(default)]
    pub selected_scan_accounts: Vec<String>,
    pub automatic_schedule: Option<ScrubSetupSchedule>,
    pub notice_setting: Option<ScrubSetupNoticeSetting>,
    #[serde(default)]
    pub not_now: bool,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScrubSetupSummary {
    pub account_count: usize,
    pub automatic_schedule: Option<ScrubSetupSchedule>,
    pub notice_setting: Option<ScrubSetupNoticeSetting>,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ScrubSetupDocument {
    version: u8,
    selected_scan_accounts: Vec<String>,
    automatic_schedule: Option<ScrubSetupSchedule>,
    notice_setting: Option<ScrubSetupNoticeSetting>,
}

impl Default for ScrubSetupDocument {
    fn default() -> Self {
        Self {
            version: SCRUB_SETUP_VERSION,
            selected_scan_accounts: Vec::new(),
            automatic_schedule: None,
            notice_setting: None,
        }
    }
}

pub struct ScrubSetupState {
    path: PathBuf,
    document: Mutex<ScrubSetupDocument>,
}

impl ScrubSetupState {
    pub fn load(path: PathBuf) -> Self {
        Self {
            document: Mutex::new(read_document(&path).unwrap_or_default()),
            path,
        }
    }

    pub fn save_command(&self, command: ScrubSetupCommand) -> Result<ScrubSetupSummary, String> {
        let document = document_from_command(command)?;
        write_document(&self.path, &document)?;
        let summary = summary(&document);
        let mut current = self
            .document
            .lock()
            .map_err(|_| "Scrub setup storage is unavailable".to_owned())?;
        *current = document;
        Ok(summary)
    }

    pub fn summary(&self) -> Result<ScrubSetupSummary, String> {
        self.document
            .lock()
            .map(|document| summary(&document))
            .map_err(|_| "Scrub setup storage is unavailable".to_owned())
    }
}

fn document_from_command(command: ScrubSetupCommand) -> Result<ScrubSetupDocument, String> {
    if command.not_now {
        return Ok(ScrubSetupDocument::default());
    }

    let accounts = normalize_accounts(command.selected_scan_accounts)?;
    let automatic_schedule = command
        .automatic_schedule
        .ok_or_else(|| "Choose an automatic Scrub schedule or Not now".to_owned())?;
    let notice_setting = command
        .notice_setting
        .ok_or_else(|| "Choose a Scrub notice setting or Not now".to_owned())?;

    Ok(ScrubSetupDocument {
        version: SCRUB_SETUP_VERSION,
        selected_scan_accounts: accounts,
        automatic_schedule: Some(automatic_schedule),
        notice_setting: Some(notice_setting),
    })
}

fn normalize_accounts(accounts: Vec<String>) -> Result<Vec<String>, String> {
    if accounts.is_empty() || accounts.len() > MAX_SCAN_ACCOUNTS {
        return Err("Choose 1-32 scan accounts or Not now".to_owned());
    }

    let mut normalized = Vec::with_capacity(accounts.len());
    for account in accounts {
        let account = account.trim();
        if account.is_empty()
            || account.len() > MAX_ACCOUNT_ID_BYTES
            || account.chars().any(|character| character.is_control())
            || normalized.iter().any(|existing| existing == account)
        {
            return Err("Scan accounts must be unique bounded printable identifiers".to_owned());
        }
        normalized.push(account.to_owned());
    }
    Ok(normalized)
}

fn summary(document: &ScrubSetupDocument) -> ScrubSetupSummary {
    ScrubSetupSummary {
        account_count: document.selected_scan_accounts.len(),
        automatic_schedule: document.automatic_schedule,
        notice_setting: document.notice_setting,
    }
}

fn read_document(path: &Path) -> Option<ScrubSetupDocument> {
    let bytes = crate::atomic_file::read_recoverable_bounded(
        path,
        MAX_SCRUB_SETUP_BYTES,
        "Scrub setup preferences",
    )
    .ok()
    .flatten()?;
    let document = serde_json::from_slice::<ScrubSetupDocument>(&bytes).ok()?;
    (document.version == SCRUB_SETUP_VERSION).then_some(document)
}

fn write_document(path: &Path, document: &ScrubSetupDocument) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(document)
        .map_err(|_| "Scrub setup preferences could not be encoded".to_owned())?;
    if bytes.len() as u64 > MAX_SCRUB_SETUP_BYTES {
        return Err("Scrub setup preferences exceed the size limit".to_owned());
    }
    crate::atomic_file::write_recoverable(path, &bytes, "Scrub setup preferences")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_file() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        std::env::temp_dir()
            .join(format!(
                "osl-hub-scrub-setup-{}-{nonce}",
                std::process::id()
            ))
            .join("scrub-setup.json")
    }

    #[test]
    fn save_scrub_setup_command_returns_selected_count_schedule_notice_and_not_now_skip() {
        let path = temporary_file();
        let state = ScrubSetupState::load(path.clone());

        let saved = state
            .save_command(ScrubSetupCommand {
                selected_scan_accounts: vec![
                    "discord:current-session".to_owned(),
                    "telegram:current-session".to_owned(),
                ],
                automatic_schedule: Some(ScrubSetupSchedule::Weekly),
                notice_setting: Some(ScrubSetupNoticeSetting::BeforeScan),
                not_now: false,
            })
            .expect("two selected scan accounts save");

        let saved_schedule = saved.automatic_schedule.expect("schedule saved").as_str();
        let saved_notice = saved.notice_setting.expect("notice saved").as_str();
        assert_eq!(saved.account_count, 2);
        assert_eq!(saved_schedule, "weekly");
        assert_eq!(saved_notice, "before_scan");
        assert_eq!(
            ScrubSetupState::load(path.clone()).summary().unwrap(),
            saved
        );
        println!(
            "scrub setup saved account_count={} schedule={} notice_setting={}",
            saved.account_count, saved_schedule, saved_notice
        );

        let skipped = state
            .save_command(ScrubSetupCommand {
                selected_scan_accounts: vec![
                    "discord:current-session".to_owned(),
                    "telegram:current-session".to_owned(),
                ],
                automatic_schedule: Some(ScrubSetupSchedule::Weekly),
                notice_setting: Some(ScrubSetupNoticeSetting::BeforeScan),
                not_now: true,
            })
            .expect("not now clears Scrub setup");

        assert_eq!(skipped.account_count, 0);
        assert_eq!(skipped.automatic_schedule, None);
        assert_eq!(skipped.notice_setting, None);
        assert_eq!(
            ScrubSetupState::load(path.clone()).summary().unwrap(),
            skipped
        );
        println!(
            "scrub setup not_now account_count={} schedule={} notice_setting={}",
            skipped.account_count,
            skipped
                .automatic_schedule
                .map(ScrubSetupSchedule::as_str)
                .unwrap_or("none"),
            skipped
                .notice_setting
                .map(ScrubSetupNoticeSetting::as_str)
                .unwrap_or("none")
        );

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
