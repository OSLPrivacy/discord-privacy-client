//! TASK 4858 fixture: one role, one channel, all 40 catalogue rows.
//!
//! The story the numbers come from. `WATCH_CAPTAIN` is a custom role in the
//! enclave `Harbour Watch`, looked at inside the channel `#ops-vault`, for a
//! member who:
//!
//! * holds the channel key but not the sealed receipt, burn, rotation or
//!   security-event keys — 5 rows can never work for them,
//! * sits inside an active relay timeout on reactions and voice — 3 rows,
//! * is in a channel that overrides four rows to deny — 4 rows,
//! * has a role whose slow mode, longest mute and hourly budget are all lower
//!   than what three metered actions ask for — 3 rows,
//! * has 8 boxes nobody ever ticked,
//!
//! leaving 17 rows the role can really use. `#ops-vault` also carries one
//! channel allow (`view_audit_log`) the role does not tick at the enclave
//! level, so the fixture exercises the allow side of channel overrides too.

use std::collections::{BTreeMap, BTreeSet};

use crate::osl_enclave_role_ability::{
    build_enclave_role_ability_view, EnclaveRoleAbilityInput, EnclaveRoleAbilityView,
};
use crate::osl_enclave_roles::{
    all_persisted_permission_names, new_enclave_role_catalog, EnclaveRelayActionBudget,
    EnclaveRelayActionMetadata, EnclaveRoleCatalog, EnclaveRoleRelayLimits,
};

pub const FIXTURE_ROLE_NAME: &str = "WATCH_CAPTAIN";
pub const FIXTURE_CHANNEL_NAME: &str = "ops-vault";
pub const FIXTURE_NOW_UNIX_SECONDS: u64 = 1_770_000_000;

/// The five sealed key domains this member's key store has no key for.
pub const MISSING_KEY_PERMISSIONS: [&str; 5] = [
    "export_receipts",
    "queue_burn",
    "approve_burn",
    "rotate_keys",
    "view_security_events",
];

/// The four rows `#ops-vault` overrides to deny.
pub const CHANNEL_DENY_PERMISSIONS: [&str; 4] = [
    "create_threads",
    "attach_files",
    "publish_announcements",
    "manage_stickers",
];

/// The one row `#ops-vault` overrides to allow, which the role does not tick.
pub const CHANNEL_ALLOW_PERMISSIONS: [&str; 1] = ["view_audit_log"];

/// The rows the relay is refusing while this member's timeout runs.
pub const RELAY_TIMEOUT_PERMISSIONS: [&str; 3] = ["react_messages", "screen_share", "start_voice"];

pub const RELAY_TIMEOUT_REMAINING_SECONDS: u64 = 420;

/// The eight boxes nobody ticked for `WATCH_CAPTAIN`.
pub const UNTICKED_PERMISSIONS: [&str; 8] = [
    "record_meetings",
    "delete_channels",
    "set_retention",
    "manage_webhooks",
    "manage_integrations",
    "manage_roles",
    "manage_presence",
    "close_polls",
];

/// The role's own limits, saved by TASK 4857.
pub const FIXTURE_SLOW_MODE_SECONDS: u64 = 17;
pub const FIXTURE_LONGEST_MUTE_SECONDS: u64 = 3_600;
pub const FIXTURE_ACTIONS_PER_HOUR_BUDGET: usize = 24;

/// What the three metered rows ask the relay for.
pub const FIXTURE_REQUESTED_MUTE_SECONDS: u64 = 86_400;
pub const FIXTURE_LAST_MESSAGE_SECONDS_AGO: u64 = 5;

pub const EXPECTED_ROW_COUNT: usize = 40;
pub const EXPECTED_ALLOWED_COUNT: usize = 17;
pub const EXPECTED_DENIED_COUNT: usize = 23;

fn names(values: &[&str]) -> BTreeSet<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

