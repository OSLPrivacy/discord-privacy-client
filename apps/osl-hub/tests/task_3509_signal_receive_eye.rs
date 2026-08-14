#![cfg(feature = "core")]

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use osl_privacy_hub::broker::ProtectedRowRecord;
use osl_privacy_hub::native_signal_row_words::{
    LiveSignalTranscriptRowWords, SignalLiveRowCapture, SignalLiveRowCaptureSource,
};
use osl_privacy_hub::services::save_messaging_risk_agreement;
use osl_privacy_hub::shipping_receive::{ShippingReceiveJournal, ShippingService};
use osl_privacy_hub::signal_eye_control::{
    open_received_signal_eye, receive_signal_rows_into_eye, SignalEyeControl, SignalEyeState,
};
use osl_privacy_hub::signal_place_reader::{
    read_signal_conversation_places, seeded_signal_conversation_places,
};

const PASSWORD: &str = "task-3509-signal-eye-password";
const ACCOUNT: &str = "signal-task-3509-signed-in";
const SIGNED_IN_SENDER: &str = "signal-task-3509-owner";
const COVER: &str = "[OSL-3509] live carrier cover";
const PRIVATE_WORDS: &str = "exact private words from the other Signal account";
const ROW_INDEX: usize = 17;

struct Storage(PathBuf);

impl Storage {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "osl-task-3509-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir(&path).expect("create storage");
        keystore::set_base_dir_override(Some(path.clone()));
        keystore::set_active_account_dir(None);
        ipc::main_password::set_file_storage_key(None);
        ipc::main_password::set_main_password(&path, PASSWORD).expect("main password");
        Self(path)
    }
    fn owner_dir(&self) -> PathBuf {
        let path = self.0.join("owner");
        fs::create_dir(&path).expect("owner directory");
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

struct FreshSignalTranscript(Vec<SignalLiveRowCapture>);

impl SignalLiveRowCaptureSource for FreshSignalTranscript {
    fn capture_rows(&mut self) -> Result<Vec<SignalLiveRowCapture>, String> {
        Ok(self.0.clone())
    }
}

fn protected_record() -> ProtectedRowRecord {
    ProtectedRowRecord {
        app_id: "signal".to_owned(),
        app_row_id: format!("signal-live-row-{ROW_INDEX:06}"),
        cover_text: COVER.to_owned(),
        private_words: PRIVATE_WORDS.to_owned(),
        opened: true,
    }
}

#[test]
fn task_3509_signal_reader_arrival_feeds_the_shipping_eye_record() {
    let storage = Storage::new();
    keystore::set_active_account_dir(Some(storage.owner_dir()));
    let owner = keystore::generate_identity("task-3509-owner".to_owned())
        .user_id
        .clone();
    save_messaging_risk_agreement(&owner, "signal", ACCOUNT).expect("Signal enabled");
    let places =
        read_signal_conversation_places(&owner, ACCOUNT, &seeded_signal_conversation_places())
            .expect("signed-in Signal places");
    let place = places
        .iter()
        .find(|place| place.label == "SCRUB-S")
        .expect("direct place");
    let mut source = LiveSignalTranscriptRowWords::new(
        place.place_id.clone(),
        FreshSignalTranscript(vec![SignalLiveRowCapture::new(ROW_INDEX, COVER)]),
    );
    let records = vec![protected_record()];
    let mut journal = ShippingReceiveJournal::default();
    let mut eye = SignalEyeControl::default();

    println!("TASK3509_CLOSED_ROWS_BEFORE={}", eye.rows().len());
    assert_eq!(eye.rows().len(), 0);
    let accepted = receive_signal_rows_into_eye(
        &owner,
        ACCOUNT,
        place,
        SIGNED_IN_SENDER,
        &mut source,
        &records,
        &mut journal,
        &mut eye,
    )
    .expect("fresh Signal arrival reaches the shared job");
    assert_eq!(accepted, 1);
    assert_eq!(journal.services(), &[ShippingService::Signal]);
    assert_eq!(eye.rows().len(), 1);
    assert_eq!(eye.rows()[0].state, SignalEyeState::Closed);
    assert_eq!(eye.rows()[0].text, COVER);
    println!("TASK3509_RECEIVE_JOB=shipping_receive::receive_arrived_message");
    println!("TASK3509_SIGNAL_READER=read_signal_messages_for_scrub");
    println!("TASK3509_CLOSED_ROWS_AFTER={}", eye.rows().len());
    println!("TASK3509_LIVE_CARRIER_COVER={}", eye.rows()[0].text);

    let opened = open_received_signal_eye(&mut eye, &records[0].app_row_id, &records)
        .expect("open eye uses the received protected-row record");
    assert_eq!(opened.text, PRIVATE_WORDS);
    println!("TASK3509_OPEN_PRIVATE_WORDS={}", opened.text);

    let closed = eye
        .close(&records[0].app_row_id)
        .expect("close restores exact cover");
    assert_eq!(closed.text, COVER);
    println!("TASK3509_CLOSED_LIVE_CARRIER_COVER={}", closed.text);
}

#[test]
fn task_3509_blocked_arrival_leaves_the_eye_at_zero_rows() {
    let storage = Storage::new();
    keystore::set_active_account_dir(Some(storage.owner_dir()));
    let owner = keystore::generate_identity("task-3509-blocked-owner".to_owned())
        .user_id
        .clone();
    save_messaging_risk_agreement(&owner, "signal", ACCOUNT).expect("Signal enabled");
    let places =
        read_signal_conversation_places(&owner, ACCOUNT, &seeded_signal_conversation_places())
            .expect("places");
    let place = places
        .iter()
        .find(|place| place.label == "SCRUB-S")
        .expect("direct place");
    let mut source = LiveSignalTranscriptRowWords::new(
        place.place_id.clone(),
        FreshSignalTranscript(Vec::new()),
    );
    let mut journal = ShippingReceiveJournal::default();
    let mut eye = SignalEyeControl::default();
    let accepted = receive_signal_rows_into_eye(
        &owner,
        ACCOUNT,
        place,
        SIGNED_IN_SENDER,
        &mut source,
        &[protected_record()],
        &mut journal,
        &mut eye,
    )
    .expect("blocked arrival is safely empty");
    assert_eq!(accepted, 0);
    assert_eq!(eye.rows().len(), 0);
    println!("TASK3509_BLOCKED_ARRIVAL_ROWS=0");
}
