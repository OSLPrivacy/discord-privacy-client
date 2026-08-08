//! TASK 1464 - scheduled runner rules.
//!
//! Decides whether a service-connection action may start for an account in
//! the scheduled-run queue. Pure: no I/O, no live clock read (`now` is
//! always an explicit input on the trigger), no Tauri, no global state.
//!
//! Rules:
//! - Only one account runs at a time. A queue with an account already
//!   active refuses to start a second one, no matter the trigger.
//! - An account that is asleep or unavailable never starts a service
//!   connection action, even on an explicit Run now naming that exact
//!   account. Sleep/unavailability pauses that account only; other due,
//!   available accounts are unaffected.
//! - Absent an explicit Run now, an account only starts once its schedule's
//!   next run time has arrived: a scheduled tick fired before that time
//!   takes no action for it. This is the "resume only on Run now or next
//!   schedule" half of the rule.

use serde::{Deserialize, Serialize};

pub const SCHEDULED_RUNNER_DECIDE_COMMAND: &str = "scheduled_runner_decide";

pub const REASON_ACCOUNT_ALREADY_RUNNING: &str = "account_already_running";
pub const REASON_UNKNOWN_ACCOUNT: &str = "unknown_account";
pub const REASON_ACCOUNT_NOT_APPROVED: &str = "account_not_approved";
pub const REASON_ACCOUNT_SLEEPING: &str = "account_sleeping";
pub const REASON_ACCOUNT_UNAVAILABLE: &str = "account_unavailable";
pub const REASON_NOT_DUE: &str = "not_due";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountAvailability {
    Available,
    Sleeping,
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledRunnerAccount {
    pub account_id: String,
    pub label: String,
    pub approved: bool,
    pub availability: AccountAvailability,
    /// `None` means the account has no automatic schedule (e.g. "only when
    /// I choose"): a scheduled tick never starts it, only Run now can.
    pub next_run_unix_secs: Option<i64>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledRunnerQueue {
    pub accounts: Vec<ScheduledRunnerAccount>,
    /// The account id currently running a service connection action, if
    /// any. Enforces "one account at a time" across triggers.
    #[serde(default)]
    pub active_account_id: Option<String>,
}

impl ScheduledRunnerQueue {
    fn find(&self, account_id: &str) -> Option<&ScheduledRunnerAccount> {
        self.accounts.iter().find(|a| a.account_id == account_id)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunnerTrigger {
    #[serde(rename_all = "camelCase")]
    RunNow {
        account_id: String,
        now_unix_secs: i64,
    },
    #[serde(rename_all = "camelCase")]
    ScheduledTick { now_unix_secs: i64 },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "decision", content = "data", rename_all = "snake_case")]
pub enum RunnerDecision {
    #[serde(rename_all = "camelCase")]
    Start { account_id: String },
    #[serde(rename_all = "camelCase")]
    NoAction {
        reason_code: &'static str,
        detail: String,
    },
}

impl RunnerDecision {
    pub fn started_account_id(&self) -> Option<&str> {
        match self {
            RunnerDecision::Start { account_id } => Some(account_id),
            RunnerDecision::NoAction { .. } => None,
        }
    }
}

pub fn decide_scheduled_runner_action(
    queue: &ScheduledRunnerQueue,
    trigger: &RunnerTrigger,
) -> RunnerDecision {
    if let Some(active) = &queue.active_account_id {
        return RunnerDecision::NoAction {
            reason_code: REASON_ACCOUNT_ALREADY_RUNNING,
            detail: format!(
                "{active} is already running a service connection action; \
                 only one account runs at a time"
            ),
        };
    }

    match trigger {
        RunnerTrigger::RunNow { account_id, .. } => decide_run_now(queue, account_id),
        RunnerTrigger::ScheduledTick { now_unix_secs } => {
            decide_scheduled_tick(queue, *now_unix_secs)
        }
    }
}

fn decide_run_now(queue: &ScheduledRunnerQueue, account_id: &str) -> RunnerDecision {
    let Some(account) = queue.find(account_id) else {
        return RunnerDecision::NoAction {
            reason_code: REASON_UNKNOWN_ACCOUNT,
            detail: format!("{account_id} is not a known account"),
        };
    };
    if !account.approved {
        return RunnerDecision::NoAction {
            reason_code: REASON_ACCOUNT_NOT_APPROVED,
            detail: format!("{account_id} is not approved to run"),
        };
    }
    match account.availability {
        AccountAvailability::Sleeping => RunnerDecision::NoAction {
            reason_code: REASON_ACCOUNT_SLEEPING,
            detail: format!("{account_id} is asleep; Run now does not wake it"),
        },
        AccountAvailability::Unavailable => RunnerDecision::NoAction {
            reason_code: REASON_ACCOUNT_UNAVAILABLE,
            detail: format!("{account_id} is unavailable"),
        },
        AccountAvailability::Available => RunnerDecision::Start {
            account_id: account.account_id.clone(),
        },
    }
}

fn decide_scheduled_tick(queue: &ScheduledRunnerQueue, now_unix_secs: i64) -> RunnerDecision {
    let mut due: Vec<&ScheduledRunnerAccount> = queue
        .accounts
        .iter()
        .filter(|a| a.approved)
        .filter(|a| a.availability == AccountAvailability::Available)
        .filter(|a| a.next_run_unix_secs.is_some_and(|t| t <= now_unix_secs))
        .collect();
    due.sort_by(|a, b| {
        a.next_run_unix_secs
            .cmp(&b.next_run_unix_secs)
            .then_with(|| a.account_id.cmp(&b.account_id))
    });
    match due.first() {
        Some(account) => RunnerDecision::Start {
            account_id: account.account_id.clone(),
        },
        None => RunnerDecision::NoAction {
            reason_code: REASON_NOT_DUE,
            detail: "no approved, available account is due yet".to_owned(),
        },
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScheduledRunnerDecideRequest {
    queue: ScheduledRunnerQueue,
    trigger: RunnerTrigger,
}

/// The direct-invoke surface: `scheduled_runner_decide`. Every reply is a
/// JSON object carrying `ok`, matching the convention the other
/// `crates/ipc` command modules use.
pub fn run_scheduled_runner_command(command: &str, request_json: &str) -> String {
    if command != SCHEDULED_RUNNER_DECIDE_COMMAND {
        return json_error_reply(
            command,
            "unknown_command",
            &format!("unknown command '{command}'"),
        );
    }
    let request: ScheduledRunnerDecideRequest = match serde_json::from_str(request_json) {
        Ok(request) => request,
        Err(err) => return json_error_reply(command, "invalid_request", &err.to_string()),
    };
    let decision = decide_scheduled_runner_action(&request.queue, &request.trigger);
    let result = serde_json::to_value(&decision).expect("RunnerDecision always serializes");
    serde_json::json!({
        "command": command,
        "ok": true,
        "result": result,
    })
    .to_string()
}

fn json_error_reply(command: &str, error_code: &str, message: &str) -> String {
    serde_json::json!({
        "command": command,
        "ok": false,
        "errorCode": error_code,
        "error": message,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(
        id: &str,
        approved: bool,
        availability: AccountAvailability,
        next_run_unix_secs: Option<i64>,
    ) -> ScheduledRunnerAccount {
        ScheduledRunnerAccount {
            account_id: id.to_owned(),
            label: format!("{id} label"),
            approved,
            availability,
            next_run_unix_secs,
        }
    }

    fn queue(accounts: Vec<ScheduledRunnerAccount>) -> ScheduledRunnerQueue {
        ScheduledRunnerQueue {
            accounts,
            active_account_id: None,
        }
    }

    #[test]
    fn a_sleeping_account_never_starts_on_run_now() {
        let q = queue(vec![account(
            "acct-a",
            true,
            AccountAvailability::Sleeping,
            Some(0),
        )]);
        let decision = decide_scheduled_runner_action(
            &q,
            &RunnerTrigger::RunNow {
                account_id: "acct-a".to_owned(),
                now_unix_secs: 100,
            },
        );
        assert_eq!(
            decision,
            RunnerDecision::NoAction {
                reason_code: REASON_ACCOUNT_SLEEPING,
                detail: "acct-a is asleep; Run now does not wake it".to_owned(),
            }
        );
        assert_eq!(decision.started_account_id(), None);
    }

    #[test]
    fn an_unavailable_account_never_starts_on_a_scheduled_tick() {
        let q = queue(vec![account(
            "acct-a",
            true,
            AccountAvailability::Unavailable,
            Some(0),
        )]);
        let decision = decide_scheduled_runner_action(
            &q,
            &RunnerTrigger::ScheduledTick { now_unix_secs: 100 },
        );
        assert_eq!(
            decision,
            RunnerDecision::NoAction {
                reason_code: REASON_NOT_DUE,
                detail: "no approved, available account is due yet".to_owned(),
            }
        );
    }

    #[test]
    fn run_now_starts_an_approved_available_account() {
        let q = queue(vec![account(
            "acct-a",
            true,
            AccountAvailability::Available,
            None,
        )]);
        let decision = decide_scheduled_runner_action(
            &q,
            &RunnerTrigger::RunNow {
                account_id: "acct-a".to_owned(),
                now_unix_secs: 100,
            },
        );
        assert_eq!(
            decision,
            RunnerDecision::Start {
                account_id: "acct-a".to_owned(),
            }
        );
    }

    #[test]
    fn run_now_refuses_an_unapproved_account() {
        let q = queue(vec![account(
            "acct-a",
            false,
            AccountAvailability::Available,
            None,
        )]);
        let decision = decide_scheduled_runner_action(
            &q,
            &RunnerTrigger::RunNow {
                account_id: "acct-a".to_owned(),
                now_unix_secs: 100,
            },
        );
        assert_eq!(
            decision,
            RunnerDecision::NoAction {
                reason_code: REASON_ACCOUNT_NOT_APPROVED,
                detail: "acct-a is not approved to run".to_owned(),
            }
        );
    }

    #[test]
    fn run_now_refuses_an_unknown_account() {
        let q = queue(vec![]);
        let decision = decide_scheduled_runner_action(
            &q,
            &RunnerTrigger::RunNow {
                account_id: "ghost".to_owned(),
                now_unix_secs: 100,
            },
        );
        assert_eq!(
            decision,
            RunnerDecision::NoAction {
                reason_code: REASON_UNKNOWN_ACCOUNT,
                detail: "ghost is not a known account".to_owned(),
            }
        );
    }

    #[test]
    fn one_account_at_a_time_refuses_a_second_start_regardless_of_trigger() {
        let mut q = queue(vec![
            account("acct-a", true, AccountAvailability::Available, Some(0)),
            account("acct-b", true, AccountAvailability::Available, Some(0)),
        ]);
        q.active_account_id = Some("acct-a".to_owned());

        let via_tick =
            decide_scheduled_runner_action(&q, &RunnerTrigger::ScheduledTick { now_unix_secs: 50 });
        let via_run_now = decide_scheduled_runner_action(
            &q,
            &RunnerTrigger::RunNow {
                account_id: "acct-b".to_owned(),
                now_unix_secs: 50,
            },
        );
        for decision in [via_tick, via_run_now] {
            assert_eq!(
                decision,
                RunnerDecision::NoAction {
                    reason_code: REASON_ACCOUNT_ALREADY_RUNNING,
                    detail: "acct-a is already running a service connection action; only one account runs at a time".to_owned(),
                }
            );
        }
    }

    #[test]
    fn scheduled_tick_takes_no_action_before_the_next_run_time() {
        let q = queue(vec![account(
            "acct-a",
            true,
            AccountAvailability::Available,
            Some(200),
        )]);
        let decision = decide_scheduled_runner_action(
            &q,
            &RunnerTrigger::ScheduledTick { now_unix_secs: 100 },
        );
        assert_eq!(
            decision,
            RunnerDecision::NoAction {
                reason_code: REASON_NOT_DUE,
                detail: "no approved, available account is due yet".to_owned(),
            }
        );
    }

    #[test]
    fn scheduled_tick_starts_the_account_once_its_next_run_time_arrives() {
        let q = queue(vec![account(
            "acct-a",
            true,
            AccountAvailability::Available,
            Some(100),
        )]);
        let decision = decide_scheduled_runner_action(
            &q,
            &RunnerTrigger::ScheduledTick { now_unix_secs: 100 },
        );
        assert_eq!(
            decision,
            RunnerDecision::Start {
                account_id: "acct-a".to_owned(),
            }
        );
    }

    #[test]
    fn scheduled_tick_skips_a_sleeping_due_account_and_starts_the_next_due_account() {
        let q = queue(vec![
            account("acct-sleepy", true, AccountAvailability::Sleeping, Some(50)),
            account(
                "acct-awake",
                true,
                AccountAvailability::Available,
                Some(100),
            ),
        ]);
        let decision = decide_scheduled_runner_action(
            &q,
            &RunnerTrigger::ScheduledTick { now_unix_secs: 100 },
        );
        assert_eq!(
            decision,
            RunnerDecision::Start {
                account_id: "acct-awake".to_owned(),
            }
        );
    }

    #[test]
    fn scheduled_tick_picks_the_earliest_due_account_when_several_are_due() {
        let q = queue(vec![
            account("acct-later", true, AccountAvailability::Available, Some(90)),
            account(
                "acct-earlier",
                true,
                AccountAvailability::Available,
                Some(10),
            ),
        ]);
        let decision = decide_scheduled_runner_action(
            &q,
            &RunnerTrigger::ScheduledTick { now_unix_secs: 100 },
        );
        assert_eq!(
            decision,
            RunnerDecision::Start {
                account_id: "acct-earlier".to_owned(),
            }
        );
    }

    #[test]
    fn a_sleeping_account_is_skipped_but_does_not_block_other_accounts_next_schedule() {
        // Sleep pauses only the sleeping account; it must not stop the
        // runner from picking up a different account whose own schedule
        // is due next.
        let q = queue(vec![account(
            "acct-sleepy",
            true,
            AccountAvailability::Sleeping,
            Some(10),
        )]);
        let decision = decide_scheduled_runner_action(
            &q,
            &RunnerTrigger::ScheduledTick { now_unix_secs: 999 },
        );
        assert_eq!(
            decision,
            RunnerDecision::NoAction {
                reason_code: REASON_NOT_DUE,
                detail: "no approved, available account is due yet".to_owned(),
            }
        );
    }

    #[test]
    fn direct_invoke_unknown_command_is_refused() {
        let reply = run_scheduled_runner_command("bogus", "{}");
        let value: serde_json::Value = serde_json::from_str(&reply).unwrap();
        assert_eq!(value["ok"], false);
        assert_eq!(value["errorCode"], "unknown_command");
    }
}
