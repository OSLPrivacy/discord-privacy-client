//! TASK 1472 - write AutoScrub activity record.
//!
//! Once a reviewed AutoScrub run finishes -- whether it ran on this device
//! or on a disposable cloud worker (see `apps/osl-hub`'s `autoscrub_run.rs`
//! and `cloud_autoscrub_execution.rs`) -- something has to keep a record of
//! what actually happened: which account, when it started and ended, how
//! many messages it matched, how many it deleted, how many attempts failed,
//! and where it ran. This module is that record: a pure builder plus a
//! small in-memory store, so a direct activity command can answer "what
//! happened on this run" with every field intact.

use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};

pub const AUTOSCRUB_ACTIVITY_RECORD_COMMAND: &str = "autoscrub_activity_record";
pub const AUTOSCRUB_ACTIVITY_GET_COMMAND: &str = "autoscrub_activity_get";

/// Where the run actually executed: on this device, or on a disposable
/// cloud worker.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoScrubRunLocation {
    Local,
    Cloud,
}

impl AutoScrubRunLocation {
    fn label(self) -> &'static str {
        match self {
            Self::Local => "this device",
            Self::Cloud => "a disposable cloud worker",
        }
    }
}

/// What the caller already knows about a finished run. No I/O and no clock
/// read happen here -- `start_unix_secs`/`end_unix_secs` are explicit
/// inputs, matching every other pure module in this crate.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutoScrubRunOutcome {
    pub run_id: String,
    pub account_id: String,
    pub start_unix_secs: i64,
    pub end_unix_secs: i64,
    pub matched_count: u32,
    pub deleted_count: u32,
    pub failed_count: u32,
    pub location: AutoScrubRunLocation,
}

/// The saved activity record for one run: start, end, account, matches,
/// deletions, failures, a human-readable summary text, and the location it
/// ran in.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubActivityRecord {
    pub run_id: String,
    pub account_id: String,
    pub start_unix_secs: i64,
    pub end_unix_secs: i64,
    pub matched_count: u32,
    pub deleted_count: u32,
    pub failed_count: u32,
    pub text: String,
    pub location: AutoScrubRunLocation,
}

fn summary_text(outcome: &AutoScrubRunOutcome) -> String {
    format!(
        "{account}: matched {matched}, deleted {deleted}, failed {failed} (ran on {location})",
        account = outcome.account_id,
        matched = outcome.matched_count,
        deleted = outcome.deleted_count,
        failed = outcome.failed_count,
        location = outcome.location.label(),
    )
}

/// Builds the activity record for one finished run. Pure: given the same
/// outcome, always returns the same record.
pub fn build_activity_record(
    outcome: &AutoScrubRunOutcome,
) -> Result<AutoScrubActivityRecord, String> {
    if outcome.end_unix_secs < outcome.start_unix_secs {
        return Err("AutoScrub run cannot end before it starts".to_owned());
    }
    Ok(AutoScrubActivityRecord {
        run_id: outcome.run_id.clone(),
        account_id: outcome.account_id.clone(),
        start_unix_secs: outcome.start_unix_secs,
        end_unix_secs: outcome.end_unix_secs,
        matched_count: outcome.matched_count,
        deleted_count: outcome.deleted_count,
        failed_count: outcome.failed_count,
        text: summary_text(outcome),
        location: outcome.location,
    })
}

#[derive(Default)]
struct AutoScrubActivityStore {
    records: Vec<AutoScrubActivityRecord>,
}

static ACTIVITY_STORE: OnceLock<Mutex<AutoScrubActivityStore>> = OnceLock::new();

fn activity_store() -> &'static Mutex<AutoScrubActivityStore> {
    ACTIVITY_STORE.get_or_init(|| Mutex::new(AutoScrubActivityStore::default()))
}

#[cfg(test)]
fn reset_activity_store_for_test() {
    *activity_store()
        .lock()
        .expect("AutoScrub activity store lock") = AutoScrubActivityStore::default();
}

/// Builds the activity record for `outcome` and saves it, replacing any
/// earlier record for the same `run_id`.
pub fn record_autoscrub_activity(
    outcome: &AutoScrubRunOutcome,
) -> Result<AutoScrubActivityRecord, String> {
    let record = build_activity_record(outcome)?;
    let mut store = activity_store()
        .lock()
        .map_err(|_| "AutoScrub activity store is unavailable".to_owned())?;
    store.records.retain(|r| r.run_id != record.run_id);
    store.records.push(record.clone());
    Ok(record)
}

