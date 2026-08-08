//! Task 1454: the AutoScrub Pro gate.
//!
//! AutoScrub is the paid half of Scrub. Free finds and reviews; Pro schedules,
//! records, and can delete. This module is the single place that decides
//! whether the four AutoScrub commands — setup, schedule, deletion, and
//! activity history — may run at all.
//!
//! The gate is keyed on an **active Pro code**, not on a tier flag. A tier flag
//! is a boolean somebody can flip; a code has to be presented, matched exactly
//! against the directory, and still be `Active` there. Three consequences the
//! tests pin down:
//!
//! - Every one of the four commands is refused **by its own name** while no
//!   active code is held. The refusal names the command, so a caller (and a
//!   screen built on [`AutoScrubProSurface::controls`]) can say which control
//!   is locked rather than showing a generic paywall.
//! - The refusal happens at the entry of each command, **before** any state is
//!   read or written. A refused Free call leaves no schedule, deletes nothing,
//!   and returns no activity history — not even an empty list, because "here is
//!   your empty history" is still an answer only Pro is entitled to.
//! - A direct invoke through [`run_autoscrub_pro_command`] is refused by the
//!   same check. The control listing hiding a button is presentation; this is
//!   the enforcement. Removing the listing does not open the command.
//!
//! Matching is **exact**. A lowercase copy, a copy with a stray space, and a
//! one-character-off copy of the active fixture code are all refused, and so
//! are the expired and revoked fixture codes. Only the exact active code
//! changes what is available.
//!
//! The code directory is injected ([`ProCodeDirectory`]), so this module holds
//! no shipping secret and performs no I/O. [`fixture_pro_code_directory`] is
//! the named fixture the Task 1454 check and its direct-invoke example share.

use serde::{Deserialize, Serialize};
use serde_json::json;

/// The four AutoScrub commands this gate covers, by the exact name a caller
/// invokes.
pub const AUTOSCRUB_SETUP_COMMAND: &str = "autoscrub_setup";
pub const AUTOSCRUB_SCHEDULE_COMMAND: &str = "autoscrub_schedule";
pub const AUTOSCRUB_DELETION_COMMAND: &str = "autoscrub_deletion";
pub const AUTOSCRUB_ACTIVITY_COMMAND: &str = "autoscrub_activity";

/// Command names in the order the AutoScrub screen lists them.
pub const AUTOSCRUB_PRO_COMMANDS: [&str; 4] = [
    AUTOSCRUB_SETUP_COMMAND,
    AUTOSCRUB_SCHEDULE_COMMAND,
    AUTOSCRUB_DELETION_COMMAND,
    AUTOSCRUB_ACTIVITY_COMMAND,
];

/// Machine-readable refusal reason shared by all four commands.
pub const PRO_CODE_REQUIRED: &str = "pro_code_required";

/// Refusal reason for a command name this gate does not own.
pub const UNKNOWN_COMMAND: &str = "unknown_command";

/// Fixture codes for the Task 1454 check. These are test data for an injected
/// [`ProCodeDirectory`]; nothing in the gate special-cases them, so a shipping
/// caller supplies a real directory and the same rules apply.
pub const FIXTURE_ACTIVE_PRO_CODE: &str = "OSL-1454-AUTO-SCRB-PRO1";
pub const FIXTURE_EXPIRED_PRO_CODE: &str = "OSL-1454-AUTO-SCRB-EXPD";
pub const FIXTURE_REVOKED_PRO_CODE: &str = "OSL-1454-AUTO-SCRB-RVKD";

/// Status the directory holds for a known code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProCodeStatus {
    Active,
    Expired,
    Revoked,
}

impl ProCodeStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Expired => "expired",
            Self::Revoked => "revoked",
        }
    }
}

/// One code the directory knows about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProCodeRecord {
    pub code: String,
    pub status: ProCodeStatus,
}

impl ProCodeRecord {
    pub fn new(code: impl Into<String>, status: ProCodeStatus) -> Self {
        Self {
            code: code.into(),
            status,
        }
    }
}

