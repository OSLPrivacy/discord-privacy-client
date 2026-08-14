#![cfg(feature = "core")]

use osl_privacy_hub::services::{
    read_x_shared_places, save_messaging_risk_agreement, ConversationPlaceKind, XBrowserMachine,
    XBrowserPlace, XBrowserPlaceKind,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const TEST_MAIN_PASSWORD: &str = "task-3028-x-shared-place-reader-password";

struct AccountStorage {
    root: PathBuf,
}

impl AccountStorage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-task-3028-x-reader-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time after unix epoch")
                .as_nanos()
        ));
        fs::create_dir(&root).expect("create isolated task 3028 root");
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

fn x_browser_machine() -> XBrowserMachine {
    XBrowserMachine::new([
        XBrowserPlace::new(
            "x-dm-scrub-x",
            "SCRUB-X",
            XBrowserPlaceKind::DirectMessage,
        ),
        XBrowserPlace::new(
            "x-group-scrub-x",
            "SCRUB-X group",
            XBrowserPlaceKind::GroupDirectMessage,
        ),
        XBrowserPlace::new(
            "x-public-scrub-x",
            "SCRUB-X post and reply",
            XBrowserPlaceKind::OwnPostOrReply,
        ),
    ])
}

#[test]
fn task_3028_x_browser_machine_returns_three_ticked_places_and_none_unticked() {
    let storage = AccountStorage::new();
    let owner_dir = storage.account_dir();
    keystore::set_active_account_dir(Some(owner_dir));

    let owner = keystore::generate_identity("task-3028-owner".to_owned())
        .user_id
        .clone();
    let ticked_account = "x-ticked";
    let unticked_account = "x-unticked";
    save_messaging_risk_agreement(&owner, "x", ticked_account).expect("tick X account");

    let browser_machine = x_browser_machine();
    let ticked_places = read_x_shared_places(&owner, ticked_account, &browser_machine)
        .expect("read ticked X browser machine");
    let unticked_places = read_x_shared_places(&owner, unticked_account, &browser_machine)
        .expect("read unticked X browser machine");

    println!("TASK3028_BROWSER_MACHINE=x");
    println!("TASK3028_TICKED_PLACE_COUNT={}", ticked_places.len());
    for place in &ticked_places {
        println!(
            "TASK3028_PLACE id={} name={} kind={}",
            place.place_id,
            place.label,
            place.place_kind.as_str()
        );
    }
    println!("TASK3028_UNTICKED_PLACE_COUNT={}", unticked_places.len());

    assert_eq!(ticked_places.len(), 3);
    assert_eq!(
        ticked_places
            .iter()
            .map(|place| (place.label.as_str(), place.place_kind))
            .collect::<Vec<_>>(),
        vec![
            ("SCRUB-X", ConversationPlaceKind::DirectMessage),
            ("SCRUB-X group", ConversationPlaceKind::GroupChat),
            ("SCRUB-X post and reply", ConversationPlaceKind::PublicPost),
        ]
    );
    assert!(unticked_places.is_empty());
}
