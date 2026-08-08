//! Task 1455: the AutoScrub account switches.
//!
//! AutoScrub never picks its own accounts. A person first walks the normal
//! Scrub account list, ticks the accounts Scrub may touch, and saves that
//! choice (Task 1401: "save chosen Scrub accounts"). Only an account that came
//! out of *that* step may be switched on for AutoScrub here.
//!
//! Three things this module holds to:
//!
//! - **Unavailable before approval.** [`AutoScrubAccountSwitchSurface::switches`]
//!   lists every signed-in account AutoScrub could cover, and a row for an
//!   account the normal Scrub consent has not approved is `available: false`
//!   and carries the refusal that names why. A screen renders the row shut; it
//!   does not have to invent the reason.
//! - **The listing is presentation, the command is enforcement.** A direct
//!   invoke through [`run_autoscrub_account_switch_command`] skips the listing
//!   entirely and runs the same check, so deleting the greyed-out row from a
//!   screen does not open the switch.
//! - **Only the exact approval opens it.** Matching is byte-exact against the
//!   saved consent record. A sibling account being approved, the account being
//!   merely *available* in the Scrub list but left unticked, a one-character-off
//!   id, and an active Pro code on its own all leave the switch shut.
//!
//! The Pro gate from Task 1454 stays in front of the mutation: AutoScrub is the
//! paid half of Scrub, so [`AutoScrubAccountSwitchSurface`] holds an
//! [`AutoScrubProSurface`] and a switch cannot be moved while it is locked.
//! Consent decides *which* accounts may be switched; the Pro code decides
//! whether AutoScrub may be set up at all. Both have to say yes.
//!
//! Withdrawing consent is not a no-op either: saving a Scrub consent record
//! that no longer names an account drops that account's AutoScrub record, so a
//! switch can never stay on for an account Scrub is no longer allowed to touch.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::autoscrub_pro_gate::{AutoScrubProSurface, PRO_CODE_REQUIRED};

/// The two commands this module owns, by the exact name a caller invokes.
pub const AUTOSCRUB_ACCOUNT_SWITCHES_COMMAND: &str = "autoscrub_account_switches";
pub const AUTOSCRUB_ACCOUNT_SWITCH_COMMAND: &str = "autoscrub_account_switch";

/// Machine-readable refusal reason for an account the normal Scrub consent has
/// not approved.
pub const ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB: &str = "account_not_approved_for_autoscrub";

/// Machine-readable refusal reason for an account that is not a signed-in
/// account AutoScrub knows about at all.
pub const UNKNOWN_ACCOUNT: &str = "unknown_account";

/// Refusal reason for a command name this module does not own.
pub const UNKNOWN_COMMAND: &str = "unknown_command";

/// The most accounts one consent record may carry. Same bound the Task 1401
/// store enforces, so a record that round-trips through this module cannot
/// become one the store would reject.
pub const MAX_SCRUB_ACCOUNTS: usize = 32;

/// What the normal Scrub account step was handed: everything on offer, and the
/// subset the person ticked. Same shape as the Task 1401 command input
/// (`apps/osl-hub/src/preferences.rs::ScrubAccountPermissionInput`).
#[derive(Debug, Clone, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrubAccountPermissionInput {
    #[serde(default)]
    pub available_account_ids: Vec<String>,
    #[serde(default)]
    pub selected_account_ids: Vec<String>,
}

impl ScrubAccountPermissionInput {
    pub fn new(available: &[&str], selected: &[&str]) -> Self {
        Self {
            available_account_ids: available.iter().map(|id| (*id).to_string()).collect(),
            selected_account_ids: selected.iter().map(|id| (*id).to_string()).collect(),
        }
    }
}

/// What a read of the normal Scrub consent returns: the approved account ids
/// and nothing else. Same shape as Task 1401's `ScrubAccountPermissionRead`.
#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrubAccountPermissionRead {
    pub account_ids: Vec<String>,
}

/// `true` iff `account_id` is a well-formed Scrub account id. Same rule the
/// Task 1401 store applies: lowercase ascii, digits and dashes, no leading or
/// trailing dash, 1..=64 bytes. An id that fails here never becomes an
/// approval, so it can never open a switch.
pub fn is_scrub_account_id(account_id: &str) -> bool {
    let bytes = account_id.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && (bytes[bytes.len() - 1].is_ascii_lowercase() || bytes[bytes.len() - 1].is_ascii_digit())
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

/// The saved normal-Scrub consent: the accounts a person ticked and saved in
/// the Scrub account list. This is the *only* thing that can approve an account
/// for AutoScrub.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct ScrubAccountConsent {
    approved: BTreeSet<String>,
}

