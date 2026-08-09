//! Recording per-message outcomes of an accepted marked deletion (TASK 1449).
//!
//! `delete_marked_messages` in [`crate::pro_marked_deletion`] decides *that* a
//! marked set may be deleted; it never touches storage. Something downstream
//! then attempts the actual deletion, one message at a time, against real
//! carriers -- and a single attempt can succeed, fail outright (the carrier
//! rejected it, the network dropped), or come back needing attention (the
//! message was already gone, moved, or the locator no longer resolves).
//!
//! This module performs no I/O itself. It takes the attempt result the caller
//! already produced for each message and files it under `deleted`, `failed`,
//! or `needs_attention` -- always carrying the message's account and locator
//! along with it, so a failed or needs-attention entry never loses the place
//! it was about.

use serde::{Deserialize, Serialize};

use crate::pro_marked_deletion::MarkedMessageRef;

/// What happened when this module's caller actually attempted to delete one
/// marked message.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind", deny_unknown_fields)]
pub enum DeletionAttemptResult {
    Deleted,
    /// The attempt was made and it did not succeed.
    Failed {
        reason: String,
    },
    /// The attempt could not be completed as a clean success or failure --
    /// e.g. the locator no longer resolves, or the message already moved.
    NeedsAttention {
        reason: String,
    },
}

/// One message's attempt result, paired with the location it was about.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeletionAttempt {
    pub message: MarkedMessageRef,
    pub result: DeletionAttemptResult,
}

/// One recorded outcome. `reason` is `None` for a clean deletion.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeletionOutcomeEntry {
    pub account_id: String,
    pub message_locator: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl DeletionOutcomeEntry {
    fn from_attempt(attempt: &DeletionAttempt, reason: Option<&str>) -> Self {
        Self {
            account_id: attempt.message.account_id.clone(),
            message_locator: attempt.message.message_locator.clone(),
            reason: reason.map(str::to_owned),
        }
    }
}

/// The full record of a deletion run: every marked message ends up in
/// exactly one of these three lists, each entry still carrying its location.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeletionOutcomeReport {
    pub deleted_count: usize,
    pub failed_count: usize,
    pub needs_attention_count: usize,
    pub deleted: Vec<DeletionOutcomeEntry>,
    pub failed: Vec<DeletionOutcomeEntry>,
    pub needs_attention: Vec<DeletionOutcomeEntry>,
}

/// Files each attempt's result under `deleted`, `failed`, or
/// `needs_attention`, in the order the attempts were given, never dropping
/// the account/locator a failed or needs-attention entry was about.
pub fn record_deletion_outcomes(attempts: &[DeletionAttempt]) -> DeletionOutcomeReport {
    let mut deleted = Vec::new();
    let mut failed = Vec::new();
    let mut needs_attention = Vec::new();

    for attempt in attempts {
        match &attempt.result {
            DeletionAttemptResult::Deleted => {
                deleted.push(DeletionOutcomeEntry::from_attempt(attempt, None));
            }
            DeletionAttemptResult::Failed { reason } => {
                failed.push(DeletionOutcomeEntry::from_attempt(attempt, Some(reason)));
            }
            DeletionAttemptResult::NeedsAttention { reason } => {
                needs_attention.push(DeletionOutcomeEntry::from_attempt(attempt, Some(reason)));
            }
        }
    }

    DeletionOutcomeReport {
        deleted_count: deleted.len(),
        failed_count: failed.len(),
        needs_attention_count: needs_attention.len(),
        deleted,
        failed,
        needs_attention,
    }
}

