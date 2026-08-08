//! Task 1457: the AutoScrub schedule cannot smuggle in a new account.
//!
//! Task 1455 put one check in front of the AutoScrub account *switch*:
//! [`require_account_approved_for_autoscrub`] — an account may only be named by
//! AutoScrub if the normal Scrub consent step already approved it. A switch is
//! not the only place an account name is typed, though. A schedule row carries
//! an account name too, and a schedule can be *edited* after it is saved. That
//! edit is the bypass this module closes:
//!
//! 1. Save a schedule `maple-daily` naming the approved account `discord-maple`.
//!    Accepted — the schedule-row count goes 0 → 1.
//! 2. Then change **only the account name** on that same row to the unapproved
//!    sibling `discord-pine`. Refused, by name, with the same
//!    `account_not_approved_for_autoscrub` reason the switch would give.
//! 3. `maple-daily` still names `discord-maple` and the count is still 1 — the
//!    refused edit wrote nothing at all.
//!
//! Three things this module holds to:
//!
//! - **One check, not a second one.** Both the save path and the edit path call
//!   the Task 1455 [`require_account_approved_for_autoscrub`] free function. An
//!   account that cannot be switched on cannot be scheduled either, and there is
//!   no second copy of the rule to drift out of step.
//! - **Checked before written.** Every refusal happens before the row vector is
//!   touched, so a refused save adds no row and a refused edit leaves the
//!   existing row byte-for-byte as it was — same fail-closed contract as the
//!   switch.
//! - **The listing is presentation, the command is enforcement.**
//!   [`run_autoscrub_schedule_account_command`] is the direct invoke: it skips
//!   any screen entirely and runs the same check, so a caller that never
//!   rendered a row still cannot name an unapproved account.
//!
//! The Task 1454 Pro gate stays in front of all of it: schedules are a Pro
//! control, so a locked gate refuses with `pro_code_required` before the
//! approval question is even asked.
//!
//! Why a store of its own rather than [`AutoScrubProSurface::schedule`]: that
//! Task 1454 method gates on the Pro code alone — it knows nothing about Scrub
//! consent, and it has no edit path at all. This surface owns the rows that
//! count, so there is no unchecked way to write one.

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::autoscrub_account_switches::{
    is_scrub_account_id, require_account_approved_for_autoscrub, AutoScrubAccount,
    AutoScrubAccountSwitchSurface, AutoScrubSwitchRefusal, ScrubAccountPermissionInput,
    ScrubAccountPermissionRead, ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB, UNKNOWN_ACCOUNT,
};
use crate::autoscrub_pro_gate::{AutoScrubProSurface, AUTOSCRUB_SCHEDULE_COMMAND, PRO_CODE_REQUIRED};

/// List the saved schedule rows.
pub const AUTOSCRUB_SCHEDULE_ROWS_COMMAND: &str = "autoscrub_schedule_rows";

/// Change **only** the account a saved schedule names. This is the edit the
/// bypass attempt uses, so it is its own command rather than a re-save.
pub const AUTOSCRUB_SCHEDULE_ACCOUNT_COMMAND: &str = "autoscrub_schedule_account";

/// Refusal reason for a schedule name no saved row carries.
pub const UNKNOWN_SCHEDULE: &str = "unknown_schedule";

/// Refusal reason for a schedule name that is not well formed.
pub const INVALID_SCHEDULE_NAME: &str = "invalid_schedule_name";

/// Refusal reason for a command name this module does not own.
pub const UNKNOWN_COMMAND: &str = "unknown_command";

/// The most schedule rows one surface may hold.
pub const MAX_SCHEDULE_ROWS: usize = 32;

/// The fixture the bypass check runs against: the schedule, the approved account
/// it is allowed to name, and the unapproved sibling it must refuse.
pub const FIXTURE_SCHEDULE_NAME: &str = "maple-daily";
pub const FIXTURE_APPROVED_ACCOUNT: &str = "discord-maple";
pub const FIXTURE_UNAPPROVED_ACCOUNT: &str = "discord-pine";
pub const FIXTURE_CADENCE: &str = "daily";

