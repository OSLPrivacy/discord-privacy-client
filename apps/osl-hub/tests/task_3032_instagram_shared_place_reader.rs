#![cfg(feature = "core")]

use osl_privacy_hub::services::{
    read_instagram_shared_places, save_messaging_risk_agreement, ConversationPlaceKind,
    InstagramBrowserMachine, InstagramBrowserPlace, InstagramBrowserPlaceKind,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const TEST_MAIN_PASSWORD: &str = "task-3032-instagram-shared-place-reader-password";

struct AccountStorage {
    root: PathBuf,
}

impl AccountStorage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-task-3032-instagram-reader-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time after unix epoch")
                .as_nanos()
        ));
        fs::create_dir(&root).expect("create isolated task 3032 root");
        keystore::set_base_dir_override(Some(root.clone()));
        keystore::set_active_account_dir(None);
        ipc::main_password::set_file_storage_key(None);
        ipc::main_password::set_main_password(&root, TEST_MAIN_PASSWORD)
            .expect("set isolated main password");
        Self { root }
    }

    fn account_dir(&self) -> PathBuf {
        let dir = self.root.join("owner");
        fs::create_dir(&dir).expect("create isolated owner directory");
        dir
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

fn instagram_browser_machine() -> InstagramBrowserMachine {
    InstagramBrowserMachine::new([
        InstagramBrowserPlace::new(
            "instagram-dm-scrub-i",
            "SCRUB-I",
            InstagramBrowserPlaceKind::DirectMessage,
        ),
        InstagramBrowserPlace::new(
            "instagram-group-scrub-i",
            "SCRUB-I group",
            InstagramBrowserPlaceKind::GroupDirectMessage,
        ),
        InstagramBrowserPlace::new(
            "instagram-post-scrub-i",
            "SCRUB-I post",
            InstagramBrowserPlaceKind::OwnPost,
        ),
        InstagramBrowserPlace::new(
            "instagram-comment-scrub-i",
            "SCRUB-I comment",
            InstagramBrowserPlaceKind::OwnComment,
        ),
    ])
}

#[test]
fn task_3032_instagram_browser_machine_returns_four_ticked_places_and_none_unticked() {
    let storage = AccountStorage::new();
    let owner_dir = storage.account_dir();
    keystore::set_active_account_dir(Some(owner_dir.clone()));

    let owner = keystore::generate_identity("task-3032-owner".to_owned())
        .user_id
        .clone();
    let ticked_account = "instagram-ticked";
    let unticked_account = "instagram-unticked";
    save_messaging_risk_agreement(&owner, "instagram", ticked_account)
        .expect("tick Instagram account");

    let browser_machine = instagram_browser_machine();
    let ticked_places = read_instagram_shared_places(&owner, ticked_account, &browser_machine)
        .expect("read ticked Instagram browser machine");
    let unticked_places = read_instagram_shared_places(&owner, unticked_account, &browser_machine)
        .expect("read unticked Instagram browser machine");

    println!("TASK3032_BROWSER_MACHINE=instagram");
    println!("TASK3032_TICKED_PLACE_COUNT={}", ticked_places.len());
    for place in &ticked_places {
        println!(
            "TASK3032_PLACE id={} name={} kind={}",
            place.place_id,
            place.label,
            place.place_kind.as_str()
        );
    }
    println!("TASK3032_UNTICKED_PLACE_COUNT={}", unticked_places.len());

    assert_eq!(ticked_places.len(), 4);
    assert_eq!(
        ticked_places
            .iter()
            .map(|place| (place.label.as_str(), place.place_kind))
            .collect::<Vec<_>>(),
        vec![
            ("SCRUB-I", ConversationPlaceKind::DirectMessage),
            ("SCRUB-I group", ConversationPlaceKind::GroupChat),
            ("SCRUB-I post", ConversationPlaceKind::PublicPost),
            ("SCRUB-I comment", ConversationPlaceKind::Comment),
        ]
    );
    assert!(unticked_places.is_empty());
}
