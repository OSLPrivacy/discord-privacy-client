//! TASK 3324 — a carrier refusal cannot leave a protected email readable.
//!
//! The timed-delete job is the shipping code from TASK 3323.  This oracle
//! gives it a due protected email and a not-due neighbour, while the carrier
//! is deliberately configured to refuse deletion if anybody calls it.  Email
//! has no such call: its cover is a keep item and only the service object is
//! in the destroy set.

use std::collections::BTreeSet;
use std::path::Path;

use task_3323_stored_protected_part::timed_delete_sweep_job::{
    DueTimedDeleteRecord, ProtectedPartStore, SharedAppCleaners, TimedDeleteSweepJob,
    PROTECTED_PART_GONE,
};
use task_3323_stored_protected_part::{
    FetchOutcome, FileRecordStore, ScriptedClock, ServiceProtectedParts, StoredTimedDeleteRecord,
};

#[path = "../../src/email_timer_contract.rs"]
pub mod email_timer_contract;

pub const NOW: i64 = 1_900_324_000;
pub const EMAIL_APP: &str = "email";
pub const CONVERSATION: &str = "mailbox:protected-email-3324";
pub const DUE_LOCATOR: &str = "osl-object-email-3324-due";
pub const NEIGHBOR_LOCATOR: &str = "osl-object-email-3324-neighbor";
pub const CARRIER_COVER: &str = "mail-cover-email-3324-due";
pub const NEIGHBOR_COVER: &str = "mail-cover-email-3324-neighbor";

fn due_record() -> DueTimedDeleteRecord {
    DueTimedDeleteRecord {
        app_id: EMAIL_APP.to_owned(),
        conversation_id: CONVERSATION.to_owned(),
        message_locator: DUE_LOCATOR.to_owned(),
        sent_at_unix_seconds: NOW - 3_600,
        delete_at_unix_seconds: NOW,
        protection: "protected".to_owned(),
    }
}