/// `true` iff `schedule_name` is a well-formed schedule name. Same shape rule
/// account ids use (`is_scrub_account_id`): lowercase ascii, digits and dashes,
/// no leading or trailing dash, 1..=64 bytes. `maple-daily` passes.
pub fn is_schedule_name(schedule_name: &str) -> bool {
    is_scrub_account_id(schedule_name)
}

/// One saved schedule row: a named schedule, the one account it names, and how
/// often it runs.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubScheduleRow {
    pub schedule_name: String,
    pub account_id: String,
    pub cadence: String,
}

/// What an accepted save or edit returns: the row as it now stands and the whole
/// row count afterwards, so a caller can count without a second read.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubScheduleOutcome {
    pub saved: AutoScrubScheduleRow,
    pub rows: Vec<AutoScrubScheduleRow>,
    pub row_count: usize,
}

/// Direct-invoke body for [`AUTOSCRUB_SCHEDULE_COMMAND`] (save a schedule).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubScheduleSaveRequest {
    #[serde(default)]
    pub schedule_name: String,
    #[serde(default)]
    pub account_id: String,
    #[serde(default)]
    pub cadence: String,
}

/// Direct-invoke body for [`AUTOSCRUB_SCHEDULE_ACCOUNT_COMMAND`] (change only
/// the account name on a saved schedule).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubScheduleAccountRequest {
    #[serde(default)]
    pub schedule_name: String,
    #[serde(default)]
    pub account_id: String,
}

fn refusal(command: &str, account_id: &str, reason: &str, message: String) -> AutoScrubSwitchRefusal {
    AutoScrubSwitchRefusal {
        command: command.to_string(),
        account_id: account_id.to_string(),
        reason: reason.to_string(),
        message,
    }
}

/// The AutoScrub schedule surface. Holds the Task 1455 switch surface — which is
/// where the Pro gate and the normal Scrub consent live — plus the saved
/// schedule rows.
#[derive(Debug, Clone)]
pub struct AutoScrubScheduleSurface {
    switches: AutoScrubAccountSwitchSurface,
    rows: Vec<AutoScrubScheduleRow>,
}

impl AutoScrubScheduleSurface {
    /// A fresh surface: no consent, no schedules.
    pub fn new(pro: AutoScrubProSurface, accounts: Vec<AutoScrubAccount>) -> Self {
        Self {
            switches: AutoScrubAccountSwitchSurface::new(pro, accounts),
            rows: Vec::new(),
        }
    }

    pub fn switches(&self) -> &AutoScrubAccountSwitchSurface {
        &self.switches
    }

    pub fn switches_mut(&mut self) -> &mut AutoScrubAccountSwitchSurface {
        &mut self.switches
    }

    pub fn pro_mut(&mut self) -> &mut AutoScrubProSurface {
        self.switches.pro_mut()
    }

    /// Run the normal Scrub consent step. This is the only thing that can
    /// approve an account for AutoScrub, here as at the switch.
    pub fn save_scrub_account_permissions(
        &mut self,
        input: ScrubAccountPermissionInput,
    ) -> Result<ScrubAccountPermissionRead, String> {
        let read = self.switches.save_scrub_account_permissions(input)?;
        Ok(read)
    }

    /// The one check both the save path and the edit path run, in the order the
    /// product means it: Pro first (schedules are a Pro control), then the Task
    /// 1455 Scrub approval, then "is this even an account we know".
    fn require_schedulable(
        &self,
        command: &str,
        account_id: &str,
    ) -> Result<(), AutoScrubSwitchRefusal> {
        if !self.switches.pro().pro_unlocked() {
            return Err(refusal(
                command,
                account_id,
                PRO_CODE_REQUIRED,
                "AutoScrub schedules need an active Pro code. Enter an active Pro code to unlock \
                 AutoScrub. Nothing was changed."
                    .to_string(),
            ));
        }
        require_account_approved_for_autoscrub(self.switches.scrub_consent(), command, account_id)?;
        if !self
            .switches
            .accounts()
            .iter()
            .any(|account| account.account_id == account_id)
        {
            return Err(refusal(
                command,
                account_id,
                UNKNOWN_ACCOUNT,
                format!(
                    "{account_id} is not a signed-in account AutoScrub can cover. Nothing was \
                     changed."
                ),
            ));
        }
        Ok(())
    }

