//! The provider-neutral mail snapshot the shared mailbox readers read.
//!
//! One [`SharedMailboxSnapshot`] is one signed-in mail account: the labels or
//! folders the service shows, and the messages sitting in them. Gate 3047's
//! Gmail reader reads exactly this shape, and the Gmail fill-in of the shared
//! mail deleter (TASK 3049) writes to exactly this shape, so a delete acts on
//! the same mailbox the review read.
//!
//! These types lived in `service_connections.rs` and still read from there
//! through a `pub use`, so every existing `service_connections::…` path is
//! unchanged. They moved for the same reason the mail owner check moved in
//! TASK 3045: the deleter needs the mailbox shape and none of the mail-website
//! plumbing that file also carries.

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
