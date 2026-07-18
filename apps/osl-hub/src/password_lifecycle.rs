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
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::core_bridge::HubCoreState;

const NATIVE_ID_DOMAIN: &[u8] = b"OSL-NATIVE-IDENTITY-v1";
const NATIVE_ID_HASH_BYTES: usize = 20;
const MAX_RECOVERY_PHRASE_BYTES: usize = 256;

const ACCOUNT_STATE_FILES: &[&str] = &[
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
    let unlocked = !password_status.is_set || ipc::main_password::get_file_storage_key().is_some();
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
        password_status.is_set,
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

pub fn create_native_identity(state: &HubCoreState) -> Result<HubIdentitySetupResult, String> {
    let _lifecycle = state
        .lifecycle_lock
        .lock()
        .map_err(|_| "OSL account lifecycle is unavailable".to_owned())?;
    let current = readiness(state);
    if !current.can_create_identity {
        return Err(
            "OSL identity creation is not available in the current access state".to_owned(),
        );
    }
    let dir = isolated_account_dir()?;
    ensure_empty_identity_slot(&state.osl, &dir)?;
    let sealer = persistent_sealer()?;

    let mut identity = keystore::generate_identity("osl-pending".to_owned());
    identity.user_id = native_user_id(&identity);
    let phrase = identity_recovery_phrase(&identity)?;
    let result = install_identity(
        &state.osl,
        identity,
        &dir,
        sealer.as_ref(),
        Some(phrase),
        !current.main_password_set,
    )?;
    initialise_keyserver(&state.osl, &dir);
    Ok(result)
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
    *state
        .identity
        .lock()
        .map_err(|_| "OSL identity state is unavailable".to_owned())? = Some(identity);
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
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    static FILE_KEY_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("osl-hub-{label}-{}-{nonce}", std::process::id()))
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
}
