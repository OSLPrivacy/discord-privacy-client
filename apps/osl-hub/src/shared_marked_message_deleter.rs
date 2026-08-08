//! Shared marked-message deleter that every service fills in.
//!
//! Two separate facts decide whether a found message may be removed, and this
//! module is the one place that joins them:
//!
//! * the review decides what is *marked* (`SharedReviewDecision`), and
//! * the shared owner check in [`crate::privacy_scan`] decides what the
//!   signed-in account *sent*.
//!
//! A service adapter fills in [`SharedMarkedMessageRemover`] -- the only part
//! that touches a real service -- and never re-implements either refusal. This
//! module performs no I/O of its own and never parses or opens a place locator;
//! the locator is carried through opaquely.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::privacy_scan::{
    did_signed_in_account_send_message, MessageOwnerCheckError, MessageOwnerCheckInput,
};

/// A deletion request is a review result, not a mailbox sweep, so it is bounded
/// by the same order of magnitude the review store itself carries.
pub const MAX_MARKED_MESSAGES_PER_REQUEST: usize = 500;
const MAX_ID_BYTES: usize = 256;
const MAX_PLACE_BYTES: usize = 256;

/// What the review decided for one found message.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SharedReviewDecision {
    Keep,
    MarkedForDeletion,
}

impl SharedReviewDecision {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Keep => "keep",
            Self::MarkedForDeletion => "markedForDeletion",
        }
    }

    pub const fn is_marked_for_deletion(self) -> bool {
        matches!(self, Self::MarkedForDeletion)
    }
}

/// One reviewed message a caller is asking the deleter to remove.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SharedMarkedMessage {
    pub service_id: String,
    pub message_id: String,
    pub place: String,
    /// The sender the service reported for this message. `None` means unknown,
    /// which is refused rather than guessed.
    pub message_sender: Option<String>,
    /// Whether the review actually reached this message. A decision that was
    /// never reviewed is not a mark.
    #[serde(default)]
    pub reviewed: bool,
    pub decision: SharedReviewDecision,
}

impl SharedMarkedMessage {
    pub fn new(
        service_id: impl Into<String>,
        message_id: impl Into<String>,
        place: impl Into<String>,
        message_sender: Option<String>,
        reviewed: bool,
        decision: SharedReviewDecision,
    ) -> Self {
        Self {
            service_id: service_id.into(),
            message_id: message_id.into(),
            place: place.into(),
            message_sender,
            reviewed,
            decision,
        }
    }

