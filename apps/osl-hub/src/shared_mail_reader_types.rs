//! The provider-neutral mail reader types (TASK 3042/3044), in their own module.
//!
//! These carried a seeded mailbox, its folders and its messages for every mail
//! service reader. They lived in `service_connections.rs` and still read from
//! there through a `pub use`, so every existing `service_connections::…` path is
//! unchanged. They moved for the same reason the owner check did in TASK 3045:
//! a reader or a deleter that only needs the mailbox shape should not have to
//! drag in that file's mail-website plumbing, which is what the iCloud deleter
//! (TASK 3073) needs.

use crate::mail_owner_check::{mail_message_who_wrote_it, MailOwnerCheckError, VisibleMailMessage};
use crate::row_who_wrote_it::SharedRowWhoWroteIt;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SharedMailLabel {
    pub label_id: String,
    pub name: String,
}

impl SharedMailLabel {
    pub fn new(label_id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            label_id: label_id.into(),
            name: name.into(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SharedMailMessageRecord {
    pub label_id: String,
    pub message_id: String,
    pub subject: String,
    pub time: i64,
    pub sender_address: Option<String>,
    pub body: String,
}

impl SharedMailMessageRecord {
    pub fn new(
        label_id: impl Into<String>,
        message_id: impl Into<String>,
        subject: impl Into<String>,
        time: i64,
        sender_address: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            label_id: label_id.into(),
            message_id: message_id.into(),
            subject: subject.into(),
            time,
            sender_address: Some(sender_address.into()),
            body: body.into(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SharedMailboxSnapshot {
    pub signed_in_address: String,
    pub labels: Vec<SharedMailLabel>,
    pub messages: Vec<SharedMailMessageRecord>,
}

impl SharedMailboxSnapshot {
    pub fn new(
        signed_in_address: impl Into<String>,
        labels: impl IntoIterator<Item = SharedMailLabel>,
        messages: impl IntoIterator<Item = SharedMailMessageRecord>,
    ) -> Self {
        Self {
            signed_in_address: signed_in_address.into(),
            labels: labels.into_iter().collect(),
            messages: messages.into_iter().collect(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SharedMailMessageSummary {
    pub label_id: String,
    pub message_id: String,
    pub subject: String,
    pub time: i64,
    pub sender: String,
    pub who_wrote_it: SharedRowWhoWroteIt,
    pub called: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct OpenedSharedMailMessage {
    pub label_id: String,
    pub message_id: String,
    pub subject: String,
    pub time: i64,
    pub sender: String,
    pub body: String,
    pub who_wrote_it: SharedRowWhoWroteIt,
    pub called: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct SharedMailboxPagingConfig {
    pub page_size: usize,
    pub pause_after_page_ms: u64,
}

impl SharedMailboxPagingConfig {
    pub const fn new(page_size: usize, pause_after_page_ms: u64) -> Self {
        Self {
            page_size,
            pause_after_page_ms,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SharedMailboxReadPage {
    pub page_number: usize,
    pub messages: Vec<SharedMailMessageSummary>,
    pub pause_after_page_ms: Option<u64>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SharedMailboxPagedRead {
    pub folder_id: String,
    pub page_size: usize,
    pub pause_after_page_ms: u64,
    pub pages: Vec<SharedMailboxReadPage>,
    pub messages_read: usize,
    pub stopped: bool,
    pub stopped_on_page: Option<usize>,
}

pub trait SharedMailboxPagingPause {
    fn pause_after_page(&mut self, page_number: usize, pause_ms: u64);
}

pub trait SharedMailboxPagingStop {
    fn should_stop(&mut self, folder_id: &str, page_number: usize, messages_read: usize) -> bool;
}

#[derive(Debug, Default)]
pub struct SharedMailboxNoopPause;

impl SharedMailboxPagingPause for SharedMailboxNoopPause {
    fn pause_after_page(&mut self, _page_number: usize, _pause_ms: u64) {}
}

#[derive(Debug, Default)]
pub struct SharedMailboxNeverStop;

impl SharedMailboxPagingStop for SharedMailboxNeverStop {
    fn should_stop(
        &mut self,
        _folder_id: &str,
        _page_number: usize,
        _messages_read: usize,
    ) -> bool {
        false
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum SharedMailboxReaderError {
    InvalidGmailMailbox,
    InvalidGmxMailbox,
    InvalidIcloudMailbox,
    InvalidOutlookWebMailbox,
    InvalidPagingConfig,
    UnknownLabel,
    UnknownFolder,
    MessageNotFound,
    DuplicateMessage,
    Owner(MailOwnerCheckError),
}

impl SharedMailboxReaderError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::InvalidGmailMailbox => "OSL: Gmail mailbox reader data is invalid",
            Self::InvalidGmxMailbox => "OSL: GMX mailbox reader data is invalid",
            Self::InvalidIcloudMailbox => "OSL: iCloud mailbox reader data is invalid",
            Self::InvalidOutlookWebMailbox => "OSL: Outlook web mailbox reader data is invalid",
            Self::InvalidPagingConfig => "OSL: shared mailbox paging config is invalid",
            Self::UnknownLabel => "OSL: Gmail label was not found",
            Self::UnknownFolder => "OSL: iCloud folder was not found",
            Self::MessageNotFound => "OSL: Gmail message was not found",
            Self::DuplicateMessage => "OSL: Gmail message id is duplicated",
            Self::Owner(error) => error.reason(),
        }
    }
}

impl From<MailOwnerCheckError> for SharedMailboxReaderError {
    fn from(error: MailOwnerCheckError) -> Self {
        Self::Owner(error)
    }
}

pub(crate) fn readable_sender(
    message: &SharedMailMessageRecord,
) -> Result<String, SharedMailboxReaderError> {
    message
        .sender_address
        .as_deref()
        .map(str::trim)
        .filter(|sender| !sender.is_empty())
        .map(str::to_owned)
        .ok_or(SharedMailboxReaderError::Owner(
            MailOwnerCheckError::SenderAddressUnreadable,
        ))
}

pub(crate) fn who_wrote_it_for_sender(
    signed_in_address: &str,
    message: &SharedMailMessageRecord,
) -> Result<SharedRowWhoWroteIt, SharedMailboxReaderError> {
    let visible = VisibleMailMessage {
        message_id: message.message_id.clone(),
        mailbox: message.label_id.clone(),
        sender_address: message.sender_address.clone(),
    };
    Ok(mail_message_who_wrote_it(signed_in_address, &visible)?)
}

pub(crate) fn legacy_mail_owner_call(who_wrote_it: SharedRowWhoWroteIt) -> &'static str {
    match who_wrote_it {
        SharedRowWhoWroteIt::Yours => "yours",
        SharedRowWhoWroteIt::Theirs | SharedRowWhoWroteIt::NotPublishedByApp => "not_yours",
    }
}

pub(crate) fn owner_call_for_sender(
    signed_in_address: &str,
    message: &SharedMailMessageRecord,
) -> Result<String, SharedMailboxReaderError> {
    Ok(legacy_mail_owner_call(who_wrote_it_for_sender(signed_in_address, message)?).to_owned())
}

pub(crate) fn shared_mail_message_summary(
    signed_in_address: &str,
    message: &SharedMailMessageRecord,
) -> Result<SharedMailMessageSummary, SharedMailboxReaderError> {
    let sender = readable_sender(message)?;
    let who_wrote_it = who_wrote_it_for_sender(signed_in_address, message)?;
    let called = legacy_mail_owner_call(who_wrote_it).to_owned();
    Ok(SharedMailMessageSummary {
        label_id: message.label_id.clone(),
        message_id: message.message_id.clone(),
        subject: message.subject.clone(),
        time: message.time,
        sender,
        who_wrote_it,
        called,
    })
}
