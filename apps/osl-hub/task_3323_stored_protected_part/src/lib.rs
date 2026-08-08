//! TASK 3323 — the stored protected part goes when the message goes.
//!
//! A protected message is two halves: the cover words the app shows, and the
//! protected part the OSL service holds. Taking away the cover words and
//! leaving the protected part stored is half a delete, and the pointer in the
//! cover words would still fetch it.
//!
//! Nothing in here reimplements the job. `timed_delete_sweep_job` below is the
//! hub's own `apps/osl-hub/src/timed_delete_sweep_job.rs`, compiled through
//! `#[path]`, so the job under test is byte-for-byte the file the hub ships.
//!
//! What this crate adds is the other side of each seam, for real:
//!
//! * a record store that really holds records in a file,
//! * a per-app cleaner that really holds that app's messages,
//! * an OSL service that really holds each protected part as a file on disk,
//!   counts what it holds, and refuses a fetch of one it no longer has.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use serde::{Deserialize, Serialize};

#[path = "../../src/timed_delete_sweep_job.rs"]
pub mod timed_delete_sweep_job;

use timed_delete_sweep_job::{
    DueTimedDeleteRecord, ProtectedPartFetchRefusal, ProtectedPartRemoval, ProtectedPartStore,
    SharedAppCleaner, SweepClock, SweepWaitOutcome, TimedDeleteWorkSource, PROTECTED_PART_GONE,
};

/// The job's path, for the report.
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

/// One app's shared cleaner: it really holds that app's cover words and really
/// removes the one it is handed.
pub struct AppMessageBox {
    app_id: String,
    state: Rc<RefCell<AppMessageBoxState>>,
    refuse: bool,
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
                refuse: false,
            },
            state,
        )
    }

    /// The same cleaner, but one that refuses every delete the app is asked for.
    pub fn that_refuses(mut self) -> Self {
        self.refuse = true;
        self
    }
}

impl SharedAppCleaner for AppMessageBox {
    fn app_id(&self) -> &str {
        &self.app_id
    }

