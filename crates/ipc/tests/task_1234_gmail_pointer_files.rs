use ipc::email_pointer_files::{
    send_gmail_fixture_protected_pointer_file, GmailFixtureProtectedFile,
    GMAIL_COVER_DRAFT_LIMIT_BYTES,
};

const MIB: u64 = 1024 * 1024;
const PROTECTED_FILE_BYTES: u64 = 19 * MIB;

#[test]
fn task_1234_gmail_pointer_files_ignore_mail_size() {
    let delivery = send_gmail_fixture_protected_pointer_file(GmailFixtureProtectedFile {
        display_name: "task1234-protected-video.bin".to_owned(),
        size_bytes: PROTECTED_FILE_BYTES,
        pointer_id: "ptr-task1234-gmail-fixture".to_owned(),
    })
    .expect("Gmail fixture sends a protected pointer for the large file");

    println!("TASK1234 gmail_fixture=send_gmail_fixture_protected_pointer_file");
    println!("TASK1234 split_step={}", delivery.split_step);
    println!("TASK1234 cover_draft_bytes={}", delivery.cover_draft_bytes);
    println!(
        "TASK1234 cover_draft_limit_bytes={}",
        GMAIL_COVER_DRAFT_LIMIT_BYTES
    );
    println!(
        "TASK1234 protected_file_record_bytes={}",
        delivery.protected_file_record_bytes
    );

    assert_eq!(delivery.split_step, "protected_file_record");
    assert!(delivery.cover_draft_bytes < GMAIL_COVER_DRAFT_LIMIT_BYTES);
    assert!(delivery.protected_file_record_bytes > GMAIL_COVER_DRAFT_LIMIT_BYTES);
}
