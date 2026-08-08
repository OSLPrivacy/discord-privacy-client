//! TASK 3323 — the stored protected part goes at the same time as the message.
//!
//! The finish line is checked twice: once by driving the job directly, and once
//! by running the built command and asserting its exit code and its output.

use std::path::PathBuf;
use std::process::Command;

use task_3323_stored_protected_part::timed_delete_sweep_job::{
    ProtectedPartStore, SharedAppCleaners, TimedDeleteSweepJob, PROTECTED_PART_GONE,
    PROTECTED_PART_NOT_REMOVED, PROTECTED_PART_STILL_STORED,
};
use task_3323_stored_protected_part::{
    due_record, run_task_3323_fixture, AppMessageBox, FetchOutcome, FileRecordStore,
    ImmovableProtectedParts, LyingProtectedParts, ScriptedClock, ServiceProtectedParts,
    StoredTimedDeleteRecord, DUE_PROTECTED_BYTES, DUE_PROTECTED_LOCATOR, FIXTURE_APP, FIXTURE_NOW,
    NOT_DUE_PROTECTED_LOCATOR,
};

fn scratch(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("task-3323-{name}-{}", std::process::id()))
}

fn command_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_BIN_EXE_task-3323-stored-protected-part"));
    path.set_extension(std::env::consts::EXE_EXTENSION);
    path
}

#[test]
fn task_3323_one_timed_delete_takes_the_stored_protected_item_from_1_to_0() {
    let directory = scratch("one-to-zero");
    let outcome = run_task_3323_fixture(&directory).expect("fixture run");
    let _ = std::fs::remove_dir_all(&directory);

    assert_eq!(outcome.wake_count, 1, "one timed delete, one wake");
    assert_eq!(
        outcome.stored_parts_before, 1,
        "the service held exactly one protected item for the message"
    );
    assert_eq!(
        outcome.fetch_before,
        FetchOutcome::Read(DUE_PROTECTED_BYTES.len()),
        "and it really read back before the delete"
    );
    assert_eq!(
        outcome.stored_parts_after, 0,
        "the stored protected item count went 1 to 0, got {} (service still holds {:?})",
        outcome.stored_parts_after, outcome.stored_item_names_after
    );

    // Not a number the job kept in its head: the item is off the disk.
    assert_eq!(outcome.stored_item_names_before.len(), 2);
    assert_eq!(
        outcome.stored_item_names_after.len(),
        1,
        "only the not-due message's item is left, got {:?}",
        outcome.stored_item_names_after
    );

    // The other half of the delete happened too.
    assert!(
        !outcome.visible_message_still_there(),
        "the cover words are gone from the app as well"
    );
    assert_eq!(outcome.swept.len(), 1, "one record swept");
    assert_eq!(
        outcome.service_calls,
        vec![format!(
            "{FIXTURE_APP}/dm:task-3323/{DUE_PROTECTED_LOCATOR}"
        )],
        "the service was asked about the due message and nothing else"
    );
    assert_eq!(
        outcome.protected_parts_removed,
        vec![(outcome.part_name.clone(), 1, 0)],
        "the job reported the one removal, 1 before and 0 after"
    );
}

#[test]
fn task_3323_a_fetch_of_the_protected_part_afterwards_is_refused_by_name() {
    let directory = scratch("fetch-refused");
    let outcome = run_task_3323_fixture(&directory).expect("fixture run");
    let _ = std::fs::remove_dir_all(&directory);

    assert_eq!(
        outcome.fetch_after.refusal_code(),
        Some(PROTECTED_PART_GONE),
        "the fetch afterwards is refused by name, got {:?}",
        outcome.fetch_after
    );
    assert_eq!(
        outcome.fetch_after.refused_part_name(),
        Some(outcome.part_name.as_str()),
        "and the refusal names the protected item"
    );
    match &outcome.fetch_after {
        FetchOutcome::Refused { reason, .. } => assert!(
            reason.contains(&outcome.part_name),
            "the refusal reason names the item: {reason}"
        ),
        FetchOutcome::Read(bytes) => panic!("the protected part still read back, {bytes} bytes"),
    }

    // The control message's protected part is untouched, so the removal was of
    // one item and not of everything the service held.
    assert_eq!(outcome.control_stored_parts_before, 1);
    assert_eq!(outcome.control_stored_parts_after, 1);
    assert!(
        outcome.control_fetch_after.was_read(),
        "the not-due message's protected part still fetches"
    );
    assert_eq!(
        outcome.records_left,
        vec![NOT_DUE_PROTECTED_LOCATOR.to_owned()]
    );
}

