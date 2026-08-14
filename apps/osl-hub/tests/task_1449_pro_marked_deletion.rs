//! TASK 1449: deletion of marked messages is Pro-only, reviewed-only, and only
//! after a final count has been shown.
//!
//! Finish line: Free is refused, and Pro receives the marked count before
//! confirmation.

use osl_privacy_hub::burn_contract::ProductTier;
use osl_privacy_hub::pro_marked_deletion::{
    count_marked_messages, delete_marked_messages, run_pro_marked_deletion_command,
    MarkedAccountCount, MarkedDeletionError, MarkedMessageRef, ReviewDecision, ReviewedMessage,
    PRO_REQUIRED_REFUSAL,
};

fn message(
    account: &str,
    locator: &str,
    decision: ReviewDecision,
    reviewed: bool,
) -> ReviewedMessage {
    ReviewedMessage {
        account_id: account.to_owned(),
        message_locator: locator.to_owned(),
        decision,
        reviewed,
    }
}

/// Same two accounts the TASK 1444 review store fixture uses, with locators in
/// the blocked, non-jumpable form TASK 1445 leaves behind.
fn fixture() -> Vec<ReviewedMessage> {
    vec![
        message(
            "discord-account-alpha-1444",
            "blocked-local-reference-1",
            ReviewDecision::MarkedForDeletion,
            true,
        ),
        message(
            "discord-account-alpha-1444",
            "blocked-local-reference-2",
            ReviewDecision::Kept,
            true,
        ),
        message(
            "telegram-account-beta-1444",
            "blocked-local-reference-3",
            ReviewDecision::MarkedForDeletion,
            true,
        ),
        message(
            "telegram-account-beta-1444",
            "blocked-local-reference-4",
            ReviewDecision::Pending,
            false,
        ),
    ]
}

fn fixture_json() -> String {
    serde_json::to_string(&fixture()).expect("fixture serializes")
}

#[test]
fn task_1449_free_is_refused() {
    let messages = fixture();

    let count = count_marked_messages(ProductTier::Free, &messages);
    assert_eq!(count, Err(MarkedDeletionError::ProRequired));
    let delete = delete_marked_messages(ProductTier::Free, &messages, "any-token", true);
    assert_eq!(delete, Err(MarkedDeletionError::ProRequired));

    let reply = run_pro_marked_deletion_command(
        "count",
        &format!("{{\"plan\":\"free\",\"messages\":{}}}", fixture_json()),
    );
    let value: serde_json::Value = serde_json::from_str(&reply).expect("reply is json");
    assert_eq!(value["ok"], serde_json::json!(false));
    assert_eq!(value["errorCode"], serde_json::json!("pro_required"));
    assert_eq!(value["error"], serde_json::json!(PRO_REQUIRED_REFUSAL));
    // Free is refused before any count exists, so no count leaks in the refusal.
    assert!(value.get("result").is_none());
    println!("task_1449_free_count_reply={reply}");
}

#[test]
fn task_1449_pro_receives_the_marked_count_before_confirmation() {
    let messages = fixture();
    let count = count_marked_messages(ProductTier::Pro, &messages).expect("pro receives a count");

    assert_eq!(count.marked_count, 2);
    assert_eq!(count.kept_count, 1);
    assert_eq!(count.pending_count, 1);
    assert!(count.confirmation_required);
    assert_eq!(
        count.confirmation_prompt,
        "Delete 2 marked messages? This cannot be undone."
    );
    assert_eq!(count.confirmation_token.len(), 64);
    assert_eq!(
        count.account_counts,
        vec![
            MarkedAccountCount {
                account_id: "discord-account-alpha-1444".to_owned(),
                marked_count: 1,
            },
            MarkedAccountCount {
                account_id: "telegram-account-beta-1444".to_owned(),
                marked_count: 1,
            },
        ]
    );
    println!(
        "task_1449_pro_count marked_count={} kept_count={} pending_count={} prompt=\"{}\"",
        count.marked_count, count.kept_count, count.pending_count, count.confirmation_prompt
    );

    // Confirmation cannot come first: without the token the count issues, and
    // without an explicit confirmation, deletion is refused.
    assert_eq!(
        delete_marked_messages(ProductTier::Pro, &messages, "", true),
        Err(MarkedDeletionError::CountNotShown)
    );
    assert_eq!(
        delete_marked_messages(
            ProductTier::Pro,
            &messages,
            &count.confirmation_token,
            false
        ),
        Err(MarkedDeletionError::NotConfirmed)
    );

    let outcome =
        delete_marked_messages(ProductTier::Pro, &messages, &count.confirmation_token, true)
            .expect("delete after the count and the confirmation");
    assert_eq!(outcome.deleted_count, count.marked_count);
    assert_eq!(outcome.kept_untouched_count, 2);
    assert_eq!(
        outcome.deleted,
        vec![
            MarkedMessageRef {
                account_id: "discord-account-alpha-1444".to_owned(),
                message_locator: "blocked-local-reference-1".to_owned(),
            },
            MarkedMessageRef {
                account_id: "telegram-account-beta-1444".to_owned(),
                message_locator: "blocked-local-reference-3".to_owned(),
            },
        ]
    );
    println!(
        "task_1449_pro_delete deleted_count={} kept_untouched_count={}",
        outcome.deleted_count, outcome.kept_untouched_count
    );
}

