//! Repeatable large-profile fixture for local-store scale checks.
//!
//! The ten gigabytes are represented by a sparse, local-only test file.  Its
//! logical size is exact while the fixture remains safe to run on developer
//! and CI disks: no network request is made and no ten-gigabyte allocation is
//! performed.

use ipc::commands::cmd_osl_persist_inbound;
use ipc::state::AppState;
use std::fs::{self, File};
use store::MessageStore;
use tempfile::TempDir;

const SECRET: [u8; 32] = [0x85; 32];
const CONVERSATION_ID: &str = "osl-chat:task-3685-large-existing-store";
const SENDER_ID: &str = "task-3685-restored-history-sender";
const MESSAGE_COUNT: usize = 20_000;
const ATTACHMENT_COUNT: usize = 2_000;
const SAFE_TEST_FILE_BYTES: u64 = 10_000_000_000;
const SAFE_TEST_FILE_NAME: &str = "task-3685-safe-test-file-bytes.bin";

fn fresh_profile(dir: &std::path::Path) -> AppState {
    let state = AppState::new();
    let store = MessageStore::open(&dir.join("store"), &SECRET).expect("open fresh profile");
    *state.message_store.lock().expect("message store mutex") = Some(store);
    state
}

fn profile_counts(state: &AppState, profile_dir: &std::path::Path) -> (usize, usize, u64) {
    let store = state.message_store.lock().expect("message store mutex");
    let store = store.as_ref().expect("profile has a message store");
    let messages = store
        .count_live_by_channel(CONVERSATION_ID, None)
        .expect("count profile messages");
    let attachments = store
        .live_attachment_count()
        .expect("count profile attachment records");
    let bytes = fs::metadata(profile_dir.join(SAFE_TEST_FILE_NAME))
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    (messages, attachments, bytes)
}

#[test]
fn task_3685_makes_repeatable_large_existing_local_store() {
    let profile = TempDir::new().expect("create fresh profile");
    let state = fresh_profile(profile.path());

    let before = profile_counts(&state, profile.path());
    println!(
        "TASK3685 fresh_profile messages={} attachment_records={} safe_test_file_bytes={}",
        before.0, before.1, before.2
    );
    assert_eq!(before, (0, 0, 0), "a fresh profile must report all zeroes");

    for number in 1..=MESSAGE_COUNT {
        cmd_osl_persist_inbound(
            &state,
            CONVERSATION_ID.to_owned(),
            format!("task-3685-restored-message-{number:05}"),
            SENDER_ID.to_owned(),
            format!("TASK3685 restored chat message {number:05}"),
        )
        .expect("persist restored chat-history message");
    }

    {
        let store = state.message_store.lock().expect("message store mutex");
        let store = store.as_ref().expect("profile has a message store");
        for number in 1..=ATTACHMENT_COUNT {
            store
                .put_attachment(
                    &format!("task-3685-restored-message-{number:05}"),
                    &format!("task-3685-attachment-{number:05}.bin"),
                    "application/octet-stream",
                    b"task-3685-safe-attachment-record",
                    Some("osl_chat"),
                    Some(CONVERSATION_ID),
                    Some(SENDER_ID),
                )
                .expect("persist received attachment record");
        }
    }

    // set_len creates a sparse test file on normal local filesystems.  The
    // file is owned by TempDir and removed on drop, so it cannot affect a
    // user's files or consume ten GB of physical storage.
    File::create(profile.path().join(SAFE_TEST_FILE_NAME))
        .expect("create safe local test file")
        .set_len(SAFE_TEST_FILE_BYTES)
        .expect("size safe local test file");

    let after = profile_counts(&state, profile.path());
    println!(
        "TASK3685 populated_profile messages={} attachment_records={} safe_test_file_bytes={}",
        after.0, after.1, after.2
    );
    println!(
        "TASK3685 finish_line fresh=0,0,0 populated={},{},{}",
        after.0, after.1, after.2
    );
    assert_eq!(after.0, MESSAGE_COUNT);
    assert_eq!(after.1, ATTACHMENT_COUNT);
    assert_eq!(after.2, SAFE_TEST_FILE_BYTES);
}
