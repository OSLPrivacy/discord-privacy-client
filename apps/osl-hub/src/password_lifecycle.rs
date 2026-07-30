//! Trusted-local OSL identity and main-password lifecycle for the app.
//!
//! This module never accepts a platform account identifier. Native OSL IDs are
//! derived from the locally generated identity signing key, so a remote service
//! page cannot select, replace, or bind an OSL identity. The Tauri wrappers for
//! these functions are granted only to the bundled `main` webview.

use std::path::Path;

use bip39::{Language, Mnemonic};
use ipc::AppState;
use keystore::{Identity, Sealer};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::core_bridge::HubCoreState;

const NATIVE_ID_DOMAIN: &[u8] = b"OSL-NATIVE-IDENTITY-v1";
const NATIVE_ID_HASH_BYTES: usize = 20;
const MAX_RECOVERY_PHRASE_BYTES: usize = 256;

const ACCOUNT_STATE_FILES: &[&str] = &[
    "prekeys.json",
    "peer_map.json",
    "whitelist_state.json",
    "sender_key_state.json",
    "channels.json",
    "burned_scopes.json",
    "membership.json",
    "scope_ttl.json",
    "scope_blobs.json",
    "store/messages.sqlite",
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HubPasswordReadiness {
    pub access_state: &'static str,
    pub identity_loaded: bool,
    pub main_password_set: bool,
    pub unlocked: bool,
    pub service_neutral_identity_supported: bool,
    pub can_create_identity: bool,
    pub can_import_identity_phrase: bool,
    pub password_attempts_used: u32,
    pub password_lockout_seconds_remaining: i64,
}

impl std::fmt::Debug for HubIdentitySetupResult {
    /// Never derive Debug here: `identity_recovery_phrase` is the recovery phrase
    /// itself, and `user_id` is an account identifier. Report only whether a phrase
    /// is present, never its value.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HubIdentitySetupResult")
            .field("user_id", &"[REDACTED]")
            .field(
                "identity_recovery_phrase",
                &if self.identity_recovery_phrase.is_some() {
                    "[REDACTED-PRESENT]"
                } else {
                    "none"
                },
            )
            .field("storage_method", &self.storage_method)
            .field("password_setup_required", &self.password_setup_required)
            .finish()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HubIdentitySetupResult {
    pub user_id: String,
    /// Present once for a newly created identity. An import does not echo the
    /// phrase the user supplied.
    pub identity_recovery_phrase: Option<String>,
    pub storage_method: String,
    pub password_setup_required: bool,
}

#[derive(Clone, Copy, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HubIdentityCreationOwnerSignoff {
    pub owner_present: bool,
    pub reviewed_no_existing_identity_replacement: bool,
    pub accepts_recovery_phrase_responsibility: bool,
}

impl std::fmt::Debug for HubIdentityCreationOwnerSignoff {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HubIdentityCreationOwnerSignoff")
            .field("owner_present", &self.owner_present)
            .field(
                "reviewed_no_existing_identity_replacement",
                &self.reviewed_no_existing_identity_replacement,
            )
            .field(
                "accepts_recovery_phrase_responsibility",
                &self.accepts_recovery_phrase_responsibility,
            )
            .finish()
    }
}

