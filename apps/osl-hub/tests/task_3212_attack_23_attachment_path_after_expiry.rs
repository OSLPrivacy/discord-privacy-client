#![cfg(feature = "core")]

use message_lifecycle::AcceptedPart;
use osl_privacy_hub::message_expiry::{
    absolute_release, note_delivered_at_path, record_first_open_at_path,
    record_timed_attachment_artifact, run_pass, TimedAttachmentArtifactKind,
};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::path::{Path, PathBuf};
use store::{MessageStore, StoredMessage};
use tempfile::TempDir;
use uuid::Uuid;

const KEY: [u8; 32] = [0x32; 32];
const SCOPE_KEY: &str = "dm:task3212";
const FILENAME: &str = "task3212-timed-attachment.txt";
const MIME: &str = "text/plain";
const WITHOUT_TIMED_ATTACHMENT: &str = "without-timed-attachment";

struct ConfigDirGuard;

impl Drop for ConfigDirGuard {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(None);
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
    }
}

fn use_receiver_account_dir(dir: &Path) -> ConfigDirGuard {
    ipc::main_password::set_file_storage_key(None);
    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(Some(dir.to_path_buf()));
    ipc::main_password::set_file_storage_key(Some(KEY));
    ConfigDirGuard
}

#[test]
fn task_3212_attack_23_timed_attachment_path_is_unreadable_after_expiry() {
    let sender = TempDir::new().expect("sender fixture root");
    let receiver = TempDir::new().expect("receiver fixture root");
    let config_dir = receiver.path().join("config");
    let local_data_dir = receiver.path().join("temporary-folder");
    std::fs::create_dir_all(&config_dir).expect("create receiver config");
    std::fs::create_dir_all(&local_data_dir).expect("create receiver temporary folder");
    let _account = use_receiver_account_dir(&config_dir);

    let fixture_mode = std::env::var("OSL_TASK3212_FIXTURE").unwrap_or_default();
    let has_timed_attachment = fixture_mode != WITHOUT_TIMED_ATTACHMENT;
    let message_id = format!("task3212-{}", Uuid::new_v4().simple());
    let marker = format!("TASK3212-FILE-MARK-{}", Uuid::new_v4().simple());
    let mut plaintext = vec![0x5a; 16 * 1024];
    plaintext[257..257 + marker.len()].copy_from_slice(marker.as_bytes());
    let source_path = sender.path().join(FILENAME);
    std::fs::write(&source_path, &plaintext).expect("write sender attachment");

    let sent_at = ipc::main_password::now_unix_secs_pub();
    let release = absolute_release(sent_at, ipc::cipher_store_client::TTL_1H)
        .expect("one-hour timed attachment release");
    let expires_at = release.absolute_expires_at;
    let just_before_expiry = expires_at - 1;
    let ledger_path = config_dir.join("message_open_clock.json");
    let store =
        MessageStore::open(&config_dir.join("message-store"), &KEY).expect("open receiver cache");

    let mut opened_path: Option<PathBuf> = None;
    let mut opened_guard = None;
    let mut fixture_timed_attachment_count = 0usize;
    if has_timed_attachment {
        store
            .put(&StoredMessage {
                discord_message_id: message_id.clone(),
                channel_id: "task3212-channel".to_owned(),
                sender_discord_id: "task3212-sender".to_owned(),
                sender_osl_user_id: "task3212-osl-sender".to_owned(),
                plaintext: "timed attachment notice".to_owned(),
                decrypted_at: sent_at,
                reply_parent_id: None,
                edit_revision: 1,
                burned: false,
            })
            .expect("cache timed attachment notice");
        store
            .put_attachment(
                &message_id,
                FILENAME,
                MIME,
                &plaintext,
                Some("dm"),
                Some("task3212-channel"),
                Some("task3212-sender"),
            )
            .expect("cache timed attachment");

        let mut source = File::open(&source_path).expect("open sender attachment");
        let staged = osl_privacy_hub::peer_attachment_io::encrypt_file(
            &local_data_dir,
            &mut source,
            FILENAME,
            MIME,
            crypto::aead::Key::from_bytes([0x23; 32]),
            vec![0x12; 16],
            0,
        )
        .expect("send and seal timed attachment");
        let (sealed_digest, _sealed_size) =
            osl_privacy_hub::peer_attachment_io::sha256_file(staged.path())
                .expect("measure sealed timed attachment");
        note_delivered_at_path(
            &ledger_path,
            &KEY,
            SCOPE_KEY,
            &message_id,
            release,
            vec![AcceptedPart {
                index: 0,
                // The lifecycle part is the compact attachment notice, not
                // the separately transported attachment object.
                sealed_bytes: 512,
                digest: sealed_digest,
            }],
            Sha256::digest(message_id.as_bytes()).into(),
            Some(message_id.clone()),
            sent_at,
        )
        .expect("record timed attachment delivery");

        assert!(
            record_first_open_at_path(
                &ledger_path,
                &KEY,
                SCOPE_KEY,
                &message_id,
                Sha256::digest(b"task3212 authenticated open").into(),
                just_before_expiry,
            )
            .is_readable(),
            "timed attachment must be readable immediately before expiry"
        );
        let mut sealed = File::open(staged.path()).expect("open sealed receiver file");
        let opened = osl_privacy_hub::peer_attachment_io::decrypt_file(
            &local_data_dir,
            &mut sealed,
            FILENAME,
            MIME,
            crypto::aead::Key::from_bytes([0x23; 32]),
        )
        .expect("open timed attachment just before expiry");
        osl_privacy_hub::peer_attachment_io::remove_staged_file(staged)
            .expect("remove received sealed staging file");
        let path = opened
            .path()
            .expect("opened attachment has a path")
            .to_owned();
        record_timed_attachment_artifact(
            &local_data_dir,
            &path,
            TimedAttachmentArtifactKind::UnlockedCopy,
            expires_at,
        )
        .expect("bind opened path to timed attachment expiry");
        opened_path = Some(path);
        opened_guard = Some(opened);
        fixture_timed_attachment_count = 1;
    }

    assert_eq!(
        fixture_timed_attachment_count, 1,
        "TASK3212 fixture has no timed attachment"
    );
    let opened_path = opened_path.expect("timed fixture records the opened path");
    let before_expiry_read_count = usize::from(
        std::fs::read(&opened_path).is_ok_and(|bytes| bytes == plaintext)
            && store
                .get_attachment(&message_id, FILENAME)
                .expect("read attachment cache before expiry")
                .is_some_and(|(_, bytes)| bytes == plaintext),
    );
    assert_eq!(before_expiry_read_count, 1);
    println!("TASK3212_BEFORE_EXPIRY_READ_COUNT={before_expiry_read_count}");
    println!("TASK3212_RECORDED_FILE_PATH={}", opened_path.display());

    let expiry = run_pass(&local_data_dir, Some(&store), expires_at);
    assert!(expiry.ran);
    assert!(!expiry.degraded);
    assert_eq!(expiry.expired_messages, 1);
    assert_eq!(expiry.shredded_cache_rows, 1);
    assert_eq!(expiry.removed_timed_attachment_artifacts, 1);
    assert_eq!(expiry.removed_staging_files, 1);

    let at_place_file_count = usize::from(opened_path.is_file());
    let at_place_preview_count = usize::from(opened_path.with_extension("preview").is_file());
    let at_place_small_picture_count =
        usize::from(opened_path.with_extension("thumbnail").is_file());
    let at_place_unlocked_copy_count = usize::from(
        std::fs::read(&opened_path).is_ok_and(|bytes| contains_bytes(&bytes, marker.as_bytes())),
    );
    let temp_file_count = staging_file_count(&local_data_dir);
    let temp_preview_count = named_file_count(&local_data_dir, "preview");
    let temp_small_picture_count = named_file_count(&local_data_dir, "thumbnail");
    let temp_unlocked_copy_count = marker_file_count(&local_data_dir, marker.as_bytes());
    let cached_attachment_count = usize::from(
        store
            .get_attachment(&message_id, FILENAME)
            .expect("read attachment cache after expiry")
            .is_some(),
    );

    for count in [
        at_place_file_count,
        at_place_preview_count,
        at_place_small_picture_count,
        at_place_unlocked_copy_count,
        temp_file_count,
        temp_preview_count,
        temp_small_picture_count,
        temp_unlocked_copy_count,
        cached_attachment_count,
    ] {
        assert_eq!(count, 0);
    }
    assert!(
        store
            .get(&message_id)
            .expect("read message after expiry")
            .is_none(),
        "the expired attachment notice must be shredded with its file"
    );

    println!(
        "TASK3212_AFTER_AT_PLACE file_count={at_place_file_count} preview_count={at_place_preview_count} small_picture_count={at_place_small_picture_count} unlocked_copy_count={at_place_unlocked_copy_count}"
    );
    println!(
        "TASK3212_AFTER_TEMP_FOLDER file_count={temp_file_count} preview_count={temp_preview_count} small_picture_count={temp_small_picture_count} unlocked_copy_count={temp_unlocked_copy_count}"
    );
    println!("TASK3212_AFTER_CACHE_ATTACHMENT_COUNT={cached_attachment_count}");
    println!(
        "TASK3212_EXPIRY expired_messages={} shredded_cache_rows={} removed_timed_attachment_artifacts={}",
        expiry.expired_messages,
        expiry.shredded_cache_rows,
        expiry.removed_timed_attachment_artifacts
    );

    // The lifecycle pass removed the file behind the guard. Dropping it must
    // remain idempotent and must not recreate an unlocked copy.
    drop(opened_guard.take());
    assert_eq!(marker_file_count(&local_data_dir, marker.as_bytes()), 0);
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack.len() >= needle.len()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

fn walk_files(root: &Path) -> Vec<PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(path) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => pending.push(path),
                Ok(kind) if kind.is_file() => files.push(path),
                _ => {}
            }
        }
    }
    files
}

fn staging_file_count(root: &Path) -> usize {
    walk_files(&root.join("peer-attachment-staging"))
        .into_iter()
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| matches!(extension, "oslatt" | "part"))
        })
        .count()
}

fn named_file_count(root: &Path, fragment: &str) -> usize {
    walk_files(root)
        .into_iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.contains(fragment))
        })
        .count()
}

fn marker_file_count(root: &Path, marker: &[u8]) -> usize {
    walk_files(root)
        .into_iter()
        .filter(|path| std::fs::read(path).is_ok_and(|bytes| contains_bytes(&bytes, marker)))
        .count()
}
