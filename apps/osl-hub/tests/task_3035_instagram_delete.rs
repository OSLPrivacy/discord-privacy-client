#![cfg(feature = "core")]

use osl_privacy_hub::instagram_marked_message_deleter::{
    delete_marked_instagram_message, delete_marked_instagram_messages_and_record,
    InstagramReviewSelection,
};
use osl_privacy_hub::services::{
    read_instagram_shared_messages, save_messaging_risk_agreement, InstagramBrowserMachine,
    InstagramBrowserMessage, InstagramBrowserPlace, InstagramBrowserPlaceKind,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const PASSWORD: &str = "task-3035-instagram-delete-password";
const ACCOUNT: &str = "instagram-task-3035";
const DM: &str = "instagram-dm-task-3035";
const POST: &str = "instagram-post-task-3035";
const COMMENT: &str = "instagram-comment-task-3035";
const MARKER: &str = "SCRUB-I-DEL";

struct Storage(PathBuf);

impl Storage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-task-3035-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time after epoch")
                .as_nanos()
        ));
        fs::create_dir(&root).expect("create isolated task root");
        keystore::set_base_dir_override(Some(root.clone()));
        keystore::set_active_account_dir(None);
        ipc::main_password::set_file_storage_key(None);
        ipc::main_password::set_main_password(&root, PASSWORD).expect("set main password");
        Self(root)
    }

    fn owner_dir(&self) -> PathBuf {
        let path = self.0.join("owner");
        fs::create_dir(&path).expect("create owner directory");
        path
    }
}

impl Drop for Storage {
    fn drop(&mut self) {
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
        ipc::main_password::set_file_storage_key(None);
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn seeded_browser() -> InstagramBrowserMachine {
    InstagramBrowserMachine::new([
        InstagramBrowserPlace::new(
            DM,
            "Task 3035 direct message",
            InstagramBrowserPlaceKind::DirectMessage,
        ),
        InstagramBrowserPlace::new(
            POST,
            "Task 3035 own post",
            InstagramBrowserPlaceKind::OwnPost,
        ),
        InstagramBrowserPlace::new(
            COMMENT,
            "Task 3035 own comment",
            InstagramBrowserPlaceKind::OwnComment,
        ),
    ])
    .with_messages([
        // Exactly three rows match SCRUB-I-DEL, one in each required kind.
        // Only `dm-marked` is initially supplied as a confirmed selection.
        InstagramBrowserMessage::new(DM, "dm-marked", "SCRUB-I-DEL direct", 200, true),
        InstagramBrowserMessage::new(POST, "own-post", "SCRUB-I-DEL post", 400, true),
        InstagramBrowserMessage::new(COMMENT, "own-comment", "SCRUB-I-DEL comment", 500, true),
        // A comment by somebody else on the signed-in account's own post.
        InstagramBrowserMessage::new(POST, "other-comment", "OTHER-COMMENT-STAYS", 600, false),
    ])
}

fn matching_count(owner: &str, browser: &InstagramBrowserMachine, marker: &str) -> usize {
    [DM, POST, COMMENT]
        .into_iter()
        .map(|place_id| {
            read_instagram_shared_messages(owner, ACCOUNT, place_id, browser)
                .expect("read Instagram items through gate 3033")
                .into_iter()
                .filter(|message| message.text.contains(marker))
                .count()
        })
        .sum()
}

#[test]
fn task_3035_deletes_only_the_marked_instagram_item_and_refuses_another_accounts_comment() {
    let storage = Storage::new();
    keystore::set_active_account_dir(Some(storage.owner_dir()));
    let owner = keystore::generate_identity("task-3035-owner".to_owned())
        .user_id
        .clone();
    save_messaging_risk_agreement(&owner, "instagram", ACCOUNT)
        .expect("tick the Instagram account");

    let mut browser = seeded_browser();
    let before = matching_count(&owner, &browser, MARKER);
    let record = delete_marked_instagram_messages_and_record(
        &mut browser,
        [InstagramReviewSelection::marked(DM, "dm-marked")],
    )
    .expect("resolve the marked row from the Instagram reader snapshot");
    let after = matching_count(&owner, &browser, MARKER);

    assert_eq!(before, 3);
    assert_eq!(after, 2);
    assert_eq!(record.deleted_count, 1);
    assert_eq!(record.failed_count, 0);
    assert_eq!(record.needs_attention_count, 0);

    // Exercise the other two in-scope surfaces through the same shared
    // deleter fill-in, after the required three-to-two observation above.
    delete_marked_instagram_message(
        &mut browser,
        &InstagramReviewSelection::marked(POST, "own-post"),
    )
    .expect("delete the signed-in account's own post");
    delete_marked_instagram_message(
        &mut browser,
        &InstagramReviewSelection::marked(COMMENT, "own-comment"),
    )
    .expect("delete the signed-in account's own comment");

    let rows_before_refusal = browser.messages.len();
    let refused = delete_marked_instagram_message(
        &mut browser,
        &InstagramReviewSelection::marked(POST, "other-comment"),
    )
    .expect_err("another account's comment must be refused");
    assert_eq!(refused.code(), "not_yours");
    assert_eq!(browser.messages.len(), rows_before_refusal);
    assert!(browser
        .messages
        .iter()
        .any(|message| message.message_id == "other-comment"));

    println!("TASK3035_SCRUB_I_DEL_COUNT_BEFORE={before}");
    println!("TASK3035_SCRUB_I_DEL_COUNT_AFTER={after}");
    println!("TASK3035_RECORDED_DELETED_COUNT={}", record.deleted_count);
    println!("TASK3035_SUPPORTED_KINDS=direct_message,public_post,comment");
    println!("TASK3035_OTHER_COMMENT_ERROR={}", refused.code());
}
