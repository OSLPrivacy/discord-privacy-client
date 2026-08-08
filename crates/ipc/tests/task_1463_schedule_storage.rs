//! TASK 1463 - write schedule storage.
//!
//! Finish line: direct commands create one schedule of each type with the
//! expected next run.

use ipc::schedule_storage::{
    run_schedule_command, ScheduleStore, SCHEDULE_GET_COMMAND, SCHEDULE_LIST_COMMAND,
    SCHEDULE_SAVE_COMMAND,
};
use serde_json::json;

// 2026-08-07 12:00:00 UTC is a Friday.
const NOW: u64 = 1_786_104_000;

fn save(store: &mut ScheduleStore, body: serde_json::Value) -> serde_json::Value {
    let reply = run_schedule_command(store, SCHEDULE_SAVE_COMMAND, &body.to_string());
    serde_json::from_str(&reply).unwrap()
}

#[test]
fn direct_commands_create_one_schedule_of_each_type_with_expected_next_run() {
    let mut store = ScheduleStore::new();

    let daily = save(
        &mut store,
        json!({
            "accountId": "acct-daily",
            "kind": "daily",
            "hour": 18,
            "minute": 0,
            "nowUnixSecs": NOW,
        }),
    );
    assert_eq!(daily["ok"], true);
    // Friday 2026-08-07 12:00 UTC -> next 18:00 same day.
    assert_eq!(daily["schedule"]["nextRunUnixSecs"], 1_786_125_600u64);

    let weekly = save(
        &mut store,
        json!({
            "accountId": "acct-weekly",
            "kind": "weekly",
            "weekday": "mon",
            "hour": 9,
            "minute": 0,
            "nowUnixSecs": NOW,
        }),
    );
    assert_eq!(weekly["ok"], true);
    // Next Monday (2026-08-10) 09:00 UTC.
    assert_eq!(weekly["schedule"]["nextRunUnixSecs"], 1_786_352_400u64);

    let monthly = save(
        &mut store,
        json!({
            "accountId": "acct-monthly",
            "kind": "monthly",
            "day": 20,
            "hour": 0,
            "minute": 0,
            "nowUnixSecs": NOW,
        }),
    );
    assert_eq!(monthly["ok"], true);
    // 2026-08-20 00:00 UTC.
    assert_eq!(monthly["schedule"]["nextRunUnixSecs"], 1_787_184_000u64);

    let only_when_chosen = save(
        &mut store,
        json!({
            "accountId": "acct-manual",
            "kind": "only_when_chosen",
            "nowUnixSecs": NOW,
        }),
    );
    assert_eq!(only_when_chosen["ok"], true);
    assert_eq!(
        only_when_chosen["schedule"]["nextRunUnixSecs"],
        serde_json::Value::Null
    );

    // All four exist, independently retrievable, and the store holds exactly
    // one schedule per account (4 total, not fewer from overwrite, not more).
    let list_reply = run_schedule_command(&mut store, SCHEDULE_LIST_COMMAND, "{}");
    let list: serde_json::Value = serde_json::from_str(&list_reply).unwrap();
    assert_eq!(list["count"], 4);

    for account_id in ["acct-daily", "acct-weekly", "acct-monthly", "acct-manual"] {
        let get_reply = run_schedule_command(
            &mut store,
            SCHEDULE_GET_COMMAND,
            &json!({ "accountId": account_id }).to_string(),
        );
        let get: serde_json::Value = serde_json::from_str(&get_reply).unwrap();
        assert_eq!(get["ok"], true, "expected {account_id} to be saved");
        assert_eq!(get["schedule"]["accountId"], account_id);
    }
}

#[test]
fn resaving_an_account_replaces_rather_than_adds() {
    let mut store = ScheduleStore::new();
    save(
        &mut store,
        json!({
            "accountId": "acct-daily",
            "kind": "daily",
            "hour": 6,
            "minute": 0,
            "nowUnixSecs": NOW,
        }),
    );
    let second = save(
        &mut store,
        json!({
            "accountId": "acct-daily",
            "kind": "only_when_chosen",
            "nowUnixSecs": NOW,
        }),
    );
    assert_eq!(second["ok"], true);
    assert_eq!(store.len(), 1);
    assert_eq!(store.get("acct-daily").unwrap().next_run_unix_secs, None);
}

#[test]
fn invalid_time_is_refused_and_writes_nothing() {
    let mut store = ScheduleStore::new();
    let reply = save(
        &mut store,
        json!({
            "accountId": "acct-bad",
            "kind": "daily",
            "hour": 24,
            "minute": 0,
            "nowUnixSecs": NOW,
        }),
    );
    assert_eq!(reply["ok"], false);
    assert_eq!(reply["errorCode"], "invalid_schedule_time");
    assert!(store.is_empty());
}
