use std::fs;
use std::path::Path;

use tempfile::TempDir;

use osl_privacy_hub::update_apply::{
    begin_update_apply_with_old_files_at, read_update_apply_record, record_replaced_built_files_at,
    rollback_failed_update_after_start_failure_at, UpdateApplyStatus,
};
use osl_privacy_hub::update_state_backup::{
    copy_identity_and_history_before_update, identity_and_friend_counts_for_config,
    update_state_copy_record_path,
};

#[test]
fn task_3174_failed_new_build_start_rolls_back_build_identity_and_friends() {
    let root = TempDir::new().expect("temp root");
    let install = root.path().join("install");
    let staged = root.path().join("staged");
    let config = root.path().join("config");
    let core = config.join("osl-core");
    fs::create_dir_all(&install).expect("create install");
    fs::create_dir_all(&staged).expect("create staged");
    fs::create_dir_all(&core).expect("create core");

    fs::write(
        install.join("OSL Privacy.exe"),
        b"version=0.1.0\nold executable",
    )
    .expect("write old executable");
    fs::write(
        install.join("WebView2Loader.dll"),
        b"version=0.1.0\nold loader",
    )
    .expect("write old loader");
    fs::write(
        staged.join("OSL Privacy.exe"),
        b"version=0.2.0\nnew executable",
    )
    .expect("write new executable");
    fs::write(
        staged.join("WebView2Loader.dll"),
        b"version=0.2.0\nnew loader",
    )
    .expect("write new loader");
    write_identity_files(&core, 3);
    write_people_file(&core, 2);

    let state_copy = copy_identity_and_history_before_update(&config, "0.2.0")
        .expect("copy state before update");
    let state_copy_record = update_state_copy_record_path(Path::new(&state_copy.backup_dir));
    assert_eq!(state_copy.copied_counts.identity_count, 3);
    assert_eq!(state_copy.copied_counts.friend_count, 2);

    let built_files = vec![
        "OSL Privacy.exe".to_owned(),
        "WebView2Loader.dll".to_owned(),
    ];
    let pending = begin_update_apply_with_old_files_at(
        &config,
        &install,
        "0.1.0",
        "0.2.0",
        built_files.clone(),
        Some(&state_copy_record),
        1_720_000_100,
    )
    .expect("begin update apply with rollback copy");
    for relative in &built_files {
        fs::copy(staged.join(relative), install.join(relative)).expect("replace built file");
    }
    let applied = record_replaced_built_files_at(pending, built_files.clone(), 1_720_000_100)
        .expect("record pending update");
    assert_eq!(applied.status, UpdateApplyStatus::PendingRestart);
    assert_eq!(installed_version(&install.join("OSL Privacy.exe")), "0.2.0");

    force_bad_new_build_state(&core);
    let (bad_identity_count, bad_friend_count) =
        identity_and_friend_counts_for_config(&config).expect("count bad live state");
    assert_eq!(bad_identity_count, 1);
    assert_eq!(bad_friend_count, 1);

    let failed = rollback_failed_update_after_start_failure_at(
        &config,
        "forced start failure: task 3174",
        1_720_000_130,
    )
    .expect("rollback failed update")
    .expect("pending update record exists");
    let persisted = read_update_apply_record(&config.join("update-apply-record.json"))
        .expect("read failed update record");
    assert_eq!(persisted, failed);
    assert_eq!(failed.status, UpdateApplyStatus::Failed);
    assert_eq!(
        failed.failure_reason.as_deref(),
        Some("forced start failure: task 3174")
    );
    assert_eq!(failed.failed_at_unix_seconds, Some(1_720_000_130));
    assert_eq!(failed.successful_start_count, 0);
    assert_eq!(installed_version(&install.join("OSL Privacy.exe")), "0.1.0");
    assert_eq!(
        installed_version(&install.join("WebView2Loader.dll")),
        "0.1.0"
    );

    let (identity_count, friend_count) =
        identity_and_friend_counts_for_config(&config).expect("count restored live state");
    assert_eq!(identity_count, state_copy.copied_counts.identity_count);
    assert_eq!(friend_count, state_copy.copied_counts.friend_count);

    println!("TASK3174_FORCED_FAILED_START=true");
    println!("TASK3174_ROLLED_BACK_VERSION={}", failed.previous_version);
    println!(
        "TASK3174_OLD_FILE_RESTORED_VERSION={}",
        installed_version(&install.join("OSL Privacy.exe"))
    );
    println!(
        "TASK3174_RESTORED_BUILT_FILES={}",
        failed.built_files.join(",")
    );
    println!(
        "TASK3174_COPY_IDENTITY_COUNT={}",
        state_copy.copied_counts.identity_count
    );
    println!("TASK3174_LIVE_IDENTITY_COUNT_AFTER_ROLLBACK={identity_count}");
    println!(
        "TASK3174_COPY_FRIEND_COUNT={}",
        state_copy.copied_counts.friend_count
    );
    println!("TASK3174_LIVE_FRIEND_COUNT_AFTER_ROLLBACK={friend_count}");
    println!("TASK3174_RECORD_STATUS={:?}", failed.status);
    println!(
        "TASK3174_FAILURE_REASON={}",
        failed.failure_reason.as_deref().unwrap_or("")
    );
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

fn installed_version(path: &Path) -> String {
    let text = fs::read_to_string(path).expect("read installed file");
    text.lines()
        .find_map(|line| line.strip_prefix("version="))
        .expect("installed file carries version")
        .to_owned()
}
