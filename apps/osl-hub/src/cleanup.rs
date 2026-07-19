//! OSL Privacy destructive cleanup with an explicit, inspectable manifest.
//!
//! Paths are derived from trusted application roots, never accepted from a
//! platform page. The original Discord OSL directory is outside this manifest
//! and cannot be reached by any relative target.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::core_bridge::HubCoreState;
use crate::identity_registry::{self, RemoteUnregisterState};

const HUB_CORE_DIR: &str = "osl-core";
const PROFILE_DIR: &str = "service-profiles-v2";
const NATIVE_PROFILE_DIR: &str = "native-window-profiles-v1";
const GATE_BURN_JOURNAL: &str = ".gate-burn-journal.json";
const GATE_BURN_JOURNAL_TMP: &str = ".gate-burn-journal.tmp";
const MAX_IDENTITIES: usize = 16;

#[derive(Debug, Clone, serde::Deserialize, Serialize)]
struct GateBurnJournal {
    version: u32,
    state: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HubCleanupTargetDto {
    pub id: &'static str,
    pub contains: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HubFullCleanupManifest {
    pub version: u32,
    pub targets: Vec<HubCleanupTargetDto>,
    pub includes_profile_tombstones: bool,
    pub includes_scope_ttl: bool,
    pub includes_scope_blobs: bool,
    pub includes_local_protected_ledger: bool,
    pub original_discord_data_untouched: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteUnregisterSummary {
    pub identities_found: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub unavailable: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HubFullCleanupResult {
    pub local_cleanup_complete: bool,
    pub removed_targets: Vec<String>,
    pub failed_targets: Vec<String>,
    pub remote_unregister: RemoteUnregisterSummary,
    pub restart_required: bool,
    pub original_discord_data_untouched: bool,
}

struct CleanupTarget {
    id: &'static str,
    path: PathBuf,
}

pub fn full_cleanup_manifest() -> HubFullCleanupManifest {
    HubFullCleanupManifest {
        version: 1,
        targets: vec![
            HubCleanupTargetDto {
                id: "hub_core",
                contains: "all OSL identities, encrypted message state, People, TTL, blobs, and local protected ledgers",
            },
            HubCleanupTargetDto {
                id: "service_profiles",
                contains: "OSL Privacy browser profiles, cookies, site storage, and cleanup tombstones",
            },
            HubCleanupTargetDto {
                id: "native_profiles",
                contains: "OSL-owned isolated native-app profiles and their local sessions",
            },
            HubCleanupTargetDto {
                id: "service_registry",
                contains: "linked-service labels and account profile registry",
            },
            HubCleanupTargetDto {
                id: "service_scope_index",
                contains: "encrypted service-account scope coverage and burn journals",
            },
            HubCleanupTargetDto {
                id: "preview_preferences",
                contains: "OSL Privacy onboarding and composer preferences",
            },
        ],
        includes_profile_tombstones: true,
        includes_scope_ttl: true,
        includes_scope_blobs: true,
        includes_local_protected_ledger: true,
        original_discord_data_untouched: true,
    }
}

/// Delete every known OSL Privacy artifact after the trusted host has closed all
/// service webviews. Remote unregister is best-effort and separately counted.
/// Local cleanup proceeds even when the network is unavailable.
pub fn execute_full_hub_cleanup(
    core: &HubCoreState,
    app_config_dir: &Path,
    app_local_data_dir: &Path,
    service_hosts_shutdown: bool,
) -> Result<HubFullCleanupResult, String> {
    if !service_hosts_shutdown {
        return Err("OSL full cleanup requires every service host to be closed first".to_owned());
    }
    ipc::main_password::get_file_storage_key()
        .ok_or_else(|| "OSL main password must be unlocked".to_owned())?;
    validate_trusted_roots(app_config_dir, app_local_data_dir)?;
    let (identities, unreadable_identities) = collect_identities(core, app_config_dir)?;
    let client = core
        .osl
        .keyserver
        .lock()
        .map_err(|_| "OSL keyserver state is unavailable".to_owned())?
        .clone();
    let mut remote = RemoteUnregisterSummary {
        identities_found: identities.len().saturating_add(unreadable_identities),
        unavailable: unreadable_identities,
        ..RemoteUnregisterSummary::default()
    };
    for identity in &identities {
        match identity_registry::attempt_unregister(identity, client.clone()) {
            RemoteUnregisterState::Succeeded => remote.succeeded += 1,
            RemoteUnregisterState::Failed => remote.failed += 1,
            RemoteUnregisterState::Unavailable => remote.unavailable += 1,
        }
    }

    identity_registry::reset_account_scoped_state(&core.osl);
    ipc::main_password::set_file_storage_key(None);
    keystore::set_active_account_dir(None);

    let targets = cleanup_targets(app_config_dir, app_local_data_dir);
    let mut removed_targets = Vec::new();
    let mut failed_targets = Vec::new();
    for target in targets {
        match remove_target_without_following_links(&target.path) {
            Ok(()) => removed_targets.push(target.id.to_owned()),
            Err(()) => failed_targets.push(target.id.to_owned()),
        }
    }
    Ok(HubFullCleanupResult {
        local_cleanup_complete: failed_targets.is_empty(),
        removed_targets,
        failed_targets,
        remote_unregister: remote,
        restart_required: true,
        original_discord_data_untouched: true,
    })
}

/// Execute the destructive consequence of an already-verified burn password.
///
/// Unlike the ordinary settings cleanup, this path does not need the main
/// password's file key. It first persists a fixed-root recovery journal. Once
/// that journal is durable, a crash can only delay deletion: startup resumes
/// the same idempotent local purge before loading any OSL identity.
pub fn execute_verified_gate_burn(
    core: &HubCoreState,
    app_config_dir: &Path,
    app_local_data_dir: &Path,
    service_hosts_shutdown: bool,
) -> Result<HubFullCleanupResult, String> {
    if !service_hosts_shutdown {
        return Err("OSL burn requires every OSL-owned service host to be closed first".to_owned());
    }
    validate_trusted_roots(app_config_dir, app_local_data_dir)?;

    // Collect only what is already available in trusted memory. Sealed
    // identities that cannot be opened without the ordinary password are
    // counted as unavailable; no plaintext secret is exported to make remote
    // unregister possible.
    let (identities, unreadable_identities) = collect_identities(core, app_config_dir)?;
    let client = core
        .osl
        .keyserver
        .lock()
        .map_err(|_| "OSL keyserver state is unavailable".to_owned())?
        .clone();

    write_gate_burn_journal(app_config_dir)?;

    let mut remote = RemoteUnregisterSummary {
        identities_found: identities.len().saturating_add(unreadable_identities),
        unavailable: unreadable_identities,
        ..RemoteUnregisterSummary::default()
    };
    for identity in &identities {
        match identity_registry::attempt_unregister(identity, client.clone()) {
            RemoteUnregisterState::Succeeded => remote.succeeded += 1,
            RemoteUnregisterState::Failed => remote.failed += 1,
            RemoteUnregisterState::Unavailable => remote.unavailable += 1,
        }
    }

    identity_registry::reset_account_scoped_state(&core.osl);
    ipc::main_password::set_file_storage_key(None);
    keystore::set_active_account_dir(None);

    let (removed_targets, failed_targets) = purge_fixed_targets(app_config_dir, app_local_data_dir);
    if failed_targets.is_empty() {
        remove_gate_burn_journal(app_config_dir)?;
    }
    Ok(HubFullCleanupResult {
        local_cleanup_complete: failed_targets.is_empty(),
        removed_targets,
        failed_targets,
        remote_unregister: remote,
        restart_required: false,
        original_discord_data_untouched: true,
    })
}

/// Resume a burn that crossed its durable commit point before a crash. This is
/// intentionally local-only: remote unregister was best effort in the first
/// process and must never block destruction of local decrypt capability.
pub fn resume_interrupted_gate_burn(
    app_config_dir: &Path,
    app_local_data_dir: &Path,
) -> Result<bool, String> {
    validate_trusted_roots(app_config_dir, app_local_data_dir)?;
    let journal_path = app_config_dir.join(GATE_BURN_JOURNAL);
    let bytes = match std::fs::read(&journal_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Err("OSL burn recovery journal is unavailable".to_owned()),
    };
    let journal: GateBurnJournal = serde_json::from_slice(&bytes).map_err(|_| {
        "OSL burn recovery journal is invalid; no deletion was attempted".to_owned()
    })?;
    if journal.version != 1 || journal.state != "purging" {
        return Err("OSL burn recovery journal is invalid; no deletion was attempted".to_owned());
    }
    let (_, failed_targets) = purge_fixed_targets(app_config_dir, app_local_data_dir);
    if !failed_targets.is_empty() {
        return Err("OSL burn recovery remains pending".to_owned());
    }
    remove_gate_burn_journal(app_config_dir)?;
    ipc::main_password::set_file_storage_key(None);
    keystore::set_active_account_dir(None);
    Ok(true)
}

fn write_gate_burn_journal(app_config_dir: &Path) -> Result<(), String> {
    let journal = GateBurnJournal {
        version: 1,
        state: "purging".to_owned(),
    };
    let bytes = serde_json::to_vec(&journal)
        .map_err(|_| "OSL burn recovery journal could not be encoded".to_owned())?;
    let path = app_config_dir.join(GATE_BURN_JOURNAL);
    let tmp = app_config_dir.join(GATE_BURN_JOURNAL_TMP);
    std::fs::create_dir_all(app_config_dir)
        .map_err(|_| "OSL burn recovery journal directory is unavailable".to_owned())?;
    let mut file = std::fs::File::create(&tmp)
        .map_err(|_| "OSL burn recovery journal could not be created".to_owned())?;
    use std::io::Write;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "OSL burn recovery journal could not be committed".to_owned())?;
    std::fs::rename(&tmp, &path)
        .map_err(|_| "OSL burn recovery journal could not be committed".to_owned())?;
    if let Ok(directory) = std::fs::File::open(app_config_dir) {
        let _ = directory.sync_all();
    }
    Ok(())
}

fn remove_gate_burn_journal(app_config_dir: &Path) -> Result<(), String> {
    for path in [
        app_config_dir.join(GATE_BURN_JOURNAL),
        app_config_dir.join(GATE_BURN_JOURNAL_TMP),
    ] {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("OSL burn recovery journal could not be cleared".to_owned()),
        }
    }
    Ok(())
}

fn purge_fixed_targets(
    app_config_dir: &Path,
    app_local_data_dir: &Path,
) -> (Vec<String>, Vec<String>) {
    let mut removed_targets = Vec::new();
    let mut failed_targets = Vec::new();
    for target in cleanup_targets(app_config_dir, app_local_data_dir) {
        match remove_target_without_following_links(&target.path) {
            Ok(()) => removed_targets.push(target.id.to_owned()),
            Err(()) => failed_targets.push(target.id.to_owned()),
        }
    }
    (removed_targets, failed_targets)
}

fn validate_trusted_roots(app_config_dir: &Path, app_local_data_dir: &Path) -> Result<(), String> {
    if !app_config_dir.is_absolute() || !app_local_data_dir.is_absolute() {
        return Err("OSL cleanup roots must be absolute trusted application paths".to_owned());
    }
    let expected_core = app_config_dir.join(HUB_CORE_DIR);
    let configured_core = keystore::osl_base_dir()
        .map_err(|_| "OSL Privacy base storage is unavailable".to_owned())?;
    if normalise_lexical(&configured_core) != normalise_lexical(&expected_core) {
        return Err(
            "OSL cleanup refused a root that is not the active isolated OSL Privacy core"
                .to_owned(),
        );
    }
    if app_config_dir.parent().is_none() || app_local_data_dir.parent().is_none() {
        return Err("OSL cleanup refused a filesystem root".to_owned());
    }
    Ok(())
}

fn cleanup_targets(app_config_dir: &Path, app_local_data_dir: &Path) -> Vec<CleanupTarget> {
    let mut targets = vec![
        CleanupTarget {
            id: "hub_core",
            path: app_config_dir.join(HUB_CORE_DIR),
        },
        CleanupTarget {
            id: "service_profiles",
            path: app_local_data_dir.join(PROFILE_DIR),
        },
        CleanupTarget {
            id: "native_profiles",
            path: app_local_data_dir.join(NATIVE_PROFILE_DIR),
        },
        CleanupTarget {
            id: "service_registry",
            path: app_config_dir.join("service-registry.json"),
        },
        CleanupTarget {
            id: "service_registry_backup",
            path: app_config_dir.join("service-registry.json.bak"),
        },
        CleanupTarget {
            id: "service_registry_atomic_backup",
            path: app_config_dir.join("service-registry.bak"),
        },
        CleanupTarget {
            id: "service_registry_atomic_tmp",
            path: app_config_dir.join("service-registry.tmp"),
        },
        CleanupTarget {
            id: "service_registry_legacy_quarantine",
            path: app_config_dir.join("service-registry.json.legacy-untrusted"),
        },
        CleanupTarget {
            id: "service_registry_backup_legacy_quarantine",
            path: app_config_dir.join("service-registry.bak.legacy-untrusted"),
        },
        CleanupTarget {
            id: "service_scope_index",
            path: app_config_dir.join("service-scope-index.json"),
        },
        CleanupTarget {
            id: "service_scope_index_backup",
            path: app_config_dir.join("service-scope-index.bak"),
        },
        CleanupTarget {
            id: "service_scope_index_tmp",
            path: app_config_dir.join("service-scope-index.tmp"),
        },
        CleanupTarget {
            id: "preview_preferences",
            path: app_config_dir.join("preview-preferences.json"),
        },
        CleanupTarget {
            id: "preview_preferences_tmp",
            path: app_config_dir.join("preview-preferences.json.tmp"),
        },
        CleanupTarget {
            id: "preview_preferences_atomic_backup",
            path: app_config_dir.join("preview-preferences.bak"),
        },
        CleanupTarget {
            id: "preview_preferences_atomic_tmp",
            path: app_config_dir.join("preview-preferences.tmp"),
        },
    ];
    // Service registry staging names carry process/sequence suffixes. Only
    // immediate, bounded, exact-prefix regular entries are included.
    if let Ok(entries) = std::fs::read_dir(app_config_dir) {
        for entry in entries.flatten().take(256) {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("service-registry.json.tmp-")
                && name.len() <= 160
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
            {
                targets.push(CleanupTarget {
                    id: "service_registry_staging",
                    path: entry.path(),
                });
            }
        }
    }
    targets
}

fn collect_identities(
    core: &HubCoreState,
    app_config_dir: &Path,
) -> Result<(Vec<keystore::Identity>, usize), String> {
    let mut identities = Vec::new();
    let mut users = HashSet::new();
    if let Some(identity) = core
        .osl
        .identity
        .lock()
        .map_err(|_| "OSL identity state is unavailable".to_owned())?
        .clone()
    {
        users.insert(identity.user_id.clone());
        identities.push(identity);
    }
    let base = app_config_dir.join(HUB_CORE_DIR);
    let paths = identity_paths(&base);
    let Ok(sealer) = crate::password_lifecycle::persistent_sealer() else {
        let unreadable = paths.len().saturating_sub(identities.len());
        return Ok((identities, unreadable));
    };
    let mut unreadable = 0usize;
    for path in paths {
        if identities.len() >= MAX_IDENTITIES {
            break;
        }
        match keystore::load_identity(&path, sealer.as_ref()) {
            Ok(identity) => {
                if users.insert(identity.user_id.clone()) {
                    identities.push(identity);
                }
            }
            Err(_) => unreadable += 1,
        }
    }
    Ok((identities, unreadable))
}

fn identity_paths(base: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let flat = base.join("identity.json");
    if flat.is_file() {
        paths.push(flat);
    }
    if let Ok(entries) = std::fs::read_dir(base.join("hub-identities")) {
        for entry in entries.flatten().take(MAX_IDENTITIES + 1) {
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                let identity = entry.path().join("identity.json");
                if identity.is_file() {
                    paths.push(identity);
                }
            }
        }
    }
    paths
}

fn remove_target_without_following_links(path: &Path) -> Result<(), ()> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(()),
    };
    if metadata.file_type().is_symlink() || metadata.is_file() {
        std::fs::remove_file(path).map_err(|_| ())
    } else if metadata.is_dir() {
        std::fs::remove_dir_all(path).map_err(|_| ())
    } else {
        Err(())
    }
}

