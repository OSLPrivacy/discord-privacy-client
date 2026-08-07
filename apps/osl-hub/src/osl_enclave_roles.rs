use std::collections::{BTreeMap, BTreeSet};

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

pub fn new_enclave_role_catalog() -> EnclaveRoleCatalog {
    EnclaveRoleCatalog {
        roles: vec![
            EnclaveRoleRow::new(OWNER_ROLE_NAME, all_persisted_permission_names()),
            EnclaveRoleRow::new(MOD_ROLE_NAME, mod_permission_names()),
            EnclaveRoleRow::new(MEMBER_ROLE_NAME, member_permission_names()),
        ],
    }
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

impl EnclaveRoleCatalog {
    pub fn roles(&self) -> &[EnclaveRoleRow] {
        &self.roles
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
