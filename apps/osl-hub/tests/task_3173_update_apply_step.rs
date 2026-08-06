use std::fs;
use std::path::Path;

use rusqlite::{params, Connection};
use tempfile::TempDir;

use osl_privacy_hub::update_apply::{
    apply_staged_build_update_at, begin_update_apply,
    mark_update_finished_after_successful_start_at, read_update_apply_record,
    record_replaced_built_files_at, UpdateApplyStatus,
};

#[test]
fn task_3173_replaces_build_files_and_finishes_after_new_build_start() {
    let root = TempDir::new().expect("temp root");
    let install = root.path().join("install");
    let staged = root.path().join("staged");
    let config = root.path().join("config");
    let core = config.join("osl-core");
    fs::create_dir_all(&install).expect("create install");
    fs::create_dir_all(&staged).expect("create staged");
    fs::create_dir_all(&core).expect("create core");

    fs::write(install.join("OSL Privacy.exe"), b"old build executable")
        .expect("write old executable");
    fs::write(install.join("WebView2Loader.dll"), b"old loader").expect("write old loader");
    fs::write(staged.join("OSL Privacy.exe"), b"new build executable")
        .expect("write new executable");
    fs::write(staged.join("WebView2Loader.dll"), b"new loader").expect("write new loader");
    write_identity(&core);
    write_message_history(&core.join("store"), 3);

    let identity_path = core.join("identity.json");
    let history_path = core.join("store").join("messages.sqlite");
    let identity_before = fs::read(&identity_path).expect("read identity before");
    let history_before = fs::read(&history_path).expect("read history before");

    let record =
        apply_staged_build_update_at(&install, &staged, &config, "0.1.0", "0.2.0", 1_720_000_000)
            .expect("apply staged build update");
    assert_eq!(record.previous_version, "0.1.0");
    assert_eq!(record.applied_version, "0.2.0");
    assert_ne!(record.previous_version, record.applied_version);
    assert_eq!(record.status, UpdateApplyStatus::PendingRestart);
    assert_eq!(record.successful_start_count, 0);
    assert_eq!(record.finished_at_unix_seconds, None);
    assert_eq!(
        fs::read(install.join("OSL Privacy.exe")).expect("read replaced executable"),
        b"new build executable"
    );
    assert_eq!(
        fs::read(install.join("WebView2Loader.dll")).expect("read replaced loader"),
        b"new loader"
    );
    assert_eq!(
        fs::read(&identity_path).expect("read identity after apply"),
        identity_before
    );
    assert_eq!(
        fs::read(&history_path).expect("read history after apply"),
        history_before
    );
    assert_eq!(
        record.identity_sha256_before,
        record.identity_sha256_after_apply
    );
    assert_eq!(
        record.message_history_sha256_before,
        record.message_history_sha256_after_apply
    );

    let old_start = mark_update_finished_after_successful_start_at(&config, "0.1.0", 1_720_000_010)
        .expect("old build start is observed");
    let old_start = old_start.expect("pending record still exists");
    assert_eq!(old_start.status, UpdateApplyStatus::PendingRestart);
    assert_eq!(old_start.successful_start_count, 0);
    assert_eq!(old_start.finished_at_unix_seconds, None);

    let finished = mark_update_finished_after_successful_start_at(&config, "0.2.0", 1_720_000_020)
        .expect("new build start finishes update")
        .expect("finished record exists");
    assert_eq!(finished.status, UpdateApplyStatus::Finished);
    assert_eq!(finished.successful_start_count, 1);
    assert_eq!(finished.finished_at_unix_seconds, Some(1_720_000_020));
    assert_eq!(
        fs::read(&identity_path).expect("read identity after finish"),
        identity_before
    );
    assert_eq!(
        fs::read(&history_path).expect("read history after finish"),
        history_before
    );

    let persisted = read_update_apply_record(&config.join("update-apply-record.json"))
        .expect("read persisted update apply record");
    assert_eq!(persisted, finished);

    let pending = begin_update_apply(&config, &install, "0.2.0", "0.3.0")
        .expect("real updater pending apply guard");
    fs::write(
        install.join("OSL Privacy.exe"),
        b"newer signed updater executable",
    )
    .expect("replace signed updater executable");
    let production_pending =
        record_replaced_built_files_at(pending, vec!["OSL Privacy.exe".to_owned()], 1_720_000_030)
            .expect("real updater records pending update only after replacement");
    assert_eq!(production_pending.status, UpdateApplyStatus::PendingRestart);
    assert_eq!(production_pending.successful_start_count, 0);

    println!("TASK3173_PREVIOUS_VERSION={}", finished.previous_version);
    println!("TASK3173_APPLIED_VERSION={}", finished.applied_version);
    println!(
        "TASK3173_VERSION_CHANGED={}",
        finished.previous_version != finished.applied_version
    );
    println!(
        "TASK3173_REPLACED_BUILT_FILES={}",
        finished.built_files.join(",")
    );
    println!(
        "TASK3173_IDENTITY_SAME={}",
        finished.identity_sha256_before == finished.identity_sha256_after_apply
            && fs::read(&identity_path).expect("identity still readable") == identity_before
    );
    println!(
        "TASK3173_HISTORY_SAME={}",
        finished.message_history_sha256_before == finished.message_history_sha256_after_apply
            && fs::read(&history_path).expect("history still readable") == history_before
    );
    println!("TASK3173_OLD_START_STATUS={:?}", old_start.status);
    println!(
        "TASK3173_OLD_START_COUNT={}",
        old_start.successful_start_count
    );
    println!("TASK3173_FINISHED_STATUS={:?}", finished.status);
    println!(
        "TASK3173_SUCCESSFUL_START_COUNT={}",
        finished.successful_start_count
    );
    println!(
        "TASK3173_FINISHED_AT_UNIX_SECONDS={}",
        finished.finished_at_unix_seconds.unwrap()
    );
    println!(
        "TASK3173_REAL_UPDATER_RECORD_STATUS={:?}",
        production_pending.status
    );
}

fn write_identity(core: &Path) {
    fs::write(core.join("identity.json"), b"sealed task 3173 identity").expect("write identity");
}

fn write_message_history(store_dir: &Path, count: usize) {
    fs::create_dir_all(store_dir).expect("create store dir");
    let conn = Connection::open(store_dir.join("messages.sqlite")).expect("open messages db");
    conn.execute_batch(
        "CREATE TABLE messages (mid_bi BLOB PRIMARY KEY, ciphertext BLOB NOT NULL);",
    )
    .expect("schema messages");
    for index in 0..count {
        conn.execute(
            "INSERT INTO messages (mid_bi, ciphertext) VALUES (?1, ?2)",
            params![
                format!("message-{index}"),
                format!("ciphertext-{index}").as_bytes()
            ],
        )
        .expect("insert message");
    }
}
