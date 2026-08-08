//! TASK 1476 - write AutoScrub notices.
//!
//! Finish line: fixture notices contain the exact scheduled account count and
//! the exact finished deletion count, both matching the run records.
//!
//! Nothing here hand-writes a count into a notice. The fixture records four
//! runs through TASK 1472's activity command, sends the batch's notices
//! through TASK 1476's send command, then reads the counts back out of 1472's
//! own `autoscrub_activity_get` replies and asserts the notice text carries
//! exactly those numbers.

use ipc::autoscrub_activity::{
    run_autoscrub_activity_command, AutoScrubRunLocation, AutoScrubRunOutcome,
    AUTOSCRUB_ACTIVITY_GET_COMMAND, AUTOSCRUB_ACTIVITY_RECORD_COMMAND,
};
use ipc::autoscrub_notices::{
    run_autoscrub_notices_command, AUTOSCRUB_NOTICES_LIST_COMMAND, AUTOSCRUB_NOTICES_SEND_COMMAND,
    AUTOSCRUB_NOTICE_OPEN_COMMAND,
};
use serde_json::Value;

const BATCH_ID: &str = "batch-1476-nightly";

/// Four scheduled accounts, all four finished. Deletions: 9 + 6 + 11 + 0.
/// Failures on two of them.
fn fixture_runs() -> Vec<AutoScrubRunOutcome> {
    vec![
        run("run-1476-a", "discord-account-alpha-1476", 12, 9, 3),
        run("run-1476-b", "telegram-account-beta-1476", 6, 6, 0),
        run("run-1476-c", "signal-account-gamma-1476", 14, 11, 2),
        run("run-1476-d", "instagram-account-delta-1476", 0, 0, 0),
    ]
}

fn run(
    run_id: &str,
    account_id: &str,
    matched: u32,
    deleted: u32,
    failed: u32,
) -> AutoScrubRunOutcome {
    AutoScrubRunOutcome {
        run_id: run_id.to_owned(),
        account_id: account_id.to_owned(),
        start_unix_secs: 1_786_400_000,
        end_unix_secs: 1_786_400_900,
        matched_count: matched,
        deleted_count: deleted,
        failed_count: failed,
        location: AutoScrubRunLocation::Local,
    }
}

fn reply(command: &'static str, request: Value, notices: bool) -> Value {
    let raw = if notices {
        run_autoscrub_notices_command(command, &request.to_string())
    } else {
        run_autoscrub_activity_command(command, &request.to_string())
    };
    serde_json::from_str(&raw).expect("every reply is JSON")
}

/// Records the fixture runs through TASK 1472 and returns their run ids.
fn record_fixture_runs() -> Vec<String> {
    let mut run_ids = Vec::new();
    for outcome in fixture_runs() {
        let recorded = reply(
            AUTOSCRUB_ACTIVITY_RECORD_COMMAND,
            serde_json::to_value(&outcome).unwrap(),
            false,
        );
        assert_eq!(recorded["ok"], true, "run {} records", outcome.run_id);
        run_ids.push(outcome.run_id);
    }
    run_ids
}

/// Reads a run's saved activity record back through TASK 1472's get command.
fn record_for(run_id: &str) -> Value {
    let got = reply(
        AUTOSCRUB_ACTIVITY_GET_COMMAND,
        serde_json::json!({ "runId": run_id }),
        false,
    );
    assert_eq!(got["ok"], true, "run {run_id} has a record");
    got["result"].clone()
}

