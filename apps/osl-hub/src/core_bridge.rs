//! Narrow trusted bridge to the original Discord OSL core.
//!
//! The app links the existing `ipc` crate instead of duplicating cryptography
//! or security-sensitive state machines. These types intentionally expose only
//! local readiness and a feature inventory. Remote service webviews receive no
//! Tauri capability and therefore cannot call this bridge.

use ipc::AppState;
use serde::Serialize;
use std::sync::Mutex;

pub struct HubCoreState {
    pub osl: AppState,
    bootstrap_attempted: bool,
    /// Serialises trusted identity/password transitions so Create, Import,
    /// Setup, and Unlock cannot race each other into replacing disk state.
    pub(crate) lifecycle_lock: Mutex<()>,
}

impl Default for HubCoreState {
    fn default() -> Self {
        Self {
            osl: AppState::new(),
            bootstrap_attempted: false,
            lifecycle_lock: Mutex::new(()),
        }
    }
}

impl HubCoreState {
    /// Load the original OSL account and security state from its sealed local
    /// configuration. Missing, locked, or corrupt state remains unavailable.
    pub fn bootstrap_from_disk() -> Self {
        let state = Self {
            osl: AppState::new(),
            bootstrap_attempted: true,
            lifecycle_lock: Mutex::new(()),
        };
        crate::original_bootstrap::run_autostart_local(&state.osl);
        state
    }

