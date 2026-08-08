//! TASK 4858: the "show me what this role can actually do" view.
//!
//! This is the resolution half of the ability screen. It walks the whole
//! enclave permission catalogue for one role in one channel and answers, per
//! row, whether the role can really do the thing right now — after channel
//! overrides, key possession, relay timeouts and the role's own limits.
//!
//! It owns no precedence rules of its own. Every allow/deny answer comes from
//! the single TASK 4860 resolver (`resolve_enclave_permission`) and the relay
//! limit gate (`resolve_enclave_relay_limits` /
//! `EnclaveRelayActionBudget`); this module only turns the rule that decided a
//! row into a plain reason a person can read. A denied row without a reason is
//! a bug, so the builder refuses to produce one.

use std::collections::{BTreeMap, BTreeSet};

use super::osl_enclave_roles::{
    enclave_permissions, resolve_enclave_permission, resolve_enclave_relay_limits,
    EnclavePermissionDecisionRule, EnclavePermissionResolveInput, EnclaveRelayActionBudget,
    EnclaveRelayActionMetadata, EnclaveRelayLimitInput, EnclaveRelayLimitKind, EnclaveRoleCatalog,
    EnclaveRoleRelayLimits,
};

/// The role's member does not hold the sealed key this permission is bound to,
/// so nothing the enclave can say would make the row work.
pub const REASON_NO_CHANNEL_KEY: &str = "no channel key";
/// This channel carries an explicit deny override for the row.
pub const REASON_CHANNEL_OVERRIDE_DENY: &str = "channel override says deny";
/// OSL's relay is refusing the action for this member while a timeout runs.
pub const REASON_RELAY_TIMEOUT_ACTIVE: &str = "relay timeout is active";
/// The row is granted, but the role's own configured limit is below what the
/// action asks for.
pub const REASON_ROLE_LIMIT_LOWER: &str = "role limit is lower";
/// Nobody ticked the box.
pub const REASON_PERMISSION_OFF: &str = "permission is off";

/// Every reason the ability view is allowed to print for a denied row.
pub const ENCLAVE_ROLE_ABILITY_REASONS: [&str; 5] = [
    REASON_NO_CHANNEL_KEY,
    REASON_CHANNEL_OVERRIDE_DENY,
    REASON_RELAY_TIMEOUT_ACTIVE,
    REASON_ROLE_LIMIT_LOWER,
    REASON_PERMISSION_OFF,
];

