//! Per-volume USB dead-man bindings.
//!
//! The Windows USB monitor owns device notifications. This module owns the
//! durable choice associated with a stable volume interface identifier and
//! dispatches a matching removal to the established lock or burn mechanism.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::cleanup::{self, HubFullCleanupResult};
use crate::core_bridge::HubCoreState;

const DOCUMENT_VERSION: u8 = 1;
const MAX_BINDINGS: usize = 16;
const MAX_DOCUMENT_BYTES: u64 = 16 * 1024;

/// The consequence of removing a bound volume. Lock is recoverable; wipe is
/// destructive and must only be selected by the typed-confirmation UI.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DeadmanAction {
    Lock,
    Wipe,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeadmanBinding {
    pub volume_device_id: String,
    pub action: DeadmanAction,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeadmanDocument {
    version: u8,
    bindings: Vec<DeadmanBinding>,
}

impl Default for DeadmanDocument {
    fn default() -> Self {
        Self {
            version: DOCUMENT_VERSION,
            bindings: Vec::new(),
        }
    }
}

/// The observable consequence of a matched removal.
pub enum DeadmanRemovalOutcome {
    NoBinding,
    Locked,
    Wiped(HubFullCleanupResult),
}

/// Save (or replace) the action associated with one Windows volume interface
/// identifier. The identifier is intentionally not a drive letter: Windows
/// may reassign those whenever a device is mounted.
pub fn bind_volume(
    path: &Path,
    volume_device_id: String,
    action: DeadmanAction,
) -> Result<DeadmanBinding, String> {
    validate_volume_device_id(&volume_device_id)?;
    let mut document = load_document(path)?;
    let binding = DeadmanBinding {
        volume_device_id,
        action,
    };
    if let Some(existing) = document
        .bindings
        .iter_mut()
        .find(|existing| existing.volume_device_id == binding.volume_device_id)
    {
        *existing = binding.clone();
    } else {
        if document.bindings.len() >= MAX_BINDINGS {
            return Err("too many USB dead-man bindings".to_owned());
        }
        document.bindings.push(binding.clone());
    }
    write_document(path, &document)?;
    Ok(binding)
}

/// Return the bindings that survived strict parsing. A corrupt or unsupported
/// file fails closed: it cannot turn an arbitrary removal into a wipe.
pub fn bindings(path: &Path) -> Result<Vec<DeadmanBinding>, String> {
    Ok(load_document(path)?.bindings)
}

/// Consume one removal reported by the USB monitor. Only an exact stable
/// volume identifier match has an effect. Wipe deliberately delegates to the
/// verified burn path so the target set and its durable journal stay identical
/// to a burn-password invocation.
pub fn handle_volume_removal(
    binding_path: &Path,
    removed_volume_device_id: &str,
    state: &HubCoreState,
    app_config_dir: &Path,
    app_local_data_dir: &Path,
    service_hosts_shutdown: bool,
) -> Result<DeadmanRemovalOutcome, String> {
    let action = load_document(binding_path)?
        .bindings
        .into_iter()
        .find(|binding| binding.volume_device_id == removed_volume_device_id)
        .map(|binding| binding.action);

    match action {
        None => Ok(DeadmanRemovalOutcome::NoBinding),
        Some(DeadmanAction::Lock) => {
            ipc::main_password::lock_main_password_session(&state.osl);
            Ok(DeadmanRemovalOutcome::Locked)
        }
        Some(DeadmanAction::Wipe) => cleanup::execute_verified_gate_burn(
            state,
            app_config_dir,
            app_local_data_dir,
            service_hosts_shutdown,
        )
        .map(DeadmanRemovalOutcome::Wiped),
    }
}

fn validate_volume_device_id(volume_device_id: &str) -> Result<(), String> {
    if volume_device_id.starts_with(r"\\?\") && volume_device_id.len() <= 1024 {
        Ok(())
    } else {
        Err("USB dead-man bindings require a Windows volume interface identifier".to_owned())
    }
}

fn load_document(path: &Path) -> Result<DeadmanDocument, String> {
    let bytes = crate::atomic_file::read_recoverable_bounded(
        path,
        MAX_DOCUMENT_BYTES,
        "USB dead-man bindings",
    )
    .map_err(|error| format!("could not read USB dead-man bindings: {error}"))?
    .unwrap_or_default();
    if bytes.is_empty() {
        return Ok(DeadmanDocument::default());
    }
    let document = serde_json::from_slice::<DeadmanDocument>(&bytes)
        .map_err(|_| "USB dead-man bindings are invalid".to_owned())?;
    if document.version != DOCUMENT_VERSION || document.bindings.len() > MAX_BINDINGS {
        return Err("USB dead-man bindings are unsupported".to_owned());
    }
    for binding in &document.bindings {
        validate_volume_device_id(&binding.volume_device_id)?;
    }
    Ok(document)
}

fn write_document(path: &Path, document: &DeadmanDocument) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(document)
        .map_err(|_| "USB dead-man bindings could not be encoded".to_owned())?;
    if bytes.len() as u64 > MAX_DOCUMENT_BYTES {
        return Err("USB dead-man bindings exceed the size limit".to_owned());
    }
    crate::atomic_file::write_recoverable(path, &bytes, "USB dead-man bindings")
        .map_err(|error| format!("could not save USB dead-man bindings: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_file() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        std::env::temp_dir()
            .join(format!("osl-deadman-{}-{nonce}", std::process::id()))
            .join("bindings.json")
    }

    fn cleanup_roots(path: &Path) -> (PathBuf, PathBuf, PathBuf, PathBuf, PathBuf) {
        let root = path.parent().expect("temporary directory");
        let config = root.join("config");
        let local = root.join("local");
        let core = config.join("osl-core");
        let profiles = local.join("service-profiles-v2");
        let native_profiles = local.join("native-window-profiles-v1");
        fs::create_dir_all(&core).expect("core root");
        fs::create_dir_all(&profiles).expect("profiles root");
        fs::create_dir_all(&native_profiles).expect("native profiles root");
        fs::write(core.join("peer_map.json"), b"sealed").expect("core fixture");
        fs::write(profiles.join("cache"), b"sealed").expect("profile fixture");
        fs::write(native_profiles.join("cache"), b"sealed").expect("native fixture");
        (config, local, core, profiles, native_profiles)
    }

    #[test]
    fn binds_each_volume_and_persists_its_action() {
        let path = temporary_file();
        let alpha = r"\\?\Volume{alpha}".to_owned();
        let beta = r"\\?\Volume{beta}".to_owned();

        bind_volume(&path, alpha.clone(), DeadmanAction::Lock).expect("bind lock");
        bind_volume(&path, beta.clone(), DeadmanAction::Wipe).expect("bind wipe");
        bind_volume(&path, alpha.clone(), DeadmanAction::Wipe).expect("replace lock choice");

        assert_eq!(
            bindings(&path).expect("reload bindings"),
            vec![
                DeadmanBinding {
                    volume_device_id: alpha,
                    action: DeadmanAction::Wipe,
                },
                DeadmanBinding {
                    volume_device_id: beta,
                    action: DeadmanAction::Wipe,
                },
            ]
        );
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn drive_letters_cannot_be_bound_as_a_deadman_device() {
        let path = temporary_file();
        let error = bind_volume(&path, "E:".to_owned(), DeadmanAction::Lock)
            .expect_err("drive letters are unstable");
        assert!(error.contains("volume interface identifier"));
    }

    #[test]
    fn removal_locks_without_deleting_but_wipe_uses_burn_targets() {
        let _serial = crate::global_keystore_test_lock();
        let path = temporary_file();
        let (config, local, core, profiles, native_profiles) = cleanup_roots(&path);
        // The wipe path must only purge the currently configured isolated
        // OSL core. Establish the same trusted-root invariant as the host
        // before exercising a bound-volume removal.
        keystore::set_base_dir_override(Some(core.clone()));
        let state = HubCoreState::default();
        let lock_volume = r"\\?\Volume{lock}".to_owned();
        let wipe_volume = r"\\?\Volume{wipe}".to_owned();

        ipc::main_password::set_file_storage_key(Some([7; 32]));
        bind_volume(&path, lock_volume.clone(), DeadmanAction::Lock).expect("bind lock");
        assert!(matches!(
            handle_volume_removal(&path, &lock_volume, &state, &config, &local, true),
            Ok(DeadmanRemovalOutcome::Locked)
        ));
        assert_eq!(ipc::main_password::get_file_storage_key(), None);
        assert!(core.exists());
        assert!(profiles.exists());
        assert!(native_profiles.exists());

        bind_volume(&path, wipe_volume.clone(), DeadmanAction::Wipe).expect("bind wipe");
        let outcome = handle_volume_removal(&path, &wipe_volume, &state, &config, &local, true)
            .expect("wipe removal");
        let DeadmanRemovalOutcome::Wiped(result) = outcome else {
            panic!("wipe binding must use the burn path");
        };
        assert!(result
            .removed_targets
            .iter()
            .any(|target| target == "hub_core"));
        assert!(result
            .removed_targets
            .iter()
            .any(|target| target == "service_profiles"));
        assert!(result
            .removed_targets
            .iter()
            .any(|target| target == "native_profiles"));
        assert!(!core.exists());
        assert!(!profiles.exists());
        assert!(!native_profiles.exists());
        keystore::set_base_dir_override(None);
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }
}
