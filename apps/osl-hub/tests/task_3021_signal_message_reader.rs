#![cfg(feature = "core")]

use osl_privacy_hub::services::save_messaging_risk_agreement;
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

const PASSWORD: &str = "task-3021-signal-message-reader";
const OWNER: &str = "task-3021-owner";
const ACCOUNT: &str = "signal-scrub";
const SIGNED_IN_SENDER: &str = "signal-scrub-owner";

struct Storage(PathBuf);

impl Storage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-task-3021-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time after epoch")
                .as_nanos()
        ));
        fs::create_dir(&root).expect("create isolated task 3021 root");
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

struct SeededOpenScreen {
    snapshot: SignalOpenScreenSnapshot,
    action_log: Vec<SignalScreenReadAction>,
}

impl SignalOpenScreenSource for SeededOpenScreen {
    fn read_open_screen(&mut self) -> Result<SignalOpenScreenSnapshot, String> {
        self.action_log.push(SignalScreenReadAction::ReadOpenScreen);
        Ok(self.snapshot.clone())
    }

    fn action_log(&self) -> &[SignalScreenReadAction] {
        &self.action_log
    }
}

#[test]
fn task_3021_reads_three_signal_messages_from_open_screen_without_key_presses() {
    let storage = Storage::new();
    keystore::set_active_account_dir(Some(storage.owner_dir()));
    let owner = keystore::generate_identity(OWNER.to_owned())
        .user_id
        .clone();
    save_messaging_risk_agreement(&owner, "signal", ACCOUNT).expect("tick Signal account");

    let places =
        read_signal_conversation_places(&owner, ACCOUNT, &seeded_signal_conversation_places())
            .expect("read seeded Signal places");
    let scrub_s = places
        .iter()
        .find(|place| place.label == "SCRUB-S")
        .expect("find SCRUB-S direct conversation");
    let mut screen = SeededOpenScreen {
        snapshot: SignalOpenScreenSnapshot::new(
            &scrub_s.place_id,
            [
                SignalOpenScreenMessage::new(
                    "signal-scrub-003",
                    "SCRUB-S-THEIRS-2",
                    300,
                    "signal-friend-two",
                ),
                SignalOpenScreenMessage::new(
                    "signal-scrub-002",
                    "SCRUB-S-MINE",
                    200,
                    SIGNED_IN_SENDER,
                ),
                SignalOpenScreenMessage::new(
                    "signal-scrub-001",
                    "SCRUB-S-THEIRS-1",
                    100,
                    "signal-friend-one",
                ),
            ],
        ),
        action_log: Vec::new(),
    };

    let read =
        read_signal_messages_for_scrub(&owner, ACCOUNT, scrub_s, SIGNED_IN_SENDER, &mut screen)
            .expect("read only the already-open SCRUB-S screen");

    println!("TASK3021_PLACE={}", scrub_s.label);
    println!("TASK3021_MESSAGE_COUNT={}", read.messages.len());
    for message in &read.messages {
        println!(
            "TASK3021_MESSAGE time={} text={} yours={}",
            message.time, message.text, message.yours
        );
    }
    println!("TASK3021_ACTION_LOG={:?}", read.action_log);
    println!("TASK3021_ACTION_COUNT={}", read.action_log.len());
    println!("TASK3021_KEY_PRESS_COUNT={}", read.key_press_count());

    assert_eq!(scrub_s.label, "SCRUB-S");
    assert_eq!(read.messages.len(), 3);
    assert_eq!(
        read.messages
            .iter()
            .map(|message| (message.time, message.text.as_str(), message.yours))
            .collect::<Vec<_>>(),
        [
            (100, "SCRUB-S-THEIRS-1", false),
            (200, "SCRUB-S-MINE", true),
            (300, "SCRUB-S-THEIRS-2", false),
        ]
    );
    assert_eq!(read.action_log, [SignalScreenReadAction::ReadOpenScreen]);
    assert_eq!(read.key_press_count(), 0);
}
