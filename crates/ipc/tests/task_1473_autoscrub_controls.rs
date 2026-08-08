//! TASK 1473 - write AutoScrub controls.
//!
//! The stop check deliberately observes the schedule before and after the
//! native safe-step callback.  A controller that deletes the schedule at the
//! click, or that removes it without calling the safe step, fails this test.

use std::cell::Cell;
use std::time::{Duration, UNIX_EPOCH};

use ipc::autoscrub_controls::{
    run_autoscrub_control_command, AutoScrubControlSurface, AUTOSCRUB_PAUSE_COMMAND,
    AUTOSCRUB_RESUME_COMMAND, AUTOSCRUB_RUN_NOW_COMMAND, AUTOSCRUB_STOP_AND_TURN_OFF_COMMAND,
    AUTOSCRUB_VIEW_ACTIVITY_COMMAND,
};
use ipc::schedule_storage::ScheduleKind;
use serde_json::json;

const NOW: u64 = 1_786_104_000; // 2026-08-07 12:00:00 UTC
const ACCOUNT: &str = "discord-maple";

fn request() -> String {
    json!({ "accountId": ACCOUNT }).to_string()
}

fn reply(value: String) -> serde_json::Value {
    serde_json::from_str(&value).expect("control reply is JSON")
}

#[test]
fn controls_preserve_next_run_finish_safe_step_and_turn_off_every_schedule() {
    let mut surface = AutoScrubControlSurface::new();
    let saved = surface
        .save_schedule(
            ACCOUNT,
            ScheduleKind::Daily {
                hour: 18,
                minute: 0,
            },
            UNIX_EPOCH + Duration::from_secs(NOW),
        )
        .expect("schedule exists before controls operate");
    let next_run = saved
        .next_run_unix_secs
        .expect("automatic schedule has next run");

    let run_now = reply(run_autoscrub_control_command(
        &mut surface,
        AUTOSCRUB_RUN_NOW_COMMAND,
        &request(),
    ));
    assert_eq!(run_now["ok"], true);
    assert_eq!(run_now["result"]["safeStepPending"], true);

    let paused = reply(run_autoscrub_control_command(
        &mut surface,
        AUTOSCRUB_PAUSE_COMMAND,
        &request(),
    ));
    assert_eq!(paused["ok"], true);
    assert_eq!(paused["result"]["nextRunUnixSecs"], next_run);
    let resumed = reply(run_autoscrub_control_command(
        &mut surface,
        AUTOSCRUB_RESUME_COMMAND,
        &request(),
    ));
    assert_eq!(resumed["ok"], true);
    assert_eq!(resumed["result"]["nextRunUnixSecs"], next_run);

    let stopping = reply(run_autoscrub_control_command(
        &mut surface,
        AUTOSCRUB_STOP_AND_TURN_OFF_COMMAND,
        &request(),
    ));
    assert_eq!(stopping["ok"], true);
    assert_eq!(stopping["result"]["turnOffPending"], true);
    assert_eq!(stopping["result"]["scheduleCount"], 1);

    let safe_steps = Cell::new(0usize);
    let stopped = surface
        .complete_safe_step(ACCOUNT, || {
            safe_steps.set(safe_steps.get() + 1);
            Ok(())
        })
        .expect("stop waits for and then finishes its safe step");
    assert_eq!(safe_steps.get(), 1, "safe step is never skipped");
    assert_eq!(stopped.schedule_count, 0);
    assert_eq!(surface.schedule_count(), 0);

    let activity = reply(run_autoscrub_control_command(
        &mut surface,
        AUTOSCRUB_VIEW_ACTIVITY_COMMAND,
        "{}",
    ));
    assert_eq!(activity["ok"], true);
    assert_eq!(activity["result"]["entryCount"], 6);
    assert!(activity["result"]["entries"]
        .as_array()
        .expect("activity entries")
        .iter()
        .any(|entry| entry["action"] == "safe_step"));

    println!(
        "TASK1473 next_run_before_pause={} next_run_after_pause={} next_run_after_resume={} schedules_after_safe_step={} safe_steps={} activity_entries={}",
        next_run,
        paused["result"]["nextRunUnixSecs"],
        resumed["result"]["nextRunUnixSecs"],
        surface.schedule_count(),
        safe_steps.get(),
        activity["result"]["entryCount"],
    );
}

#[test]
fn a_failed_safe_step_cannot_turn_off_the_schedule_early() {
    let mut surface = AutoScrubControlSurface::new();
    surface
        .save_schedule(
            ACCOUNT,
            ScheduleKind::OnlyWhenChosen,
            UNIX_EPOCH + Duration::from_secs(NOW),
        )
        .unwrap();
    surface.run_now(ACCOUNT).unwrap();
    surface.stop_and_turn_off(ACCOUNT).unwrap();

    assert_eq!(
        surface.complete_safe_step(ACCOUNT, || Err("safe step failed".to_owned())),
        Err("safe step failed".to_owned())
    );
    assert_eq!(surface.schedule_count(), 1);
    let after = surface.complete_safe_step(ACCOUNT, || Ok(())).unwrap();
    assert_eq!(after.schedule_count, 0);
}