    /// Marked *in the review*: reviewed, and the decision that review recorded
    /// is deletion. An unreviewed row is never treated as marked.
    pub fn is_marked_in_review(&self) -> bool {
        self.reviewed && self.decision.is_marked_for_deletion()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SharedMarkedDeletionRequest {
    pub service_id: String,
    /// The sender string the signed-in account writes as, for this service.
    pub signed_in_account_sender: Option<String>,
    pub messages: Vec<SharedMarkedMessage>,
}

impl SharedMarkedDeletionRequest {
    pub fn new(
        service_id: impl Into<String>,
        signed_in_account_sender: Option<String>,
        messages: Vec<SharedMarkedMessage>,
    ) -> Self {
        Self {
            service_id: service_id.into(),
            signed_in_account_sender,
            messages,
        }
    }

    pub fn one(
        service_id: impl Into<String>,
        signed_in_account_sender: Option<String>,
        message: SharedMarkedMessage,
    ) -> Self {
        Self::new(service_id, signed_in_account_sender, vec![message])
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedMarkedDeletionReport {
    pub service_id: String,
    pub requested_count: usize,
    pub deleted_count: usize,
    pub deleted_message_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SharedMarkedDeletionError {
    /// The message the caller named is not marked for deletion in the review.
    NotMarked {
        message_id: String,
    },
    /// The owner check says the signed-in account did not send this message.
    NotYours {
        message_id: String,
    },
    /// The owner check could not answer, so nothing is deleted on a guess.
    OwnerUnknown {
        message_id: String,
        cause: MessageOwnerCheckError,
    },
    NothingRequested,
    TooManyMessages {
        requested_count: usize,
    },
    DuplicateMessage {
        message_id: String,
    },
    WrongService {
        message_id: String,
    },
    InvalidField {
        field: &'static str,
    },
    /// The service fill-in failed on a row that passed both checks.
    ServiceRemovalFailed {
        message_id: String,
        cause: String,
    },
}

impl SharedMarkedDeletionError {
    /// Stable machine code, so a refusal is never matched on prose.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::NotMarked { .. } => "not_marked",
            Self::NotYours { .. } => "not_yours",
            Self::OwnerUnknown { .. } => "owner_unknown",
            Self::NothingRequested => "nothing_requested",
            Self::TooManyMessages { .. } => "too_many_messages",
            Self::DuplicateMessage { .. } => "duplicate_message",
            Self::WrongService { .. } => "wrong_service",
            Self::InvalidField { .. } => "invalid_field",
            Self::ServiceRemovalFailed { .. } => "service_removal_failed",
        }
    }

    pub fn message_id(&self) -> Option<&str> {
        match self {
            Self::NotMarked { message_id }
            | Self::NotYours { message_id }
            | Self::OwnerUnknown { message_id, .. }
            | Self::DuplicateMessage { message_id }
            | Self::WrongService { message_id }
            | Self::ServiceRemovalFailed { message_id, .. } => Some(message_id),
            Self::NothingRequested | Self::TooManyMessages { .. } | Self::InvalidField { .. } => {
                None
            }
        }
    }
}

impl std::fmt::Display for SharedMarkedDeletionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotMarked { message_id } => write!(
                f,
                "message {message_id} is not marked for deletion in the review"
            ),
            Self::NotYours { message_id } => write!(
                f,
                "message {message_id} is not yours: the signed-in account did not send it"
            ),
            Self::OwnerUnknown { message_id, cause } => {
                write!(f, "message {message_id} owner check refused: {cause}")
            }
            Self::NothingRequested => f.write_str("no marked message was requested for deletion"),
            Self::TooManyMessages { requested_count } => write!(
                f,
                "{requested_count} messages exceed the {MAX_MARKED_MESSAGES_PER_REQUEST}-message deletion limit"
            ),
            Self::DuplicateMessage { message_id } => {
                write!(f, "message {message_id} is listed twice")
            }
            Self::WrongService { message_id } => write!(
                f,
                "message {message_id} belongs to a different service than the request"
            ),
            Self::InvalidField { field } => write!(f, "deletion request {field} is invalid"),
            Self::ServiceRemovalFailed { message_id, cause } => {
                write!(f, "service could not remove message {message_id}: {cause}")
            }
        }
    }
}

impl std::error::Error for SharedMarkedDeletionError {}

/// The per-service half. Discord, Telegram, Signal and the mail adapters each
/// fill this in with their own removal; nothing here decides *whether* a
/// message may go.
pub trait SharedMarkedMessageRemover {
    /// The service this fill-in speaks for, checked against every row.
    fn service_id(&self) -> &str;

    /// Remove one message that the shared deleter has already cleared.
    fn remove_marked_message(&mut self, message: &SharedMarkedMessage) -> Result<(), String>;
}

