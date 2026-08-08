use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub const OWNER_ROLE_NAME: &str = "OWNER";
pub const MOD_ROLE_NAME: &str = "MOD";
pub const MEMBER_ROLE_NAME: &str = "MEMBER";

const DEFAULT_ROLE_NAMES: [&str; 3] = [OWNER_ROLE_NAME, MOD_ROLE_NAME, MEMBER_ROLE_NAME];

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EnclavePermission {
    pub persisted_name: &'static str,
    pub label: &'static str,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EnclaveRoleRow {
    pub name: String,
    pub ticked_permission_names: BTreeSet<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EnclaveRoleCatalog {
    roles: Vec<EnclaveRoleRow>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EnclavePermissionMigrationReport {
    pub introduced_permission_names: BTreeSet<String>,
    pub existing_role_count: usize,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EnclavePermissionResolveInput<'a> {
    pub permission_name: &'a str,
    pub key_present: bool,
    pub relay_sanctioned: bool,
    pub channel_allow_permission_names: &'a BTreeSet<String>,
    pub channel_deny_permission_names: &'a BTreeSet<String>,
    pub role_name: &'a str,
    pub role_catalog: &'a EnclaveRoleCatalog,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum EnclavePermissionDecision {
    Allowed,
    Denied,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum EnclavePermissionDecisionRule {
    Key,
    Relay,
    ChannelDeny,
    ChannelAllow,
    RoleGrants,
    Off,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EnclavePermissionResolution {
    pub decision: EnclavePermissionDecision,
    pub rule: EnclavePermissionDecisionRule,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct EnclaveRoleRelayLimits {
    pub slow_mode_seconds: u64,
    pub longest_mute_seconds: u64,
    pub actions_per_hour_budget: usize,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum EnclaveRelayActionMetadata {
    SendMessage,
    CreateMute { duration_seconds: u64 },
    GovernanceAction,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EnclaveRelayLimitInput<'a> {
    pub role_name: &'a str,
    pub role_limits: &'a BTreeMap<String, EnclaveRoleRelayLimits>,
    pub action: EnclaveRelayActionMetadata,
    pub now_unix_seconds: u64,
    pub last_message_unix_seconds: Option<u64>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum EnclaveRelayLimitKind {
    SlowMode,
    LongestMute,
    ActionBudget,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EnclaveRelayLimitResolution {
    pub decision: EnclavePermissionDecision,
    pub rule: EnclavePermissionDecisionRule,
    pub limit: Option<EnclaveRelayLimitKind>,
    pub retry_after_seconds: Option<u64>,
}

#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct EnclaveRelayActionBudget {
    action_unix_seconds: BTreeMap<String, VecDeque<u64>>,
}

pub fn new_enclave_role_catalog() -> EnclaveRoleCatalog {
    new_enclave_role_catalog_from_template_values(
        all_persisted_permission_names(),
        mod_permission_names(),
        member_permission_names(),
    )
    .expect("built-in enclave role templates are valid")
}

pub fn new_enclave_role_catalog_from_template_values(
    all_permission_names: BTreeSet<String>,
    mod_default_permission_names: BTreeSet<String>,
    member_default_permission_names: BTreeSet<String>,
) -> Result<EnclaveRoleCatalog, String> {
    ensure_subset(
        &mod_default_permission_names,
        &all_permission_names,
        MOD_ROLE_NAME,
    )?;
    ensure_subset(
        &member_default_permission_names,
        &all_permission_names,
        MEMBER_ROLE_NAME,
    )?;
    Ok(EnclaveRoleCatalog {
        roles: vec![
            EnclaveRoleRow::new(OWNER_ROLE_NAME, all_permission_names),
            EnclaveRoleRow::new(MOD_ROLE_NAME, mod_default_permission_names),
            EnclaveRoleRow::new(MEMBER_ROLE_NAME, member_default_permission_names),
        ],
    })
}

pub fn enclave_permissions() -> &'static [EnclavePermission] {
    &ENCLAVE_PERMISSIONS
}

pub fn all_persisted_permission_names() -> BTreeSet<String> {
    ENCLAVE_PERMISSIONS
        .iter()
        .map(|permission| permission.persisted_name.to_owned())
        .collect()
}

pub fn persisted_permission_name_search(needles: &[&str]) -> Vec<&'static str> {
    ENCLAVE_PERMISSIONS
        .iter()
        .map(|permission| permission.persisted_name)
        .filter(|name| needles.iter().any(|needle| name == needle))
        .collect()
}

pub fn resolve_enclave_permission(
    input: EnclavePermissionResolveInput<'_>,
) -> EnclavePermissionResolution {
    if !input.key_present || !all_persisted_permission_names().contains(input.permission_name) {
        return EnclavePermissionResolution::denied(EnclavePermissionDecisionRule::Key);
    }
    if input.relay_sanctioned {
        return EnclavePermissionResolution::denied(EnclavePermissionDecisionRule::Relay);
    }

    if input
        .channel_deny_permission_names
        .contains(input.permission_name)
    {
        return EnclavePermissionResolution::denied(EnclavePermissionDecisionRule::ChannelDeny);
    }
    if input
        .channel_allow_permission_names
        .contains(input.permission_name)
    {
        return EnclavePermissionResolution::allow_by(EnclavePermissionDecisionRule::ChannelAllow);
    }

    if input
        .role_catalog
        .role(input.role_name)
        .is_some_and(|role| role.permission_is_ticked(input.permission_name))
    {
        return EnclavePermissionResolution::allow_by(EnclavePermissionDecisionRule::RoleGrants);
    }

    EnclavePermissionResolution::denied(EnclavePermissionDecisionRule::Off)
}

pub fn resolve_enclave_relay_limits(
    input: EnclaveRelayLimitInput<'_>,
) -> EnclaveRelayLimitResolution {
    let Some(limits) = input.role_limits.get(input.role_name) else {
        return EnclaveRelayLimitResolution::denied(EnclaveRelayLimitKind::ActionBudget, None);
    };

    match input.action {
        EnclaveRelayActionMetadata::SendMessage => {
            if let Some(last_message) = input.last_message_unix_seconds {
                let next_allowed = last_message.saturating_add(limits.slow_mode_seconds);
                if limits.slow_mode_seconds > 0 && input.now_unix_seconds < next_allowed {
                    return EnclaveRelayLimitResolution::denied(
                        EnclaveRelayLimitKind::SlowMode,
                        Some(next_allowed - input.now_unix_seconds),
                    );
                }
            }
        }
        EnclaveRelayActionMetadata::CreateMute { duration_seconds } => {
            if duration_seconds > limits.longest_mute_seconds {
                return EnclaveRelayLimitResolution::denied(
                    EnclaveRelayLimitKind::LongestMute,
                    None,
                );
            }
        }
        EnclaveRelayActionMetadata::GovernanceAction => {}
    }

    EnclaveRelayLimitResolution::allow_by_relay()
}

impl EnclavePermissionResolution {
    fn allow_by(rule: EnclavePermissionDecisionRule) -> Self {
        Self {
            decision: EnclavePermissionDecision::Allowed,
            rule,
        }
    }

    fn denied(rule: EnclavePermissionDecisionRule) -> Self {
        Self {
            decision: EnclavePermissionDecision::Denied,
            rule,
        }
    }

    pub fn denied_by(&self) -> Option<&'static str> {
        match (self.decision, self.rule) {
            (EnclavePermissionDecision::Denied, EnclavePermissionDecisionRule::Key) => Some("KEY"),
            (EnclavePermissionDecision::Denied, EnclavePermissionDecisionRule::Relay) => {
                Some("RELAY")
            }
            (EnclavePermissionDecision::Denied, EnclavePermissionDecisionRule::ChannelDeny) => {
                Some("CHANNEL_DENY")
            }
            (EnclavePermissionDecision::Denied, EnclavePermissionDecisionRule::Off) => Some("OFF"),
            (EnclavePermissionDecision::Denied, EnclavePermissionDecisionRule::RoleGrants) => {
                Some("ROLE_GRANTS")
            }
            (EnclavePermissionDecision::Denied, EnclavePermissionDecisionRule::ChannelAllow) => {
                Some("CHANNEL_ALLOW")
            }
            (EnclavePermissionDecision::Allowed, _) => None,
        }
    }

    pub fn decided_by(&self) -> &'static str {
        match self.rule {
            EnclavePermissionDecisionRule::Key => "KEY",
            EnclavePermissionDecisionRule::Relay => "RELAY",
            EnclavePermissionDecisionRule::ChannelDeny => "CHANNEL_DENY",
            EnclavePermissionDecisionRule::ChannelAllow => "CHANNEL_ALLOW",
            EnclavePermissionDecisionRule::RoleGrants => "ROLE_GRANTS",
            EnclavePermissionDecisionRule::Off => "OFF",
        }
    }

    pub fn allowed(&self) -> bool {
        self.decision == EnclavePermissionDecision::Allowed
    }
}

impl EnclaveRelayLimitResolution {
    fn allow_by_relay() -> Self {
        Self {
            decision: EnclavePermissionDecision::Allowed,
            rule: EnclavePermissionDecisionRule::Relay,
            limit: None,
            retry_after_seconds: None,
        }
    }

    fn denied(limit: EnclaveRelayLimitKind, retry_after_seconds: Option<u64>) -> Self {
        Self {
            decision: EnclavePermissionDecision::Denied,
            rule: EnclavePermissionDecisionRule::Relay,
            limit: Some(limit),
            retry_after_seconds,
        }
    }

    pub fn allowed(&self) -> bool {
        self.decision == EnclavePermissionDecision::Allowed
    }

    pub fn denied_by(&self) -> Option<&'static str> {
        if self.allowed() {
            None
        } else {
            Some("RELAY")
        }
    }

    pub fn decided_by(&self) -> &'static str {
        "RELAY"
    }
}

impl EnclaveRelayActionBudget {
    pub fn check_and_record(
        &mut self,
        role_name: &str,
        limits: &BTreeMap<String, EnclaveRoleRelayLimits>,
        now_unix_seconds: u64,
    ) -> EnclaveRelayLimitResolution {
        let Some(limit) = limits.get(role_name) else {
            return EnclaveRelayLimitResolution::denied(EnclaveRelayLimitKind::ActionBudget, None);
        };
        let actions = self
            .action_unix_seconds
            .entry(role_name.to_owned())
            .or_default();
        while actions
            .front()
            .is_some_and(|recorded_at| recorded_at.saturating_add(3600) <= now_unix_seconds)
        {
            actions.pop_front();
        }
        if actions.len() >= limit.actions_per_hour_budget {
            return EnclaveRelayLimitResolution::denied(EnclaveRelayLimitKind::ActionBudget, None);
        }
        actions.push_back(now_unix_seconds);
        EnclaveRelayLimitResolution::allow_by_relay()
    }
}

impl EnclaveRoleCatalog {
    pub fn roles(&self) -> &[EnclaveRoleRow] {
        &self.roles
    }

    pub fn role(&self, role_name: &str) -> Option<&EnclaveRoleRow> {
        self.roles.iter().find(|role| role.name == role_name)
    }

    pub fn role_names(&self) -> Vec<&str> {
        self.roles.iter().map(|role| role.name.as_str()).collect()
    }

    pub fn add_custom_role(
        &mut self,
        name: impl Into<String>,
        ticked_permission_names: impl IntoIterator<Item = String>,
    ) -> Result<&EnclaveRoleRow, String> {
        let name = normalize_role_name(name.into())?;
        if DEFAULT_ROLE_NAMES.contains(&name.as_str()) {
            return Err("custom role name is reserved".to_owned());
        }
        if self
            .roles
            .iter()
            .any(|role| role.name.eq_ignore_ascii_case(&name))
        {
            return Err("custom role name already exists".to_owned());
        }
        let permissions = normalize_permission_names(ticked_permission_names)?;
        self.roles.push(EnclaveRoleRow::new(name, permissions));
        Ok(self.roles.last().expect("role was just pushed"))
    }

    pub fn set_permission_for_role(
        &mut self,
        role_name: &str,
        permission_name: impl Into<String>,
        ticked: bool,
    ) -> Result<(), String> {
        let permission_name = permission_name.into();
        let role = self
            .roles
            .iter_mut()
            .find(|role| role.name == role_name)
            .ok_or_else(|| format!("unknown enclave role: {role_name}"))?;
        if ticked {
            role.ticked_permission_names.insert(permission_name);
        } else {
            role.ticked_permission_names.remove(&permission_name);
        }
        Ok(())
    }

    pub fn migrate_existing_roles_for_introduced_permissions(
        &mut self,
        previous_permission_names: &BTreeSet<String>,
        current_permission_names: &BTreeSet<String>,
    ) -> EnclavePermissionMigrationReport {
        let introduced_permission_names: BTreeSet<String> = current_permission_names
            .difference(previous_permission_names)
            .cloned()
            .collect();
        for role in &mut self.roles {
            for permission_name in &introduced_permission_names {
                role.ticked_permission_names.remove(permission_name);
            }
        }
        EnclavePermissionMigrationReport {
            introduced_permission_names,
            existing_role_count: self.roles.len(),
        }
    }

    pub fn require_introduced_permissions_off(
        &self,
        introduced_permission_names: &BTreeSet<String>,
    ) -> Result<(), String> {
        for role in &self.roles {
            for permission_name in introduced_permission_names {
                if role.ticked_permission_names.contains(permission_name) {
                    return Err(format!(
                        "introduced permission {permission_name} is on for existing role {}",
                        role.name
                    ));
                }
            }
        }
        Ok(())
    }
}

impl EnclaveRoleRow {
    fn new(name: impl Into<String>, ticked_permission_names: BTreeSet<String>) -> Self {
        Self {
            name: name.into(),
            ticked_permission_names,
        }
    }

    pub fn ticked_permission_count(&self) -> usize {
        self.ticked_permission_names.len()
    }

    pub fn administrator_flag_count(&self) -> usize {
        persisted_permission_name_search(&["admin", "administrator", "bypass_overrides"])
            .into_iter()
            .filter(|name| self.ticked_permission_names.contains(*name))
            .count()
    }

    pub fn permission_is_ticked(&self, permission_name: &str) -> bool {
        self.ticked_permission_names.contains(permission_name)
    }
}

fn normalize_role_name(name: String) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty()
        || trimmed.len() > 48
        || trimmed.chars().any(|character| character.is_control())
    {
        return Err("role name must be 1-48 printable characters".to_owned());
    }
    Ok(trimmed.to_owned())
}

