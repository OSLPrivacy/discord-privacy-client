//! Reply-gated effects for destructive burns and completed downloads.
//!
//! These direct operations are the mutation boundary shared by service-backed
//! callers and focused integration checks. A successful HTTP status is not an
//! instruction to mutate local state: the exact reply must pass
//! [`keystore::service_reply`] first. Keeping validation and mutation in the
//! same call prevents a caller from accidentally changing a message or
//! offering a file before checking the response.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

use keystore::service_reply::{
    validate_service_reply, ServiceReply, ServiceReplyExpectation, ServiceReplyRefusal,
};
use serde_json::json;

pub const BURN_SERVICE_OPERATION: &str = "burn";
pub const DOWNLOAD_SERVICE_OPERATION: &str = "download";

/// Identifies the exact request whose reply may authorize an item change.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServiceOperationRequest<'a> {
    pub request_id: &'a str,
    pub item_id: &'a str,
}

/// The item collections visible to direct burn and download callers.
///
/// A set models the externally important behavior: burn removes one named
/// message and download offers one named completed file. Counts are derived
/// from the collections so a refused reply cannot increment a detached
/// success counter while leaving the actual item state unchanged.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReplyGatedServiceItems {
    messages: BTreeMap<String, String>,
    completed_files: BTreeSet<String>,
}

impl ReplyGatedServiceItems {
    pub fn with_messages<I, S>(messages: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            messages: messages
                .into_iter()
                .map(|message| (message.into(), String::new()))
                .collect(),
            completed_files: BTreeSet::new(),
        }
    }

    /// Seed named message records when a caller must observe exact text as
    /// well as presence. Reply validation still happens before either the id
    /// or its text can be removed.
    pub fn with_message_records<I, Id, Text>(messages: I) -> Self
    where
        I: IntoIterator<Item = (Id, Text)>,
        Id: Into<String>,
        Text: Into<String>,
    {
        Self {
            messages: messages
                .into_iter()
                .map(|(id, text)| (id.into(), text.into()))
                .collect(),
            completed_files: BTreeSet::new(),
        }
    }

    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    pub fn completed_file_count(&self) -> usize {
        self.completed_files.len()
    }

    pub fn has_message(&self, item_id: &str) -> bool {
        self.messages.contains_key(item_id)
    }

    pub fn message_text(&self, item_id: &str) -> Option<&str> {
        self.messages.get(item_id).map(String::as_str)
    }

    pub fn offers_completed_file(&self, item_id: &str) -> bool {
        self.completed_files.contains(item_id)
    }

    /// Validate the exact burn acknowledgement before removing the message.
    pub fn burn(
        &mut self,
        request: ServiceOperationRequest<'_>,
        reply: ServiceReply<'_>,
    ) -> Result<ServiceOperationReceipt, ReplyGatedOperationError> {
        validate_operation_reply(BURN_SERVICE_OPERATION, request, reply)?;

        if self.messages.remove(request.item_id).is_none() {
            return Err(ReplyGatedOperationError::UnknownMessage {
                item_id: request.item_id.to_owned(),
            });
        }
        Ok(ServiceOperationReceipt {
            operation: BURN_SERVICE_OPERATION,
            request_id: request.request_id.to_owned(),
            item_id: request.item_id.to_owned(),
        })
    }

    /// Validate the exact download acknowledgement before offering the file.
    pub fn offer_completed_download(
        &mut self,
        request: ServiceOperationRequest<'_>,
        reply: ServiceReply<'_>,
    ) -> Result<ServiceOperationReceipt, ReplyGatedOperationError> {
        validate_operation_reply(DOWNLOAD_SERVICE_OPERATION, request, reply)?;

        if !self.completed_files.insert(request.item_id.to_owned()) {
            return Err(ReplyGatedOperationError::CompletedFileAlreadyOffered {
                item_id: request.item_id.to_owned(),
            });
        }
        Ok(ServiceOperationReceipt {
            operation: DOWNLOAD_SERVICE_OPERATION,
            request_id: request.request_id.to_owned(),
            item_id: request.item_id.to_owned(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceOperationReceipt {
    pub operation: &'static str,
    pub request_id: String,
    pub item_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReplyGatedOperationError {
    ReplyRefused(ServiceReplyRefusal),
    UnknownMessage { item_id: String },
    CompletedFileAlreadyOffered { item_id: String },
}

impl ReplyGatedOperationError {
    /// Stable name for reply refusals and local item-state failures.
    pub fn name(&self) -> &str {
        match self {
            Self::ReplyRefused(refusal) => refusal.name(),
            Self::UnknownMessage { .. } => "unknown-message",
            Self::CompletedFileAlreadyOffered { .. } => "completed-file-already-offered",
        }
    }
}

impl fmt::Display for ReplyGatedOperationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReplyRefused(refusal) => refusal.fmt(formatter),
            Self::UnknownMessage { item_id } => {
                write!(formatter, "unknown-message: {item_id}")
            }
            Self::CompletedFileAlreadyOffered { item_id } => {
                write!(formatter, "completed-file-already-offered: {item_id}")
            }
        }
    }
}

impl Error for ReplyGatedOperationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ReplyRefused(refusal) => Some(refusal),
            Self::UnknownMessage { .. } | Self::CompletedFileAlreadyOffered { .. } => None,
        }
    }
}

impl From<ServiceReplyRefusal> for ReplyGatedOperationError {
    fn from(refusal: ServiceReplyRefusal) -> Self {
        Self::ReplyRefused(refusal)
    }
}

fn validate_operation_reply(
    operation: &'static str,
    request: ServiceOperationRequest<'_>,
    reply: ServiceReply<'_>,
) -> Result<(), ServiceReplyRefusal> {
    let expected = ServiceReplyExpectation::json(
        "application/json",
        [
            ("accepted", json!(true)),
            ("item_id", json!(request.item_id)),
            ("operation", json!(operation)),
            ("request_id", json!(request.request_id)),
        ],
    );
    validate_service_reply(reply, &expected).map(|_| ())
}