#[test]
fn task_1449_only_reviewed_marked_messages_are_deletable() {
    let mut messages = fixture();
    messages.push(message(
        "discord-account-alpha-1444",
        "blocked-local-reference-5",
        ReviewDecision::MarkedForDeletion,
        false,
    ));
    assert_eq!(
        count_marked_messages(ProductTier::Pro, &messages),
        Err(MarkedDeletionError::MarkedButNotReviewed)
    );
    assert_eq!(
        delete_marked_messages(ProductTier::Pro, &messages, "token", true),
        Err(MarkedDeletionError::MarkedButNotReviewed)
    );

    let kept_only = vec![message(
        "discord-account-alpha-1444",
        "blocked-local-reference-2",
        ReviewDecision::Kept,
        true,
    )];
    assert_eq!(
        count_marked_messages(ProductTier::Pro, &kept_only),
        Err(MarkedDeletionError::NothingMarked)
    );
}

#[test]
fn task_1449_a_stale_count_cannot_confirm_a_changed_marked_set() {
    let messages = fixture();
    let first = count_marked_messages(ProductTier::Pro, &messages).expect("first count");

    let mut changed = messages.clone();
    changed.push(message(
        "discord-account-alpha-1444",
        "blocked-local-reference-9",
        ReviewDecision::MarkedForDeletion,
        true,
    ));
    assert_eq!(
        delete_marked_messages(ProductTier::Pro, &changed, &first.confirmation_token, true),
        Err(MarkedDeletionError::CountChanged)
    );

    let recount = count_marked_messages(ProductTier::Pro, &changed).expect("second count");
    assert_eq!(recount.marked_count, 3);
    assert_ne!(recount.confirmation_token, first.confirmation_token);
    assert_eq!(
        delete_marked_messages(
            ProductTier::Pro,
            &changed,
            &recount.confirmation_token,
            true
        )
        .expect("delete after the recount")
        .deleted_count,
        3
    );
}

#[test]
fn task_1449_json_command_reports_the_count_for_pro() {
    let reply = run_pro_marked_deletion_command(
        "count",
        &format!("{{\"plan\":\"pro\",\"messages\":{}}}", fixture_json()),
    );
    let value: serde_json::Value = serde_json::from_str(&reply).expect("reply is json");
    assert_eq!(value["ok"], serde_json::json!(true));
    assert_eq!(value["result"]["markedCount"], serde_json::json!(2));
    assert_eq!(
        value["result"]["confirmationRequired"],
        serde_json::json!(true)
    );
    println!("task_1449_pro_count_reply={reply}");

    let token = value["result"]["confirmationToken"]
        .as_str()
        .expect("a confirmation token");
    let deleted = run_pro_marked_deletion_command(
        "delete",
        &format!(
            "{{\"plan\":\"pro\",\"messages\":{},\"confirmationToken\":\"{token}\",\"confirmed\":true}}",
            fixture_json()
        ),
    );
    let deleted_value: serde_json::Value =
        serde_json::from_str(&deleted).expect("delete reply is json");
    assert_eq!(deleted_value["ok"], serde_json::json!(true));
    assert_eq!(
        deleted_value["result"]["deletedCount"],
        serde_json::json!(2)
    );
    println!("task_1449_pro_delete_reply={deleted}");
}