impl ScrubAccountConsent {
    /// Nothing approved yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// The normal Scrub consent step. Mirrors Task 1401: a ticked account has
    /// to be one of the available accounts, ids and ticks have to be unique,
    /// and only the ticked accounts are saved. The saved set *replaces* the
    /// previous one, because that is what the screen's Continue button means.
    pub fn save_scrub_account_permissions(
        &mut self,
        input: ScrubAccountPermissionInput,
    ) -> Result<ScrubAccountPermissionRead, String> {
        if input.available_account_ids.len() > MAX_SCRUB_ACCOUNTS
            || input.selected_account_ids.len() > MAX_SCRUB_ACCOUNTS
        {
            return Err("Scrub account permission list is too large".to_owned());
        }
        let mut available = BTreeSet::<String>::new();
        for account_id in &input.available_account_ids {
            if !is_scrub_account_id(account_id) {
                return Err("Scrub account id is invalid".to_owned());
            }
            if !available.insert(account_id.clone()) {
                return Err("Scrub available accounts must be unique".to_owned());
            }
        }
        let mut selected = BTreeSet::<String>::new();
        for account_id in &input.selected_account_ids {
            if !is_scrub_account_id(account_id) {
                return Err("Scrub account id is invalid".to_owned());
            }
            if !available.contains(account_id) {
                return Err("Scrub selected account is not available".to_owned());
            }
            if !selected.insert(account_id.clone()) {
                return Err("Scrub selected accounts must be unique".to_owned());
            }
        }

        self.approved = selected;
        Ok(self.read())
    }

    /// What the read command returns.
    pub fn read(&self) -> ScrubAccountPermissionRead {
        ScrubAccountPermissionRead {
            account_ids: self.approved_account_ids(),
        }
    }

    pub fn approved_account_ids(&self) -> Vec<String> {
        self.approved.iter().cloned().collect()
    }

    pub fn approved_count(&self) -> usize {
        self.approved.len()
    }

    /// Byte-exact. A near-miss id is a different account, not a spelling of
    /// this one.
    pub fn is_approved(&self, account_id: &str) -> bool {
        self.approved.contains(account_id)
    }
}

/// One signed-in account AutoScrub could cover, as the account list (Task 1400)
/// reports it.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubAccount {
    pub account_id: String,
    pub label: String,
}

impl AutoScrubAccount {
    pub fn new(account_id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            account_id: account_id.into(),
            label: label.into(),
        }
    }
}

/// A refused AutoScrub switch. Always names the command and the account it was
/// about, so a caller never has to guess which row went red.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubSwitchRefusal {
    pub command: String,
    pub account_id: String,
    pub reason: String,
    pub message: String,
}

impl AutoScrubSwitchRefusal {
    /// A refusal in this shape for a reason this module does not itself own.
    /// Task 1457's schedule surface uses it so a schedule refusal reads exactly
    /// like a switch refusal.
    pub fn new(
        command: impl Into<String>,
        account_id: impl Into<String>,
        reason: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            command: command.into(),
            account_id: account_id.into(),
            reason: reason.into(),
            message: message.into(),
        }
    }

    pub(crate) fn pro_code_required(command: &str, account_id: &str) -> Self {
        Self {
            command: command.to_string(),
            account_id: account_id.to_string(),
            reason: PRO_CODE_REQUIRED.to_string(),
            message: "AutoScrub account switches need an active Pro code. Enter an active Pro \
                      code to unlock AutoScrub. Nothing was changed."
                .to_string(),
        }
    }

    fn account_not_approved(command: &str, account_id: &str) -> Self {
        Self {
            command: command.to_string(),
            account_id: account_id.to_string(),
            reason: ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB.to_string(),
            message: format!(
                "{account_id} is not approved for AutoScrub. Approve this account in the Scrub \
                 account list first. Nothing was changed."
            ),
        }
    }

    pub(crate) fn unknown_account(command: &str, account_id: &str) -> Self {
        Self {
            command: command.to_string(),
            account_id: account_id.to_string(),
            reason: UNKNOWN_ACCOUNT.to_string(),
            message: format!(
                "{account_id} is not a signed-in account AutoScrub can cover. Nothing was changed."
            ),
        }
    }
}

impl std::fmt::Display for AutoScrubSwitchRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{} refused: {}", self.command, self.message)
    }
}

impl std::error::Error for AutoScrubSwitchRefusal {}