/// JSON command surface: `request_json` is a bare array of [`DeletionAttempt`].
/// The reply is always a JSON object with an `ok` field.
pub fn run_pro_marked_deletion_outcomes_command(request_json: &str) -> String {
    let attempts: Vec<DeletionAttempt> = match serde_json::from_str(request_json) {
        Ok(attempts) => attempts,
        Err(error) => {
            return serde_json::json!({
                "ok": false,
                "errorCode": "bad_request",
                "error": format!("This request could not be read: {error}"),
            })
            .to_string();
        }
    };
    let report = record_deletion_outcomes(&attempts);
    let value = match serde_json::to_value(&report) {
        Ok(value) => value,
        Err(error) => {
            return serde_json::json!({
                "ok": false,
                "errorCode": "bad_request",
                "error": format!("This request could not be read: {error}"),
            })
            .to_string();
        }
    };
    serde_json::json!({ "ok": true, "result": value }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attempt(account: &str, locator: &str, result: DeletionAttemptResult) -> DeletionAttempt {
        DeletionAttempt {
            message: MarkedMessageRef {
                account_id: account.to_owned(),
                message_locator: locator.to_owned(),
            },
            result,
        }
    }

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
    fn task_1451_mixed_fixture_returns_all_three_outcomes_with_locations() {
        let attempts = mixed_fixture();
        let report = record_deletion_outcomes(&attempts);

        assert_eq!(report.deleted_count, 2);
        assert_eq!(report.failed_count, 1);
        assert_eq!(report.needs_attention_count, 1);

        assert_eq!(
            report.deleted,
            vec![
                DeletionOutcomeEntry {
                    account_id: "discord-account-alpha-1444".to_owned(),
                    message_locator: "blocked-local-reference-1".to_owned(),
                    reason: None,
                },
                DeletionOutcomeEntry {
                    account_id: "discord-account-alpha-1444".to_owned(),
                    message_locator: "blocked-local-reference-9".to_owned(),
                    reason: None,
                },
            ]
        );
        assert_eq!(
            report.failed,
            vec![DeletionOutcomeEntry {
                account_id: "telegram-account-beta-1444".to_owned(),
                message_locator: "blocked-local-reference-3".to_owned(),
                reason: Some("carrier rejected the delete request".to_owned()),
            }]
        );
        assert_eq!(
            report.needs_attention,
            vec![DeletionOutcomeEntry {
                account_id: "signal-account-gamma-1444".to_owned(),
                message_locator: "blocked-local-reference-7".to_owned(),
                reason: Some("locator no longer resolves; message may have moved".to_owned()),
            }]
        );

        println!(
            "TASK1451_MIXED deleted_count={} failed_count={} needs_attention_count={}",
            report.deleted_count, report.failed_count, report.needs_attention_count
        );
        for entry in &report.deleted {
            println!(
                "TASK1451_DELETED account={} locator={}",
                entry.account_id, entry.message_locator
            );
        }
        for entry in &report.failed {
            println!(
                "TASK1451_FAILED account={} locator={} reason=\"{}\"",
                entry.account_id,
                entry.message_locator,
                entry.reason.as_deref().unwrap_or("")
            );
        }
        for entry in &report.needs_attention {
            println!(
                "TASK1451_NEEDS_ATTENTION account={} locator={} reason=\"{}\"",
                entry.account_id,
                entry.message_locator,
                entry.reason.as_deref().unwrap_or("")
            );
        }
    }

    #[test]
    fn task_1451_all_deleted_leaves_the_other_two_lists_empty() {
        let attempts = vec![
            attempt(
                "discord-account-alpha-1444",
                "blocked-local-reference-1",
                DeletionAttemptResult::Deleted,
            ),
            attempt(
                "discord-account-alpha-1444",
                "blocked-local-reference-2",
                DeletionAttemptResult::Deleted,
            ),
        ];
        let report = record_deletion_outcomes(&attempts);
        assert_eq!(report.deleted_count, 2);
        assert!(report.failed.is_empty());
        assert!(report.needs_attention.is_empty());
    }

    #[test]
    fn task_1451_json_command_reports_locations_for_all_three_outcomes() {
        let attempts_json = serde_json::to_string(&mixed_fixture()).expect("fixture json");
        let reply = run_pro_marked_deletion_outcomes_command(&attempts_json);
        let value: serde_json::Value = serde_json::from_str(&reply).expect("reply json");
        assert_eq!(value["ok"], serde_json::json!(true));
        assert_eq!(value["result"]["deletedCount"], serde_json::json!(2));
        assert_eq!(value["result"]["failedCount"], serde_json::json!(1));
        assert_eq!(value["result"]["needsAttentionCount"], serde_json::json!(1));
        assert_eq!(
            value["result"]["failed"][0]["messageLocator"],
            serde_json::json!("blocked-local-reference-3")
        );
        assert_eq!(
            value["result"]["needsAttention"][0]["accountId"],
            serde_json::json!("signal-account-gamma-1444")
        );
        println!("TASK1451_JSON={reply}");
    }

    #[test]
    fn task_1451_bad_request_json_is_refused() {
        let reply = run_pro_marked_deletion_outcomes_command("not json");
        let value: serde_json::Value = serde_json::from_str(&reply).expect("reply json");
        assert_eq!(value["ok"], serde_json::json!(false));
        assert_eq!(value["errorCode"], serde_json::json!("bad_request"));
    }
}
