use std::fs;

use osl_privacy_hub::installed_build::{
    installed_build_chat_warning, store_and_read_installed_build_record,
    InstalledBuildWarningReason, CHANGED_BUILD_CHAT_WARNING, INSTALLED_BUILD_RECORD_FILE,
};

#[test]
fn task_1358_changed_and_corrupt_build_proofs_warn_without_blocking_chat_send() {
    let directory = tempfile::tempdir().expect("temp dir");
    let built_file = directory.path().join("installed.bin");
    let record_path = directory.path().join(INSTALLED_BUILD_RECORD_FILE);

    fs::write(&built_file, b"installed build").expect("write installed build");
    store_and_read_installed_build_record(&record_path, &built_file)
        .expect("write clean installed-build proof");
    assert_eq!(installed_build_chat_warning(&record_path), None);

    fs::write(&built_file, b"installed build changed").expect("change installed build");
    let changed = installed_build_chat_warning(&record_path)
        .expect("changed build must produce the chat warning");
    assert_eq!(changed.reason, InstalledBuildWarningReason::Changed);
    assert_eq!(changed.message, CHANGED_BUILD_CHAT_WARNING);
    assert!(changed.message_sending_available);
    println!(
        "TASK1358 changed_warning=\"{}\" reason={:?} message_sending_available={}",
        changed.message, changed.reason, changed.message_sending_available
    );

    fs::write(&record_path, b"{not-json").expect("corrupt installed-build proof");
    let corrupt = installed_build_chat_warning(&record_path)
        .expect("corrupt proof must produce the chat warning");
    assert_eq!(corrupt.reason, InstalledBuildWarningReason::CorruptProof);
    assert_eq!(corrupt.message, CHANGED_BUILD_CHAT_WARNING);
    assert!(corrupt.message_sending_available);
    println!(
        "TASK1358 corrupt_warning=\"{}\" reason={:?} message_sending_available={}",
        corrupt.message, corrupt.reason, corrupt.message_sending_available
    );
}
