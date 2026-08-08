//! TASK 3611: a real staged-build update must preserve old private messages,
//! attachments, timers, and the sender's burn-right answers exactly.

use std::fs;
use std::path::Path;

use osl_privacy_hub::update_apply::{
    apply_staged_build_update_at, mark_update_finished_after_successful_start_at, UpdateApplyStatus,
};
use sha2::{Digest, Sha256};
use store::{MessageStore, StoredMessage};
use tempfile::TempDir;

const PROFILE_ID: &str = "task-3611-profile-owner";
const CHANNEL_ID: &str = "task-3611-private-channel";
const PREVIOUS_VERSION: &str = "3611.0.0";
const APPLIED_VERSION: &str = "3611.0.1";
const STORE_SECRET: &[u8; 32] = &[0x36; 32];

const MESSAGES: [(&str, &str, &str, u32); 3] = [
    (
        "task-3611-marked-message-1",
        "TASK3611-MARK private text alpha",
        PROFILE_ID,
        15,
    ),
    (
        "task-3611-marked-message-2",
        "TASK3611-MARK private text beta",
        "task-3611-other-sender",
        60,
    ),
    (
        "task-3611-marked-message-3",
        "TASK3611-MARK private text gamma",
        PROFILE_ID,
        1_440,
    ),
];

