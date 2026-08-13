//! Shipping composition for the X reader, one receive job, and X eye state.
//!
//! This module deliberately owns no browser traversal.  It asks the shared X
//! reader from `services` for the selected live conversation, then gives the
//! authenticated row to the one shipping receive job.  Only that job's output
//! is admitted to the eye.

use crate::broker::ProtectedRowRecord;
use crate::services::{read_x_shared_messages, SharedConversationMessage, XBrowserMachine};
use crate::shipping_receive::{self, ArrivedMessageRow, ShippingReceiveJournal};
use crate::x_eye_state::{
    XEyeStateStore, XReceivedFeed, XReceivingJobOutput, XReceivingJobRow, XRowKind,
};

/// The private words opened by OSL for one authenticated, peer-authored X row.
/// The browser reader supplies the cover separately; callers cannot seed it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XProtectedArrival {
    pub message_id: String,
    pub private_words: String,
    pub receiving_job_run_id: String,
}

fn received_x_row(
    owner_osl_user_id: &str,
    account_id: &str,
    place_id: &str,
    browser: &XBrowserMachine,
    arrival: &XProtectedArrival,
) -> Result<SharedConversationMessage, String> {
    if arrival.private_words.is_empty() || arrival.receiving_job_run_id.trim().is_empty() {
        return Err("X protected arrival is incomplete".to_owned());
    }
    let rows = read_x_shared_messages(owner_osl_user_id, account_id, place_id, browser)?;
    rows.into_iter()
        .find(|row| row.message_id == arrival.message_id && !row.yours && !row.text.is_empty())
        .ok_or_else(|| "X live reader did not return the peer-authored arrival row".to_owned())
}

/// Feed exactly one shared-reader X row through the sole shipping receive job
/// and append it to the eye.  No X-specific browser reader is created here.
pub fn receive_x_arrival_for_eye(
    owner_osl_user_id: &str,
    account_id: &str,
    place_id: &str,
    browser: &XBrowserMachine,
    arrival: XProtectedArrival,
    journal: &mut ShippingReceiveJournal,
    eye: &mut XEyeStateStore,
) -> Result<(), String> {
    let row = received_x_row(owner_osl_user_id, account_id, place_id, browser, &arrival)?;
    shipping_receive::receive_arrived_message(
        ArrivedMessageRow::x(row.message_id.clone(), (row, arrival)),
        journal,
        |row| {
            let (observed, arrival) = row.payload;
            // This is the common protected-row record introduced by 3504.  X
            // supplies its provider row identity and cover through 3029; the
            // private words are the authenticated receive result.
            let record = ProtectedRowRecord {
                app_id: "x".to_owned(),
                app_row_id: observed.message_id,
                cover_text: observed.text,
                private_words: arrival.private_words,
                opened: true,
            };
            let output = XReceivingJobOutput {
                run_id: arrival.receiving_job_run_id,
                rows: vec![XReceivingJobRow {
                    kind: XRowKind::DirectMessage,
                    marker: record.app_row_id,
                    normal_text: record.cover_text,
                    protected_text: record.private_words,
                }],
            };
            let feed = XReceivedFeed::recorded_by(&output);
            eye.append_from_receiving_job(&output, &feed)
                .map_err(|error| error.to_string())
        },
    )
}
