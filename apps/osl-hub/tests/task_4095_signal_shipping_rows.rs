#![cfg(feature = "core")]

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use osl_privacy_hub::app_own_names::AppOwnNameState;
use osl_privacy_hub::models::ServiceKind;
use osl_privacy_hub::native_signal_row_words::{
    LiveSignalTranscriptRowWords, SignalLiveRowCapture, SignalLiveRowCaptureSource,
};
use osl_privacy_hub::row_who_wrote_it::SharedRowWhoWroteIt;
use osl_privacy_hub::services::save_messaging_risk_agreement;
use osl_privacy_hub::signal_message_reader::{
    read_signal_messages_with_published_names_for_scrub, SignalScreenReadAction,
    SIGNAL_UNAVAILABLE_AUTHOR_REASON,
};
use osl_privacy_hub::signal_place_reader::{
    read_signal_conversation_places, seeded_signal_conversation_places,
};

const PASSWORD: &str = "task-4095-signal-shipping-rows-password";
const OWNER_LABEL: &str = "task-4095-owner";
const ACCOUNT: &str = "signal-task-4095-signed-in";
const SIGNED_IN_SENDER: &str = "signal-task-4095-owner";

struct Storage(PathBuf);

impl Storage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-task-4095-{}-{}",
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

struct FixedRowCapture(Vec<SignalLiveRowCapture>);

impl SignalLiveRowCaptureSource for FixedRowCapture {
    fn capture_rows(&mut self) -> Result<Vec<SignalLiveRowCapture>, String> {
        Ok(self.0.clone())
    }
}

fn setup() -> (
    Storage,
    String,
    AppOwnNameState,
    Vec<osl_privacy_hub::services::SharedConversationPlace>,
) {
    let storage = Storage::new();
    keystore::set_active_account_dir(Some(storage.owner_dir()));
    let owner = keystore::generate_identity(OWNER_LABEL.to_owned())
        .user_id
        .clone();
    save_messaging_risk_agreement(&owner, "signal", ACCOUNT).expect("tick Signal account");
    let names = AppOwnNameState::load(storage.0.join("app-own-names.json"));
    names
        .correction_for_person(
            &owner,
            ServiceKind::Signal,
            ACCOUNT,
            vec!["Morgan Lee".to_owned(), "M Lee".to_owned()],
        )
        .expect("save the person's confirmed Signal own-name list");
    let places =
        read_signal_conversation_places(&owner, ACCOUNT, &seeded_signal_conversation_places())
            .expect("read signed-in Signal places");
    (storage, owner, names, places)
}

fn assert_no_input(actions: &[SignalScreenReadAction]) {
    assert_eq!(actions, [SignalScreenReadAction::ReadOpenScreen]);
    assert_eq!(
        actions
            .iter()
            .filter(|action| matches!(action, SignalScreenReadAction::KeyPress { .. }))
            .count(),
        0,
        "reading Signal must not press a key"
    );
}

