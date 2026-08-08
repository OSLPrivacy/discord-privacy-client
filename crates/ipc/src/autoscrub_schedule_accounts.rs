//! Task 1457: an AutoScrub schedule may only name an approved account.
//!
//! The switch (Task 1455) is not the only place an account id can be typed. A
//! schedule names an account too, and a schedule row is the thing that makes
//! AutoScrub act on that account unattended. If the schedule path did its own
//! (or no) account check, a brand-new account could be slipped into AutoScrub
//! by naming it in a schedule rather than by moving its switch — the normal
//! Scrub consent step would never have been walked for it. That is the bypass
//! this module closes.
//!
//! It closes it by *reusing* the one check, not by writing a second one:
//! [`require_account_approved_for_autoscrub`] is the same function the switch
//! calls, so an account that cannot be switched on cannot be scheduled either,
//! for the same reason and with the same words.
//!
//! The order of the checks matches the switch surface exactly:
//!
//! 1. **Pro** — AutoScrub is the paid half of Scrub (Task 1454).
//! 2. **The normal Scrub approval** — byte-exact against the saved consent.
//! 3. **Is this an account we know at all.**
//! 4. Only then the schedule's own shape: its name and its cadence.
//!
//! Every refusal happens *before* anything is written, so a refused save leaves
//! the schedule rows byte-for-byte as they were. That is what makes the Task
//! 1457 differential meaningful: two saves that differ in the account name
//! alone, where the second one must not be able to touch what the first one
//! wrote.
//!
//! Saving under a name that already exists **replaces** that row rather than
//! adding a second one — editing a schedule is a thing people do, and it also
//! means an unapproved account that got through would *rewrite* an existing
//! schedule's account rather than merely add a row. The Task 1457 check reads
//! both: the row count stays 1 *and* `maple-daily` still names `discord-maple`.
//!
//! Withdrawing consent takes the schedules with it, the same way Task 1455
//! drops the switch record: a schedule can never outlive the approval that
//! allowed it.

use serde::Serialize;
use serde_json::json;

use crate::autoscrub_account_switches::{
    is_scrub_account_id, require_account_approved_for_autoscrub, AutoScrubAccount,
    AutoScrubAccountSwitchSurface, AutoScrubSwitchRefusal, ScrubAccountPermissionInput,
    ScrubAccountPermissionRead,
};
use crate::autoscrub_pro_gate::{
    AutoScrubProSurface, AutoScrubSchedule, AutoScrubScheduleOutcome, AutoScrubScheduleRequest,
    AUTOSCRUB_SCHEDULE_COMMAND,
};

/// Listing command for the saved schedule rows. The save command is Task 1454's
/// [`AUTOSCRUB_SCHEDULE_COMMAND`], reused by name so a refusal names the command
/// a caller actually invoked.
pub const AUTOSCRUB_SCHEDULES_COMMAND: &str = "autoscrub_schedules";

/// Refusal reason for a schedule name that is not a well-formed name.
pub const SCHEDULE_NAME_INVALID: &str = "schedule_name_invalid";

/// Refusal reason for a cadence AutoScrub does not run on.
pub const SCHEDULE_CADENCE_INVALID: &str = "schedule_cadence_invalid";

/// Refusal reason for a command name this module does not own.
pub const UNKNOWN_COMMAND: &str = "unknown_command";

/// The cadences a schedule may run on. Same four choices the schedule storage
/// task (1463) names: daily, weekly, monthly, or only when chosen.
pub const SCHEDULE_CADENCES: [&str; 4] = ["daily", "weekly", "monthly", "when-chosen"];

/// The most schedule rows one surface may hold.
pub const MAX_SCHEDULES: usize = 32;

/// `true` iff `name` is a well-formed schedule name. Deliberately the same
/// shape rule Scrub account ids use (lowercase ascii, digits and dashes, no
/// leading or trailing dash, 1..=64 bytes) — `maple-daily` reads the same way
/// `discord-maple` does, and one rule is one rule to get wrong.
pub fn is_schedule_name(name: &str) -> bool {
    is_scrub_account_id(name)
}

