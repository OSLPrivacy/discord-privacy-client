//! Connects the shared marked-message deleter
//! ([`crate::shared_marked_message_deleter`], TASK 3009) to the deletion
//! outcome record ([`crate::pro_marked_deletion_outcomes`], TASK 1451).
//!
//! The shared deleter decides, one message at a time, whether a row may be
//! removed at all and calls into a service fill-in to remove the rows that
//! clear that check; it records nothing. This module is the one place that
//! turns each of those per-row results into a
//! [`DeletionAttempt`](crate::pro_marked_deletion_outcomes::DeletionAttempt)
//! and files it with `record_deletion_outcomes` -- the record TASK 1451
//! already built. It keeps that as the only record: every row ends up in the
//! one [`DeletionOutcomeReport`](crate::pro_marked_deletion_outcomes::DeletionOutcomeReport)
//! this module returns, and nothing here defines a second one.
//!
//! A row the shared deleter refuses before ever calling the service fill-in
//! (not marked, not yours, owner unknown, wrong service, a malformed field)
//! is filed as `needs_attention`: no delete attempt against a carrier was
//! made at all, so the row is neither a clean success nor an attempt that
//! failed. A row that reaches the service fill-in and that fill-in rejects
//! is filed as `failed`. A row the fill-in accepts is `deleted`. Unlike
//! [`delete_marked_messages`](crate::shared_marked_message_deleter::delete_marked_messages),
//! one row's refusal or failure never stops the run: every row in the request
//! gets exactly one outcome.
//!
//! The shared deleter carries no separate account id, only a service id and
//! a place; the service id fills the `account_id` slot of each outcome
//! entry, and the row's `place` fills `message_locator`, so a failed or
//! needs-attention entry never loses the place it was about.

use crate::pro_marked_deletion::MarkedMessageRef;
use crate::pro_marked_deletion_outcomes::{
    record_deletion_outcomes, DeletionAttempt, DeletionAttemptResult, DeletionOutcomeReport,
};
use crate::shared_marked_message_deleter::{
    delete_marked_message, SharedMarkedDeletionError, SharedMarkedDeletionRequest,
    SharedMarkedMessage, SharedMarkedMessageRemover,
};

fn outcome_ref(message: &SharedMarkedMessage) -> MarkedMessageRef {
    MarkedMessageRef {
        account_id: message.service_id.clone(),
        message_locator: message.place.clone(),
    }
}

