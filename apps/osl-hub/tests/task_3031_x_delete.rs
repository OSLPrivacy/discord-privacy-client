#![cfg(feature = "core")]

use osl_privacy_hub::services::{
    read_x_shared_messages, save_messaging_risk_agreement, XBrowserMachine, XBrowserMessage,
    XBrowserPlace, XBrowserPlaceKind,
};
use osl_privacy_hub::x_marked_message_deleter::{
    delete_marked_x_message, delete_marked_x_messages_and_record, XReviewSelection,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const PASSWORD: &str = "task-3031-x-delete-password";
const ACCOUNT: &str = "x-task-3031";
const DM: &str = "x-dm-task-3031";
const POST: &str = "x-post-task-3031";
const MARKER: &str = "SCRUB-X-DEL";

struct Storage(PathBuf);

impl Storage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-task-3031-{}-{}",
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

fn seeded_browser() -> XBrowserMachine {
    XBrowserMachine::new([
        XBrowserPlace::new(
            DM,
            "Task 3031 direct messages",
            XBrowserPlaceKind::DirectMessage,
        ),
        XBrowserPlace::new(
            POST,
            "Task 3031 own posts",
            XBrowserPlaceKind::OwnPostOrReply,
        ),
    ])
    .with_messages([
        // Exactly three rows match SCRUB-X-DEL; only `dm-marked` is marked.
        XBrowserMessage::new(DM, "dm-marked", "SCRUB-X-DEL direct marked", 100, true),
        XBrowserMessage::new(DM, "dm-kept", "SCRUB-X-DEL direct kept", 200, true),
        XBrowserMessage::new(POST, "own-post", "SCRUB-X-DEL own post", 300, true),
        // Provider authorship is false, so the shared owner check must refuse it.
        XBrowserMessage::new(POST, "other-post", "OTHER-ACCOUNT-POST", 400, false),
    ])
}

fn matching_count(owner: &str, browser: &XBrowserMachine, marker: &str) -> usize {
    [DM, POST]
        .into_iter()
        .map(|place_id| {
            read_x_shared_messages(owner, ACCOUNT, place_id, browser)
                .expect("read X items through gate 3029")
                .into_iter()
                .filter(|message| message.text.contains(marker))
                .count()
        })
        .sum()
}

#[test]
fn task_3031_deletes_only_the_marked_x_item_and_refuses_someone_elses_post() {
    let storage = Storage::new();
    keystore::set_active_account_dir(Some(storage.owner_dir()));
    let owner = keystore::generate_identity("task-3031-owner".to_owned())
        .user_id
        .clone();
    save_messaging_risk_agreement(&owner, "x", ACCOUNT).expect("tick the X account");

    let mut browser = seeded_browser();
    let before = matching_count(&owner, &browser, MARKER);
    let record = delete_marked_x_messages_and_record(
        &mut browser,
        [XReviewSelection::marked(DM, "dm-marked")],
    )
    .expect("resolve the marked X row from the gate 3029 snapshot");
    let after = matching_count(&owner, &browser, MARKER);

    assert_eq!(before, 3);
    assert_eq!(after, 2);
    assert_eq!(record.deleted_count, 1);
    assert_eq!(record.failed_count, 0);
    assert_eq!(record.needs_attention_count, 0);

    // Exercise own-post deletion through the same service adapter after the
    // required three-to-two observation.
    delete_marked_x_message(&mut browser, &XReviewSelection::marked(POST, "own-post"))
        .expect("delete the signed-in account's own X post");

    let rows_before_refusal = browser.messages.len();
    let refused =
        delete_marked_x_message(&mut browser, &XReviewSelection::marked(POST, "other-post"))
            .expect_err("someone else's X post must be refused");
    assert_eq!(refused.code(), "not_yours");
    assert_eq!(browser.messages.len(), rows_before_refusal);
    assert!(browser
        .messages
        .iter()
        .any(|message| message.message_id == "other-post"));

    println!("TASK3031_SCRUB_X_DEL_COUNT_BEFORE={before}");
    println!("TASK3031_SCRUB_X_DEL_COUNT_AFTER={after}");
    println!("TASK3031_RECORDED_DELETED_COUNT={}", record.deleted_count);
    println!("TASK3031_SUPPORTED_KINDS=direct_message,public_post");
    println!("TASK3031_OTHER_POST_ERROR={}", refused.code());
}