fn normalize_permission_names(
    ticked_permission_names: impl IntoIterator<Item = String>,
) -> Result<BTreeSet<String>, String> {
    let valid: BTreeSet<&'static str> = ENCLAVE_PERMISSIONS
        .iter()
        .map(|permission| permission.persisted_name)
        .collect();
    let mut normalized = BTreeSet::new();
    for name in ticked_permission_names {
        if !valid.contains(name.as_str()) {
            return Err(format!("unknown enclave permission: {name}"));
        }
        normalized.insert(name);
    }
    Ok(normalized)
}

fn named_permissions(names: &[&str]) -> BTreeSet<String> {
    let by_name: BTreeMap<&'static str, &'static str> = ENCLAVE_PERMISSIONS
        .iter()
        .map(|permission| (permission.persisted_name, permission.persisted_name))
        .collect();
    names
        .iter()
        .map(|name| {
            by_name
                .get(name)
                .unwrap_or_else(|| panic!("default enclave permission is not declared: {name}"))
                .to_string()
        })
        .collect()
}

fn ensure_subset(
    role_permission_names: &BTreeSet<String>,
    all_permission_names: &BTreeSet<String>,
    role_name: &str,
) -> Result<(), String> {
    for permission_name in role_permission_names {
        if !all_permission_names.contains(permission_name) {
            return Err(format!(
                "{role_name} default enclave permission is not declared: {permission_name}"
            ));
        }
    }
    Ok(())
}

