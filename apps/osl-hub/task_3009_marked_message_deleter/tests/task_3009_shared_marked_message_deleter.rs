//! TASK 3009 finish line: a direct command deletes one marked message the
//! account sent, refuses a marked message the account did not send with a
//! not-yours error, and refuses an unmarked message the account did send with
//! a not-marked error.

use std::process::Command;

use task_3009_marked_message_deleter::shared_marked_message_deleter::{
    delete_marked_message, SharedMarkedMessage, SharedReviewDecision,
};
use task_3009_marked_message_deleter::{ServicePlaceMessage, ServicePlaceRemover};

const SERVICE_ID: &str = "discord";
const PLACE: &str = "discord:dm:task-3009-alpha";
const SIGNED_IN_SENDER: &str = "task-3009-signed-in-account";
const OTHER_SENDER: &str = "task-3009-other-account";
const COMMAND: &str = env!("CARGO_BIN_EXE_task-3009-marked-message-deleter");

fn service_place() -> ServicePlaceRemover {
    ServicePlaceRemover::new(
        SERVICE_ID,
        PLACE,
        vec![
            ServicePlaceMessage::new("task-3009-marked-mine", SIGNED_IN_SENDER, "mine, marked"),
            ServicePlaceMessage::new("task-3009-marked-theirs", OTHER_SENDER, "theirs, marked"),
            ServicePlaceMessage::new("task-3009-unmarked-mine", SIGNED_IN_SENDER, "mine, kept"),
        ],
    )
}

fn reviewed(message_id: &str, sender: &str, decision: SharedReviewDecision) -> SharedMarkedMessage {
    SharedMarkedMessage::new(
        SERVICE_ID,
        message_id,
        PLACE,
        Some(sender.to_owned()),
        true,
        decision,
    )
}

fn run_case(case: &str) -> (i32, String) {
    let output = Command::new(COMMAND)
        .args(["--case", case])
        .output()
        .expect("run the task 3009 direct command");
    let mut text = String::from_utf8(output.stdout).expect("command stdout is utf-8");
    text.push_str(&String::from_utf8(output.stderr).expect("command stderr is utf-8"));
    (
        output.status.code().expect("command reports an exit code"),
        text,
    )
}

#[test]
fn task_3009_direct_command_deletes_a_marked_message_the_account_sent() {
    let mut place = service_place();
    let report = delete_marked_message(
        &mut place,
        SERVICE_ID,
        Some(SIGNED_IN_SENDER.to_owned()),
        reviewed(
            "task-3009-marked-mine",
            SIGNED_IN_SENDER,
            SharedReviewDecision::MarkedForDeletion,
        ),
    )
    .expect("a marked message the account sent is deleted");

    assert_eq!(report.deleted_count, 1);
    assert_eq!(report.deleted_message_ids, vec!["task-3009-marked-mine"]);
    assert_eq!(place.removal_calls(), ["task-3009-marked-mine"]);
    assert!(!place.holds("task-3009-marked-mine"));
    assert!(place.holds("task-3009-marked-theirs"));
    assert!(place.holds("task-3009-unmarked-mine"));

    let (code, text) = run_case("marked-and-mine");
    println!("{text}");
    assert_eq!(code, 0, "the direct command exits 0 on a deletion");
    assert!(
        text.contains("result=DELETED")
            && text.contains("deleted_count=1")
            && text.contains("deleted=[\"task-3009-marked-mine\"]"),
        "command must report the deletion: {text}"
    );
    assert!(
        text.contains("still_present=false"),
        "the message must be gone from the service place: {text}"
    );
}

#[test]
fn task_3009_direct_command_refuses_a_marked_message_the_account_did_not_send() {
    let mut place = service_place();
    let refused = delete_marked_message(
        &mut place,
        SERVICE_ID,
        Some(SIGNED_IN_SENDER.to_owned()),
        reviewed(
            "task-3009-marked-theirs",
            OTHER_SENDER,
            SharedReviewDecision::MarkedForDeletion,
        ),
    )
    .expect_err("a marked message the account did not send is refused");

    assert_eq!(refused.code(), "not_yours");
    assert!(refused.to_string().contains("is not yours"));
    assert!(
        place.removal_calls().is_empty(),
        "a refusal must not reach the service fill-in"
    );
    assert!(place.holds("task-3009-marked-theirs"));

    let (code, text) = run_case("marked-not-mine");
    println!("{text}");
    assert_eq!(code, 1, "the direct command exits 1 on a refusal");
    assert!(
        text.contains("result=ERR") && text.contains("code=not_yours"),
        "command must refuse with the not-yours error: {text}"
    );
    assert!(
        text.contains("service_removal_calls=[]") && text.contains("still_present=true"),
        "nothing may be removed on a not-yours refusal: {text}"
    );
}

#[test]
fn task_3009_direct_command_refuses_an_unmarked_message_the_account_did_send() {
    let mut place = service_place();
    let refused = delete_marked_message(
        &mut place,
        SERVICE_ID,
        Some(SIGNED_IN_SENDER.to_owned()),
        reviewed(
            "task-3009-unmarked-mine",
            SIGNED_IN_SENDER,
            SharedReviewDecision::Keep,
        ),
    )
    .expect_err("an unmarked message the account did send is refused");

    assert_eq!(refused.code(), "not_marked");
    assert!(refused.to_string().contains("is not marked for deletion"));
    assert!(
        place.removal_calls().is_empty(),
        "a refusal must not reach the service fill-in"
    );
    assert!(place.holds("task-3009-unmarked-mine"));

    let (code, text) = run_case("unmarked-and-mine");
    println!("{text}");
    assert_eq!(code, 1, "the direct command exits 1 on a refusal");
    assert!(
        text.contains("result=ERR") && text.contains("code=not_marked"),
        "command must refuse with the not-marked error: {text}"
    );
    assert!(
        text.contains("service_removal_calls=[]") && text.contains("still_present=true"),
        "nothing may be removed on a not-marked refusal: {text}"
    );
}

#[test]
fn task_3009_owner_check_is_the_shared_one_from_task_3001() {
    // The deleter must not answer ownership itself: an unknown sender is
    // refused by the shared check rather than guessed either way.
    let mut place = service_place();
    let mut unknown_sender = reviewed(
        "task-3009-marked-mine",
        SIGNED_IN_SENDER,
        SharedReviewDecision::MarkedForDeletion,
    );
    unknown_sender.message_sender = None;
    let refused = delete_marked_message(
        &mut place,
        SERVICE_ID,
        Some(SIGNED_IN_SENDER.to_owned()),
        unknown_sender,
    )
    .expect_err("an unknown sender is refused");

    assert_eq!(refused.code(), "owner_unknown");
    assert!(refused.to_string().contains("message sender is unknown"));
    assert!(place.removal_calls().is_empty());

    // And it is literally the TASK 3001 command.
    let yes = task_3009_marked_message_deleter::privacy_scan::did_signed_in_account_send_message(
        task_3009_marked_message_deleter::privacy_scan::MessageOwnerCheckInput {
            signed_in_account_sender: Some(SIGNED_IN_SENDER.to_owned()),
            message_sender: Some(SIGNED_IN_SENDER.to_owned()),
        },
    )
    .expect("the shared owner check answers a known pair");
    assert!(yes);
}