/// Delete exactly the messages that the review marked and the owner check
/// attributes to the signed-in account.
///
/// Every row is checked before any row is removed, so a refusal leaves the
/// service untouched. Checked in this order per row: shape, service, marked in
/// review, then owner. Marked-ness is checked first because the review is what
/// puts a message in scope at all -- a row the review never marked is not a
/// message this command is about, whoever sent it.
pub fn delete_marked_messages<R>(
    remover: &mut R,
    request: SharedMarkedDeletionRequest,
) -> Result<SharedMarkedDeletionReport, SharedMarkedDeletionError>
where
    R: SharedMarkedMessageRemover + ?Sized,
{
    let SharedMarkedDeletionRequest {
        service_id,
        signed_in_account_sender,
        messages,
    } = request;

    if !valid_id(&service_id) {
        return Err(SharedMarkedDeletionError::InvalidField {
            field: "service id",
        });
    }
    if service_id != remover.service_id() {
        return Err(SharedMarkedDeletionError::InvalidField {
            field: "service id for this remover",
        });
    }
    if messages.is_empty() {
        return Err(SharedMarkedDeletionError::NothingRequested);
    }
    if messages.len() > MAX_MARKED_MESSAGES_PER_REQUEST {
        return Err(SharedMarkedDeletionError::TooManyMessages {
            requested_count: messages.len(),
        });
    }

    let mut seen = BTreeSet::new();
    for message in &messages {
        if !valid_id(&message.message_id) {
            return Err(SharedMarkedDeletionError::InvalidField {
                field: "message id",
            });
        }
        if message.place.is_empty()
            || message.place.len() > MAX_PLACE_BYTES
            || message.place.contains('\0')
        {
            return Err(SharedMarkedDeletionError::InvalidField { field: "place" });
        }
        if !seen.insert(message.message_id.clone()) {
            return Err(SharedMarkedDeletionError::DuplicateMessage {
                message_id: message.message_id.clone(),
            });
        }
        if message.service_id != service_id {
            return Err(SharedMarkedDeletionError::WrongService {
                message_id: message.message_id.clone(),
            });
        }
        if !message.is_marked_in_review() {
            return Err(SharedMarkedDeletionError::NotMarked {
                message_id: message.message_id.clone(),
            });
        }
        let sent_by_signed_in_account =
            did_signed_in_account_send_message(MessageOwnerCheckInput {
                signed_in_account_sender: signed_in_account_sender.clone(),
                message_sender: message.message_sender.clone(),
            })
            .map_err(|cause| SharedMarkedDeletionError::OwnerUnknown {
                message_id: message.message_id.clone(),
                cause,
            })?;
        if !sent_by_signed_in_account {
            return Err(SharedMarkedDeletionError::NotYours {
                message_id: message.message_id.clone(),
            });
        }
    }

    let mut deleted_message_ids = Vec::with_capacity(messages.len());
    for message in &messages {
        remover.remove_marked_message(message).map_err(|cause| {
            SharedMarkedDeletionError::ServiceRemovalFailed {
                message_id: message.message_id.clone(),
                cause,
            }
        })?;
        deleted_message_ids.push(message.message_id.clone());
    }

    Ok(SharedMarkedDeletionReport {
        service_id,
        requested_count: messages.len(),
        deleted_count: deleted_message_ids.len(),
        deleted_message_ids,
    })
}