#[derive(Debug, Clone)]
pub struct EnclaveRoleAbilityInput<'a> {
    pub role_name: &'a str,
    pub role_catalog: &'a EnclaveRoleCatalog,
    pub channel_name: &'a str,
    /// Permissions whose sealed key this member's key store does not hold.
    pub missing_key_permission_names: &'a BTreeSet<String>,
    /// Permissions OSL's relay is currently refusing for this member.
    pub relay_timeout_permission_names: &'a BTreeSet<String>,
    pub relay_timeout_remaining_seconds: u64,
    pub channel_allow_permission_names: &'a BTreeSet<String>,
    pub channel_deny_permission_names: &'a BTreeSet<String>,
    /// The relay-metered action a permission stands for, when it has one. A
    /// permission with no entry here is not subject to the role's limits.
    pub limited_actions: &'a BTreeMap<String, EnclaveRelayActionMetadata>,
    pub role_limits: &'a BTreeMap<String, EnclaveRoleRelayLimits>,
    /// Actions this role has already spent in the current hour. The view asks
    /// a clone of this budget, so previewing what a role can do never spends
    /// any of it.
    pub relay_action_budget: &'a EnclaveRelayActionBudget,
    pub now_unix_seconds: u64,
    pub last_message_unix_seconds: Option<u64>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EnclaveRoleAbilityRow {
    pub permission_name: &'static str,
    pub label: &'static str,
    pub allowed: bool,
    /// The rule that decided the row, in the TASK 4860 vocabulary.
    pub decided_by: &'static str,
    /// Exactly one of [`ENCLAVE_ROLE_ABILITY_REASONS`] on a denied row, and
    /// `None` on an allowed one.
    pub reason: Option<&'static str>,
    /// The specific numbers behind the reason, for the second line of the row.
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EnclaveRoleAbilityView {
    pub role_name: String,
    pub channel_name: String,
    pub rows: Vec<EnclaveRoleAbilityRow>,
}

pub fn build_enclave_role_ability_view(
    input: EnclaveRoleAbilityInput<'_>,
) -> Result<EnclaveRoleAbilityView, String> {
    let mut rows = Vec::with_capacity(enclave_permissions().len());
    for permission in enclave_permissions() {
        let permission_name = permission.persisted_name;
        let resolution = resolve_enclave_permission(EnclavePermissionResolveInput {
            permission_name,
            key_present: !input.missing_key_permission_names.contains(permission_name),
            relay_sanctioned: input
                .relay_timeout_permission_names
                .contains(permission_name),
            channel_allow_permission_names: input.channel_allow_permission_names,
            channel_deny_permission_names: input.channel_deny_permission_names,
            role_name: input.role_name,
            role_catalog: input.role_catalog,
        });

        if !resolution.allowed() {
            let (reason, detail) = explain_denial(&input, permission_name, resolution.rule)?;
            rows.push(EnclaveRoleAbilityRow {
                permission_name,
                label: permission.label,
                allowed: false,
                decided_by: resolution.decided_by(),
                reason: Some(reason),
                detail: Some(detail),
            });
            continue;
        }

        // The row is granted. It can still be out of reach right now because
        // the role's own limit is below what the action asks for.
        match limit_denial(&input, permission_name) {
            Some((limit, detail)) => rows.push(EnclaveRoleAbilityRow {
                permission_name,
                label: permission.label,
                allowed: false,
                decided_by: limit_rule_name(limit),
                reason: Some(REASON_ROLE_LIMIT_LOWER),
                detail: Some(detail),
            }),
            None => rows.push(EnclaveRoleAbilityRow {
                permission_name,
                label: permission.label,
                allowed: true,
                decided_by: resolution.decided_by(),
                reason: None,
                detail: None,
            }),
        }
    }

    Ok(EnclaveRoleAbilityView {
        role_name: input.role_name.to_owned(),
        channel_name: input.channel_name.to_owned(),
        rows,
    })
}

fn explain_denial(
    input: &EnclaveRoleAbilityInput<'_>,
    permission_name: &str,
    rule: EnclavePermissionDecisionRule,
) -> Result<(&'static str, String), String> {
    match rule {
        EnclavePermissionDecisionRule::Key => Ok((
            REASON_NO_CHANNEL_KEY,
            format!(
                "This member's key store holds no key for {} in #{}, so the enclave cannot grant it.",
                permission_name, input.channel_name
            ),
        )),
        EnclavePermissionDecisionRule::Relay => Ok((
            REASON_RELAY_TIMEOUT_ACTIVE,
            format!(
                "OSL's relay is refusing this for another {} seconds. It still cannot read what you write.",
                input.relay_timeout_remaining_seconds
            ),
        )),
        EnclavePermissionDecisionRule::ChannelDeny => Ok((
            REASON_CHANNEL_OVERRIDE_DENY,
            format!(
                "#{} overrides this row to deny, and a channel deny beats every enclave grant.",
                input.channel_name
            ),
        )),
        EnclavePermissionDecisionRule::Off => Ok((
            REASON_PERMISSION_OFF,
            format!(
                "Nobody has ticked this box for {}, and no channel override turns it on.",
                input.role_name
            ),
        )),
        // The resolver only ever denies by the four rules above. If that ever
        // changes, the ability view must learn a new plain reason before it is
        // allowed to draw the row.
        EnclavePermissionDecisionRule::ChannelAllow | EnclavePermissionDecisionRule::RoleGrants => {
            Err(format!(
                "TASK4858: the resolver denied {permission_name} by a rule the ability view has no reason for"
            ))
        }
    }
}

fn limit_denial(
    input: &EnclaveRoleAbilityInput<'_>,
    permission_name: &str,
) -> Option<(EnclaveRelayLimitKind, String)> {
    let action = *input.limited_actions.get(permission_name)?;
    let limits = input.role_limits.get(input.role_name)?;

    let resolution = resolve_enclave_relay_limits(EnclaveRelayLimitInput {
        role_name: input.role_name,
        role_limits: input.role_limits,
        action,
        now_unix_seconds: input.now_unix_seconds,
        last_message_unix_seconds: input.last_message_unix_seconds,
    });
    if !resolution.allowed() {
        let limit = resolution
            .limit
            .expect("a denied relay limit resolution names its limit");
        return Some((limit, limit_detail(limit, limits, action, &resolution)));
    }

    // Every relay-metered action also spends the role's hourly budget. Ask a
    // clone so drawing the screen never spends a real action.
    let mut preview_budget = input.relay_action_budget.clone();
    let budget_resolution =
        preview_budget.check_and_record(input.role_name, input.role_limits, input.now_unix_seconds);
    if budget_resolution.allowed() {
        return None;
    }
    let used = input
        .relay_action_budget
        .recorded_action_count(input.role_name, input.now_unix_seconds);
    Some((
        EnclaveRelayLimitKind::ActionBudget,
        format!(
            "This role's budget is {} actions an hour and {} are already spent.",
            limits.actions_per_hour_budget, used
        ),
    ))
}

fn limit_detail(
    limit: EnclaveRelayLimitKind,
    limits: &EnclaveRoleRelayLimits,
    action: EnclaveRelayActionMetadata,
    resolution: &super::osl_enclave_roles::EnclaveRelayLimitResolution,
) -> String {
    match limit {
        EnclaveRelayLimitKind::SlowMode => format!(
            "This role's slow mode is {} seconds and {} of them are still to run.",
            limits.slow_mode_seconds,
            resolution.retry_after_seconds.unwrap_or_default()
        ),
        EnclaveRelayLimitKind::LongestMute => {
            let asked = match action {
                EnclaveRelayActionMetadata::CreateMute { duration_seconds } => duration_seconds,
                _ => 0,
            };
            format!(
                "This role's longest mute is {} seconds and this action asks for {}.",
                limits.longest_mute_seconds, asked
            )
        }
        EnclaveRelayLimitKind::ActionBudget => format!(
            "This role's budget is {} actions an hour and it is already spent.",
            limits.actions_per_hour_budget
        ),
    }
}

fn limit_rule_name(limit: EnclaveRelayLimitKind) -> &'static str {
    match limit {
        EnclaveRelayLimitKind::SlowMode => "ROLE_LIMIT_SLOW_MODE",
        EnclaveRelayLimitKind::LongestMute => "ROLE_LIMIT_LONGEST_MUTE",
        EnclaveRelayLimitKind::ActionBudget => "ROLE_LIMIT_ACTION_BUDGET",
    }
}

