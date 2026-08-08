//! The executor half of Pro marked-message deletion, for mail accounts.
//!
//! TASK 1449 decided *whether* a deletion may run: Pro only, reviewed only,
//! and only after a final count the requester was shown. This module is the
//! step after that decision — it takes one already-reviewed mail message and
//! removes it from the local mailbox record, and it refuses any message whose
//! review decision was not "mark for deletion".
//!
//! The whole point of the `marked` flag is that it is the *only* thing that
//! separates a message the requester asked to delete from one they read and
//! kept. So this module treats an unmarked message as a refusal
//! (`message_not_marked`), never as a no-op that silently reports success, and
//! it performs every check before it mutates anything: a refused request
//! leaves the store bit-for-bit as it found it.
//!
//! Pure, like the rest of this crate: no I/O, no network, no provider
//! credentials, and no authority to reach the real mail account. The caller
//! owns the store and is responsible for persisting it.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The mail service these fixtures and records name. Kept as a constant so a
/// deletion record can never be attributed to a service by a typo.
pub const MAPLE_MAIL_SERVICE: &str = "maple-mail";

/// The reference-code prefix a maple-mail message carries in its subject and
/// body (for example `MAPLE-4172`).
const MAPLE_REFERENCE_PREFIX: &str = "MAPLE-";

/// The requester's plan. Deletion of marked messages is Pro-only (TASK 1449);
/// this is carried on the request so the executor cannot be reached by a Free
/// caller who skipped the counting command.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MailRequesterTier {
    Free,
    Pro,
}

/// One stored mail message, as the local mailbox record holds it after a
/// review session has been through it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkedMailMessage {
    pub service: String,
    pub account_id: String,
    pub mailbox: String,
    pub message_id: String,
    pub subject: String,
    pub body: String,
    pub received_at_unix_ms: i64,
    /// The review session looked at this message.
    pub reviewed: bool,
    /// The review decision was "mark for deletion". Carried separately from
    /// `reviewed` so a marked-but-unreviewed row is a refusal rather than a
    /// quiet deletion.
    pub marked: bool,
}

/// What a completed deletion did. Only `Deleted` is produced here; the failed
/// and needs-attention outcomes belong to the per-message reporting layer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MailDeletionOutcome {
    Deleted,
}

/// The record a deletion leaves behind. It names the service and the message's
/// own reference code, so the requester can tell exactly which mail went
/// without the record carrying a route back to a message that no longer
/// exists.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MailDeletionRecord {
    pub service: String,
    pub account_id: String,
    pub mailbox: String,
    pub message_id: String,
    pub subject: String,
    /// The `MAPLE-nnnn` reference read out of the message itself, or `None`
    /// when the message carries no such code. Never fabricated.
    pub reference: Option<String>,
    pub received_at_unix_ms: i64,
    pub outcome: MailDeletionOutcome,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkedMailDeleteRequest {
    pub tier: MailRequesterTier,
    pub account_id: String,
    pub mailbox: String,
    pub message_id: String,
    pub confirmed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
pub enum MarkedMailDeletionError {
    #[error("deleting marked messages is a Pro feature, so nothing was deleted")]
    ProRequired,
    #[error("that message is not in this mailbox, so nothing was deleted")]
    MessageNotFound,
    #[error("that message was never reviewed, so nothing was deleted")]
    MessageNotReviewed,
    #[error("message not marked: it was not marked for deletion, so nothing was deleted")]
    MessageNotMarked,
    #[error("the deletion was not confirmed, so nothing was deleted")]
    NotConfirmed,
}

impl MarkedMailDeletionError {
    /// A stable machine-readable code, so a caller never has to match on
    /// English to tell a refusal apart from a transport failure.
    pub fn code(self) -> &'static str {
        match self {
            Self::ProRequired => "pro_required",
            Self::MessageNotFound => "message_not_found",
            Self::MessageNotReviewed => "message_not_reviewed",
            Self::MessageNotMarked => "message_not_marked",
            Self::NotConfirmed => "not_confirmed",
        }
    }
}

/// The local mailbox record a deletion acts on, plus the deletions it has
/// already performed.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkedMailStore {
    messages: Vec<MarkedMailMessage>,
    deletions: Vec<MailDeletionRecord>,
}

impl MarkedMailStore {
    pub fn from_messages(messages: Vec<MarkedMailMessage>) -> Self {
        Self {
            messages,
            deletions: Vec::new(),
        }
    }

    pub fn messages(&self) -> &[MarkedMailMessage] {
        &self.messages
    }

    /// How many messages the mailbox record still holds.
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// How many messages this store has actually deleted. The count the finish
    /// line reads: 0 before a deletion, 1 after one.
    pub fn deleted_count(&self) -> usize {
        self.deletions.len()
    }

    pub fn deletions(&self) -> &[MailDeletionRecord] {
        &self.deletions
    }

