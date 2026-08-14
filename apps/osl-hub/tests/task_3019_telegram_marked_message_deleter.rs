#![cfg(feature = "core")]

use osl_privacy_hub::services::{
    read_telegram_desktop_shared_messages, save_messaging_risk_agreement, TelegramDesktopMachine,
    TelegramDesktopMessage, TelegramDesktopPlace, TelegramDesktopPlaceKind,
};
use osl_privacy_hub::telegram_marked_message_deleter::{
    delete_marked_telegram_message, delete_marked_telegram_messages_and_record,
    TelegramDeletionScope, TelegramReviewSelection,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const PASSWORD: &str = "task-3019-telegram-marked-message-deleter-password";
const ACCOUNT: &str = "telegram-task-3019";
const PLACE: &str = "telegram-direct-task-3019";
const MARKER: &str = "SCRUB-T-DEL";
const MARKED_ID: &str = "telegram-task-3019-marked";
const FOREIGN_ID: &str = "telegram-task-3019-not-yours";
const EVERYONE_ID: &str = "telegram-task-3019-for-everyone";

struct Storage(PathBuf);

impl Storage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-task-3019-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time after unix epoch")
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

fn seeded_desktop() -> TelegramDesktopMachine {
    TelegramDesktopMachine::new([TelegramDesktopPlace::new(
        PLACE,
        "Task 3019 Telegram direct chat",
        TelegramDesktopPlaceKind::DirectChat,
    )])
    .with_messages([
        // Exactly three rows match SCRUB-T-DEL; only MARKED_ID is initially
        // supplied as a marked review selection.
        TelegramDesktopMessage::new(PLACE, MARKED_ID, "SCRUB-T-DEL marked mine", 100, true),
        TelegramDesktopMessage::new(
            PLACE,
            "telegram-task-3019-kept",
            "SCRUB-T-DEL kept mine",
            200,
            true,
        ),
        TelegramDesktopMessage::new(PLACE, FOREIGN_ID, "SCRUB-T-DEL foreign", 300, false),
        // Non-marker probe proves an explicit review choice reaches the
        // delete-for-everyone affordance without changing the 3 -> 2 count.
        TelegramDesktopMessage::new(PLACE, EVERYONE_ID, "scope probe", 400, true),
    ])
}

fn matching_messages(owner: &str, desktop: &TelegramDesktopMachine) -> Vec<String> {
    read_telegram_desktop_shared_messages(owner, ACCOUNT, PLACE, desktop)
        .expect("read Telegram messages through gate 3017")
        .into_iter()
        .filter(|message| message.text.contains(MARKER))
        .map(|message| message.message_id)
        .collect()
}

#[test]
fn task_3019_deletes_only_the_marked_telegram_message_and_refuses_not_yours() {
    let storage = Storage::new();
    keystore::set_active_account_dir(Some(storage.owner_dir()));
    let owner = keystore::generate_identity("task-3019-owner".to_owned())
        .user_id
        .clone();
    save_messaging_risk_agreement(&owner, "telegram", ACCOUNT).expect("tick the Telegram account");

    let mut desktop = seeded_desktop();
    let before_ids = matching_messages(&owner, &desktop);
    let recorded = delete_marked_telegram_messages_and_record(
        &mut desktop,
        [TelegramReviewSelection::marked(PLACE, MARKED_ID)],
    )
    .expect("delete the one marked Telegram message");
    let after_ids = matching_messages(&owner, &desktop);

    assert_eq!(before_ids.len(), 3);
    assert_eq!(after_ids.len(), 2);
    assert!(before_ids.iter().any(|id| id == MARKED_ID));
    assert!(!after_ids.iter().any(|id| id == MARKED_ID));
    assert_eq!(recorded.outcomes.deleted_count, 1);
    assert_eq!(recorded.outcomes.failed_count, 0);
    assert_eq!(recorded.outcomes.needs_attention_count, 0);
    assert_eq!(recorded.removals.len(), 1);
    assert_eq!(
        recorded.removals[0].scope,
        TelegramDeletionScope::DeleteForMe,
        "an omitted review scope must delete for this account only"
    );

    let count_before_refusal = desktop.messages.len();
    let refused = delete_marked_telegram_message(
        &mut desktop,
        &TelegramReviewSelection::marked(PLACE, FOREIGN_ID),
    )
    .expect_err("a message the signed-in Telegram account did not send is refused");
    assert_eq!(refused.code(), "not_yours");
    assert_eq!(desktop.messages.len(), count_before_refusal);
    assert!(desktop
        .messages
        .iter()
        .any(|message| message.message_id == FOREIGN_ID));

    let explicit = delete_marked_telegram_message(
        &mut desktop,
        &TelegramReviewSelection::marked_for_everyone(PLACE, EVERYONE_ID),
    )
    .expect("honor the review's explicit delete-for-everyone choice");
    assert_eq!(explicit.shared.deleted_count, 1);
    assert_eq!(explicit.removals.len(), 1);
    assert_eq!(
        explicit.removals[0].scope,
        TelegramDeletionScope::DeleteForEveryone
    );

    println!("TASK3019_SCRUB_T_DEL_COUNT_BEFORE={}", before_ids.len());
    println!("TASK3019_SCRUB_T_DEL_COUNT_AFTER={}", after_ids.len());
    println!("TASK3019_MISSING_MESSAGE_ID={MARKED_ID}");
    println!(
        "TASK3019_DEFAULT_SCOPE={}",
        recorded.removals[0].scope.as_str()
    );
    println!(
        "TASK3019_EXPLICIT_SCOPE={}",
        explicit.removals[0].scope.as_str()
    );
    println!("TASK3019_NOT_YOURS_ERROR={}", refused.code());
}