/// `true` iff `cadence` is one of [`SCHEDULE_CADENCES`]. Byte-exact.
pub fn is_schedule_cadence(cadence: &str) -> bool {
    SCHEDULE_CADENCES.contains(&cadence)
}

/// One row of the schedule listing a screen renders.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubScheduleRow {
    pub schedule_name: String,
    pub account: String,
    pub cadence: String,
    /// The label of the account this row names, as the account list reports it.
    pub account_label: String,
}

/// What the listing command returns.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubScheduleListing {
    pub schedules: Vec<AutoScrubScheduleRow>,
    pub schedule_count: usize,
}

/// The account-aware AutoScrub schedule surface: the Task 1455 switch surface
/// (which carries the Pro gate and the normal Scrub consent) plus the saved
/// schedule rows, which live in the Task 1454 Pro surface's own schedule list so
/// there is exactly one store to count.
#[derive(Debug, Clone)]
pub struct AutoScrubScheduleSurface {
    switches: AutoScrubAccountSwitchSurface,
}

impl AutoScrubScheduleSurface {
    /// A fresh surface: nothing approved, so no account may be scheduled.
    pub fn new(pro: AutoScrubProSurface, accounts: Vec<AutoScrubAccount>) -> Self {
        Self {
            switches: AutoScrubAccountSwitchSurface::new(pro, accounts),
        }
    }

    pub fn switches(&self) -> &AutoScrubAccountSwitchSurface {
        &self.switches
    }

    pub fn switches_mut(&mut self) -> &mut AutoScrubAccountSwitchSurface {
        &mut self.switches
    }

    pub fn pro(&self) -> &AutoScrubProSurface {
        self.switches.pro()
    }

    pub fn pro_mut(&mut self) -> &mut AutoScrubProSurface {
        self.switches.pro_mut()
    }

    /// Run the normal Scrub consent step. An account that drops out of the
    /// consent record loses its schedules as well as its switch record, so a
    /// schedule never outlives the approval that allowed it.
    pub fn save_scrub_account_permissions(
        &mut self,
        input: ScrubAccountPermissionInput,
    ) -> Result<ScrubAccountPermissionRead, String> {
        let read = self.switches.save_scrub_account_permissions(input)?;
        let approved = self.switches.scrub_consent().clone();
        self.switches
            .pro_mut()
            .retain_schedules(|schedule| approved.is_approved(&schedule.account));
        Ok(read)
    }

    fn label_for(&self, account_id: &str) -> String {
        self.switches
            .accounts()
            .iter()
            .find(|account| account.account_id == account_id)
            .map(|account| account.label.clone())
            .unwrap_or_else(|| account_id.to_string())
    }

    fn is_known(&self, account_id: &str) -> bool {
        self.switches
            .accounts()
            .iter()
            .any(|account| account.account_id == account_id)
    }

    /// The check a schedule save runs, in the same order the switch runs it:
    /// Pro, then the normal Scrub approval, then "is this an account we know",
    /// then the schedule's own shape. Nothing is written before this returns.
    fn require_schedulable(
        &self,
        request: &AutoScrubScheduleRequest,
    ) -> Result<(), AutoScrubSwitchRefusal> {
        let command = AUTOSCRUB_SCHEDULE_COMMAND;
        let account = request.account.as_str();
        if !self.switches.pro().pro_unlocked() {
            return Err(AutoScrubSwitchRefusal::pro_code_required(command, account));
        }
        let _ = require_account_approved_for_autoscrub(self.switches.scrub_consent(), command, account);
        if !self.is_known(account) {
            return Err(AutoScrubSwitchRefusal::unknown_account(command, account));
        }
        if !is_schedule_name(&request.schedule_name) {
            return Err(AutoScrubSwitchRefusal::new(
                command,
                account,
                SCHEDULE_NAME_INVALID,
                format!(
                    "{:?} is not a schedule name AutoScrub can save. Nothing was changed.",
                    request.schedule_name
                ),
            ));
        }
        if !is_schedule_cadence(&request.cadence) {
            return Err(AutoScrubSwitchRefusal::new(
                command,
                account,
                SCHEDULE_CADENCE_INVALID,
                format!(
                    "{:?} is not a schedule AutoScrub runs on. Nothing was changed.",
                    request.cadence
                ),
            ));
        }
        if self.schedule_named(&request.schedule_name).is_none()
            && self.schedule_row_count() >= MAX_SCHEDULES
        {
            return Err(AutoScrubSwitchRefusal::new(
                command,
                account,
                SCHEDULE_NAME_INVALID,
                format!("AutoScrub holds at most {MAX_SCHEDULES} schedules. Nothing was changed."),
            ));
        }
        Ok(())
    }

