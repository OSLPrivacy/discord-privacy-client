//! TASK 3308 — the repeating job that deletes what is due.
//!
//! Each test both drives the job directly and, where the finish line names a
//! direct run, runs the built command and asserts its exit code and its output.

use std::path::PathBuf;
use std::process::Command;

use task_3308_timed_delete_sweep_job::timed_delete_sweep_job::{
    DueTimedDeleteRecord, SharedAppCleaner, SharedAppCleaners, SweepClock, TimedDeleteSweepJob,
    TimedDeleteWorkSource,
};
use task_3308_timed_delete_sweep_job::{
    run_task_3308_fixture, scan_job_source_for_direct_deletion, scanned_code_line_count,
    AppMessageBox, PlantedProtectedParts, DIRECT_DELETION_PATTERNS, DUE_DISCORD_LOCATOR,
    DUE_WHATSAPP_LOCATOR, FIXTURE_NOW, FIXTURE_WAKE_EVERY_SECONDS, JOB_SOURCE, NOT_DUE_LOCATOR,
};

fn scratch(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("task-3308-{name}-{}", std::process::id()))
}

fn command_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_BIN_EXE_task-3308-timed-delete-sweep-job"));
    path.set_extension(std::env::consts::EXE_EXTENSION);
    path
}

#[test]
fn task_3308_direct_run_deletes_exactly_the_two_due_records_and_leaves_the_one_that_is_not_due() {
    let directory = scratch("direct-run");
    let outcome = run_task_3308_fixture(&directory).expect("fixture run");
    let _ = std::fs::remove_dir_all(&directory);

    assert_eq!(outcome.planted_records.len(), 3, "three records planted");
    assert_eq!(outcome.planted_messages.len(), 3, "three messages planted");

    assert_eq!(
        outcome.deleted_count(),
        2,
        "exactly 2 deleted, got {:?}",
        outcome.deleted_messages()
    );
    assert_eq!(
        outcome.left_count(),
        1,
        "exactly 1 left, got {:?}",
        outcome.messages_left()
    );
    assert_eq!(
        outcome.messages_left(),
        vec![NOT_DUE_LOCATOR.to_owned()],
        "the one left is the not-due record"
    );

    let mut deleted = outcome.deleted_messages();
    deleted.sort();
    let mut expected = vec![
        DUE_DISCORD_LOCATOR.to_owned(),
        DUE_WHATSAPP_LOCATOR.to_owned(),
    ];
    expected.sort();
    assert_eq!(deleted, expected, "the two deleted are the two due records");

    assert_eq!(
        outcome.records_left,
        vec![NOT_DUE_LOCATOR.to_owned()],
        "only the not-due record is still in the store"
    );

    // The command agrees, and says so.
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
        stdout.contains("TASK3308_RESULT result=PASS deleted=2 left=1 direct_delete_places=0"),
        "{stdout}"
    );
    assert!(
        stdout.contains("deleted_count=2") && stdout.contains("left_count=1"),
        "{stdout}"
    );
}

#[test]
fn task_3308_a_search_of_the_jobs_code_finds_no_place_that_deletes_a_message_directly() {
    let hits = scan_job_source_for_direct_deletion(JOB_SOURCE);
    assert_eq!(
        hits.len(),
        0,
        "the job's own code deletes directly at {hits:?}"
    );

    // The search is not decoration: the same patterns over a file that really
    // does delete find it.
    let deleting = r#"
fn take_it_out(messages: &mut Vec<String>, locator: &str) {
    messages.retain(|held| held != locator);
    std::fs::remove_file(locator).unwrap();
}
"#;
    assert!(
        scan_job_source_for_direct_deletion(deleting).len() >= 2,
        "the scan must be able to hit"
    );

    // And it really did read the job, not an empty string.
    assert!(
        scanned_code_line_count(JOB_SOURCE) > 100,
        "scanned {} code lines",
        scanned_code_line_count(JOB_SOURCE)
    );
    assert!(DIRECT_DELETION_PATTERNS.len() >= 20);
}

#[test]
fn task_3308_the_job_routes_each_due_record_to_the_shared_cleaner_for_its_own_app() {
    let directory = scratch("routing");
    let outcome = run_task_3308_fixture(&directory).expect("fixture run");
    let _ = std::fs::remove_dir_all(&directory);

    assert_eq!(
        outcome.discord_clean_calls,
        vec![DUE_DISCORD_LOCATOR.to_owned()],
        "discord's cleaner got only discord's due message"
    );
    assert_eq!(
        outcome.whatsapp_clean_calls,
        vec![DUE_WHATSAPP_LOCATOR.to_owned()],
        "whatsapp's cleaner got only whatsapp's due message"
    );
    assert!(
        !outcome
            .discord_clean_calls
            .iter()
            .any(|it| it == NOT_DUE_LOCATOR),
        "the not-due record never reached a cleaner"
    );
}

#[test]
fn task_3308_the_job_repeats_and_a_second_wake_sweeps_nothing_twice() {
    let directory = scratch("repeat");
    let outcome = run_task_3308_fixture(&directory).expect("fixture run");
    let _ = std::fs::remove_dir_all(&directory);

    assert_eq!(outcome.wake_count, 2, "the job woke twice");
    assert_eq!(
        outcome.waited_until,
        vec![
            FIXTURE_NOW + i64::from(FIXTURE_WAKE_EVERY_SECONDS),
            FIXTURE_NOW
                + i64::from(FIXTURE_WAKE_EVERY_SECONDS)
                + i64::from(FIXTURE_WAKE_EVERY_SECONDS),
        ],
        "it slept one interval between wakes"
    );
    assert_eq!(
        outcome.swept.len(),
        2,
        "two records across the whole run, not four"
    );
    assert_eq!(
        outcome.discord_clean_calls.len() + outcome.whatsapp_clean_calls.len(),
        2,
        "the second wake handed nothing to a cleaner again"
    );
}

