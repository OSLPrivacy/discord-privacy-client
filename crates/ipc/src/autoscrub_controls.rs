//! Task 1473: AutoScrub controls built on the schedule store.
//!
//! A pause is deliberately metadata beside a saved schedule.  It never
//! recomputes, advances, or clears `next_run_unix_secs`, so resume continues
//! to show the exact time the owner saw before pressing Pause.  "Stop and turn
//! off" is different: when a run has crossed into a safe step, it records a
//! pending stop and removes the schedule *only after* that step returns.  This
//! keeps a stop request from bypassing work that has already begun while also
//! guaranteeing the schedule count reaches zero afterwards.

use std::collections::{BTreeMap, BTreeSet};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::schedule_storage::{ScheduleError, ScheduleKind, ScheduleRecord, ScheduleStore};

pub const AUTOSCRUB_RUN_NOW_COMMAND: &str = "autoscrub_run_now";
pub const AUTOSCRUB_PAUSE_COMMAND: &str = "autoscrub_pause";
pub const AUTOSCRUB_RESUME_COMMAND: &str = "autoscrub_resume";
pub const AUTOSCRUB_STOP_AND_TURN_OFF_COMMAND: &str = "autoscrub_stop_and_turn_off";
pub const AUTOSCRUB_VIEW_ACTIVITY_COMMAND: &str = "autoscrub_view_activity";
pub const UNKNOWN_COMMAND: &str = "unknown_command";
pub const SAFE_STEP_PENDING: &str = "safe_step_pending";

