//! TASK 3308 — the direct run of the repeating timed-delete sweep job.
//!
//! Nothing in here reimplements the job. `timed_delete_sweep_job` below is the
//! hub's own `apps/osl-hub/src/timed_delete_sweep_job.rs`, compiled through
//! `#[path]`, so the job under test is byte-for-byte the file `lib.rs` declares.
//!
//! What this crate adds is only the two things TASK 3309 will later supply for
//! real: a record store that really holds records on disk, and a shared cleaner
//! per app that really holds messages and really removes them.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use serde::{Deserialize, Serialize};

#[path = "../../src/timed_delete_sweep_job.rs"]
pub mod timed_delete_sweep_job;

use timed_delete_sweep_job::{
    DueTimedDeleteRecord, SharedAppCleaner, SweepClock, SweepWaitOutcome, TimedDeleteWorkSource,
};

/// The job's own source, embedded at compile time so the scan below can never
/// drift from the file that was actually built.
pub const JOB_SOURCE: &str = include_str!("../../src/timed_delete_sweep_job.rs");

/// The job's path, for the scan report.
pub const JOB_SOURCE_PATH: &str = "apps/osl-hub/src/timed_delete_sweep_job.rs";

// ---------------------------------------------------------------------------
// The record store, in the persisted shape TASK 3306/3307 writes
// ---------------------------------------------------------------------------

/// One timed-delete record exactly as TASK 3306/3307 persists it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoredTimedDeleteRecord {
    pub app_id: String,
    pub conversation_id: String,
    pub message_locator: String,
    pub sent_at_unix_seconds: i64,
    pub delete_at_unix_seconds: i64,
    pub protection: String,
}

impl From<&StoredTimedDeleteRecord> for DueTimedDeleteRecord {
    fn from(stored: &StoredTimedDeleteRecord) -> Self {
        Self {
            app_id: stored.app_id.clone(),
            conversation_id: stored.conversation_id.clone(),
            message_locator: stored.message_locator.clone(),
            sent_at_unix_seconds: stored.sent_at_unix_seconds,
            delete_at_unix_seconds: stored.delete_at_unix_seconds,
            protection: stored.protection.clone(),
        }
    }
}

#[derive(Default, Deserialize, Serialize)]
struct StoreFile {
    version: u32,
    records: Vec<StoredTimedDeleteRecord>,
}

/// A timed-delete record store backed by a real file on disk.
///
/// This is the seam TASK 3309 replaces with the sealed ledger. The retirement
/// of a swept record happens here, on the store's side of the seam — the job
/// only says which record the shared cleaner accepted.
pub struct FileRecordStore {
    path: PathBuf,
}

impl FileRecordStore {
    /// Write the starting records and return a store over them.
    pub fn plant(path: &Path, records: &[StoredTimedDeleteRecord]) -> Result<Self, String> {
        let file = StoreFile {
            version: 1,
            records: records.to_vec(),
        };
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&file).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        Ok(Self {
            path: path.to_path_buf(),
        })
    }

    /// Fresh-read every record currently held.
    pub fn read(path: &Path) -> Result<Vec<StoredTimedDeleteRecord>, String> {
        let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
        let file: StoreFile = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        Ok(file.records)
    }

    /// Fresh-read the locators currently held, in store order.
    pub fn locators(path: &Path) -> Result<Vec<String>, String> {
        Ok(Self::read(path)?
            .into_iter()
            .map(|record| record.message_locator)
            .collect())
    }
}

impl TimedDeleteWorkSource for FileRecordStore {
    fn all_records(&self) -> Result<Vec<DueTimedDeleteRecord>, String> {
        Ok(Self::read(&self.path)?
            .iter()
            .map(DueTimedDeleteRecord::from)
            .collect())
    }

    fn retire_swept_record(&mut self, record: &DueTimedDeleteRecord) -> Result<(), String> {
        let kept: Vec<StoredTimedDeleteRecord> = Self::read(&self.path)?
            .into_iter()
            .filter(|held| {
                !(held.app_id == record.app_id
                    && held.conversation_id == record.conversation_id
                    && held.message_locator == record.message_locator)
            })
            .collect();
        let file = StoreFile {
            version: 1,
            records: kept,
        };
        std::fs::write(
            &self.path,
            serde_json::to_vec_pretty(&file).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())
    }
}