/// The set of codes the gate will consider. Lookup is byte-exact: no trimming,
/// no case folding, no dash-stripping. A code the user mistyped is a code the
/// directory does not contain.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProCodeDirectory {
    records: Vec<ProCodeRecord>,
}

impl ProCodeDirectory {
    pub fn new(records: Vec<ProCodeRecord>) -> Self {
        Self { records }
    }

    pub fn status_of(&self, presented: &str) -> Option<ProCodeStatus> {
        self.records
            .iter()
            .find(|record| record.code == presented)
            .map(|record| record.status)
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Flip a known code's status. Used by the "Pro ended" path and by the
    /// check that proves a relock follows the code, not a separate switch.
    pub fn set_status(&mut self, code: &str, status: ProCodeStatus) -> bool {
        for record in self.records.iter_mut() {
            if record.code == code {
                record.status = status;
                return true;
            }
        }
        false
    }
}

/// The Task 1454 fixture directory: one active code, one expired, one revoked.
pub fn fixture_pro_code_directory() -> ProCodeDirectory {
    ProCodeDirectory::new(vec![
        ProCodeRecord::new(FIXTURE_ACTIVE_PRO_CODE, ProCodeStatus::Active),
        ProCodeRecord::new(FIXTURE_EXPIRED_PRO_CODE, ProCodeStatus::Expired),
        ProCodeRecord::new(FIXTURE_REVOKED_PRO_CODE, ProCodeStatus::Revoked),
    ])
}

/// What the gate made of a presented code. Only [`ProCodeVerdict::Active`]
/// unlocks anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProCodeVerdict {
    /// Exact match on a code the directory holds as `Active`.
    Active,
    /// Exact match, but the directory holds it as `Expired`.
    Expired,
    /// Exact match, but the directory holds it as `Revoked`.
    Revoked,
    /// Correctly shaped, but no such code.
    NotRecognized,
    /// Not an `OSL-XXXX-XXXX-XXXX-XXXX` code at all (wrong length, lowercase,
    /// stray whitespace, wrong separators).
    Malformed,
    /// Nothing was entered.
    Missing,
}

impl ProCodeVerdict {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Expired => "expired",
            Self::Revoked => "revoked",
            Self::NotRecognized => "not_recognized",
            Self::Malformed => "malformed",
            Self::Missing => "missing",
        }
    }

    /// The single place that decides whether a verdict opens AutoScrub.
    pub const fn unlocks(self) -> bool {
        matches!(self, Self::Active)
    }
}

/// `true` iff `candidate` is exactly `OSL-XXXX-XXXX-XXXX-XXXX` with uppercase
/// alphanumeric groups. Deliberately strict: the entry field on the Pro code
/// screen already uppercases and caps length, so anything else reaching here is
/// a different string, not a variant spelling of the same one.
pub fn is_pro_code_shaped(candidate: &str) -> bool {
    let mut groups = candidate.split('-');
    if groups.next() != Some("OSL") {
        return false;
    }
    let mut seen = 0usize;
    for group in groups {
        seen += 1;
        if seen > 4 {
            return false;
        }
        if group.len() != 4 {
            return false;
        }
        if !group
            .chars()
            .all(|character| character.is_ascii_uppercase() || character.is_ascii_digit())
        {
            return false;
        }
    }
    seen == 4
}

/// Classify a presented code against the directory without touching any gate
/// state.
pub fn classify_pro_code(directory: &ProCodeDirectory, presented: &str) -> ProCodeVerdict {
    if presented.is_empty() {
        return ProCodeVerdict::Missing;
    }
    if !is_pro_code_shaped(presented) {
        return ProCodeVerdict::Malformed;
    }
    match directory.status_of(presented) {
        Some(ProCodeStatus::Active) => ProCodeVerdict::Active,
        Some(ProCodeStatus::Expired) => ProCodeVerdict::Expired,
        Some(ProCodeStatus::Revoked) => ProCodeVerdict::Revoked,
        None => ProCodeVerdict::NotRecognized,
    }
}

