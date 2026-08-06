#![cfg(feature = "core")]

const CLIPBOARD_PNG: &[u8] = b"\x89PNG\r\n\x1a\nOSL clipboard image fixture";

#[test]
fn task_0637_direct_clipboard_image_command_returns_tray_count_one() {
    let result = osl_privacy_hub::broker::cmd_clipboard_image_attachment_tray(
        CLIPBOARD_PNG,
        "image/png",
        false,
    )
    .expect("direct clipboard-image command accepts a pasted PNG");

    assert_eq!(result.tray_count, 1);
    assert_eq!(result.records.len(), 1);
    let record = &result.records[0];
    assert!(record.checked);
    assert_eq!(record.original_filename, "clipboard-image.png");
    assert_eq!(record.mime_type, "image/png");
    assert_eq!(record.plaintext_size, CLIPBOARD_PNG.len() as u64);
    assert!(!record.view_once);
    assert!(
        record.attachment_id.starts_with("peer-"),
        "clipboard image tray record must carry an attachment id"
    );

    println!(
        "TASK_0637_CLIPBOARD_IMAGE command=cmd_clipboard_image_attachment_tray tray_count={} records={} checked={} filename={} mime_type={} plaintext_size={}",
        result.tray_count,
        result.records.len(),
        record.checked,
        record.original_filename,
        record.mime_type,
        record.plaintext_size
    );
}