// ---------------------------------------------------------------------------
// The shared cleaner for one app
// ---------------------------------------------------------------------------

/// What one app's shared cleaner holds and does.
#[derive(Debug, Default)]
pub struct AppMessageBoxState {
    /// The messages planted in this app, by locator.
    pub messages: Vec<String>,
    /// Every locator the job handed to this cleaner, in order.
    pub clean_calls: Vec<String>,
}

/// One app's shared cleaner: it really holds that app's messages and really
/// removes the one it is handed.
///
/// The job never calls anything but `clean_due_message`; every removal below
/// happens on this side of the seam.
pub struct AppMessageBox {
    app_id: String,
    state: Rc<RefCell<AppMessageBoxState>>,
}

impl AppMessageBox {
    /// A cleaner for `app_id` holding `messages`, plus a handle to look at it.
    pub fn plant(app_id: &str, messages: &[&str]) -> (Self, Rc<RefCell<AppMessageBoxState>>) {
        let state = Rc::new(RefCell::new(AppMessageBoxState {
            messages: messages
                .iter()
                .map(|locator| (*locator).to_owned())
                .collect(),
            clean_calls: Vec::new(),
        }));
        (
            Self {
                app_id: app_id.to_owned(),
                state: Rc::clone(&state),
            },
            state,
        )
    }
}

impl SharedAppCleaner for AppMessageBox {
    fn app_id(&self) -> &str {
        &self.app_id
    }