/// A refused AutoScrub command. Always names the command that was refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AutoScrubProRefusal {
    pub command: String,
    pub reason: String,
    pub message: String,
}

impl AutoScrubProRefusal {
    fn pro_code_required(command: &str) -> Self {
        Self {
            command: command.to_string(),
            reason: PRO_CODE_REQUIRED.to_string(),
            message: format!(
                "{} Enter an active Pro code to unlock AutoScrub. Nothing was changed.",
                locked_sentence(command)
            ),
        }
    }
}

impl std::fmt::Display for AutoScrubProRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{} refused: {}", self.command, self.message)
    }
}

impl std::error::Error for AutoScrubProRefusal {}

fn locked_sentence(command: &str) -> &'static str {
    match command {
        AUTOSCRUB_SETUP_COMMAND => "AutoScrub setup needs an active Pro code.",
        AUTOSCRUB_SCHEDULE_COMMAND => "AutoScrub schedules need an active Pro code.",
        AUTOSCRUB_DELETION_COMMAND => "AutoScrub deletion needs an active Pro code.",
        AUTOSCRUB_ACTIVITY_COMMAND => "AutoScrub activity history needs an active Pro code.",
        _ => "This AutoScrub control needs an active Pro code.",
    }
}

/// The human label the AutoScrub screen puts on each control.
pub fn control_label(command: &str) -> &'static str {
    match command {
        AUTOSCRUB_SETUP_COMMAND => "Set up AutoScrub",
        AUTOSCRUB_SCHEDULE_COMMAND => "AutoScrub schedules",
        AUTOSCRUB_DELETION_COMMAND => "AutoScrub deletion",
        AUTOSCRUB_ACTIVITY_COMMAND => "AutoScrub activity history",
        _ => "AutoScrub",
    }
}

/// One row of the AutoScrub control listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AutoScrubControl {
    pub command: String,
    pub label: String,
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal: Option<AutoScrubProRefusal>,
}

/// Setup request: which already-approved accounts AutoScrub may act on.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubSetupRequest {
    #[serde(default)]
    pub accounts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubSetupOutcome {
    pub accounts: Vec<String>,
    pub account_count: usize,
}