/// `WATCH_CAPTAIN` ticks everything except the eight unticked rows and
/// `view_audit_log`, which it reaches only through the channel allow.
pub fn fixture_role_catalog() -> EnclaveRoleCatalog {
    let mut catalog = new_enclave_role_catalog();
    let mut ticked = all_persisted_permission_names();
    for unticked in UNTICKED_PERMISSIONS {
        ticked.remove(unticked);
    }
    for channel_allowed in CHANNEL_ALLOW_PERMISSIONS {
        ticked.remove(channel_allowed);
    }
    catalog
        .add_custom_role(FIXTURE_ROLE_NAME, ticked)
        .expect("the fixture role name is free and its permissions are declared");
    catalog
}

pub fn fixture_role_limits() -> BTreeMap<String, EnclaveRoleRelayLimits> {
    BTreeMap::from([(
        FIXTURE_ROLE_NAME.to_owned(),
        EnclaveRoleRelayLimits {
            slow_mode_seconds: FIXTURE_SLOW_MODE_SECONDS,
            longest_mute_seconds: FIXTURE_LONGEST_MUTE_SECONDS,
            actions_per_hour_budget: FIXTURE_ACTIONS_PER_HOUR_BUDGET,
        },
    )])
}

/// The relay-metered action behind each of the three limited rows.
pub fn fixture_limited_actions() -> BTreeMap<String, EnclaveRelayActionMetadata> {
    BTreeMap::from([
        (
            "send_messages".to_owned(),
            EnclaveRelayActionMetadata::SendMessage,
        ),
        (
            "mute_members".to_owned(),
            EnclaveRelayActionMetadata::CreateMute {
                duration_seconds: FIXTURE_REQUESTED_MUTE_SECONDS,
            },
        ),
        (
            "invite_members".to_owned(),
            EnclaveRelayActionMetadata::GovernanceAction,
        ),
    ])
}

/// The role has already spent its whole hourly budget.
pub fn fixture_relay_action_budget() -> EnclaveRelayActionBudget {
    let mut budget = EnclaveRelayActionBudget::default();
    for spent in 0..FIXTURE_ACTIONS_PER_HOUR_BUDGET {
        // Oldest first, all inside the trailing hour.
        budget.record_spent_action(
            FIXTURE_ROLE_NAME,
            FIXTURE_NOW_UNIX_SECONDS - 3_000 + (spent as u64 * 100),
        );
    }
    budget
}

pub fn fixture_ability_view() -> EnclaveRoleAbilityView {
    let catalog = fixture_role_catalog();
    let missing_keys = names(&MISSING_KEY_PERMISSIONS);
    let relay_timeouts = names(&RELAY_TIMEOUT_PERMISSIONS);
    let channel_allow = names(&CHANNEL_ALLOW_PERMISSIONS);
    let channel_deny = names(&CHANNEL_DENY_PERMISSIONS);
    let limited_actions = fixture_limited_actions();
    let role_limits = fixture_role_limits();
    let budget = fixture_relay_action_budget();

    build_enclave_role_ability_view(EnclaveRoleAbilityInput {
        role_name: FIXTURE_ROLE_NAME,
        role_catalog: &catalog,
        channel_name: FIXTURE_CHANNEL_NAME,
        missing_key_permission_names: &missing_keys,
        relay_timeout_permission_names: &relay_timeouts,
        relay_timeout_remaining_seconds: RELAY_TIMEOUT_REMAINING_SECONDS,
        channel_allow_permission_names: &channel_allow,
        channel_deny_permission_names: &channel_deny,
        limited_actions: &limited_actions,
        role_limits: &role_limits,
        relay_action_budget: &budget,
        now_unix_seconds: FIXTURE_NOW_UNIX_SECONDS,
        last_message_unix_seconds: Some(
            FIXTURE_NOW_UNIX_SECONDS - FIXTURE_LAST_MESSAGE_SECONDS_AGO,
        ),
    })
    .expect("the ability view has a plain reason for every denied fixture row")
}