fn mod_permission_names() -> BTreeSet<String> {
    named_permissions(&[
        "read_messages",
        "send_messages",
        "edit_own_messages",
        "delete_own_messages",
        "create_threads",
        "reply_threads",
        "react_messages",
        "attach_files",
        "view_members",
        "invite_members",
        "remove_members",
        "mute_members",
        "manage_channels",
        "rename_channels",
        "archive_channels",
        "pin_messages",
        "start_voice",
        "screen_share",
        "view_audit_log",
        "export_receipts",
        "queue_burn",
        "approve_burn",
        "rotate_keys",
        "manage_invites",
    ])
}

fn member_permission_names() -> BTreeSet<String> {
    named_permissions(&[
        "read_messages",
        "send_messages",
        "edit_own_messages",
        "delete_own_messages",
        "create_threads",
        "reply_threads",
        "react_messages",
        "attach_files",
        "view_members",
        "start_voice",
        "screen_share",
        "view_presence",
    ])
}

const ENCLAVE_PERMISSIONS: [EnclavePermission; 40] = [
    EnclavePermission {
        persisted_name: "read_messages",
        label: "Read messages",
    },
    EnclavePermission {
        persisted_name: "send_messages",
        label: "Send messages",
    },
    EnclavePermission {
        persisted_name: "edit_own_messages",
        label: "Edit own messages",
    },
    EnclavePermission {
        persisted_name: "delete_own_messages",
        label: "Delete own messages",
    },
    EnclavePermission {
        persisted_name: "create_threads",
        label: "Create threads",
    },
    EnclavePermission {
        persisted_name: "reply_threads",
        label: "Reply in threads",
    },
    EnclavePermission {
        persisted_name: "react_messages",
        label: "React to messages",
    },
    EnclavePermission {
        persisted_name: "attach_files",
        label: "Attach files",
    },
    EnclavePermission {
        persisted_name: "view_members",
        label: "View members",
    },
    EnclavePermission {
        persisted_name: "invite_members",
        label: "Invite members",
    },
    EnclavePermission {
        persisted_name: "remove_members",
        label: "Remove members",
    },
    EnclavePermission {
        persisted_name: "mute_members",
        label: "Mute members",
    },
    EnclavePermission {
        persisted_name: "manage_channels",
        label: "Manage channels",
    },
    EnclavePermission {
        persisted_name: "rename_channels",
        label: "Rename channels",
    },
    EnclavePermission {
        persisted_name: "archive_channels",
        label: "Archive channels",
    },
    EnclavePermission {
        persisted_name: "pin_messages",
        label: "Pin messages",
    },
    EnclavePermission {
        persisted_name: "start_voice",
        label: "Start voice",
    },
    EnclavePermission {
        persisted_name: "screen_share",
        label: "Share screen",
    },
    EnclavePermission {
        persisted_name: "record_meetings",
        label: "Record meetings",
    },
    EnclavePermission {
        persisted_name: "view_audit_log",
        label: "View audit log",
    },
    EnclavePermission {
        persisted_name: "export_receipts",
        label: "Export receipts",
    },
    EnclavePermission {
        persisted_name: "queue_burn",
        label: "Queue burn",
    },
    EnclavePermission {
        persisted_name: "approve_burn",
        label: "Approve burn",
    },
    EnclavePermission {
        persisted_name: "rotate_keys",
        label: "Rotate keys",
    },
    EnclavePermission {
        persisted_name: "manage_invites",
        label: "Manage invites",
    },
    EnclavePermission {
        persisted_name: "create_channels",
        label: "Create channels",
    },
    EnclavePermission {
        persisted_name: "delete_channels",
        label: "Delete channels",
    },
    EnclavePermission {
        persisted_name: "set_retention",
        label: "Set retention",
    },
    EnclavePermission {
        persisted_name: "manage_webhooks",
        label: "Manage webhooks",
    },
    EnclavePermission {
        persisted_name: "manage_integrations",
        label: "Manage integrations",
    },
    EnclavePermission {
        persisted_name: "publish_announcements",
        label: "Publish announcements",
    },
    EnclavePermission {
        persisted_name: "edit_enclave_profile",
        label: "Edit enclave profile",
    },
    EnclavePermission {
        persisted_name: "manage_roles",
        label: "Manage roles",
    },
    EnclavePermission {
        persisted_name: "assign_roles",
        label: "Assign roles",
    },
    EnclavePermission {
        persisted_name: "view_presence",
        label: "View presence",
    },
    EnclavePermission {
        persisted_name: "manage_presence",
        label: "Manage presence",
    },
    EnclavePermission {
        persisted_name: "create_polls",
        label: "Create polls",
    },
    EnclavePermission {
        persisted_name: "close_polls",
        label: "Close polls",
    },
    EnclavePermission {
        persisted_name: "manage_stickers",
        label: "Manage stickers",
    },
    EnclavePermission {
        persisted_name: "view_security_events",
        label: "View security events",
    },
];