/// Schedule request: one named schedule for one account.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubScheduleRequest {
    #[serde(default)]
    pub schedule_name: String,
    #[serde(default)]
    pub account: String,
    #[serde(default)]
    pub cadence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubSchedule {
    pub schedule_name: String,
    pub account: String,
    pub cadence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubScheduleOutcome {
    pub saved: AutoScrubSchedule,
    pub schedule_count: usize,
}

/// Deletion request: the messages a review already marked.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubDeletionRequest {
    #[serde(default)]
    pub account: String,
    #[serde(default)]
    pub marked_locators: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubDeletionOutcome {
    pub account: String,
    pub deleted: Vec<String>,
    pub deleted_count: usize,
}

/// One row of AutoScrub activity history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubActivityEntry {
    pub command: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubActivityHistory {
    pub entries: Vec<AutoScrubActivityEntry>,
    pub entry_count: usize,
}

/// The gated AutoScrub surface: the code the user holds plus the state the four
/// commands own.
#[derive(Debug, Clone)]
pub struct AutoScrubProSurface {
    directory: ProCodeDirectory,
    active_code: Option<String>,
    accounts: Vec<String>,
    schedules: Vec<AutoScrubSchedule>,
    activity: Vec<AutoScrubActivityEntry>,
}

impl AutoScrubProSurface {
    /// A brand-new surface holds no code, so every control is locked.
    pub fn new(directory: ProCodeDirectory) -> Self {
        Self {
            directory,
            active_code: None,
            accounts: Vec::new(),
            schedules: Vec::new(),
            activity: Vec::new(),
        }
    }

    pub fn directory(&self) -> &ProCodeDirectory {
        &self.directory
    }

    /// The exact code currently unlocking AutoScrub, if any.
    pub fn active_code(&self) -> Option<&str> {
        self.active_code.as_deref()
    }

    /// The one load-bearing predicate. Every command ANDs against this, and it
    /// re-checks the held code against the directory on each call so a code
    /// that stopped being `Active` stops unlocking immediately.
    pub fn pro_unlocked(&self) -> bool {
        match self.active_code.as_deref() {
            Some(code) => self.directory.status_of(code) == Some(ProCodeStatus::Active),
            None => false,
        }
    }

    /// Present a code. Only an `Active` verdict changes what is unlocked; every
    /// other verdict leaves the surface exactly as it was.
    pub fn present_pro_code(&mut self, presented: &str) -> ProCodeVerdict {
        let verdict = classify_pro_code(&self.directory, presented);
        if verdict.unlocks() {
            self.active_code = Some(presented.to_string());
        }
        verdict
    }

    /// Give back the code currently held (a sign-out / "remove this code"), not
    /// a way to unlock.
    pub fn clear_pro_code(&mut self) {
        self.active_code = None;
    }

    fn require_pro(&self, command: &str) -> Result<(), AutoScrubProRefusal> {
        if self.pro_unlocked() {
            return Ok(());
        }
        Err(AutoScrubProRefusal::pro_code_required(command))
    }

    /// The control listing a screen renders. Locked rows carry the same
    /// by-name refusal the command itself would return.
    pub fn controls(&self) -> Vec<AutoScrubControl> {
        AUTOSCRUB_PRO_COMMANDS
            .iter()
            .map(|command| {
                let refusal = self.require_pro(command).err();
                AutoScrubControl {
                    command: (*command).to_string(),
                    label: control_label(command).to_string(),
                    available: refusal.is_none(),
                    refusal,
                }
            })
            .collect()
    }

    pub fn available_commands(&self) -> Vec<String> {
        self.controls()
            .into_iter()
            .filter(|control| control.available)
            .map(|control| control.command)
            .collect()
    }

    pub fn refused_commands(&self) -> Vec<String> {
        self.controls()
            .into_iter()
            .filter(|control| !control.available)
            .map(|control| control.command)
            .collect()
    }

    fn record(&mut self, command: &str, detail: String) {
        self.activity.push(AutoScrubActivityEntry {
            command: command.to_string(),
            detail,
        });
    }

    /// Setup. Refused before the gate opens, so a Free caller cannot even name
    /// the accounts AutoScrub would cover.
    pub fn setup(
        &mut self,
        request: AutoScrubSetupRequest,
    ) -> Result<AutoScrubSetupOutcome, AutoScrubProRefusal> {
        self.require_pro(AUTOSCRUB_SETUP_COMMAND)?;
        self.accounts = request.accounts.clone();
        let outcome = AutoScrubSetupOutcome {
            account_count: self.accounts.len(),
            accounts: self.accounts.clone(),
        };
        self.record(
            AUTOSCRUB_SETUP_COMMAND,
            format!("set up {} account(s)", outcome.account_count),
        );
        Ok(outcome)
    }

    /// Save one schedule.
    pub fn schedule(
        &mut self,
        request: AutoScrubScheduleRequest,
    ) -> Result<AutoScrubScheduleOutcome, AutoScrubProRefusal> {
        self.require_pro(AUTOSCRUB_SCHEDULE_COMMAND)?;
        let saved = AutoScrubSchedule {
            schedule_name: request.schedule_name,
            account: request.account,
            cadence: request.cadence,
        };
        self.schedules.push(saved.clone());
        self.record(
            AUTOSCRUB_SCHEDULE_COMMAND,
            format!("saved schedule {}", saved.schedule_name),
        );
        Ok(AutoScrubScheduleOutcome {
            saved,
            schedule_count: self.schedules.len(),
        })
    }

    pub fn schedules(&self) -> &[AutoScrubSchedule] {
        &self.schedules
    }

    /// Drop every saved schedule the predicate does not keep. The account-aware
    /// schedule surface (Task 1457) uses this to re-save a schedule under a name
    /// it already holds, and to drop the schedules of an account whose normal
    /// Scrub consent has been withdrawn. It never *adds* a schedule, so it
    /// cannot become a way around [`AutoScrubProSurface::schedule`]'s Pro gate.
    pub fn retain_schedules<F>(&mut self, keep: F)
    where
        F: FnMut(&AutoScrubSchedule) -> bool,
    {
        self.schedules.retain(keep);
    }

    /// Delete the messages a review already marked.
    pub fn deletion(
        &mut self,
        request: AutoScrubDeletionRequest,
    ) -> Result<AutoScrubDeletionOutcome, AutoScrubProRefusal> {
        self.require_pro(AUTOSCRUB_DELETION_COMMAND)?;
        let outcome = AutoScrubDeletionOutcome {
            account: request.account,
            deleted_count: request.marked_locators.len(),
            deleted: request.marked_locators,
        };
        self.record(
            AUTOSCRUB_DELETION_COMMAND,
            format!("deleted {} message(s)", outcome.deleted_count),
        );
        Ok(outcome)
    }

    /// Activity history. A locked caller gets a refusal, not an empty list:
    /// "you have no AutoScrub history" is still an answer about their account.
    pub fn activity(&self) -> Result<AutoScrubActivityHistory, AutoScrubProRefusal> {
        self.require_pro(AUTOSCRUB_ACTIVITY_COMMAND)?;
        Ok(AutoScrubActivityHistory {
            entries: self.activity.clone(),
            entry_count: self.activity.len(),
        })
    }
}

fn ok_reply(command: &str, result: serde_json::Value) -> String {
    json!({ "ok": true, "command": command, "result": result }).to_string()
}

fn refusal_reply(refusal: &AutoScrubProRefusal) -> String {
    json!({
        "ok": false,
        "command": refusal.command,
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

/// The direct-invoke surface: what a caller reaches when it skips the control
/// listing entirely and names the command itself. Every reply is a JSON object
/// carrying `ok`, so a refusal is never mistaken for a transport failure.
pub fn run_autoscrub_pro_command(
    surface: &mut AutoScrubProSurface,
    command: &str,
    request_json: &str,
) -> String {
    match command {
        AUTOSCRUB_SETUP_COMMAND => {
            if let Err(refusal) = surface.require_pro(command) {
                return refusal_reply(&refusal);
            }
            match serde_json::from_str::<AutoScrubSetupRequest>(request_json) {
                Ok(request) => match surface.setup(request) {
                    Ok(outcome) => ok_reply(command, json!(outcome)),
                    Err(refusal) => refusal_reply(&refusal),
                },
                Err(error) => bad_request_reply(command, error.to_string()),
            }
        }
        AUTOSCRUB_SCHEDULE_COMMAND => {
            if let Err(refusal) = surface.require_pro(command) {
                return refusal_reply(&refusal);
            }
            match serde_json::from_str::<AutoScrubScheduleRequest>(request_json) {
                Ok(request) => match surface.schedule(request) {
                    Ok(outcome) => ok_reply(command, json!(outcome)),
                    Err(refusal) => refusal_reply(&refusal),
                },
                Err(error) => bad_request_reply(command, error.to_string()),
            }
        }
        AUTOSCRUB_DELETION_COMMAND => {
            if let Err(refusal) = surface.require_pro(command) {
                return refusal_reply(&refusal);
            }
            match serde_json::from_str::<AutoScrubDeletionRequest>(request_json) {
                Ok(request) => match surface.deletion(request) {
                    Ok(outcome) => ok_reply(command, json!(outcome)),
                    Err(refusal) => refusal_reply(&refusal),
                },
                Err(error) => bad_request_reply(command, error.to_string()),
            }
        }
        AUTOSCRUB_ACTIVITY_COMMAND => match surface.activity() {
            Ok(history) => ok_reply(command, json!(history)),
            Err(refusal) => refusal_reply(&refusal),
        },
        other => json!({
            "ok": false,
            "command": other,
            "errorCode": UNKNOWN_COMMAND,
            "error": format!("{other} is not an AutoScrub command."),
        })
        .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn locked_surface() -> AutoScrubProSurface {
        AutoScrubProSurface::new(fixture_pro_code_directory())
    }

    #[test]
    fn a_new_surface_refuses_all_four_commands_by_name() {
        let surface = locked_surface();
        assert!(!surface.pro_unlocked());
        assert_eq!(surface.available_commands(), Vec::<String>::new());
        assert_eq!(
            surface.refused_commands(),
            vec![
                AUTOSCRUB_SETUP_COMMAND,
                AUTOSCRUB_SCHEDULE_COMMAND,
                AUTOSCRUB_DELETION_COMMAND,
                AUTOSCRUB_ACTIVITY_COMMAND,
            ]
        );
        for control in surface.controls() {
            let refusal = control.refusal.expect("locked control carries a refusal");
            assert_eq!(refusal.command, control.command);
            assert_eq!(refusal.reason, PRO_CODE_REQUIRED);
        }
    }

    #[test]
    fn only_the_exact_active_fixture_code_unlocks() {
        for candidate in [
            FIXTURE_EXPIRED_PRO_CODE,
            FIXTURE_REVOKED_PRO_CODE,
            "OSL-1454-AUTO-SCRB-PRO2",
            "osl-1454-auto-scrb-pro1",
            "OSL-1454-AUTO-SCRB-PRO1 ",
            "",
        ] {
            let mut surface = locked_surface();
            let verdict = surface.present_pro_code(candidate);
            assert!(!verdict.unlocks(), "{candidate} must not unlock");
            assert!(!surface.pro_unlocked(), "{candidate} must not unlock");
            assert_eq!(surface.active_code(), None);
        }

        let mut surface = locked_surface();
        assert_eq!(
            surface.present_pro_code(FIXTURE_ACTIVE_PRO_CODE),
            ProCodeVerdict::Active
        );
        assert!(surface.pro_unlocked());
        assert_eq!(surface.active_code(), Some(FIXTURE_ACTIVE_PRO_CODE));
    }

    #[test]
    fn a_refused_free_command_writes_nothing() {
        let mut surface = locked_surface();
        assert!(surface
            .schedule(AutoScrubScheduleRequest {
                schedule_name: "maple-daily".to_string(),
                account: "discord-maple".to_string(),
                cadence: "daily".to_string(),
            })
            .is_err());
        surface.present_pro_code(FIXTURE_ACTIVE_PRO_CODE);
        assert_eq!(surface.schedules().len(), 0);
        assert_eq!(
            surface.activity().expect("pro reads history").entry_count,
            0
        );
    }

    #[test]
    fn a_direct_invoke_is_refused_without_a_code() {
        let mut surface = locked_surface();
        let reply = run_autoscrub_pro_command(
            &mut surface,
            AUTOSCRUB_DELETION_COMMAND,
            r#"{"account":"discord-maple","markedLocators":["a","b"]}"#,
        );
        let parsed: serde_json::Value = serde_json::from_str(&reply).expect("json reply");
        assert_eq!(parsed["ok"], false);
        assert_eq!(parsed["command"], AUTOSCRUB_DELETION_COMMAND);
        assert_eq!(parsed["errorCode"], PRO_CODE_REQUIRED);
        assert!(parsed["result"].is_null());
    }

    #[test]
    fn the_same_code_going_inactive_relocks() {
        let mut surface = locked_surface();
        surface.present_pro_code(FIXTURE_ACTIVE_PRO_CODE);
        assert!(surface.pro_unlocked());
        assert!(surface
            .directory
            .set_status(FIXTURE_ACTIVE_PRO_CODE, ProCodeStatus::Revoked));
        assert!(!surface.pro_unlocked());
        assert_eq!(surface.refused_commands().len(), 4);
    }
}
