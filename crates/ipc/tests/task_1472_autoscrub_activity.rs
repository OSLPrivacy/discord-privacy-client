//! TASK 1472 - write AutoScrub activity record.
//!
//! Finish line: a direct activity command returns all fields for a mixed
//! fixture run.

use ipc::autoscrub_activity::{
    run_autoscrub_activity_command, AutoScrubRunLocation, AutoScrubRunOutcome,
    AUTOSCRUB_ACTIVITY_GET_COMMAND, AUTOSCRUB_ACTIVITY_RECORD_COMMAND,
};

fn mixed_fixture() -> AutoScrubRunOutcome {
    AutoScrubRunOutcome {
        run_id: "run-1472-integration-mixed".to_owned(),
        account_id: "telegram-account-beta-1472".to_owned(),
        start_unix_secs: 1_786_200_000,
        end_unix_secs: 1_786_200_555,
        matched_count: 11,
        deleted_count: 7,
        failed_count: 3,
        location: AutoScrubRunLocation::Local,
    }
}

#[test]
fn a_direct_activity_command_returns_all_fields_for_a_mixed_fixture_run() {
    let outcome = mixed_fixture();

    let record_reply = run_autoscrub_activity_command(
        AUTOSCRUB_ACTIVITY_RECORD_COMMAND,
        &serde_json::to_string(&outcome).unwrap(),
    );
    let record_reply: serde_json::Value = serde_json::from_str(&record_reply).unwrap();
    println!("TASK1472_RECORD_REPLY={record_reply}");
    assert_eq!(record_reply["ok"], true);

    let get_reply = run_autoscrub_activity_command(
        AUTOSCRUB_ACTIVITY_GET_COMMAND,
        &serde_json::json!({ "runId": outcome.run_id }).to_string(),
    );
    let get_reply: serde_json::Value = serde_json::from_str(&get_reply).unwrap();
    println!("TASK1472_GET_REPLY={get_reply}");
    assert_eq!(get_reply["ok"], true);

    let result = &get_reply["result"];
    assert_eq!(result["runId"], "run-1472-integration-mixed");
    assert_eq!(result["accountId"], "telegram-account-beta-1472");
    assert_eq!(result["startUnixSecs"], 1_786_200_000i64);
    assert_eq!(result["endUnixSecs"], 1_786_200_555i64);
    assert_eq!(result["matchedCount"], 11);
    assert_eq!(result["deletedCount"], 7);
    assert_eq!(result["failedCount"], 3);
    assert_eq!(result["location"], "local");
    assert_eq!(
        result["text"],
        "telegram-account-beta-1472: matched 11, deleted 7, failed 3 (ran on this device)"
    );

    println!("TASK1472_FINISH_LINE=met");
}

#[test]
fn direct_command_with_unknown_run_id_is_not_found() {
    let reply = run_autoscrub_activity_command(
        AUTOSCRUB_ACTIVITY_GET_COMMAND,
        &serde_json::json!({ "runId": "run-does-not-exist-1472" }).to_string(),
    );
    let reply: serde_json::Value = serde_json::from_str(&reply).unwrap();
    assert_eq!(reply["ok"], false);
    assert_eq!(reply["errorCode"], "not_found");
}