    fn clean_due_message(&mut self, record: &DueTimedDeleteRecord) -> Result<(), String> {
        let mut state = self.state.borrow_mut();
        state.clean_calls.push(record.message_locator.clone());
        let before = state.messages.len();
        state
            .messages
            .retain(|locator| locator != &record.message_locator);
        if state.messages.len() == before {
            return Err(format!(
                "{} has no message {}",
                self.app_id, record.message_locator
            ));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// A scripted clock, so a direct run is not a twenty-minute wait
// ---------------------------------------------------------------------------

/// A clock that hands the job a scripted list of wake seconds and then stops it.
pub struct ScriptedClock {
    wakes: VecDeque<i64>,
    now: i64,
    /// Every second the job was asked to sleep until.
    pub waited_until: Vec<i64>,
}

impl ScriptedClock {
    /// A clock that wakes the job once per entry in `wakes`.
    pub fn new(wakes: &[i64]) -> Self {
        let mut wakes: VecDeque<i64> = wakes.iter().copied().collect();
        let now = wakes.pop_front().unwrap_or(0);
        Self {
            wakes,
            now,
            waited_until: Vec::new(),
        }
    }
}

impl SweepClock for ScriptedClock {
    fn now_unix_seconds(&self) -> i64 {
        self.now
    }

    fn wait_until(&mut self, wake_at_unix_seconds: i64) -> SweepWaitOutcome {
        self.waited_until.push(wake_at_unix_seconds);
        match self.wakes.pop_front() {
            Some(next) => {
                self.now = next;
                SweepWaitOutcome::Wake
            }
            None => SweepWaitOutcome::Stop,
        }
    }
}

// ---------------------------------------------------------------------------
// The search of this job's own code for direct deletion
// ---------------------------------------------------------------------------

/// One place in the job's code that deletes a message directly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectDeletionHit {
    pub line_number: usize,
    pub pattern: &'static str,
    pub line: String,
}

/// Every way a Rust file can remove a message, a cache row, a record or a file
/// without going through a seam.
///
/// A hit on any of these in the job's own code means the job grew deleting code
/// of its own, which is exactly what TASK 3308 forbids.
pub const DIRECT_DELETION_PATTERNS: &[&str] = &[
    // filesystem removal and rewriting
    "fs::remove_file",
    "fs::remove_dir",
    "remove_file(",
    "remove_dir_all(",
    "fs::write",
    "File::create",
    "OpenOptions",
    "set_len(",
    // in-memory removal of a held collection of messages or records
    ".remove(",
    ".retain(",
    ".clear(",
    ".drain(",
    ".truncate(",
    ".pop(",
    ".swap_remove(",
    ".remove_entry(",
    // shelling out to something that deletes
    "Command::new",
    // a per-app or per-store delete action called by name instead of by seam
    "delete_message",
    "remove_message",
    "erase_message",
    "destroy_message",
    "shred",
    "unlink",
    "purge",
    "wipe",
    // the record store's own removal paths
    "store_timed_delete_ledger",
    "expire_timed_delete_records",
    "fire_due_timed_deletes",
];

fn is_comment(line: &str) -> bool {
    line.trim_start().starts_with("//")
}

/// Search the job's own code for a place that deletes a message directly.
///
/// Comment and doc-comment lines are skipped: prose that says the job must not
/// delete is not a place that deletes.
pub fn scan_job_source_for_direct_deletion(source: &str) -> Vec<DirectDeletionHit> {
    let mut hits = Vec::new();
    for (index, line) in source.lines().enumerate() {
        if is_comment(line) {
            continue;
        }
        for pattern in DIRECT_DELETION_PATTERNS {
            if line.contains(pattern) {
                hits.push(DirectDeletionHit {
                    line_number: index + 1,
                    pattern,
                    line: line.trim().to_owned(),
                });
            }
        }
    }
    hits
}

/// How many code lines of the job the scan looked at.
pub fn scanned_code_line_count(source: &str) -> usize {
    source
        .lines()
        .filter(|line| !is_comment(line) && !line.trim().is_empty())
        .count()
}

// ---------------------------------------------------------------------------
// The TASK 3308 fixture: two due records and one not-due record
// ---------------------------------------------------------------------------

/// The second the direct run judges records against.
pub const FIXTURE_NOW: i64 = 1_900_003_600;
/// The second of the job's second wake, one interval later.
pub const FIXTURE_SECOND_WAKE: i64 = FIXTURE_NOW + FIXTURE_WAKE_EVERY_SECONDS as i64;
/// How often the fixture job wakes.
pub const FIXTURE_WAKE_EVERY_SECONDS: u32 = 60;

/// The first due record: a protected Discord message whose second has arrived.
pub const DUE_DISCORD_LOCATOR: &str = "discord-message-3308-due-a";
/// The second due record: an ordinary WhatsApp message, overdue.
pub const DUE_WHATSAPP_LOCATOR: &str = "whatsapp-message-3308-due-b";
/// The one record whose time has not passed.
pub const NOT_DUE_LOCATOR: &str = "discord-message-3308-not-due";

/// The three records the direct run plants.
pub fn fixture_records() -> Vec<StoredTimedDeleteRecord> {
    vec![
        StoredTimedDeleteRecord {
            app_id: "discord".to_owned(),
            conversation_id: "dm:task-3308-a".to_owned(),
            message_locator: DUE_DISCORD_LOCATOR.to_owned(),
            sent_at_unix_seconds: 1_900_000_000,
            delete_at_unix_seconds: FIXTURE_NOW,
            protection: "protected".to_owned(),
        },
        StoredTimedDeleteRecord {
            app_id: "discord".to_owned(),
            conversation_id: "dm:task-3308-a".to_owned(),
            message_locator: NOT_DUE_LOCATOR.to_owned(),
            sent_at_unix_seconds: 1_900_000_000,
            delete_at_unix_seconds: 1_900_090_000,
            protection: "ordinary".to_owned(),
        },
        StoredTimedDeleteRecord {
            app_id: "whatsapp".to_owned(),
            conversation_id: "chat:task-3308-b".to_owned(),
            message_locator: DUE_WHATSAPP_LOCATOR.to_owned(),
            sent_at_unix_seconds: 1_900_001_000,
            delete_at_unix_seconds: 1_900_003_000,
            protection: "ordinary".to_owned(),
        },
    ]
}

/// Everything the direct run observed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixtureOutcome {
    /// Message locators planted across both apps before the run.
    pub planted_messages: Vec<String>,
    /// Record locators in the store before the run.
    pub planted_records: Vec<String>,
    /// How many times the job woke.
    pub wake_count: usize,
    /// The seconds the job slept until between wakes.
    pub waited_until: Vec<i64>,
    /// Record identities the job swept, across the whole run.
    pub swept: Vec<String>,
    /// Record identities the job left alone because they are not yet due.
    pub retained_first_wake: Vec<String>,
    /// Locators each app's shared cleaner was handed, in order.
    pub discord_clean_calls: Vec<String>,
    pub whatsapp_clean_calls: Vec<String>,
    /// Messages still present in each app after the run.
    pub discord_messages_left: Vec<String>,
    pub whatsapp_messages_left: Vec<String>,
    /// Records still in the store after the run.
    pub records_left: Vec<String>,
}

impl FixtureOutcome {
    /// Messages deleted across both apps.
    pub fn deleted_messages(&self) -> Vec<String> {
        let left: Vec<&String> = self
            .discord_messages_left
            .iter()
            .chain(self.whatsapp_messages_left.iter())
            .collect();
        self.planted_messages
            .iter()
            .filter(|locator| !left.contains(locator))
            .cloned()
            .collect()
    }