    /// `true` iff this account could be named by a schedule right now. Same
    /// answer [`AutoScrubAccountSwitchSurface::switch_available`] gives, because
    /// it is the same check.
    pub fn account_schedulable(&self, account_id: &str) -> bool {
        self.switches.switch_available(account_id)
    }

    /// Save one schedule. Refused before anything is written, so a refused save
    /// leaves the schedule rows exactly as they were. Saving under a name that
    /// already exists replaces that row.
    pub fn save_schedule(
        &mut self,
        request: AutoScrubScheduleRequest,
    ) -> Result<AutoScrubScheduleOutcome, AutoScrubSwitchRefusal> {
        self.require_schedulable(&request)?;
        let name = request.schedule_name.clone();
        self.switches
            .pro_mut()
            .retain_schedules(|schedule| schedule.schedule_name != name);
        match self.switches.pro_mut().schedule(request) {
            Ok(outcome) => Ok(outcome),
            // Unreachable in practice: `require_schedulable` already checked the
            // very same Pro gate. Mapped rather than unwrapped so a future
            // refusal reason cannot turn into a panic.
            Err(refusal) => Err(AutoScrubSwitchRefusal::new(
                refusal.command,
                String::new(),
                refusal.reason,
                refusal.message,
            )),
        }
    }

    pub fn schedules(&self) -> &[AutoScrubSchedule] {
        self.switches.pro().schedules()
    }

    /// The number of saved schedule rows — the count the Task 1457 finish line
    /// reads.
    pub fn schedule_row_count(&self) -> usize {
        self.schedules().len()
    }

    pub fn schedule_named(&self, schedule_name: &str) -> Option<&AutoScrubSchedule> {
        self.schedules()
            .iter()
            .find(|schedule| schedule.schedule_name == schedule_name)
    }

    /// Every account named by any saved schedule, in row order. The Task 1457
    /// check reads this to prove an unapproved account never got in.
    pub fn scheduled_account_ids(&self) -> Vec<String> {
        self.schedules()
            .iter()
            .map(|schedule| schedule.account.clone())
            .collect()
    }

    /// The accounts named by one schedule. A schedule names exactly one account,
    /// so this is one id or none — but the Task 1457 finish line says "names
    /// only discord-maple", and a list is what answers "only".
    pub fn accounts_named_by(&self, schedule_name: &str) -> Vec<String> {
        self.schedules()
            .iter()
            .filter(|schedule| schedule.schedule_name == schedule_name)
            .map(|schedule| schedule.account.clone())
            .collect()
    }

    pub fn listing(&self) -> AutoScrubScheduleListing {
        let schedules: Vec<AutoScrubScheduleRow> = self
            .schedules()
            .iter()
            .map(|schedule| AutoScrubScheduleRow {
                schedule_name: schedule.schedule_name.clone(),
                account: schedule.account.clone(),
                cadence: schedule.cadence.clone(),
                account_label: self.label_for(&schedule.account),
            })
            .collect();
        AutoScrubScheduleListing {
            schedule_count: schedules.len(),
            schedules,
        }
    }
}

