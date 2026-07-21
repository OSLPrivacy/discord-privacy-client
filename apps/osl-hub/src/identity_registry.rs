//! Durable, OSL-native multiple-identity lifecycle.
//!
//! Only one identity is active process-wide. Slot metadata is encrypted by
//! the device main-password file key; the pre-bootstrap marker contains only
//! an opaque slot id so startup can select a directory while still locked.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Mutex;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::core_bridge::HubCoreState;

const REGISTRY_FILE: &str = "hub_identity_registry.json";
const ACTIVE_MARKER_FILE: &str = "hub-active-identity";
const IDENTITIES_DIR: &str = "hub-identities";
const REGISTRY_VERSION: u32 = 1;
const MAX_IDENTITIES: usize = 16;
const MAX_LABEL_BYTES: usize = 80;
const MAX_REGISTRY_BYTES: u64 = 128 * 1024;
const SLOT_DOMAIN: &[u8] = b"OSL-HUB-IDENTITY-SLOT-v1";

const ACCOUNT_ARTIFACTS: &[&str] = &[
    "identity.json",
    "peer_map.json",
    "whitelist_state.json",
    "sender_key_state.json",
    "channels.json",
    "burned_scopes.json",
    "membership.json",
    "scope_ttl.json",
    "scope_blobs.json",
    "pending_rotation.json",
    "pending_invitations.json",
    "hub_local_protected.json",
    "hub_people.json",
    "hub_profile.json",
    "hub_security_preferences.json",
    "store",
];

