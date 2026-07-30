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
    Duress,
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
    /// `unlocked`, `decoy`, `burned`, `duress`, or `wrong`.
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

    pub fn duress(verification: GatePasswordVerification) -> Self {
        Self {
            outcome: "duress",
            lockout_seconds_remaining: verification.lockout_seconds_remaining,
            attempts_used: verification.attempts_used,
            readiness: None,
            burn: None,
        }
    }
}

pub fn verify_password_role(
    state: &HubCoreState,
    password: String,
) -> Result<GatePasswordVerification, String> {
    let result = ipc::commands::cmd_osl_verify_gate_password(&state.osl, password)?;
    let parsed_role = match result.result.as_str() {
        "main" => VerifiedGateRole::Main,
        "stealth" => VerifiedGateRole::Stealth,
        "burn" => VerifiedGateRole::Burn,
        "duress" => VerifiedGateRole::Duress,
        "wrong" => VerifiedGateRole::Wrong,
        _ => return Err("OSL password gate returned an invalid role".to_owned()),
    };
    let role = role_after_duress_threshold(parsed_role, result.attempts_used);
    Ok(GatePasswordVerification {
        role,
        lockout_seconds_remaining: result.lockout_seconds_remaining,
        attempts_used: result.attempts_used,
    })
}

fn role_after_duress_threshold(role: VerifiedGateRole, attempts_used: u32) -> VerifiedGateRole {
    if role == VerifiedGateRole::Wrong
        && attempts_used >= ipc::main_password::DURESS_WRONG_PASSWORD_ATTEMPT_LIMIT
    {
        VerifiedGateRole::Duress
    } else {
        role
    }
}