impl HubIdentityCreationOwnerSignoff {
    pub fn owner_authorized_for_new_identity() -> Self {
        Self {
            owner_present: true,
            reviewed_no_existing_identity_replacement: true,
            accepts_recovery_phrase_responsibility: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum IdentityCreationOwnerAuthorization {
    ExplicitOwnerSignoff,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum IdentityCreationAuthorizationError {
    OwnerSignoffRequired,
}

impl std::fmt::Display for IdentityCreationAuthorizationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OwnerSignoffRequired => {
                f.write_str("OSL identity creation requires explicit owner authorization")
            }
        }
    }
}

pub fn require_identity_creation_owner_authorization(
    authorization: Option<IdentityCreationOwnerAuthorization>,
) -> Result<IdentityCreationOwnerAuthorization, IdentityCreationAuthorizationError> {
    authorization.ok_or(IdentityCreationAuthorizationError::OwnerSignoffRequired)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HubMainPasswordSetupResult {
    /// The original password-gate recovery phrase. This is distinct from the
    /// OSL identity recovery phrase and is returned only by initial setup.
    pub password_recovery_phrase: String,
    pub encrypted_state_reload_complete: bool,
    pub encrypted_state_reload_issue_count: usize,
    pub readiness: HubPasswordReadiness,
}

pub fn readiness(state: &HubCoreState) -> HubPasswordReadiness {
    let identity_loaded = state
        .osl
        .identity
        .lock()
        .map(|identity| identity.is_some())
        .unwrap_or(false);
    let Ok(password_status) = ipc::commands::cmd_osl_password_status() else {
        return unavailable_readiness(identity_loaded);
    };
    let qa_device_gate =
        cfg!(feature = "discord-qa-shell") && ipc::main_password::get_file_storage_key().is_some();
    let unlocked = qa_device_gate
        || !password_status.is_set
        || ipc::main_password::get_file_storage_key().is_some();
    let lockout = ipc::commands::cmd_osl_lockout_status().ok();
    let remaining = lockout
        .as_ref()
        .and_then(|status| status.password_locked_until.map(|until| until - status.now))
        .unwrap_or(0)
        .max(0);
    let attempts = lockout
        .as_ref()
        .map(|status| status.password_attempts_used)
        .unwrap_or(0);

    readiness_from(
        identity_loaded,
        password_status.is_set || qa_device_gate,
        unlocked,
        attempts,
        remaining,
    )
}

fn readiness_from(
    identity_loaded: bool,
    main_password_set: bool,
    unlocked: bool,
    password_attempts_used: u32,
    password_lockout_seconds_remaining: i64,
) -> HubPasswordReadiness {
    let access_state = match (identity_loaded, main_password_set, unlocked) {
        (false, true, false) | (true, true, false) => "passwordRequired",
        (false, _, true) => "identitySetupRequired",
        (true, false, true) => "passwordSetupRequired",
        (true, true, true) => "ready",
        // A password that is not set is definitionally unlocked. Treat an
        // impossible combination as unavailable rather than guessing.
        (_, false, false) => "unavailable",
    };
    let may_install_identity = !identity_loaded && (!main_password_set || unlocked);
    HubPasswordReadiness {
        access_state,
        identity_loaded,
        main_password_set,
        unlocked,
        service_neutral_identity_supported: true,
        can_create_identity: may_install_identity,
        can_import_identity_phrase: may_install_identity,
        password_attempts_used,
        password_lockout_seconds_remaining,
    }
}

fn unavailable_readiness(identity_loaded: bool) -> HubPasswordReadiness {
    HubPasswordReadiness {
        access_state: "unavailable",
        identity_loaded,
        main_password_set: true,
        unlocked: false,
        service_neutral_identity_supported: true,
        can_create_identity: false,
        can_import_identity_phrase: false,
        password_attempts_used: 0,
        password_lockout_seconds_remaining: 0,
    }
}

pub fn create_native_identity_with_owner_authorization_signoff(
    state: &HubCoreState,
    owner_authorization_signoff: HubIdentityCreationOwnerSignoff,
) -> Result<HubIdentitySetupResult, String> {
    create_native_identity_after_owner_authorization_signoff(
        state,
        owner_authorization_signoff,
    )
}

pub fn create_native_identity(
    state: &HubCoreState,
    authorization: Option<IdentityCreationOwnerAuthorization>,
) -> Result<HubIdentitySetupResult, String> {
    require_identity_creation_owner_authorization(authorization)
        .map_err(|error| error.to_string())?;
    create_native_identity_after_owner_authorization_signoff(
        state,
        HubIdentityCreationOwnerSignoff::owner_authorized_for_new_identity(),
    )
}

fn create_native_identity_after_owner_authorization_signoff(
    state: &HubCoreState,
    owner_authorization_signoff: HubIdentityCreationOwnerSignoff,
) -> Result<HubIdentitySetupResult, String> {
    let _lifecycle = state
        .lifecycle_lock
        .lock()
        .map_err(|_| "OSL account lifecycle is unavailable".to_owned())?;
    let current = readiness(state);
    let dir = isolated_account_dir()?;
    let sealer = persistent_sealer()?;
    let result = create_native_identity_after_owner_authorization_signoff_using(
        state,
        &current,
        &dir,
        sealer.as_ref(),
        owner_authorization_signoff,
    )?;
    initialise_keyserver(&state.osl, &dir);
    Ok(result)
}

fn create_native_identity_after_owner_authorization_signoff_using(
    state: &HubCoreState,
    current: &HubPasswordReadiness,
    dir: &Path,
    sealer: &dyn Sealer,
    owner_authorization_signoff: HubIdentityCreationOwnerSignoff,
) -> Result<HubIdentitySetupResult, String> {
    require_identity_creation_owner_authorization_signoff(owner_authorization_signoff)?;
    if !current.can_create_identity {
        return Err(
            "OSL identity creation is not available in the current access state".to_owned(),
        );
    }
    ensure_empty_identity_slot(&state.osl, dir)?;
    let mut identity = keystore::generate_identity("osl-pending".to_owned());
    identity.user_id = native_user_id(&identity);
    let phrase = identity_recovery_phrase(&identity)?;
    let result = install_identity(
        &state.osl,
        identity,
        dir,
        sealer,
        Some(phrase),
        !current.main_password_set,
    )?;
    Ok(result)
}

fn require_identity_creation_owner_authorization_signoff(
    signoff: HubIdentityCreationOwnerSignoff,
) -> Result<(), String> {
    if signoff.owner_present
        && signoff.reviewed_no_existing_identity_replacement
        && signoff.accepts_recovery_phrase_responsibility
    {
        return Ok(());
    }
    Err("OSL identity creation requires owner authorization sign-off".to_owned())
}

pub fn import_native_identity_phrase(
    state: &HubCoreState,
    phrase: String,
) -> Result<HubIdentitySetupResult, String> {
    let _lifecycle = state
        .lifecycle_lock
        .lock()
        .map_err(|_| "OSL account lifecycle is unavailable".to_owned())?;
    let current = readiness(state);
    if !current.can_import_identity_phrase {
        return Err("OSL identity import is not available in the current access state".to_owned());
    }
    let dir = isolated_account_dir()?;
    ensure_empty_identity_slot(&state.osl, &dir)?;
    let entropy = parse_identity_phrase(&phrase)?;
    let mut identity = keystore::identity_from_entropy(entropy, "osl-pending".to_owned());
    identity.user_id = native_user_id(&identity);
    let sealer = persistent_sealer()?;
    let result = install_identity(
        &state.osl,
        identity,
        &dir,
        sealer.as_ref(),
        None,
        !current.main_password_set,
    )?;
    initialise_keyserver(&state.osl, &dir);
    Ok(result)
}

pub fn setup_main_password(
    state: &HubCoreState,
    password: String,
) -> Result<HubMainPasswordSetupResult, String> {
    ipc::main_password::validate_new_password(&password).map_err(|_| {
        "OSL main password must contain 6 to 128 printable keyboard characters".to_owned()
    })?;
    let _lifecycle = state
        .lifecycle_lock
        .lock()
        .map_err(|_| "OSL account lifecycle is unavailable".to_owned())?;
    let current = readiness(state);
    if !current.identity_loaded {
        return Err("Create or import a local OSL identity before setting its password".to_owned());
    }
    if current.main_password_set {
        return Err("OSL main password is already configured".to_owned());
    }
    let account_dir = isolated_account_dir()?;

    let outcome = setup_main_password_using(state, account_dir.as_path(), || {
        // Use the original IPC command so marker, Argon2id, lockout reset,
        // recovery phrase, and file-key installation cannot drift from OSL.
        ipc::commands::cmd_osl_set_main_password(password)
    })?;
    // Initial password setup happens after first-run bootstrap, so the first
    // bootstrap could not have opened password-protected state (or a store for
    // a just-created identity). Re-run the original bootstrap now that the
    // file key is installed; this is the same production load path used by the
    // original client, not an OSL Privacy-specific partial approximation.
    crate::original_bootstrap::run_autostart(&state.osl);
    initialise_keyserver(&state.osl, account_dir.as_path());
    Ok(HubMainPasswordSetupResult {
        password_recovery_phrase: outcome.password_recovery_phrase,
        encrypted_state_reload_complete: outcome.reload_issue_count == 0,
        encrypted_state_reload_issue_count: outcome.reload_issue_count,
        readiness: readiness(state),
    })
}

/// Verify a locally-entered duress PIN and, only on the burn-password role,
/// run the full fixed-root cleanup path that returns the user-visible report.
pub fn enter_duress_pin_for_full_wipe_report(
    state: &HubCoreState,
    duress_pin: String,
    app_config_dir: &Path,
    app_local_data_dir: &Path,
    service_hosts_shutdown: bool,
) -> Result<crate::cleanup::HubFullCleanupResult, String> {
    ipc::main_password::validate_password(&duress_pin)
        .map_err(|_| "OSL duress PIN was rejected".to_owned())?;
    let _lifecycle = state
        .lifecycle_lock
        .lock()
        .map_err(|_| "OSL account lifecycle is unavailable".to_owned())?;
    let password_dir = keystore::osl_base_dir()
        .map_err(|_| "OSL password gate storage is unavailable".to_owned())?;
    let marker = ipc::main_password::read_marker_pub(&password_dir)
        .map_err(|_| "OSL password gate storage is unavailable".to_owned())?;
    match ipc::main_password::verify_gate_password_with_marker(&marker, &duress_pin)
        .map_err(|_| "OSL password gate storage is unavailable".to_owned())?
    {
        ipc::main_password::GateMatch::Burn => {}
        ipc::main_password::GateMatch::Wrong => {
            return Err("OSL duress PIN was rejected".to_owned())
        }
        // GateMatch::Main now carries the derived file_storage_key. This is a
        // refusal path, so bind it with `_` and let it drop immediately: a main
        // password must not yield usable key material on the duress route.
        ipc::main_password::GateMatch::Main(_)
        | ipc::main_password::GateMatch::Stealth
        | ipc::main_password::GateMatch::Duress => {
            return Err("OSL duress action requires the burn password".to_owned())
        }
    }

    let verification = crate::startup_gate::verify_password_role(state, duress_pin)?;
    match verification.role {
        crate::startup_gate::VerifiedGateRole::Burn => crate::cleanup::execute_verified_gate_burn(
            state,
            app_config_dir,
            app_local_data_dir,
            service_hosts_shutdown,
        ),
        crate::startup_gate::VerifiedGateRole::Wrong => {
            Err("OSL duress PIN was rejected".to_owned())
        }
        crate::startup_gate::VerifiedGateRole::Duress => {
            Err("OSL duress action requires the burn password".to_owned())
        }
        crate::startup_gate::VerifiedGateRole::Main
        | crate::startup_gate::VerifiedGateRole::Stealth => {
            Err("OSL duress action requires the burn password".to_owned())
        }
    }
}

struct PasswordSetupOutcome {
    password_recovery_phrase: String,
    reload_issue_count: usize,
}

fn setup_main_password_using<F>(
    state: &HubCoreState,
    account_dir: &Path,
    set_password: F,
) -> Result<PasswordSetupOutcome, String>
where
    F: FnOnce() -> Result<String, String>,
{
    let phrase = set_password()?;
    let report = ipc::state_reload::reload_encrypted_state_after_unlock(&state.osl, account_dir)
        .map_err(|_| {
            "OSL password was set, but encrypted state reload could not start".to_owned()
        })?;
    let issue_count = report.errors.len();
    Ok(PasswordSetupOutcome {
        password_recovery_phrase: phrase,
        reload_issue_count: issue_count,
    })
}

fn isolated_account_dir() -> Result<std::path::PathBuf, String> {
    keystore::osl_config_dir().map_err(|_| "OSL Privacy account storage is unavailable".to_owned())
}

pub(crate) fn persistent_sealer() -> Result<Box<dyn Sealer>, String> {
    let sealer = keystore::select_best_sealer();
    match sealer.method_label() {
        keystore::sealer::METHOD_TPM | keystore::sealer::METHOD_KEYRING => Ok(sealer),
        _ => Err(
            "Persistent TPM or operating-system credential storage is unavailable; identity was not created"
                .to_owned(),
        ),
    }
}

fn ensure_empty_identity_slot(state: &AppState, dir: &Path) -> Result<(), String> {
    if state
        .identity
        .lock()
        .map_err(|_| "OSL identity state is unavailable".to_owned())?
        .is_some()
    {
        return Err("An OSL identity is already loaded".to_owned());
    }
    if dir.join("identity.json").exists() {
        return Err("A sealed OSL identity already exists but could not be loaded".to_owned());
    }
    if ACCOUNT_STATE_FILES
        .iter()
        .any(|relative| dir.join(relative).exists())
    {
        return Err(
            "Existing OSL account data requires full account recovery; no identity was replaced"
                .to_owned(),
        );
    }
    Ok(())
}

fn install_identity(
    state: &AppState,
    identity: Identity,
    dir: &Path,
    sealer: &dyn Sealer,
    identity_recovery_phrase: Option<String>,
    password_setup_required: bool,
) -> Result<HubIdentitySetupResult, String> {
    std::fs::create_dir_all(dir)
        .map_err(|_| "OSL identity directory could not be created".to_owned())?;
    let path = dir.join("identity.json");
    keystore::save_identity(&path, &identity, sealer)
        .map_err(|_| "OSL identity could not be sealed to device storage".to_owned())?;
    let user_id = identity.user_id.clone();
    state
        .try_install_identity(identity)
        .map_err(|_| "OSL identity state is unavailable".to_owned())?;
    Ok(HubIdentitySetupResult {
        user_id,
        identity_recovery_phrase,
        storage_method: sealer.method_label().to_owned(),
        password_setup_required,
    })
}

pub(crate) fn native_user_id(identity: &Identity) -> String {
    let mut hash = Sha256::new();
    hash.update(NATIVE_ID_DOMAIN);
    hash.update(identity.ed25519_public.as_bytes());
    hash.update(identity.x25519_public.as_bytes());
    let digest = hash.finalize();
    let mut encoded = String::with_capacity(4 + NATIVE_ID_HASH_BYTES * 2);
    encoded.push_str("osl_");
    for byte in &digest[..NATIVE_ID_HASH_BYTES] {
        use std::fmt::Write as _;
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

pub(crate) fn identity_recovery_phrase(identity: &Identity) -> Result<String, String> {
    let entropy = identity
        .recovery_entropy
        .ok_or_else(|| "OSL identity recovery material is unavailable".to_owned())?;
    Mnemonic::from_entropy_in(Language::English, &entropy)
        .map(|mnemonic| mnemonic.to_string())
        .map_err(|_| "OSL identity recovery phrase could not be created".to_owned())
}

pub(crate) fn parse_identity_phrase(phrase: &str) -> Result<[u8; 16], String> {
    if phrase.len() > MAX_RECOVERY_PHRASE_BYTES {
        return Err("OSL identity recovery phrase is invalid".to_owned());
    }
    let mnemonic =
        Mnemonic::parse_in_normalized(Language::English, phrase.trim()).map_err(|_| {
            "OSL identity recovery phrase must contain exactly twelve valid words".to_owned()
        })?;
    let entropy = mnemonic.to_entropy();
    if entropy.len() != 16 {
        return Err(
            "OSL identity recovery phrase must contain exactly twelve valid words".to_owned(),
        );
    }
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&entropy);
    Ok(bytes)
}

fn initialise_keyserver(state: &AppState, dir: &Path) {
    let base_url = ipc::commands::resolve_keyserver_base_url(dir);
    ipc::commands::ensure_keyserver_registered(state, &base_url, None);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    // Deliberately the crate-wide lock, not a private one. Password setup mutates
    // the process-wide unlocked-key and base-dir statics, so serialising only
    // against this module's own tests is no protection at all: a sibling test in
    // another module holding `GLOBAL_KEYSTORE_TEST_LOCK` would still run
    // concurrently and read back the wrong key. Two mutexes over one global is
    // the same as none.
    use crate::GLOBAL_KEYSTORE_TEST_LOCK as FILE_KEY_TEST_LOCK;

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("osl-hub-{label}-{}-{nonce}", std::process::id()))
    }

    struct KeystoreGlobalReset;

    impl Drop for KeystoreGlobalReset {
        fn drop(&mut self) {
            ipc::main_password::set_file_storage_key(None);
            keystore::set_active_account_dir(None);
            keystore::set_base_dir_override(None);
        }
    }

    fn assert_removed_target(report: &crate::cleanup::HubFullCleanupResult, target: &str) {
        assert!(
            report
                .removed_targets
                .iter()
                .any(|removed| removed == target),
            "cleanup report did not include removed target {target}; report={:?}",
            report.removed_targets
        );
        assert!(
            !report.failed_targets.iter().any(|failed| failed == target),
            "cleanup report included failed target {target}; report={:?}",
            report.failed_targets
        );
    }

    #[test]
    fn readiness_distinguishes_identity_password_setup_and_unlock() {
        assert_eq!(
            readiness_from(false, false, true, 0, 0).access_state,
            "identitySetupRequired"
        );
        assert_eq!(
            readiness_from(true, false, true, 0, 0).access_state,
            "passwordSetupRequired"
        );
        assert_eq!(
            readiness_from(true, true, false, 2, 30).access_state,
            "passwordRequired"
        );
        assert_eq!(readiness_from(true, true, true, 0, 0).access_state, "ready");
    }

    #[test]
    fn native_id_is_stable_and_not_a_platform_identifier() {
        let a = keystore::identity_from_entropy([7; 16], "discord-123".to_owned());
        let b = keystore::identity_from_entropy([7; 16], "instagram-456".to_owned());
        let id = native_user_id(&a);
        assert_eq!(id, native_user_id(&b));
        assert!(id.starts_with("osl_"));
        assert!(!id.contains("discord"));
        assert!(!id.contains("123"));
    }

    #[test]
    fn phrase_round_trip_recreates_native_identity() {
        let identity = keystore::identity_from_entropy([11; 16], "ignored".to_owned());
        let phrase = identity_recovery_phrase(&identity).unwrap();
        let recovered = keystore::identity_from_entropy(
            parse_identity_phrase(&phrase).unwrap(),
            "ignored-again".to_owned(),
        );
        assert_eq!(native_user_id(&identity), native_user_id(&recovered));
        assert_eq!(phrase.split_whitespace().count(), 12);
    }

    #[test]
    fn identity_creation_requires_explicit_owner_authorization_token() {
        assert_eq!(
            require_identity_creation_owner_authorization(None),
            Err(IdentityCreationAuthorizationError::OwnerSignoffRequired)
        );
        let state = HubCoreState::default();
        assert_eq!(
            create_native_identity(&state, None).unwrap_err(),
            "OSL identity creation requires explicit owner authorization"
        );
        assert_eq!(
            require_identity_creation_owner_authorization(Some(
                IdentityCreationOwnerAuthorization::ExplicitOwnerSignoff
            )),
            Ok(IdentityCreationOwnerAuthorization::ExplicitOwnerSignoff)
        );
    }

    #[test]
    fn identity_install_is_device_sealed_and_refuses_replacement() {
        let dir = temp_dir("identity");
        let state = AppState::new();
        let sealer = keystore::MemorySealer::new();
        let mut identity = keystore::identity_from_entropy([13; 16], "pending".to_owned());
        identity.user_id = native_user_id(&identity);
        let user_id = identity.user_id.clone();
        install_identity(&state, identity, &dir, &sealer, None, true).unwrap();
        assert_eq!(
            keystore::load_identity(&dir.join("identity.json"), &sealer)
                .unwrap()
                .user_id,
            user_id
        );
        assert!(ensure_empty_identity_slot(&AppState::new(), &dir).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn identity_creation_requires_owner_authorization_signoff() {
        let dir = temp_dir("identity-signoff");
        let state = HubCoreState::default();
        let current = readiness_from(false, false, true, 0, 0);
        let sealer = keystore::MemorySealer::new();

        let missing_owner = HubIdentityCreationOwnerSignoff {
            owner_present: false,
            reviewed_no_existing_identity_replacement: true,
            accepts_recovery_phrase_responsibility: true,
        };
        let error = match create_native_identity_after_owner_authorization_signoff_using(
            &state,
            &current,
            &dir,
            &sealer,
            missing_owner,
        ) {
            Ok(_) => panic!("identity creation must refuse without owner sign-off"),
            Err(error) => error,
        };
        assert!(error.contains("owner authorization sign-off"));
        assert!(!dir.join("identity.json").exists());
        assert!(state.osl.identity.lock().unwrap().is_none());

        let result = create_native_identity_after_owner_authorization_signoff_using(
            &state,
            &current,
            &dir,
            &sealer,
            HubIdentityCreationOwnerSignoff::owner_authorized_for_new_identity(),
        )
        .unwrap();
        assert!(result.user_id.starts_with("osl_"));
        assert_eq!(
            result
                .identity_recovery_phrase
                .as_deref()
                .unwrap()
                .split_whitespace()
                .count(),
            12
        );
        assert!(dir.join("identity.json").exists());
        assert!(state.osl.identity.lock().unwrap().is_some());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn password_setup_uses_only_temp_paths_and_reloads_state() {
        let _guard = FILE_KEY_TEST_LOCK.lock().unwrap();
        let dir = temp_dir("password");
        let state = HubCoreState::default();
        *state.osl.identity.lock().unwrap() = Some(keystore::identity_from_entropy(
            [17; 16],
            "osl_test".to_owned(),
        ));
        let result = setup_main_password_using(&state, &dir, || {
            ipc::main_password::set_main_password(&dir, "aB3!z9")
        })
        .unwrap();
        assert_eq!(
            result.password_recovery_phrase.split_whitespace().count(),
            12
        );
        assert_eq!(result.reload_issue_count, 0);
        assert!(dir.join("password_marker.json").exists());
        ipc::main_password::set_file_storage_key(None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn entering_duress_pin_triggers_full_wipe_report() {
        let _guard = FILE_KEY_TEST_LOCK.lock().unwrap();
        let _reset = KeystoreGlobalReset;
        let config_dir = temp_dir("duress-config");
        let local_data_dir = temp_dir("duress-local");
        let core_dir = config_dir.join("osl-core");
        let service_profiles = local_data_dir.join("service-profiles-v2");
        let native_profiles = local_data_dir.join("native-window-profiles-v1");
        std::fs::create_dir_all(&core_dir).unwrap();
        std::fs::create_dir_all(&service_profiles).unwrap();
        std::fs::create_dir_all(&native_profiles).unwrap();
        std::fs::write(core_dir.join("peer_map.json"), br#"{}"#).unwrap();
        std::fs::write(core_dir.join("whitelist_state.json"), br#"{}"#).unwrap();
        std::fs::write(
            service_profiles.join("profile-cache"),
            b"local profile bytes",
        )
        .unwrap();
        std::fs::write(
            native_profiles.join("native-cache"),
            b"native profile bytes",
        )
        .unwrap();
        std::fs::write(config_dir.join("service-registry.json"), br#"{}"#).unwrap();
        std::fs::write(config_dir.join("service-scope-index.json"), br#"{}"#).unwrap();
        std::fs::write(config_dir.join("preview-preferences.json"), br#"{}"#).unwrap();

        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(Some(core_dir.clone()));
        let state = HubCoreState::default();
        *state.osl.identity.lock().unwrap() = Some(keystore::identity_from_entropy(
            [29; 16],
            "osl_test_duress".to_owned(),
        ));

        let main_password = "main-pin-7421";
        let duress_pin = "duress-pin-9381";
        ipc::commands::cmd_osl_set_main_password(main_password.to_owned()).unwrap();
        ipc::commands::cmd_osl_set_burn_password(main_password.to_owned(), duress_pin.to_owned())
            .unwrap();
        assert!(core_dir.join("password_marker.json").exists());
        ipc::main_password::set_file_storage_key(None);

        let ordinary_unlock_error = enter_duress_pin_for_full_wipe_report(
            &state,
            main_password.to_owned(),
            &config_dir,
            &local_data_dir,
            true,
        )
        .expect_err("ordinary main password must not trigger a full wipe");
        assert!(ordinary_unlock_error.contains("burn password"));
        assert!(core_dir.exists());
        assert!(service_profiles.exists());
        ipc::main_password::set_file_storage_key(None);

        let report = enter_duress_pin_for_full_wipe_report(
            &state,
            duress_pin.to_owned(),
            &config_dir,
            &local_data_dir,
            true,
        )
        .unwrap();
        assert!(report.local_cleanup_complete);
        assert!(report.failed_targets.is_empty());
        assert!(!report.restart_required);
        assert!(report.original_discord_data_untouched);
        for target in [
            "hub_core",
            "service_profiles",
            "native_profiles",
            "service_registry",
            "service_scope_index",
            "preview_preferences",
        ] {
            assert_removed_target(&report, target);
        }
        assert!(!core_dir.exists());
        assert!(!service_profiles.exists());
        assert!(!native_profiles.exists());
        assert!(!config_dir.join("service-registry.json").exists());
        assert!(!config_dir.join("service-scope-index.json").exists());
        assert!(!config_dir.join("preview-preferences.json").exists());
        assert!(ipc::main_password::get_file_storage_key().is_none());

        let _ = std::fs::remove_dir_all(config_dir);
        let _ = std::fs::remove_dir_all(local_data_dir);
    }
}
