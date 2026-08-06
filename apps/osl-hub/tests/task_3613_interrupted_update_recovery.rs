use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};
use tempfile::TempDir;

use osl_privacy_hub::update_apply::{
    apply_staged_build_update_at, begin_update_apply_with_old_files_at,
    mark_update_finished_after_successful_start_at, read_update_apply_record,
    record_interrupted_update_built_file_replaced_at, record_interrupted_update_data_step_at,
    recover_interrupted_update_before_start_at, InterruptedUpdateRecoveryStatus, UpdateApplyStatus,
};
use osl_privacy_hub::update_state_backup::{
    copy_identity_and_history_before_update, update_state_copy_record_path,
};

const OLD_VERSION: &str = "0.1.0";
const NEW_VERSION: &str = "0.2.0";

#[test]
fn task_3613_stopped_replacement_and_data_steps_recover_before_start() {
    let mut started_versions = Vec::new();
    let mut opened_counts = Vec::new();
    let mut stopped_steps = Vec::new();
    let mut mixed_version_runs = 0usize;

    let data = stopped_after_recorded_data_change();
    collect_success(
        data,
        &mut started_versions,
        &mut opened_counts,
        &mut stopped_steps,
        &mut mixed_version_runs,
    );

    let exe = stopped_after_one_replacement("OSL Privacy.exe");
    collect_success(
        exe,
        &mut started_versions,
        &mut opened_counts,
        &mut stopped_steps,
        &mut mixed_version_runs,
    );

    let loader = stopped_after_two_replacements();
    collect_success(
        loader,
        &mut started_versions,
        &mut opened_counts,
        &mut stopped_steps,
        &mut mixed_version_runs,
    );

    let committed = stopped_after_apply_record_written();
    collect_success(
        committed,
        &mut started_versions,
        &mut opened_counts,
        &mut stopped_steps,
        &mut mixed_version_runs,
    );

    let failed = run_that_cannot_start_fails();
    assert_eq!(
        failed.record_status, "Failed",
        "unstartable recovery must write a failed apply record"
    );

    assert_eq!(opened_counts, vec![2, 2, 2, 2]);
    assert_eq!(mixed_version_runs, 0);
    assert_eq!(started_versions, vec!["0.1.0", "0.1.0", "0.1.0", "0.2.0"]);

    println!("TASK3613_STOPPED_RUN_COUNT={}", started_versions.len());
    println!("TASK3613_STOPPED_STEPS={}", stopped_steps.join(","));
    println!(
        "TASK3613_STARTED_COMPLETE_VERSIONS={}",
        started_versions.join(",")
    );
    println!(
        "TASK3613_OPENED_MARKED_MESSAGES_PER_RUN={}",
        opened_counts
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(",")
    );
    println!("TASK3613_MIXED_VERSION_RUNS={mixed_version_runs}");
    println!(
        "TASK3613_CANNOT_START_RECORD_STATUS={}",
        failed.record_status
    );
    println!("TASK3613_CANNOT_START_REASON={}", failed.reason);
}

fn collect_success(
    result: SuccessfulStoppedRun,
    started_versions: &mut Vec<String>,
    opened_counts: &mut Vec<usize>,
    stopped_steps: &mut Vec<String>,
    mixed_version_runs: &mut usize,
) {
    assert!(!result.mixed_version_run);
    assert_eq!(result.opened_marked_messages, 2);
    started_versions.push(result.complete_version);
    opened_counts.push(result.opened_marked_messages);
    stopped_steps.push(result.stopped_step);
    if result.mixed_version_run {
        *mixed_version_runs += 1;
    }
}

struct SuccessfulStoppedRun {
    stopped_step: String,
    complete_version: String,
    opened_marked_messages: usize,
    mixed_version_run: bool,
}

struct FailedStoppedRun {
    record_status: String,
    reason: String,
}

struct Fixture {
    _root: TempDir,
    install: PathBuf,
    staged: PathBuf,
    config: PathBuf,
    built_files: Vec<String>,
}

fn stopped_after_recorded_data_change() -> SuccessfulStoppedRun {
    let fixture = prepared_pending_fixture(1_720_001_000);
    let core = fixture.config.join("osl-core");
    force_bad_new_build_state(&core);
    write_message_history(&core.join("store"), 1, false);
    record_interrupted_update_data_step_at(
        &fixture.config,
        "protected_message_store_changed",
        1_720_001_001,
    )
    .expect("record data-change step");

    let report = recover_interrupted_update_before_start_at(&fixture.config, 1_720_001_010)
        .expect("recover data step")
        .expect("recovery report exists");
    assert_eq!(
        report.status,
        InterruptedUpdateRecoveryStatus::RestoredOldBeforeStart
    );
    started_run(
        "data_changed",
        &fixture,
        report.complete_version.unwrap(),
        report.mixed_version_run,
    )
}

