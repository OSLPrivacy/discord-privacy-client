use std::fs;
use std::path::Path;

use rusqlite::{params, Connection};
use tempfile::TempDir;

use osl_privacy_hub::update_apply::{
    apply_staged_build_update_at, mark_update_finished_after_successful_start_at,
    read_update_apply_record, roll_back_failed_update_after_start_at, UpdateApplyStatus,
};

const OLD_VERSION: &str = "0.1.0";
const NEW_VERSION: &str = "0.2.0";
const MARKER: &str = "UPD-BACK";

#[test]
fn task_3181_failed_first_start_rolls_back_without_losing_friends_or_messages() {
    let failed = Scenario::new();
    let failed_record = failed.apply_update();
    assert_eq!(failed.install_version(), NEW_VERSION);
    assert_eq!(failed.friend_count(), 3);
    assert_eq!(failed.message_count(), 10);
    assert_eq!(failed_record.status, UpdateApplyStatus::PendingRestart);

    let failed_after_rollback = roll_back_failed_update_after_start_at(
        &failed.config,
        &failed.install,
        NEW_VERSION,
        "first start failed: UPD-BACK",
        1_720_000_010,
    )
    .expect("rollback failed update")
    .expect("failed update record exists");
    assert_eq!(failed_after_rollback.status, UpdateApplyStatus::Failed);
    assert_eq!(
        failed_after_rollback.failed_at_unix_seconds,
        Some(1_720_000_010)
    );
    assert_eq!(
        failed_after_rollback.failure_message,
        "first start failed: UPD-BACK"
    );
    assert_eq!(failed.install_version(), OLD_VERSION);
    assert_eq!(failed.friend_count(), 3);
    assert_eq!(failed.message_count(), 10);
    assert_eq!(
        read_update_apply_record(&failed.config.join("update-apply-record.json"))
            .expect("read failed record")
            .status,
        UpdateApplyStatus::Failed
    );

    let working = Scenario::new();
    working.apply_update();
    let finished =
        mark_update_finished_after_successful_start_at(&working.config, NEW_VERSION, 1_720_000_020)
            .expect("working update start finishes")
            .expect("finished update record exists");
    assert_eq!(finished.status, UpdateApplyStatus::Finished);
    assert_eq!(finished.successful_start_count, 1);
    assert_eq!(working.install_version(), NEW_VERSION);
    assert_eq!(working.friend_count(), 3);
    assert_eq!(working.message_count(), 10);

    println!("TASK3181_FAILED_AFTER_VERSION={}", failed.install_version());
    println!("TASK3181_FAILED_FRIEND_COUNT={}", failed.friend_count());
    println!("TASK3181_FAILED_MESSAGE_COUNT={}", failed.message_count());
    println!(
        "TASK3181_FAILED_RECORD_STATUS={}",
        status_label(&failed_after_rollback.status)
    );
    println!(
        "TASK3181_WORKING_AFTER_VERSION={}",
        working.install_version()
    );
    println!("TASK3181_WORKING_FRIEND_COUNT={}", working.friend_count());
    println!("TASK3181_WORKING_MESSAGE_COUNT={}", working.message_count());
    println!(
        "TASK3181_WORKING_RECORD_STATUS={}",
        status_label(&finished.status)
    );
}

struct Scenario {
    _root: TempDir,
    install: std::path::PathBuf,
    staged: std::path::PathBuf,
    config: std::path::PathBuf,
}

impl Scenario {
    fn new() -> Self {
        let root = TempDir::new().expect("temp root");
        let install = root.path().join("install");
        let staged = root.path().join("staged");
        let config = root.path().join("config");
        let core = config.join("osl-core");
        fs::create_dir_all(&install).expect("create install");
        fs::create_dir_all(&staged).expect("create staged");
        fs::create_dir_all(&core).expect("create core");

        fs::write(install.join("version.txt"), OLD_VERSION).expect("write old version");
        fs::write(install.join("OSL Privacy.exe"), b"old build executable")
            .expect("write old executable");
        fs::write(staged.join("version.txt"), NEW_VERSION).expect("write new version");
        fs::write(staged.join("OSL Privacy.exe"), b"new build executable")
            .expect("write new executable");
        write_people_file(&core, 3);
        write_message_history(&core.join("store"), 10);
        Self {
            _root: root,
            install,
            staged,
            config,
        }
    }

    fn apply_update(&self) -> osl_privacy_hub::update_apply::UpdateApplyRecord {
        apply_staged_build_update_at(
            &self.install,
            &self.staged,
            &self.config,
            OLD_VERSION,
            NEW_VERSION,
            1_720_000_000,
        )
        .expect("apply staged update")
    }

    fn install_version(&self) -> String {
        fs::read_to_string(self.install.join("version.txt"))
            .expect("read installed version")
            .trim()
            .to_owned()
    }

    fn friend_count(&self) -> usize {
        friend_count(&self.config.join("osl-core").join("hub_people.json"))
    }

    fn message_count(&self) -> usize {
        message_count(
            &self
                .config
                .join("osl-core")
                .join("store")
                .join("messages.sqlite"),
        )
    }
}

fn write_people_file(core: &Path, count: usize) {
    let people = serde_json::json!({
        "version": 3,
        "people": (0..count)
            .map(|index| {
                (
                    format!("{MARKER}-friend-{index}"),
                    serde_json::json!({
                        "osl_user_id": format!("{MARKER}-osl-friend-{index}"),
                        "ed25519_public": format!("ed25519-{index}"),
                        "safety_number_verified": true
                    }),
                )
            })
            .collect::<serde_json::Map<_, _>>()
    });
    fs::write(
        core.join("hub_people.json"),
        serde_json::to_vec(&people).expect("encode people"),
    )
    .expect("write people");
}

fn write_message_history(store_dir: &Path, count: usize) {
    fs::create_dir_all(store_dir).expect("create store dir");
    let conn = Connection::open(store_dir.join("messages.sqlite")).expect("open messages db");
    conn.execute_batch(
        "CREATE TABLE messages (mid_bi TEXT PRIMARY KEY, ciphertext BLOB NOT NULL);",
    )
    .expect("schema messages");
    for index in 0..count {
        conn.execute(
            "INSERT INTO messages (mid_bi, ciphertext) VALUES (?1, ?2)",
            params![
                format!("{MARKER}-message-{index}"),
                format!("ciphertext-{MARKER}-{index}").as_bytes()
            ],
        )
        .expect("insert message");
    }
}

fn friend_count(path: &Path) -> usize {
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(path).expect("read people")).expect("parse people");
    value
        .get("people")
        .and_then(serde_json::Value::as_object)
        .expect("people object")
        .keys()
        .filter(|key| key.starts_with(MARKER))
        .count()
}

fn message_count(path: &Path) -> usize {
    let conn = Connection::open(path).expect("open messages db");
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM messages WHERE mid_bi LIKE 'UPD-BACK-%'",
            [],
            |row| row.get(0),
        )
        .expect("count messages");
    usize::try_from(count).expect("valid message count")
}

fn status_label(status: &UpdateApplyStatus) -> &'static str {
    match status {
        UpdateApplyStatus::PendingRestart => "pending_restart",
        UpdateApplyStatus::Finished => "finished",
        UpdateApplyStatus::Failed => "failed",
    }
}