/// Returns the saved activity record for `run_id`, if one has been recorded.
pub fn get_autoscrub_activity(run_id: &str) -> Result<Option<AutoScrubActivityRecord>, String> {
    let store = activity_store()
        .lock()
        .map_err(|_| "AutoScrub activity store is unavailable".to_owned())?;
    Ok(store.records.iter().find(|r| r.run_id == run_id).cloned())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AutoScrubActivityGetRequest {
    run_id: String,
}

/// The direct-invoke surface: `autoscrub_activity_record` saves a run's
/// activity record, `autoscrub_activity_get` reads it back. Every reply is a
/// JSON object carrying `ok`, matching the convention the other
/// `crates/ipc` command modules use.
pub fn run_autoscrub_activity_command(command: &str, request_json: &str) -> String {
    match command {
        AUTOSCRUB_ACTIVITY_RECORD_COMMAND => {
            let outcome: AutoScrubRunOutcome = match serde_json::from_str(request_json) {
                Ok(outcome) => outcome,
                Err(err) => return json_error_reply(command, "invalid_request", &err.to_string()),
            };
            match record_autoscrub_activity(&outcome) {
                Ok(record) => json_ok_reply(command, &record),
                Err(err) => json_error_reply(command, "bad_request", &err),
            }
        }
        AUTOSCRUB_ACTIVITY_GET_COMMAND => {
            let request: AutoScrubActivityGetRequest = match serde_json::from_str(request_json) {
                Ok(request) => request,
                Err(err) => return json_error_reply(command, "invalid_request", &err.to_string()),
            };
            match get_autoscrub_activity(&request.run_id) {
                Ok(Some(record)) => json_ok_reply(command, &record),
                Ok(None) => {
                    json_error_reply(command, "not_found", "no activity record for this run_id")
                }
                Err(err) => json_error_reply(command, "bad_request", &err),
            }
        }
        _ => json_error_reply(
            command,
            "unknown_command",
            &format!("unknown command '{command}'"),
        ),
    }
}

fn json_ok_reply<T: Serialize>(command: &str, result: &T) -> String {
    let result = serde_json::to_value(result).expect("AutoScrubActivityRecord always serializes");
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

    fn mixed_fixture() -> AutoScrubRunOutcome {
        AutoScrubRunOutcome {
            run_id: "run-1472-mixed".to_owned(),
            account_id: "discord-account-alpha-1472".to_owned(),
            start_unix_secs: 1_786_100_000,
            end_unix_secs: 1_786_100_420,
            matched_count: 9,
            deleted_count: 6,
            failed_count: 2,
            location: AutoScrubRunLocation::Cloud,
        }
    }

    #[test]
    fn a_mixed_fixture_run_builds_a_record_with_every_field() {
        let outcome = mixed_fixture();
        let record = build_activity_record(&outcome).expect("mixed fixture is valid");
        assert_eq!(record.run_id, "run-1472-mixed");
        assert_eq!(record.account_id, "discord-account-alpha-1472");
        assert_eq!(record.start_unix_secs, 1_786_100_000);
        assert_eq!(record.end_unix_secs, 1_786_100_420);
        assert_eq!(record.matched_count, 9);
        assert_eq!(record.deleted_count, 6);
        assert_eq!(record.failed_count, 2);
        assert_eq!(record.location, AutoScrubRunLocation::Cloud);
        assert_eq!(
            record.text,
            "discord-account-alpha-1472: matched 9, deleted 6, failed 2 (ran on a disposable cloud worker)"
        );
    }

    #[test]
    fn an_end_before_start_is_refused() {
        let mut outcome = mixed_fixture();
        outcome.end_unix_secs = outcome.start_unix_secs - 1;
        let err = build_activity_record(&outcome).expect_err("must be refused");
        assert_eq!(err, "AutoScrub run cannot end before it starts");
    }

    #[test]
    fn recording_and_reading_back_a_run_returns_the_same_record() {
        reset_activity_store_for_test();
        let outcome = mixed_fixture();
        let recorded = record_autoscrub_activity(&outcome).expect("records cleanly");
        let fetched = get_autoscrub_activity(&outcome.run_id)
            .expect("store is available")
            .expect("record exists");
        assert_eq!(recorded, fetched);
    }

    #[test]
    fn recording_twice_for_the_same_run_id_replaces_the_earlier_record() {
        reset_activity_store_for_test();
        let mut outcome = mixed_fixture();
        record_autoscrub_activity(&outcome).expect("first record");
        outcome.deleted_count = 7;
        outcome.failed_count = 1;
        record_autoscrub_activity(&outcome).expect("second record");

        let fetched = get_autoscrub_activity(&outcome.run_id)
            .expect("store is available")
            .expect("record exists");
        assert_eq!(fetched.deleted_count, 7);
        assert_eq!(fetched.failed_count, 1);
    }

    #[test]
    fn an_unknown_run_id_returns_no_record() {
        reset_activity_store_for_test();
        let fetched = get_autoscrub_activity("run-does-not-exist").expect("store is available");
        assert_eq!(fetched, None);
    }

    #[test]
    fn direct_invoke_unknown_command_is_refused() {
        let reply = run_autoscrub_activity_command("bogus", "{}");
        let value: serde_json::Value = serde_json::from_str(&reply).unwrap();
        assert_eq!(value["ok"], false);
        assert_eq!(value["errorCode"], "unknown_command");
    }

    #[test]
    fn direct_invoke_get_on_an_unknown_run_returns_not_found() {
        reset_activity_store_for_test();
        let reply = run_autoscrub_activity_command(
            AUTOSCRUB_ACTIVITY_GET_COMMAND,
            &serde_json::json!({ "runId": "run-does-not-exist" }).to_string(),
        );
        let value: serde_json::Value = serde_json::from_str(&reply).unwrap();
        assert_eq!(value["ok"], false);
        assert_eq!(value["errorCode"], "not_found");
    }
}
