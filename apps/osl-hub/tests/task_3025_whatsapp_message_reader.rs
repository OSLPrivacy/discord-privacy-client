#![cfg(feature = "core")]

use osl_privacy_hub::services::save_messaging_risk_agreement;
use osl_privacy_hub::whatsapp_message_reader::{
    read_whatsapp_messages_for_scrub, WhatsAppBrowserMessage,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const PASSWORD: &str = "task-3025-whatsapp-message-reader";
const OWNER: &str = "task-3025-owner";
const ACCOUNT: &str = "whatsapp-scrub";
const PLACE: &str = "dm-scrub-w";
const SIGNED_IN_AUTHOR: &str = "whatsapp-scrub-owner";

struct AccountStorage {
    root: PathBuf,
}

impl AccountStorage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-task-3025-{}-{}",
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

fn seeded_browser_rows() -> Vec<WhatsAppBrowserMessage> {
    vec![
        WhatsAppBrowserMessage::new(
            PLACE,
            "whatsapp-scrub-003",
            "SCRUB-W-THEIRS-2",
            3,
            "whatsapp-friend-two",
        ),
        WhatsAppBrowserMessage::new(
            PLACE,
            "whatsapp-scrub-001",
            "SCRUB-W-MINE",
            1,
            SIGNED_IN_AUTHOR,
        ),
        WhatsAppBrowserMessage::new(
            PLACE,
            "whatsapp-scrub-002",
            "SCRUB-W-THEIRS-1",
            2,
            "whatsapp-friend-one",
        ),
    ]
}

#[test]
fn task_3025_whatsapp_shared_message_reader_returns_ordered_owned_seeded_rows_only_when_ticked() {
    let storage = AccountStorage::new();
    let account_dir = storage.root.join("owner");
    fs::create_dir(&account_dir).expect("create account directory");
    activate(&account_dir);
    let owner = keystore::generate_identity(OWNER.to_owned())
        .user_id
        .clone();
    save_messaging_risk_agreement(&owner, "whatsapp", ACCOUNT).expect("tick WhatsApp account");

    let messages = read_whatsapp_messages_for_scrub(
        &owner,
        ACCOUNT,
        PLACE,
        true,
        SIGNED_IN_AUTHOR,
        seeded_browser_rows(),
    )
    .expect("read ticked WhatsApp place");
    let unticked = read_whatsapp_messages_for_scrub(
        &owner,
        ACCOUNT,
        PLACE,
        false,
        SIGNED_IN_AUTHOR,
        seeded_browser_rows(),
    )
    .expect("unticked WhatsApp place is a normal empty result");

    println!("TASK3025_MESSAGE_COUNT={}", messages.len());
    for message in &messages {
        println!(
            "TASK3025_MESSAGE text={} time={} yours={}",
            message.text, message.time, message.yours
        );
    }
    println!("TASK3025_UNTICKED_MESSAGE_COUNT={}", unticked.len());

    assert_eq!(messages.len(), 3);
    assert_eq!(
        messages
            .iter()
            .map(|message| message.text.as_str())
            .collect::<Vec<_>>(),
        ["SCRUB-W-MINE", "SCRUB-W-THEIRS-1", "SCRUB-W-THEIRS-2"]
    );
    assert_eq!(
        messages
            .iter()
            .map(|message| message.yours)
            .collect::<Vec<_>>(),
        [true, false, false]
    );
    assert!(unticked.is_empty());
}
