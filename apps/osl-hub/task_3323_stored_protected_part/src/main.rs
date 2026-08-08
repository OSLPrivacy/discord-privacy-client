//! TASK 3323 — direct run: the stored protected part goes with the message.
//!
//! One due protected message is planted with its cover words in the app and its
//! protected part on the OSL service. After one timed delete the service must
//! hold 0 protected items for that message, and a fetch of it must be refused
//! by name.
//!
//! Exit 0 when every finish-line item holds. Exit 1 otherwise, naming — by the
//! service's own name for it — any protected item that is still stored.

use std::path::PathBuf;
use std::process::ExitCode;

use task_3323_stored_protected_part::timed_delete_sweep_job::PROTECTED_PART_GONE;
use task_3323_stored_protected_part::{
    run_task_3323_fixture, FetchOutcome, DUE_PROTECTED_BYTES, DUE_PROTECTED_LOCATOR, FIXTURE_APP,
    FIXTURE_CONVERSATION, FIXTURE_NOW, JOB_SOURCE_PATH, NOT_DUE_PROTECTED_LOCATOR,
};

fn scratch_directory() -> PathBuf {
    std::env::temp_dir().join(format!("task-3323-protected-part-{}", std::process::id()))
}

fn describe(fetch: &FetchOutcome) -> String {
    match fetch {
        FetchOutcome::Read(bytes) => format!("READ bytes={bytes}"),
        FetchOutcome::Refused {
            code,
            part_name,
            reason,
        } => format!("REFUSED code={code} part_name={part_name:?} reason={reason:?}"),
    }
}

fn main() -> ExitCode {
    let directory = scratch_directory();
    let mut failures: Vec<String> = Vec::new();

    let outcome = match run_task_3323_fixture(&directory) {
        Ok(outcome) => outcome,
        Err(error) => {
            println!("TASK3323_RUN result=ERR error={error:?}");
            let _ = std::fs::remove_dir_all(&directory);
            return ExitCode::FAILURE;
        }
    };

    println!(
        "TASK3323_JOB source={JOB_SOURCE_PATH} app={FIXTURE_APP} conversation={FIXTURE_CONVERSATION} now={FIXTURE_NOW}"
    );
    println!(
        "TASK3323_PLANT due_message={DUE_PROTECTED_LOCATOR} not_due_message={NOT_DUE_PROTECTED_LOCATOR} protected_bytes={}",
        DUE_PROTECTED_BYTES.len()
    );
    println!(
        "TASK3323_BEFORE stored_protected_items={} part_name={:?} fetch={} visible_messages={:?} service_items={:?}",
        outcome.stored_parts_before,
        outcome.part_name,
        describe(&outcome.fetch_before),
        outcome.visible_messages_before,
        outcome.stored_item_names_before,
    );
    println!(
        "TASK3323_SWEEP wake_count={} swept={:?} protected_parts_removed={:?} service_asked_for={:?}",
        outcome.wake_count,
        outcome.swept,
        outcome.protected_parts_removed,
        outcome.service_calls,
    );
    println!(
        "TASK3323_AFTER stored_protected_items={} visible_messages={:?} service_items={:?} records_left={:?}",
        outcome.stored_parts_after,
        outcome.visible_messages_after,
        outcome.stored_item_names_after,
        outcome.records_left,
    );
    println!("TASK3323_FETCH_AFTER {}", describe(&outcome.fetch_after));
    println!(
        "TASK3323_CONTROL not_due_stored_protected_items_before={} after={} fetch={}",
        outcome.control_stored_parts_before,
        outcome.control_stored_parts_after,
        describe(&outcome.control_fetch_after),
    );

    // --- the finish line ----------------------------------------------------

    // 1. the stored protected item count for that message goes from 1 to 0
    if outcome.stored_parts_before != 1 {
        failures.push(format!(
            "the service should have held exactly 1 protected item for {DUE_PROTECTED_LOCATOR} before the delete, it held {}",
            outcome.stored_parts_before
        ));
    }
    if !outcome.fetch_before.was_read() {
        failures.push(format!(
            "the protected item {} could not be fetched before the delete, so 1 to 0 would prove nothing",
            outcome.part_name
        ));
    }
    if outcome.stored_parts_after != 0 {
        failures.push(format!(
            "protected item {} is still stored on the OSL service after the timed delete: the count went {} to {}, not 1 to 0",
            outcome.part_name, outcome.stored_parts_before, outcome.stored_parts_after
        ));
    }

    // 2. a fetch of it afterwards is refused by name
    match outcome.fetch_after.refusal_code() {
        Some(code) if code == PROTECTED_PART_GONE => {}
        Some(code) => failures.push(format!(
            "the fetch afterwards was refused by the wrong name: {code}, expected {PROTECTED_PART_GONE}"
        )),
        None => failures.push(format!(
            "the fetch afterwards was not refused: protected item {} still reads back",
            outcome.part_name
        )),
    }
    if outcome.fetch_after.refused_part_name() != Some(outcome.part_name.as_str()) {
        failures.push(format!(
            "the refusal did not name the protected item {}, it named {:?}",
            outcome.part_name,
            outcome.fetch_after.refused_part_name()
        ));
    }

    // The rest of the delete, so a run that only removed the stored half — or
    // only the visible half — cannot pass.
    if outcome.visible_message_still_there() {
        failures.push(format!(
            "the visible message {DUE_PROTECTED_LOCATOR} is still in {FIXTURE_APP}: the whole message was not deleted"
        ));
    }
    if outcome.protected_parts_removed.len() != 1 {
        failures.push(format!(
            "the job reported {} protected part removals, expected exactly 1",
            outcome.protected_parts_removed.len()
        ));
    }
    if outcome.control_stored_parts_after != 1 || !outcome.control_fetch_after.was_read() {
        failures.push(format!(
            "the not-due message's protected item should have been left alone: count {} and fetch {}",
            outcome.control_stored_parts_after,
            describe(&outcome.control_fetch_after)
        ));
    }
    if outcome.records_left != vec![NOT_DUE_PROTECTED_LOCATOR.to_owned()] {
        failures.push(format!(
            "only {NOT_DUE_PROTECTED_LOCATOR} should be left in the record store, got {:?}",
            outcome.records_left
        ));
    }

    let _ = std::fs::remove_dir_all(&directory);

    if failures.is_empty() {
        println!(
            "TASK3323_RESULT result=PASS stored_protected_items=1->0 fetch_after=refused:{PROTECTED_PART_GONE} visible_message=deleted"
        );
        ExitCode::SUCCESS
    } else {
        for failure in &failures {
            println!("TASK3323_FAILURE {failure}");
        }
        println!(
            "TASK3323_RESULT result=FAIL failures={} stored_protected_items={}->{} fetch_after={}",
            failures.len(),
            outcome.stored_parts_before,
            outcome.stored_parts_after,
            describe(&outcome.fetch_after),
        );
        ExitCode::FAILURE
    }
}