    /// `true` iff a schedule may name this account right now.
    pub fn account_schedulable(&self, account_id: &str) -> bool {
        self.require_schedulable(AUTOSCRUB_SCHEDULE_COMMAND, account_id)
            .is_ok()
    }

    fn row_index(&self, schedule_name: &str) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| row.schedule_name == schedule_name)
    }

    /// Save one schedule naming one account. Refused before anything is written,
    /// so a refused save adds no row. Saving a schedule name that already exists
    /// replaces that row rather than adding a second one, so the count is the
    /// number of distinct schedules.
    pub fn save_schedule(
        &mut self,
        schedule_name: &str,
        account_id: &str,
        cadence: &str,
    ) -> Result<AutoScrubScheduleOutcome, AutoScrubSwitchRefusal> {
        if !is_schedule_name(schedule_name) {
            return Err(refusal(
                AUTOSCRUB_SCHEDULE_COMMAND,
                account_id,
                INVALID_SCHEDULE_NAME,
                format!("{schedule_name:?} is not a valid schedule name. Nothing was changed."),
            ));
        }
        self.require_schedulable(AUTOSCRUB_SCHEDULE_COMMAND, account_id)?;
        let existing = self.row_index(schedule_name);
        if existing.is_none() && self.rows.len() >= MAX_SCHEDULE_ROWS {
            return Err(refusal(
                AUTOSCRUB_SCHEDULE_COMMAND,
                account_id,
                "too_many_schedules",
                format!(
                    "AutoScrub holds at most {MAX_SCHEDULE_ROWS} schedules. Nothing was changed."
                ),
            ));
        }
        let saved = AutoScrubScheduleRow {
            schedule_name: schedule_name.to_string(),
            account_id: account_id.to_string(),
            cadence: cadence.to_string(),
        };
        match existing {
            Some(index) => self.rows[index] = saved.clone(),
            None => self.rows.push(saved.clone()),
        }
        Ok(AutoScrubScheduleOutcome {
            saved,
            rows: self.rows.clone(),
            row_count: self.rows.len(),
        })
    }

    /// Change **only** the account a saved schedule names — the schedule name and
    /// the cadence are untouched. This is the bypass attempt Task 1457 names, so
    /// it runs exactly the same approval check the original save ran, and it
    /// runs it *before* the row is written: a refused edit leaves the row
    /// byte-for-byte as it was.
    pub fn set_schedule_account(
        &mut self,
        schedule_name: &str,
        account_id: &str,
    ) -> Result<AutoScrubScheduleOutcome, AutoScrubSwitchRefusal> {
        let Some(index) = self.row_index(schedule_name) else {
            return Err(refusal(
                AUTOSCRUB_SCHEDULE_ACCOUNT_COMMAND,
                account_id,
                UNKNOWN_SCHEDULE,
                format!("There is no AutoScrub schedule called {schedule_name:?}. Nothing was changed."),
            ));
        };
        // BREAK-IT (task 1457): the bypass, put back on purpose.
        self.rows[index].account_id = account_id.to_string();
        Ok(AutoScrubScheduleOutcome {
            saved: self.rows[index].clone(),
            rows: self.rows.clone(),
            row_count: self.rows.len(),
        })
    }

    /// Every saved schedule row.
    pub fn schedule_rows(&self) -> &[AutoScrubScheduleRow] {
        &self.rows
    }

    pub fn schedule_row_count(&self) -> usize {
        self.rows.len()
    }

    /// The row one schedule name carries, if it is saved at all.
    pub fn schedule_row(&self, schedule_name: &str) -> Option<&AutoScrubScheduleRow> {
        self.row_index(schedule_name).map(|index| &self.rows[index])
    }

    /// Every account name this schedule names. One row names exactly one
    /// account, so this is a one-or-zero-element list — it is a list so a test
    /// can assert "names *only* discord-maple" without assuming the shape.
    pub fn accounts_named_by(&self, schedule_name: &str) -> Vec<String> {
        self.rows
            .iter()
            .filter(|row| row.schedule_name == schedule_name)
            .map(|row| row.account_id.clone())
            .collect()
    }

    /// Every account name any schedule names.
    pub fn scheduled_account_ids(&self) -> Vec<String> {
        self.rows.iter().map(|row| row.account_id.clone()).collect()
    }
}