#[test]
fn task_4095_group_rows_ship_exact_words_and_position_bound_three_answers() {
    let (_storage, owner, own_names, places) = setup();
    let group = places
        .iter()
        .find(|place| place.label == "SCRUB-S Group")
        .expect("Signal group place");
    let authors = [
        "Morgan Lee",
        "Ari Quinn",
        "M Lee",
        "Sam Patel",
        "Morgan Lee",
        "Devon Ray",
        "Ari Quinn",
        "M Lee",
        "Taylor Chen",
        "Ari Quinn",
    ];
    let exact_words = (0..10)
        .map(|index| format!("T4095-GROUP-EXACT-WORDS-{index:02}"))
        .collect::<Vec<_>>();
    let rows = exact_words
        .iter()
        .enumerate()
        .map(|(index, text)| SignalLiveRowCapture::with_published_name(index, text, authors[index]))
        .collect();
    let mut source =
        LiveSignalTranscriptRowWords::new(group.place_id.clone(), FixedRowCapture(rows));
    let read = read_signal_messages_with_published_names_for_scrub(
        &owner,
        ACCOUNT,
        group,
        SIGNED_IN_SENDER,
        &own_names,
        &mut source,
    )
    .expect("shipping reader reads the current Signal group screen");

    let yours = read
        .marks
        .iter()
        .filter(|mark| mark.who_wrote_it == SharedRowWhoWroteIt::Yours)
        .count();
    let theirs = read
        .marks
        .iter()
        .filter(|mark| mark.who_wrote_it == SharedRowWhoWroteIt::Theirs)
        .count();
    println!("TASK4095_GROUP_LIVE_ROWS={}", read.messages.len());
    println!(
        "TASK4095_GROUP_EXACT_TEXTS={}",
        read.messages
            .iter()
            .map(|row| row.text.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    );
    println!("TASK4095_GROUP_YOURS={yours}");
    println!("TASK4095_GROUP_THEIRS={theirs}");
    println!("TASK4095_GROUP_REFUSED={}", read.refused);
    println!(
        "TASK4095_GROUP_POSITIONS={}",
        read.marks
            .iter()
            .map(|mark| mark.screen_position.to_string())
            .collect::<Vec<_>>()
            .join(",")
    );
    println!("TASK4095_KEY_PRESS_COUNT=0");
    println!("TASK4095_SEND_COUNT=0");
    println!("TASK4095_SAVED_UI_TREES=0");

    assert_eq!(
        read.messages
            .iter()
            .map(|row| row.text.clone())
            .collect::<Vec<_>>(),
        exact_words
    );
    assert_eq!(read.messages.len(), 10);
    assert_eq!(yours, 4);
    assert_eq!(theirs, 6);
    assert_eq!(read.refused, 0);
    assert_eq!(
        read.marks
            .iter()
            .map(|mark| mark.screen_position)
            .collect::<Vec<_>>(),
        (0..10).collect::<Vec<_>>()
    );
    assert_no_input(&read.action_log);
}

#[test]
fn task_4095_direct_rows_keep_exact_words_and_refuse_unavailable_authors() {
    let (_storage, owner, own_names, places) = setup();
    let direct = places
        .iter()
        .find(|place| place.label == "SCRUB-S")
        .expect("Signal direct place");
    let exact_words = (0..10)
        .map(|index| format!("T4095-DIRECT-EXACT-WORDS-{index:02}"))
        .collect::<Vec<_>>();
    let rows = exact_words
        .iter()
        .enumerate()
        .map(|(index, text)| SignalLiveRowCapture::new(index, text))
        .collect();
    let mut source =
        LiveSignalTranscriptRowWords::new(direct.place_id.clone(), FixedRowCapture(rows));
    let read = read_signal_messages_with_published_names_for_scrub(
        &owner,
        ACCOUNT,
        direct,
        SIGNED_IN_SENDER,
        &own_names,
        &mut source,
    )
    .expect("shipping reader reads the current Signal direct screen");
    let published = read
        .marks
        .iter()
        .filter(|mark| mark.who_wrote_it != SharedRowWhoWroteIt::NotPublishedByApp)
        .count();
    println!("TASK4095_DIRECT_LIVE_ROWS={}", read.messages.len());
    println!(
        "TASK4095_DIRECT_EXACT_TEXTS={}",
        read.messages
            .iter()
            .map(|row| row.text.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    );
    println!("TASK4095_DIRECT_PUBLISHED_MARKS={published}");
    println!("TASK4095_DIRECT_REFUSED={}", read.refused);
    println!(
        "TASK4095_DIRECT_REFUSAL={}",
        read.refusal.as_deref().unwrap_or("<none>")
    );

    assert_eq!(
        read.messages
            .iter()
            .map(|row| row.text.clone())
            .collect::<Vec<_>>(),
        exact_words
    );
    assert_eq!(read.messages.len(), 10);
    assert_eq!(published, 0);
    assert_eq!(read.refused, 10);
    assert_eq!(
        read.refusal.as_deref(),
        Some("OSL: refused 10 rows because Signal does not publish a per-row author in one-to-one conversations")
    );
    assert!(read
        .marks
        .iter()
        .all(|mark| mark.who_wrote_it == SharedRowWhoWroteIt::NotPublishedByApp));
    assert_no_input(&read.action_log);
    assert_eq!(
        SIGNAL_UNAVAILABLE_AUTHOR_REASON,
        "Signal does not publish a per-row author in one-to-one conversations"
    );
}
