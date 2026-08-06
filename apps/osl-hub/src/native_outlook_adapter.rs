//! Outlook desktop read-only adapter pieces.
//!
//! The mailbox reader is deliberately local and read-only. It adapts Outlook
//! desktop message facts into the shared mailbox reader contract used by Scrub.

use crate::services::{
    open_shared_mailbox_message, read_shared_mailbox_folders, read_shared_mailbox_messages,
    MailboxFolderCandidate, MailboxMessageCandidate, MailboxReaderSnapshot, SharedMailboxFolder,
    SharedMailboxMessage, SharedMailboxMessageSummary,
};

pub const OUTLOOK_DESKTOP_MAIL_READER_ID: &str = "outlook-desktop-shared-mailbox-reader";
pub const OUTLOOK_DESKTOP_SERVICE_ID: &str = "outlook";
pub const OUTLOOK_DESKTOP_SEEDED_OWNER: &str = "osl_task_3053_owner";
pub const OUTLOOK_DESKTOP_SEEDED_ACCOUNT: &str = "outlook-desktop-scrub";
pub const OUTLOOK_DESKTOP_SEEDED_SIGNED_IN_ADDRESS: &str = "scrub.owner@example.test";
pub const OUTLOOK_DESKTOP_MINE_MARKER: &str = "SCRUB-OD-MINE";

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct OutlookDesktopMailbox {
    owner_osl_user_id: String,
    account_id: String,
    signed_in_address: String,
    snapshot: MailboxReaderSnapshot,
}

impl OutlookDesktopMailbox {
    pub fn new(
        owner_osl_user_id: impl Into<String>,
        account_id: impl Into<String>,
        signed_in_address: impl Into<String>,
        snapshot: MailboxReaderSnapshot,
    ) -> Self {
        Self {
            owner_osl_user_id: owner_osl_user_id.into(),
            account_id: account_id.into(),
            signed_in_address: signed_in_address.into(),
            snapshot,
        }
    }

    pub fn signed_in_address(&self) -> &str {
        &self.signed_in_address
    }

    pub fn read_folders(&self) -> Result<Vec<SharedMailboxFolder>, String> {
        read_shared_mailbox_folders(
            &self.owner_osl_user_id,
            OUTLOOK_DESKTOP_SERVICE_ID,
            &self.account_id,
            &self.snapshot,
        )
    }

    pub fn read_messages(
        &self,
        folder_id: &str,
    ) -> Result<Vec<SharedMailboxMessageSummary>, String> {
        read_shared_mailbox_messages(
            &self.owner_osl_user_id,
            OUTLOOK_DESKTOP_SERVICE_ID,
            &self.account_id,
            folder_id,
            &self.snapshot,
        )
    }

    pub fn open_message(
        &self,
        folder_id: &str,
        message_id: &str,
    ) -> Result<SharedMailboxMessage, String> {
        open_shared_mailbox_message(
            &self.owner_osl_user_id,
            OUTLOOK_DESKTOP_SERVICE_ID,
            &self.account_id,
            folder_id,
            message_id,
            &self.snapshot,
        )
    }
}

pub fn seeded_outlook_desktop_scrub_mailbox() -> OutlookDesktopMailbox {
    OutlookDesktopMailbox::new(
        OUTLOOK_DESKTOP_SEEDED_OWNER,
        OUTLOOK_DESKTOP_SEEDED_ACCOUNT,
        OUTLOOK_DESKTOP_SEEDED_SIGNED_IN_ADDRESS,
        MailboxReaderSnapshot::new(
            [
                MailboxFolderCandidate::new("Inbox", "Inbox"),
                MailboxFolderCandidate::new("Sent Items", "Sent Items"),
                MailboxFolderCandidate::new("Archive", "Archive"),
                MailboxFolderCandidate::new("Deleted Items", "Deleted Items"),
            ],
            [
                MailboxMessageCandidate::new(
                    "Sent Items",
                    "outlook-desktop-sent-001",
                    "SCRUB-OD-MINE",
                    1_786_032_000,
                    OUTLOOK_DESKTOP_SEEDED_SIGNED_IN_ADDRESS,
                    "Outlook desktop seeded owner message.",
                ),
                MailboxMessageCandidate::new(
                    "Sent Items",
                    "outlook-desktop-sent-002",
                    "Outlook desktop cleanup receipt",
                    1_786_035_600,
                    "delegate@example.test",
                    "Second seeded Sent Items body.",
                ),
                MailboxMessageCandidate::new(
                    "Sent Items",
                    "outlook-desktop-sent-003",
                    "Outlook desktop account notice",
                    1_786_039_200,
                    "noreply@example.test",
                    "Third seeded Sent Items body.",
                ),
                MailboxMessageCandidate::new(
                    "Inbox",
                    "outlook-desktop-inbox-001",
                    "Inbox task 3053 first",
                    1_786_042_800,
                    "friend@example.test",
                    "First seeded Inbox body.",
                ),
                MailboxMessageCandidate::new(
                    "Inbox",
                    "outlook-desktop-inbox-002",
                    "Inbox task 3053 second",
                    1_786_046_400,
                    "alerts@example.test",
                    "Second seeded Inbox body.",
                ),
            ],
        ),
    )
}