    pub fn find(
        &self,
        account_id: &str,
        mailbox: &str,
        message_id: &str,
    ) -> Option<&MarkedMailMessage> {
        self.messages.iter().find(|message| {
            message.account_id == account_id
                && message.mailbox == mailbox
                && message.message_id == message_id
        })
    }

    /// Change one message's review decision and nothing else. This is how a
    /// caller makes an otherwise identical copy of a store that differs only
    /// in whether the message is marked. Returns false if no such message.
    pub fn set_marked(&mut self, message_id: &str, marked: bool) -> bool {
        match self
            .messages
            .iter_mut()
            .find(|message| message.message_id == message_id)
        {
            Some(message) => {
                message.marked = marked;
                true
            }
            None => false,
        }
    }
}

/// Read the `MAPLE-nnnn` reference out of a piece of message text.
pub fn maple_reference(text: &str) -> Option<String> {
    let start = text.find(MAPLE_REFERENCE_PREFIX)?;
    let digits: String = text[start + MAPLE_REFERENCE_PREFIX.len()..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    if digits.is_empty() {
        return None;
    }
    Some(format!("{MAPLE_REFERENCE_PREFIX}{digits}"))
}

/// Delete one reviewed, marked mail message.
///
/// Every refusal happens before any mutation, so a refused request leaves the
/// store exactly as it was: same messages, same bodies, same deletion count.
pub fn delete_marked_mail(
    store: &mut MarkedMailStore,
    request: &MarkedMailDeleteRequest,
) -> Result<MailDeletionRecord, MarkedMailDeletionError> {
    if request.tier != MailRequesterTier::Pro {
        return Err(MarkedMailDeletionError::ProRequired);
    }
    let index = store
        .messages
        .iter()
        .position(|message| {
            message.account_id == request.account_id
                && message.mailbox == request.mailbox
                && message.message_id == request.message_id
        })
        .ok_or(MarkedMailDeletionError::MessageNotFound)?;
    let message = &store.messages[index];
    if !message.reviewed {
        return Err(MarkedMailDeletionError::MessageNotReviewed);
    }
    if !message.marked {
        return Err(MarkedMailDeletionError::MessageNotMarked);
    }
    if !request.confirmed {
        return Err(MarkedMailDeletionError::NotConfirmed);
    }

    let message = store.messages.remove(index);
    let record = MailDeletionRecord {
        reference: maple_reference(&message.subject).or_else(|| maple_reference(&message.body)),
        service: message.service,
        account_id: message.account_id,
        mailbox: message.mailbox,
        message_id: message.message_id,
        subject: message.subject,
        received_at_unix_ms: message.received_at_unix_ms,
        outcome: MailDeletionOutcome::Deleted,
    };
    store.deletions.push(record.clone());
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(marked: bool) -> MarkedMailMessage {
        MarkedMailMessage {
            service: MAPLE_MAIL_SERVICE.to_owned(),
            account_id: "maple-account-1452".to_owned(),
            mailbox: "INBOX".to_owned(),
            message_id: "<maple-4172@maple-mail.test>".to_owned(),
            subject: "Maple order MAPLE-4172".to_owned(),
            body: "Your maple order MAPLE-4172 is on its way.".to_owned(),
            received_at_unix_ms: 1_754_000_000_000,
            reviewed: true,
            marked,
        }
    }

    fn request(confirmed: bool) -> MarkedMailDeleteRequest {
        MarkedMailDeleteRequest {
            tier: MailRequesterTier::Pro,
            account_id: "maple-account-1452".to_owned(),
            mailbox: "INBOX".to_owned(),
            message_id: "<maple-4172@maple-mail.test>".to_owned(),
            confirmed,
        }
    }

    #[test]
    fn marked_mail_is_deleted_and_recorded_once() {
        let mut store = MarkedMailStore::from_messages(vec![message(true)]);
        assert_eq!(store.deleted_count(), 0);
        let record = delete_marked_mail(&mut store, &request(true)).expect("marked mail deletes");
        assert_eq!(record.service, MAPLE_MAIL_SERVICE);
        assert_eq!(record.reference.as_deref(), Some("MAPLE-4172"));
        assert_eq!(record.outcome, MailDeletionOutcome::Deleted);
        assert_eq!(store.deleted_count(), 1);
        assert_eq!(store.message_count(), 0);
    }

    #[test]
    fn unmarked_mail_is_refused_and_left_alone() {
        let mut store = MarkedMailStore::from_messages(vec![message(false)]);
        let before = store.clone();
        let error = delete_marked_mail(&mut store, &request(true)).expect_err("unmarked refuses");
        assert_eq!(error, MarkedMailDeletionError::MessageNotMarked);
        assert_eq!(error.code(), "message_not_marked");
        assert_eq!(store, before);
    }

    #[test]
    fn a_reference_is_read_not_invented() {
        assert_eq!(
            maple_reference("Maple order MAPLE-4172").as_deref(),
            Some("MAPLE-4172")
        );
        assert_eq!(maple_reference("no reference here"), None);
        assert_eq!(maple_reference("MAPLE-"), None);
    }
}
