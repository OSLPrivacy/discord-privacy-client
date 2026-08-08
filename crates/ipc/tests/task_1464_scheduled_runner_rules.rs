//! TASK 1464 - write scheduled runner rules.
//!
//! Finish line: a sleeping fixture starts no service connection action and a
//! Run now starts one approved account.

use ipc::scheduled_runner_rules::{
    decide_scheduled_runner_action, run_scheduled_runner_command, AccountAvailability,
    RunnerDecision, RunnerTrigger, ScheduledRunnerAccount, ScheduledRunnerQueue,
    SCHEDULED_RUNNER_DECIDE_COMMAND,
};
use serde_json::json;

const NOW: i64 = 1_786_104_000;

fn sleeping_fixture() -> ScheduledRunnerQueue {
    ScheduledRunnerQueue {
        accounts: vec![ScheduledRunnerAccount {
            account_id: "acct-sleeping-maple".to_owned(),
            label: "Discord app (asleep)".to_owned(),
            approved: true,
            availability: AccountAvailability::Sleeping,
            // Already due, so the only reason it must not start is sleep.
            next_run_unix_secs: Some(NOW - 1),
        }],
        active_account_id: None,
    }
}

fn approved_fixture() -> ScheduledRunnerQueue {
    ScheduledRunnerQueue {
        accounts: vec![ScheduledRunnerAccount {
            account_id: "acct-approved-pine".to_owned(),
            label: "Discord app".to_owned(),
            approved: true,
            availability: AccountAvailability::Available,
            // Not due on its own schedule; only an explicit Run now starts it.
            next_run_unix_secs: Some(NOW + 3_600),
        }],
        active_account_id: None,
    }
}

#[test]
fn a_sleeping_fixture_starts_no_service_connection_action() {
    let queue = sleeping_fixture();
    let mut started = Vec::new();

    let via_tick = decide_scheduled_runner_action(
        &queue,
        &RunnerTrigger::ScheduledTick { now_unix_secs: NOW },
    );
    if let Some(id) = via_tick.started_account_id() {
        started.push(id.to_owned());
    }
    println!("TASK1464_SLEEPING_SCHEDULED_TICK_DECISION={via_tick:?}");

    let via_run_now = decide_scheduled_runner_action(
        &queue,
        &RunnerTrigger::RunNow {
            account_id: "acct-sleeping-maple".to_owned(),
            now_unix_secs: NOW,
        },
    );
    if let Some(id) = via_run_now.started_account_id() {
        started.push(id.to_owned());
    }
    println!("TASK1464_SLEEPING_RUN_NOW_DECISION={via_run_now:?}");

    assert!(matches!(via_tick, RunnerDecision::NoAction { .. }));
    assert!(matches!(via_run_now, RunnerDecision::NoAction { .. }));
    println!(
        "TASK1464_SLEEPING_FIXTURE_STARTED_ACTIONS={}",
        started.len()
    );
    assert_eq!(started.len(), 0);
}

#[test]
fn a_run_now_starts_one_approved_account() {
    let queue = approved_fixture();

    // A scheduled tick before the next run time must not start it (proves
    // Run now, not the tick, is what fires here).
    let before_due = decide_scheduled_runner_action(
        &queue,
        &RunnerTrigger::ScheduledTick { now_unix_secs: NOW },
    );
    println!("TASK1464_APPROVED_NOT_YET_DUE_DECISION={before_due:?}");
    assert!(matches!(before_due, RunnerDecision::NoAction { .. }));

    let decision = decide_scheduled_runner_action(
        &queue,
        &RunnerTrigger::RunNow {
            account_id: "acct-approved-pine".to_owned(),
            now_unix_secs: NOW,
        },
    );
    println!("TASK1464_RUN_NOW_DECISION={decision:?}");

    let started: Vec<&str> = decision.started_account_id().into_iter().collect();
    println!("TASK1464_RUN_NOW_STARTED_COUNT={}", started.len());
    println!("TASK1464_RUN_NOW_STARTED_ACCOUNTS={started:?}");

    assert_eq!(started, vec!["acct-approved-pine"]);
    assert_eq!(
        decision,
        RunnerDecision::Start {
            account_id: "acct-approved-pine".to_owned(),
        }
    );
}

#[test]
fn the_finish_line_via_the_direct_invoke_surface() {
    // Sleeping fixture, scheduled tick: no service connection action.
    let sleeping = sleeping_fixture();
    let sleeping_json = serde_json::to_value(&sleeping).unwrap();
    let tick_reply = run_scheduled_runner_command(
        SCHEDULED_RUNNER_DECIDE_COMMAND,
        &json!({
            "queue": sleeping_json,
            "trigger": { "kind": "scheduled_tick", "nowUnixSecs": NOW },
        })
        .to_string(),
    );
    let tick_reply: serde_json::Value = serde_json::from_str(&tick_reply).unwrap();
    println!("TASK1464_DIRECT_INVOKE_SLEEPING_TICK={tick_reply}");
    assert_eq!(tick_reply["ok"], true);
    assert_eq!(tick_reply["result"]["decision"], "no_action");

    let run_now_on_sleeping_reply = run_scheduled_runner_command(
        SCHEDULED_RUNNER_DECIDE_COMMAND,
        &json!({
            "queue": sleeping_json,
            "trigger": {
                "kind": "run_now",
                "accountId": "acct-sleeping-maple",
                "nowUnixSecs": NOW,
            },
        })
        .to_string(),
    );
    let run_now_on_sleeping_reply: serde_json::Value =
        serde_json::from_str(&run_now_on_sleeping_reply).unwrap();
    println!("TASK1464_DIRECT_INVOKE_SLEEPING_RUN_NOW={run_now_on_sleeping_reply}");
    assert_eq!(run_now_on_sleeping_reply["ok"], true);
    assert_eq!(run_now_on_sleeping_reply["result"]["decision"], "no_action");
    assert_eq!(
        run_now_on_sleeping_reply["result"]["data"]["reasonCode"],
        "account_sleeping"
    );

    // Approved fixture, Run now: starts exactly one approved account.
    let approved = approved_fixture();
    let run_now_reply = run_scheduled_runner_command(
        SCHEDULED_RUNNER_DECIDE_COMMAND,
        &json!({
            "queue": serde_json::to_value(&approved).unwrap(),
            "trigger": {
                "kind": "run_now",
                "accountId": "acct-approved-pine",
                "nowUnixSecs": NOW,
            },
        })
        .to_string(),
    );
    let run_now_reply: serde_json::Value = serde_json::from_str(&run_now_reply).unwrap();
    println!("TASK1464_DIRECT_INVOKE_RUN_NOW={run_now_reply}");
    assert_eq!(run_now_reply["ok"], true);
    assert_eq!(run_now_reply["result"]["decision"], "start");
    assert_eq!(
        run_now_reply["result"]["data"]["accountId"],
        "acct-approved-pine"
    );

    println!("TASK1464_FINISH_LINE=met");
}