impl EnclaveRoleAbilityView {
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    pub fn allowed_count(&self) -> usize {
        self.rows.iter().filter(|row| row.allowed).count()
    }

    pub fn denied_count(&self) -> usize {
        self.rows.iter().filter(|row| !row.allowed).count()
    }

    /// How many denied rows carry each reason, keyed by the reason sentence.
    pub fn denied_reason_counts(&self) -> BTreeMap<&'static str, usize> {
        let mut counts: BTreeMap<&'static str, usize> = ENCLAVE_ROLE_ABILITY_REASONS
            .iter()
            .map(|reason| (*reason, 0))
            .collect();
        for row in self.rows.iter().filter(|row| !row.allowed) {
            if let Some(reason) = row.reason {
                *counts.entry(reason).or_default() += 1;
            }
        }
        counts
    }

    /// The permission names of denied rows with no reason on them. The screen
    /// must never draw one, so this is the check every caller runs.
    pub fn denied_rows_without_reason(&self) -> Vec<&'static str> {
        self.rows
            .iter()
            .filter(|row| !row.allowed)
            .filter(|row| {
                row.reason
                    .is_none_or(|reason| !ENCLAVE_ROLE_ABILITY_REASONS.contains(&reason))
                    || row
                        .detail
                        .as_ref()
                        .is_none_or(|detail| detail.trim().is_empty())
            })
            .map(|row| row.permission_name)
            .collect()
    }

    /// The rows as the JSON the ability screen renders. Written by hand so the
    /// role contract harness needs no serialization dependency.
    pub fn to_json(&self) -> String {
        let rows = self
            .rows
            .iter()
            .map(|row| {
                format!(
                    "    {{\"permission\": {}, \"label\": {}, \"state\": {}, \"decidedBy\": {}, \"reason\": {}, \"detail\": {}}}",
                    json_string(row.permission_name),
                    json_string(row.label),
                    json_string(if row.allowed { "allowed" } else { "denied" }),
                    json_string(row.decided_by),
                    json_string(row.reason.unwrap_or_default()),
                    json_string(row.detail.as_deref().unwrap_or_default()),
                )
            })
            .collect::<Vec<_>>()
            .join(",\n");
        format!(
            "{{\n  \"task\": \"4858\",\n  \"roleName\": {},\n  \"channelName\": {},\n  \"rows\": [\n{}\n  ]\n}}\n",
            json_string(&self.role_name),
            json_string(&self.channel_name),
            rows
        )
    }
}

fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if control.is_control() => {
                out.push_str(&format!("\\u{:04x}", control as u32));
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}
