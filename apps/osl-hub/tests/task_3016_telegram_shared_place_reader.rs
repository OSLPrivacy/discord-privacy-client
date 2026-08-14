#![cfg(feature = "core")]

use osl_privacy_hub::services::{
    read_telegram_desktop_shared_places, save_messaging_risk_agreement, ConversationPlaceKind,
    TelegramDesktopMachine, TelegramDesktopPlace, TelegramDesktopPlaceKind,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const TEST_MAIN_PASSWORD: &str = "task-3016-telegram-shared-place-reader-password";

struct AccountStorage {
    root: PathBuf,
}

impl AccountStorage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-task-3016-telegram-reader-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time after unix epoch")
                .as_nanos()
        ));
        fs::create_dir(&root).expect("create isolated task 3016 root");
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

fn telegram_desktop_machine() -> TelegramDesktopMachine {
    TelegramDesktopMachine::new([
        TelegramDesktopPlace::new(
            "telegram-direct-scrub-t",
            "SCRUB-T",
            TelegramDesktopPlaceKind::DirectChat,
        ),
        TelegramDesktopPlace::new(
            "telegram-direct-scrub-t-second",
            "SCRUB-T direct two",
            TelegramDesktopPlaceKind::DirectChat,
        ),
        TelegramDesktopPlace::new(
            "telegram-group-scrub-t",
            "SCRUB-T group",
            TelegramDesktopPlaceKind::Group,
        ),
        TelegramDesktopPlace::new(
            "telegram-channel-scrub-t",
            "SCRUB-T channel",
            TelegramDesktopPlaceKind::Channel,
        ),
    ])
}

#[test]
fn task_3016_telegram_desktop_machine_returns_four_ticked_places_and_none_unticked() {
    let storage = AccountStorage::new();
    let owner_dir = storage.account_dir();
    keystore::set_active_account_dir(Some(owner_dir));

    let owner = keystore::generate_identity("task-3016-owner".to_owned())
        .user_id
        .clone();
    let ticked_account = "telegram-ticked";
    let unticked_account = "telegram-unticked";
    save_messaging_risk_agreement(&owner, "telegram", ticked_account)
        .expect("tick Telegram account");

    let desktop_machine = telegram_desktop_machine();
    let ticked_places =
        read_telegram_desktop_shared_places(&owner, ticked_account, &desktop_machine)
            .expect("read ticked Telegram Desktop machine");
    let unticked_places =
        read_telegram_desktop_shared_places(&owner, unticked_account, &desktop_machine)
            .expect("read unticked Telegram Desktop machine");

    println!("TASK3016_DESKTOP_MACHINE=telegram");
    println!("TASK3016_TICKED_PLACE_COUNT={}", ticked_places.len());
    for place in &ticked_places {
        println!(
            "TASK3016_PLACE id={} name={} kind={} server={}",
            place.place_id,
            place.label,
            place.place_kind.as_str(),
            place
                .server
                .as_ref()
                .map(|server| server.id.as_str())
                .unwrap_or("none"),
        );
    }
    println!("TASK3016_UNTICKED_PLACE_COUNT={}", unticked_places.len());

    assert_eq!(ticked_places.len(), 4);
    assert_eq!(
        ticked_places
            .iter()
            .map(|place| (
                place.label.as_str(),
                place.place_kind,
                place.server.is_some()
            ))
            .collect::<Vec<_>>(),
        vec![
            ("SCRUB-T", ConversationPlaceKind::DirectMessage, false),
            (
                "SCRUB-T direct two",
                ConversationPlaceKind::DirectMessage,
                false
            ),
            ("SCRUB-T group", ConversationPlaceKind::Group, false),
            ("SCRUB-T channel", ConversationPlaceKind::Channel, false),
        ]
    );
    assert!(unticked_places.is_empty());
}
