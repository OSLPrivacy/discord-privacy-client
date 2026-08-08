//! TASK 3010: connect the shared deleter (TASK 3009) to the deletion record
//! (TASK 1451).
//!
//! Finish line: a run over three marked messages where one fails writes
//! three lines with three different outcomes and three places, and the
//! record has zero lines before the run.

use osl_privacy_hub::pro_marked_deletion_outcomes::record_deletion_outcomes;
use osl_privacy_hub::shared_marked_deletion_record::run_shared_marked_deletion_and_record;
use osl_privacy_hub::shared_marked_message_deleter::{
    SharedMarkedDeletionRequest, SharedMarkedMessage, SharedMarkedMessageRemover,
    SharedReviewDecision,
};

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
fn task_3010_a_run_over_three_marked_messages_where_one_fails_writes_three_outcomes_with_places() {
    let mut remover = FailingOnRemover {
        service_id: "discord".to_owned(),
        fails_on: "task-3010-failed",
        removed: Vec::new(),
    };

    // The record has zero lines before the run.
    let record_before = record_deletion_outcomes(&[]);
    assert_eq!(record_before.deleted_count, 0);
    assert_eq!(record_before.failed_count, 0);
    assert_eq!(record_before.needs_attention_count, 0);
    assert!(record_before.deleted.is_empty());
    assert!(record_before.failed.is_empty());
    assert!(record_before.needs_attention.is_empty());

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
            // Never reviewed, so the shared deleter refuses this row before
            // the service fill-in is ever called -- needs attention, not a
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

    // Three lines, three different outcomes.
    assert_eq!(report.deleted_count, 1);
    assert_eq!(report.failed_count, 1);
    assert_eq!(report.needs_attention_count, 1);
    assert_eq!(
        report.deleted_count + report.failed_count + report.needs_attention_count,
        3
    );
    assert_eq!(remover.removed, vec!["task-3010-deleted"]);

    // Three places, one per line, each still carrying its account/service.
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
        "TASK3010_BEFORE deleted_count={} failed_count={} needs_attention_count={}",
        record_before.deleted_count, record_before.failed_count, record_before.needs_attention_count
    );
    println!(
        "TASK3010_AFTER deleted_count={} failed_count={} needs_attention_count={}",
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
fn task_3010_no_second_record_is_built_the_shared_deleter_writes_the_1451_report() {
    // The connector returns exactly the TASK 1451 `DeletionOutcomeReport`
    // type -- proven by using it directly here without any adapter/mapping
    // step -- so there is no second record type in play.
    let mut remover = FailingOnRemover {
        service_id: "discord".to_owned(),
        fails_on: "unused",
        removed: Vec::new(),
    };
    let request = SharedMarkedDeletionRequest::one(
        "discord",
        Some(SIGNED_IN.to_owned()),
        message(
            "task-3010-single",
            "discord:dm:task-3010:single",
            Some(SIGNED_IN),
            true,
            SharedReviewDecision::MarkedForDeletion,
        ),
    );

    let report: osl_privacy_hub::pro_marked_deletion_outcomes::DeletionOutcomeReport =
        run_shared_marked_deletion_and_record(&mut remover, request);

    assert_eq!(report.deleted_count, 1);
    assert_eq!(report.deleted[0].message_locator, "discord:dm:task-3010:single");
}