#[test]
fn task_3323_the_direct_command_passes_and_says_so() {
    let output = Command::new(command_path())
        .output()
        .expect("run the direct command");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "direct command exited {:?}\n{stdout}",
        output.status.code()
    );
    assert!(
        stdout.contains(
            "TASK3323_RESULT result=PASS stored_protected_items=1->0 fetch_after=refused:osl_protected_part_gone visible_message=deleted"
        ),
        "{stdout}"
    );
    assert!(
        stdout.contains("TASK3323_BEFORE stored_protected_items=1"),
        "{stdout}"
    );
    assert!(
        stdout.contains("TASK3323_AFTER stored_protected_items=0"),
        "{stdout}"
    );
    assert!(
        stdout.contains("TASK3323_FETCH_AFTER REFUSED code=osl_protected_part_gone"),
        "{stdout}"
    );
}

#[test]
fn task_3323_a_service_that_will_not_let_go_stops_the_delete_instead_of_half_doing_it() {
    let directory = scratch("immovable");
    std::fs::create_dir_all(&directory).expect("scratch");
    let store_path = directory.join("timed_delete_records.json");
    let due = due_record();
    let mut store = FileRecordStore::plant(
        &store_path,
        &[StoredTimedDeleteRecord {
            app_id: due.app_id.clone(),
            conversation_id: due.conversation_id.clone(),
            message_locator: due.message_locator.clone(),
            sent_at_unix_seconds: due.sent_at_unix_seconds,
            delete_at_unix_seconds: due.delete_at_unix_seconds,
            protection: due.protection.clone(),
        }],
    )
    .expect("plant");

    let (cleaner, app_state) = AppMessageBox::plant(FIXTURE_APP, &[DUE_PROTECTED_LOCATOR]);
    let mut cleaners = SharedAppCleaners::new();
    cleaners.register(Box::new(cleaner)).expect("register");

    let job = TimedDeleteSweepJob::new(60).expect("job");
    let mut parts = ImmovableProtectedParts { held: 1 };
    let pass = job
        .wake(FIXTURE_NOW, &mut store, &mut cleaners, &mut parts)
        .expect("wake");

    assert_eq!(pass.swept_count(), 0, "nothing was swept");
    assert_eq!(pass.refused.len(), 1);
    assert_eq!(pass.refused[0].code, PROTECTED_PART_NOT_REMOVED);
    assert_eq!(
        app_state.borrow().messages,
        vec![DUE_PROTECTED_LOCATOR.to_owned()],
        "the cover words were left where they are, not deleted into half a delete"
    );
    assert!(
        app_state.borrow().clean_calls.is_empty(),
        "the app was never even asked"
    );
    assert_eq!(
        FileRecordStore::locators(&store_path).expect("locators"),
        vec![DUE_PROTECTED_LOCATOR.to_owned()],
        "the record stays, to be tried again"
    );
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn task_3323_a_service_that_says_it_let_go_while_still_holding_is_refused() {
    let directory = scratch("lying");
    std::fs::create_dir_all(&directory).expect("scratch");
    let store_path = directory.join("timed_delete_records.json");
    let due = due_record();
    let mut store = FileRecordStore::plant(
        &store_path,
        &[StoredTimedDeleteRecord {
            app_id: due.app_id.clone(),
            conversation_id: due.conversation_id.clone(),
            message_locator: due.message_locator.clone(),
            sent_at_unix_seconds: due.sent_at_unix_seconds,
            delete_at_unix_seconds: due.delete_at_unix_seconds,
            protection: due.protection.clone(),
        }],
    )
    .expect("plant");

    let (cleaner, app_state) = AppMessageBox::plant(FIXTURE_APP, &[DUE_PROTECTED_LOCATOR]);
    let mut cleaners = SharedAppCleaners::new();
    cleaners.register(Box::new(cleaner)).expect("register");

    let job = TimedDeleteSweepJob::new(60).expect("job");
    let mut parts = LyingProtectedParts;
    let pass = job
        .wake(FIXTURE_NOW, &mut store, &mut cleaners, &mut parts)
        .expect("wake");

    assert_eq!(pass.swept_count(), 0);
    assert_eq!(pass.refused.len(), 1);
    assert_eq!(pass.refused[0].code, PROTECTED_PART_STILL_STORED);
    assert!(
        pass.refused[0].reason.contains("still holds 1 protected"),
        "{}",
        pass.refused[0].reason
    );
    assert_eq!(
        app_state.borrow().messages,
        vec![DUE_PROTECTED_LOCATOR.to_owned()],
        "a service that only claims to have let go does not get the visible half deleted"
    );
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn task_3323_an_ordinary_message_has_no_stored_part_to_take_away() {
    let directory = scratch("ordinary");
    std::fs::create_dir_all(&directory).expect("scratch");
    let store_path = directory.join("timed_delete_records.json");
    let mut store = FileRecordStore::plant(
        &store_path,
        &[StoredTimedDeleteRecord {
            app_id: FIXTURE_APP.to_owned(),
            conversation_id: "dm:task-3323".to_owned(),
            message_locator: "discord-message-3323-ordinary".to_owned(),
            sent_at_unix_seconds: 1_900_000_000,
            delete_at_unix_seconds: FIXTURE_NOW,
            protection: "ordinary".to_owned(),
        }],
    )
    .expect("plant");

    let (cleaner, app_state) =
        AppMessageBox::plant(FIXTURE_APP, &["discord-message-3323-ordinary"]);
    let mut cleaners = SharedAppCleaners::new();
    cleaners.register(Box::new(cleaner)).expect("register");

    let (mut parts, calls) =
        ServiceProtectedParts::open(&directory.join("service")).expect("service");
    let job = TimedDeleteSweepJob::new(60).expect("job");
    let mut clock = ScriptedClock::new(&[FIXTURE_NOW]);
    let run = job
        .run_repeating(&mut clock, &mut store, &mut cleaners, &mut parts)
        .expect("run");

    assert_eq!(run.swept_count(), 1, "the ordinary message still deletes");
    assert_eq!(
        run.protected_part_count(),
        0,
        "and nothing was asked of the service"
    );
    assert!(calls.borrow().is_empty(), "the service was not called");
    assert!(app_state.borrow().messages.is_empty());
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn task_3323_the_service_store_really_holds_and_really_lets_go() {
    // The seam under all of the above is not a stub that always answers 0.
    let directory = scratch("service-store");
    let (mut parts, _calls) = ServiceProtectedParts::open(&directory).expect("service");
    let due = due_record();

    assert_eq!(parts.stored_part_count(&due).expect("count"), 0);
    assert!(parts.fetch_protected_part(&due).is_err(), "nothing yet");

    parts.put(&due, DUE_PROTECTED_BYTES).expect("put");
    assert_eq!(parts.stored_part_count(&due).expect("count"), 1);
    assert_eq!(
        parts.fetch_protected_part(&due).expect("fetch"),
        DUE_PROTECTED_BYTES.to_vec(),
        "the very bytes that were stored"
    );

    let removal = parts.remove_protected_part(&due).expect("remove");
    assert_eq!(removal.stored_before, 1);
    assert_eq!(removal.stored_after, 0);
    assert_eq!(parts.stored_item_count().expect("count"), 0);

    // Asking twice is not an error, so a retried sweep cannot get stuck.
    let again = parts.remove_protected_part(&due).expect("remove again");
    assert_eq!(again.stored_before, 0);
    assert_eq!(again.stored_after, 0);

    let refusal = parts
        .fetch_protected_part(&due)
        .expect_err("the fetch is refused");
    assert_eq!(refusal.code, PROTECTED_PART_GONE);
    assert_eq!(refusal.part_name, ServiceProtectedParts::part_name(&due));
    let _ = std::fs::remove_dir_all(&directory);
}