    pub fn register_after_local_bootstrap(&self) {
        crate::original_bootstrap::register_after_local_bootstrap(&self.osl);
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreReadiness {
    pub original_core_linked: bool,
    pub bootstrap_attempted: bool,
    pub password_gate_required: bool,
    pub unlocked: bool,
    pub active_osl_user_id: Option<String>,
    pub bootstrap_status: &'static str,
    pub identity_loaded: bool,
    pub keyserver_initialised: bool,
    pub cloud_registration_state: &'static str,
    pub group_sender_keys_enabled: bool,
    pub remote_service_has_native_access: bool,
}

/// Bounded license view for the trusted OSL Privacy UI. The activation code itself is
/// never returned to JavaScript after validation, and hosted service webviews
/// have no capability for the commands that expose this type.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HubLicenseState {
    pub access: &'static str,
    pub status: &'static str,
    pub current_period_end: Option<i64>,
    pub last_validated_at: Option<i64>,
}

pub fn license_state(state: &HubCoreState) -> Result<HubLicenseState, String> {
    ipc::commands::cmd_osl_get_license_state(&state.osl)
        .map(hub_license_state)
        .map_err(|_| "OSL activation state is unavailable".to_owned())
}

pub fn validate_activation_code(
    state: &HubCoreState,
    activation_code: String,
) -> Result<HubLicenseState, String> {
    let activation_code = normalize_activation_code(&activation_code)?;
    let response = ipc::commands::cmd_osl_validate_license(&state.osl, activation_code)
        .map_err(|error| friendly_activation_error(&error))?;

    if !response.checksum_ok || response.status == "UNKNOWN" {
        return Err("That activation code was not recognized".to_owned());
    }
    match response.status.as_str() {
        "ACTIVE" | "CANCELLED" | "GRACE" => {
            let saved = license_state(state)?;
            if saved.access == "free" {
                Err("Activation was confirmed but could not be saved on this device".to_owned())
            } else {
                Ok(saved)
            }
        }
        "REVOKED" => Err("This activation code has been revoked".to_owned()),
        "EXPIRED" => Err("This activation code has expired".to_owned()),
        "PENDING" => Err("This activation code is not active yet".to_owned()),
        _ => Err("That activation code was not recognized".to_owned()),
    }
}

pub fn clear_activation_code(state: &HubCoreState) -> Result<HubLicenseState, String> {
    ipc::commands::cmd_osl_clear_license(&state.osl)
        .map_err(|_| "The saved activation code could not be cleared".to_owned())?;
    license_state(state)
}

fn normalize_activation_code(value: &str) -> Result<String, String> {
    normalize_activation_code_for_build(value, cfg!(debug_assertions))
}

fn normalize_activation_code_for_build(
    value: &str,
    allow_qa_issuer: bool,
) -> Result<String, String> {
    let value = value.trim().to_ascii_uppercase();
    let parts: Vec<&str> = value.split('-').collect();
    let valid = parts.len() == 5
        && (parts[0] == "OSL" || (allow_qa_issuer && parts[0] == "OSLQ"))
        && parts[1..].iter().all(|part| {
            part.len() == 4
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
        });
    if valid {
        Ok(value)
    } else {
        Err("Enter the activation code shown after checkout".to_owned())
    }
}

fn friendly_activation_error(error: &str) -> String {
    if error.starts_with(ipc::commands::OSL_VALIDATE_ERR_PREFIX)
        && error.contains("\"kind\":\"unreachable\"")
    {
        "OSL could not reach the activation server".to_owned()
    } else {
        "That activation code was not recognized".to_owned()
    }
}

fn hub_license_state(value: keystore::LicenseStateDto) -> HubLicenseState {
    let access = match value.state {
        keystore::LicenseState::Paid => "pro",
        keystore::LicenseState::PaidOfflineGrace => "offlineGrace",
        keystore::LicenseState::Free => "free",
    };
    let status = match value.raw_status.as_str() {
        "ACTIVE" => "ACTIVE",
        "CANCELLED" => "CANCELLED",
        "GRACE" => "GRACE",
        "EXPIRED" => "EXPIRED",
        "REVOKED" => "REVOKED",
        "UNKNOWN" => "UNKNOWN",
        "PENDING" => "PENDING",
        "Unconfigured" => "UNCONFIGURED",
        _ => "UNKNOWN",
    };
    HubLicenseState {
        access,
        status,
        current_period_end: value.current_period_end,
        last_validated_at: value.last_validated_at,
    }
}

pub fn readiness(state: &HubCoreState) -> CoreReadiness {
    let status = ipc::commands::cmd_status(&state.osl);
    let password_gate_required = ipc::commands::cmd_osl_password_status()
        .map(|value| value.is_set)
        .unwrap_or(true);
    let unlocked = !password_gate_required || ipc::main_password::get_file_storage_key().is_some();
    // The original Discord command resolves the identity through a
    // Discord-snowflake row in peer_map.json. A native OSL Privacy identity is
    // deliberately service-neutral and has no such row, so use the loaded
    // identity's own OSL user id here. Remote service pages never receive this
    // state directly; only the trusted OSL Privacy broker consumes it.
    let active_osl_user_id = if unlocked && status.identity_loaded {
        state
            .osl
            .identity
            .lock()
            .ok()
            .and_then(|identity| identity.as_ref().map(|id| id.user_id.clone()))
    } else {
        None
    };
    let bootstrap_status = classify_bootstrap_status(
        state.bootstrap_attempted,
        status.identity_loaded,
        password_gate_required,
        unlocked,
        status.keyserver_initialised,
        state.osl.cloud_registration_state() == ipc::state::CloudRegistrationState::Registered,
        active_osl_user_id.is_some(),
    );
    CoreReadiness {
        original_core_linked: true,
        bootstrap_attempted: state.bootstrap_attempted,
        password_gate_required,
        unlocked,
        active_osl_user_id,
        bootstrap_status,
        identity_loaded: status.identity_loaded,
        keyserver_initialised: status.keyserver_initialised,
        cloud_registration_state: state.osl.cloud_registration_state().as_str(),
        // The original core deliberately leaves v5 disabled because one social
        // account on two physical devices can currently desynchronise it.
        group_sender_keys_enabled: false,
        remote_service_has_native_access: false,
    }
}

fn classify_bootstrap_status(
    bootstrap_attempted: bool,
    identity_loaded: bool,
    password_set: bool,
    unlocked: bool,
    keyserver_initialised: bool,
    cloud_registration_confirmed: bool,
    active_user_available: bool,
) -> &'static str {
    if !bootstrap_attempted {
        "notAttempted"
    } else if !identity_loaded || !password_set {
        // First-run Settings decides whether the missing local prerequisite is
        // identity creation/import or main-password setup. Keep this distinct
        // from an existing password gate awaiting user input.
        "setupRequired"
    } else if !unlocked {
        "passwordRequired"
    } else if !keyserver_initialised || !cloud_registration_confirmed || !active_user_available {
        "failed"
    } else {
        "ready"
    }
}

