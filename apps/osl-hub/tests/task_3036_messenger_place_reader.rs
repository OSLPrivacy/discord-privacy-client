#![cfg(feature = "core")]

use osl_privacy_hub::messenger_place_reader::{MessengerBrowserConversation, MessengerPlaceKind};
use osl_privacy_hub::services::{
    read_messenger_conversation_places, save_messaging_risk_agreement, ConversationPlaceKind,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const PASSWORD: &str = "task-3036-messenger-place-reader";

struct AccountStorage {
    root: PathBuf,
}

impl AccountStorage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-task-3036-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock after epoch")
                .as_nanos()
        ));
        fs::create_dir(&root).expect("create isolated root");
        keystore::set_base_dir_override(Some(root.clone()));
        keystore::set_active_account_dir(None);
        ipc::main_password::set_file_storage_key(None);
        ipc::main_password::set_main_password(&root, PASSWORD).expect("set main password");
        Self { root }
    }
}

impl Drop for AccountStorage {
    fn drop(&mut self) {
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
        ipc::main_password::set_file_storage_key(None);
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn activate(dir: &Path) {
    keystore::set_active_account_dir(Some(dir.to_owned()));
}

fn seeded_browser_rows() -> Vec<MessengerBrowserConversation> {
    vec![
        MessengerBrowserConversation::new("dm-scrub-m", "SCRUB-M", MessengerPlaceKind::DirectChat),
        MessengerBrowserConversation::new(
            "group-scrub-m",
            "SCRUB-M group",
            MessengerPlaceKind::GroupChat,
        ),
        MessengerBrowserConversation::new(
            "community-scrub-m",
            "SCRUB-M community",
            MessengerPlaceKind::Community,
        ),
    ]
}

#[test]
fn task_3036_messenger_shared_reader_returns_three_seeded_places_only_when_ticked() {
    let storage = AccountStorage::new();
    let account_dir = storage.root.join("owner");
    fs::create_dir(&account_dir).expect("create account directory");
    activate(&account_dir);

    let owner = keystore::generate_identity("task-3036-owner".to_owned())
        .user_id
        .clone();
    let ticked_account = "messenger-ticked";
    let unticked_account = "messenger-unticked";
    save_messaging_risk_agreement(&owner, "messenger", ticked_account)
        .expect("tick Messenger account");

    let places = read_messenger_conversation_places(&owner, ticked_account, seeded_browser_rows())
        .expect("read ticked Messenger browser rows");
    let unticked =
        read_messenger_conversation_places(&owner, unticked_account, seeded_browser_rows())
            .expect("unticked account returns empty places");

    println!("TASK3036_TICKED_PLACE_COUNT={}", places.len());
    for place in &places {
        println!(
            "TASK3036_PLACE label={} kind={}",
            place.label,
            place.place_kind.as_str()
        );
    }
    println!("TASK3036_UNTICKED_PLACE_COUNT={}", unticked.len());

    assert_eq!(places.len(), 3);
    assert!(places.iter().any(|place| {
        place.label == "SCRUB-M" && place.place_kind == ConversationPlaceKind::DirectMessage
    }));
    assert!(places
        .iter()
        .any(|place| place.place_kind == ConversationPlaceKind::Group));
    assert!(places
        .iter()
        .any(|place| place.place_kind == ConversationPlaceKind::Community));
    assert!(unticked.is_empty());
}
