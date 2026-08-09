#![cfg(feature = "core")]

use osl_privacy_hub::services::save_messaging_risk_agreement;
use osl_privacy_hub::signal_marked_message_deleter::{
    delete_marked_signal_message, delete_marked_signal_messages_and_record, SignalReviewSelection,
};
use osl_privacy_hub::signal_message_reader::{
    read_signal_messages_for_scrub, SignalOpenScreenMessage, SignalOpenScreenSnapshot,
    SignalOpenScreenSource, SignalScreenReadAction,
};
use osl_privacy_hub::signal_place_reader::{
    read_signal_conversation_places, seeded_signal_conversation_places,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const PASSWORD: &str = "task-3023-signal-marked-message-deleter";
const OWNER: &str = "task-3023-owner";
const ACCOUNT: &str = "signal-task-3023";
const SIGNED_IN_SENDER: &str = "signal-task-3023-owner";
const OTHER_SENDER: &str = "signal-task-3023-other-account";
const PLACE: &str = "signal-scrub-s-direct";
const MARKER: &str = "SCRUB-S-DEL";
const FIRST_MARKED_ID: &str = "signal-task-3023-first-marked";
const SECOND_MARKED_ID: &str = "signal-task-3023-second-marked";
const FOREIGN_ID: &str = "signal-task-3023-not-yours";

struct Storage(PathBuf);

impl Storage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-task-3023-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time after epoch")
                .as_nanos()
        ));
        fs::create_dir(&root).expect("create isolated task 3023 root");
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

struct ScreenRead {
    snapshot: SignalOpenScreenSnapshot,
    actions: Vec<SignalScreenReadAction>,
}

impl SignalOpenScreenSource for ScreenRead {
    fn read_open_screen(&mut self) -> Result<SignalOpenScreenSnapshot, String> {
        self.actions.push(SignalScreenReadAction::ReadOpenScreen);
        Ok(self.snapshot.clone())
    }

    fn action_log(&self) -> &[SignalScreenReadAction] {
        &self.actions
    }
}

fn seeded_screen() -> SignalOpenScreenSnapshot {
    SignalOpenScreenSnapshot::new(
        PLACE,
        [
            SignalOpenScreenMessage::new(
                FIRST_MARKED_ID,
                "SCRUB-S-DEL first marked owned",
                100,
                SIGNED_IN_SENDER,
            ),
            SignalOpenScreenMessage::new(
                "signal-task-3023-kept",
                "SCRUB-S-DEL kept owned",
                200,
                SIGNED_IN_SENDER,
            ),
            SignalOpenScreenMessage::new(
                SECOND_MARKED_ID,
                "SCRUB-S-DEL second marked owned",
                300,
                SIGNED_IN_SENDER,
            ),
            SignalOpenScreenMessage::new(FOREIGN_ID, "foreign ownership probe", 400, OTHER_SENDER),
        ],
    )
}

fn matching_messages(
    owner: &str,
    place: &osl_privacy_hub::services::SharedConversationPlace,
    screen: &SignalOpenScreenSnapshot,
) -> Vec<(String, bool)> {
    let mut source = ScreenRead {
        snapshot: screen.clone(),
        actions: Vec::new(),
    };
    read_signal_messages_for_scrub(owner, ACCOUNT, place, SIGNED_IN_SENDER, &mut source)
        .expect("read the open Signal screen through gate 3021")
        .messages
        .into_iter()
        .filter(|message| message.text.contains(MARKER))
        .map(|message| (message.message_id, message.yours))
        .collect()
}

#[test]
fn task_3023_deletes_two_marked_owned_signal_messages_and_refuses_not_yours() {
    let storage = Storage::new();
    keystore::set_active_account_dir(Some(storage.owner_dir()));
    let owner = keystore::generate_identity(OWNER.to_owned())
        .user_id
        .clone();
    save_messaging_risk_agreement(&owner, "signal", ACCOUNT).expect("tick Signal account");
    let places =
        read_signal_conversation_places(&owner, ACCOUNT, &seeded_signal_conversation_places())
            .expect("read seeded Signal places");
    let place = places
        .iter()
        .find(|place| place.place_id == PLACE)
        .expect("find the SCRUB-S place");

    let mut screen = seeded_screen();
    let marked_owned = [
        SignalReviewSelection::marked(PLACE, FIRST_MARKED_ID),
        SignalReviewSelection::marked(PLACE, SECOND_MARKED_ID),
    ];
    let before = matching_messages(&owner, place, &screen);
    assert_eq!(marked_owned.len(), 2);
    assert_eq!(before.len(), 3);
    assert!(before.iter().all(|(_, yours)| *yours));

    let first = delete_marked_signal_messages_and_record(
        &mut screen,
        SIGNED_IN_SENDER,
        [marked_owned[0].clone()],
    )
    .expect("delete the first marked owned Signal message");
    let after_first = matching_messages(&owner, place, &screen);
    assert_eq!(first.outcomes.deleted_count, 1);
    assert_eq!(first.outcomes.failed_count, 0);
    assert_eq!(first.outcomes.needs_attention_count, 0);
    assert_eq!(first.removals.len(), 1);
    assert_eq!(before.len(), 3);
    assert_eq!(after_first.len(), 2);
    assert!(!after_first.iter().any(|(id, _)| id == FIRST_MARKED_ID));

    let rows_before_refusal = screen.messages.len();
    let refused = delete_marked_signal_message(
        &mut screen,
        SIGNED_IN_SENDER,
        &SignalReviewSelection::marked(PLACE, FOREIGN_ID),
    )
    .expect_err("another Signal account's message is refused");
    let foreign_present = screen
        .messages
        .iter()
        .any(|message| message.message_id == FOREIGN_ID);
    assert_eq!(refused.code(), "not_yours");
    assert_eq!(screen.messages.len(), rows_before_refusal);
    assert!(foreign_present);
    let after_refusal = matching_messages(&owner, place, &screen);
    assert_eq!(after_refusal, after_first);

    let second = delete_marked_signal_message(&mut screen, SIGNED_IN_SENDER, &marked_owned[1])
        .expect("delete the second marked owned Signal message");
    let after_second = matching_messages(&owner, place, &screen);
    assert_eq!(second.shared.deleted_count, 1);
    assert_eq!(second.removals.len(), 1);
    assert_eq!(after_first.len(), 2);
    assert_eq!(after_second.len(), 1);
    assert!(!after_second.iter().any(|(id, _)| id == SECOND_MARKED_ID));

    println!("TASK3023_MARKED_OWNED_COUNT={}", marked_owned.len());
    println!("TASK3023_SCRUB_S_DEL_COUNT_BEFORE={}", before.len());
    println!(
        "TASK3023_SCRUB_S_DEL_COUNT_AFTER_FIRST={}",
        after_first.len()
    );
    println!("TASK3023_NOT_YOURS_ERROR={}", refused.code());
    println!("TASK3023_FOREIGN_PRESENT_AFTER_REFUSAL={foreign_present}");
    println!(
        "TASK3023_SCRUB_S_DEL_COUNT_BEFORE_SECOND={}",
        after_refusal.len()
    );
    println!(
        "TASK3023_SCRUB_S_DEL_COUNT_AFTER_SECOND={}",
        after_second.len()
    );
}