fn stopped_after_one_replacement(relative: &str) -> SuccessfulStoppedRun {
    let fixture = prepared_pending_fixture(1_720_001_100);
    fs::copy(
        fixture.staged.join(relative),
        fixture.install.join(relative),
    )
    .expect("replace first built file");
    record_interrupted_update_built_file_replaced_at(&fixture.config, relative, 1_720_001_101)
        .expect("record replacement step");

    let report = recover_interrupted_update_before_start_at(&fixture.config, 1_720_001_110)
        .expect("recover first replacement")
        .expect("recovery report exists");
    assert_eq!(
        report.status,
        InterruptedUpdateRecoveryStatus::RestoredOldBeforeStart
    );
    started_run(
        relative,
        &fixture,
        report.complete_version.unwrap(),
        report.mixed_version_run,
    )
}

fn stopped_after_two_replacements() -> SuccessfulStoppedRun {
    let fixture = prepared_pending_fixture(1_720_001_200);
    for relative in &fixture.built_files {
        fs::copy(
            fixture.staged.join(relative),
            fixture.install.join(relative),
        )
        .expect("replace built file");
        record_interrupted_update_built_file_replaced_at(&fixture.config, relative, 1_720_001_201)
            .expect("record replacement step");
    }

    let report = recover_interrupted_update_before_start_at(&fixture.config, 1_720_001_210)
        .expect("recover second replacement")
        .expect("recovery report exists");
    assert_eq!(
        report.status,
        InterruptedUpdateRecoveryStatus::RestoredOldBeforeStart
    );
    started_run(
        "WebView2Loader.dll",
        &fixture,
        report.complete_version.unwrap(),
        report.mixed_version_run,
    )
}

fn stopped_after_apply_record_written() -> SuccessfulStoppedRun {
    let fixture = new_fixture();
    let record = apply_staged_build_update_at(
        &fixture.install,
        &fixture.staged,
        &fixture.config,
        OLD_VERSION,
        NEW_VERSION,
        1_720_001_300,
    )
    .expect("apply staged build");
    assert_eq!(record.status, UpdateApplyStatus::PendingRestart);

    let report = recover_interrupted_update_before_start_at(&fixture.config, 1_720_001_310)
        .expect("recover committed replacement")
        .expect("recovery report exists");
    assert_eq!(
        report.status,
        InterruptedUpdateRecoveryStatus::CompleteNewPendingRestart
    );
    let complete_version = report.complete_version.unwrap();
    let finished = mark_update_finished_after_successful_start_at(
        &fixture.config,
        &complete_version,
        1_720_001_320,
    )
    .expect("mark successful start")
    .expect("update record exists");
    assert_eq!(finished.status, UpdateApplyStatus::Finished);
    assert_eq!(finished.successful_start_count, 1);
    started_run(
        "apply_record_written",
        &fixture,
        complete_version,
        report.mixed_version_run,
    )
}

fn run_that_cannot_start_fails() -> FailedStoppedRun {
    let fixture = prepared_pending_fixture(1_720_001_400);
    fs::remove_file(
        fixture
            .config
            .join("update-old-build-backups")
            .join("pre-update-1720001400-0.2.0-0")
            .join("OSL Privacy.exe"),
    )
    .expect("remove old executable backup");
    fs::copy(
        fixture.staged.join("OSL Privacy.exe"),
        fixture.install.join("OSL Privacy.exe"),
    )
    .expect("replace executable without restorable backup");
    record_interrupted_update_built_file_replaced_at(
        &fixture.config,
        "OSL Privacy.exe",
        1_720_001_401,
    )
    .expect("record unstartable replacement");

    let report = recover_interrupted_update_before_start_at(&fixture.config, 1_720_001_410)
        .expect("failed recovery report")
        .expect("recovery report exists");
    assert_eq!(
        report.status,
        InterruptedUpdateRecoveryStatus::FailedCannotStart
    );
    let record = read_update_apply_record(&fixture.config.join("update-apply-record.json"))
        .expect("read failed apply record");
    FailedStoppedRun {
        record_status: format!("{:?}", record.status),
        reason: record.failure_reason.unwrap_or_default(),
    }
}

fn prepared_pending_fixture(started_at: u64) -> Fixture {
    let fixture = new_fixture();
    let state_copy = copy_identity_and_history_before_update(&fixture.config, NEW_VERSION)
        .expect("copy state before update");
    let state_copy_record = update_state_copy_record_path(Path::new(&state_copy.backup_dir));
    begin_update_apply_with_old_files_at(
        &fixture.config,
        &fixture.install,
        OLD_VERSION,
        NEW_VERSION,
        fixture.built_files.clone(),
        Some(&state_copy_record),
        started_at,
    )
    .expect("begin recoverable pending update");
    fixture
}

