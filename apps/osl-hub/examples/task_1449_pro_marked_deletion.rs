//! Direct command for TASK 1449.
//!
//! Runs the Pro marked-deletion command end to end against a fixture that
//! reuses the TASK 1444 account ids, and checks the finish line itself:
//! Free is refused, and Pro receives the marked count before confirmation.
//! Exits 1 with `TASK1449_ERROR=finish line mismatch` if either fails.

use serde_json::Value;

use osl_privacy_hub::pro_marked_deletion::{run_pro_marked_deletion_command, PRO_REQUIRED_REFUSAL};

const FIXTURE: &str = r#"[
    {"accountId":"discord-account-alpha-1444","messageLocator":"blocked-local-reference-1","decision":"markedForDeletion","reviewed":true},
    {"accountId":"discord-account-alpha-1444","messageLocator":"blocked-local-reference-2","decision":"kept","reviewed":true},
    {"accountId":"telegram-account-beta-1444","messageLocator":"blocked-local-reference-3","decision":"markedForDeletion","reviewed":true},
    {"accountId":"telegram-account-beta-1444","messageLocator":"blocked-local-reference-4","decision":"pending","reviewed":false}
]"#;

fn usage() -> ! {
    eprintln!("usage: task_1449_pro_marked_deletion marked-deletion");
    std::process::exit(2);
}

fn parse(label: &str, reply: &str) -> Value {
    serde_json::from_str(reply).unwrap_or_else(|error| {
        eprintln!("TASK1449_ERROR={label} reply is not json: {error}");
        std::process::exit(1);
    })
}

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let Some("marked-deletion") = args.first().map(String::as_str) else {
        usage();
    };
    if args.len() != 1 {
        usage();
    }

    let messages = FIXTURE.replace(['\n', ' '], "");
    println!("TASK1449_DIRECT_COMMAND=marked-deletion");
    println!("TASK1449_FIXTURE_MESSAGE_COUNT=4");

    // Free asks for the count first, exactly as Pro would.
    let free_count = run_pro_marked_deletion_command(
        "count",
        &format!("{{\"plan\":\"free\",\"messages\":{messages}}}"),
    );
    let free_count_value = parse("free count", &free_count);
    println!("TASK1449_FREE_COUNT={free_count}");

    // Free then tries to delete outright.
    let free_delete = run_pro_marked_deletion_command(
        "delete",
        &format!(
            "{{\"plan\":\"free\",\"messages\":{messages},\"confirmationToken\":\"any\",\"confirmed\":true}}"
        ),
    );
    let free_delete_value = parse("free delete", &free_delete);
    println!("TASK1449_FREE_DELETE={free_delete}");

    let pro_count = run_pro_marked_deletion_command(
        "count",
        &format!("{{\"plan\":\"pro\",\"messages\":{messages}}}"),
    );
    let pro_count_value = parse("pro count", &pro_count);
    println!("TASK1449_PRO_COUNT={pro_count}");

    let token = pro_count_value["result"]["confirmationToken"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let marked_count = pro_count_value["result"]["markedCount"].as_u64();
    println!(
        "TASK1449_PRO_MARKED_COUNT={}",
        marked_count
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_owned())
    );
    println!(
        "TASK1449_PRO_CONFIRMATION_PROMPT={}",
        pro_count_value["result"]["confirmationPrompt"]
            .as_str()
            .unwrap_or("none")
    );

    // The count has to arrive before the confirmation: without the token that
    // only the count issues, deletion is refused.
    let pro_no_count = run_pro_marked_deletion_command(
        "delete",
        &format!(
            "{{\"plan\":\"pro\",\"messages\":{messages},\"confirmationToken\":\"\",\"confirmed\":true}}"
        ),
    );
    let pro_no_count_value = parse("pro delete without count", &pro_no_count);
    println!("TASK1449_PRO_DELETE_WITHOUT_COUNT={pro_no_count}");

    let pro_unconfirmed = run_pro_marked_deletion_command(
        "delete",
        &format!(
            "{{\"plan\":\"pro\",\"messages\":{messages},\"confirmationToken\":\"{token}\",\"confirmed\":false}}"
        ),
    );
    let pro_unconfirmed_value = parse("pro delete unconfirmed", &pro_unconfirmed);
    println!("TASK1449_PRO_DELETE_UNCONFIRMED={pro_unconfirmed}");

    let pro_delete = run_pro_marked_deletion_command(
        "delete",
        &format!(
            "{{\"plan\":\"pro\",\"messages\":{messages},\"confirmationToken\":\"{token}\",\"confirmed\":true}}"
        ),
    );
    let pro_delete_value = parse("pro delete", &pro_delete);
    println!("TASK1449_PRO_DELETE={pro_delete}");
    println!(
        "TASK1449_PRO_DELETED_COUNT={}",
        pro_delete_value["result"]["deletedCount"]
            .as_u64()
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_owned())
    );

    let mut failures = Vec::new();

    // Free is refused, and is told why, and gets no count at all.
    if free_count_value["ok"] != Value::Bool(false)
        || free_count_value["errorCode"] != Value::String("pro_required".to_owned())
        || free_count_value["error"] != Value::String(PRO_REQUIRED_REFUSAL.to_owned())
        || free_count_value.get("result").is_some()
    {
        failures.push("free count was not refused with pro_required and no result".to_owned());
    }
    if free_delete_value["ok"] != Value::Bool(false)
        || free_delete_value["errorCode"] != Value::String("pro_required".to_owned())
        || free_delete_value.get("result").is_some()
    {
        failures.push("free delete was not refused with pro_required".to_owned());
    }

    // Pro receives the marked count.
    if pro_count_value["ok"] != Value::Bool(true) || marked_count != Some(2) {
        failures.push("pro did not receive markedCount=2".to_owned());
    }
    if token.len() != 64 {
        failures.push("pro count did not issue a confirmation token".to_owned());
    }
    if pro_count_value["result"]["confirmationRequired"] != Value::Bool(true) {
        failures.push("pro count did not require a confirmation".to_owned());
    }
    if pro_count_value["result"]["confirmationPrompt"]
        != Value::String("Delete 2 marked messages? This cannot be undone.".to_owned())
    {
        failures.push("pro count prompt did not carry the count".to_owned());
    }

    // The count comes before the confirmation.
    if pro_no_count_value["ok"] != Value::Bool(false)
        || pro_no_count_value["errorCode"] != Value::String("count_not_shown".to_owned())
    {
        failures.push("pro delete without a shown count was not refused".to_owned());
    }
    if pro_unconfirmed_value["ok"] != Value::Bool(false)
        || pro_unconfirmed_value["errorCode"] != Value::String("not_confirmed".to_owned())
    {
        failures.push("pro delete without a confirmation was not refused".to_owned());
    }

    // Only the reviewed, marked messages go.
    if pro_delete_value["ok"] != Value::Bool(true)
        || pro_delete_value["result"]["deletedCount"] != Value::Number(2.into())
        || pro_delete_value["result"]["keptUntouchedCount"] != Value::Number(2.into())
    {
        failures
            .push("pro delete did not remove exactly the 2 reviewed marked messages".to_owned());
    }

    if failures.is_empty() {
        println!("TASK1449_FINISH_LINE=met");
        return;
    }
    for failure in &failures {
        eprintln!("TASK1449_FAILURE={failure}");
    }
    eprintln!("TASK1449_ERROR=finish line mismatch");
    std::process::exit(1);
}
