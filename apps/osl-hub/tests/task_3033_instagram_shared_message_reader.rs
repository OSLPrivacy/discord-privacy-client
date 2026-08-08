#![cfg(feature = "core")]

use osl_privacy_hub::services::{
    read_instagram_shared_messages, save_messaging_risk_agreement, InstagramBrowserMachine,
    InstagramBrowserMessage, InstagramBrowserPlace, InstagramBrowserPlaceKind,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const PASSWORD: &str = "task-3033-instagram-shared-message-reader-password";
const PLACE: &str = "instagram-dm-scrub-i";

struct Storage(PathBuf);

impl Storage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-task-3033-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
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

fn browser_machine() -> InstagramBrowserMachine {
    InstagramBrowserMachine::new([InstagramBrowserPlace::new(
        PLACE,
        "SCRUB-I",
        InstagramBrowserPlaceKind::DirectMessage,
    )])
    // Deliberately provider-order rows: the reader must order by time.
    .with_messages([
        InstagramBrowserMessage::new(PLACE, "third", "SCRUB-I-THEIRS-2", 300, false),
        InstagramBrowserMessage::new(PLACE, "first", "SCRUB-I-THEIRS-1", 100, false),
        InstagramBrowserMessage::new(PLACE, "second", "SCRUB-I-MINE", 200, true),
    ])
}

#[test]
fn task_3033_instagram_messages_are_ordered_owned_and_silent_when_unticked() {
    let storage = Storage::new();
    keystore::set_active_account_dir(Some(storage.owner_dir()));
    let owner = keystore::generate_identity("task-3033-owner".to_owned())
        .user_id
        .clone();
    save_messaging_risk_agreement(&owner, "instagram", "instagram-ticked")
        .expect("tick Instagram account");

    let browser = browser_machine();
    let read = read_instagram_shared_messages(&owner, "instagram-ticked", PLACE, &browser)
        .expect("read ticked Instagram messages");
    let unticked = read_instagram_shared_messages(
        &owner,
        "instagram-ticked",
        "instagram-dm-not-ticked",
        &browser,
    )
    .expect("unticked Instagram place remains silent");

    println!("TASK3033_TICKED_MESSAGE_COUNT={}", read.len());
    for message in &read {
        println!(
            "TASK3033_MESSAGE time={} text={} yours={}",
            message.time, message.text, message.yours
        );
    }
    println!("TASK3033_UNTICKED_PLACE_MESSAGE_COUNT={}", unticked.len());

    assert_eq!(read.len(), 3);
    assert_eq!(
        read.iter()
            .map(|message| (message.time, message.text.as_str(), message.yours))
            .collect::<Vec<_>>(),
        [
            (100, "SCRUB-I-THEIRS-1", false),
            (200, "SCRUB-I-MINE", true),
            (300, "SCRUB-I-THEIRS-2", false)
        ]
    );
    assert!(unticked.is_empty());
}