fn neighbor_record() -> DueTimedDeleteRecord {
    DueTimedDeleteRecord {
        app_id: EMAIL_APP.to_owned(),
        conversation_id: CONVERSATION.to_owned(),
        message_locator: NEIGHBOR_LOCATOR.to_owned(),
        sent_at_unix_seconds: NOW - 3_600,
        delete_at_unix_seconds: NOW + 3_600,
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

fn observed_fetch(
    result: Result<
        Vec<u8>,
        task_3323_stored_protected_part::timed_delete_sweep_job::ProtectedPartFetchRefusal,
    >,
) -> FetchOutcome {
    match result {
        Ok(bytes) => FetchOutcome::Read(bytes.len()),
        Err(refusal) => FetchOutcome::Refused {
            code: refusal.code.to_owned(),
            part_name: refusal.part_name,
            reason: refusal.reason,
        },
    }
}

/// A separate carrier read boundary.  It has no access to the OSL service;
/// a requested delete would be forcibly refused and counted.
#[derive(Clone, Debug)]
pub struct ForcedFailureCarrier {
    messages: BTreeSet<String>,
    pub delete_attempts: usize,
    pub forced_failure: bool,
}

impl ForcedFailureCarrier {
    fn planted() -> Self {
        Self {
            messages: [CARRIER_COVER.to_owned(), NEIGHBOR_COVER.to_owned()]
                .into_iter()
                .collect(),
            delete_attempts: 0,
            forced_failure: true,
        }
    }

    /// An independent provider read, intentionally separate from the service
    /// store and timed-delete record source.
    pub fn read(&self, identity: &str) -> bool {
        self.messages.contains(identity)
    }

    /// Present only to prove that forced carrier failure is irrelevant: email
    /// pointer expiry must never call it.
    pub fn delete(&mut self, identity: &str) -> Result<(), String> {
        self.delete_attempts += 1;
        if self.forced_failure {
            return Err(format!("forced shipping-carrier failure for {identity}"));
        }
        self.messages.remove(identity);
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Task3324Outcome {
    pub destroy_set: Vec<String>,
    pub keep_set: Vec<String>,
    pub due_part_before: usize,
    pub due_part_after: usize,
    pub neighbor_part_before: usize,
    pub neighbor_part_after: usize,
    pub due_pointer_after: FetchOutcome,
    pub cover_before: bool,
    pub cover_after: bool,
    pub neighbor_cover_after: bool,
    pub carrier_delete_attempts: usize,
    pub carrier_forced_failure: bool,
    pub records_left: Vec<String>,
    pub email_words: String,
    pub ordinary_refusal: String,
    pub uninstalled_timer_paths: Vec<(String, usize)>,
}

/// Drive the real protected-part service and the hub's sweep source.  There is
/// no email cleaner registered: a pointer-only email path has no carrier
/// adapter to promote or call.
pub fn run_shipping_oracle(directory: &Path) -> Result<Task3324Outcome, String> {
    std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let due = due_record();
    let neighbor = neighbor_record();
    let store_path = directory.join("timed-delete-records.json");
    let mut records =
        FileRecordStore::plant(&store_path, &[persisted(&due), persisted(&neighbor)])?;
    let (mut service, _) = ServiceProtectedParts::open(&directory.join("protected-service"))?;
    let due_part_name = service.put(&due, b"private due protected email 3324")?;
    let neighbor_part_name = service.put(&neighbor, b"private unexpired protected email 3324")?;
    let carrier = ForcedFailureCarrier::planted();

    let due_part_before = service.stored_part_count(&due)?;
    let neighbor_part_before = service.stored_part_count(&neighbor)?;
    let cover_before = carrier.read(CARRIER_COVER);

    let job = TimedDeleteSweepJob::new(60)?;
    let mut clock = ScriptedClock::new(&[NOW]);
    let mut cleaners = SharedAppCleaners::new();
    job.run_repeating(&mut clock, &mut records, &mut cleaners, &mut service)?;

    let due_part_after = service.stored_part_count(&due)?;
    let neighbor_part_after = service.stored_part_count(&neighbor)?;
    let due_pointer_after = observed_fetch(service.fetch_protected_part(&due));
    let cover_after = carrier.read(CARRIER_COVER);
    let neighbor_cover_after = carrier.read(NEIGHBOR_COVER);

    let ordinary_refusal =
        email_timer_contract::arm_protected_email_timer("ordinary-email-cover-3324", "", NOW)
            .expect_err("ordinary email must not gain a timer path")
            .to_owned();
    let uninstalled_timer_paths = email_timer_contract::installed_non_email_timer_path_inventory()
        .into_iter()
        .map(|row| (row.app_id.to_owned(), row.installed_timer_paths))
        .collect();

    Ok(Task3324Outcome {
        destroy_set: vec![due_part_name],
        keep_set: vec![CARRIER_COVER.to_owned(), neighbor_part_name],
        due_part_before,
        due_part_after,
        neighbor_part_before,
        neighbor_part_after,
        due_pointer_after,
        cover_before,
        cover_after,
        neighbor_cover_after,
        carrier_delete_attempts: carrier.delete_attempts,
        carrier_forced_failure: carrier.forced_failure,
        records_left: FileRecordStore::locators(&store_path)?,
        email_words: email_timer_contract::EMAIL_TIMER_DISCLOSURE.to_owned(),
        ordinary_refusal,
        uninstalled_timer_paths,
    })
}

/// Check all result fields independently.  The error labels are deliberately
/// stable so a false success identifies under-deletion, over-deletion, or a
/// scope breach instead of merely returning a generic failure.
pub fn verify(outcome: &Task3324Outcome) -> Vec<String> {
    let due = due_record();
    let neighbor = neighbor_record();
    let expected_destroy = vec![ServiceProtectedParts::part_name(&due)];
    let expected_keep = vec![
        CARRIER_COVER.to_owned(),
        ServiceProtectedParts::part_name(&neighbor),
    ];
    let mut failures = Vec::new();

    if outcome.destroy_set != expected_destroy {
        failures.push(format!(
            "under-deletion destroy set is {:?}, expected {:?}",
            outcome.destroy_set, expected_destroy
        ));
    }
    if outcome.keep_set != expected_keep {
        failures.push(format!(
            "over-deletion keep set is {:?}, expected {:?}",
            outcome.keep_set, expected_keep
        ));
    }
    if (outcome.due_part_before, outcome.due_part_after) != (1, 0) {
        failures.push(format!(
            "under-deletion due protected object changed {} to {}, expected 1 to 0",
            outcome.due_part_before, outcome.due_part_after
        ));
    }
    if (outcome.neighbor_part_before, outcome.neighbor_part_after) != (1, 1) {
        failures.push(format!(
            "over-deletion unexpired neighbour changed {} to {}, expected 1 to 1",
            outcome.neighbor_part_before, outcome.neighbor_part_after
        ));
    }
    if outcome.due_pointer_after.refusal_code() != Some(PROTECTED_PART_GONE) {
        failures.push(format!(
            "under-deletion due pointer outcome is {:?}, expected refusal {PROTECTED_PART_GONE}",
            outcome.due_pointer_after
        ));
    }
    if outcome.due_pointer_after.refused_part_name() != Some(expected_destroy[0].as_str()) {
        failures.push(format!(
            "under-deletion pointer refusal names {:?}, expected {}",
            outcome.due_pointer_after.refused_part_name(),
            expected_destroy[0]
        ));
    }
    if !outcome.cover_before || !outcome.cover_after || !outcome.neighbor_cover_after {
        failures.push(
            "over-deletion carrier cover or unexpired neighbour cover did not remain".to_owned(),
        );
    }
    if !outcome.carrier_forced_failure || outcome.carrier_delete_attempts != 0 {
        failures.push(format!(
            "scope breach email carrier delete count is {}, expected 0 under forced failure",
            outcome.carrier_delete_attempts
        ));
    }
    if outcome.records_left != vec![NEIGHBOR_LOCATOR.to_owned()] {
        failures.push(format!(
            "scope breach records left are {:?}, expected only {NEIGHBOR_LOCATOR}",
            outcome.records_left
        ));
    }
    if outcome.email_words != email_timer_contract::EMAIL_TIMER_DISCLOSURE {
        failures.push(format!(
            "scope breach email words are {:?}",
            outcome.email_words
        ));
    }
    if !outcome
        .ordinary_refusal
        .contains("ordinary email is refused")
        || !outcome.ordinary_refusal.contains("records=0")
    {
        failures.push(format!(
            "scope breach ordinary email timer was enabled: {}",
            outcome.ordinary_refusal
        ));
    }
    let expected_paths = vec![
        ("x".to_owned(), 0),
        ("instagram".to_owned(), 0),
        ("messenger".to_owned(), 0),
    ];
    if outcome.uninstalled_timer_paths != expected_paths {
        failures.push(format!(
            "scope breach installed timer paths are {:?}, expected {:?}",
            outcome.uninstalled_timer_paths, expected_paths
        ));
    }
    failures
}

/// Faults used only by the command's break-it mode; they alter the independent
/// observed result after the real shipping run, demonstrating that `verify`
/// cannot report a false green for each named violation.
pub fn inject_oracle_fault(outcome: &mut Task3324Outcome, fault: &str) -> Result<(), String> {
    match fault {
        "leave-object" => outcome.due_part_after = 1,
        "delete-keep" => outcome.neighbor_part_after = 0,
        "enable-ordinary" => outcome.ordinary_refusal = "ordinary timer enabled".to_owned(),
        "promote-adapter" => outcome.uninstalled_timer_paths[1].1 = 1,
        _ => return Err(format!("unknown TASK3324 fault {fault}")),
    }
    Ok(())
}
