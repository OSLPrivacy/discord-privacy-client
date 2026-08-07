//! TASK 0623: the attachment tray keeps valid files when an oversized file is
//! added alongside them.
//!
//! The check stores one valid file and one file over
//! `MAX_ATTACHMENT_SIZE`, queries the tray, and requires that exactly the
//! valid file remains (tray count 1). A second test points the same check at
//! a fixture WITHOUT the valid file and requires the check to fail, so the
//! check cannot pass with the feature absent.

#![cfg(feature = "core")]

use osl_privacy_hub::native_attachment_jobs::{
    AttachmentTrayFileInput, NativeAttachmentJobRegistry, MAX_ATTACHMENT_SIZE,
};

const CONTEXT: &str = "discord:conversation:task-0623";
const VALID_NAME: &str = "keepable notes.txt";
const VALID_SIZE: u64 = 512;

fn oversized_file() -> AttachmentTrayFileInput {
    AttachmentTrayFileInput {
        name: "oversized.bin".to_owned(),
        r#type: "application/octet-stream".to_owned(),
        size: MAX_ATTACHMENT_SIZE + 1,
    }
}

fn fixture_with_valid_file() -> Vec<AttachmentTrayFileInput> {
    vec![
        AttachmentTrayFileInput {
            name: VALID_NAME.to_owned(),
            r#type: "text/plain".to_owned(),
            size: VALID_SIZE,
        },
        oversized_file(),
    ]
}

fn fixture_without_valid_file() -> Vec<AttachmentTrayFileInput> {
    vec![oversized_file()]
}

fn run_tray_check(fixture: Vec<AttachmentTrayFileInput>) {
    let mut registry = NativeAttachmentJobRegistry::default();
    registry
        .store_tray_records(CONTEXT, fixture)
        .expect("storing the fixture files succeeds");
    let records = registry.query_tray_records(CONTEXT);
    println!(
        "TASK0623 tray_keeps_valid_files tray_count={} names={:?} oversized_bytes={}",
        records.len(),
        records
            .iter()
            .map(|record| record.name.as_str())
            .collect::<Vec<_>>(),
        MAX_ATTACHMENT_SIZE + 1
    );
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].name, VALID_NAME);
    assert_eq!(records[0].size, VALID_SIZE);
}

#[test]
fn task_0623_tray_keeps_the_valid_file_after_an_oversized_file_is_added() {
    run_tray_check(fixture_with_valid_file());
}

#[test]
fn task_0623_check_fails_when_pointed_at_a_fixture_without_the_valid_file() {
    let failed =
        std::panic::catch_unwind(|| run_tray_check(fixture_without_valid_file())).is_err();
    println!("TASK0623 negative_control check_failed={failed}");
    assert!(failed, "the tray check must go red on the bad fixture");
}
