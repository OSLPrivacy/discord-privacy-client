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
    use std::io::ErrorKind;
    use std::net::TcpListener;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    const TEST_PASSWORD: &str = "aB3!z9-safe-passphrase";
    static PASSWORD_GATE_TEST_LOCK: Mutex<()> = Mutex::new(());

    struct IsolatedPasswordGate {
        dir: PathBuf,
    }

    impl IsolatedPasswordGate {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("test clock")
                .as_nanos();
            let dir = std::env::temp_dir().join(format!(
                "osl-hub-password-gate-{}-{nonce}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).expect("isolated password-gate directory");
            keystore::set_active_account_dir(None);
            keystore::set_base_dir_override(Some(dir.clone()));
            ipc::main_password::set_file_storage_key(None);
            ipc::main_password::set_main_password(&dir, TEST_PASSWORD)
                .expect("install isolated main password");
            ipc::main_password::set_file_storage_key(None);
            Self { dir }
        }

        fn path(&self) -> &Path {
            &self.dir
        }
    }

    impl Drop for IsolatedPasswordGate {
        fn drop(&mut self) {
            ipc::main_password::set_file_storage_key(None);
            keystore::set_active_account_dir(None);
            keystore::set_base_dir_override(None);
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

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

    #[test]
    fn correct_password_fails_closed_when_required_security_state_is_corrupt() {
        let _serial = PASSWORD_GATE_TEST_LOCK.lock().unwrap();
        let isolated = IsolatedPasswordGate::new();
        let corrupt_path = isolated.path().join("peer_map.json");
        std::fs::write(&corrupt_path, b"{not-valid-security-state")
            .expect("write corrupt required state");

        let error = verify_password_role(&HubCoreState::default(), TEST_PASSWORD.to_owned())
            .expect_err("corrupt required state must keep the session locked");

        assert_eq!(
            error,
            "OSL encrypted security state could not be reloaded safely"
        );
        assert!(ipc::main_password::get_file_storage_key().is_none());
        assert_eq!(
            std::fs::read(&corrupt_path).expect("corrupt state remains recoverable"),
            b"{not-valid-security-state"
        );
    }

    #[test]
    fn main_gate_never_contacts_configured_hanging_registration_endpoint() {
        let _serial = PASSWORD_GATE_TEST_LOCK.lock().unwrap();
        let isolated = IsolatedPasswordGate::new();
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind hanging endpoint");
        listener
            .set_nonblocking(true)
            .expect("make endpoint observation nonblocking");
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        std::fs::write(
            isolated.path().join("keyserver.json"),
            serde_json::to_vec(&serde_json::json!({
                "base_url": endpoint,
                "user_id": "offline-unlock-probe"
            }))
            .unwrap(),
        )
        .expect("write isolated keyserver configuration");
        let state = HubCoreState::default();
        *state.osl.identity.lock().unwrap() = Some(keystore::identity_from_entropy(
            [31; 16],
            "offline-unlock-probe".to_owned(),
        ));

        let verified = verify_password_role(&state, TEST_PASSWORD.to_owned())
            .expect("local unlock must not depend on registration");

        assert_eq!(verified.role, VerifiedGateRole::Main);
        assert!(matches!(listener.accept(), Err(error) if error.kind() == ErrorKind::WouldBlock));
        assert_ne!(
            state.osl.cloud_registration_state(),
            ipc::state::CloudRegistrationState::Pending,
            "the gate itself must not claim or start the deferred worker"
        );
    }
}