/// The single place that decides whether an account may be named by anything
/// AutoScrub does. Schedules and runs reuse this, so a new account cannot slip
/// in by being named somewhere other than the switch.
pub fn require_account_approved_for_autoscrub(
    consent: &ScrubAccountConsent,
    command: &str,
    account_id: &str,
) -> Result<(), AutoScrubSwitchRefusal> {
    if consent.is_approved(account_id) {
        return Ok(());
    }
    Err(AutoScrubSwitchRefusal::account_not_approved(
        command, account_id,
    ))
}

/// One row of the AutoScrub account switch listing.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubAccountSwitchRow {
    pub account_id: String,
    pub label: String,
    pub available: bool,
    pub on: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal: Option<AutoScrubSwitchRefusal>,
}

/// One saved AutoScrub record: this approved account is switched on.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubAccountRecord {
    pub account_id: String,
    pub label: String,
    pub on: bool,
}

/// What a accepted switch returns: the account it moved and the whole record
/// set afterwards, so a caller can count without a second read.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubAccountSwitchOutcome {
    pub account_id: String,
    pub on: bool,
    pub records: Vec<AutoScrubAccountRecord>,
    pub record_count: usize,
}

/// Direct-invoke request body for [`AUTOSCRUB_ACCOUNT_SWITCH_COMMAND`].
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubAccountSwitchRequest {
    #[serde(default)]
    pub account_id: String,
    #[serde(default)]
    pub on: bool,
}

/// The AutoScrub account-switch surface: the Pro gate, the normal Scrub consent
/// it reads approvals from, the signed-in accounts, and the saved on-records.
#[derive(Debug, Clone)]
pub struct AutoScrubAccountSwitchSurface {
    pro: AutoScrubProSurface,
    consent: ScrubAccountConsent,
    accounts: Vec<AutoScrubAccount>,
    records: Vec<AutoScrubAccountRecord>,
}

impl AutoScrubAccountSwitchSurface {
    /// A fresh surface: nothing approved, so every switch is shut.
    pub fn new(pro: AutoScrubProSurface, accounts: Vec<AutoScrubAccount>) -> Self {
        Self {
            pro,
            consent: ScrubAccountConsent::new(),
            accounts,
            records: Vec::new(),
        }
    }

    pub fn pro(&self) -> &AutoScrubProSurface {
        &self.pro
    }

    pub fn pro_mut(&mut self) -> &mut AutoScrubProSurface {
        &mut self.pro
    }

    pub fn scrub_consent(&self) -> &ScrubAccountConsent {
        &self.consent
    }

    pub fn accounts(&self) -> &[AutoScrubAccount] {
        &self.accounts
    }

    /// Run the normal Scrub consent step. Approving an account is the only way
    /// to make its AutoScrub switch available; un-approving one drops any
    /// AutoScrub record it had, so a switch never outlives its consent.
    pub fn save_scrub_account_permissions(
        &mut self,
        input: ScrubAccountPermissionInput,
    ) -> Result<ScrubAccountPermissionRead, String> {
        let read = self.consent.save_scrub_account_permissions(input)?;
        let consent = &self.consent;
        self.records
            .retain(|record| consent.is_approved(&record.account_id));
        Ok(read)
    }

    fn label_for(&self, account_id: &str) -> String {
        self.accounts
            .iter()
            .find(|account| account.account_id == account_id)
            .map(|account| account.label.clone())
            .unwrap_or_else(|| account_id.to_string())
    }

    fn is_known(&self, account_id: &str) -> bool {
        self.accounts
            .iter()
            .any(|account| account.account_id == account_id)
    }

    fn is_on(&self, account_id: &str) -> bool {
        self.records
            .iter()
            .any(|record| record.account_id == account_id)
    }

    /// The check every switch move runs, in the order the product means it:
    /// Pro first (AutoScrub is the paid half), then the normal Scrub approval,
    /// then "is this even an account we know".
    fn require_switchable(
        &self,
        command: &str,
        account_id: &str,
    ) -> Result<(), AutoScrubSwitchRefusal> {
        if !self.pro.pro_unlocked() {
            return Err(AutoScrubSwitchRefusal::pro_code_required(
                command, account_id,
            ));
        }
        require_account_approved_for_autoscrub(&self.consent, command, account_id)?;
        if !self.is_known(account_id) {
            return Err(AutoScrubSwitchRefusal::unknown_account(command, account_id));
        }
        Ok(())
    }

    /// The listing a screen renders. A row that cannot be moved carries the
    /// same refusal the command itself would return, so the screen and the
    /// command always agree on the reason.
    pub fn switches(&self) -> Vec<AutoScrubAccountSwitchRow> {
        self.accounts
            .iter()
            .map(|account| {
                let refusal = self
                    .require_switchable(AUTOSCRUB_ACCOUNT_SWITCH_COMMAND, &account.account_id)
                    .err();
                AutoScrubAccountSwitchRow {
                    account_id: account.account_id.clone(),
                    label: account.label.clone(),
                    available: refusal.is_none(),
                    on: self.is_on(&account.account_id),
                    refusal,
                }
            })
            .collect()
    }