fn started_run(
    stopped_step: &str,
    fixture: &Fixture,
    recovered_version: String,
    mixed_version_run: bool,
) -> SuccessfulStoppedRun {
    let installed_version = complete_installed_version(&fixture.install)
        .expect("install must be exactly one complete version");
    assert_eq!(installed_version, recovered_version);
    let opened_marked_messages = open_marked_messages(&fixture.config);
    SuccessfulStoppedRun {
        stopped_step: stopped_step.to_owned(),
        complete_version: installed_version,
        opened_marked_messages,
        mixed_version_run,
    }
}

fn new_fixture() -> Fixture {
    let root = TempDir::new().expect("temp root");
    let install = root.path().join("install");
    let staged = root.path().join("staged");
    let config = root.path().join("config");
    let core = config.join("osl-core");
    fs::create_dir_all(&install).expect("create install");
    fs::create_dir_all(&staged).expect("create staged");
    fs::create_dir_all(&core).expect("create core");

    write_build_file(&install, "OSL Privacy.exe", OLD_VERSION, "old executable");
    write_build_file(&install, "WebView2Loader.dll", OLD_VERSION, "old loader");
    write_build_file(&staged, "OSL Privacy.exe", NEW_VERSION, "new executable");
    write_build_file(&staged, "WebView2Loader.dll", NEW_VERSION, "new loader");
    write_identity_files(&core, 3);
    write_people_file(&core, 2);
    write_message_history(&core.join("store"), 2, true);

    Fixture {
        _root: root,
        install,
        staged,
        config,
        built_files: vec![
            "OSL Privacy.exe".to_owned(),
            "WebView2Loader.dll".to_owned(),
        ],
    }
}

fn write_build_file(root: &Path, name: &str, version: &str, body: &str) {
    fs::write(root.join(name), format!("version={version}\n{body}")).expect("write build file");
}

fn complete_installed_version(install: &Path) -> Option<String> {
    let exe = installed_version(&install.join("OSL Privacy.exe"));
    let loader = installed_version(&install.join("WebView2Loader.dll"));
    (exe == loader).then_some(exe)
}

fn installed_version(path: &Path) -> String {
    let text = fs::read_to_string(path).expect("read installed file");
    text.lines()
        .find_map(|line| line.strip_prefix("version="))
        .expect("installed file carries version")
        .to_owned()
}

fn write_identity_files(core: &Path, count: usize) {
    fs::write(core.join("identity.json"), b"sealed identity 0").expect("write flat identity");
    for index in 1..count {
        let slot_dir = core.join("hub-identities").join(format!("slot-{index}"));
        fs::create_dir_all(&slot_dir).expect("create identity slot");
        fs::write(
            slot_dir.join("identity.json"),
            format!("sealed identity {index}"),
        )
        .expect("write slot identity");
    }
}

fn write_people_file(core: &Path, count: usize) {
    let people = serde_json::json!({
        "version": 3,
        "people": (0..count)
            .map(|index| {
                (
                    format!("person-{index}"),
                    serde_json::json!({
                        "osl_user_id": format!("osl_friend_{index}"),
                        "ed25519_public": format!("ed25519-{index}"),
                        "safety_number_verified": true
                    }),
                )
            })
            .collect::<serde_json::Map<_, _>>()
    });
    fs::write(
        core.join("hub_people.json"),
        serde_json::to_vec(&people).unwrap(),
    )
    .expect("write people");
}

fn force_bad_new_build_state(core: &Path) {
    let _ = fs::remove_dir_all(core.join("hub-identities"));
    fs::write(core.join("identity.json"), b"broken new build identity")
        .expect("write bad identity");
    write_people_file(core, 1);
}

fn write_message_history(store_dir: &Path, count: usize, marked: bool) {
    fs::create_dir_all(store_dir).expect("create store dir");
    let path = store_dir.join("messages.sqlite");
    let _ = fs::remove_file(&path);
    let conn = Connection::open(path).expect("open messages db");
    conn.execute_batch(
        "CREATE TABLE messages (
          mid_bi BLOB PRIMARY KEY,
          ciphertext BLOB NOT NULL,
          marked INTEGER NOT NULL
        );",
    )
    .expect("schema messages");
    for index in 0..count {
        conn.execute(
            "INSERT INTO messages (mid_bi, ciphertext, marked) VALUES (?1, ?2, ?3)",
            params![
                format!("task-3613-message-{index}"),
                format!("TASK3613_MARKED_MESSAGE_{index}").as_bytes(),
                if marked { 1 } else { 0 }
            ],
        )
        .expect("insert message");
    }
}

fn open_marked_messages(config: &Path) -> usize {
    let conn = Connection::open(
        config
            .join("osl-core")
            .join("store")
            .join("messages.sqlite"),
    )
    .expect("open messages db");
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM messages
             WHERE marked = 1 AND CAST(ciphertext AS TEXT) LIKE 'TASK3613_MARKED_MESSAGE_%'",
            [],
            |row| row.get(0),
        )
        .expect("count marked messages");
    usize::try_from(count).expect("valid marked message count")
}
