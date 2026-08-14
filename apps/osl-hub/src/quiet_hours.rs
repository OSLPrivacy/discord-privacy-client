//! Quiet hours storage: a start time, an end time, and an on or off choice.
//!
//! The window may cross midnight (start 22:30, end 06:45), so a start after
//! the end is a valid overnight window. The one shape that is refused is a
//! start equal to the end: that window is either zero-length or always-on
//! depending on who reads it, so it is refused by name instead of being
//! quietly reinterpreted.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

const QUIET_HOURS_VERSION: u8 = 1;
const MAX_QUIET_HOURS_BYTES: u64 = 4 * 1024;

#[derive(Debug, Clone, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuietHoursCommand {
    pub start: String,
    pub end: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuietHoursSettings {
    pub start: String,
    pub end: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct QuietHoursDocument {
    version: u8,
    start: String,
    end: String,
    enabled: bool,
}

impl Default for QuietHoursDocument {
    fn default() -> Self {
        Self {
            version: QUIET_HOURS_VERSION,
            start: "22:00".to_owned(),
            end: "08:00".to_owned(),
            enabled: false,
        }
    }
}

pub struct QuietHoursState {
    path: PathBuf,
    document: Mutex<QuietHoursDocument>,
}

impl QuietHoursState {
    pub fn load(path: PathBuf) -> Self {
        Self {
            document: Mutex::new(read_document(&path).unwrap_or_default()),
            path,
        }
    }

    pub fn save_command(&self, command: QuietHoursCommand) -> Result<QuietHoursSettings, String> {
        let document = document_from_command(command)?;
        write_document(&self.path, &document)?;
        let saved = settings(&document);
        let mut current = self
            .document
            .lock()
            .map_err(|_| "Quiet hours storage is unavailable".to_owned())?;
        *current = document;
        Ok(saved)
    }

    pub fn settings(&self) -> Result<QuietHoursSettings, String> {
        self.document
            .lock()
            .map(|document| settings(&document))
            .map_err(|_| "Quiet hours storage is unavailable".to_owned())
    }
}

fn document_from_command(command: QuietHoursCommand) -> Result<QuietHoursDocument, String> {
    let start = normalize_time(&command.start, "start")?;
    let end = normalize_time(&command.end, "end")?;
    if start == end {
        return Err(format!(
            "Quiet hours were refused: the start time {start} equals the end time {end}; \
             a quiet hours window needs two different times"
        ));
    }
    Ok(QuietHoursDocument {
        version: QUIET_HOURS_VERSION,
        start,
        end,
        enabled: command.enabled,
    })
}

fn normalize_time(raw: &str, which: &str) -> Result<String, String> {
    let (hours, minutes) = parse_time(raw, which)?;
    Ok(format!("{hours:02}:{minutes:02}"))
}

/// The minute of the day (0..=1439) named by a 24-hour HH:MM time, refused
/// with the same wording as a refused save.
pub fn time_minutes(raw: &str, which: &str) -> Result<u16, String> {
    let (hours, minutes) = parse_time(raw, which)?;
    Ok(u16::from(hours) * 60 + u16::from(minutes))
}

fn parse_time(raw: &str, which: &str) -> Result<(u8, u8), String> {
    let refusal = || {
        format!(
            "Quiet hours were refused: the {which} time {raw:?} is not a \
             24-hour HH:MM time"
        )
    };
    let (hours, minutes) = raw.split_once(':').ok_or_else(refusal)?;
    if hours.len() != 2 || minutes.len() != 2 {
        return Err(refusal());
    }
    let hours: u8 = hours.parse().map_err(|_| refusal())?;
    let minutes: u8 = minutes.parse().map_err(|_| refusal())?;
    if hours > 23 || minutes > 59 {
        return Err(refusal());
    }
    Ok((hours, minutes))
}

fn settings(document: &QuietHoursDocument) -> QuietHoursSettings {
    QuietHoursSettings {
        start: document.start.clone(),
        end: document.end.clone(),
        enabled: document.enabled,
    }
}

fn read_document(path: &Path) -> Option<QuietHoursDocument> {
    let bytes =
        crate::atomic_file::read_recoverable_bounded(path, MAX_QUIET_HOURS_BYTES, "Quiet hours")
            .ok()
            .flatten()?;
    let document = serde_json::from_slice::<QuietHoursDocument>(&bytes).ok()?;
    (document.version == QUIET_HOURS_VERSION).then_some(document)
}

fn write_document(path: &Path, document: &QuietHoursDocument) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(document)
        .map_err(|_| "Quiet hours could not be encoded".to_owned())?;
    if bytes.len() as u64 > MAX_QUIET_HOURS_BYTES {
        return Err("Quiet hours exceed the size limit".to_owned());
    }
    crate::atomic_file::write_recoverable(path, &bytes, "Quiet hours")
}
