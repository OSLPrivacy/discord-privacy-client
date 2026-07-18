//! Trusted-local configuration for the original OSL password roles.
//!
//! These functions deliberately reuse the IPC core's Argon2 marker and shared
//! lockout implementation. They configure the roles only; destructive burn
//! execution and the stealth landing screen remain separate, explicit flows.

use serde::Serialize;

use crate::core_bridge::{self, HubCoreState};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HubPasswordRoleStatus {
    pub main_password_set: bool,
    pub stealth_password_set: bool,
    pub burn_password_set: bool,
    pub unlocked: bool,
    pub stealth_action_wired: bool,
    pub burn_action_wired: bool,
}

pub fn password_role_status(state: &HubCoreState) -> Result<HubPasswordRoleStatus, String> {
    let readiness = core_bridge::readiness(state);
    let main_password_set = ipc::commands::cmd_osl_password_status()
        .map_err(|_| "OSL password state is unavailable".to_owned())?
        .is_set;
    let stealth_password_set = ipc::commands::cmd_osl_stealth_password_status()
        .map_err(|_| "OSL stealth-password state is unavailable".to_owned())?
        .is_set;
    let burn_password_set = ipc::commands::cmd_osl_burn_password_status()
        .map_err(|_| "OSL burn-password state is unavailable".to_owned())?
        .is_set;
    Ok(HubPasswordRoleStatus {
        main_password_set,
        stealth_password_set,
        burn_password_set,
        unlocked: readiness.unlocked && readiness.identity_loaded,
        // Configuration is now linked, but the Hub must not claim a role is
        // usable at login until its distinct visible consequence is wired.
        stealth_action_wired: false,
        burn_action_wired: false,
    })
}

pub fn set_stealth_password(
    state: &HubCoreState,
    current_main: String,
    new_stealth: String,
) -> Result<HubPasswordRoleStatus, String> {
    require_unlocked_identity(state)?;
    reject_equal_roles(&current_main, &new_stealth)?;
    ipc::commands::cmd_osl_set_stealth_password(current_main, new_stealth)
        .map_err(|_| "OSL could not set the stealth password".to_owned())?;
    password_role_status(state)
}

pub fn remove_stealth_password(
    state: &HubCoreState,
    current_main: String,
) -> Result<HubPasswordRoleStatus, String> {
    require_unlocked_identity(state)?;
    ipc::commands::cmd_osl_remove_stealth_password(current_main)
        .map_err(|_| "OSL could not remove the stealth password".to_owned())?;
    password_role_status(state)
}

pub fn set_burn_password(
    state: &HubCoreState,
    current_main: String,
    new_burn: String,
) -> Result<HubPasswordRoleStatus, String> {
    require_unlocked_identity(state)?;
    reject_equal_roles(&current_main, &new_burn)?;
    ipc::commands::cmd_osl_set_burn_password(current_main, new_burn)
        .map_err(|_| "OSL could not set the burn password".to_owned())?;
    password_role_status(state)
}

pub fn remove_burn_password(
    state: &HubCoreState,
    current_main: String,
) -> Result<HubPasswordRoleStatus, String> {
    require_unlocked_identity(state)?;
    ipc::commands::cmd_osl_remove_burn_password(current_main)
        .map_err(|_| "OSL could not remove the burn password".to_owned())?;
    password_role_status(state)
}

fn require_unlocked_identity(state: &HubCoreState) -> Result<(), String> {
    let readiness = core_bridge::readiness(state);
    if !readiness.unlocked || !readiness.identity_loaded {
        return Err("Unlock an OSL identity before changing password roles".to_owned());
    }
    Ok(())
}

fn reject_equal_roles(main: &str, alternate: &str) -> Result<(), String> {
    ipc::main_password::validate_new_password(alternate)
        .map_err(|_| "OSL passwords must contain 6 to 128 keyboard characters".to_owned())?;
    if main == alternate {
        return Err("The alternate password must differ from the main password".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alternate_password_cannot_equal_main_password() {
        assert!(reject_equal_roles("ordinary-password", "ordinary-password").is_err());
        assert!(reject_equal_roles("ordinary-password", "different-password").is_ok());
    }

    #[test]
    fn alternate_password_uses_the_original_password_policy() {
        assert!(reject_equal_roles("ordinary-password", "tiny").is_err());
        assert!(reject_equal_roles("ordinary-password", "sixsix").is_ok());
    }
}