#[derive(Debug, Default)]
pub struct HubIdentityRegistryState {
    transition: Mutex<()>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HubIdentitySlotDto {
    pub slot_id: String,
    pub label: String,
    pub osl_user_id: String,
    pub safety_number: String,
    pub active: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HubIdentitySlotCreation {
    pub identity: HubIdentitySlotDto,
    /// Returned exactly once for a newly generated slot. Recovery/import does
    /// not echo the phrase supplied by the user.
    pub identity_recovery_phrase: Option<String>,
    pub storage_method: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HubIdentitySwitchResult {
    pub previous_slot_id: String,
    pub active_identity: HubIdentitySlotDto,
    pub context_invalidation_required: bool,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteUnregisterState {
    Succeeded,
    Failed,
    Unavailable,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HubIdentityBurnResult {
    pub burned_slot_id: String,
    pub remote_unregister: RemoteUnregisterState,
    pub local_identity_detached: bool,
    pub local_cleanup_complete: bool,
    pub cleanup_pending: bool,
    pub next_active_identity: Option<HubIdentitySlotDto>,
    pub context_invalidation_required: bool,
    pub original_discord_data_untouched: bool,
}

#[derive(Clone, Serialize, Deserialize)]
struct IdentitySlotRecord {
    slot_id: String,
    label: String,
    osl_user_id: String,
    ed25519_public_b64: String,
    created_at: i64,
}

#[derive(Default, Serialize, Deserialize)]
struct IdentityRegistryFile {
    version: u32,
    #[serde(default)]
    slots: BTreeMap<String, IdentitySlotRecord>,
}

/// Select the persisted opaque identity directory before the original core
/// bootstrap runs. This reads no encrypted metadata and loads no key material.
pub fn select_active_identity_before_bootstrap() -> Result<Option<String>, String> {
    let base = base_dir()?;
    let selected = select_slot_from_disk(&base);
    match selected {
        Some(slot_id) => {
            validate_slot_id(&slot_id)?;
            let dir = slot_dir(&base, &slot_id)?;
            keystore::set_active_account_dir(Some(dir));
            Ok(Some(slot_id))
        }
        None => {
            keystore::set_active_account_dir(None);
            Ok(None)
        }
    }
}

fn select_slot_from_disk(base: &Path) -> Option<String> {
    read_active_marker(base)
        .filter(|slot_id| {
            slot_dir(base, slot_id).is_ok_and(|dir| dir.join("identity.json").is_file())
        })
        .or_else(|| single_recoverable_slot(base))
}

pub fn list_identity_slots(
    core: &HubCoreState,
    registry_state: &HubIdentityRegistryState,
) -> Result<Vec<HubIdentitySlotDto>, String> {
    require_unlocked()?;
    let _transition = registry_state
        .transition
        .lock()
        .map_err(|_| "OSL identity registry is unavailable".to_owned())?;
    let _lifecycle = core
        .lifecycle_lock
        .lock()
        .map_err(|_| "OSL account lifecycle is unavailable".to_owned())?;
    let _account = core
        .osl
        .account_switch_lock
        .lock()
        .map_err(|_| "OSL identity switch is unavailable".to_owned())?;
    let base = base_dir()?;
    let registry = ensure_current_identity_slotted(core, &base)?;
    let active = active_slot_required(&base)?;
    Ok(registry
        .slots
        .values()
        .map(|record| slot_dto(record, record.slot_id == active))
        .collect())
}

pub fn create_identity_slot(
    core: &HubCoreState,
    registry_state: &HubIdentityRegistryState,
    label: String,
) -> Result<HubIdentitySlotCreation, String> {
    create_or_recover_identity_slot(core, registry_state, label, None)
}

pub fn recover_identity_slot(
    core: &HubCoreState,
    registry_state: &HubIdentityRegistryState,
    label: String,
    identity_recovery_phrase: String,
) -> Result<HubIdentitySlotCreation, String> {
    create_or_recover_identity_slot(core, registry_state, label, Some(identity_recovery_phrase))
}

pub fn switch_identity_slot(
    core: &HubCoreState,
    registry_state: &HubIdentityRegistryState,
    slot_id: String,
) -> Result<HubIdentitySwitchResult, String> {
    require_unlocked()?;
    validate_slot_id(&slot_id)?;
    let _transition = registry_state
        .transition
        .lock()
        .map_err(|_| "OSL identity registry is unavailable".to_owned())?;
    let _lifecycle = core
        .lifecycle_lock
        .lock()
        .map_err(|_| "OSL account lifecycle is unavailable".to_owned())?;
    let _account = core
        .osl
        .account_switch_lock
        .lock()
        .map_err(|_| "OSL identity switch is unavailable".to_owned())?;
    let base = base_dir()?;
    let registry = ensure_current_identity_slotted(core, &base)?;
    let previous = active_slot_required(&base)?;
    let target = registry
        .slots
        .get(&slot_id)
        .ok_or_else(|| "OSL identity slot is unknown".to_owned())?;
    if previous == slot_id {
        return Ok(HubIdentitySwitchResult {
            previous_slot_id: previous,
            active_identity: slot_dto(target, true),
            context_invalidation_required: false,
        });
    }
    let target_dir = slot_dir(&base, &slot_id)?;
    validate_slot_identity(&target_dir, target)?;
    let previous_dir = slot_dir(&base, &previous)?;

    reset_account_scoped_state(&core.osl);
    keystore::set_active_account_dir(Some(target_dir));
    if let Err(error) = write_active_marker(&base, &slot_id) {
        keystore::set_active_account_dir(Some(previous_dir));
        crate::original_bootstrap::run_autostart(&core.osl);
        return Err(error);
    }
    crate::original_bootstrap::run_autostart(&core.osl);
    let loaded = active_user_id(core).ok();
    if loaded.as_deref() != Some(target.osl_user_id.as_str()) {
        reset_account_scoped_state(&core.osl);
        keystore::set_active_account_dir(Some(previous_dir));
        let _ = write_active_marker(&base, &previous);
        crate::original_bootstrap::run_autostart(&core.osl);
        return Err(
            "OSL identity switch failed closed because the loaded identity did not match"
                .to_owned(),
        );
    }
    Ok(HubIdentitySwitchResult {
        previous_slot_id: previous,
        active_identity: slot_dto(target, true),
        context_invalidation_required: true,
    })
}

/// Permanently detach and delete the active OSL Privacy identity. Keyserver
/// unregister is attempted first and reported honestly, but a network failure
/// does not retain local key material against the user's burn request.
pub fn burn_active_identity(
    core: &HubCoreState,
    registry_state: &HubIdentityRegistryState,
) -> Result<HubIdentityBurnResult, String> {
    require_unlocked()?;
    let _transition = registry_state
        .transition
        .lock()
        .map_err(|_| "OSL identity registry is unavailable".to_owned())?;
    let _lifecycle = core
        .lifecycle_lock
        .lock()
        .map_err(|_| "OSL account lifecycle is unavailable".to_owned())?;
    let _account = core
        .osl
        .account_switch_lock
        .lock()
        .map_err(|_| "OSL identity switch is unavailable".to_owned())?;
    let base = base_dir()?;
    let mut registry = ensure_current_identity_slotted(core, &base)?;
    let active = active_slot_required(&base)?;
    let active_record =
        registry.slots.get(&active).cloned().ok_or_else(|| {
            "OSL active identity is missing from its encrypted registry".to_owned()
        })?;
    let identity = core
        .osl
        .identity
        .lock()
        .map_err(|_| "OSL identity state is unavailable".to_owned())?
        .clone()
        .ok_or_else(|| "OSL identity is not loaded".to_owned())?;
    if identity.user_id != active_record.osl_user_id {
        return Err("OSL active identity does not match its encrypted registry".to_owned());
    }
    let remote_unregister = attempt_unregister(
        &identity,
        core.osl
            .keyserver
            .lock()
            .map_err(|_| "OSL keyserver state is unavailable".to_owned())?
            .clone(),
    );

    let next = registry
        .slots
        .iter()
        .find(|(slot_id, _)| *slot_id != &active)
        .map(|(slot_id, record)| (slot_id.clone(), record.clone()));
    if let Some((slot_id, record)) = &next {
        validate_slot_identity(&slot_dir(&base, slot_id)?, record)?;
    }

    let active_dir = slot_dir(&base, &active)?;
    let tombstone = base.join(IDENTITIES_DIR).join(format!(
        ".burned-{active}-{}-{}",
        std::process::id(),
        ipc::main_password::now_unix_secs_pub()
    ));
    if tombstone.exists() {
        return Err("OSL identity burn has a conflicting cleanup tombstone".to_owned());
    }
    let taken = core
        .osl
        .message_store
        .lock()
        .map_err(|_| "OSL message store is unavailable".to_owned())?
        .take();
    drop(taken);
    std::fs::rename(&active_dir, &tombstone)
        .map_err(|_| "OSL identity could not be detached for deletion".to_owned())?;

    let prior_registry = IdentityRegistryFile {
        version: registry.version,
        slots: registry.slots.clone(),
    };
    registry.slots.remove(&active);
    let marker_result = match &next {
        Some((slot_id, _)) => write_active_marker(&base, slot_id),
        None => remove_active_marker(&base),
    };
    if let Err(error) = marker_result {
        let _ = std::fs::rename(&tombstone, &active_dir);
        crate::original_bootstrap::run_autostart(&core.osl);
        return Err(error);
    }
    if let Err(error) = write_registry(&base, &registry) {
        let _ = write_registry(&base, &prior_registry);
        let _ = write_active_marker(&base, &active);
        let _ = std::fs::rename(&tombstone, &active_dir);
        crate::original_bootstrap::run_autostart(&core.osl);
        return Err(error);
    }

    reset_account_scoped_state(&core.osl);
    let next_active_identity = match next {
        Some((slot_id, record)) => {
            keystore::set_active_account_dir(Some(slot_dir(&base, &slot_id)?));
            crate::original_bootstrap::run_autostart(&core.osl);
            if active_user_id(core).ok().as_deref() != Some(record.osl_user_id.as_str()) {
                reset_account_scoped_state(&core.osl);
                keystore::set_active_account_dir(None);
                None
            } else {
                Some(slot_dto(&record, true))
            }
        }
        None => {
            keystore::set_active_account_dir(None);
            None
        }
    };
    let cleanup_pending = std::fs::remove_dir_all(&tombstone).is_err();
    Ok(HubIdentityBurnResult {
        burned_slot_id: active,
        remote_unregister,
        local_identity_detached: true,
        local_cleanup_complete: !cleanup_pending,
        cleanup_pending,
        next_active_identity,
        context_invalidation_required: true,
        original_discord_data_untouched: true,
    })
}

fn create_or_recover_identity_slot(
    core: &HubCoreState,
    registry_state: &HubIdentityRegistryState,
    label: String,
    recovery_phrase: Option<String>,
) -> Result<HubIdentitySlotCreation, String> {
    require_unlocked()?;
    validate_label(&label)?;
    let _transition = registry_state
        .transition
        .lock()
        .map_err(|_| "OSL identity registry is unavailable".to_owned())?;
    let _lifecycle = core
        .lifecycle_lock
        .lock()
        .map_err(|_| "OSL account lifecycle is unavailable".to_owned())?;
    let _account = core
        .osl
        .account_switch_lock
        .lock()
        .map_err(|_| "OSL identity switch is unavailable".to_owned())?;
    let base = base_dir()?;
    let mut registry = ensure_current_identity_slotted(core, &base)?;
    if registry.slots.len() >= MAX_IDENTITIES {
        return Err(format!(
            "OSL supports at most {MAX_IDENTITIES} local identities"
        ));
    }
    let (mut identity, phrase_to_return) = match recovery_phrase {
        Some(phrase) => {
            let entropy = crate::password_lifecycle::parse_identity_phrase(&phrase)?;
            (
                keystore::identity_from_entropy(entropy, "osl-pending".to_owned()),
                None,
            )
        }
        None => {
            let identity = keystore::generate_identity("osl-pending".to_owned());
            let phrase = crate::password_lifecycle::identity_recovery_phrase(&identity)?;
            (identity, Some(phrase))
        }
    };
    identity.user_id = crate::password_lifecycle::native_user_id(&identity);
    if registry
        .slots
        .values()
        .any(|record| record.osl_user_id == identity.user_id)
    {
        return Err("OSL recovery phrase already belongs to a local identity".to_owned());
    }
    let slot_id = slot_id_for_identity(&identity);
    let record = record_for_identity(&identity, slot_id.clone(), label);
    let dir = slot_dir(&base, &slot_id)?;
    if dir.exists() {
        return Err("OSL identity slot already exists".to_owned());
    }
    let parent = dir
        .parent()
        .ok_or_else(|| "OSL identity storage is invalid".to_owned())?;
    std::fs::create_dir_all(parent)
        .map_err(|_| "OSL identity storage could not be created".to_owned())?;
    let stage = parent.join(format!(".{slot_id}.create-{}.tmp", std::process::id()));
    if stage.exists() {
        return Err("OSL identity creation has an unfinished prior transaction".to_owned());
    }
    std::fs::create_dir(&stage)
        .map_err(|_| "OSL identity staging directory could not be created".to_owned())?;
    let sealer = crate::password_lifecycle::persistent_sealer()?;
    if let Err(error) =
        keystore::save_identity(&stage.join("identity.json"), &identity, sealer.as_ref())
    {
        let _ = std::fs::remove_dir_all(&stage);
        return Err(format!("OSL identity could not be sealed: {error}"));
    }
    if let Err(error) = std::fs::rename(&stage, &dir) {
        let _ = std::fs::remove_dir_all(&stage);
        return Err(format!("OSL identity slot could not be committed: {error}"));
    }
    registry.slots.insert(slot_id.clone(), record.clone());
    if let Err(error) = write_registry(&base, &registry) {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(error);
    }
    Ok(HubIdentitySlotCreation {
        identity: slot_dto(&record, false),
        identity_recovery_phrase: phrase_to_return,
        storage_method: sealer.method_label().to_owned(),
    })
}

fn ensure_current_identity_slotted(
    core: &HubCoreState,
    base: &Path,
) -> Result<IdentityRegistryFile, String> {
    let identity = core
        .osl
        .identity
        .lock()
        .map_err(|_| "OSL identity state is unavailable".to_owned())?
        .clone()
        .ok_or_else(|| "OSL identity is not loaded".to_owned())?;
    let mut registry = load_registry(base)?;
    let current_marker = read_active_marker(base);
    let slot_id = slot_id_for_identity(&identity);
    if current_marker.as_deref() != Some(slot_id.as_str()) {
        if keystore::active_account_dir().is_none() {
            migrate_flat_account_to_slot(core, base, &slot_id)?;
        } else {
            let current_dir = keystore::active_account_dir()
                .ok_or_else(|| "OSL active identity directory is unavailable".to_owned())?;
            if current_dir != slot_dir(base, &slot_id)? {
                return Err("OSL active identity directory does not match its key".to_owned());
            }
            write_active_marker(base, &slot_id)?;
        }
    }
    registry
        .slots
        .entry(slot_id.clone())
        .or_insert_with(|| record_for_identity(&identity, slot_id, "Primary identity".to_owned()));
    reconcile_slot_directories(base, &mut registry)?;
    registry.version = REGISTRY_VERSION;
    write_registry(base, &registry)?;
    Ok(registry)
}

fn migrate_flat_account_to_slot(
    core: &HubCoreState,
    base: &Path,
    slot_id: &str,
) -> Result<(), String> {
    let final_dir = slot_dir(base, slot_id)?;
    if final_dir.exists() {
        return Err("OSL target identity slot already exists".to_owned());
    }
    let slots = base.join(IDENTITIES_DIR);
    std::fs::create_dir_all(&slots)
        .map_err(|_| "OSL identity slot storage could not be created".to_owned())?;
    let stage = slots.join(format!(".{slot_id}.migrate-{}.tmp", std::process::id()));
    if stage.exists() {
        return Err("OSL identity migration has an unfinished prior transaction".to_owned());
    }
    std::fs::create_dir(&stage).map_err(|_| "OSL identity migration could not start".to_owned())?;
    let taken = core
        .osl
        .message_store
        .lock()
        .map_err(|_| "OSL message store is unavailable".to_owned())?
        .take();
    drop(taken);
    let mut moved = Vec::new();
    for relative in ACCOUNT_ARTIFACTS {
        let source = base.join(relative);
        if !source.exists() {
            continue;
        }
        let destination = stage.join(relative);
        if let Some(parent) = destination.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                rollback_moved(&stage, base, &moved);
                let _ = std::fs::remove_dir_all(&stage);
                crate::original_bootstrap::run_autostart(&core.osl);
                return Err("OSL identity migration could not create a directory".to_owned());
            }
        }
        if let Err(error) = std::fs::rename(&source, &destination) {
            rollback_moved(&stage, base, &moved);
            let _ = std::fs::remove_dir_all(&stage);
            crate::original_bootstrap::run_autostart(&core.osl);
            return Err(format!("OSL identity migration failed: {error}"));
        }
        moved.push((*relative).to_owned());
    }
    if !stage.join("identity.json").is_file() {
        rollback_moved(&stage, base, &moved);
        let _ = std::fs::remove_dir_all(&stage);
        crate::original_bootstrap::run_autostart(&core.osl);
        return Err("OSL identity migration found no sealed identity".to_owned());
    }
    if let Err(error) = std::fs::rename(&stage, &final_dir) {
        rollback_moved(&stage, base, &moved);
        let _ = std::fs::remove_dir_all(&stage);
        crate::original_bootstrap::run_autostart(&core.osl);
        return Err(format!("OSL identity migration could not commit: {error}"));
    }
    if let Err(error) = write_active_marker(base, slot_id) {
        let _ = std::fs::rename(&final_dir, &stage);
        rollback_moved(&stage, base, &moved);
        let _ = std::fs::remove_dir_all(&stage);
        crate::original_bootstrap::run_autostart(&core.osl);
        return Err(error);
    }
    reset_account_scoped_state(&core.osl);
    keystore::set_active_account_dir(Some(final_dir));
    crate::original_bootstrap::run_autostart(&core.osl);
    let loaded_slot = core
        .osl
        .identity
        .lock()
        .ok()
        .and_then(|identity| identity.as_ref().map(slot_id_for_identity));
    if loaded_slot.as_deref() != Some(slot_id) {
        reset_account_scoped_state(&core.osl);
        keystore::set_active_account_dir(None);
        let _ = remove_active_marker(base);
        let _ = std::fs::rename(slot_dir(base, slot_id)?, &stage);
        rollback_moved(&stage, base, &moved);
        let _ = std::fs::remove_dir_all(&stage);
        crate::original_bootstrap::run_autostart(&core.osl);
        return Err("OSL identity migration failed closed during reload".to_owned());
    }
    Ok(())
}

fn rollback_moved(stage: &Path, base: &Path, moved: &[String]) {
    for relative in moved.iter().rev() {
        let source = stage.join(relative);
        let destination = base.join(relative);
        if let Some(parent) = destination.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::rename(source, destination);
    }
}

fn reconcile_slot_directories(
    base: &Path,
    registry: &mut IdentityRegistryFile,
) -> Result<(), String> {
    let root = base.join(IDENTITIES_DIR);
    let entries = match std::fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err("OSL identity slots could not be enumerated".to_owned()),
    };
    let sealer = crate::password_lifecycle::persistent_sealer()?;
    for entry in entries.take(MAX_IDENTITIES + 1) {
        let entry = entry.map_err(|_| "OSL identity slot could not be inspected".to_owned())?;
        let slot_id = entry.file_name().to_string_lossy().to_string();
        if validate_slot_id(&slot_id).is_err()
            || entry.file_type().map(|kind| !kind.is_dir()).unwrap_or(true)
        {
            continue;
        }
        let identity =
            match keystore::load_identity(&entry.path().join("identity.json"), sealer.as_ref()) {
                Ok(identity) => identity,
                Err(_) => continue,
            };
        if slot_id_for_identity(&identity) != slot_id {
            continue;
        }
        registry.slots.entry(slot_id.clone()).or_insert_with(|| {
            record_for_identity(&identity, slot_id, "Recovered identity".to_owned())
        });
    }
    registry.slots.retain(|slot_id, _| {
        slot_dir(base, slot_id).is_ok_and(|path| path.join("identity.json").is_file())
    });
    Ok(())
}

fn validate_slot_identity(dir: &Path, record: &IdentitySlotRecord) -> Result<(), String> {
    let sealer = crate::password_lifecycle::persistent_sealer()?;
    let identity = keystore::load_identity(&dir.join("identity.json"), sealer.as_ref())
        .map_err(|_| "OSL identity slot could not be unsealed".to_owned())?;
    if identity.user_id != record.osl_user_id || slot_id_for_identity(&identity) != record.slot_id {
        return Err("OSL identity slot does not match its encrypted registry record".to_owned());
    }
    Ok(())
}

fn record_for_identity(
    identity: &keystore::Identity,
    slot_id: String,
    label: String,
) -> IdentitySlotRecord {
    use base64::engine::general_purpose::STANDARD;
    IdentitySlotRecord {
        slot_id,
        label,
        osl_user_id: identity.user_id.clone(),
        ed25519_public_b64: STANDARD.encode(identity.ed25519_public.as_bytes()),
        created_at: ipc::main_password::now_unix_secs_pub(),
    }
}

fn slot_dto(record: &IdentitySlotRecord, active: bool) -> HubIdentitySlotDto {
    HubIdentitySlotDto {
        slot_id: record.slot_id.clone(),
        label: record.label.clone(),
        osl_user_id: record.osl_user_id.clone(),
        safety_number: ipc::tofu::safety_number(&record.ed25519_public_b64),
        active,
    }
}

fn slot_id_for_identity(identity: &keystore::Identity) -> String {
    let mut hash = Sha256::new();
    hash.update(SLOT_DOMAIN);
    hash.update(identity.ed25519_public.as_bytes());
    let digest = hash.finalize();
    format!("id-{}", URL_SAFE_NO_PAD.encode(&digest[..18]))
}

fn active_user_id(core: &HubCoreState) -> Result<String, String> {
    core.osl
        .identity
        .lock()
        .map_err(|_| "OSL identity state is unavailable".to_owned())?
        .as_ref()
        .map(|identity| identity.user_id.clone())
        .ok_or_else(|| "OSL identity is not loaded".to_owned())
}

fn active_slot_required(base: &Path) -> Result<String, String> {
    read_active_marker(base).ok_or_else(|| "OSL active identity marker is missing".to_owned())
}

fn validate_label(value: &str) -> Result<(), String> {
    if value.trim().is_empty()
        || value.len() > MAX_LABEL_BYTES
        || value.chars().any(char::is_control)
    {
        return Err("OSL identity label is invalid".to_owned());
    }
    Ok(())
}

fn validate_slot_id(value: &str) -> Result<(), String> {
    if !value.starts_with("id-")
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("OSL identity slot id is invalid".to_owned());
    }
    Ok(())
}

fn base_dir() -> Result<PathBuf, String> {
    keystore::osl_base_dir().map_err(|_| "OSL Privacy base storage is unavailable".to_owned())
}

fn slot_dir(base: &Path, slot_id: &str) -> Result<PathBuf, String> {
    validate_slot_id(slot_id)?;
    Ok(base.join(IDENTITIES_DIR).join(slot_id))
}

fn marker_path(base: &Path) -> PathBuf {
    base.join(ACTIVE_MARKER_FILE)
}

fn read_active_marker(base: &Path) -> Option<String> {
    let path = marker_path(base);
    read_marker_file(&path).or_else(|| {
        let backup = path.with_extension("bak");
        let value = read_marker_file(&backup)?;
        let _ = std::fs::rename(backup, path);
        Some(value)
    })
}

fn read_marker_file(path: &Path) -> Option<String> {
    let value = std::fs::read_to_string(path).ok()?;
    let value = value.trim();
    validate_slot_id(value).ok()?;
    Some(value.to_owned())
}

fn write_active_marker(base: &Path, slot_id: &str) -> Result<(), String> {
    validate_slot_id(slot_id)?;
    std::fs::create_dir_all(base)
        .map_err(|_| "OSL active identity marker directory could not be created".to_owned())?;
    let path = marker_path(base);
    let tmp = path.with_extension("tmp");
    let backup = path.with_extension("bak");
    {
        use std::io::Write as _;
        let mut file = std::fs::File::create(&tmp)
            .map_err(|_| "OSL active identity marker could not be written".to_owned())?;
        file.write_all(slot_id.as_bytes())
            .map_err(|_| "OSL active identity marker could not be written".to_owned())?;
        file.sync_all()
            .map_err(|_| "OSL active identity marker could not be synchronized".to_owned())?;
    }
    if backup.exists() {
        std::fs::remove_file(&backup)
            .map_err(|_| "OSL stale identity marker backup could not be removed".to_owned())?;
    }
    if path.exists() {
        std::fs::rename(&path, &backup)
            .map_err(|_| "OSL prior identity marker could not be preserved".to_owned())?;
    }
    if std::fs::rename(&tmp, &path).is_err() {
        let _ = std::fs::rename(&backup, &path);
        let _ = std::fs::remove_file(&tmp);
        return Err("OSL active identity marker could not be committed".to_owned());
    }
    let _ = std::fs::remove_file(backup);
    Ok(())
}

fn remove_active_marker(base: &Path) -> Result<(), String> {
    let path = marker_path(base);
    for candidate in [
        path.clone(),
        path.with_extension("tmp"),
        path.with_extension("bak"),
    ] {
        match std::fs::remove_file(candidate) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("OSL active identity marker could not be removed".to_owned()),
        }
    }
    Ok(())
}

