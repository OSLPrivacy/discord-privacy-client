//! TASK 1466 - break asleep schedule.
//!
//! Trigger a due schedule while the fixture computer is asleep. The same
//! check must reject an otherwise identical fixture after only the sleep
//! state is removed.

use ipc::scheduled_runner_rules::{
    run_scheduled_runner_command, AccountAvailability, ScheduledRunnerQueue,
    SCHEDULED_RUNNER_DECIDE_COMMAND,
};
use serde_json::{json, Value};
use std::{env, fs, path::Path};

const NOW: i64 = 1_786_190_400;
const ACCOUNT_ID: &str = "discord-maple";
const PAUSED_FOR_SLEEP: &str = "paused for sleep";

#[derive(Clone)]
struct FixtureComputer {
    queue: ScheduledRunnerQueue,
    opened_accounts: Vec<String>,
    activity: Vec<String>,
}

impl FixtureComputer {
    fn from_queue(queue: ScheduledRunnerQueue) -> Self {
        Self {
            queue,
            opened_accounts: Vec::new(),
            activity: Vec::new(),
        }
    }

    fn trigger_due_schedule(&mut self) {
        let reply = run_scheduled_runner_command(
            SCHEDULED_RUNNER_DECIDE_COMMAND,
            &json!({
                "queue": &self.queue,
                "trigger": { "kind": "scheduled_tick", "nowUnixSecs": NOW },
            })
            .to_string(),
        );
        let reply: Value = serde_json::from_str(&reply).expect("runner reply is JSON");
        assert_eq!(reply["ok"], true, "direct runner command must succeed");

        match reply["result"]["decision"].as_str() {
            Some("start") => {
                self.opened_accounts.push(
                    reply["result"]["data"]["accountId"]
                        .as_str()
                        .expect("start decision names its account")
                        .to_owned(),
                );
                self.activity.push("started scheduled account".to_owned());
            }
            Some("no_action") => self.activity.push(
                reply["result"]["data"]["detail"]
                    .as_str()
                    .expect("paused decision carries activity text")
                    .to_owned(),
            ),
            other => panic!("unexpected runner decision: {other:?}"),
        }
    }
}

fn check_due_schedule_pauses_for_sleep(fixture: &mut FixtureComputer) -> Result<(), String> {
    fixture.trigger_due_schedule();
    if !fixture.opened_accounts.is_empty() {
        return Err(format!(
            "expected 0 opened accounts, got {}: {:?}",
            fixture.opened_accounts.len(),
            fixture.opened_accounts
        ));
    }
    if fixture.activity.as_slice() != [PAUSED_FOR_SLEEP] {
        return Err(format!(
            "expected activity {PAUSED_FOR_SLEEP:?}, got {:?}",
            fixture.activity
        ));
    }
    Ok(())
}

fn parse_fixture(source: &str) -> ScheduledRunnerQueue {
    serde_json::from_str(source).expect("task 1466 fixture must match ScheduledRunnerQueue")
}

fn selected_fixture() -> ScheduledRunnerQueue {
    match env::var_os("OSL_TASK_1466_FIXTURE") {
        Some(path) => {
            let path = Path::new(&path);
            let source = fs::read_to_string(path).or_else(|_| {
                let workspace_relative = Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../..")
                    .join(path);
                fs::read_to_string(workspace_relative)
            });
            parse_fixture(&source.expect("OSL_TASK_1466_FIXTURE must be a readable file"))
        }
        None => parse_fixture(include_str!("fixtures/task-1466-computer-asleep.json")),
    }
}

#[test]
fn due_schedule_while_asleep_opens_no_account_and_records_pause_activity() {
    let mut asleep = FixtureComputer::from_queue(selected_fixture());
    check_due_schedule_pauses_for_sleep(&mut asleep)
        .expect("the asleep fixture must meet the finish line");

    println!(
        "TASK1466_ASLEEP_OPENED_ACCOUNTS={}",
        asleep.opened_accounts.len()
    );
    println!("TASK1466_ASLEEP_ACTIVITY={}", asleep.activity[0]);

    assert_eq!(asleep.opened_accounts.len(), 0);
    assert_eq!(asleep.activity, [PAUSED_FOR_SLEEP]);
}

#[test]
fn the_same_check_fails_when_the_fixture_has_no_sleep_state() {
    let awake_queue = parse_fixture(include_str!("fixtures/task-1466-computer-awake.json"));
    assert_eq!(
        awake_queue.accounts[0].availability,
        AccountAvailability::Available
    );
    let mut awake = FixtureComputer::from_queue(awake_queue);
    let failure = check_due_schedule_pauses_for_sleep(&mut awake)
        .expect_err("removing only the sleep state must make this check fail");

    println!("TASK1466_NO_SLEEP_CHECK=FAILED");
    println!(
        "TASK1466_NO_SLEEP_OPENED_ACCOUNTS={}",
        awake.opened_accounts.len()
    );
    println!("TASK1466_NO_SLEEP_FAILURE={failure}");

    assert_eq!(awake.opened_accounts, [ACCOUNT_ID]);
    assert_eq!(
        failure,
        "expected 0 opened accounts, got 1: [\"discord-maple\"]"
    );
}
