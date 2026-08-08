//! TASK 3308 — direct run of the repeating timed-delete sweep job.
//!
//! Two due records and one not-due record go in; exactly two messages must come
//! out deleted, one must be left, and a search of the job's own code must find
//! no place that deletes a message directly.
//!
//! Exit 0 when every finish-line item holds. Exit 1 otherwise, naming what went
//! wrong — including, by locator, a not-due record that was deleted anyway.

use std::path::PathBuf;
use std::process::ExitCode;

use task_3308_timed_delete_sweep_job::{
    run_task_3308_fixture, scan_job_source_for_direct_deletion, scanned_code_line_count,
    DIRECT_DELETION_PATTERNS, DUE_DISCORD_LOCATOR, DUE_WHATSAPP_LOCATOR, FIXTURE_NOW,
    FIXTURE_SECOND_WAKE, FIXTURE_WAKE_EVERY_SECONDS, JOB_SOURCE, JOB_SOURCE_PATH, NOT_DUE_LOCATOR,
};

fn scratch_directory() -> PathBuf {
    std::env::temp_dir().join(format!("task-3308-sweep-{}", std::process::id()))
}

fn main() -> ExitCode {
    let directory = scratch_directory();
    let mut failures: Vec<String> = Vec::new();

    // --- the direct run -----------------------------------------------------
    let outcome = match run_task_3308_fixture(&directory) {
        Ok(outcome) => outcome,
        Err(error) => {
            println!("TASK3308_RUN result=ERR error={error:?}");
            let _ = std::fs::remove_dir_all(&directory);
            return ExitCode::FAILURE;
        }
    };

    println!(
        "TASK3308_JOB command=timed_delete_sweep_job wake_every_seconds={FIXTURE_WAKE_EVERY_SECONDS} first_wake={FIXTURE_NOW} second_wake={FIXTURE_SECOND_WAKE}"
    );
    println!(
        "TASK3308_BEFORE planted_records={:?} planted_messages={:?} due={:?} not_due={:?}",
        outcome.planted_records,
        outcome.planted_messages,
        [DUE_DISCORD_LOCATOR, DUE_WHATSAPP_LOCATOR],
        [NOT_DUE_LOCATOR],
    );
    println!(
        "TASK3308_WAKES wake_count={} waited_until={:?}",
        outcome.wake_count, outcome.waited_until
    );
    println!(
        "TASK3308_SWEPT swept={:?} retained_first_wake={:?}",
        outcome.swept, outcome.retained_first_wake
    );
    println!(
        "TASK3308_CLEANERS discord_clean_calls={:?} whatsapp_clean_calls={:?}",
        outcome.discord_clean_calls, outcome.whatsapp_clean_calls
    );
    println!(
        "TASK3308_AFTER deleted_count={} deleted={:?} left_count={} left={:?} records_left={:?}",
        outcome.deleted_count(),
        outcome.deleted_messages(),
        outcome.left_count(),
        outcome.messages_left(),
        outcome.records_left,
    );

    if outcome.deleted_count() != 2 {
        failures.push(format!(
            "expected exactly 2 deleted, got {} ({:?})",
            outcome.deleted_count(),
            outcome.deleted_messages()
        ));
    }
    if outcome.left_count() != 1 {
        failures.push(format!(
            "expected exactly 1 left, got {} ({:?})",
            outcome.left_count(),
            outcome.messages_left()
        ));
    }
    for due in [DUE_DISCORD_LOCATOR, DUE_WHATSAPP_LOCATOR] {
        if !outcome.deleted_messages().iter().any(|it| it == due) {
            failures.push(format!("due record {due} was not deleted"));
        }
    }
    if !outcome
        .messages_left()
        .iter()
        .any(|it| it == NOT_DUE_LOCATOR)
    {
        failures.push(format!(
            "not-due record {NOT_DUE_LOCATOR} was deleted: its delete time has not passed"
        ));
    }
    if outcome.records_left != vec![NOT_DUE_LOCATOR.to_owned()] {
        failures.push(format!(
            "expected only {NOT_DUE_LOCATOR} left in the store, got {:?}",
            outcome.records_left
        ));
    }
    if outcome
        .discord_clean_calls
        .iter()
        .any(|it| it == NOT_DUE_LOCATOR)
    {
        failures.push(format!(
            "not-due record {NOT_DUE_LOCATOR} was handed to a shared cleaner"
        ));
    }

    // --- the search of the job's own code -----------------------------------
    let hits = scan_job_source_for_direct_deletion(JOB_SOURCE);
    println!(
        "TASK3308_SCAN file={JOB_SOURCE_PATH} code_lines_searched={} patterns={} direct_delete_places={}",
        scanned_code_line_count(JOB_SOURCE),
        DIRECT_DELETION_PATTERNS.len(),
        hits.len()
    );
    for hit in &hits {
        println!(
            "TASK3308_SCAN_HIT line={} pattern={:?} code={:?}",
            hit.line_number, hit.pattern, hit.line
        );
    }
    if !hits.is_empty() {
        failures.push(format!(
            "the job's own code has {} place(s) that delete a message directly",
            hits.len()
        ));
    }

    let _ = std::fs::remove_dir_all(&directory);

    if failures.is_empty() {
        println!("TASK3308_RESULT result=PASS deleted=2 left=1 direct_delete_places=0");
        ExitCode::SUCCESS
    } else {
        for failure in &failures {
            println!("TASK3308_FAILURE {failure}");
        }
        println!(
            "TASK3308_RESULT result=FAIL failures={} deleted={} left={} direct_delete_places={}",
            failures.len(),
            outcome.deleted_count(),
            outcome.left_count(),
            hits.len()
        );
        ExitCode::FAILURE
    }
}