/// The labels are part of the backend-owned control surface.  Renderers can
/// display these without inventing a second set of names for destructive
/// actions.
pub const AUTOSCRUB_CONTROL_LABELS: [(&str, &str); 5] = [
    (AUTOSCRUB_RUN_NOW_COMMAND, "Run now"),
    (AUTOSCRUB_PAUSE_COMMAND, "Pause"),
    (AUTOSCRUB_RESUME_COMMAND, "Resume"),
    (AUTOSCRUB_STOP_AND_TURN_OFF_COMMAND, "Stop and turn off"),
    (AUTOSCRUB_VIEW_ACTIVITY_COMMAND, "View activity"),
];

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubControlStatus {
    pub account_id: String,
    pub paused: bool,
    pub next_run_unix_secs: Option<u64>,
    pub safe_step_pending: bool,
    pub turn_off_pending: bool,
    pub schedule_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubActivityEntry {
    pub account_id: String,
    pub action: String,
    pub detail: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubActivity {
    pub entries: Vec<AutoScrubActivityEntry>,
    pub entry_count: usize,
}

#[derive(Default)]
pub struct AutoScrubControlSurface {
    schedules: ScheduleStore,
    paused_accounts: BTreeSet<String>,
    /// An entry means a run has started and the next safe step has to execute
    /// before the account may be stopped or started again.
    safe_steps: BTreeSet<String>,
    turn_off_after_safe_step: BTreeSet<String>,
    activity: Vec<AutoScrubActivityEntry>,
}

impl AutoScrubControlSurface {
    pub fn new() -> Self {
        Self::default()
    }

    /// Save a schedule through the Task 1463 store.  This is intentionally a
    /// narrow setup seam; every control below uses the same stored record.
    pub fn save_schedule(
        &mut self,
        account_id: &str,
        kind: ScheduleKind,
        now: SystemTime,
    ) -> Result<ScheduleRecord, ScheduleError> {
        self.schedules.save_schedule(account_id, kind, now)
    }

    pub fn schedule_count(&self) -> usize {
        self.schedules.len()
    }

    pub fn schedule(&self, account_id: &str) -> Result<ScheduleRecord, ScheduleError> {
        self.schedules.get(account_id)
    }

    pub fn run_now(&mut self, account_id: &str) -> Result<AutoScrubControlStatus, String> {
        self.require_schedule(account_id)?;
        if self.safe_steps.contains(account_id) {
            return Err(SAFE_STEP_PENDING.to_owned());
        }
        self.safe_steps.insert(account_id.to_owned());
        self.record(account_id, "run_now", "Run now started; safe step pending.");
        self.status(account_id)
    }

    pub fn pause(&mut self, account_id: &str) -> Result<AutoScrubControlStatus, String> {
        // Read and retain the exact stored timestamp.  In particular, do not
        // save the schedule again with a new clock here.
        self.require_schedule(account_id)?;
        self.paused_accounts.insert(account_id.to_owned());
        self.record(account_id, "pause", "Schedule paused; next run unchanged.");
        self.status(account_id)
    }

    pub fn resume(&mut self, account_id: &str) -> Result<AutoScrubControlStatus, String> {
        self.require_schedule(account_id)?;
        self.paused_accounts.remove(account_id);
        self.record(
            account_id,
            "resume",
            "Schedule resumed; next run unchanged.",
        );
        self.status(account_id)
    }

    /// Request removal.  If a safe step is in progress, removal is deferred
    /// until [`Self::complete_safe_step`] has actually run it.  With no active
    /// safe step there is nothing to wait for, so the schedule is removed now.
    pub fn stop_and_turn_off(
        &mut self,
        account_id: &str,
    ) -> Result<AutoScrubControlStatus, String> {
        self.require_schedule(account_id)?;
        if self.safe_steps.contains(account_id) {
            self.turn_off_after_safe_step.insert(account_id.to_owned());
            self.record(
                account_id,
                "stop_and_turn_off",
                "Stop requested; schedule remains until the safe step completes.",
            );
            return self.status(account_id);
        }
        self.remove_schedule(account_id)?;
        self.record(
            account_id,
            "stop_and_turn_off",
            "Schedule stopped and turned off.",
        );
        self.status_after_removal(account_id)
    }

    /// Run the already-pending safe step exactly once.  State is not advanced
    /// before the callback succeeds, so an error leaves the safe step pending
    /// and, if requested, leaves the schedule in place for a later retry.
    pub fn complete_safe_step<F>(
        &mut self,
        account_id: &str,
        safe_step: F,
    ) -> Result<AutoScrubControlStatus, String>
    where
        F: FnOnce() -> Result<(), String>,
    {
        if !self.safe_steps.contains(account_id) {
            return Err(SAFE_STEP_PENDING.to_owned());
        }
        safe_step()?;
        self.safe_steps.remove(account_id);
        self.record(account_id, "safe_step", "Safe step completed.");
        if self.turn_off_after_safe_step.remove(account_id) {
            self.remove_schedule(account_id)?;
            self.record(
                account_id,
                "stop_and_turn_off",
                "Schedule stopped and turned off.",
            );
            return self.status_after_removal(account_id);
        }
        self.status(account_id)
    }

    pub fn view_activity(&self) -> AutoScrubActivity {
        AutoScrubActivity {
            entries: self.activity.clone(),
            entry_count: self.activity.len(),
        }
    }

    fn require_schedule(&self, account_id: &str) -> Result<ScheduleRecord, String> {
        self.schedules.get(account_id).map_err(schedule_error)
    }

    fn remove_schedule(&mut self, account_id: &str) -> Result<(), String> {
        self.schedules.remove(account_id).map_err(schedule_error)?;
        self.paused_accounts.remove(account_id);
        Ok(())
    }

    fn status(&self, account_id: &str) -> Result<AutoScrubControlStatus, String> {
        let schedule = self.require_schedule(account_id)?;
        Ok(AutoScrubControlStatus {
            account_id: account_id.to_owned(),
            paused: self.paused_accounts.contains(account_id),
            next_run_unix_secs: schedule.next_run_unix_secs,
            safe_step_pending: self.safe_steps.contains(account_id),
            turn_off_pending: self.turn_off_after_safe_step.contains(account_id),
            schedule_count: self.schedules.len(),
        })
    }

    fn status_after_removal(&self, account_id: &str) -> Result<AutoScrubControlStatus, String> {
        Ok(AutoScrubControlStatus {
            account_id: account_id.to_owned(),
            paused: false,
            next_run_unix_secs: None,
            safe_step_pending: self.safe_steps.contains(account_id),
            turn_off_pending: self.turn_off_after_safe_step.contains(account_id),
            schedule_count: self.schedules.len(),
        })
    }

    fn record(&mut self, account_id: &str, action: &str, detail: &str) {
        self.activity.push(AutoScrubActivityEntry {
            account_id: account_id.to_owned(),
            action: action.to_owned(),
            detail: detail.to_owned(),
        });
    }
}

fn schedule_error(error: ScheduleError) -> String {
    error.to_string()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountRequest {
    account_id: String,
}

fn ok_reply(command: &str, result: serde_json::Value) -> String {
    json!({ "ok": true, "command": command, "result": result }).to_string()
}

fn error_reply(command: &str, error_code: &str, error: String) -> String {
    json!({
        "ok": false,
        "command": command,
        "errorCode": error_code,
        "error": error,
    })
    .to_string()
}

/// Direct command surface for the five owner-facing controls.  Safe-step
/// execution belongs to the native runner, not a renderer command; callers
/// invoke [`AutoScrubControlSurface::complete_safe_step`] with the actual
/// checked operation when the in-flight run reaches that boundary.
pub fn run_autoscrub_control_command(
    surface: &mut AutoScrubControlSurface,
    command: &str,
    request_json: &str,
) -> String {
    match command {
        AUTOSCRUB_VIEW_ACTIVITY_COMMAND => ok_reply(command, json!(surface.view_activity())),
        AUTOSCRUB_RUN_NOW_COMMAND
        | AUTOSCRUB_PAUSE_COMMAND
        | AUTOSCRUB_RESUME_COMMAND
        | AUTOSCRUB_STOP_AND_TURN_OFF_COMMAND => {
            let request = match serde_json::from_str::<AccountRequest>(request_json) {
                Ok(request) => request,
                Err(error) => return error_reply(command, "bad_request", error.to_string()),
            };
            let result = match command {
                AUTOSCRUB_RUN_NOW_COMMAND => surface.run_now(&request.account_id),
                AUTOSCRUB_PAUSE_COMMAND => surface.pause(&request.account_id),
                AUTOSCRUB_RESUME_COMMAND => surface.resume(&request.account_id),
                AUTOSCRUB_STOP_AND_TURN_OFF_COMMAND => {
                    surface.stop_and_turn_off(&request.account_id)
                }
                _ => unreachable!("outer match limits commands"),
            };
            match result {
                Ok(status) => ok_reply(command, json!(status)),
                Err(error) => error_reply(command, &error, error.clone()),
            }
        }
        other => error_reply(
            other,
            UNKNOWN_COMMAND,
            format!("{other} is not an AutoScrub control command."),
        ),
    }
}

/// A snapshot by account used by native callers that need to render every
/// active control row without reconstructing control state themselves.
pub fn control_statuses(
    surface: &AutoScrubControlSurface,
) -> BTreeMap<String, AutoScrubControlStatus> {
    surface
        .schedules
        .list()
        .into_iter()
        .filter_map(|schedule| {
            surface
                .status(&schedule.account_id)
                .ok()
                .map(|status| (schedule.account_id, status))
        })
        .collect()
}