fn single_recoverable_slot(base: &Path) -> Option<String> {
    if base.join("identity.json").exists() {
        return None;
    }
    let mut slots = std::fs::read_dir(base.join(IDENTITIES_DIR))
        .ok()?
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| {
            let id = entry.file_name().to_string_lossy().to_string();
            (validate_slot_id(&id).is_ok() && entry.path().join("identity.json").is_file())
                .then_some(id)
        });
    let only = slots.next()?;
    slots.next().is_none().then_some(only)
}

fn registry_path(base: &Path) -> PathBuf {
    base.join(REGISTRY_FILE)
}

fn load_registry(base: &Path) -> Result<IdentityRegistryFile, String> {
    let key = require_unlocked()?;
    let path = registry_path(base);
    let backup = path.with_extension("bak");
    let primary = std::fs::read(&path).ok();
    let registry = primary
        .as_deref()
        .and_then(|bytes| decode_registry(bytes, &key).ok())
        .or_else(|| {
            let bytes = std::fs::read(&backup).ok()?;
            let recovered = decode_registry(&bytes, &key).ok()?;
            let _ = std::fs::copy(&backup, &path);
            Some(recovered)
        });
    let Some(registry) = registry else {
        if !path.exists() && !backup.exists() {
            return Ok(IdentityRegistryFile::default());
        }
        return Err("OSL identity registry is invalid, locked, or not encrypted".to_owned());
    };
    if registry.version != REGISTRY_VERSION && registry.version != 0 {
        return Err("OSL identity registry version is unsupported".to_owned());
    }
    if registry.slots.len() > MAX_IDENTITIES {
        return Err("OSL identity registry exceeds the supported identity limit".to_owned());
    }
    Ok(registry)
}