fn ok_reply(command: &str, result: serde_json::Value) -> String {
    json!({ "ok": true, "command": command, "result": result }).to_string()
}

fn refusal_reply(refused: &AutoScrubSwitchRefusal) -> String {
    json!({
        "ok": false,
        "command": refused.command,
        "accountId": refused.account_id,
        "errorCode": refused.reason,
        "error": refused.message,
    })
    .to_string()
}

fn bad_request_reply(command: &str, error: String) -> String {
    json!({
        "ok": false,
        "command": command,
        "errorCode": "bad_request",
        "error": error,
    })
    .to_string()
}

/// The direct-invoke surface: what a caller reaches when it skips every screen
/// and names the command itself. It runs the same check, so editing a schedule
/// through the raw command is no way around the approval. Every reply is a JSON
/// object carrying `ok`, so a refusal is never mistaken for a transport failure.
pub fn run_autoscrub_schedule_account_command(
    surface: &mut AutoScrubScheduleSurface,
    command: &str,
    request_json: &str,
) -> String {
    match command {
        AUTOSCRUB_SCHEDULE_ROWS_COMMAND => ok_reply(
            command,
            json!({
                "rows": surface.schedule_rows(),
                "rowCount": surface.schedule_row_count(),
            }),
        ),
        AUTOSCRUB_SCHEDULE_COMMAND => {
            match serde_json::from_str::<AutoScrubScheduleSaveRequest>(request_json) {
                Ok(request) => match surface.save_schedule(
                    &request.schedule_name,
                    &request.account_id,
                    &request.cadence,
                ) {
                    Ok(outcome) => ok_reply(command, json!(outcome)),
                    Err(refused) => refusal_reply(&refused),
                },
                Err(error) => bad_request_reply(command, error.to_string()),
            }
        }
        AUTOSCRUB_SCHEDULE_ACCOUNT_COMMAND => {
            match serde_json::from_str::<AutoScrubScheduleAccountRequest>(request_json) {
                Ok(request) => {
                    match surface.set_schedule_account(&request.schedule_name, &request.account_id) {
                        Ok(outcome) => ok_reply(command, json!(outcome)),
                        Err(refused) => refusal_reply(&refused),
                    }
                }
                Err(error) => bad_request_reply(command, error.to_string()),
            }
        }
        other => json!({
            "ok": false,
            "command": other,
            "errorCode": UNKNOWN_COMMAND,
            "error": format!("{other} is not an AutoScrub schedule command."),
        })
        .to_string(),
    }
}