pub fn verify_duress_pin(
    state: &HubCoreState,
    pin: String,
) -> Result<GatePasswordVerification, String> {
    let dir =
        keystore::osl_base_dir().map_err(|_| "OSL password storage is unavailable".to_owned())?;
    let mut lock = ipc::main_password::read_lockout_pub(&dir);
    let now = ipc::main_password::now_unix_secs_pub();
    if let Some(until) = lock.password_locked_until {
        if now < until {
            return Ok(GatePasswordVerification {
                role: VerifiedGateRole::Wrong,
                lockout_seconds_remaining: until - now,
                attempts_used: lock.password_failed_attempts,
            });
        }
    }

    let marker = ipc::main_password::read_marker_pub(&dir)?;
    match ipc::main_password::verify_gate_password_with_marker(&marker, &pin)? {
        ipc::main_password::GateMatch::Burn => {
            lock.password_failed_attempts = 0;
            lock.password_locked_until = None;
            let _ = ipc::main_password::write_lockout_pub(&dir, &lock);
            Ok(GatePasswordVerification {
                role: VerifiedGateRole::Burn,
                lockout_seconds_remaining: 0,
                attempts_used: 0,
            })
        }
        ipc::main_password::GateMatch::Duress => {
            lock.password_failed_attempts = 0;
            lock.password_locked_until = None;
            let _ = ipc::main_password::write_lockout_pub(&dir, &lock);
            ipc::main_password::execute_gate_duress(&state.osl)?;
            Ok(GatePasswordVerification {
                role: VerifiedGateRole::Duress,
                lockout_seconds_remaining: 0,
                attempts_used: 0,
            })
        }
        ipc::main_password::GateMatch::Main(_)
        | ipc::main_password::GateMatch::Stealth
        | ipc::main_password::GateMatch::Wrong => {
            match ipc::main_password::record_wrong_password_attempt_or_duress(
                &state.osl, &mut lock, now,
            )? {
                ipc::main_password::WrongPasswordAttemptAction::Wrong {
                    attempts_used,
                    lockout_seconds_remaining,
                } => {
                    let _ = ipc::main_password::write_lockout_pub(&dir, &lock);
                    Ok(GatePasswordVerification {
                        role: VerifiedGateRole::Wrong,
                        lockout_seconds_remaining,
                        attempts_used,
                    })
                }
                ipc::main_password::WrongPasswordAttemptAction::DuressTriggered {
                    attempts_used,
                } => {
                    let _ = ipc::main_password::write_lockout_pub(&dir, &lock);
                    Ok(GatePasswordVerification {
                        role: VerifiedGateRole::Duress,
                        lockout_seconds_remaining: 0,
                        attempts_used,
                    })
                }
            }
        }
    }
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
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct KeystoreGlobalReset;

    impl Drop for KeystoreGlobalReset {
        fn drop(&mut self) {
            ipc::main_password::set_file_storage_key(None);
            keystore::set_active_account_dir(None);
            keystore::set_base_dir_override(None);
        }
    }

    fn temp_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "osl-startup-gate-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn populate_cleanup_roots(label: &str) -> (PathBuf, PathBuf, PathBuf, PathBuf, PathBuf) {
        let config_dir = temp_dir(&format!("{label}-config"));
        let local_data_dir = temp_dir(&format!("{label}-local"));
        let core_dir = config_dir.join("osl-core");
        let service_profiles = local_data_dir.join("service-profiles-v2");
        let native_profiles = local_data_dir.join("native-window-profiles-v1");
        std::fs::create_dir_all(&core_dir).unwrap();
        std::fs::create_dir_all(&service_profiles).unwrap();
        std::fs::create_dir_all(&native_profiles).unwrap();
        std::fs::write(core_dir.join("peer_map.json"), br#"{}"#).unwrap();
        std::fs::write(service_profiles.join("profile-cache"), b"profile").unwrap();
        std::fs::write(native_profiles.join("native-cache"), b"native").unwrap();
        std::fs::write(config_dir.join("service-registry.json"), br#"{}"#).unwrap();
        std::fs::write(config_dir.join("service-scope-index.json"), br#"{}"#).unwrap();
        std::fs::write(config_dir.join("preview-preferences.json"), br#"{}"#).unwrap();
        (
            config_dir,
            local_data_dir,
            core_dir,
            service_profiles,
            native_profiles,
        )
    }

    fn gate_result_for_verification(
        state: &HubCoreState,
        verification: GatePasswordVerification,
        config_dir: &std::path::Path,
        local_data_dir: &std::path::Path,
    ) -> HubGateUnlockResult {
        match verification.role {
            VerifiedGateRole::Burn => {
                let burn = crate::cleanup::execute_verified_gate_burn(
                    state,
                    config_dir,
                    local_data_dir,
                    true,
                )
                .unwrap();
                HubGateUnlockResult::burned(verification, burn)
            }
            VerifiedGateRole::Duress => {
                let _ = (state, config_dir, local_data_dir);
                HubGateUnlockResult::duress(verification)
            }
            VerifiedGateRole::Wrong => HubGateUnlockResult::wrong(verification),
            VerifiedGateRole::Main | VerifiedGateRole::Stealth => {
                panic!("test only routes burn, duress, and wrong gate roles")
            }
        }
    }

    fn assert_cleanup_result_removed(
        result: &HubGateUnlockResult,
        expected_outcome: &str,
        target: &str,
        removed_path: &std::path::Path,
    ) {
        assert_eq!(result.outcome, expected_outcome);
        assert!(result.readiness.is_none());
        let burn = result.burn.as_ref().expect("burn result is present");
        assert!(burn.local_cleanup_complete);
        assert!(burn.failed_targets.is_empty());
        assert!(!burn.restart_required);
        assert!(burn.original_discord_data_untouched);
        assert!(
            burn.removed_targets.iter().any(|removed| removed == target),
            "cleanup report did not include removed target {target}; report={:?}",
            burn.removed_targets
        );
        assert!(
            !removed_path.exists(),
            "cleanup target {target} still exists at {}",
            removed_path.display()
        );
    }

    #[test]
    fn every_gate_role_has_exactly_one_action() {
        let actions = [
            (VerifiedGateRole::Main, "unlocked"),
            (VerifiedGateRole::Stealth, "decoy"),
            (VerifiedGateRole::Burn, "burned"),
            (VerifiedGateRole::Duress, "duress"),
            (VerifiedGateRole::Wrong, "wrong"),
        ];
        assert_eq!(actions.len(), 5);
        for (index, (_, outcome)) in actions.iter().enumerate() {
            assert!(
                actions
                    .iter()
                    .enumerate()
                    .all(|(candidate, (_, other))| candidate == index || other != outcome),
                "gate outcome {outcome} must stay distinct"
            );
        }
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

    #[test]
    fn burn_code_and_wrong_password_threshold_keep_distinct_paths() {
        let _serial = crate::GLOBAL_KEYSTORE_TEST_LOCK.lock().unwrap();
        let _reset = KeystoreGlobalReset;
        let state = HubCoreState::default();

        let below_threshold = GatePasswordVerification {
            role: role_after_duress_threshold(
                VerifiedGateRole::Wrong,
                ipc::main_password::DURESS_WRONG_PASSWORD_ATTEMPT_LIMIT - 1,
            ),
            lockout_seconds_remaining: 3600,
            attempts_used: ipc::main_password::DURESS_WRONG_PASSWORD_ATTEMPT_LIMIT - 1,
        };
        let below_result = HubGateUnlockResult::wrong(below_threshold);
        assert_eq!(below_result.outcome, "wrong");
        assert!(below_result.burn.is_none());

        let threshold_verification = GatePasswordVerification {
            role: role_after_duress_threshold(
                VerifiedGateRole::Wrong,
                ipc::main_password::DURESS_WRONG_PASSWORD_ATTEMPT_LIMIT,
            ),
            lockout_seconds_remaining: 3600,
            attempts_used: ipc::main_password::DURESS_WRONG_PASSWORD_ATTEMPT_LIMIT,
        };
        assert_eq!(threshold_verification.role, VerifiedGateRole::Duress);
        let threshold_result = HubGateUnlockResult::duress(threshold_verification);
        assert_eq!(threshold_result.outcome, "duress");
        assert!(threshold_result.burn.is_none());

        let (burn_config, burn_local, burn_core, burn_profiles, _) =
            populate_cleanup_roots("burn");
        keystore::set_base_dir_override(Some(burn_core.clone()));
        let burn_result = gate_result_for_verification(
            &state,
            GatePasswordVerification {
                role: VerifiedGateRole::Burn,
                lockout_seconds_remaining: 0,
                attempts_used: 0,
            },
            &burn_config,
            &burn_local,
        );
        assert_cleanup_result_removed(&burn_result, "burned", "hub_core", &burn_core);
        assert_cleanup_result_removed(&burn_result, "burned", "service_profiles", &burn_profiles);
        assert_eq!(burn_result.outcome, "burned");

        let _ = std::fs::remove_dir_all(burn_config);
        let _ = std::fs::remove_dir_all(burn_local);
    }
}
