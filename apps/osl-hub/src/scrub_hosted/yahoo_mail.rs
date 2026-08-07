//! Yahoo Mail fill-in for the shared mailbox reader.

use crate::services::{
    mail_message_is_owned_by_signed_in_address, open_shared_mailbox_message,
    read_shared_mailbox_folders, read_shared_mailbox_messages, MailboxReaderSnapshot,
    SharedMailboxFolder, SharedMailboxMessage, SharedMailboxMessageSummary, VisibleMailMessage,
};

pub const YAHOO_MAIL_SERVICE_ID: &str = "yahoo";

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct YahooMailboxMessage {
    pub summary: SharedMailboxMessageSummary,
    pub yours: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct YahooMailboxRead {
    pub folders: Vec<SharedMailboxFolder>,
    pub sent: Vec<YahooMailboxMessage>,
    pub inbox: Vec<YahooMailboxMessage>,
}

pub fn read_yahoo_mailbox_for_scrub(
    owner_osl_user_id: &str,
    account_id: &str,
    signed_in_address: &str,
    yahoo_filled_mailbox: &MailboxReaderSnapshot,
) -> Result<YahooMailboxRead, String> {
    Ok(YahooMailboxRead {
        folders: read_shared_mailbox_folders(
            owner_osl_user_id,
            YAHOO_MAIL_SERVICE_ID,
            account_id,
            yahoo_filled_mailbox,
        )?,
        sent: read_yahoo_mailbox_folder_for_scrub(
            owner_osl_user_id,
            account_id,
            signed_in_address,
            "Sent",
            yahoo_filled_mailbox,
        )?,
        inbox: read_yahoo_mailbox_folder_for_scrub(
            owner_osl_user_id,
            account_id,
            signed_in_address,
            "Inbox",
            yahoo_filled_mailbox,
        )?,
    })
}

pub fn read_yahoo_mailbox_folder_for_scrub(
    owner_osl_user_id: &str,
    account_id: &str,
    signed_in_address: &str,
    folder_id: &str,
    yahoo_filled_mailbox: &MailboxReaderSnapshot,
) -> Result<Vec<YahooMailboxMessage>, String> {
    read_shared_mailbox_messages(
        owner_osl_user_id,
        YAHOO_MAIL_SERVICE_ID,
        account_id,
        folder_id,
        yahoo_filled_mailbox,
    )?
    .into_iter()
    .map(|summary| {
        let yours = mail_message_is_owned_by_signed_in_address(
            signed_in_address,
            &VisibleMailMessage {
                message_id: summary.message_id.clone(),
                mailbox: summary.folder_id.clone(),
                sender_address: Some(summary.sender.clone()),
            },
        )
        .map_err(|error| error.reason().to_owned())?;
        Ok(YahooMailboxMessage { summary, yours })
    })
    .collect()
}

pub fn open_yahoo_mailbox_message_for_scrub(
    owner_osl_user_id: &str,
    account_id: &str,
    folder_id: &str,
    message_id: &str,
    yahoo_filled_mailbox: &MailboxReaderSnapshot,
) -> Result<SharedMailboxMessage, String> {
    open_shared_mailbox_message(
        owner_osl_user_id,
        YAHOO_MAIL_SERVICE_ID,
        account_id,
        folder_id,
        message_id,
        yahoo_filled_mailbox,
    )
}
