//! Signal's eye control is fed only by the one shipping arrival job.
//!
//! The transcript is read by TASK 3021's `SignalOpenScreenSource`; this module
//! deliberately has no UIA implementation or second Signal reader.  A live
//! row becomes openable only when its current carrier words match one exact
//! protected-row record produced by the receive path (TASK 3504).

use crate::broker::ProtectedRowRecord;
use crate::services::SharedConversationPlace;
use crate::shipping_receive::{self, ArrivedMessageRow, ShippingReceiveJournal};
use crate::signal_message_reader::{
    read_signal_messages_for_scrub, SharedSignalMessage, SignalOpenScreenSource,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SignalEyeState {
    Closed,
    Open,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignalEyeRow {
    pub carrier_row_id: String,
    pub carrier_cover: String,
    pub state: SignalEyeState,
    pub text: String,
}

#[derive(Default)]
pub struct SignalEyeControl {
    rows: Vec<SignalEyeRow>,
}

impl SignalEyeControl {
    pub fn rows(&self) -> &[SignalEyeRow] {
        &self.rows
    }

    pub fn close(&mut self, carrier_row_id: &str) -> Result<SignalEyeRow, String> {
        self.set_state(carrier_row_id, SignalEyeState::Closed)
    }

    fn accept_shipping_record(
        &mut self,
        message: SharedSignalMessage,
        record: ProtectedRowRecord,
    ) -> Result<(), String> {
        if record.app_id != "signal"
            || record.app_row_id != message.message_id
            || record.cover_text != message.text
            || record.private_words.is_empty()
        {
            return Err("Signal arrival does not match its protected-row record".to_owned());
        }
        if self
            .rows
            .iter()
            .any(|row| row.carrier_row_id == message.message_id)
        {
            return Err("Signal arrival row was already recorded".to_owned());
        }
        self.rows.push(SignalEyeRow {
            carrier_row_id: message.message_id,
            carrier_cover: record.cover_text.clone(),
            state: SignalEyeState::Closed,
            text: record.cover_text,
        });
        Ok(())
    }

    fn set_state(
        &mut self,
        carrier_row_id: &str,
        state: SignalEyeState,
    ) -> Result<SignalEyeRow, String> {
        let row = self
            .rows
            .iter_mut()
            .find(|row| row.carrier_row_id == carrier_row_id)
            .ok_or_else(|| "Signal eye has no received protected row".to_owned())?;
        row.state = state;
        row.text = match state {
            SignalEyeState::Closed => row.carrier_cover.clone(),
            SignalEyeState::Open => {
                return Err("Signal eye may only open a shipping record".to_owned())
            }
        };
        Ok(row.clone())
    }

    fn open_shipping_record(
        &mut self,
        carrier_row_id: &str,
        record: &ProtectedRowRecord,
    ) -> Result<SignalEyeRow, String> {
        let row = self
            .rows
            .iter_mut()
            .find(|row| row.carrier_row_id == carrier_row_id)
            .ok_or_else(|| "Signal eye has no received protected row".to_owned())?;
        if record.app_id != "signal"
            || record.app_row_id != row.carrier_row_id
            || record.cover_text != row.carrier_cover
        {
            return Err("Signal eye row is not bound to its shipping record".to_owned());
        }
        row.state = SignalEyeState::Open;
        row.text = record.private_words.clone();
        Ok(row.clone())
    }
}

/// Read the existing Signal screen with TASK 3021's reader and route each
/// matching incoming record through TASK 3505a's sole shipping receive job.
/// No caller can add an eye row directly.
pub fn receive_signal_rows_into_eye(
    owner_osl_user_id: &str,
    account_id: &str,
    selected_place: &SharedConversationPlace,
    signed_in_sender_id: &str,
    source: &mut dyn SignalOpenScreenSource,
    protected_records: &[ProtectedRowRecord],
    journal: &mut ShippingReceiveJournal,
    eye: &mut SignalEyeControl,
) -> Result<usize, String> {
    let read = read_signal_messages_for_scrub(
        owner_osl_user_id,
        account_id,
        selected_place,
        signed_in_sender_id,
        source,
    )?;
    let mut accepted = 0;
    for message in read.messages.into_iter().filter(|message| !message.yours) {
        let matches = protected_records
            .iter()
            .filter(|record| {
                record.app_id == "signal"
                    && record.app_row_id == message.message_id
                    && record.cover_text == message.text
                    && record.opened
            })
            .collect::<Vec<_>>();
        if matches.len() > 1 {
            return Err("Signal protected-row record is ambiguous".to_owned());
        }
        let Some(record) = matches.into_iter().next().cloned() else {
            continue;
        };
        shipping_receive::receive_arrived_message(
            ArrivedMessageRow::signal(message.message_id.clone(), (message, record)),
            journal,
            |row| eye.accept_shipping_record(row.payload.0, row.payload.1),
        )?;
        accepted += 1;
    }
    Ok(accepted)
}

/// The open-eye action must use the record that arrived through the receive
/// job. This is separate from `open` so there is no direct private-state call.
pub fn open_received_signal_eye(
    eye: &mut SignalEyeControl,
    carrier_row_id: &str,
    protected_records: &[ProtectedRowRecord],
) -> Result<SignalEyeRow, String> {
    let record = protected_records
        .iter()
        .filter(|record| {
            record.app_id == "signal" && record.app_row_id == carrier_row_id && record.opened
        })
        .collect::<Vec<_>>();
    match record.as_slice() {
        [record] => eye.open_shipping_record(carrier_row_id, record),
        [] => Err("Signal eye has no shipping protected-row record".to_owned()),
        _ => Err("Signal eye protected-row record is ambiguous".to_owned()),
    }
}
