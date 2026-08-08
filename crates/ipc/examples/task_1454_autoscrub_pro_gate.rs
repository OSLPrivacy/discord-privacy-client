//! TASK 1454 direct invoke: name each AutoScrub command straight at the gate,
//! with no control listing and no screen in the way.
//!
//! Exits 0 only when every finish-line item held. Prints the exact JSON each
//! invoke returned.

use ipc::autoscrub_pro_gate::{
    fixture_pro_code_directory, run_autoscrub_pro_command, AutoScrubProSurface,
    AUTOSCRUB_ACTIVITY_COMMAND, AUTOSCRUB_DELETION_COMMAND, AUTOSCRUB_PRO_COMMANDS,
    AUTOSCRUB_SCHEDULE_COMMAND, AUTOSCRUB_SETUP_COMMAND, FIXTURE_ACTIVE_PRO_CODE,
    FIXTURE_EXPIRED_PRO_CODE, FIXTURE_REVOKED_PRO_CODE, PRO_CODE_REQUIRED,
};

const NEAR_MISS_CODES: [&str; 8] = [
    FIXTURE_EXPIRED_PRO_CODE,
    FIXTURE_REVOKED_PRO_CODE,
    "OSL-1454-AUTO-SCRB-PRO2",
    "OSL-1454-AUTO-SCRB-PR01",
    "osl-1454-auto-scrb-pro1",
    "OSL-1454-AUTO-SCRB-PRO1 ",
    " OSL-1454-AUTO-SCRB-PRO1",
    "",
];

fn request_json_for(command: &str) -> &'static str {
    match command {
        AUTOSCRUB_SETUP_COMMAND => r#"{"accounts":["discord-maple","telegram-pine"]}"#,
        AUTOSCRUB_SCHEDULE_COMMAND => {
            r#"{"scheduleName":"maple-daily","account":"discord-maple","cadence":"daily"}"#
        }
        AUTOSCRUB_DELETION_COMMAND => {
            r#"{"account":"discord-maple","markedLocators":["marked-1","marked-2","marked-3"]}"#
        }
        _ => "{}",
    }
}

fn parse(reply: &str) -> serde_json::Value {
    serde_json::from_str(reply).expect("every AutoScrub reply is a JSON object")
}

fn main() {
    let mut failures: Vec<String> = Vec::new();
    let mut surface = AutoScrubProSurface::new(fixture_pro_code_directory());

    println!("TASK1454_FIXTURE_ACTIVE_CODE={FIXTURE_ACTIVE_PRO_CODE}");
    println!(
        "TASK1454_FIXTURE_DIRECTORY_SIZE={}",
        surface.directory().len()
    );
    println!("TASK1454_COMMANDS={}", AUTOSCRUB_PRO_COMMANDS.join(","));

    // 1 + 2: Free, direct invoke, one command at a time.
    let mut refused_free = 0usize;
    for command in AUTOSCRUB_PRO_COMMANDS {
        let reply = run_autoscrub_pro_command(&mut surface, command, request_json_for(command));
        println!("TASK1454_FREE_{}={reply}", command.to_uppercase());
        let parsed = parse(&reply);
        if parsed["ok"] == false
            && parsed["command"] == command
            && parsed["errorCode"] == PRO_CODE_REQUIRED
            && parsed["result"].is_null()
        {
            refused_free += 1;
        } else {
            failures.push(format!("{command} was not refused by name for Free"));
        }
    }
    println!("TASK1454_FREE_REFUSED_COUNT={refused_free}");
    println!(
        "TASK1454_FREE_AVAILABLE_COUNT={}",
        surface.available_commands().len()
    );

    // 4 (first half): no near-miss code opens anything.
    let mut rejected_codes = 0usize;
    for candidate in NEAR_MISS_CODES {
        let verdict = surface.present_pro_code(candidate);
        let unlocked = surface.pro_unlocked();
        println!(
            "TASK1454_REJECTED_CODE code={candidate:?} verdict={} unlocked={unlocked} available={}",
            verdict.as_str(),
            surface.available_commands().len()
        );
        if !unlocked && !verdict.unlocks() && surface.available_commands().is_empty() {
            rejected_codes += 1;
        } else {
            failures.push(format!("{candidate:?} changed the AutoScrub gate"));
        }
    }
    println!("TASK1454_REJECTED_CODE_COUNT={rejected_codes}");

    // 3: the exact active fixture code.
    let verdict = surface.present_pro_code(FIXTURE_ACTIVE_PRO_CODE);
    println!("TASK1454_ACTIVE_CODE_VERDICT={}", verdict.as_str());
    println!(
        "TASK1454_UNLOCKED_AVAILABLE={}",
        surface.available_commands().join(",")
    );
    if !verdict.unlocks() || surface.available_commands().len() != 4 {
        failures.push("the exact active fixture code did not open all four controls".to_string());
    }

    let mut accepted_pro = 0usize;
    for command in AUTOSCRUB_PRO_COMMANDS {
        let reply = run_autoscrub_pro_command(&mut surface, command, request_json_for(command));
        println!("TASK1454_PRO_{}={reply}", command.to_uppercase());
        let parsed = parse(&reply);
        if parsed["ok"] == true && parsed["command"] == command && parsed["result"].is_object() {
            accepted_pro += 1;
        } else {
            failures.push(format!("{command} did not run for the active code"));
        }
    }
    println!("TASK1454_PRO_ACCEPTED_COUNT={accepted_pro}");

    // The activity history the refused Free invokes never produced: exactly the
    // three Pro actions above, in order.
    let history_reply = run_autoscrub_pro_command(&mut surface, AUTOSCRUB_ACTIVITY_COMMAND, "{}");
    let history = parse(&history_reply);
    let entry_count = history["result"]["entryCount"].as_u64().unwrap_or(0);
    println!("TASK1454_PRO_ACTIVITY_ENTRY_COUNT={entry_count}");
    if entry_count != 3 {
        failures.push(format!(
            "activity history recorded {entry_count} entries; expected the 3 Pro actions only"
        ));
    }

    // 4 (second half): a near-miss code presented while unlocked changes nothing.
    let mut undisturbed = 0usize;
    for candidate in NEAR_MISS_CODES {
        surface.present_pro_code(candidate);
        if surface.pro_unlocked() && surface.active_code() == Some(FIXTURE_ACTIVE_PRO_CODE) {
            undisturbed += 1;
        } else {
            failures.push(format!("{candidate:?} disturbed the active code"));
        }
    }
    println!("TASK1454_ACTIVE_CODE_UNDISTURBED_BY={undisturbed}");

    if failures.is_empty() {
        println!("TASK1454_FINISH_LINE=met");
        return;
    }
    for failure in &failures {
        eprintln!("TASK1454_FAILURE={failure}");
    }
    eprintln!("TASK1454_ERROR=finish line mismatch");
    std::process::exit(1);
}
