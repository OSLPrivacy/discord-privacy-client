//! TASK 1469 finish-line proof: a due Find-only run records real matches and
//! makes zero calls through the same delete port exercised by Find-and-delete.

use ipc::autoscrub_schedule_mode::{
    AutoScrubDueRunRecord, AutoScrubMatch, AutoScrubRunPort, AutoScrubScheduleMode,
    AutoScrubScheduleModeError, AutoScrubScheduleModeStore, DueAutoScrubRun, FIND_AND_DELETE_LABEL,
    FIND_ONLY_LABEL,
};

const DUE_AT: i64 = 1_786_104_000;

struct FixturePort {
    matches: Vec<AutoScrubMatch>,
    delete_calls: usize,
}

impl FixturePort {
    fn with_matches(ids: &[&str]) -> Self {
        Self {
            matches: ids.iter().map(|id| AutoScrubMatch::new(*id)).collect(),
            delete_calls: 0,
        }
    }
}

impl AutoScrubRunPort for FixturePort {
    fn find_matches(
        &mut self,
        _account_id: &str,
    ) -> Result<Vec<AutoScrubMatch>, AutoScrubScheduleModeError> {
        Ok(self.matches.clone())
    }

    fn delete(
        &mut self,
        _account_id: &str,
        _matched: &AutoScrubMatch,
    ) -> Result<(), AutoScrubScheduleModeError> {
        self.delete_calls += 1;
        Ok(())
    }
}

fn due(run_id: &str, account_id: &str) -> DueAutoScrubRun {
    DueAutoScrubRun {
        run_id: run_id.to_owned(),
        account_id: account_id.to_owned(),
        due_at_unix_secs: DUE_AT,
        now_unix_secs: DUE_AT,
    }
}

fn assert_recorded(record: &AutoScrubDueRunRecord, expected_ids: &[&str]) {
    assert_eq!(record.matched_count, expected_ids.len());
    assert_eq!(
        record
            .matches
            .iter()
            .map(|matched| matched.item_id.as_str())
            .collect::<Vec<_>>(),
        expected_ids
    );
}

#[test]
fn task_1469_find_only_due_run_records_matches_and_makes_zero_delete_calls() {
    let mut store = AutoScrubScheduleModeStore::new();
    let find_only = store
        .save_mode("discord-maple-find", AutoScrubScheduleMode::FindOnly)
        .expect("Find only saves");
    let find_and_delete = store
        .save_mode("discord-maple-delete", AutoScrubScheduleMode::FindAndDelete)
        .expect("Find and delete saves separately");
    assert_eq!(find_only.label, FIND_ONLY_LABEL);
    assert_eq!(find_and_delete.label, FIND_AND_DELETE_LABEL);
    assert_eq!(
        store.mode_for("discord-maple-find").unwrap().mode,
        AutoScrubScheduleMode::FindOnly
    );
    assert_eq!(
        store.mode_for("discord-maple-delete").unwrap().mode,
        AutoScrubScheduleMode::FindAndDelete
    );

    // The find-only fixture is deliberately nonempty: zero delete calls cannot
    // be explained by there being nothing to delete.
    let mut find_port =
        FixturePort::with_matches(&["message-1469-a", "message-1469-b", "message-1469-c"]);
    let find_record = store
        .run_due(due("run-1469-find", "discord-maple-find"), &mut find_port)
        .expect("due Find-only run succeeds");
    assert_recorded(
        &find_record,
        &["message-1469-a", "message-1469-b", "message-1469-c"],
    );
    assert_eq!(find_record.mode, AutoScrubScheduleMode::FindOnly);
    assert_eq!(find_record.delete_call_count, 0);
    assert_eq!(find_record.deleted_count, 0);
    assert_eq!(find_port.delete_calls, 0);
    assert_eq!(store.activity()[0], find_record);

    // Negative control: the separately saved destructive mode sends the same
    // live callback two matches, proving the counter is capable of going red.
    let mut delete_port = FixturePort::with_matches(&["message-delete-a", "message-delete-b"]);
    let delete_record = store
        .run_due(
            due("run-1469-delete-control", "discord-maple-delete"),
            &mut delete_port,
        )
        .expect("due Find-and-delete control succeeds");
    assert_recorded(&delete_record, &["message-delete-a", "message-delete-b"]);
    assert_eq!(delete_record.delete_call_count, 2);
    assert_eq!(delete_port.delete_calls, 2);

    println!(
        "TASK1469 find_only_saved={} find_and_delete_saved={} due={} matches_recorded={} delete_calls={} negative_control_delete_calls={} activity_records={}",
        find_only.label,
        find_and_delete.label,
        find_record.ran_at_unix_secs >= find_record.due_at_unix_secs,
        find_record.matched_count,
        find_port.delete_calls,
        delete_port.delete_calls,
        store.activity().len(),
    );
}