#[test]
fn fixture_notices_carry_the_exact_scheduled_account_count_and_deletion_count() {
    let run_ids = record_fixture_runs();
    let scheduled_account_ids: Vec<String> = fixture_runs()
        .iter()
        .map(|r| r.account_id.clone())
        .collect();

    // The two numbers the finish line is about, taken from the saved records
    // -- not from the fixture literals and not from the notices.
    let scheduled_from_records = run_ids.len();
    let deleted_from_records: u64 = run_ids
        .iter()
        .map(|run_id| record_for(run_id)["deletedCount"].as_u64().expect("count"))
        .sum();
    println!("TASK1476_RECORDS_SCHEDULED_ACCOUNTS={scheduled_from_records}");
    println!("TASK1476_RECORDS_DELETED_TOTAL={deleted_from_records}");

    let sent = reply(
        AUTOSCRUB_NOTICES_SEND_COMMAND,
        serde_json::json!({
            "batchId": BATCH_ID,
            "scheduledAccountIds": scheduled_account_ids,
            "finishedRunIds": run_ids,
            "nowUnixSecs": 1_786_401_000i64,
        }),
        true,
    );
    assert_eq!(sent["ok"], true, "notices send: {sent}");
    let set = &sent["result"];

    // --- before-run notice: the exact scheduled account count -------------
    let before_run = &set["beforeRun"];
    println!("TASK1476_BEFORE_RUN_ID={}", before_run["id"]);
    println!("TASK1476_BEFORE_RUN_TITLE={}", before_run["title"]);
    println!("TASK1476_BEFORE_RUN_BODY={}", before_run["body"]);
    assert_eq!(before_run["kind"], "before_run");
    assert_eq!(
        before_run["scheduledAccountCount"].as_u64().unwrap(),
        scheduled_from_records as u64
    );
    assert_eq!(
        before_run["body"],
        format!("AutoScrub is scheduled to run for {scheduled_from_records} accounts.")
    );
    assert_eq!(
        before_run["body"], "AutoScrub is scheduled to run for 4 accounts.",
        "the fixture's scheduled account count is 4"
    );

    // --- per-account notices: one each, quoting its own record ------------
    let per_account = set["perAccount"].as_array().expect("per-account notices");
    assert_eq!(per_account.len(), scheduled_from_records);
    for (notice, run_id) in per_account.iter().zip(run_ids.iter()) {
        let record = record_for(run_id);
        println!("TASK1476_ACCOUNT_NOTICE_BODY={}", notice["body"]);
        assert_eq!(notice["kind"], "account");
        assert_eq!(notice["accountId"], record["accountId"]);
        assert_eq!(notice["opens"]["runId"], *run_id);
        assert_eq!(
            notice["body"],
            format!(
                "{}: matched {}, deleted {}, failed {}.",
                record["accountId"].as_str().unwrap(),
                record["matchedCount"].as_u64().unwrap(),
                record["deletedCount"].as_u64().unwrap(),
                record["failedCount"].as_u64().unwrap(),
            )
        );
    }

    // --- deletion-count notice: the exact finished deletion count ---------
    let deletion = &set["deletionCount"];
    println!("TASK1476_DELETION_NOTICE_TITLE={}", deletion["title"]);
    println!("TASK1476_DELETION_NOTICE_BODY={}", deletion["body"]);
    assert_eq!(deletion["kind"], "deletion_count");
    assert_eq!(
        deletion["deletedCount"].as_u64().unwrap(),
        deleted_from_records
    );
    assert_eq!(
        deletion["body"],
        format!(
            "AutoScrub finished {scheduled_from_records} of {scheduled_from_records} scheduled \
             accounts and deleted {deleted_from_records} messages."
        )
    );
    assert_eq!(
        deletion["body"], "AutoScrub finished 4 of 4 scheduled accounts and deleted 26 messages.",
        "the fixture's finished deletion count is 26"
    );
    assert_eq!(deletion["title"], "AutoScrub deleted 26 messages");

    // --- failure notices: one per record that failed anything -------------
    let failures = set["failures"].as_array().expect("failure notices");
    let failed_records: Vec<Value> = run_ids
        .iter()
        .map(|run_id| record_for(run_id))
        .filter(|r| r["failedCount"].as_u64().unwrap() > 0)
        .collect();
    assert_eq!(failures.len(), failed_records.len());
    for (notice, record) in failures.iter().zip(failed_records.iter()) {
        println!("TASK1476_FAILURE_NOTICE_BODY={}", notice["body"]);
        assert_eq!(notice["kind"], "failure");
        assert_eq!(notice["accountId"], record["accountId"]);
        assert_eq!(
            notice["body"],
            format!(
                "{}: {} messages could not be deleted.",
                record["accountId"].as_str().unwrap(),
                record["failedCount"].as_u64().unwrap(),
            )
        );
    }

    // --- the set's own totals agree with the records ----------------------
    assert_eq!(
        set["scheduledAccountCount"].as_u64().unwrap(),
        scheduled_from_records as u64
    );
    assert_eq!(
        set["finishedAccountCount"].as_u64().unwrap(),
        scheduled_from_records as u64
    );
    assert_eq!(
        set["totalDeletedCount"].as_u64().unwrap(),
        deleted_from_records
    );

    println!("TASK1476_NOTICE_SCHEDULED_ACCOUNT_COUNT={scheduled_from_records}");
    println!("TASK1476_NOTICE_DELETED_COUNT={deleted_from_records}");
    println!("TASK1476_FINISH_LINE=met");
}

