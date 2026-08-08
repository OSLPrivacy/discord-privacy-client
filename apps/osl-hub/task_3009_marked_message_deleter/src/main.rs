//! TASK 3009 direct command for the shared marked-message deleter.
//!
//! Usage:
//!   task-3009-marked-message-deleter --case marked-and-mine
//!   task-3009-marked-message-deleter --case marked-not-mine
//!   task-3009-marked-message-deleter --case unmarked-and-mine
//!
//! Exit 0 when the message was deleted, 1 when the shared deleter refused,
//! 2 on a usage error.

use task_3009_marked_message_deleter::shared_marked_message_deleter::{
    delete_marked_message, SharedMarkedMessage, SharedReviewDecision,
};
use task_3009_marked_message_deleter::{ServicePlaceMessage, ServicePlaceRemover};

const SERVICE_ID: &str = "discord";
const PLACE: &str = "discord:dm:task-3009-alpha";
const SIGNED_IN_SENDER: &str = "task-3009-signed-in-account";
const OTHER_SENDER: &str = "task-3009-other-account";

const MARKED_AND_MINE: &str = "task-3009-marked-mine";
const MARKED_NOT_MINE: &str = "task-3009-marked-theirs";
const UNMARKED_AND_MINE: &str = "task-3009-unmarked-mine";

fn service_place() -> ServicePlaceRemover {
    ServicePlaceRemover::new(
        SERVICE_ID,
        PLACE,
        vec![
            ServicePlaceMessage::new(
                MARKED_AND_MINE,
                SIGNED_IN_SENDER,
                "Marked in the review and sent by this account",
            ),
            ServicePlaceMessage::new(
                MARKED_NOT_MINE,
                OTHER_SENDER,
                "Marked in the review but sent by someone else",
            ),
            ServicePlaceMessage::new(
                UNMARKED_AND_MINE,
                SIGNED_IN_SENDER,
                "Sent by this account but kept in the review",
            ),
        ],
    )
}

/// The review's decision for each fixture message.
fn reviewed_message(case: &str) -> Option<SharedMarkedMessage> {
    let (message_id, sender, reviewed, decision) = match case {
        "marked-and-mine" => (
            MARKED_AND_MINE,
            SIGNED_IN_SENDER,
            true,
            SharedReviewDecision::MarkedForDeletion,
        ),
        "marked-not-mine" => (
            MARKED_NOT_MINE,
            OTHER_SENDER,
            true,
            SharedReviewDecision::MarkedForDeletion,
        ),
        "unmarked-and-mine" => (
            UNMARKED_AND_MINE,
            SIGNED_IN_SENDER,
            true,
            SharedReviewDecision::Keep,
        ),
        _ => return None,
    };
    Some(SharedMarkedMessage::new(
        SERVICE_ID,
        message_id,
        PLACE,
        Some(sender.to_owned()),
        reviewed,
        decision,
    ))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let case = match args.as_slice() {
        [flag, case] if flag == "--case" => case.clone(),
        _ => {
            eprintln!(
                "TASK3009_USAGE command=delete_marked_messages expected=\"--case marked-and-mine|marked-not-mine|unmarked-and-mine\""
            );
            std::process::exit(2);
        }
    };

    let Some(message) = reviewed_message(&case) else {
        eprintln!("TASK3009_USAGE command=delete_marked_messages unknown_case=\"{case}\"");
        std::process::exit(2);
    };

    let mut place = service_place();
    println!(
        "TASK3009_BEFORE command=delete_marked_messages case={case} service={SERVICE_ID} place=\"{}\" place_messages={:?}",
        place.place(),
        place.message_ids()
    );
    println!(
        "TASK3009_REVIEW command=delete_marked_messages case={case} message={} sender=\"{}\" signed_in_sender=\"{SIGNED_IN_SENDER}\" reviewed={} decision={} marked_in_review={}",
        message.message_id,
        message.message_sender.clone().unwrap_or_default(),
        message.reviewed,
        message.decision.as_str(),
        message.is_marked_in_review()
    );

    let message_id = message.message_id.clone();
    let outcome = delete_marked_message(
        &mut place,
        SERVICE_ID,
        Some(SIGNED_IN_SENDER.to_owned()),
        message,
    );

    match outcome {
        Ok(report) => {
            println!(
                "TASK3009_DELETE command=delete_marked_messages case={case} message={message_id} result=DELETED requested_count={} deleted_count={} deleted={:?}",
                report.requested_count, report.deleted_count, report.deleted_message_ids
            );
            println!(
                "TASK3009_AFTER command=delete_marked_messages case={case} place_messages={:?} service_removal_calls={:?} still_present={}",
                place.message_ids(),
                place.removal_calls(),
                place.holds(&message_id)
            );
            std::process::exit(0);
        }
        Err(error) => {
            println!(
                "TASK3009_REFUSAL command=delete_marked_messages case={case} message={message_id} result=ERR code={} error=\"{error}\"",
                error.code()
            );
            println!(
                "TASK3009_AFTER command=delete_marked_messages case={case} place_messages={:?} service_removal_calls={:?} still_present={}",
                place.message_ids(),
                place.removal_calls(),
                place.holds(&message_id)
            );
            std::process::exit(1);
        }
    }
}