    /// How many messages the run deleted.
    pub fn deleted_count(&self) -> usize {
        self.deleted_messages().len()
    }

    /// Messages still present across both apps.
    pub fn messages_left(&self) -> Vec<String> {
        self.discord_messages_left
            .iter()
            .chain(self.whatsapp_messages_left.iter())
            .cloned()
            .collect()
    }

    /// How many messages the run left in place.
    pub fn left_count(&self) -> usize {
        self.messages_left().len()
    }
}

/// Plant two due records and one not-due record, run the repeating job for two
/// wakes, and report what happened.
pub fn run_task_3308_fixture(directory: &Path) -> Result<FixtureOutcome, String> {
    use timed_delete_sweep_job::{SharedAppCleaners, TimedDeleteSweepJob};

    std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let store_path = directory.join("timed_delete_records.json");

    let records = fixture_records();
    let mut store = FileRecordStore::plant(&store_path, &records)?;
    let planted_records: Vec<String> = records
        .iter()
        .map(|record| record.message_locator.clone())
        .collect();

    let (discord, discord_state) =
        AppMessageBox::plant("discord", &[DUE_DISCORD_LOCATOR, NOT_DUE_LOCATOR]);
    let (whatsapp, whatsapp_state) = AppMessageBox::plant("whatsapp", &[DUE_WHATSAPP_LOCATOR]);
    let planted_messages: Vec<String> = discord_state
        .borrow()
        .messages
        .iter()
        .chain(whatsapp_state.borrow().messages.iter())
        .cloned()
        .collect();

    let mut cleaners = SharedAppCleaners::new();
    cleaners.register(Box::new(discord))?;
    cleaners.register(Box::new(whatsapp))?;

    let job = TimedDeleteSweepJob::new(FIXTURE_WAKE_EVERY_SECONDS)?;
    let mut clock = ScriptedClock::new(&[FIXTURE_NOW, FIXTURE_SECOND_WAKE]);
    let run = job.run_repeating(&mut clock, &mut store, &mut cleaners)?;

    let discord_clean_calls = discord_state.borrow().clean_calls.clone();
    let discord_messages_left = discord_state.borrow().messages.clone();
    let whatsapp_clean_calls = whatsapp_state.borrow().clean_calls.clone();
    let whatsapp_messages_left = whatsapp_state.borrow().messages.clone();
    let records_left = FileRecordStore::locators(&store_path)?;

    Ok(FixtureOutcome {
        planted_messages,
        planted_records,
        wake_count: run.wake_count(),
        waited_until: clock.waited_until.clone(),
        swept: run.swept(),
        retained_first_wake: run
            .passes
            .first()
            .map(|pass| pass.retained.clone())
            .unwrap_or_default(),
        discord_clean_calls,
        whatsapp_clean_calls,
        discord_messages_left,
        whatsapp_messages_left,
        records_left,
    })
}
