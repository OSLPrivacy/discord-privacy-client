//! TASK 1451: record deleted, failed, and needs-attention per marked message
//! without dropping its location.
//!
//! Finish line: a mixed fixture deletion returns all three outcomes with
//! locations.

use osl_privacy_hub::pro_marked_deletion::MarkedMessageRef;
use osl_privacy_hub::pro_marked_deletion_outcomes::{
    record_deletion_outcomes, run_pro_marked_deletion_outcomes_command, DeletionAttempt,
    DeletionAttemptResult, DeletionOutcomeEntry,
};

fn attempt(account: &str, locator: &str, result: DeletionAttemptResult) -> DeletionAttempt {
    DeletionAttempt {
        message: MarkedMessageRef {
            account_id: account.to_owned(),
            message_locator: locator.to_owned(),
        },
        result,
    }
}

/// One deleted, one failed, one needs-attention, across three accounts, plus
/// a second deleted message on the first account.
fn mixed_fixture() -> Vec<DeletionAttempt> {
    vec![
        attempt(
            "discord-account-alpha-1444",
            "blocked-local-reference-1",
            DeletionAttemptResult::Deleted,
        ),
        attempt(
            "telegram-account-beta-1444",
            "blocked-local-reference-3",
            DeletionAttemptResult::Failed {
                reason: "carrier rejected the delete request".to_owned(),
            },
        ),
        attempt(
            "signal-account-gamma-1444",
            "blocked-local-reference-7",
            DeletionAttemptResult::NeedsAttention {
                reason: "locator no longer resolves; message may have moved".to_owned(),
            },
        ),
        attempt(
            "discord-account-alpha-1444",
            "blocked-local-reference-9",
            DeletionAttemptResult::Deleted,
        ),
    ]
}

#[test]
fn task_1451_mixed_fixture_deletion_returns_all_three_outcomes_with_locations() {
    let attempts = mixed_fixture();
    let report = record_deletion_outcomes(&attempts);

    assert_eq!(report.deleted_count, 2);
    assert_eq!(report.failed_count, 1);
    assert_eq!(report.needs_attention_count, 1);

    assert!(report.deleted.contains(&DeletionOutcomeEntry {
        account_id: "discord-account-alpha-1444".to_owned(),
        message_locator: "blocked-local-reference-1".to_owned(),
        reason: None,
    }));
    assert!(report.deleted.contains(&DeletionOutcomeEntry {
        account_id: "discord-account-alpha-1444".to_owned(),
        message_locator: "blocked-local-reference-9".to_owned(),
        reason: None,
    }));
    assert!(report.failed.contains(&DeletionOutcomeEntry {
        account_id: "telegram-account-beta-1444".to_owned(),
        message_locator: "blocked-local-reference-3".to_owned(),
        reason: Some("carrier rejected the delete request".to_owned()),
    }));
    assert!(report.needs_attention.contains(&DeletionOutcomeEntry {
        account_id: "signal-account-gamma-1444".to_owned(),
        message_locator: "blocked-local-reference-7".to_owned(),
        reason: Some("locator no longer resolves; message may have moved".to_owned()),
    }));

    println!(
        "TASK1451_FINISH_LINE deleted_count={} failed_count={} needs_attention_count={}",
        report.deleted_count, report.failed_count, report.needs_attention_count
    );

    let attempts_json = serde_json::to_string(&attempts).expect("attempts json");
    let reply = run_pro_marked_deletion_outcomes_command(&attempts_json);
    let value: serde_json::Value = serde_json::from_str(&reply).expect("reply json");
    assert_eq!(value["ok"], serde_json::json!(true));
    assert_eq!(value["result"]["deletedCount"], serde_json::json!(2));
    assert_eq!(value["result"]["failedCount"], serde_json::json!(1));
    assert_eq!(value["result"]["needsAttentionCount"], serde_json::json!(1));
    println!("TASK1451_JSON_COMMAND={reply}");
}

#[test]
fn task_1451_empty_attempt_list_yields_empty_outcomes() {
    let report = record_deletion_outcomes(&[]);
    assert_eq!(report.deleted_count, 0);
    assert_eq!(report.failed_count, 0);
    assert_eq!(report.needs_attention_count, 0);
    assert!(report.deleted.is_empty());
    assert!(report.failed.is_empty());
    assert!(report.needs_attention.is_empty());
}