fn decode_registry(bytes: &[u8], key: &[u8; 32]) -> Result<IdentityRegistryFile, String> {
    if bytes.len() as u64 > MAX_REGISTRY_BYTES || !ipc::main_password::has_enc_magic(bytes) {
        return Err("invalid registry envelope".to_owned());
    }
    let plain = ipc::main_password::decrypt_at_rest(bytes, key)
        .map_err(|_| "registry decrypt failed".to_owned())?;
    serde_json::from_slice(&plain).map_err(|_| "registry parse failed".to_owned())
}

fn write_registry(base: &Path, registry: &IdentityRegistryFile) -> Result<(), String> {
    let key = require_unlocked()?;
    let body = serde_json::to_vec(registry)
        .map_err(|_| "OSL identity registry could not be encoded".to_owned())?;
    if body.len() as u64 > MAX_REGISTRY_BYTES {
        return Err("OSL identity registry is too large".to_owned());
    }
    let sealed = ipc::main_password::encrypt_at_rest(&body, &key)
        .map_err(|_| "OSL identity registry could not be encrypted".to_owned())?;
    std::fs::create_dir_all(base)
        .map_err(|_| "OSL identity registry directory could not be created".to_owned())?;
    let path = registry_path(base);
    let tmp = path.with_extension("tmp");
    let backup = path.with_extension("bak");
    {
        use std::io::Write as _;
        let mut file = std::fs::File::create(&tmp)
            .map_err(|_| "OSL identity registry could not be written".to_owned())?;
        file.write_all(&sealed)
            .map_err(|_| "OSL identity registry could not be written".to_owned())?;
        file.sync_all()
            .map_err(|_| "OSL identity registry could not be synchronized".to_owned())?;
    }
    if backup.exists() {
        std::fs::remove_file(&backup)
            .map_err(|_| "OSL stale identity registry backup could not be removed".to_owned())?;
    }
    let had_previous = path.exists();
    if had_previous {
        std::fs::rename(&path, &backup)
            .map_err(|_| "OSL prior identity registry could not be preserved".to_owned())?;
    }
    if std::fs::rename(&tmp, &path).is_err() {
        if had_previous {
            let _ = std::fs::rename(&backup, &path);
        }
        let _ = std::fs::remove_file(&tmp);
        return Err("OSL identity registry could not be committed".to_owned());
    }
    if had_previous {
        let _ = std::fs::remove_file(backup);
    }
    Ok(())
}