    fn clean_due_message(&mut self, record: &DueTimedDeleteRecord) -> Result<(), String> {
        let mut state = self.state.borrow_mut();
        state.clean_calls.push(record.message_locator.clone());
        if self.refuse {
            return Err(format!("{} refused the delete", self.app_id));
        }
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
// The OSL service: the protected parts it really holds, as files on disk
// ---------------------------------------------------------------------------

/// An OSL service that holds the protected part of a message as a real file.
///
/// The count this reports is a count of files actually on disk, read fresh
/// every time, so "1 to 0" is a claim about the filesystem and not about a
/// number the job kept in its head.
pub struct ServiceProtectedParts {
    directory: PathBuf,
    /// Every message the service was asked to let go of, in order.
    calls: Rc<RefCell<Vec<String>>>,
}

impl ServiceProtectedParts {
    /// An empty service store rooted at `directory`.
    pub fn open(directory: &Path) -> Result<(Self, Rc<RefCell<Vec<String>>>), String> {
        std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
        let calls = Rc::new(RefCell::new(Vec::new()));
        Ok((
            Self {
                directory: directory.to_path_buf(),
                calls: Rc::clone(&calls),
            },
            calls,
        ))
    }

    /// The name the service knows one message's protected part by.
    pub fn part_name(record: &DueTimedDeleteRecord) -> String {
        format!("osl-protected-part:{}", record.identity())
    }

    fn part_path(&self, record: &DueTimedDeleteRecord) -> PathBuf {
        let file_name: String = Self::part_name(record)
            .chars()
            .map(|character| match character {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '.' => character,
                _ => '_',
            })
            .collect();
        self.directory.join(format!("{file_name}.part"))
    }

    /// Put the protected part of `record` on the service.
    pub fn put(&self, record: &DueTimedDeleteRecord, bytes: &[u8]) -> Result<String, String> {
        std::fs::write(self.part_path(record), bytes).map_err(|error| error.to_string())?;
        Ok(Self::part_name(record))
    }

    /// Every protected item the service holds right now, by file name.
    pub fn stored_item_names(&self) -> Result<Vec<String>, String> {
        let mut names = Vec::new();
        for entry in std::fs::read_dir(&self.directory).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
        names.sort();
        Ok(names)
    }

    /// How many protected items the service holds in total.
    pub fn stored_item_count(&self) -> Result<usize, String> {
        Ok(self.stored_item_names()?.len())
    }
}

impl ProtectedPartStore for ServiceProtectedParts {
    fn stored_part_count(&self, record: &DueTimedDeleteRecord) -> Result<usize, String> {
        Ok(usize::from(self.part_path(record).exists()))
    }

    fn remove_protected_part(
        &mut self,
        record: &DueTimedDeleteRecord,
    ) -> Result<ProtectedPartRemoval, String> {
        self.calls.borrow_mut().push(record.identity());
        let path = self.part_path(record);
        let stored_before = usize::from(path.exists());
        if stored_before > 0 {
            std::fs::remove_file(&path).map_err(|error| {
                format!(
                    "OSL service could not let go of {}: {error}",
                    Self::part_name(record)
                )
            })?;
        }
        Ok(ProtectedPartRemoval {
            part_name: Self::part_name(record),
            stored_before,
            stored_after: usize::from(path.exists()),
        })
    }

    fn fetch_protected_part(
        &self,
        record: &DueTimedDeleteRecord,
    ) -> Result<Vec<u8>, ProtectedPartFetchRefusal> {
        std::fs::read(self.part_path(record)).map_err(|_| ProtectedPartFetchRefusal {
            code: PROTECTED_PART_GONE,
            part_name: Self::part_name(record),
            reason: format!(
                "OSL service no longer holds the protected part {}",
                Self::part_name(record)
            ),
        })
    }
}

/// A service store that refuses to let go of anything, for the check that a
/// half delete is refused rather than quietly recorded as done.
pub struct ImmovableProtectedParts {
    /// How many items it claims to hold for every message.
    pub held: usize,
}

impl ProtectedPartStore for ImmovableProtectedParts {
    fn stored_part_count(&self, _record: &DueTimedDeleteRecord) -> Result<usize, String> {
        Ok(self.held)
    }

    fn remove_protected_part(
        &mut self,
        record: &DueTimedDeleteRecord,
    ) -> Result<ProtectedPartRemoval, String> {
        Err(format!(
            "OSL service would not let go of {}",
            ServiceProtectedParts::part_name(record)
        ))
    }

    fn fetch_protected_part(
        &self,
        _record: &DueTimedDeleteRecord,
    ) -> Result<Vec<u8>, ProtectedPartFetchRefusal> {
        Ok(b"still here".to_vec())
    }
}

/// A service store that says it let go while still holding the item — the
/// half-delete a report alone could not catch.
pub struct LyingProtectedParts;

impl ProtectedPartStore for LyingProtectedParts {
    fn stored_part_count(&self, _record: &DueTimedDeleteRecord) -> Result<usize, String> {
        Ok(1)
    }

    fn remove_protected_part(
        &mut self,
        record: &DueTimedDeleteRecord,
    ) -> Result<ProtectedPartRemoval, String> {
        Ok(ProtectedPartRemoval {
            part_name: ServiceProtectedParts::part_name(record),
            stored_before: 1,
            stored_after: 1,
        })
    }

    fn fetch_protected_part(
        &self,
        _record: &DueTimedDeleteRecord,
    ) -> Result<Vec<u8>, ProtectedPartFetchRefusal> {
        Ok(b"still here".to_vec())
    }
}

// ---------------------------------------------------------------------------
// A scripted clock, so a direct run is not a wait
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
// The TASK 3323 fixture: one due protected message, one not-due protected
// message that must be left completely alone
// ---------------------------------------------------------------------------

/// The second the direct run judges records against.
pub const FIXTURE_NOW: i64 = 1_900_003_600;
/// How often the fixture job wakes.
pub const FIXTURE_WAKE_EVERY_SECONDS: u32 = 60;

/// The app the fixture message lives in.
pub const FIXTURE_APP: &str = "discord";
/// The conversation the fixture message lives in.
pub const FIXTURE_CONVERSATION: &str = "dm:task-3323";
/// The due protected message: cover words in the app, protected part on the
/// service.
pub const DUE_PROTECTED_LOCATOR: &str = "discord-message-3323-due-protected";
/// A protected message whose second has not arrived, as a control: its stored
/// part must still be there afterwards.
pub const NOT_DUE_PROTECTED_LOCATOR: &str = "discord-message-3323-not-due-protected";

/// The private words held on the service for the due message.
pub const DUE_PROTECTED_BYTES: &[u8] = b"the private words of task 3323";
/// The private words held on the service for the control message.
pub const NOT_DUE_PROTECTED_BYTES: &[u8] = b"the control message's private words";

/// The due record, in the shape the sweep job routes.
pub fn due_record() -> DueTimedDeleteRecord {
    DueTimedDeleteRecord {
        app_id: FIXTURE_APP.to_owned(),
        conversation_id: FIXTURE_CONVERSATION.to_owned(),
        message_locator: DUE_PROTECTED_LOCATOR.to_owned(),
        sent_at_unix_seconds: 1_900_000_000,
        delete_at_unix_seconds: FIXTURE_NOW,
        protection: "protected".to_owned(),
    }
}

/// The control record, whose second has not arrived.
pub fn not_due_record() -> DueTimedDeleteRecord {
    DueTimedDeleteRecord {
        app_id: FIXTURE_APP.to_owned(),
        conversation_id: FIXTURE_CONVERSATION.to_owned(),
        message_locator: NOT_DUE_PROTECTED_LOCATOR.to_owned(),
        sent_at_unix_seconds: 1_900_000_000,
        delete_at_unix_seconds: 1_900_090_000,
        protection: "protected".to_owned(),
    }
}

fn persisted(record: &DueTimedDeleteRecord) -> StoredTimedDeleteRecord {
    StoredTimedDeleteRecord {
        app_id: record.app_id.clone(),
        conversation_id: record.conversation_id.clone(),
        message_locator: record.message_locator.clone(),
        sent_at_unix_seconds: record.sent_at_unix_seconds,
        delete_at_unix_seconds: record.delete_at_unix_seconds,
        protection: record.protection.clone(),
    }
}

/// How a fetch of one protected part came back.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FetchOutcome {
    /// The service handed the protected part over, this many bytes of it.
    Read(usize),
    /// The service refused, by this name.
    Refused {
        code: String,
        part_name: String,
        reason: String,
    },
}

impl FetchOutcome {
    fn of(result: Result<Vec<u8>, ProtectedPartFetchRefusal>) -> Self {
        match result {
            Ok(bytes) => Self::Read(bytes.len()),
            Err(refusal) => Self::Refused {
                code: refusal.code.to_owned(),
                part_name: refusal.part_name,
                reason: refusal.reason,
            },
        }
    }

