#![cfg(feature = "core")]

use osl_privacy_hub::services::{
    read_x_shared_messages, save_messaging_risk_agreement, XBrowserMachine, XBrowserMessage,
    XBrowserPlace, XBrowserPlaceKind,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const PASSWORD: &str = "task-3029-x-shared-message-reader-password";
const DIRECT_MESSAGE_PLACE: &str = "x-dm-scrub-x";
const PUBLIC_POST_PLACE: &str = "x-public-scrub-x";

struct Storage(PathBuf);

impl Storage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-task-3029-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time after unix epoch")
                .as_nanos()
        ));
        fs::create_dir(&root).expect("create isolated root");
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

fn browser_machine() -> XBrowserMachine {
    XBrowserMachine::new([
        XBrowserPlace::new(
            DIRECT_MESSAGE_PLACE,
            "SCRUB-X",
            XBrowserPlaceKind::DirectMessage,
        ),
        XBrowserPlace::new(
            PUBLIC_POST_PLACE,
            "SCRUB-X-POST",
            XBrowserPlaceKind::OwnPostOrReply,
        ),
    ])
    // Deliberately provider-order rows: the reader must order by time.
    .with_messages([
        XBrowserMessage::new(
            DIRECT_MESSAGE_PLACE,
            "third",
            "SCRUB-X-THEIRS-2",
            300,
            false,
        ),
        XBrowserMessage::new(
            DIRECT_MESSAGE_PLACE,
            "first",
            "SCRUB-X-THEIRS-1",
            100,
            false,
        ),
        XBrowserMessage::new(DIRECT_MESSAGE_PLACE, "second", "SCRUB-X-MINE", 200, true),
        XBrowserMessage::new(PUBLIC_POST_PLACE, "own-post", "SCRUB-X-POST", 400, true),
    ])
}

#[test]
fn task_3029_x_messages_are_time_ordered_and_preserve_provider_authorship() {
    let storage = Storage::new();
    keystore::set_active_account_dir(Some(storage.owner_dir()));
    let owner = keystore::generate_identity("task-3029-owner".to_owned())
        .user_id
        .clone();
    let account = "x-ticked";
    save_messaging_risk_agreement(&owner, "x", account).expect("tick X account");

    let browser = browser_machine();
    let direct_messages = read_x_shared_messages(&owner, account, DIRECT_MESSAGE_PLACE, &browser)
        .expect("read SCRUB-X direct messages");
    let public_post = read_x_shared_messages(&owner, account, PUBLIC_POST_PLACE, &browser)
        .expect("read SCRUB-X own public post");

    println!("TASK3029_SCRUB_X_MESSAGE_COUNT={}", direct_messages.len());
    for message in &direct_messages {
        println!(
            "TASK3029_SCRUB_X_MESSAGE time={} text={} yours={}",
            message.time, message.text, message.yours
        );
    }
    println!("TASK3029_SCRUB_X_POST_COUNT={}", public_post.len());
    for message in &public_post {
        println!(
            "TASK3029_SCRUB_X_POST time={} text={} yours={}",
            message.time, message.text, message.yours
        );
    }

    assert_eq!(direct_messages.len(), 3);
    assert_eq!(
        direct_messages
            .iter()
            .map(|message| (message.time, message.text.as_str(), message.yours))
            .collect::<Vec<_>>(),
        [
            (100, "SCRUB-X-THEIRS-1", false),
            (200, "SCRUB-X-MINE", true),
            (300, "SCRUB-X-THEIRS-2", false),
        ]
    );
    assert_eq!(
        public_post
            .iter()
            .map(|message| (message.text.as_str(), message.yours))
            .collect::<Vec<_>>(),
        [("SCRUB-X-POST", true)]
    );
}
