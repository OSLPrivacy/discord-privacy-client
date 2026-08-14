//! Direct command for TASK 1451.
//!
//! Runs the deletion-outcome recorder against a mixed fixture (one deleted,
//! one failed, one needs-attention, plus a second deleted message) and checks
//! the finish line itself: all three outcomes come back, each still carrying
//! its account and locator. Exits 1 with `TASK1451_ERROR=finish line
//! mismatch` if it does not.

use serde_json::Value;

use osl_privacy_hub::pro_marked_deletion_outcomes::run_pro_marked_deletion_outcomes_command;

const FIXTURE: &str = r#"[{"message":{"accountId":"discord-account-alpha-1444","messageLocator":"blocked-local-reference-1"},"result":{"kind":"deleted"}},{"message":{"accountId":"telegram-account-beta-1444","messageLocator":"blocked-local-reference-3"},"result":{"kind":"failed","reason":"carrier rejected the delete request"}},{"message":{"accountId":"signal-account-gamma-1444","messageLocator":"blocked-local-reference-7"},"result":{"kind":"needsAttention","reason":"locator no longer resolves; message may have moved"}},{"message":{"accountId":"discord-account-alpha-1444","messageLocator":"blocked-local-reference-9"},"result":{"kind":"deleted"}}]"#;

fn usage() -> ! {
    eprintln!("usage: task_1451_pro_marked_deletion_outcomes record");
    std::process::exit(2);
}

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let Some("record") = args.first().map(String::as_str) else {
        usage();
    };
    if args.len() != 1 {
        usage();
    }

    println!("TASK1451_DIRECT_COMMAND=record");
    println!("TASK1451_FIXTURE_ATTEMPT_COUNT=4");

    let reply = run_pro_marked_deletion_outcomes_command(FIXTURE);
    println!("TASK1451_REPLY={reply}");
    let value: Value = serde_json::from_str(&reply).unwrap_or_else(|error| {
        eprintln!("TASK1451_ERROR=reply is not json: {error}");
        std::process::exit(1);
    });

    let mut failures = Vec::new();

    if value["ok"] != Value::Bool(true) {
        failures.push("record was not accepted as ok".to_owned());
    }
    if value["result"]["deletedCount"] != Value::Number(2.into()) {
        failures.push("deletedCount was not 2".to_owned());
    }
    if value["result"]["failedCount"] != Value::Number(1.into()) {
        failures.push("failedCount was not 1".to_owned());
    }
    if value["result"]["needsAttentionCount"] != Value::Number(1.into()) {
        failures.push("needsAttentionCount was not 1".to_owned());
    }
    if value["result"]["deleted"][0]["accountId"]
        != Value::String("discord-account-alpha-1444".to_owned())
        || value["result"]["deleted"][0]["messageLocator"]
            != Value::String("blocked-local-reference-1".to_owned())
    {
        failures.push("deleted[0] lost its location".to_owned());
    }
    if value["result"]["failed"][0]["accountId"]
        != Value::String("telegram-account-beta-1444".to_owned())
        || value["result"]["failed"][0]["messageLocator"]
            != Value::String("blocked-local-reference-3".to_owned())
        || value["result"]["failed"][0]["reason"]
            != Value::String("carrier rejected the delete request".to_owned())
    {
        failures.push("failed[0] lost its location or reason".to_owned());
    }
    if value["result"]["needsAttention"][0]["accountId"]
        != Value::String("signal-account-gamma-1444".to_owned())
        || value["result"]["needsAttention"][0]["messageLocator"]
            != Value::String("blocked-local-reference-7".to_owned())
        || value["result"]["needsAttention"][0]["reason"]
            != Value::String("locator no longer resolves; message may have moved".to_owned())
    {
        failures.push("needsAttention[0] lost its location or reason".to_owned());
    }

    if failures.is_empty() {
        println!("TASK1451_FINISH_LINE=met");
        return;
    }
    for failure in &failures {
        eprintln!("TASK1451_FAILURE={failure}");
    }
    eprintln!("TASK1451_ERROR=finish line mismatch");
    std::process::exit(1);
}
