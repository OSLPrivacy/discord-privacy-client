//! Durable, exact-scope agreement for AutoScrub's "Find and delete" mode.
//!
//! The agreement is not a generic consent bit. It records the accounts and
//! the exact saved rule snapshots the person reviewed, plus each risk shown
//! beside the tick. A later Find-and-delete request must name that same scope
//! and the rules must still have the same values. Changing an account, a rule,
//! or a private word therefore requires a new agreement.

use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use crate::autoscrub_account_switches::{is_scrub_account_id, MAX_SCRUB_ACCOUNTS};
use crate::bad_message_rules::{parse_bad_message_rule_name, BadMessageRule};

pub const AUTOSCRUB_DELETION_AGREEMENT_VERSION: u32 = 1;
pub const DELETION_AGREEMENT_REQUIRED: &str = "OSL: deletion agreement required";
const MAX_SELECTED_RULES: usize = 32;

/// Everything displayed beside the one explicit deletion tick.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutoScrubDeletionAgreementRequest {
    #[serde(default)]
    pub selected_account_ids: Vec<String>,
    #[serde(default)]
    pub selected_rule_names: Vec<String>,
    #[serde(default)]
    pub deleted_messages_can_be_permanent: bool,
    #[serde(default)]
    pub service_rules_may_forbid_automated_reading_or_deletion: bool,
    #[serde(default)]
    pub suspension_or_ban_risk_is_real: bool,
    #[serde(default)]
    pub allow_autoscrub_to_delete_matching_messages: bool,
}

/// The durable record. Rule values are snapshots, not names alone, so editing
/// a private word cannot silently reuse consent given to the old word.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutoScrubDeletionAgreement {
    pub version: u32,
    pub selected_account_ids: Vec<String>,
    pub selected_rules: Vec<BadMessageRule>,
    pub deleted_messages_can_be_permanent: bool,
    pub service_rules_may_forbid_automated_reading_or_deletion: bool,
    pub suspension_or_ban_risk_is_real: bool,
    pub allow_autoscrub_to_delete_matching_messages: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubFindAndDeleteAuthorization {
    pub mode: &'static str,
    pub selected_account_ids: Vec<String>,
    pub selected_rules: Vec<BadMessageRule>,
}

/// Validate and canonicalize a deletion agreement against AutoScrub's saved
/// Task-1459 rules. This function has no side effects, so a refused replacement
/// cannot erase or weaken the previously saved agreement.
pub fn build_deletion_agreement(
    request: AutoScrubDeletionAgreementRequest,
    saved_rules: &HashMap<String, BadMessageRule>,
) -> Result<AutoScrubDeletionAgreement, String> {
    if !request.deleted_messages_can_be_permanent
        || !request.service_rules_may_forbid_automated_reading_or_deletion
        || !request.suspension_or_ban_risk_is_real
        || !request.allow_autoscrub_to_delete_matching_messages
    {
        return Err(DELETION_AGREEMENT_REQUIRED.to_owned());
    }

    let selected_account_ids = canonical_accounts(&request.selected_account_ids)?;
    let selected_rules = canonical_rules(&request.selected_rule_names, saved_rules)?;

    Ok(AutoScrubDeletionAgreement {
        version: AUTOSCRUB_DELETION_AGREEMENT_VERSION,
        selected_account_ids,
        selected_rules,
        deleted_messages_can_be_permanent: true,
        service_rules_may_forbid_automated_reading_or_deletion: true,
        suspension_or_ban_risk_is_real: true,
        allow_autoscrub_to_delete_matching_messages: true,
    })
}

/// The load-bearing Find-and-delete gate.
pub fn authorize_find_and_delete(
    agreement: Option<&AutoScrubDeletionAgreement>,
    selected_account_ids: &[String],
    selected_rule_names: &[String],
    saved_rules: &HashMap<String, BadMessageRule>,
) -> Result<AutoScrubFindAndDeleteAuthorization, String> {
    let agreement = agreement.ok_or_else(|| DELETION_AGREEMENT_REQUIRED.to_owned())?;
    let accounts = canonical_accounts(selected_account_ids)
        .map_err(|_| DELETION_AGREEMENT_REQUIRED.to_owned())?;
    let rules = canonical_rules(selected_rule_names, saved_rules)
        .map_err(|_| DELETION_AGREEMENT_REQUIRED.to_owned())?;

    if agreement.version != AUTOSCRUB_DELETION_AGREEMENT_VERSION
        || !agreement.deleted_messages_can_be_permanent
        || !agreement.service_rules_may_forbid_automated_reading_or_deletion
        || !agreement.suspension_or_ban_risk_is_real
        || !agreement.allow_autoscrub_to_delete_matching_messages
        || agreement.selected_account_ids != accounts
        || agreement.selected_rules != rules
    {
        return Err(DELETION_AGREEMENT_REQUIRED.to_owned());
    }

    Ok(AutoScrubFindAndDeleteAuthorization {
        mode: "find_and_delete",
        selected_account_ids: accounts,
        selected_rules: rules,
    })
}

fn canonical_accounts(account_ids: &[String]) -> Result<Vec<String>, String> {
    if account_ids.is_empty() || account_ids.len() > MAX_SCRUB_ACCOUNTS {
        return Err("OSL: AutoScrub deletion agreement needs 1..=32 selected accounts".to_owned());
    }
    let mut accounts = BTreeSet::new();
    for account_id in account_ids {
        if !is_scrub_account_id(account_id) {
            return Err("OSL: AutoScrub deletion agreement account is invalid".to_owned());
        }
        if !accounts.insert(account_id.clone()) {
            return Err("OSL: AutoScrub deletion agreement accounts must be unique".to_owned());
        }
    }
    Ok(accounts.into_iter().collect())
}

fn canonical_rules(
    rule_names: &[String],
    saved_rules: &HashMap<String, BadMessageRule>,
) -> Result<Vec<BadMessageRule>, String> {
    if rule_names.is_empty() || rule_names.len() > MAX_SELECTED_RULES {
        return Err("OSL: AutoScrub deletion agreement needs 1..=32 selected rules".to_owned());
    }
    let mut names = BTreeSet::new();
    for raw_name in rule_names {
        let name = parse_bad_message_rule_name(raw_name)?.name().to_owned();
        if !names.insert(name) {
            return Err("OSL: AutoScrub deletion agreement rules must be unique".to_owned());
        }
    }
    names
        .into_iter()
        .map(|name| {
            saved_rules.get(&name).cloned().ok_or_else(|| {
                format!("OSL: AutoScrub deletion agreement rule '{name}' is not saved")
            })
        })
        .collect()
}