fn ok_reply(command: &str, result: serde_json::Value) -> String {
    json!({ "ok": true, "command": command, "result": result }).to_string()
}

fn refusal_reply(refusal: &AutoScrubSwitchRefusal) -> String {
    json!({
        "ok": false,
        "command": refusal.command,
        "accountId": refusal.account_id,
        "errorCode": refusal.reason,
        "error": refusal.message,
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

/// The direct-invoke surface: what a caller reaches when it skips the schedule
/// screen and names the command itself. It runs the same check, so typing an
/// account id into a schedule is not a way past the switch.
pub fn run_autoscrub_schedule_command(
    surface: &mut AutoScrubScheduleSurface,
    command: &str,
    request_json: &str,
) -> String {
    match command {
        AUTOSCRUB_SCHEDULES_COMMAND => ok_reply(command, json!(surface.listing())),
        AUTOSCRUB_SCHEDULE_COMMAND => {
            match serde_json::from_str::<AutoScrubScheduleRequest>(request_json) {
                Ok(request) => match surface.save_schedule(request) {
                    Ok(outcome) => ok_reply(command, json!(outcome)),
                    Err(refusal) => refusal_reply(&refusal),
                },
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

/// The Task 1457 fixture: the approved account the good schedule names.
pub const TASK_1457_APPROVED_ACCOUNT: &str = "discord-maple";

/// The Task 1457 fixture: the unapproved account the bad copy names. A sibling
/// id on the same app, so what is being told apart is the approval and not the
/// spelling.
pub const TASK_1457_UNAPPROVED_ACCOUNT: &str = "discord-pine";

/// The Task 1457 fixture schedule name.
pub const TASK_1457_SCHEDULE_NAME: &str = "maple-daily";

/// The Task 1457 fixture cadence.
pub const TASK_1457_SCHEDULE_CADENCE: &str = "daily";

/// The schedule request the good save uses, as a request body.
pub fn task_1457_schedule_request(account: &str) -> serde_json::Value {
    json!({
        "scheduleName": TASK_1457_SCHEDULE_NAME,
        "account": account,
        "cadence": TASK_1457_SCHEDULE_CADENCE,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autoscrub_account_switches::{
        fixture_autoscrub_accounts, fixture_available_account_ids,
        ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB, UNKNOWN_ACCOUNT,
    };
    use crate::autoscrub_pro_gate::{
        fixture_pro_code_directory, AutoScrubProSurface, FIXTURE_ACTIVE_PRO_CODE, PRO_CODE_REQUIRED,
    };

    fn locked_surface() -> AutoScrubScheduleSurface {
        AutoScrubScheduleSurface::new(
            AutoScrubProSurface::new(fixture_pro_code_directory()),
            fixture_autoscrub_accounts(),
        )
    }

    fn unlocked_surface() -> AutoScrubScheduleSurface {
        let mut surface = locked_surface();
        surface.pro_mut().present_pro_code(FIXTURE_ACTIVE_PRO_CODE);
        surface
    }

    fn approve(surface: &mut AutoScrubScheduleSurface, selected: &[&str]) {
        let available: Vec<String> = fixture_available_account_ids();
        let available_refs: Vec<&str> = available.iter().map(String::as_str).collect();
        surface
            .save_scrub_account_permissions(ScrubAccountPermissionInput::new(
                &available_refs,
                selected,
            ))
            .expect("consent save");
    }

    fn request(account: &str) -> AutoScrubScheduleRequest {
        serde_json::from_value(task_1457_schedule_request(account)).expect("request")
    }

    #[test]
    fn an_approved_account_can_be_scheduled() {
        let mut surface = unlocked_surface();
        approve(&mut surface, &[TASK_1457_APPROVED_ACCOUNT]);
        assert_eq!(surface.schedule_row_count(), 0);
        let outcome = surface
            .save_schedule(request(TASK_1457_APPROVED_ACCOUNT))
            .expect("approved account schedules");
        assert_eq!(outcome.schedule_count, 1);
        assert_eq!(outcome.saved.account, TASK_1457_APPROVED_ACCOUNT);
        assert_eq!(surface.schedule_row_count(), 1);
    }

    #[test]
    fn an_unapproved_account_cannot_be_scheduled() {
        let mut surface = unlocked_surface();
        approve(&mut surface, &[TASK_1457_APPROVED_ACCOUNT]);
        let refusal = surface
            .save_schedule(request(TASK_1457_UNAPPROVED_ACCOUNT))
            .expect_err("unapproved account is refused");
        assert_eq!(refusal.reason, ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB);
        assert_eq!(refusal.account_id, TASK_1457_UNAPPROVED_ACCOUNT);
        assert_eq!(surface.schedule_row_count(), 0);
    }

    #[test]
    fn the_bad_copy_cannot_rewrite_the_good_schedule() {
        let mut surface = unlocked_surface();
        approve(&mut surface, &[TASK_1457_APPROVED_ACCOUNT]);
        surface
            .save_schedule(request(TASK_1457_APPROVED_ACCOUNT))
            .expect("good save");
        let _ = surface.save_schedule(request(TASK_1457_UNAPPROVED_ACCOUNT));
        assert_eq!(surface.schedule_row_count(), 1);
        assert_eq!(
            surface.accounts_named_by(TASK_1457_SCHEDULE_NAME),
            vec![TASK_1457_APPROVED_ACCOUNT.to_string()]
        );
    }

    #[test]
    fn re_saving_the_same_name_replaces_the_row() {
        let mut surface = unlocked_surface();
        approve(&mut surface, &[TASK_1457_APPROVED_ACCOUNT]);
        surface
            .save_schedule(request(TASK_1457_APPROVED_ACCOUNT))
            .expect("first save");
        let mut second = request(TASK_1457_APPROVED_ACCOUNT);
        second.cadence = "weekly".to_string();
        let outcome = surface.save_schedule(second).expect("second save");
        assert_eq!(outcome.schedule_count, 1);
        assert_eq!(surface.schedule_row_count(), 1);
        assert_eq!(
            surface
                .schedule_named(TASK_1457_SCHEDULE_NAME)
                .map(|schedule| schedule.cadence.as_str()),
            Some("weekly")
        );
    }

    #[test]
    fn a_locked_pro_gate_refuses_the_schedule_first() {
        let mut surface = locked_surface();
        approve(&mut surface, &[TASK_1457_APPROVED_ACCOUNT]);
        let refusal = surface
            .save_schedule(request(TASK_1457_APPROVED_ACCOUNT))
            .expect_err("locked Pro gate refuses");
        assert_eq!(refusal.reason, PRO_CODE_REQUIRED);
        assert_eq!(surface.schedule_row_count(), 0);
    }

    #[test]
    fn an_account_nobody_signed_in_is_refused_as_unapproved() {
        let mut surface = unlocked_surface();
        approve(&mut surface, &[TASK_1457_APPROVED_ACCOUNT]);
        let refusal = surface
            .save_schedule(request("discord-birch"))
            .expect_err("unknown account is refused");
        // Approval is checked before "do we know this account", so an account
        // that was never signed in is refused as unapproved: it cannot be in
        // the consent record either.
        assert_eq!(refusal.reason, ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB);
        assert_eq!(surface.schedule_row_count(), 0);
    }

    #[test]
    fn an_approved_account_that_is_not_signed_in_is_refused_as_unknown() {
        let mut surface = unlocked_surface();
        let available = vec![
            "discord-maple",
            "discord-pine",
            "telegram-pine",
            "discord-birch",
        ];
        surface
            .save_scrub_account_permissions(ScrubAccountPermissionInput::new(
                &available,
                &["discord-birch"],
            ))
            .expect("consent save");
        let refusal = surface
            .save_schedule(request("discord-birch"))
            .expect_err("unknown account is refused");
        assert_eq!(refusal.reason, UNKNOWN_ACCOUNT);
        assert_eq!(surface.schedule_row_count(), 0);
    }

    #[test]
    fn a_near_miss_id_is_not_the_approved_account() {
        let mut surface = unlocked_surface();
        approve(&mut surface, &[TASK_1457_APPROVED_ACCOUNT]);
        for id in ["discord-mapl", "discord-maple-2", "discord maple", ""] {
            let refusal = surface
                .save_schedule(request(id))
                .expect_err("near-miss id is refused");
            assert_eq!(refusal.reason, ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB, "{id}");
        }
        assert_eq!(surface.schedule_row_count(), 0);
    }

    #[test]
    fn a_bad_schedule_name_or_cadence_is_refused() {
        let mut surface = unlocked_surface();
        approve(&mut surface, &[TASK_1457_APPROVED_ACCOUNT]);
        let mut bad_name = request(TASK_1457_APPROVED_ACCOUNT);
        bad_name.schedule_name = "Maple Daily".to_string();
        assert_eq!(
            surface
                .save_schedule(bad_name)
                .expect_err("bad name")
                .reason,
            SCHEDULE_NAME_INVALID
        );
        let mut bad_cadence = request(TASK_1457_APPROVED_ACCOUNT);
        bad_cadence.cadence = "hourly".to_string();
        assert_eq!(
            surface
                .save_schedule(bad_cadence)
                .expect_err("bad cadence")
                .reason,
            SCHEDULE_CADENCE_INVALID
        );
        assert_eq!(surface.schedule_row_count(), 0);
    }

    #[test]
    fn withdrawing_consent_drops_the_schedule() {
        let mut surface = unlocked_surface();
        approve(&mut surface, &[TASK_1457_APPROVED_ACCOUNT]);
        surface
            .save_schedule(request(TASK_1457_APPROVED_ACCOUNT))
            .expect("good save");
        assert_eq!(surface.schedule_row_count(), 1);
        approve(&mut surface, &[]);
        assert_eq!(surface.schedule_row_count(), 0);
    }

    #[test]
    fn the_direct_invoke_runs_the_same_check() {
        let mut surface = unlocked_surface();
        approve(&mut surface, &[TASK_1457_APPROVED_ACCOUNT]);
        let good = run_autoscrub_schedule_command(
            &mut surface,
            AUTOSCRUB_SCHEDULE_COMMAND,
            &task_1457_schedule_request(TASK_1457_APPROVED_ACCOUNT).to_string(),
        );
        let good: serde_json::Value = serde_json::from_str(&good).expect("json");
        assert_eq!(good["ok"], serde_json::Value::Bool(true));
        assert_eq!(good["result"]["scheduleCount"], 1);

        let bad = run_autoscrub_schedule_command(
            &mut surface,
            AUTOSCRUB_SCHEDULE_COMMAND,
            &task_1457_schedule_request(TASK_1457_UNAPPROVED_ACCOUNT).to_string(),
        );
        let bad: serde_json::Value = serde_json::from_str(&bad).expect("json");
        assert_eq!(bad["ok"], serde_json::Value::Bool(false));
        assert_eq!(bad["errorCode"], ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB);
        assert_eq!(surface.schedule_row_count(), 1);
        assert_eq!(
            surface.accounts_named_by(TASK_1457_SCHEDULE_NAME),
            vec![TASK_1457_APPROVED_ACCOUNT.to_string()]
        );
    }

    #[test]
    fn an_unknown_command_is_refused_by_name() {
        let mut surface = unlocked_surface();
        let reply = run_autoscrub_schedule_command(&mut surface, "autoscrub_anything", "{}");
        let reply: serde_json::Value = serde_json::from_str(&reply).expect("json");
        assert_eq!(reply["ok"], serde_json::Value::Bool(false));
        assert_eq!(reply["errorCode"], UNKNOWN_COMMAND);
    }
}