    /// The row for one account, if AutoScrub knows that account at all.
    pub fn switch_for(&self, account_id: &str) -> Option<AutoScrubAccountSwitchRow> {
        self.switches()
            .into_iter()
            .find(|row| row.account_id == account_id)
    }

    /// `true` iff this account's switch can be moved right now.
    pub fn switch_available(&self, account_id: &str) -> bool {
        self.require_switchable(AUTOSCRUB_ACCOUNT_SWITCH_COMMAND, account_id)
            .is_ok()
    }

    /// Turn one account's AutoScrub switch on or off. Refused before anything
    /// is written, so a refused call leaves the record set byte-for-byte as it
    /// was.
    pub fn set_switch(
        &mut self,
        account_id: &str,
        on: bool,
    ) -> Result<AutoScrubAccountSwitchOutcome, AutoScrubSwitchRefusal> {
        self.require_switchable(AUTOSCRUB_ACCOUNT_SWITCH_COMMAND, account_id)?;
        if on {
            if !self.is_on(account_id) {
                self.records.push(AutoScrubAccountRecord {
                    account_id: account_id.to_string(),
                    label: self.label_for(account_id),
                    on: true,
                });
            }
        } else {
            self.records
                .retain(|record| record.account_id != account_id);
        }
        Ok(AutoScrubAccountSwitchOutcome {
            account_id: account_id.to_string(),
            on,
            records: self.records.clone(),
            record_count: self.records.len(),
        })
    }

    /// Every saved on-record.
    pub fn records(&self) -> &[AutoScrubAccountRecord] {
        &self.records
    }

    pub fn record_count(&self) -> usize {
        self.records.len()
    }

