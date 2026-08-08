//! TASK 1452 - break unmarked deletion.
//!
//! One maple-mail message carrying the reference `MAPLE-4172` is deleted from a
//! store where it is marked for deletion. An equal clean copy of that store is
//! then built in which the *only* difference is `marked: yes -> no`, and the
//! same deletion is attempted against it. The unmarked copy must be refused as
//! "message not marked", must still read MAPLE-4172 afterwards, and must not
//! disturb the record or the count the good deletion produced.
//!
//! The two copies are not asserted to be equal by eye: `store_differences`
//! walks both serialized stores and the test fails unless the single differing
//! field is `marked`. Without that, "an equal clean copy" would be a claim
//! rather than a check.

use message_lifecycle::marked_mail_deletion::{
    delete_marked_mail, maple_reference, MailDeletionOutcome, MailDeletionRecord,
    MailRequesterTier, MarkedMailDeleteRequest, MarkedMailDeletionError, MarkedMailMessage,
    MarkedMailStore, MAPLE_MAIL_SERVICE,
};
use serde_json::Value;

const REFERENCE: &str = "MAPLE-4172";
const ACCOUNT_ID: &str = "maple-account-1452";
const MAILBOX: &str = "INBOX";
const MESSAGE_ID: &str = "<maple-4172@maple-mail.test>";
const SUBJECT: &str = "Maple order MAPLE-4172";
const BODY: &str = "Your maple order MAPLE-4172 ships Tuesday. Reply to change the address.";
const RECEIVED_AT_UNIX_MS: i64 = 1_754_000_000_000;

/// The maple-mail store as the review session leaves it: one message,
/// reviewed, and marked for deletion.
fn marked_yes_store() -> MarkedMailStore {
    MarkedMailStore::from_messages(vec![MarkedMailMessage {
        service: MAPLE_MAIL_SERVICE.to_owned(),
        account_id: ACCOUNT_ID.to_owned(),
        mailbox: MAILBOX.to_owned(),
        message_id: MESSAGE_ID.to_owned(),
        subject: SUBJECT.to_owned(),
        body: BODY.to_owned(),
        received_at_unix_ms: RECEIVED_AT_UNIX_MS,
        reviewed: true,
        marked: true,
    }])
}

/// An equal clean copy of the store above with exactly one field changed.
fn marked_no_store() -> MarkedMailStore {
    let mut store = marked_yes_store();
    assert!(
        store.set_marked(MESSAGE_ID, false),
        "the clean copy must contain the same message"
    );
    store
}

fn pro_delete_request() -> MarkedMailDeleteRequest {
    MarkedMailDeleteRequest {
        tier: MailRequesterTier::Pro,
        account_id: ACCOUNT_ID.to_owned(),
        mailbox: MAILBOX.to_owned(),
        message_id: MESSAGE_ID.to_owned(),
        confirmed: true,
    }
}

fn expected_record() -> MailDeletionRecord {
    MailDeletionRecord {
        service: MAPLE_MAIL_SERVICE.to_owned(),
        account_id: ACCOUNT_ID.to_owned(),
        mailbox: MAILBOX.to_owned(),
        message_id: MESSAGE_ID.to_owned(),
        subject: SUBJECT.to_owned(),
        reference: Some(REFERENCE.to_owned()),
        received_at_unix_ms: RECEIVED_AT_UNIX_MS,
        outcome: MailDeletionOutcome::Deleted,
    }
}

fn json(store: &MarkedMailStore) -> Value {
    serde_json::to_value(store).expect("a store serializes")
}

/// Every leaf path where two JSON values disagree, as `path=left->right`.
fn differences(left: &Value, right: &Value, path: &str, found: &mut Vec<String>) {
    match (left, right) {
        (Value::Object(left_map), Value::Object(right_map)) => {
            let mut keys: Vec<&String> = left_map.keys().chain(right_map.keys()).collect();
            keys.sort();
            keys.dedup();
            for key in keys {
                let child = if path.is_empty() {
                    key.to_string()
                } else {
                    format!("{path}.{key}")
                };
                match (left_map.get(key), right_map.get(key)) {
                    (Some(a), Some(b)) => differences(a, b, &child, found),
                    (a, b) => found.push(format!(
                        "{child}={}->{}",
                        a.map(ToString::to_string).unwrap_or("<absent>".to_owned()),
                        b.map(ToString::to_string).unwrap_or("<absent>".to_owned())
                    )),
                }
            }
        }
        (Value::Array(left_items), Value::Array(right_items)) => {
            for index in 0..left_items.len().max(right_items.len()) {
                let child = format!("{path}[{index}]");
                match (left_items.get(index), right_items.get(index)) {
                    (Some(a), Some(b)) => differences(a, b, &child, found),
                    (a, b) => found.push(format!(
                        "{child}={}->{}",
                        a.map(ToString::to_string).unwrap_or("<absent>".to_owned()),
                        b.map(ToString::to_string).unwrap_or("<absent>".to_owned())
                    )),
                }
            }
        }
        (a, b) => {
            if a != b {
                found.push(format!("{path}={a}->{b}"));
            }
        }
    }
}

fn store_differences(left: &MarkedMailStore, right: &MarkedMailStore) -> Vec<String> {
    let mut found = Vec::new();
    differences(&json(left), &json(right), "", &mut found);
    found
}

fn body_of(store: &MarkedMailStore) -> String {
    store
        .find(ACCOUNT_ID, MAILBOX, MESSAGE_ID)
        .expect("the message is still in the mailbox")
        .body
        .clone()
}