const ATTACHMENTS: [(&str, &str, &[u8]); 2] = [
    (
        "task-3611-marked-message-1",
        "TASK3611-MARK-private-photo.bin",
        b"TASK3611-MARK\x00private attachment bytes one\xff",
    ),
    (
        "task-3611-marked-message-2",
        "TASK3611-MARK-private-document.bin",
        b"TASK3611-MARK\x00private attachment bytes two\xfe",
    ),
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct MessagePrivateSnapshot {
    id: String,
    plaintext_sha256: String,
    timer_minutes: u32,
    sender_can_burn: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AttachmentPrivateSnapshot {
    filename: String,
    bytes_sha256: String,
    timer_minutes: u32,
    sender_can_burn: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PrivateProfileSnapshot {
    messages: Vec<MessagePrivateSnapshot>,
    attachments: Vec<AttachmentPrivateSnapshot>,
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Burn authority is answered from authenticated message authorship: the
/// profile may burn content sent by this profile, never another sender's.
fn sender_can_burn(message: &StoredMessage) -> bool {
    message.sender_osl_user_id == PROFILE_ID
}

fn snapshot(store: &MessageStore) -> PrivateProfileSnapshot {
    let messages = MESSAGES
        .iter()
        .map(|(id, _, _, _)| {
            let message = store
                .get(id)
                .unwrap_or_else(|error| panic!("read marked message {id}: {error}"))
                .unwrap_or_else(|| panic!("marked message {id} is missing"));
            let timer_minutes = store
                .message_timer_minutes(id)
                .unwrap_or_else(|error| panic!("read timer for {id}: {error}"))
                .unwrap_or_else(|| panic!("timer for {id} is missing"));
            MessagePrivateSnapshot {
                id: (*id).to_owned(),
                plaintext_sha256: sha256(message.plaintext.as_bytes()),
                timer_minutes,
                sender_can_burn: sender_can_burn(&message),
            }
        })
        .collect();

    let attachments = ATTACHMENTS
        .iter()
        .map(|(message_id, filename, _)| {
            let parent = store
                .get(message_id)
                .unwrap_or_else(|error| panic!("read attachment parent {message_id}: {error}"))
                .unwrap_or_else(|| panic!("attachment parent {message_id} is missing"));
            let (_, bytes) = store
                .get_attachment(message_id, filename)
                .unwrap_or_else(|error| panic!("read marked attachment {filename}: {error}"))
                .unwrap_or_else(|| panic!("marked attachment {filename} is missing"));
            let timer_minutes = store
                .message_timer_minutes(message_id)
                .unwrap_or_else(|error| panic!("read attachment timer for {filename}: {error}"))
                .unwrap_or_else(|| panic!("attachment timer for {filename} is missing"));
            AttachmentPrivateSnapshot {
                filename: (*filename).to_owned(),
                bytes_sha256: sha256(&bytes),
                timer_minutes,
                sender_can_burn: sender_can_burn(&parent),
            }
        })
        .collect();

    PrivateProfileSnapshot {
        messages,
        attachments,
    }
}

fn assert_preserved(before: &PrivateProfileSnapshot, after: &PrivateProfileSnapshot) {
    assert_eq!(
        before.messages.len(),
        3,
        "TASK3611 expected 3 marked messages before update"
    );
    assert_eq!(
        after.messages.len(),
        3,
        "TASK3611 expected 3 marked messages after update"
    );
    assert_eq!(
        before.attachments.len(),
        2,
        "TASK3611 expected 2 marked attachments before update"
    );
    assert_eq!(
        after.attachments.len(),
        2,
        "TASK3611 expected 2 marked attachments after update"
    );

    for (before_message, after_message) in before.messages.iter().zip(&after.messages) {
        assert_eq!(
            before_message.id, after_message.id,
            "TASK3611 marked message order changed"
        );
        assert_eq!(
            before_message.plaintext_sha256, after_message.plaintext_sha256,
            "TASK3611 private message fingerprint changed: id={}",
            before_message.id
        );
        assert_eq!(
            before_message.timer_minutes, after_message.timer_minutes,
            "TASK3611 message timer changed: id={}",
            before_message.id
        );
        assert_eq!(
            before_message.sender_can_burn, after_message.sender_can_burn,
            "TASK3611 message burn-right answer changed: id={}",
            before_message.id
        );
    }

    for (before_attachment, after_attachment) in before.attachments.iter().zip(&after.attachments) {
        assert_eq!(
            before_attachment.filename, after_attachment.filename,
            "TASK3611 marked attachment order changed"
        );
        assert_eq!(
            before_attachment.bytes_sha256, after_attachment.bytes_sha256,
            "TASK3611 attachment fingerprint changed: filename={}",
            before_attachment.filename
        );
        assert_eq!(
            before_attachment.timer_minutes, after_attachment.timer_minutes,
            "TASK3611 attachment timer changed: filename={}",
            before_attachment.filename
        );
        assert_eq!(
            before_attachment.sender_can_burn, after_attachment.sender_can_burn,
            "TASK3611 attachment burn-right answer changed: filename={}",
            before_attachment.filename
        );
    }
}

fn seed_private_profile(store_dir: &Path) {
    let store = MessageStore::open(store_dir, STORE_SECRET).expect("open old private profile");
    for (index, (id, plaintext, sender, timer_minutes)) in MESSAGES.iter().enumerate() {
        store
            .put(&StoredMessage {
                discord_message_id: (*id).to_owned(),
                channel_id: CHANNEL_ID.to_owned(),
                sender_discord_id: format!("task-3611-discord-sender-{index}"),
                sender_osl_user_id: (*sender).to_owned(),
                plaintext: (*plaintext).to_owned(),
                decrypted_at: 1_800_003_611 + index as i64,
                reply_parent_id: None,
                edit_revision: 1,
                burned: false,
            })
            .expect("write marked private message");
        store
            .record_message_timer_minutes(id, *timer_minutes)
            .expect("write marked message timer");
    }
    for (message_id, filename, bytes) in ATTACHMENTS {
        let parent = store
            .get(message_id)
            .expect("read attachment parent")
            .expect("attachment parent exists");
        store
            .put_attachment(
                message_id,
                filename,
                "application/octet-stream",
                bytes,
                Some("dm"),
                Some(CHANNEL_ID),
                Some(&parent.sender_discord_id),
            )
            .expect("write marked private attachment");
    }
}

fn print_snapshot(stage: &str, snapshot: &PrivateProfileSnapshot) {
    println!(
        "TASK3611 {stage} marked_messages={} marked_attachments={}",
        snapshot.messages.len(),
        snapshot.attachments.len()
    );
    for message in &snapshot.messages {
        println!(
            "TASK3611 {stage} message={} private_text_sha256={} timer_minutes={} sender_can_burn={}",
            message.id,
            message.plaintext_sha256,
            message.timer_minutes,
            message.sender_can_burn
        );
    }
    for attachment in &snapshot.attachments {
        println!(
            "TASK3611 {stage} attachment={} private_bytes_sha256={} timer_minutes={} sender_can_burn={}",
            attachment.filename,
            attachment.bytes_sha256,
            attachment.timer_minutes,
            attachment.sender_can_burn
        );
    }
}

#[test]
fn task_3611_real_update_preserves_old_private_data_exactly() {
    let root = TempDir::new().expect("temporary update fixture");
    let install = root.path().join("install");
    let staged = root.path().join("staged");
    let config = root.path().join("config");
    let core = config.join("osl-core");
    let store_dir = core.join("store");
    fs::create_dir_all(&install).expect("create old install");
    fs::create_dir_all(&staged).expect("create staged update");
    fs::create_dir_all(&core).expect("create profile directory");
    fs::write(install.join("OSL Privacy.exe"), b"task 3611 old build")
        .expect("write old built file");
    fs::write(staged.join("OSL Privacy.exe"), b"task 3611 new build")
        .expect("write staged built file");
    fs::write(
        core.join("identity.json"),
        b"task 3611 sealed identity marker",
    )
    .expect("write profile identity marker");

    seed_private_profile(&store_dir);
    let before_store = MessageStore::open(&store_dir, STORE_SECRET).expect("reopen before update");
    let before = snapshot(&before_store);
    drop(before_store);
    assert_eq!(
        before
            .messages
            .iter()
            .map(|row| row.timer_minutes)
            .collect::<Vec<_>>(),
        vec![15, 60, 1_440]
    );
    assert_eq!(
        before
            .messages
            .iter()
            .map(|row| row.sender_can_burn)
            .collect::<Vec<_>>(),
        vec![true, false, true]
    );
    assert_eq!(
        before
            .attachments
            .iter()
            .map(|row| row.sender_can_burn)
            .collect::<Vec<_>>(),
        vec![true, false]
    );
    print_snapshot("before", &before);

    let applied = apply_staged_build_update_at(
        &install,
        &staged,
        &config,
        PREVIOUS_VERSION,
        APPLIED_VERSION,
        1_800_003_611,
    )
    .expect("apply real staged-build update");
    assert_eq!(applied.status, UpdateApplyStatus::PendingRestart);
    assert_eq!(applied.previous_version, PREVIOUS_VERSION);
    assert_eq!(applied.applied_version, APPLIED_VERSION);
    assert_ne!(applied.previous_version, applied.applied_version);
    assert_eq!(
        applied.message_history_sha256_before, applied.message_history_sha256_after_apply,
        "real updater changed the protected store file"
    );

    let finished =
        mark_update_finished_after_successful_start_at(&config, APPLIED_VERSION, 1_800_003_612)
            .expect("record replacement build start")
            .expect("update apply record exists");
    assert_eq!(finished.status, UpdateApplyStatus::Finished);
    assert_eq!(finished.successful_start_count, 1);

    let after_store = MessageStore::open(&store_dir, STORE_SECRET).expect("open updated profile");
    let after = snapshot(&after_store);
    print_snapshot("after", &after);
    assert_preserved(&before, &after);

    println!("TASK3611 version_before={PREVIOUS_VERSION} version_after={APPLIED_VERSION}");
    println!(
        "TASK3611 changed_values=0 update_status={:?}",
        finished.status
    );
}