    pub fn switched_on_account_ids(&self) -> Vec<String> {
        self.records
            .iter()
            .map(|record| record.account_id.clone())
            .collect()
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

/// The direct-invoke surface: what a caller reaches when it skips the switch
/// listing and names the command itself. It runs the same check the listing
/// reports, so hiding or deleting a greyed-out row does not open the switch.
/// Every reply is a JSON object carrying `ok`, so a refusal is never mistaken
/// for a transport failure.
pub fn run_autoscrub_account_switch_command(
    surface: &mut AutoScrubAccountSwitchSurface,
    command: &str,
    request_json: &str,
) -> String {
    match command {
        AUTOSCRUB_ACCOUNT_SWITCHES_COMMAND => ok_reply(
            command,
            json!({
                "switches": surface.switches(),
                "recordCount": surface.record_count(),
            }),
        ),
        AUTOSCRUB_ACCOUNT_SWITCH_COMMAND => {
            match serde_json::from_str::<AutoScrubAccountSwitchRequest>(request_json) {
                Ok(request) => match surface.set_switch(&request.account_id, request.on) {
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
            "error": format!("{other} is not an AutoScrub account switch command."),
        })
        .to_string(),
    }
}

/// The Task 1455 fixture accounts: two Discord accounts with near-miss ids and
/// one Telegram account, so "only that approval" has real siblings to be told
/// apart from. `discord-maple` and `discord-pine` are the pair Task 1457's
/// bypass check reuses.
pub fn fixture_autoscrub_accounts() -> Vec<AutoScrubAccount> {
    vec![
        AutoScrubAccount::new("discord-maple", "Discord app"),
        AutoScrubAccount::new("discord-pine", "Discord app"),
        AutoScrubAccount::new("telegram-pine", "Telegram app"),
    ]
}

/// Ids the fixture accounts are offered under in the normal Scrub account list.
pub fn fixture_available_account_ids() -> Vec<String> {
    fixture_autoscrub_accounts()
        .into_iter()
        .map(|account| account.account_id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autoscrub_pro_gate::{fixture_pro_code_directory, FIXTURE_ACTIVE_PRO_CODE};

    fn unlocked_surface() -> AutoScrubAccountSwitchSurface {
        let mut pro = AutoScrubProSurface::new(fixture_pro_code_directory());
        assert!(pro.present_pro_code(FIXTURE_ACTIVE_PRO_CODE).unlocks());
        AutoScrubAccountSwitchSurface::new(pro, fixture_autoscrub_accounts())
    }

    fn approve(surface: &mut AutoScrubAccountSwitchSurface, selected: &[&str]) {
        let available = fixture_available_account_ids();
        let available: Vec<&str> = available.iter().map(String::as_str).collect();
        surface
            .save_scrub_account_permissions(ScrubAccountPermissionInput::new(&available, selected))
            .expect("consent save");
    }

    #[test]
    fn a_switch_is_shut_before_the_account_is_approved() {
        let surface = unlocked_surface();
        let row = surface.switch_for("discord-maple").expect("row");
        assert!(!row.available);
        assert_eq!(
            row.refusal.expect("refusal").reason,
            ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB
        );
        assert_eq!(surface.record_count(), 0);
    }

    #[test]
    fn a_direct_invoke_for_an_unapproved_account_is_refused() {
        let mut surface = unlocked_surface();
        let reply = run_autoscrub_account_switch_command(
            &mut surface,
            AUTOSCRUB_ACCOUNT_SWITCH_COMMAND,
            r#"{"accountId":"discord-maple","on":true}"#,
        );
        let reply: serde_json::Value = serde_json::from_str(&reply).expect("json");
        assert_eq!(reply["ok"], serde_json::Value::Bool(false));
        assert_eq!(reply["errorCode"], ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB);
        assert_eq!(surface.record_count(), 0);
    }

    #[test]
    fn the_exact_approval_saves_exactly_one_record() {
        let mut surface = unlocked_surface();
        approve(&mut surface, &["discord-maple"]);
        let outcome = surface.set_switch("discord-maple", true).expect("switch");
        assert_eq!(outcome.record_count, 1);
        assert_eq!(surface.record_count(), 1);
        assert_eq!(surface.switched_on_account_ids(), vec!["discord-maple"]);
    }

    #[test]
    fn a_sibling_approval_does_not_open_this_switch() {
        let mut surface = unlocked_surface();
        approve(&mut surface, &["discord-pine"]);
        assert!(!surface.switch_available("discord-maple"));
        assert!(surface.set_switch("discord-maple", true).is_err());
        assert_eq!(surface.record_count(), 0);
    }

    #[test]
    fn being_available_but_unticked_is_not_approval() {
        let mut surface = unlocked_surface();
        approve(&mut surface, &[]);
        assert!(!surface.switch_available("discord-maple"));
        assert_eq!(surface.record_count(), 0);
    }

    #[test]
    fn withdrawing_consent_drops_the_record() {
        let mut surface = unlocked_surface();
        approve(&mut surface, &["discord-maple"]);
        surface.set_switch("discord-maple", true).expect("switch");
        assert_eq!(surface.record_count(), 1);
        approve(&mut surface, &[]);
        assert_eq!(surface.record_count(), 0);
        assert!(!surface.switch_available("discord-maple"));
    }

    #[test]
    fn turning_the_switch_off_leaves_no_record() {
        let mut surface = unlocked_surface();
        approve(&mut surface, &["discord-maple"]);
        surface.set_switch("discord-maple", true).expect("on");
        let outcome = surface.set_switch("discord-maple", false).expect("off");
        assert_eq!(outcome.record_count, 0);
        assert_eq!(surface.record_count(), 0);
    }

    #[test]
    fn a_locked_pro_gate_still_shuts_every_switch() {
        let mut surface = AutoScrubAccountSwitchSurface::new(
            AutoScrubProSurface::new(fixture_pro_code_directory()),
            fixture_autoscrub_accounts(),
        );
        approve(&mut surface, &["discord-maple"]);
        let row = surface.switch_for("discord-maple").expect("row");
        assert!(!row.available);
        assert_eq!(row.refusal.expect("refusal").reason, PRO_CODE_REQUIRED);
        assert!(surface.set_switch("discord-maple", true).is_err());
        assert_eq!(surface.record_count(), 0);
    }

    #[test]
    fn an_invalid_account_id_never_becomes_an_approval() {
        let mut consent = ScrubAccountConsent::new();
        let error = consent
            .save_scrub_account_permissions(ScrubAccountPermissionInput::new(
                &["DISCORD-MAPLE"],
                &["DISCORD-MAPLE"],
            ))
            .expect_err("uppercase id must be refused");
        assert_eq!(error, "Scrub account id is invalid");
        assert_eq!(consent.approved_count(), 0);
    }

    #[test]
    fn an_unticked_account_cannot_be_selected_from_outside_the_list() {
        let mut consent = ScrubAccountConsent::new();
        let error = consent
            .save_scrub_account_permissions(ScrubAccountPermissionInput::new(
                &["discord-pine"],
                &["discord-maple"],
            ))
            .expect_err("selecting an unavailable account must be refused");
        assert_eq!(error, "Scrub selected account is not available");
        assert_eq!(consent.approved_count(), 0);
    }
}