fn normalise_lexical(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "osl-cleanup-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn manifest_explicitly_covers_new_hub_artifacts() {
        let manifest = full_cleanup_manifest();
        assert!(manifest.includes_scope_ttl);
        assert!(manifest.includes_scope_blobs);
        assert!(manifest.includes_profile_tombstones);
        assert!(manifest.includes_local_protected_ledger);
        assert!(manifest.original_discord_data_untouched);
    }

    #[test]
    fn fixed_targets_cannot_reach_original_discord_directory() {
        let config = temp_root("config");
        let local = temp_root("local");
        let original = config.parent().unwrap().join("osl");
        for target in cleanup_targets(&config, &local) {
            assert!(!target.path.starts_with(&original));
            assert!(target.path.starts_with(&config) || target.path.starts_with(&local));
        }
    }

    #[test]
    fn owner_scoped_firefox_profiles_are_inside_full_cleanup_target() {
        let config = temp_root("firefox-config");
        let local = temp_root("firefox-local");
        let firefox = local.join(crate::native_apps::firefox_profile_relative_path("owner-a"));
        let other = local.join(crate::native_apps::firefox_profile_relative_path("owner-b"));
        let service_profiles = cleanup_targets(&config, &local)
            .into_iter()
            .find(|target| target.id == "service_profiles")
            .expect("service profile cleanup target exists");
        assert!(firefox.starts_with(&service_profiles.path));
        assert_ne!(firefox, service_profiles.path);
        assert_ne!(firefox, other);
    }

    #[test]
    fn target_removal_unlinks_symlink_without_following_it() {
        let root = temp_root("symlink");
        let outside = temp_root("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("keep"), b"must survive").unwrap();
        std::fs::create_dir_all(&root).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
            remove_target_without_following_links(&root.join("link")).unwrap();
            assert!(outside.join("keep").exists());
        }
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(outside);
    }

    #[test]
    fn gate_burn_recovery_is_fixed_root_idempotent_and_fail_closed() {
        let config = temp_root("gate-config");
        let local = temp_root("gate-local");
        std::fs::create_dir_all(config.join(HUB_CORE_DIR)).unwrap();
        std::fs::create_dir_all(local.join(PROFILE_DIR)).unwrap();
        std::fs::write(config.join(HUB_CORE_DIR).join("identity.json"), b"sealed").unwrap();
        std::fs::write(local.join(PROFILE_DIR).join("cache"), b"ciphertext").unwrap();
        keystore::set_base_dir_override(Some(config.join(HUB_CORE_DIR)));

        // Corrupt or attacker-authored state cannot authorize deletion.
        std::fs::write(config.join(GATE_BURN_JOURNAL), b"not a journal").unwrap();
        assert!(resume_interrupted_gate_burn(&config, &local).is_err());
        assert!(config.join(HUB_CORE_DIR).join("identity.json").exists());
        assert!(local.join(PROFILE_DIR).join("cache").exists());

        write_gate_burn_journal(&config).unwrap();
        assert!(resume_interrupted_gate_burn(&config, &local).unwrap());
        assert!(!config.join(HUB_CORE_DIR).exists());
        assert!(!local.join(PROFILE_DIR).exists());
        assert!(!config.join(GATE_BURN_JOURNAL).exists());
        // A completed recovery is a harmless no-op on the next launch.
        assert!(!resume_interrupted_gate_burn(&config, &local).unwrap());

        keystore::set_base_dir_override(None);
        let _ = std::fs::remove_dir_all(config);
        let _ = std::fs::remove_dir_all(local);
    }
}
