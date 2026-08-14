#![cfg(feature = "core")]

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use osl_privacy_hub::native_signal_row_words::{
    LiveSignalTranscriptRowWords, SignalLiveRowCapture, SignalLiveRowCaptureSource,
};
use osl_privacy_hub::services::save_messaging_risk_agreement;
use osl_privacy_hub::signal_message_reader::{
    read_signal_messages_for_scrub, SignalScreenReadAction,
};
use osl_privacy_hub::signal_place_reader::{
    read_signal_conversation_places, seeded_signal_conversation_places,
};

const PASSWORD: &str = "task-4089-signal-row-words-password";
const OWNER_LABEL: &str = "task-4089-owner";
const ACCOUNT: &str = "signal-task-4089-ticked";
const SIGNED_IN_SENDER: &str = "signal-task-4089-owner";

struct Storage(PathBuf);

impl Storage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-task-4089-{}-{}",
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

/// Reads the real TASK 4089 live receipt captured against the signed-in
/// Windows Signal account (`evidence/task-4089-after.json`, produced by
/// `scripts/qa/task-4089-signal-live-row-words.ps1` after ten unique
/// messages were sent). This is what makes this test run the shipping
/// reader over genuine live-captured `row.name` values rather than a
/// hand-written fixture.
fn live_captured_rows() -> Vec<SignalLiveRowCapture> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let default_path = PathBuf::from(manifest_dir).join("../../evidence/task-4089-after.json");
    let receipt_path =
        std::env::var("TASK4089_AFTER_RECEIPT").unwrap_or_else(|_| {
            default_path
                .to_str()
                .expect("evidence path is valid UTF-8")
                .to_owned()
        });
    let raw = fs::read_to_string(&receipt_path).unwrap_or_else(|error| {
        panic!("could not read TASK 4089 live receipt at {receipt_path}: {error}")
    });
    let json: serde_json::Value =
        serde_json::from_str(&raw).expect("TASK 4089 live receipt is valid JSON");
    assert_eq!(
        json.get("schema").and_then(|value| value.as_str()),
        Some("task-4089-signal-live-row-words/v1"),
        "TASK 4089 receipt has the live-row-words schema"
    );
    assert_eq!(
        json.get("source").and_then(|value| value.as_str()),
        Some("live Windows UI Automation"),
        "TASK 4089 rows must come from the live Windows reader"
    );
    assert_eq!(json.get("winner").and_then(|value| value.as_str()), Some("row.name"));
    assert_eq!(json.get("saved_ui_trees").and_then(|value| value.as_u64()), Some(0));
    assert_eq!(json.get("key_press_count").and_then(|value| value.as_u64()), Some(0));
    assert_eq!(json.get("send_count").and_then(|value| value.as_u64()), Some(0));
    let capture = json.get("capture").expect("TASK 4089 CopyFromScreen capture");
    assert_eq!(
        capture.get("method").and_then(|value| value.as_str()),
        Some("Windows.PowerShell.CopyFromScreen")
    );
    assert!(
        capture
            .get("distinct_colours")
            .and_then(|value| value.as_u64())
            .is_some_and(|count| count > 16),
        "TASK 4089 CopyFromScreen capture has more than 16 distinct colours"
    );
    assert_eq!(
        capture
            .get("brightness_judgments")
            .and_then(|value| value.as_str()),
        Some("forbidden-and-not-used")
    );
    let messages = json
        .get("messages")
        .and_then(|value| value.as_array())
        .expect("TASK 4089 live receipt has a messages array");
    messages
        .iter()
        .map(|message| {
            let row_index = message
                .get("row_index")
                .and_then(|value| value.as_u64())
                .expect("row_index") as usize;
            let text = message
                .get("text")
                .and_then(|value| value.as_str())
                .expect("text")
                .to_owned();
            SignalLiveRowCapture::new(row_index, text)
        })
        .collect()
}

#[test]
fn task_4089_shipping_reader_fills_every_row_with_its_live_words() {
    let storage = Storage::new();
    keystore::set_active_account_dir(Some(storage.owner_dir()));
    let owner = keystore::generate_identity(OWNER_LABEL.to_owned())
        .user_id
        .clone();
    save_messaging_risk_agreement(&owner, "signal", ACCOUNT).expect("tick Signal account");
    let places =
        read_signal_conversation_places(&owner, ACCOUNT, &seeded_signal_conversation_places())
            .expect("read seeded Signal places");
    let place = places
        .iter()
        .find(|place| place.label == "SCRUB-S")
        .expect("find the seeded Signal place");

    let rows = live_captured_rows();
    let row_count = rows.len();
    let mut source =
        LiveSignalTranscriptRowWords::new(place.place_id.clone(), FixedRowCapture(rows));
    let read = read_signal_messages_for_scrub(
        &owner,
        ACCOUNT,
        place,
        SIGNED_IN_SENDER,
        &mut source,
    )
    .expect("shipping Signal reader accepts the live-captured rows");

    let empty_texts = read
        .messages
        .iter()
        .filter(|message| message.text.is_empty())
        .count();
    let unique_texts = read
        .messages
        .iter()
        .map(|message| message.text.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len();

    println!("TASK4089_LIVE_ROW_COUNT={row_count}");
    println!("TASK4089_MESSAGE_COUNT={}", read.messages.len());
    println!("TASK4089_EMPTY_TEXT_COUNT={empty_texts}");
    println!("TASK4089_UNIQUE_TEXT_COUNT={unique_texts}");
    println!("TASK4089_ACTION_LOG={:?}", read.action_log);
    println!("TASK4089_KEY_PRESS_COUNT={}", read.key_press_count());
    for message in &read.messages {
        println!("TASK4089_MESSAGE text={:?}", message.text);
    }

    assert_eq!(read.messages.len(), 10, "expected exactly 10 live rows");
    assert_eq!(empty_texts, 0, "expected 0 empty texts among the 10 rows");
    assert_eq!(unique_texts, 10, "expected 10 unique exact texts");
    assert_eq!(read.action_log, [SignalScreenReadAction::ReadOpenScreen]);
    assert_eq!(read.key_press_count(), 0);
}

#[test]
fn task_4089_removing_the_winning_route_starves_every_row_through_the_shipping_reader() {
    let storage = Storage::new();
    keystore::set_active_account_dir(Some(storage.owner_dir()));
    let owner = keystore::generate_identity(OWNER_LABEL.to_owned())
        .user_id
        .clone();
    save_messaging_risk_agreement(&owner, "signal", ACCOUNT).expect("tick Signal account");
    let places =
        read_signal_conversation_places(&owner, ACCOUNT, &seeded_signal_conversation_places())
            .expect("read seeded Signal places");
    let place = places
        .iter()
        .find(|place| place.label == "SCRUB-S")
        .expect("find the seeded Signal place");

    // Simulates removing the shipping winning text route: every row's live
    // name arrives blank, exactly as it would if `row.name` stopped being
    // read.
    let starved_rows = (0..10)
        .map(|index| SignalLiveRowCapture::new(index, ""))
        .collect::<Vec<_>>();
    let mut source =
        LiveSignalTranscriptRowWords::new(place.place_id.clone(), FixedRowCapture(starved_rows));
    let read = read_signal_messages_for_scrub(
        &owner,
        ACCOUNT,
        place,
        SIGNED_IN_SENDER,
        &mut source,
    )
    .expect("shipping Signal reader still runs, it just has no rows to hand back");

    println!("TASK4089_STARVED_MESSAGE_COUNT={}", read.messages.len());
    assert_eq!(
        read.messages.len(),
        0,
        "removing the winning route must starve every row of its words"
    );
}