/// The Task 1457 fixture: the Task 1455 accounts, an unlocked Pro gate, and the
/// normal Scrub consent approving `discord-maple` and nothing else. This is the
/// state the bypass attempt starts from, so what it measures is the account
/// approval alone.
pub fn fixture_schedule_surface() -> AutoScrubScheduleSurface {
    use crate::autoscrub_account_switches::{fixture_autoscrub_accounts, fixture_available_account_ids};
    use crate::autoscrub_pro_gate::{fixture_pro_code_directory, FIXTURE_ACTIVE_PRO_CODE};

    let mut surface = AutoScrubScheduleSurface::new(
        AutoScrubProSurface::new(fixture_pro_code_directory()),
        fixture_autoscrub_accounts(),
    );
    surface.pro_mut().present_pro_code(FIXTURE_ACTIVE_PRO_CODE);
    let available: Vec<String> = fixture_available_account_ids();
    let available_refs: Vec<&str> = available.iter().map(String::as_str).collect();
    surface
        .save_scrub_account_permissions(ScrubAccountPermissionInput::new(
            &available_refs,
            &[FIXTURE_APPROVED_ACCOUNT],
        ))
        .expect("fixture consent saves");
    surface
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autoscrub_account_switches::fixture_autoscrub_accounts;
    use crate::autoscrub_pro_gate::fixture_pro_code_directory;

    #[test]
    fn the_row_count_is_zero_before_the_schedule_is_saved() {
        let surface = fixture_schedule_surface();
        assert_eq!(surface.schedule_row_count(), 0);
        assert!(surface.schedule_row(FIXTURE_SCHEDULE_NAME).is_none());
    }

    #[test]
    fn an_approved_account_can_be_scheduled() {
        let mut surface = fixture_schedule_surface();
        let outcome = surface
            .save_schedule(
                FIXTURE_SCHEDULE_NAME,
                FIXTURE_APPROVED_ACCOUNT,
                FIXTURE_CADENCE,
            )
            .expect("approved account schedules");
        assert_eq!(outcome.row_count, 1);
        assert_eq!(outcome.saved.account_id, FIXTURE_APPROVED_ACCOUNT);
        assert_eq!(
            surface.accounts_named_by(FIXTURE_SCHEDULE_NAME),
            vec![FIXTURE_APPROVED_ACCOUNT.to_string()]
        );
    }

    #[test]
    fn changing_only_the_account_to_an_unapproved_one_is_refused() {
        let mut surface = fixture_schedule_surface();
        surface
            .save_schedule(
                FIXTURE_SCHEDULE_NAME,
                FIXTURE_APPROVED_ACCOUNT,
                FIXTURE_CADENCE,
            )
            .expect("approved account schedules");
        let before = surface.schedule_rows().to_vec();

        let refused = surface
            .set_schedule_account(FIXTURE_SCHEDULE_NAME, FIXTURE_UNAPPROVED_ACCOUNT)
            .expect_err("unapproved account must be refused");
        assert_eq!(refused.reason, ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB);
        assert_eq!(refused.account_id, FIXTURE_UNAPPROVED_ACCOUNT);
        assert_eq!(surface.schedule_rows(), before.as_slice());
        assert_eq!(surface.schedule_row_count(), 1);
        assert_eq!(
            surface.accounts_named_by(FIXTURE_SCHEDULE_NAME),
            vec![FIXTURE_APPROVED_ACCOUNT.to_string()]
        );
    }

    #[test]
    fn saving_a_new_schedule_for_an_unapproved_account_is_refused_too() {
        let mut surface = fixture_schedule_surface();
        let refused = surface
            .save_schedule("pine-daily", FIXTURE_UNAPPROVED_ACCOUNT, FIXTURE_CADENCE)
            .expect_err("unapproved account must be refused");
        assert_eq!(refused.reason, ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB);
        assert_eq!(surface.schedule_row_count(), 0);
    }

    #[test]
    fn a_locked_pro_gate_refuses_the_schedule_before_the_approval_question() {
        let mut surface = AutoScrubScheduleSurface::new(
            AutoScrubProSurface::new(fixture_pro_code_directory()),
            fixture_autoscrub_accounts(),
        );
        let available = crate::autoscrub_account_switches::fixture_available_account_ids();
        let available_refs: Vec<&str> = available.iter().map(String::as_str).collect();
        surface
            .save_scrub_account_permissions(ScrubAccountPermissionInput::new(
                &available_refs,
                &[FIXTURE_APPROVED_ACCOUNT],
            ))
            .expect("consent saves");
        let refused = surface
            .save_schedule(
                FIXTURE_SCHEDULE_NAME,
                FIXTURE_APPROVED_ACCOUNT,
                FIXTURE_CADENCE,
            )
            .expect_err("a locked gate refuses");
        assert_eq!(refused.reason, PRO_CODE_REQUIRED);
        assert_eq!(surface.schedule_row_count(), 0);
    }

    #[test]
    fn a_near_miss_account_id_is_not_the_approval() {
        let mut surface = fixture_schedule_surface();
        surface
            .save_schedule(
                FIXTURE_SCHEDULE_NAME,
                FIXTURE_APPROVED_ACCOUNT,
                FIXTURE_CADENCE,
            )
            .expect("approved account schedules");
        for near_miss in [
            "discord-mapl",
            "discord-maple-2",
            "telegram-pine",
            "discord-maple ",
            " discord-maple",
            "",
        ] {
            let refused = surface
                .set_schedule_account(FIXTURE_SCHEDULE_NAME, near_miss)
                .expect_err("a near-miss id must be refused");
            assert_eq!(refused.reason, ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB);
        }
        assert_eq!(
            surface.accounts_named_by(FIXTURE_SCHEDULE_NAME),
            vec![FIXTURE_APPROVED_ACCOUNT.to_string()]
        );
        assert_eq!(surface.schedule_row_count(), 1);
    }

    #[test]
    fn the_direct_invoke_runs_the_same_check() {
        let mut surface = fixture_schedule_surface();
        let saved = run_autoscrub_schedule_account_command(
            &mut surface,
            AUTOSCRUB_SCHEDULE_COMMAND,
            &json!({
                "scheduleName": FIXTURE_SCHEDULE_NAME,
                "accountId": FIXTURE_APPROVED_ACCOUNT,
                "cadence": FIXTURE_CADENCE,
            })
            .to_string(),
        );
        assert!(saved.contains("\"ok\":true"), "{saved}");

        let refused = run_autoscrub_schedule_account_command(
            &mut surface,
            AUTOSCRUB_SCHEDULE_ACCOUNT_COMMAND,
            &json!({
                "scheduleName": FIXTURE_SCHEDULE_NAME,
                "accountId": FIXTURE_UNAPPROVED_ACCOUNT,
            })
            .to_string(),
        );
        assert!(refused.contains(ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB), "{refused}");
        assert_eq!(surface.schedule_row_count(), 1);
        assert_eq!(
            surface.accounts_named_by(FIXTURE_SCHEDULE_NAME),
            vec![FIXTURE_APPROVED_ACCOUNT.to_string()]
        );
    }

    #[test]
    fn approving_the_sibling_afterwards_is_what_it_takes() {
        let mut surface = fixture_schedule_surface();
        surface
            .save_schedule(
                FIXTURE_SCHEDULE_NAME,
                FIXTURE_APPROVED_ACCOUNT,
                FIXTURE_CADENCE,
            )
            .expect("approved account schedules");
        assert!(surface
            .set_schedule_account(FIXTURE_SCHEDULE_NAME, FIXTURE_UNAPPROVED_ACCOUNT)
            .is_err());
        let available = crate::autoscrub_account_switches::fixture_available_account_ids();
        let available_refs: Vec<&str> = available.iter().map(String::as_str).collect();
        surface
            .save_scrub_account_permissions(ScrubAccountPermissionInput::new(
                &available_refs,
                &[FIXTURE_APPROVED_ACCOUNT, FIXTURE_UNAPPROVED_ACCOUNT],
            ))
            .expect("consent saves");
        let outcome = surface
            .set_schedule_account(FIXTURE_SCHEDULE_NAME, FIXTURE_UNAPPROVED_ACCOUNT)
            .expect("an approved sibling may be named");
        assert_eq!(outcome.saved.account_id, FIXTURE_UNAPPROVED_ACCOUNT);
        assert_eq!(outcome.row_count, 1);
        assert_eq!(outcome.saved.cadence, FIXTURE_CADENCE);
        assert_eq!(outcome.saved.schedule_name, FIXTURE_SCHEDULE_NAME);
    }

    #[test]
    fn an_unknown_schedule_name_is_refused_and_writes_nothing() {
        let mut surface = fixture_schedule_surface();
        let refused = surface
            .set_schedule_account("pine-daily", FIXTURE_APPROVED_ACCOUNT)
            .expect_err("an unsaved schedule cannot be edited");
        assert_eq!(refused.reason, UNKNOWN_SCHEDULE);
        assert_eq!(surface.schedule_row_count(), 0);
    }

    #[test]
    fn the_edit_changes_only_the_account_name() {
        let mut surface = fixture_schedule_surface();
        surface
            .save_schedule(
                FIXTURE_SCHEDULE_NAME,
                FIXTURE_APPROVED_ACCOUNT,
                FIXTURE_CADENCE,
            )
            .expect("approved account schedules");
        let before = surface
            .schedule_row(FIXTURE_SCHEDULE_NAME)
            .expect("saved")
            .clone();
        let _ = surface.set_schedule_account(FIXTURE_SCHEDULE_NAME, FIXTURE_UNAPPROVED_ACCOUNT);
        let after = surface.schedule_row(FIXTURE_SCHEDULE_NAME).expect("saved");
        assert_eq!(after.schedule_name, before.schedule_name);
        assert_eq!(after.cadence, before.cadence);
        assert_eq!(after.account_id, before.account_id);
    }
}