    /// Whether the service handed the protected part over.
    pub fn was_read(&self) -> bool {
        matches!(self, Self::Read(_))
    }

    /// The name the fetch was refused by, if it was refused.
    pub fn refusal_code(&self) -> Option<&str> {
        match self {
            Self::Refused { code, .. } => Some(code),
            Self::Read(_) => None,
        }
    }

    /// The protected part the refusal named, if it was refused.
    pub fn refused_part_name(&self) -> Option<&str> {
        match self {
            Self::Refused { part_name, .. } => Some(part_name),
            Self::Read(_) => None,
        }
    }
}

/// Everything the direct run observed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Task3323Outcome {
    /// The name the service knows the due message's protected part by.
    pub part_name: String,
    /// Protected items on the service for the due message, before and after.
    pub stored_parts_before: usize,
    pub stored_parts_after: usize,
    /// Every protected item file the service held, before and after.
    pub stored_item_names_before: Vec<String>,
    pub stored_item_names_after: Vec<String>,
    /// A fetch of the due message's protected part, before and after.
    pub fetch_before: FetchOutcome,
    pub fetch_after: FetchOutcome,
    /// The cover words of the due message in the app, before and after.
    pub visible_messages_before: Vec<String>,
    pub visible_messages_after: Vec<String>,
    /// How many wakes the run took.
    pub wake_count: usize,
    /// Record identities the job swept.
    pub swept: Vec<String>,
    /// The protected parts the job had the service let go of: name, before,
    /// after.
    pub protected_parts_removed: Vec<(String, usize, usize)>,
    /// Which messages the service was asked to let go of, in order.
    pub service_calls: Vec<String>,
    /// The control message's stored part, before and after, and its fetch after.
    pub control_stored_parts_before: usize,
    pub control_stored_parts_after: usize,
    pub control_fetch_after: FetchOutcome,
    /// Records left in the store afterwards.
    pub records_left: Vec<String>,
}