fn require_unlocked() -> Result<[u8; 32], String> {
    ipc::main_password::get_file_storage_key()
        .ok_or_else(|| "OSL main password must be unlocked".to_owned())
}

pub(crate) fn attempt_unregister(
    identity: &keystore::Identity,
    client: Option<keystore::KeyServerClient>,
) -> RemoteUnregisterState {
    let Some(client) = client else {
        return RemoteUnregisterState::Unavailable;
    };
    match client.unregister(identity) {
        Ok(()) => RemoteUnregisterState::Succeeded,
        Err(_) => RemoteUnregisterState::Failed,
    }
}

pub(crate) fn reset_account_scoped_state(state: &ipc::AppState) {
    *state.identity.lock().expect("identity poisoned") = None;
    *state.keyserver.lock().expect("keyserver poisoned") = None;
    state.set_cloud_registration_state(ipc::state::CloudRegistrationState::NotAttempted);
    *state
        .registration_alert
        .lock()
        .expect("registration poisoned") = None;
    state.sender_pubkey_cache.clear();
    *state.message_store.lock().expect("message store poisoned") = None;
    *state.peer_map.lock().expect("peer map poisoned") = Default::default();
    *state.whitelist_state.lock().expect("whitelist poisoned") = Default::default();
    *state
        .server_defaults
        .lock()
        .expect("server defaults poisoned") = Default::default();
    *state.sender_key_state.lock().expect("sender keys poisoned") = Default::default();
    *state.burned_scopes.lock().expect("burned scopes poisoned") = Default::default();
    *state.scope_membership.lock().expect("membership poisoned") = Default::default();
    state
        .channel_members
        .lock()
        .expect("channel members poisoned")
        .clear();
    state
        .key_change_alerts
        .lock()
        .expect("key changes poisoned")
        .clear();
    state
        .friend_ids
        .lock()
        .expect("friend ids poisoned")
        .clear();
    state
        .guild_list
        .lock()
        .expect("guild list poisoned")
        .clear();
    *state
        .recovery_guard
        .lock()
        .expect("recovery guard poisoned") = Default::default();
    state
        .mode1_reassembly
        .lock()
        .expect("reassembly poisoned")
        .clear();
    *state
        .recovery_token
        .lock()
        .expect("recovery token poisoned") = None;
    *state
        .last_persist_error
        .lock()
        .expect("persist error poisoned") = None;
    state
        .identity_regenerated_this_launch
        .store(false, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_base(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "osl-hub-identities-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn marker_is_opaque_bounded_and_recovers_backup() {
        let base = temp_base("marker");
        let identity = keystore::identity_from_entropy([31; 16], "ignored".to_owned());
        let slot = slot_id_for_identity(&identity);
        write_active_marker(&base, &slot).unwrap();
        assert_eq!(read_active_marker(&base).as_deref(), Some(slot.as_str()));
        assert!(!std::fs::read_to_string(marker_path(&base))
            .unwrap()
            .contains("ignored"));
        std::fs::rename(marker_path(&base), marker_path(&base).with_extension("bak")).unwrap();
        assert_eq!(read_active_marker(&base).as_deref(), Some(slot.as_str()));
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn invalid_slot_ids_never_escape_identity_root() {
        let base = temp_base("paths");
        for invalid in ["../other", "id-../../other", "base", "id-a/b", "id-a b"] {
            assert!(slot_dir(&base, invalid).is_err());
        }
    }

    #[test]
    fn startup_ignores_missing_marked_slot_and_recovers_only_complete_slot() {
        let base = temp_base("startup-recovery");
        let missing_identity = keystore::identity_from_entropy([61; 16], "missing".to_owned());
        let missing_slot = slot_id_for_identity(&missing_identity);
        write_active_marker(&base, &missing_slot).unwrap();
        let complete_identity = keystore::identity_from_entropy([62; 16], "complete".to_owned());
        let complete_slot = slot_id_for_identity(&complete_identity);
        let complete_dir = slot_dir(&base, &complete_slot).unwrap();
        std::fs::create_dir_all(&complete_dir).unwrap();
        std::fs::write(complete_dir.join("identity.json"), b"sealed-placeholder").unwrap();
        assert_eq!(
            select_slot_from_disk(&base).as_deref(),
            Some(complete_slot.as_str())
        );
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn registry_requires_unlock_and_is_never_plain_json() {
        let base = temp_base("registry");
        ipc::main_password::set_file_storage_key(None);
        assert!(write_registry(&base, &IdentityRegistryFile::default()).is_err());
        let key = [71_u8; 32];
        ipc::main_password::set_file_storage_key(Some(key));
        let mut registry = IdentityRegistryFile {
            version: REGISTRY_VERSION,
            ..IdentityRegistryFile::default()
        };
        let mut identity = keystore::identity_from_entropy([41; 16], "ignored".to_owned());
        identity.user_id = crate::password_lifecycle::native_user_id(&identity);
        let slot = slot_id_for_identity(&identity);
        registry.slots.insert(
            slot.clone(),
            record_for_identity(&identity, slot, "Sensitive alias".to_owned()),
        );
        write_registry(&base, &registry).unwrap();
        let raw = std::fs::read(registry_path(&base)).unwrap();
        assert!(ipc::main_password::has_enc_magic(&raw));
        assert!(!raw
            .windows(b"Sensitive alias".len())
            .any(|w| w == b"Sensitive alias"));
        assert_eq!(load_registry(&base).unwrap().slots.len(), 1);
        let path = registry_path(&base);
        std::fs::rename(&path, path.with_extension("bak")).unwrap();
        std::fs::write(&path, b"truncated").unwrap();
        assert_eq!(load_registry(&base).unwrap().slots.len(), 1);
        assert!(ipc::main_password::has_enc_magic(
            &std::fs::read(&path).unwrap()
        ));
        ipc::main_password::set_file_storage_key(None);
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn failed_remote_unregister_is_not_reported_as_success() {
        let identity = keystore::identity_from_entropy([53; 16], "osl-failure".to_owned());
        let client = keystore::KeyServerClient::new("http://127.0.0.1:9").unwrap();
        assert_eq!(
            attempt_unregister(&identity, Some(client)),
            RemoteUnregisterState::Failed
        );
        assert_eq!(
            attempt_unregister(&identity, None),
            RemoteUnregisterState::Unavailable
        );
    }
}
