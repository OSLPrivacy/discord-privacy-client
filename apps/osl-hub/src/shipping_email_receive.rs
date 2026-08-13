//! Email's adapter into the one shipping receive job.
//!
//! The provider snapshot is read only through task 3042's shared mailbox
//! reader.  The opaque provider message UID is the carrier row identity; the
//! subject stays as the cover and the opened body is the exact private words.

use crate::broker::ProtectedRowRecord;
use crate::services::{open_shared_mailbox_message, MailboxReaderSnapshot};
use crate::shipping_receive::{
    self, ArrivedMessageRow, ShippingProtectedOverlay, ShippingReceiveJournal,
};

pub fn route_provider_email_arrival(
    owner_osl_user_id: &str,
    service_id: &str,
    account_id: &str,
    folder_id: &str,
    provider_uid: &str,
    mailbox_network: Option<&MailboxReaderSnapshot>,
    journal: &mut ShippingReceiveJournal,
    overlay: &mut ShippingProtectedOverlay,
) -> Result<(), String> {
    let mailbox_network =
        mailbox_network.ok_or_else(|| "provider mailbox network is unavailable".to_owned())?;
    let message = open_shared_mailbox_message(
        owner_osl_user_id,
        service_id,
        account_id,
        folder_id,
        provider_uid,
        mailbox_network,
    )?;
    let record = ProtectedRowRecord {
        app_id: "email".to_owned(),
        app_row_id: format!("{service_id}:{account_id}:{provider_uid}"),
        cover_text: message.subject,
        private_words: message.body,
        opened: true,
    };
    let row_id = record.app_row_id.clone();
    shipping_receive::receive_arrived_message(
        ArrivedMessageRow::email(row_id, record),
        journal,
        |row| overlay.record_opened(row),
    )
}