impl Task3323Outcome {
    /// Whether the due message's cover words are still in the app.
    pub fn visible_message_still_there(&self) -> bool {
        self.visible_messages_after
            .iter()
            .any(|locator| locator == DUE_PROTECTED_LOCATOR)
    }
}

/// Plant one due protected message and one not-due protected message, run one
/// wake of the job, and report what the OSL service holds afterwards.
pub fn run_task_3323_fixture(directory: &Path) -> Result<Task3323Outcome, String> {
    use timed_delete_sweep_job::{SharedAppCleaners, TimedDeleteSweepJob};

    std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let store_path = directory.join("timed_delete_records.json");
    let service_directory = directory.join("osl-service-protected-parts");

    let due = due_record();
    let not_due = not_due_record();

    let mut store = FileRecordStore::plant(&store_path, &[persisted(&due), persisted(&not_due)])?;

    let (cleaner, app_state) = AppMessageBox::plant(
        FIXTURE_APP,
        &[DUE_PROTECTED_LOCATOR, NOT_DUE_PROTECTED_LOCATOR],
    );
    let mut cleaners = SharedAppCleaners::new();
    cleaners.register(Box::new(cleaner))?;

    let (mut parts, service_calls) = ServiceProtectedParts::open(&service_directory)?;
    let part_name = parts.put(&due, DUE_PROTECTED_BYTES)?;
    parts.put(&not_due, NOT_DUE_PROTECTED_BYTES)?;

    let stored_parts_before = parts.stored_part_count(&due)?;
    let control_stored_parts_before = parts.stored_part_count(&not_due)?;
    let stored_item_names_before = parts.stored_item_names()?;
    let fetch_before = FetchOutcome::of(parts.fetch_protected_part(&due));
    let visible_messages_before = app_state.borrow().messages.clone();

    let job = TimedDeleteSweepJob::new(FIXTURE_WAKE_EVERY_SECONDS)?;
    let mut clock = ScriptedClock::new(&[FIXTURE_NOW]);
    let run = job.run_repeating(&mut clock, &mut store, &mut cleaners, &mut parts)?;

    let stored_parts_after = parts.stored_part_count(&due)?;
    let control_stored_parts_after = parts.stored_part_count(&not_due)?;
    let stored_item_names_after = parts.stored_item_names()?;
    let fetch_after = FetchOutcome::of(parts.fetch_protected_part(&due));
    let control_fetch_after = FetchOutcome::of(parts.fetch_protected_part(&not_due));
    let visible_messages_after = app_state.borrow().messages.clone();
    let service_calls = service_calls.borrow().clone();

    Ok(Task3323Outcome {
        part_name,
        stored_parts_before,
        stored_parts_after,
        stored_item_names_before,
        stored_item_names_after,
        fetch_before,
        fetch_after,
        visible_messages_before,
        visible_messages_after,
        wake_count: run.wake_count(),
        swept: run.swept(),
        protected_parts_removed: run
            .protected_parts_removed()
            .into_iter()
            .map(|removal| {
                (
                    removal.part_name,
                    removal.stored_before,
                    removal.stored_after,
                )
            })
            .collect(),
        service_calls,
        control_stored_parts_before,
        control_stored_parts_after,
        control_fetch_after,
        records_left: FileRecordStore::locators(&store_path)?,
    })
}