/// One-message form of [`delete_marked_messages`], for the direct command.
pub fn delete_marked_message<R>(
    remover: &mut R,
    service_id: &str,
    signed_in_account_sender: Option<String>,
    message: SharedMarkedMessage,
) -> Result<SharedMarkedDeletionReport, SharedMarkedDeletionError>
where
    R: SharedMarkedMessageRemover + ?Sized,
{
    delete_marked_messages(
        remover,
        SharedMarkedDeletionRequest::one(service_id, signed_in_account_sender, message),
    )
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && !value.contains('\0')
        && value.chars().all(|c| !c.is_control())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct RecordingRemover {
        service_id: String,
        removed: Vec<String>,
        fail_with: Option<String>,
    }

    impl RecordingRemover {
        fn new(service_id: &str) -> Self {
            Self {
                service_id: service_id.to_owned(),
                removed: Vec::new(),
                fail_with: None,
            }
        }
    }

    impl SharedMarkedMessageRemover for RecordingRemover {
        fn service_id(&self) -> &str {
            &self.service_id
        }

        fn remove_marked_message(&mut self, message: &SharedMarkedMessage) -> Result<(), String> {
            if let Some(cause) = &self.fail_with {
                return Err(cause.clone());
            }
            self.removed.push(message.message_id.clone());
            Ok(())
        }
    }

    const SIGNED_IN: &str = "task-3009-signed-in-account";
    const SOMEONE_ELSE: &str = "task-3009-other-account";

    fn message(
        id: &str,
        sender: &str,
        reviewed: bool,
        decision: SharedReviewDecision,
    ) -> SharedMarkedMessage {
        SharedMarkedMessage::new(
            "discord",
            id,
            format!("discord:dm:task-3009:{id}"),
            Some(sender.to_owned()),
            reviewed,
            decision,
        )
    }

    #[test]
    fn task_3009_deletes_a_marked_message_the_account_sent() {
        let mut remover = RecordingRemover::new("discord");
        let report = delete_marked_message(
            &mut remover,
            "discord",
            Some(SIGNED_IN.to_owned()),
            message(
                "task-3009-marked-mine",
                SIGNED_IN,
                true,
                SharedReviewDecision::MarkedForDeletion,
            ),
        )
        .expect("a marked message the account sent is deleted");

        assert_eq!(report.deleted_count, 1);
        assert_eq!(report.deleted_message_ids, vec!["task-3009-marked-mine"]);
        assert_eq!(remover.removed, vec!["task-3009-marked-mine"]);
    }

    #[test]
    fn task_3009_refuses_a_marked_message_the_account_did_not_send() {
        let mut remover = RecordingRemover::new("discord");
        let refused = delete_marked_message(
            &mut remover,
            "discord",
            Some(SIGNED_IN.to_owned()),
            message(
                "task-3009-marked-theirs",
                SOMEONE_ELSE,
                true,
                SharedReviewDecision::MarkedForDeletion,
            ),
        )
        .expect_err("a marked message someone else sent is refused");

        assert_eq!(refused.code(), "not_yours");
        assert!(remover.removed.is_empty());
    }

    #[test]
    fn task_3009_refuses_an_unmarked_message_the_account_did_send() {
        let mut remover = RecordingRemover::new("discord");
        let refused = delete_marked_message(
            &mut remover,
            "discord",
            Some(SIGNED_IN.to_owned()),
            message(
                "task-3009-unmarked-mine",
                SIGNED_IN,
                true,
                SharedReviewDecision::Keep,
            ),
        )
        .expect_err("an unmarked message is refused");

        assert_eq!(refused.code(), "not_marked");
        assert!(remover.removed.is_empty());
    }

    #[test]
    fn a_marked_but_unreviewed_row_is_not_a_mark() {
        let mut remover = RecordingRemover::new("discord");
        let refused = delete_marked_message(
            &mut remover,
            "discord",
            Some(SIGNED_IN.to_owned()),
            message(
                "task-3009-unreviewed",
                SIGNED_IN,
                false,
                SharedReviewDecision::MarkedForDeletion,
            ),
        )
        .expect_err("an unreviewed row is refused");

        assert_eq!(refused.code(), "not_marked");
        assert!(remover.removed.is_empty());
    }

    #[test]
    fn an_unknown_sender_is_refused_rather_than_guessed() {
        let mut remover = RecordingRemover::new("discord");
        let mut unknown = message(
            "task-3009-unknown-sender",
            SIGNED_IN,
            true,
            SharedReviewDecision::MarkedForDeletion,
        );
        unknown.message_sender = None;
        let refused =
            delete_marked_message(&mut remover, "discord", Some(SIGNED_IN.to_owned()), unknown)
                .expect_err("an unknown sender is refused");

        assert_eq!(refused.code(), "owner_unknown");
        assert!(remover.removed.is_empty());
    }

    #[test]
    fn one_refused_row_leaves_the_whole_batch_undeleted() {
        let mut remover = RecordingRemover::new("discord");
        let refused = delete_marked_messages(
            &mut remover,
            SharedMarkedDeletionRequest::new(
                "discord",
                Some(SIGNED_IN.to_owned()),
                vec![
                    message(
                        "task-3009-batch-mine",
                        SIGNED_IN,
                        true,
                        SharedReviewDecision::MarkedForDeletion,
                    ),
                    message(
                        "task-3009-batch-theirs",
                        SOMEONE_ELSE,
                        true,
                        SharedReviewDecision::MarkedForDeletion,
                    ),
                ],
            ),
        )
        .expect_err("a batch with one not-yours row is refused whole");

        assert_eq!(refused.code(), "not_yours");
        assert_eq!(
            refused.message_id(),
            Some("task-3009-batch-theirs"),
            "the refusal names the offending row"
        );
        assert!(remover.removed.is_empty());
    }

    #[test]
    fn a_service_fill_in_failure_is_reported_not_swallowed() {
        let mut remover = RecordingRemover::new("discord");
        remover.fail_with = Some("service refused the removal".to_owned());
        let refused = delete_marked_message(
            &mut remover,
            "discord",
            Some(SIGNED_IN.to_owned()),
            message(
                "task-3009-service-failure",
                SIGNED_IN,
                true,
                SharedReviewDecision::MarkedForDeletion,
            ),
        )
        .expect_err("a failing fill-in is reported");

        assert_eq!(refused.code(), "service_removal_failed");
    }

    #[test]
    fn a_row_from_another_service_is_refused() {
        let mut remover = RecordingRemover::new("discord");
        let mut foreign = message(
            "task-3009-foreign",
            SIGNED_IN,
            true,
            SharedReviewDecision::MarkedForDeletion,
        );
        foreign.service_id = "telegram".to_owned();
        let refused =
            delete_marked_message(&mut remover, "discord", Some(SIGNED_IN.to_owned()), foreign)
                .expect_err("a row from another service is refused");

        assert_eq!(refused.code(), "wrong_service");
        assert!(remover.removed.is_empty());
    }
}
