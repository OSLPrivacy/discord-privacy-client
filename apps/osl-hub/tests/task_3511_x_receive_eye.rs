#![cfg(feature = "core")]

use osl_privacy_hub::services::{
    save_messaging_risk_agreement, XBrowserMachine, XBrowserMessage, XBrowserPlace,
    XBrowserPlaceKind,
};
use osl_privacy_hub::shipping_receive::{ShippingReceiveJournal, ShippingService};
use osl_privacy_hub::x_eye_state::{XEyeState, XEyeStateStore, XReceivedFeed, XReceivingJobOutput};
use osl_privacy_hub::x_shipping_eye::{receive_x_arrival_for_eye, XProtectedArrival};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const PASSWORD: &str = "task-3511-x-eye-password";
const OWNER: &str = "task-3511-owner";
const ACCOUNT: &str = "task-3511-x-account";
const PLACE: &str = "task-3511-live-x-dm";
const MESSAGE: &str = "task-3511-live-x-row";
const COVER: &str = "TASK3511 exact live X cover";
const PRIVATE: &str = "TASK3511 exact private words";

struct Storage(PathBuf);
impl Storage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-task-3511-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        keystore::set_base_dir_override(Some(root.clone()));
        let owner = root.join("owner");
        fs::create_dir(&owner).unwrap();
        keystore::set_active_account_dir(Some(owner));
        ipc::main_password::set_file_storage_key(None);
        ipc::main_password::set_main_password(&root, PASSWORD).unwrap();
        Self(root)
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

fn empty_eye() -> XEyeStateStore {
    let output = XReceivingJobOutput {
        run_id: "task-3511-empty".into(),
        rows: vec![],
    };
    XEyeStateStore::write_from_receiving_job(&output, &XReceivedFeed::recorded_by(&output)).unwrap()
}

#[test]
fn task_3511_x_shared_reader_reaches_one_shipping_job_and_eye() {
    let _storage = Storage::new();
    save_messaging_risk_agreement(OWNER, "x", ACCOUNT).unwrap();
    let browser = XBrowserMachine::new([XBrowserPlace::new(
        PLACE,
        "TASK3511 signed-in X conversation",
        XBrowserPlaceKind::DirectMessage,
    )])
    .with_messages([XBrowserMessage::new(PLACE, MESSAGE, COVER, 3511, false)]);
    let mut eye = empty_eye();
    let mut journal = ShippingReceiveJournal::default();
    assert_eq!(eye.rows().len(), 0);
    receive_x_arrival_for_eye(
        OWNER,
        ACCOUNT,
        PLACE,
        &browser,
        XProtectedArrival {
            message_id: MESSAGE.into(),
            private_words: PRIVATE.into(),
            receiving_job_run_id: "task-3511-shipping-x-receive".into(),
        },
        &mut journal,
        &mut eye,
    )
    .unwrap();
    assert_eq!(journal.services(), &[ShippingService::X]);
    assert_eq!(journal.carrier_row_ids(), &[MESSAGE]);
    assert_eq!(eye.rows().len(), 1);
    let opened = eye
        .switch_marked_row(MESSAGE, XEyeState::Protected)
        .unwrap();
    assert_eq!(opened.displayed_text(), PRIVATE);
    let closed = eye.close_marked_row_eye(MESSAGE).unwrap();
    assert_eq!(closed.displayed_text(), COVER);
    println!("TASK3511 browser_signed_in_conversation=TASK3511 signed-in X conversation shipping_rows_before=0 shipping_rows_after=1 row_id={MESSAGE} opened_private={PRIVATE:?} closed_cover={COVER:?} receive_jobs=1");
}

#[test]
fn task_3511_refuses_seeded_or_self_authored_x_rows() {
    let _storage = Storage::new();
    save_messaging_risk_agreement(OWNER, "x", ACCOUNT).unwrap();
    let browser = XBrowserMachine::new([XBrowserPlace::new(
        PLACE,
        "signed-in",
        XBrowserPlaceKind::DirectMessage,
    )])
    .with_messages([XBrowserMessage::new(PLACE, MESSAGE, COVER, 3511, true)]);
    let mut eye = empty_eye();
    let mut journal = ShippingReceiveJournal::default();
    let error = receive_x_arrival_for_eye(
        OWNER,
        ACCOUNT,
        PLACE,
        &browser,
        XProtectedArrival {
            message_id: MESSAGE.into(),
            private_words: PRIVATE.into(),
            receiving_job_run_id: "seeded".into(),
        },
        &mut journal,
        &mut eye,
    )
    .unwrap_err();
    assert_eq!(
        error,
        "X live reader did not return the peer-authored arrival row"
    );
    assert_eq!(eye.rows().len(), 0);
    assert!(journal.services().is_empty());
    println!("TASK3511 seeded_rows=0 self_authored_refusal={error:?}");
}
