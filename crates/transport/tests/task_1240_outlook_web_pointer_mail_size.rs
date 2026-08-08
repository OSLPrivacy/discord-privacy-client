use transport::outlook_web_pointer::{
    fixture_large_protected_pointer_file, send_outlook_web_protected_pointer_file,
    OUTLOOK_WEB_MAIL_SIZE_LIMIT_BYTES,
};

#[test]
fn task_1240_outlook_web_pointer_file_record_does_not_count_against_cover_draft_size() {
    let sent = send_outlook_web_protected_pointer_file(fixture_large_protected_pointer_file());
    let cover_draft_bytes = sent.cover_draft.byte_len();
    let file_record_bytes = sent.file_record.file_size_bytes;

    println!("TASK1240 outlook_web_mail_size_limit_bytes={OUTLOOK_WEB_MAIL_SIZE_LIMIT_BYTES}");
    println!("TASK1240 cover_draft_bytes={cover_draft_bytes}");
    println!("TASK1240 file_record_bytes={file_record_bytes}");
    println!(
        "TASK1240 cover_draft_below_14_5_mb={}",
        cover_draft_bytes < OUTLOOK_WEB_MAIL_SIZE_LIMIT_BYTES
    );
    println!(
        "TASK1240 file_record_above_14_5_mb={}",
        file_record_bytes > OUTLOOK_WEB_MAIL_SIZE_LIMIT_BYTES
    );

    assert!(
        cover_draft_bytes < OUTLOOK_WEB_MAIL_SIZE_LIMIT_BYTES,
        "Outlook web cover draft must stay below 14.5 MB"
    );
    assert!(
        file_record_bytes > OUTLOOK_WEB_MAIL_SIZE_LIMIT_BYTES,
        "protected file record must be larger than 14.5 MB"
    );
}