/// Runs the shared deleter over every message in `request` and records each
/// one's result -- deleted, failed, or needs attention -- with its place, in
/// one [`DeletionOutcomeReport`].
pub fn run_shared_marked_deletion_and_record<R>(
    remover: &mut R,
    request: SharedMarkedDeletionRequest,
) -> DeletionOutcomeReport
where
    R: SharedMarkedMessageRemover + ?Sized,
{
    let SharedMarkedDeletionRequest {
        service_id,
        signed_in_account_sender,
        messages,
    } = request;

    let mut attempts = Vec::with_capacity(messages.len());
    for message in messages {
        let outcome_message = outcome_ref(&message);
        let result = match delete_marked_message(
            remover,
            &service_id,
            signed_in_account_sender.clone(),
            message,
        ) {
            Ok(_) => DeletionAttemptResult::Deleted,
            Err(SharedMarkedDeletionError::ServiceRemovalFailed { cause, .. }) => {
                DeletionAttemptResult::Failed { reason: cause }
            }
            Err(other) => DeletionAttemptResult::NeedsAttention {
                reason: other.to_string(),
            },
        };
        attempts.push(DeletionAttempt {
            message: outcome_message,
            result,
        });
    }

    record_deletion_outcomes(&attempts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::privacy_scan::MessageOwnerCheckError;
    use crate::shared_marked_message_deleter::SharedReviewDecision;

    struct FailingOnRemover {
        service_id: String,
        fails_on: &'static str,
        removed: Vec<String>,
    }

    impl SharedMarkedMessageRemover for FailingOnRemover {
        fn service_id(&self) -> &str {
            &self.service_id
        }

        fn remove_marked_message(&mut self, message: &SharedMarkedMessage) -> Result<(), String> {
            if message.message_id == self.fails_on {
                return Err("carrier rejected the delete request".to_owned());
            }
            self.removed.push(message.message_id.clone());
            Ok(())
        }
    }

    fn message(
        id: &str,
        place: &str,
        sender: Option<&str>,
        reviewed: bool,
        decision: SharedReviewDecision,
    ) -> SharedMarkedMessage {
        SharedMarkedMessage::new(
            "discord",
            id,
            place,
            sender.map(str::to_owned),
            reviewed,
            decision,
        )
    }

    const SIGNED_IN: &str = "task-3010-signed-in-account";

    #[test]
    fn task_3010_a_run_over_three_messages_where_one_fails_writes_three_outcomes_with_places() {
        let mut remover = FailingOnRemover {
            service_id: "discord".to_owned(),
            fails_on: "task-3010-failed",
            removed: Vec::new(),
        };

        let record_before = record_deletion_outcomes(&[]);
        assert_eq!(record_before.deleted_count, 0);
        assert_eq!(record_before.failed_count, 0);
        assert_eq!(record_before.needs_attention_count, 0);

        let request = SharedMarkedDeletionRequest::new(
            "discord",
            Some(SIGNED_IN.to_owned()),
            vec![
                message(
                    "task-3010-deleted",
                    "discord:dm:task-3010:deleted",
                    Some(SIGNED_IN),
                    true,
                    SharedReviewDecision::MarkedForDeletion,
                ),
                message(
                    "task-3010-failed",
                    "discord:dm:task-3010:failed",
                    Some(SIGNED_IN),
                    true,
                    SharedReviewDecision::MarkedForDeletion,
                ),
                // Never reviewed, so the shared deleter refuses it before the
                // service fill-in is ever called -- needs attention, not a
                // service failure.
                message(
                    "task-3010-needs-attention",
                    "discord:dm:task-3010:needs-attention",
                    Some(SIGNED_IN),
                    false,
                    SharedReviewDecision::MarkedForDeletion,
                ),
            ],
        );

        let report = run_shared_marked_deletion_and_record(&mut remover, request);

        assert_eq!(report.deleted_count, 1);
        assert_eq!(report.failed_count, 1);
        assert_eq!(report.needs_attention_count, 1);
        assert_eq!(remover.removed, vec!["task-3010-deleted"]);

        assert_eq!(report.deleted[0].account_id, "discord");
        assert_eq!(
            report.deleted[0].message_locator,
            "discord:dm:task-3010:deleted"
        );

        assert_eq!(report.failed[0].account_id, "discord");
        assert_eq!(
            report.failed[0].message_locator,
            "discord:dm:task-3010:failed"
        );
        assert_eq!(
            report.failed[0].reason.as_deref(),
            Some("carrier rejected the delete request")
        );

        assert_eq!(report.needs_attention[0].account_id, "discord");
        assert_eq!(
            report.needs_attention[0].message_locator,
            "discord:dm:task-3010:needs-attention"
        );
        assert!(report.needs_attention[0].reason.is_some());

        println!(
            "TASK3010_COUNTS deleted={} failed={} needs_attention={}",
            report.deleted_count, report.failed_count, report.needs_attention_count
        );
        for entry in report
            .deleted
            .iter()
            .chain(report.failed.iter())
            .chain(report.needs_attention.iter())
        {
            println!(
                "TASK3010_LINE account={} place={} reason={:?}",
                entry.account_id, entry.message_locator, entry.reason
            );
        }
    }

    #[test]
    fn task_3010_owner_check_refusal_is_needs_attention_not_failed() {
        let mut remover = FailingOnRemover {
            service_id: "discord".to_owned(),
            fails_on: "unused",
            removed: Vec::new(),
        };
        let request = SharedMarkedDeletionRequest::one(
            "discord",
            Some(SIGNED_IN.to_owned()),
            message(
                "task-3010-not-yours",
                "discord:dm:task-3010:not-yours",
                Some("task-3010-other-account"),
                true,
                SharedReviewDecision::MarkedForDeletion,
            ),
        );

        let report = run_shared_marked_deletion_and_record(&mut remover, request);

        assert_eq!(report.deleted_count, 0);
        assert_eq!(report.failed_count, 0);
        assert_eq!(report.needs_attention_count, 1);
        assert!(remover.removed.is_empty());
    }

    #[test]
    fn owner_check_error_is_never_treated_as_a_service_failure() {
        // Not a live scenario in this module's own fixtures, but pins the
        // mapping: any refusal that is not `ServiceRemovalFailed` -- an
        // `OwnerUnknown` included -- is `needs_attention`, never `failed`.
        let error = SharedMarkedDeletionError::OwnerUnknown {
            message_id: "x".to_owned(),
            cause: MessageOwnerCheckError::UnknownMessageSender,
        };
        assert!(!matches!(
            error,
            SharedMarkedDeletionError::ServiceRemovalFailed { .. }
        ));
    }
}
