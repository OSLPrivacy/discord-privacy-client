#![cfg(feature = "core")]

use osl_privacy_hub::services::{
    read_telegram_desktop_shared_messages, save_messaging_risk_agreement, TelegramDesktopMachine,
    TelegramDesktopMessage, TelegramDesktopPlace, TelegramDesktopPlaceKind,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const PASSWORD: &str = "task-3017-telegram-shared-message-reader-password";
const PLACE: &str = "telegram-direct-scrub-t";

struct Storage(PathBuf);

impl Storage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-task-3017-{}-{}",
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

fn telegram_desktop_machine() -> TelegramDesktopMachine {
    TelegramDesktopMachine::new([TelegramDesktopPlace::new(
        PLACE,
        "SCRUB-T",
        TelegramDesktopPlaceKind::DirectChat,
    )])
    // Deliberately provider-order rows: the reader must order by time.
    .with_messages([
        TelegramDesktopMessage::new(
            PLACE,
            "telegram-scrub-t-theirs-2",
            "SCRUB-T-THEIRS-2",
            300,
            false,
        ),
        TelegramDesktopMessage::new(
            PLACE,
            "telegram-scrub-t-theirs-1",
            "SCRUB-T-THEIRS-1",
            100,
            false,
        ),
        TelegramDesktopMessage::new(PLACE, "telegram-scrub-t-mine", "SCRUB-T-MINE", 200, true),
    ])
}

#[test]
fn task_3017_telegram_messages_are_ordered_owned_and_silent_when_place_is_not_ticked() {
    let storage = Storage::new();
    keystore::set_active_account_dir(Some(storage.owner_dir()));
    let owner = keystore::generate_identity("task-3017-owner".to_owned())
        .user_id
        .clone();
    let account = "telegram-ticked";
    save_messaging_risk_agreement(&owner, "telegram", account).expect("tick Telegram account");

    let desktop = telegram_desktop_machine();
    let read = read_telegram_desktop_shared_messages(&owner, account, PLACE, &desktop)
        .expect("read ticked Telegram messages");
    let unticked_place = read_telegram_desktop_shared_messages(
        &owner,
        account,
        "telegram-direct-not-ticked",
        &desktop,
    )
    .expect("unticked Telegram place remains silent");

    println!("TASK3017_TICKED_MESSAGE_COUNT={}", read.len());
    for message in &read {
        println!(
            "TASK3017_MESSAGE time={} text={} yours={}",
            message.time, message.text, message.yours
        );
    }
    println!(
        "TASK3017_UNTICKED_PLACE_MESSAGE_COUNT={}",
        unticked_place.len()
    );

    assert_eq!(read.len(), 3);
    assert_eq!(
        read.iter()
            .map(|message| (message.time, message.text.as_str(), message.yours))
            .collect::<Vec<_>>(),
        [
            (100, "SCRUB-T-THEIRS-1", false),
            (200, "SCRUB-T-MINE", true),
            (300, "SCRUB-T-THEIRS-2", false),
        ]
    );
    assert!(unticked_place.is_empty());
}