/// Unlock only the ordinary main-password role. Stealth and burn passwords
/// are intentionally not accepted here; those need dedicated guarded flows
/// with their own visible consequences.
pub fn unlock_main_password(
    state: &HubCoreState,
    password: String,
) -> Result<CoreReadiness, String> {
    let _lifecycle = state
        .lifecycle_lock
        .lock()
        .map_err(|_| "OSL account lifecycle is unavailable".to_string())?;
    ipc::main_password::validate_password(&password)
        .map_err(|_| "OSL main-password unlock was rejected".to_string())?;
    // Preserve the original structured attempt/lockout error. It contains no
    // password material and lets the trusted OSL Privacy show the real cooldown rather
    // than accidentally encouraging repeated guesses.
    ipc::commands::cmd_osl_verify_main_password(password)?;

    let outcome = (|| {
        let config_dir = keystore::osl_config_dir()
            .map_err(|_| "OSL account storage is unavailable".to_string())?;
        let report =
            ipc::state_reload::reload_encrypted_state_after_unlock(&state.osl, &config_dir)
                .map_err(|_| "OSL encrypted state could not be reloaded".to_string())?;
        if !report.errors.is_empty() {
            return Err("OSL encrypted state could not be reloaded safely".to_string());
        }

        // The cold-start bootstrap intentionally skips network work so the
        // password screen paints immediately. Once unlock has loaded the
        // sealed identity, prove its current public keys to Cloudflare before
        // returning a protection-ready state. A client object alone is not a
        // successful registration.
        ipc::commands::ensure_keyserver_registered(
            &state.osl,
            &ipc::commands::resolve_keyserver_base_url(&config_dir),
            None,
        );

        // An existing device password can legitimately be unlocked before a
        // new isolated OSL Privacy identity is created/imported. Do not deadlock that
        // safe setup path by requiring identity/keyserver readiness here.
        Ok(readiness(state))
    })();
    if outcome.is_err() {
        ipc::main_password::set_file_storage_key(None);
    }
    outcome
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreFeature {
    pub id: &'static str,
    pub group: &'static str,
    pub label: &'static str,
    pub bridge_state: &'static str,
}

pub fn feature_manifest() -> Vec<CoreFeature> {
    [
        (
            "e2ee-messages",
            "Messaging",
            "End-to-end encrypted messages",
            "source-linked",
        ),
        (
            "dm-ratchet",
            "Messaging",
            "DM Double Ratchet",
            "source-linked",
        ),
        (
            "group-encryption",
            "Messaging",
            "Group and server encryption",
            "guarded",
        ),
        (
            "attachments",
            "Messaging",
            "Encrypted images and attachments",
            "source-linked",
        ),
        (
            "edits-history",
            "Messaging",
            "Encrypted edits and local history",
            "source-linked",
        ),
        (
            "whitelists",
            "Trust",
            "People and scope whitelists",
            "source-linked",
        ),
        (
            "server-defaults",
            "Trust",
            "Group/server defaults and locks",
            "source-linked",
        ),
        (
            "safety-numbers",
            "Trust",
            "Safety numbers and key-change review",
            "source-linked",
        ),
        (
            "membership",
            "Trust",
            "Conversation membership tracking",
            "source-linked",
        ),
        (
            "main-password",
            "Access",
            "Main password and recovery",
            "source-linked",
        ),
        (
            "stealth-password",
            "Access",
            "Stealth password",
            "source-linked",
        ),
        (
            "burn-password",
            "Access",
            "Burn password and lockout",
            "source-linked",
        ),
        (
            "scope-burn",
            "Retention",
            "Conversation/scope burn",
            "source-linked",
        ),
        (
            "account-burn",
            "Retention",
            "Account burn and fresh identity",
            "source-linked",
        ),
        (
            "ttl",
            "Retention",
            "Per-conversation TTL and blob burn",
            "source-linked",
        ),
        (
            "relay",
            "Transport",
            "Ciphertext-only relay",
            "source-linked",
        ),
        (
            "control-inbox",
            "Transport",
            "Control inbox and session repair",
            "source-linked",
        ),
        (
            "account-switching",
            "Accounts",
            "Crash-safe account switching",
            "refactor-required",
        ),
        (
            "import-export",
            "Accounts",
            "Encrypted import and export",
            "source-linked",
        ),
        (
            "license",
            "Lifecycle",
            "Free/Pro license state",
            "source-linked",
        ),
        (
            "updates",
            "Lifecycle",
            "Signed update channels",
            "shell-adapter-required",
        ),
        (
            "screenshot-protection",
            "Device",
            "Windows screenshot protection",
            "shell-adapter-required",
        ),
    ]
    .into_iter()
    .map(|(id, group, label, bridge_state)| CoreFeature {
        id,
        group,
        label,
        bridge_state,
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_the_original_core_without_enabling_unsafe_group_state() {
        let state = HubCoreState::default();
        let status = readiness(&state);
        assert!(status.original_core_linked);
        assert!(!status.identity_loaded);
        assert!(!status.group_sender_keys_enabled);
        assert!(!status.remote_service_has_native_access);
    }

    #[test]
    fn setup_required_is_distinct_from_existing_password_gate() {
        assert_eq!(
            classify_bootstrap_status(true, false, false, true, false, false, false),
            "setupRequired"
        );
        assert_eq!(
            classify_bootstrap_status(true, true, false, true, true, false, true),
            "setupRequired"
        );
        assert_eq!(
            classify_bootstrap_status(true, true, true, false, true, false, false),
            "passwordRequired"
        );
        assert_eq!(
            classify_bootstrap_status(true, true, true, true, true, false, true),
            "failed"
        );
        assert_eq!(
            classify_bootstrap_status(true, true, true, true, true, true, true),
            "ready"
        );
    }

    #[test]
    fn manifest_covers_original_feature_families() {
        let features = feature_manifest();
        assert!(features.len() >= 20);
        assert!(features
            .iter()
            .all(|feature| !feature.label.to_ascii_lowercase().contains("capsule")));
        for required in [
            "e2ee-messages",
            "attachments",
            "whitelists",
            "safety-numbers",
            "main-password",
            "scope-burn",
            "account-burn",
            "ttl",
            "account-switching",
        ] {
            assert!(features.iter().any(|feature| feature.id == required));
        }
    }

    #[test]
    fn activation_codes_are_canonical_and_strictly_bounded() {
        assert_eq!(
            normalize_activation_code("  osl-ab12-cd34-ef56-gh78  ").unwrap(),
            "OSL-AB12-CD34-EF56-GH78"
        );
        assert_eq!(
            normalize_activation_code_for_build("oslq-ab12-cd34-ef56-gh78", true).unwrap(),
            "OSLQ-AB12-CD34-EF56-GH78"
        );
        assert!(normalize_activation_code_for_build("OSLQ-AB12-CD34-EF56-GH78", false,).is_err());
        for invalid in [
            "OSL-AB12-CD34-EF56",
            "OSL-AB12-CD34-EF56-GH7!",
            "OSL-AB12-CD34-EF56-GH789",
            "NOT-AB12-CD34-EF56-GH78",
        ] {
            assert!(normalize_activation_code(invalid).is_err());
        }
    }

    #[test]
    fn license_state_hides_unknown_backend_values() {
        let state = hub_license_state(keystore::LicenseStateDto {
            state: keystore::LicenseState::PaidOfflineGrace,
            raw_status: "SERVER_INTERNAL_DETAIL".to_owned(),
            current_period_end: Some(42),
            last_validated_at: Some(24),
        });
        assert_eq!(state.access, "offlineGrace");
        assert_eq!(state.status, "UNKNOWN");
        assert_eq!(state.current_period_end, Some(42));
    }
}