#[test]
fn every_sent_notice_opens_the_activity_behind_it() {
    let run_ids = record_fixture_runs();
    let scheduled_account_ids: Vec<String> = fixture_runs()
        .iter()
        .map(|r| r.account_id.clone())
        .collect();

    let sent = reply(
        AUTOSCRUB_NOTICES_SEND_COMMAND,
        serde_json::json!({
            "batchId": BATCH_ID,
            "scheduledAccountIds": scheduled_account_ids,
            "finishedRunIds": run_ids,
            "nowUnixSecs": 1_786_401_000i64,
        }),
        true,
    );
    assert_eq!(sent["ok"], true);

    let listed = reply(
        AUTOSCRUB_NOTICES_LIST_COMMAND,
        serde_json::json!({ "batchId": BATCH_ID }),
        true,
    );
    assert_eq!(listed["ok"], true);
    let notices = listed["result"]["notices"].as_array().expect("notices");
    // before-run + 4 per-account + deletion-count + 2 failures.
    assert_eq!(notices.len(), 8);

    let mut opened_total_deleted = 0u64;
    for notice in notices {
        let opened = reply(
            AUTOSCRUB_NOTICE_OPEN_COMMAND,
            serde_json::json!({ "noticeId": notice["id"] }),
            true,
        );
        assert_eq!(
            opened["ok"], true,
            "notice {} opens: {opened}",
            notice["id"]
        );
        let records = opened["result"]["records"].as_array().expect("records");
        assert!(
            !records.is_empty(),
            "notice {} opened no activity",
            notice["id"]
        );
        let expected = if notice["opens"]["runId"].is_null() {
            run_ids.len()
        } else {
            1
        };
        assert_eq!(records.len(), expected, "notice {}", notice["id"]);
        println!(
            "TASK1476_OPEN notice={} kind={} opened_records={}",
            notice["id"],
            notice["kind"],
            records.len()
        );
        if notice["kind"] == "deletion_count" {
            opened_total_deleted = records
                .iter()
                .map(|r| r["deletedCount"].as_u64().unwrap())
                .sum();
        }
    }

    // The deletion-count notice opens exactly the records its number came from.
    assert_eq!(opened_total_deleted, 26);
    println!("TASK1476_OPENED_DELETED_TOTAL={opened_total_deleted}");
}

#[test]
fn a_notice_cannot_be_sent_for_a_run_with_no_activity_record() {
    let sent = reply(
        AUTOSCRUB_NOTICES_SEND_COMMAND,
        serde_json::json!({
            "batchId": "batch-1476-unrecorded",
            "scheduledAccountIds": ["account-1476-unrecorded"],
            "finishedRunIds": ["run-1476-never-recorded"],
            "nowUnixSecs": 1_786_401_000i64,
        }),
        true,
    );
    assert_eq!(sent["ok"], false);
    assert_eq!(sent["errorCode"], "bad_request");
    assert_eq!(
        sent["error"],
        "no AutoScrub activity record for run run-1476-never-recorded; \
         cannot send a notice about it"
    );
}