#[test]
fn task_1452_marked_yes_deletes_the_maple_mail_and_names_it() {
    let mut store = marked_yes_store();
    let before = store.deleted_count();
    println!("TASK1452_GOOD_DELETE_COUNT_BEFORE={before}");
    assert_eq!(before, 0, "nothing has been deleted yet");

    let record = delete_marked_mail(&mut store, &pro_delete_request()).expect("marked yes deletes");
    let after = store.deleted_count();
    println!("TASK1452_GOOD_DELETE_COUNT_AFTER={after}");
    println!(
        "TASK1452_GOOD_RECORD={}",
        serde_json::to_string(&record).expect("a record serializes")
    );
    assert_eq!(after, 1, "exactly one message was deleted");

    assert_eq!(record.service, MAPLE_MAIL_SERVICE);
    assert_eq!(record.reference.as_deref(), Some(REFERENCE));
    assert_eq!(record.outcome, MailDeletionOutcome::Deleted);
    assert_eq!(record, expected_record(), "the record is exact");
    assert_eq!(store.deletions(), [expected_record()]);

    assert_eq!(
        store.message_count(),
        0,
        "the mail is gone from the mailbox"
    );
    assert!(store.find(ACCOUNT_ID, MAILBOX, MESSAGE_ID).is_none());
}

#[test]
fn task_1452_the_clean_copy_differs_only_in_marked() {
    let good = marked_yes_store();
    let bad = marked_no_store();
    let found = store_differences(&good, &bad);
    println!("TASK1452_COPY_DIFFERENCES={}", found.join(","));
    assert_eq!(
        found,
        vec!["messages[0].marked=true->false".to_owned()],
        "the clean copy may differ in the marked flag and nothing else"
    );
    assert_eq!(body_of(&bad), BODY);
    assert_eq!(bad.message_count(), good.message_count());
}

#[test]
fn task_1452_marked_no_is_refused_as_message_not_marked() {
    let mut store = marked_no_store();
    let untouched = store.clone();

    let error = delete_marked_mail(&mut store, &pro_delete_request())
        .expect_err("marked no must not delete");
    println!(
        "TASK1452_BAD_REFUSAL_CODE={} TASK1452_BAD_REFUSAL_TEXT={error}",
        error.code()
    );
    assert_eq!(error, MarkedMailDeletionError::MessageNotMarked);
    assert_eq!(error.code(), "message_not_marked");
    assert!(
        error.to_string().starts_with("message not marked"),
        "the refusal says the message is not marked: {error}"
    );

    println!("TASK1452_BAD_DELETE_COUNT={}", store.deleted_count());
    println!("TASK1452_BAD_MESSAGE_COUNT={}", store.message_count());
    println!("TASK1452_BAD_BODY={}", body_of(&store));
    assert_eq!(store.deleted_count(), 0, "a refusal deletes nothing");
    assert_eq!(store.message_count(), 1, "the message is still there");
    assert_eq!(
        maple_reference(&body_of(&store)).as_deref(),
        Some(REFERENCE),
        "the refused copy still reads MAPLE-4172"
    );
    assert_eq!(store, untouched, "a refusal leaves the store untouched");
}

#[test]
fn task_1452_the_refusal_leaves_the_good_record_and_the_count_alone() {
    let mut good = marked_yes_store();
    let record = delete_marked_mail(&mut good, &pro_delete_request()).expect("marked yes deletes");
    let good_record_json = serde_json::to_string(good.deletions()).expect("records serialize");
    assert_eq!(record, expected_record());
    assert_eq!(good.deleted_count(), 1);

    let mut bad = marked_no_store();
    let error =
        delete_marked_mail(&mut bad, &pro_delete_request()).expect_err("marked no is refused");
    assert_eq!(error.code(), "message_not_marked");

    let good_record_json_after =
        serde_json::to_string(good.deletions()).expect("records serialize");
    println!("TASK1452_GOOD_RECORD_AFTER_BAD_RUN={good_record_json_after}");
    println!(
        "TASK1452_BAD_BODY_AFTER_BAD_RUN={} TASK1452_BAD_REFERENCE={:?}",
        body_of(&bad),
        maple_reference(&body_of(&bad))
    );
    println!(
        "TASK1452_TOTAL_DELETE_COUNT={}",
        good.deleted_count() + bad.deleted_count()
    );

    assert_eq!(
        good_record_json_after, good_record_json,
        "the good deletion record stays exact"
    );
    assert_eq!(good.deletions(), [expected_record()]);
    assert_eq!(
        maple_reference(&body_of(&bad)).as_deref(),
        Some(REFERENCE),
        "the bad copy still reads MAPLE-4172"
    );
    assert_eq!(good.deleted_count(), 1, "the good count stays 1");
    assert_eq!(bad.deleted_count(), 0, "the refused copy deleted nothing");
    assert_eq!(
        good.deleted_count() + bad.deleted_count(),
        1,
        "the count stays 1 across both copies"
    );
}

#[test]
fn task_1452_only_the_marked_flag_decides() {
    // The refusal must be caused by `marked: no` and by nothing else about the
    // copy: flipping the one field back makes the very same request succeed
    // with the very same record.
    let mut bad = marked_no_store();
    assert_eq!(
        delete_marked_mail(&mut bad, &pro_delete_request())
            .expect_err("marked no is refused")
            .code(),
        "message_not_marked"
    );
    assert!(bad.set_marked(MESSAGE_ID, true));
    let record = delete_marked_mail(&mut bad, &pro_delete_request())
        .expect("the same copy deletes once marked");
    println!(
        "TASK1452_FLIPPED_BACK_RECORD={}",
        serde_json::to_string(&record).expect("a record serializes")
    );
    assert_eq!(record, expected_record());
    assert_eq!(bad.deleted_count(), 1);
}