#[test]
fn task_3308_a_record_one_second_early_is_not_due() {
    let job = TimedDeleteSweepJob::new(60).expect("job");
    let record = DueTimedDeleteRecord {
        app_id: "discord".to_owned(),
        conversation_id: "dm:boundary".to_owned(),
        message_locator: "discord-boundary".to_owned(),
        sent_at_unix_seconds: 1_900_000_000,
        delete_at_unix_seconds: 1_900_003_600,
        protection: "ordinary".to_owned(),
    };
    assert!(!job.is_due(1_900_003_599, &record), "one second early");
    assert!(job.is_due(1_900_003_600, &record), "the named second");
    assert!(job.is_due(1_900_003_601, &record), "one second late");
}

struct HeldRecords {
    records: Vec<DueTimedDeleteRecord>,
    retired: Vec<String>,
}

impl TimedDeleteWorkSource for HeldRecords {
    fn all_records(&self) -> Result<Vec<DueTimedDeleteRecord>, String> {
        Ok(self.records.clone())
    }

    fn retire_swept_record(&mut self, record: &DueTimedDeleteRecord) -> Result<(), String> {
        self.retired.push(record.message_locator.clone());
        Ok(())
    }
}

#[test]
fn task_3308_a_due_record_for_an_app_with_no_shared_cleaner_is_refused_not_deleted() {
    let job = TimedDeleteSweepJob::new(60).expect("job");
    let mut work = HeldRecords {
        records: vec![DueTimedDeleteRecord {
            app_id: "x".to_owned(),
            conversation_id: "dm:no-cleaner".to_owned(),
            message_locator: "x-message-3308".to_owned(),
            sent_at_unix_seconds: 1_900_000_000,
            delete_at_unix_seconds: 1_900_003_000,
            protection: "ordinary".to_owned(),
        }],
        retired: Vec::new(),
    };
    let mut cleaners = SharedAppCleaners::new();
    let (discord, _discord_state) = AppMessageBox::plant("discord", &["discord-something"]);
    cleaners.register(Box::new(discord)).expect("register");

    let pass = job
        .wake(
            FIXTURE_NOW,
            &mut work,
            &mut cleaners,
            &mut PlantedProtectedParts::default(),
        )
        .expect("wake");
    assert_eq!(pass.swept_count(), 0);
    assert_eq!(pass.refused.len(), 1);
    assert_eq!(pass.refused[0].code, "no_cleaner_for_app");
    assert!(work.retired.is_empty(), "nothing was retired");
}

#[test]
fn task_3308_a_second_cleaner_for_the_same_app_is_refused() {
    let mut cleaners = SharedAppCleaners::new();
    let (first, _first_state) = AppMessageBox::plant("discord", &["a"]);
    let (second, _second_state) = AppMessageBox::plant("discord", &["b"]);
    cleaners.register(Box::new(first)).expect("first");
    let error = cleaners
        .register(Box::new(second))
        .expect_err("a second cleaner for discord");
    assert_eq!(
        error,
        "OSL timed-delete sweep already has a cleaner for app discord"
    );
    assert_eq!(cleaners.len(), 1, "exactly one delete action per app");
}

#[test]
fn task_3308_a_record_the_cleaner_refuses_stays_in_the_store() {
    struct AlwaysRefuses;
    impl SharedAppCleaner for AlwaysRefuses {
        fn app_id(&self) -> &str {
            "discord"
        }
        fn clean_due_message(&mut self, _record: &DueTimedDeleteRecord) -> Result<(), String> {
            Err("discord refused".to_owned())
        }
    }

    let job = TimedDeleteSweepJob::new(60).expect("job");
    let mut work = HeldRecords {
        records: vec![DueTimedDeleteRecord {
            app_id: "discord".to_owned(),
            conversation_id: "dm:refused".to_owned(),
            message_locator: "discord-refused-3308".to_owned(),
            sent_at_unix_seconds: 1_900_000_000,
            delete_at_unix_seconds: 1_900_003_000,
            protection: "ordinary".to_owned(),
        }],
        retired: Vec::new(),
    };
    let mut cleaners = SharedAppCleaners::new();
    cleaners
        .register(Box::new(AlwaysRefuses))
        .expect("register");

    let pass = job
        .wake(
            FIXTURE_NOW,
            &mut work,
            &mut cleaners,
            &mut PlantedProtectedParts::default(),
        )
        .expect("wake");
    assert_eq!(pass.swept_count(), 0);
    assert_eq!(pass.refused.len(), 1);
    assert_eq!(pass.refused[0].code, "cleaner_refused");
    assert_eq!(pass.refused[0].reason, "discord refused");
    assert!(
        work.retired.is_empty(),
        "a refused record is not retired from the store"
    );
}

#[test]
fn task_3308_the_wake_interval_is_bounded() {
    assert!(TimedDeleteSweepJob::new(0).is_err(), "no zero interval");
    assert!(
        TimedDeleteSweepJob::new(86_401).is_err(),
        "no week-long nap"
    );
    let job = TimedDeleteSweepJob::new(300).expect("five minutes");
    assert_eq!(job.wake_every_seconds(), 300);
    assert_eq!(job.next_wake_at(1_900_000_000), 1_900_000_300);
}

#[test]
fn task_3308_the_real_clock_is_a_real_clock() {
    use task_3308_timed_delete_sweep_job::timed_delete_sweep_job::SystemSweepClock;
    let clock = SystemSweepClock;
    let now = clock.now_unix_seconds();
    assert!(
        now > 1_700_000_000,
        "the system clock reads a real second, got {now}"
    );
}
