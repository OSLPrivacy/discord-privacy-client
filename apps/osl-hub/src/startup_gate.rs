//! Password-gate role routing for the standalone Hub.
//!
//! Password comparison and lockout accounting remain owned by the original
//! IPC core. This module only converts its fixed role label into a typed action
//! for the trusted desktop command.

use serde::Serialize;

use crate::core_bridge::{self, CoreReadiness, HubCoreState};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum VerifiedGateRole {
    Main,
    Stealth,
    Burn,
    Wrong,
}

#[derive(Debug)]
pub struct GatePasswordVerification {
    pub role: VerifiedGateRole,
    pub lockout_seconds_remaining: i64,
    pub attempts_used: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HubGateUnlockResult {
    /// `unlocked`, `decoy`, `burned`, or `wrong`.
    pub outcome: &'static str,
    pub lockout_seconds_remaining: i64,
    pub attempts_used: u32,
    pub readiness: Option<CoreReadiness>,
    pub burn: Option<crate::cleanup::HubFullCleanupResult>,
}

impl HubGateUnlockResult {
    pub fn wrong(verification: GatePasswordVerification) -> Self {
        Self {
            outcome: "wrong",
            lockout_seconds_remaining: verification.lockout_seconds_remaining,
            attempts_used: verification.attempts_used,
            readiness: None,
            burn: None,
        }
    }

    pub fn unlocked(verification: GatePasswordVerification, readiness: CoreReadiness) -> Self {
        Self {
            outcome: "unlocked",
            lockout_seconds_remaining: verification.lockout_seconds_remaining,
            attempts_used: verification.attempts_used,
            readiness: Some(readiness),
            burn: None,
        }
    }

    pub fn decoy(verification: GatePasswordVerification) -> Self {
        Self {
            outcome: "decoy",
            lockout_seconds_remaining: verification.lockout_seconds_remaining,
            attempts_used: verification.attempts_used,
            readiness: None,
            burn: None,
        }
    }

    pub fn burned(
        verification: GatePasswordVerification,
        burn: crate::cleanup::HubFullCleanupResult,
    ) -> Self {
        Self {
            outcome: "burned",
            lockout_seconds_remaining: verification.lockout_seconds_remaining,
            attempts_used: verification.attempts_used,
            readiness: None,
            burn: Some(burn),
        }
    }
}

pub fn verify_password_role(
    state: &HubCoreState,
    password: String,
) -> Result<GatePasswordVerification, String> {
    let result = ipc::commands::cmd_osl_verify_gate_password(&state.osl, password)?;
    let role = match result.result.as_str() {
        "main" => VerifiedGateRole::Main,
        "stealth" => VerifiedGateRole::Stealth,
        "burn" => VerifiedGateRole::Burn,
        "wrong" => VerifiedGateRole::Wrong,
        _ => return Err("OSL password gate returned an invalid role".to_owned()),
    };
    Ok(GatePasswordVerification {
        role,
        lockout_seconds_remaining: result.lockout_seconds_remaining,
        attempts_used: result.attempts_used,
    })
}

pub fn readiness_after_main(state: &HubCoreState) -> CoreReadiness {
    core_bridge::readiness(state)
}

/// Make a stealth landing incapable of decrypting even if a future caller
/// mistakenly invokes the gate from an already-unlocked process.
pub fn enter_stealth_landing(state: &HubCoreState) {
    ipc::main_password::set_file_storage_key(None);
    crate::identity_registry::reset_account_scoped_state(&state.osl);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_gate_role_has_exactly_one_action() {
        let actions = [
            (VerifiedGateRole::Main, "unlocked"),
            (VerifiedGateRole::Stealth, "decoy"),
            (VerifiedGateRole::Burn, "burned"),
            (VerifiedGateRole::Wrong, "wrong"),
        ];
        assert_eq!(actions.len(), 4);
        assert_ne!(actions[0].1, actions[1].1);
        assert_ne!(actions[0].1, actions[2].1);
        assert_ne!(actions[1].1, actions[2].1);
    }

    #[test]
    fn wrong_result_never_contains_unlock_or_burn_payload() {
        let result = HubGateUnlockResult::wrong(GatePasswordVerification {
            role: VerifiedGateRole::Wrong,
            lockout_seconds_remaining: 12,
            attempts_used: 3,
        });
        assert_eq!(result.outcome, "wrong");
        assert!(result.readiness.is_none());
        assert!(result.burn.is_none());
    }
}
