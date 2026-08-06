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
/// How many identity slots the enumerator will carry at once. Anything past it
/// is COUNTED as unconfirmed rather than dropped.
const MAX_IDENTITIES_SCANNED: usize = 1024;
/// Names the residue sweep is allowed to see: the burn's own recovery journal,
/// which exists precisely because the purge is in progress and which
/// [`remove_gate_burn_journal`] deletes as its final act. Nothing else is
/// exempt, and this is not an allowlist that may grow to settle a red sweep --
/// a new persisted-state site belongs in [`HubLocalState`], as `Purge` if OSL
/// can delete it and as `Retained` with a written reason if it cannot.
const RESIDUE_SWEEP_EXEMPT: &[&str] = &[GATE_BURN_JOURNAL, GATE_BURN_JOURNAL_TMP];
/// Distinct residue names carried into `failed_targets` before the report
/// collapses the tail into `local_residue_overflow`. Nothing is ever dropped:
/// exceeding this bound still produces a failed target, so the sweep can never
/// report a clean root it did not observe to be clean.
const MAX_REPORTED_RESIDUE: usize = 8;

#[derive(Debug, Clone, serde::Deserialize, Serialize)]
struct GateBurnJournal {
    version: u32,
    state: String,
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

pub const WINDOWS_REMOVE_PROGRAM_UNINSTALL_ARG: &str =
    "--osl-windows-remove-program-uninstall-step";
pub const WINDOWS_REMOVE_PROGRAM_UNINSTALL_STEP: &str = "windows-remove-program-uninstall";

struct CleanupTarget {
    id: &'static str,
    path: PathBuf,
}

/// Which trusted application root a persisted-state site hangs off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupRoot {
    /// Tauri's `app_config_dir()`.
    Config,
    /// Tauri's `app_local_data_dir()`.
    LocalData,
}

/// What a burn does with a registered persisted-state site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    /// The burn deletes it. Failure to delete becomes a `failed_target`.
    Purge,
    /// The burn deliberately leaves it, for the written reason. A retained
    /// entry is a DISCLOSED limitation, not an exemption: it is the only way a
    /// name can be absent from `failed_targets` without having been deleted,
    /// and the reason string is what the product has to be able to say out loud.
    Retained(&'static str),
}

/// **The single source of truth for OSL's local footprint.**
///
/// D-254: the deletion set used to be a hardcoded `Vec` built inline, so a new
/// persisted-state site was never even a *candidate* for `failed_targets` and
/// could not be reported as having survived. A second, drifting copy of the
/// same knowledge lived in a `full_cleanup_manifest()` nobody called.
///
/// One macro invocation now produces the enum, the `ALL` slice, and every
/// per-variant fact. A variant cannot be added without its root, its names and
/// its disposition, because the generated `match` arms are exhaustive and
/// `ALL` is generated from the same token list — there is no second list to
/// forget. `cleanup_targets` derives from this and holds no names of its own.
///
/// This closes drift *inside* the registry. Drift *outside* it — a new file
/// written under a root by code that never touched this table — is caught at
/// run time by [`residual_local_state`], which reports anything left standing
/// in either root after the purge as a `failed_target`.
macro_rules! hub_local_state {
    ($(
        $(#[$meta:meta])*
        $variant:ident {
            id: $id:expr,
            root: $root:ident,
            names: [$($name:expr),* $(,)?],
            prefixes: [$($prefix:expr),* $(,)?],
            disposition: $disposition:expr,
            contains: $contains:expr,
        }
    ),* $(,)?) => {
        /// Every local path OSL creates under one of its two application roots.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum HubLocalState {
            $($(#[$meta])* $variant),*
        }

        impl HubLocalState {
            /// Generated from the same token list as the enum itself, so it
            /// cannot fall behind it.
            pub const ALL: &'static [HubLocalState] = &[$(HubLocalState::$variant),*];

            pub const fn id(self) -> &'static str {
                match self { $(HubLocalState::$variant => $id),* }
            }

            pub const fn root(self) -> CleanupRoot {
                match self { $(HubLocalState::$variant => CleanupRoot::$root),* }
            }

            /// Exact entry names directly under [`Self::root`].
            pub const fn names(self) -> &'static [&'static str] {
                match self { $(HubLocalState::$variant => &[$($name),*]),* }
            }

            /// Name prefixes for entries carrying a process/sequence suffix.
            pub const fn prefixes(self) -> &'static [&'static str] {
                match self { $(HubLocalState::$variant => &[$($prefix),*]),* }
            }

            pub const fn disposition(self) -> Disposition {
                match self { $(HubLocalState::$variant => $disposition),* }
            }

            /// Plain-language description of what the site holds. This is the
            /// text any user-facing "what Fresh Start removes" copy must be
            /// derivable from.
            pub const fn contains(self) -> &'static str {
                match self { $(HubLocalState::$variant => $contains),* }
            }
        }
    };
}

hub_local_state! {
    HubCore {
        id: "hub_core",
        root: Config,
        names: [HUB_CORE_DIR],
        prefixes: [],
        disposition: Disposition::Purge,
        contains: "all OSL identities, sealed key material, the encrypted message store, People, TTL, blobs, prekeys, ratchet sessions, and local protected ledgers",
    },
    ServiceRegistry {
        id: "service_registry",
        root: Config,
        names: [
            "service-registry.json",
            "service-registry.json.bak",
            "service-registry.bak",
            "service-registry.tmp",
            "service-registry.json.legacy-untrusted",
            "service-registry.bak.legacy-untrusted",
        ],
        prefixes: ["service-registry.json.tmp-"],
        disposition: Disposition::Purge,
        contains: "linked-service labels and account profile registry",
    },
    ServiceScopeIndex {
        id: "service_scope_index",
        root: Config,
        names: [
            "service-scope-index.json",
            "service-scope-index.bak",
            "service-scope-index.tmp",
        ],
        prefixes: [],
        disposition: Disposition::Purge,
        contains: "encrypted service-account scope coverage and burn journals",
    },
    PreviewPreferences {
        id: "preview_preferences",
        root: Config,
        names: [
            "preview-preferences.json",
            "preview-preferences.json.tmp",
            "preview-preferences.bak",
            "preview-preferences.tmp",
        ],
        prefixes: [],
        disposition: Disposition::Purge,
        contains: "OSL Privacy onboarding, capture and composer preferences",
    },
    /// D-254: written by `tor_pref.rs:337` through `atomic_file`, which also
    /// leaves `.tmp` and `.bak` siblings. Survived every burn before D-254.
    TorPreference {
        id: "tor_preference",
        root: Config,
        names: ["tor-preference.json", "tor-preference.tmp", "tor-preference.bak"],
        prefixes: [],
        disposition: Disposition::Purge,
        contains: "the Tor/Arti network route preference",
    },
    /// D-254: the USB dead-man WIPE bindings (`main.rs:9735`). A burn that
    /// leaves these behind leaves a rule saying which USB stick triggers the
    /// next wipe.
    DeadmanBindings {
        id: "deadman_bindings",
        root: Config,
        names: ["deadman-bindings.json", "deadman-bindings.tmp", "deadman-bindings.bak"],
        prefixes: [],
        disposition: Disposition::Purge,
        contains: "USB dead-man volume-to-action bindings",
    },
    /// D-254: sealed record of which browsers were detected on this computer
    /// and what the user consented to (`main.rs:9779`, writer
    /// `browser_footprint.rs:184` — a plain write, so no `.tmp`/`.bak`).
    BrowserFootprint {
        id: "browser_footprint",
        root: Config,
        names: ["browser-footprint.json"],
        prefixes: [],
        disposition: Disposition::Purge,
        contains: "the sealed record of browsers detected on this computer and their consent bindings",
    },
    /// D-254: optional-component store (`main.rs:2003`), holding
    /// `components.json` and downloaded `component-payloads/`.
    Components {
        id: "components",
        root: Config,
        names: ["components-v1"],
        prefixes: [],
        disposition: Disposition::Purge,
        contains: "the optional-component state and downloaded component payloads",
    },
    /// D-254: sealed WhatsApp-QA machine password (`main.rs:1593`) plus its
    /// `create_new` staging files (`main.rs:1624`), which carry a nonce suffix.
    WhatsappQaDeviceSecret {
        id: "whatsapp_qa_device_secret",
        root: Config,
        names: ["whatsapp-qa-device-secret.v1"],
        prefixes: [".whatsapp-qa-device-secret.v1."],
        disposition: Disposition::Purge,
        contains: "the sealed WhatsApp QA device secret",
    },
    /// D-254: when the QA shell is active it re-roots config under this
    /// directory (`main.rs:9602`); when it is not, a directory left by an
    /// earlier QA build still holds a complete OSL config tree.
    DiscordQaShellConfig {
        id: "discord_qa_shell_config",
        root: Config,
        names: ["discord-qa-shell-v1"],
        prefixes: [],
        disposition: Disposition::Purge,
        contains: "the Discord QA shell's separate configuration tree",
    },
    ServiceProfiles {
        id: "service_profiles",
        root: LocalData,
        names: [PROFILE_DIR],
        prefixes: [],
        disposition: Disposition::Purge,
        contains: "OSL Privacy browser profiles, cookies, site storage, the embedded WebView2 user-data trees, the OSL-owned Firefox profile, and cleanup tombstones",
    },
    NativeProfiles {
        id: "native_profiles",
        root: LocalData,
        names: [NATIVE_PROFILE_DIR],
        prefixes: [],
        disposition: Disposition::Purge,
        contains: "OSL-owned isolated native-app profiles and their local sessions",
    },
    /// D-254, found by enumerating the footprint rather than by reading the
    /// old list: `native_window_host.rs:2972` writes per-channel ownership
    /// claims here and no burn has ever removed them.
    NativeDiscordChannelClaims {
        id: "native_discord_channel_claims",
        root: LocalData,
        names: ["native-discord-channel-claims-v1"],
        prefixes: [],
        disposition: Disposition::Purge,
        contains: "OSL's per-channel native Discord ownership claims",
    },
    /// D-254: the whole companion-browser profile tree — cookies and site
    /// storage for every browser OSL hosted (`browser_companion.rs:729`).
    BrowserCompanionProfiles {
        id: "browser_companion_profiles",
        root: LocalData,
        names: ["browser-companion-profiles-v1"],
        prefixes: [],
        disposition: Disposition::Purge,
        contains: "the companion-browser profile tree, including its cookies and site storage",
    },
    /// D-254: staged copies of browser History databases (`main.rs:9775`,
    /// `browser_profile_scan.rs:430`).
    BrowserProfileSnapshots {
        id: "browser_profile_snapshots",
        root: LocalData,
        names: ["browser-profile-snapshots"],
        prefixes: [],
        disposition: Disposition::Purge,
        contains: "staged copies of browser history databases and the scan state",
    },
    /// D-254: sealed and decrypted attachment staging
    /// (`peer_attachment_io.rs:20`).
    PeerAttachmentStaging {
        id: "peer_attachment_staging",
        root: LocalData,
        names: ["peer-attachment-staging"],
        prefixes: [],
        disposition: Disposition::Purge,
        contains: "staged sealed and decrypted peer attachments",
    },
    DiscordQaShellLocal {
        id: "discord_qa_shell_local",
        root: LocalData,
        names: ["discord-qa-shell-v1"],
        prefixes: [],
        disposition: Disposition::Purge,
        contains: "the Discord QA shell's separate local-data tree",
    },
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
    execute_full_hub_cleanup_with_key_material_wipe(
        core,
        app_config_dir,
        app_local_data_dir,
        service_hosts_shutdown,
        &KeyMaterialWipe::production(),
    )
}

/// Run the local uninstall cleanup used by Windows Add/Remove Programs.
///
/// This path is deliberately local-only. The Windows uninstaller may run after
/// the user has removed the app without unlocking OSL, so it cannot make remote
/// unregister promises. It still uses the same fixed cleanup target registry
/// and residue sweep as the in-app full cleanup, then NSIS removes the program
/// files and the uninstall registry entry.
pub fn execute_windows_remove_program_uninstall(
    app_config_dir: &Path,
    app_local_data_dir: &Path,
) -> Result<HubFullCleanupResult, String> {
    validate_windows_remove_program_roots(app_config_dir, app_local_data_dir)?;
    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(Some(app_config_dir.join(HUB_CORE_DIR)));
    let core = HubCoreState::default();
    identity_registry::reset_account_scoped_state(&core.osl);
    ipc::main_password::set_file_storage_key(None);

    let (mut removed_targets, mut failed_targets) =
        purge_fixed_targets(app_config_dir, app_local_data_dir);
    failed_targets.extend(residual_local_state(app_config_dir, app_local_data_dir));
    dedupe_in_place(&mut removed_targets);
    dedupe_in_place(&mut failed_targets);
    Ok(HubFullCleanupResult {
        local_cleanup_complete: failed_targets.is_empty(),
        removed_targets,
        failed_targets,
        remote_unregister: RemoteUnregisterSummary::default(),
        restart_required: false,
        original_discord_data_untouched: true,
    })
}

pub fn execute_windows_remove_program_uninstall_from_env() -> Result<HubFullCleanupResult, String> {
    let appdata = std::env::var_os("APPDATA")
        .ok_or_else(|| "APPDATA is required for OSL Windows uninstall cleanup".to_owned())?;
    let local_appdata = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| "LOCALAPPDATA is required for OSL Windows uninstall cleanup".to_owned())?;
    execute_windows_remove_program_uninstall(
        &PathBuf::from(appdata).join("org.oslprivacy.hub"),
        &PathBuf::from(local_appdata).join("org.oslprivacy.hub"),
    )
}

fn validate_windows_remove_program_roots(
    app_config_dir: &Path,
    app_local_data_dir: &Path,
) -> Result<(), String> {
    if !app_config_dir.is_absolute() || !app_local_data_dir.is_absolute() {
        return Err("OSL Windows uninstall roots must be absolute application paths".to_owned());
    }
    if app_config_dir.file_name().and_then(|name| name.to_str()) != Some("org.oslprivacy.hub")
        || app_local_data_dir
            .file_name()
            .and_then(|name| name.to_str())
            != Some("org.oslprivacy.hub")
    {
        return Err("OSL Windows uninstall refused a non-OSL application root".to_owned());
    }
    if app_config_dir.parent().is_none() || app_local_data_dir.parent().is_none() {
        return Err("OSL Windows uninstall refused a filesystem root".to_owned());
    }
    Ok(())
}

fn execute_full_hub_cleanup_with_key_material_wipe(
    core: &HubCoreState,
    app_config_dir: &Path,
    app_local_data_dir: &Path,
    service_hosts_shutdown: bool,
    key_material: &KeyMaterialWipe,
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

    let (mut removed_targets, mut failed_targets) =
        purge_fixed_targets(app_config_dir, app_local_data_dir);
    run_key_material_wipe(key_material, &mut removed_targets, &mut failed_targets);
    failed_targets.extend(residual_local_state(app_config_dir, app_local_data_dir));
    dedupe_in_place(&mut removed_targets);
    dedupe_in_place(&mut failed_targets);
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

    let (mut removed_targets, mut failed_targets) =
        purge_fixed_targets(app_config_dir, app_local_data_dir);
    run_key_material_wipe(
        &KeyMaterialWipe::production(),
        &mut removed_targets,
        &mut failed_targets,
    );
    failed_targets.extend(residual_local_state(app_config_dir, app_local_data_dir));
    dedupe_in_place(&mut removed_targets);
    dedupe_in_place(&mut failed_targets);
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
    let (mut removed, mut failed_targets) = purge_fixed_targets(app_config_dir, app_local_data_dir);
    run_key_material_wipe(
        &KeyMaterialWipe::production(),
        &mut removed,
        &mut failed_targets,
    );
    failed_targets.extend(residual_local_state(app_config_dir, app_local_data_dir));
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
    let (targets, enumeration_failures) = cleanup_targets(app_config_dir, app_local_data_dir);
    for id in enumeration_failures {
        failed_targets.push(id.to_owned());
    }
    for target in targets {
        match remove_target_without_following_links(&target.path) {
            // D-254: `Removed` used to include targets that were never there.
            // `Absent` is not destruction, and the list shown at
            // `fresh-start.ts:26` overstated what the burn had found.
            Ok(Removal::Removed) => removed_targets.push(target.id.to_owned()),
            Ok(Removal::Absent) => {}
            Err(()) => failed_targets.push(target.id.to_owned()),
        }
    }
    dedupe_in_place(&mut removed_targets);
    dedupe_in_place(&mut failed_targets);
    (removed_targets, failed_targets)
}

/// Preserve first-seen order while removing repeats. Registry ids are shared by
/// several paths (a file and its `.tmp`/`.bak` siblings), and the frontend
/// parser bounds both lists.
fn dedupe_in_place(ids: &mut Vec<String>) {
    let mut seen = HashSet::new();
    ids.retain(|id| seen.insert(id.clone()));
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

fn root_path<'a>(
    root: CleanupRoot,
    app_config_dir: &'a Path,
    app_local_data_dir: &'a Path,
) -> &'a Path {
    match root {
        CleanupRoot::Config => app_config_dir,
        CleanupRoot::LocalData => app_local_data_dir,
    }
}

/// Does `name` belong to a registered persisted-state site?
fn registered_state_for(root: CleanupRoot, name: &str) -> Option<HubLocalState> {
    HubLocalState::ALL.iter().copied().find(|state| {
        state.root() == root
            && (state.names().contains(&name)
                || state
                    .prefixes()
                    .iter()
                    .any(|prefix| name.starts_with(prefix)))
    })
}

/// Every path a burn deletes, derived entirely from [`HubLocalState`].
///
/// The second element is the set of ids whose enumeration could not be
/// completed. **D-254: this used to be `if let Ok(entries)` with a silent
/// `.take(256)`** — an unreadable root or a 257th staging file was skipped
/// without ever reaching `failed_targets`, so cleanup reported success for
/// work it had not done. An enumeration that cannot be completed is now a
/// failure, because a target that was never listed cannot have been deleted.
fn cleanup_targets(
    app_config_dir: &Path,
    app_local_data_dir: &Path,
) -> (Vec<CleanupTarget>, Vec<&'static str>) {
    let mut targets = Vec::new();
    let mut enumeration_failures = Vec::new();

    for state in HubLocalState::ALL.iter().copied() {
        if state.disposition() != Disposition::Purge {
            continue;
        }
        let root = root_path(state.root(), app_config_dir, app_local_data_dir);
        for name in state.names() {
            targets.push(CleanupTarget {
                id: state.id(),
                path: root.join(name),
            });
        }
    }

    // Prefix-matched staging names carry process/sequence suffixes, so they can
    // only be found by enumerating the root. Every entry is examined; nothing
    // is truncated away.
    for root in [CleanupRoot::Config, CleanupRoot::LocalData] {
        let dir = root_path(root, app_config_dir, app_local_data_dir);
        let has_prefixes = HubLocalState::ALL
            .iter()
            .any(|state| state.root() == root && !state.prefixes().is_empty());
        if !has_prefixes {
            continue;
        }
        match std::fs::read_dir(dir) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => enumeration_failures.push("staging_enumeration"),
            Ok(entries) => {
                for entry in entries {
                    let Ok(entry) = entry else {
                        enumeration_failures.push("staging_enumeration");
                        continue;
                    };
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    if name.len() > 160
                        || !name.bytes().all(|byte| {
                            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')
                        })
                    {
                        continue;
                    }
                    let matched = HubLocalState::ALL.iter().copied().find(|state| {
                        state.root() == root
                            && state.disposition() == Disposition::Purge
                            && state
                                .prefixes()
                                .iter()
                                .any(|prefix| name.starts_with(prefix))
                    });
                    if let Some(state) = matched {
                        targets.push(CleanupTarget {
                            id: state.id(),
                            path: entry.path(),
                        });
                    }
                }
            }
        }
    }

    (targets, enumeration_failures)
}

/// Anything still standing in either trusted root after the purge.
///
/// **This is the drift gate.** `cleanup_targets` can only delete what the
/// registry names; this sweep observes what is actually LEFT. A persisted-state
/// site added anywhere in the app without a matching [`HubLocalState`] entry
/// survives the purge, is seen here, and lands in `failed_targets` — so Fresh
/// Start reports "partial" instead of reporting a completeness it never
/// established. Fail-closed: a root that cannot be read is reported as
/// unverified rather than assumed clean.
fn residual_local_state(app_config_dir: &Path, app_local_data_dir: &Path) -> Vec<String> {
    let mut residue: Vec<String> = Vec::new();
    let mut overflow = false;
    let mut unverified = false;

    for root in [CleanupRoot::Config, CleanupRoot::LocalData] {
        let dir = root_path(root, app_config_dir, app_local_data_dir);
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => {
                unverified = true;
                continue;
            }
        };
        for entry in entries {
            let Ok(entry) = entry else {
                unverified = true;
                continue;
            };
            let name = entry.file_name();
            let name = name.to_string_lossy().into_owned();
            if RESIDUE_SWEEP_EXEMPT.contains(&name.as_str()) {
                continue;
            }
            // A registered `Retained` site is a disclosed limitation, so it is
            // not residue. A registered `Purge` site standing here IS residue:
            // its own removal already failed and reported itself, and the
            // sweep agreeing costs nothing.
            if let Some(state) = registered_state_for(root, &name) {
                if let Disposition::Retained(_) = state.disposition() {
                    continue;
                }
            }
            let id = format!("local_residue:{name}");
            if residue.contains(&id) {
                continue;
            }
            if residue.len() >= MAX_REPORTED_RESIDUE {
                overflow = true;
                continue;
            }
            residue.push(id);
        }
    }

    if overflow {
        residue.push("local_residue_overflow".to_owned());
    }
    if unverified {
        residue.push("local_residue_unverified".to_owned());
    }
    residue
}

/// Outcome of one mandatory key-material wipe step.
enum KeyMaterialOutcome {
    /// Key material existed and was destroyed.
    Wiped,
    /// There was nothing of this kind on this machine to destroy.
    Absent,
    Failed,
}

/// The two local wipe steps `crates/keystore/src/duress.rs:34-35` classifies as
/// **mandatory** — `WipeStep::TpmEvict` and `WipeStep::KeyringPurge`.
///
/// D-254: Fresh Start ran neither. `cleanup.rs` only cleared in-process state
/// (`reset_account_scoped_state`, `set_file_storage_key(None)`), so the
/// NCrypt/TPM-persisted key and the Windows Credential Manager entry — the key
/// every `identity.json` on the machine is sealed with — both survived a burn
/// whose copy said every decrypt key on the computer was removed.
///
/// Boxed rather than called directly so a test can drive the failure branch and
/// prove the report refuses to say "complete".
pub struct KeyMaterialWipe {
    evict_tpm_key: Box<dyn Fn() -> KeyMaterialOutcome + Send + Sync>,
    purge_keyring_entry: Box<dyn Fn() -> KeyMaterialOutcome + Send + Sync>,
}

impl KeyMaterialWipe {
    /// The production binding.
    ///
    /// **D-142 applies here exactly as it applies to
    /// `crates/ipc/src/state.rs:648`.** `KeyringSealer::purge_keyring_entry()`
    /// deletes ONE machine-wide credential, and that credential is what every
    /// `identity.json` on the machine is sealed with. Deleting it is the point
    /// of a burn and catastrophic anywhere else — a unit test that reached it
    /// would destroy the developer's real identity and every other install on
    /// the box, which is how D-142 presented. So under `cfg(test)` this binds a
    /// purge scoped to a namespace unique to the test process: the step is
    /// still genuinely exercised against the real keyring code path, it just
    /// cannot reach the production credential.
    fn production() -> Self {
        Self {
            evict_tpm_key: Box::new(|| match keystore::evict_tpm_key() {
                Ok(keystore::TpmEvictOutcome::Evicted) => KeyMaterialOutcome::Wiped,
                Ok(keystore::TpmEvictOutcome::NoTpmNothingToEvict) => KeyMaterialOutcome::Absent,
                Err(_) => KeyMaterialOutcome::Failed,
            }),
            purge_keyring_entry: Box::new(|| {
                #[cfg(not(test))]
                let purged = keystore::KeyringSealer::purge_keyring_entry_namespaced_reporting("");
                #[cfg(test)]
                let purged = keystore::KeyringSealer::purge_keyring_entry_namespaced_reporting(
                    &test_keyring_namespace(),
                );
                match purged {
                    Ok(keystore::KeyringPurgeOutcome::Purged) => KeyMaterialOutcome::Wiped,
                    Ok(keystore::KeyringPurgeOutcome::NoEntryNothingToPurge) => {
                        KeyMaterialOutcome::Absent
                    }
                    Err(_) => KeyMaterialOutcome::Failed,
                }
            }),
        }
    }
}

#[cfg(test)]
fn test_keyring_namespace() -> String {
    format!("osl-hub-cleanup-test-{}", std::process::id())
}

/// Run the mandatory key-material wipe, recording each step by id.
///
/// Fail-closed: a step that errors is a `failed_target`, so
/// `local_cleanup_complete` is false and Fresh Start cannot report success.
fn run_key_material_wipe(
    wipe: &KeyMaterialWipe,
    removed_targets: &mut Vec<String>,
    failed_targets: &mut Vec<String>,
) {
    for (id, outcome) in [
        ("tpm_persisted_key", (wipe.evict_tpm_key)()),
        ("os_keyring_credential", (wipe.purge_keyring_entry)()),
    ] {
        match outcome {
            KeyMaterialOutcome::Wiped => removed_targets.push(id.to_owned()),
            // Nothing of this kind exists on this machine. Reporting it as
            // "removed" would be the `:482` defect: claiming destruction of an
            // object that was never there.
            KeyMaterialOutcome::Absent => {}
            KeyMaterialOutcome::Failed => failed_targets.push(id.to_owned()),
        }
    }
}

fn collect_identities(
    core: &HubCoreState,
    app_config_dir: &Path,
) -> Result<(Vec<keystore::Identity>, usize), String> {
    collect_identities_with_sealer(core, app_config_dir, || {
        crate::password_lifecycle::persistent_sealer()
    })
}

/// The sealer is supplied by the caller so a test can reach the
/// `MAX_IDENTITIES` bound with identities that genuinely OPEN. D-142: the
/// production sealer is the machine-global keyring credential, so no test may
/// construct it.
fn collect_identities_with_sealer<F>(
    core: &HubCoreState,
    app_config_dir: &Path,
    sealer_for: F,
) -> Result<(Vec<keystore::Identity>, usize), String>
where
    F: FnOnce() -> Result<Box<dyn keystore::Sealer>, String>,
{
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
    // D-254: `identity_paths` truncated its own read_dir at `MAX_IDENTITIES + 1`
    // AS WELL, so slots past that were dropped before `collect_identities` could
    // even see them. Both bounds now report what they walked away from.
    let (paths, beyond_scan) = identity_paths(&base);
    let Ok(sealer) = sealer_for() else {
        let unreadable = paths
            .len()
            .saturating_sub(identities.len())
            .saturating_add(beyond_scan);
        return Ok((identities, unreadable));
    };
    let mut unreadable = beyond_scan;
    for (index, path) in paths.iter().enumerate() {
        // D-254: this used to `break` at the bound WITHOUT counting what it was
        // walking away from, so a 17th identity was never remote-unregistered,
        // never counted, and `unconfirmedRemote` stayed 0 — leaving the success
        // line showing. Everything past the bound is unconfirmed by definition.
        if identities.len() >= MAX_IDENTITIES {
            unreadable = unreadable.saturating_add(paths.len().saturating_sub(index));
            break;
        }
        match keystore::load_identity(path, sealer.as_ref()) {
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

/// Identity blobs under `base`, plus the number of slots the scan refused to
/// carry. A slot that is not returned has not been unregistered, and saying so
/// is the difference between a bound and a silent loss.
fn identity_paths(base: &Path) -> (Vec<PathBuf>, usize) {
    let mut paths = Vec::new();
    let mut beyond_scan = 0usize;
    let flat = base.join("identity.json");
    if flat.is_file() {
        paths.push(flat);
    }
    if let Ok(entries) = std::fs::read_dir(base.join("hub-identities")) {
        for entry in entries {
            let Ok(entry) = entry else {
                beyond_scan = beyond_scan.saturating_add(1);
                continue;
            };
            if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            let identity = entry.path().join("identity.json");
            if !identity.is_file() {
                continue;
            }
            if paths.len() >= MAX_IDENTITIES_SCANNED {
                beyond_scan = beyond_scan.saturating_add(1);
                continue;
            }
            paths.push(identity);
        }
    }
    (paths, beyond_scan)
}

/// Whether a target was destroyed or was never there. Conflating the two is
/// how a cleanup report claims work it did not do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Removal {
    Removed,
    Absent,
}

fn remove_target_without_following_links(path: &Path) -> Result<Removal, ()> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Removal::Absent),
        Err(_) => return Err(()),
    };
    if metadata.file_type().is_symlink() || metadata.is_file() {
        std::fs::remove_file(path)
            .map(|()| Removal::Removed)
            .map_err(|_| ())
    } else if metadata.is_dir() {
        std::fs::remove_dir_all(path)
            .map(|()| Removal::Removed)
            .map_err(|_| ())
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

    /// Replaces `manifest_explicitly_covers_new_hub_artifacts`, whose subject
    /// (`full_cleanup_manifest`) was DEAD CODE and a second list that could
    /// drift from `cleanup_targets` with nothing catching it. It asserted five
    /// booleans that no burn ever read. These assertions are over the registry
    /// the burn actually uses.
    #[test]
    fn the_registry_is_the_only_list_and_covers_the_named_hub_artifacts() {
        // Every artifact the deleted manifest advertised is still covered, and
        // now by the same table the deletion loop walks.
        for id in [
            "hub_core",
            "service_profiles",
            "native_profiles",
            "service_registry",
            "service_scope_index",
            "preview_preferences",
        ] {
            assert!(
                HubLocalState::ALL.iter().any(|state| state.id() == id),
                "the cleanup registry lost {id}"
            );
        }
        // The registry is the whole deletion set: no id appears in a burn that
        // is not derived from it, and every Purge entry names at least one path.
        let config = temp_root("registry-config");
        let local = temp_root("registry-local");
        let (targets, failures) = cleanup_targets(&config, &local);
        assert!(failures.is_empty());
        let registry_ids: HashSet<&str> = HubLocalState::ALL
            .iter()
            .filter(|state| state.disposition() == Disposition::Purge)
            .map(|state| state.id())
            .collect();
        for target in &targets {
            assert!(
                registry_ids.contains(target.id),
                "target {} is not derived from HubLocalState",
                target.id
            );
        }
        for state in HubLocalState::ALL.iter().copied() {
            assert!(
                !state.names().is_empty() || !state.prefixes().is_empty(),
                "{} names nothing",
                state.id()
            );
            assert!(
                !state.contains().is_empty(),
                "{} says nothing about what it holds",
                state.id()
            );
            if state.disposition() == Disposition::Purge {
                assert!(
                    targets.iter().any(|target| target.id == state.id()),
                    "{} is registered to be purged but produced no target",
                    state.id()
                );
            }
        }
    }

    /// The frontend refuses a cleanup result whose lists exceed its bounds
    /// (`adapters.ts:parseFullCleanup`), and a refused result is shown as
    /// "no verifiable result". Growing the registry past that bound would turn
    /// a successful burn into an unreadable one, so the bound is asserted here
    /// rather than discovered in production.
    #[test]
    fn every_reportable_id_stays_within_the_frontend_parser_bound() {
        const FRONTEND_LIST_BOUND: usize = 64;
        const FRONTEND_ID_BYTES: usize = 80;
        let mut ids: Vec<String> = HubLocalState::ALL
            .iter()
            .map(|state| state.id().to_owned())
            .collect();
        ids.push("staging_enumeration".to_owned());
        ids.push("tpm_persisted_key".to_owned());
        ids.push("os_keyring_credential".to_owned());
        ids.push("local_residue_overflow".to_owned());
        ids.push("local_residue_unverified".to_owned());
        // Worst case: every registry entry fails AND the sweep reports its
        // full residue budget.
        let worst_case = ids.len() + MAX_REPORTED_RESIDUE;
        assert!(
            worst_case <= FRONTEND_LIST_BOUND,
            "a burn could report {worst_case} ids; the frontend parser accepts {FRONTEND_LIST_BOUND}"
        );
        for id in &ids {
            assert!(
                id.len() <= FRONTEND_ID_BYTES,
                "id {id} is too long to report"
            );
        }
        // Ids must be unique, or `failed_targets` cannot say which site survived.
        let distinct: HashSet<&String> = ids.iter().collect();
        assert_eq!(
            distinct.len(),
            ids.len(),
            "two registry entries share an id"
        );
    }

    #[test]
    fn fixed_targets_cannot_reach_original_discord_directory() {
        let config = temp_root("config");
        let local = temp_root("local");
        let original = config.parent().unwrap().join("osl");
        let (targets, _) = cleanup_targets(&config, &local);
        for target in targets {
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
            .0
            .into_iter()
            .find(|target| target.id == "service_profiles")
            .expect("service profile cleanup target exists");
        assert!(firefox.starts_with(&service_profiles.path));
        assert_ne!(firefox, service_profiles.path);
        assert_ne!(firefox, other);
    }

    /// Every filename OSL actually persists directly under one of its two
    /// application roots, with the writer that creates it.
    ///
    /// This list is deliberately NOT derived from [`HubLocalState`]: it is the
    /// independent half of the gate. Dropping an entry from the registry, or
    /// mistyping one of its names, makes this RED — which is the mutation
    /// "remove a newly-wired wipe" must fail on. Adding a genuinely new site to
    /// the app is caught the other way, at run time, by
    /// [`residual_local_state`].
    const APP_PERSISTED_NAMES: &[(CleanupRoot, &str)] = &[
        (CleanupRoot::Config, "osl-core"),                 // main.rs:9613
        (CleanupRoot::Config, "preview-preferences.json"), // main.rs:9642
        (CleanupRoot::Config, "tor-preference.json"),      // main.rs:9646
        (CleanupRoot::Config, "service-registry.json"),    // main.rs:9650
        (CleanupRoot::Config, "service-scope-index.json"), // main.rs:9654
        (CleanupRoot::Config, "deadman-bindings.json"),    // main.rs:9735
        (CleanupRoot::Config, "browser-footprint.json"),   // main.rs:9779
        (CleanupRoot::Config, "components-v1"),            // main.rs:2003
        (CleanupRoot::Config, "whatsapp-qa-device-secret.v1"), // main.rs:1593
        (
            CleanupRoot::Config,
            ".whatsapp-qa-device-secret.v1.a1b2.tmp",
        ), // main.rs:1624
        (CleanupRoot::Config, "discord-qa-shell-v1"),      // main.rs:9602
        (CleanupRoot::Config, "service-registry.json.tmp-7-3"), // services.rs staging
        (CleanupRoot::LocalData, "service-profiles-v2"),   // service_host.rs:1181
        (CleanupRoot::LocalData, "native-window-profiles-v1"), // native_window_host.rs:43
        (CleanupRoot::LocalData, "native-discord-channel-claims-v1"), // native_window_host.rs:2958
        (CleanupRoot::LocalData, "browser-companion-profiles-v1"), // browser_companion.rs:729
        (CleanupRoot::LocalData, "browser-profile-snapshots"), // main.rs:9775
        (CleanupRoot::LocalData, "peer-attachment-staging"), // peer_attachment_io.rs:20
        (CleanupRoot::LocalData, "discord-qa-shell-v1"),   // main.rs:9626
    ];

    /// The registry must account for every name the app writes, and account for
    /// it as something a burn destroys.
    #[test]
    fn the_registry_accounts_for_every_name_the_app_persists() {
        for (root, name) in APP_PERSISTED_NAMES {
            let state = registered_state_for(*root, name).unwrap_or_else(|| {
                panic!("{name} is written under {root:?} and no HubLocalState entry covers it")
            });
            assert_eq!(
                state.disposition(),
                Disposition::Purge,
                "{name} is registered but a burn does not destroy it, and nothing says why"
            );
        }
    }

    /// Seed one file under every registered `Purge` name and prove the burn
    /// removes all of them and leaves the roots empty.
    ///
    /// Deleting a registry entry's names, or flipping it to `Retained`, makes
    /// this go RED — the file it seeded is still standing at the end.
    #[test]
    fn every_registered_purge_target_is_actually_removed() {
        let _serial = crate::global_keystore_test_lock();
        let config = temp_root("purge-config");
        let local = temp_root("purge-local");
        std::fs::create_dir_all(config.join(HUB_CORE_DIR)).unwrap();
        std::fs::create_dir_all(&local).unwrap();
        keystore::set_base_dir_override(Some(config.join(HUB_CORE_DIR)));

        let mut seeded: Vec<PathBuf> = Vec::new();
        for state in HubLocalState::ALL.iter().copied() {
            if state.disposition() != Disposition::Purge {
                continue;
            }
            let root = root_path(state.root(), &config, &local);
            for name in state.names() {
                let path = root.join(name);
                if path.exists() {
                    continue;
                }
                std::fs::create_dir_all(&path).unwrap();
                std::fs::write(path.join("seeded"), b"local state").unwrap();
                seeded.push(path);
            }
            for prefix in state.prefixes() {
                let path = root.join(format!("{prefix}0001"));
                std::fs::write(&path, b"staged").unwrap();
                seeded.push(path);
            }
        }
        assert!(!seeded.is_empty());
        std::fs::write(config.join(HUB_CORE_DIR).join("identity.json"), b"sealed").unwrap();

        write_gate_burn_journal(&config).unwrap();
        assert!(resume_interrupted_gate_burn(&config, &local).unwrap());

        for path in &seeded {
            assert!(!path.exists(), "{} survived the burn", path.display());
        }
        assert!(residual_local_state(&config, &local).is_empty());

        keystore::set_base_dir_override(None);
        let _ = std::fs::remove_dir_all(config);
        let _ = std::fs::remove_dir_all(local);
    }

    /// **THE DRIFT GATE.** A persisted-state site added anywhere in the app
    /// without a matching [`HubLocalState`] entry survives the fixed list. The
    /// residue sweep observes what is LEFT rather than what was listed, so the
    /// unregistered file lands in `failed_targets` and the burn refuses to
    /// report completion.
    ///
    /// Deleting the `residual_local_state` call from any burn path makes this
    /// RED. This is the assertion that D-254's "closed list" cannot come back:
    /// the list stopped being the source of truth for completeness.
    #[test]
    fn an_unregistered_persisted_state_site_is_reported_rather_than_missed() {
        let _serial = crate::global_keystore_test_lock();
        let config = temp_root("drift-config");
        let local = temp_root("drift-local");
        std::fs::create_dir_all(config.join(HUB_CORE_DIR)).unwrap();
        std::fs::create_dir_all(&local).unwrap();
        keystore::set_base_dir_override(Some(config.join(HUB_CORE_DIR)));

        // Exactly the D-254 shape: a future lane persists new local state and
        // never touches the cleanup registry.
        std::fs::write(
            config.join("a-new-preference-nobody-registered.json"),
            b"{}",
        )
        .unwrap();
        std::fs::create_dir_all(local.join("a-new-profile-tree-nobody-registered")).unwrap();

        let (_, mut failed) = purge_fixed_targets(&config, &local);
        assert!(
            failed.is_empty(),
            "the fixed list cannot even see the new sites: {failed:?}"
        );
        failed.extend(residual_local_state(&config, &local));
        assert!(
            failed
                .iter()
                .any(|id| id == "local_residue:a-new-preference-nobody-registered.json"),
            "an unregistered config-root site was not reported: {failed:?}"
        );
        assert!(
            failed
                .iter()
                .any(|id| id == "local_residue:a-new-profile-tree-nobody-registered"),
            "an unregistered local-data-root site was not reported: {failed:?}"
        );
        // A burn path must refuse to complete while residue stands.
        write_gate_burn_journal(&config).unwrap();
        assert!(
            resume_interrupted_gate_burn(&config, &local).is_err(),
            "a burn reported success with unregistered local state still on disk"
        );

        keystore::set_base_dir_override(None);
        let _ = std::fs::remove_dir_all(config);
        let _ = std::fs::remove_dir_all(local);
    }

    /// The residue sweep must never report a root it could not read as clean,
    /// and must never silently drop residue it ran out of room to name.
    #[test]
    fn the_residue_sweep_fails_closed_on_overflow_and_on_an_unreadable_root() {
        let config = temp_root("residue-config");
        let local = temp_root("residue-local");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::create_dir_all(&local).unwrap();
        for index in 0..(MAX_REPORTED_RESIDUE + 4) {
            std::fs::write(config.join(format!("unregistered-{index}.json")), b"{}").unwrap();
        }
        let residue = residual_local_state(&config, &local);
        assert_eq!(
            residue
                .iter()
                .filter(|id| id.starts_with("local_residue:"))
                .count(),
            MAX_REPORTED_RESIDUE
        );
        assert!(residue.iter().any(|id| id == "local_residue_overflow"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let sealed = temp_root("residue-sealed");
            std::fs::create_dir_all(&sealed).unwrap();
            std::fs::set_permissions(&sealed, std::fs::Permissions::from_mode(0o000)).unwrap();
            let unreadable = residual_local_state(&sealed, &local);
            // Running as root defeats the mode bits; only assert when the
            // refusal is actually reproducible.
            if std::fs::read_dir(&sealed).is_err() {
                assert!(
                    unreadable.iter().any(|id| id == "local_residue_unverified"),
                    "an unreadable root was treated as clean: {unreadable:?}"
                );
            }
            std::fs::set_permissions(&sealed, std::fs::Permissions::from_mode(0o755)).unwrap();
            let _ = std::fs::remove_dir_all(sealed);
        }

        let _ = std::fs::remove_dir_all(config);
        let _ = std::fs::remove_dir_all(local);
    }

    /// D-254's sharpest miss: the two steps `duress.rs:34-35` classifies as
    /// MANDATORY were not run by Fresh Start at all.
    ///
    /// Removing either call from `run_key_material_wipe` makes this RED.
    #[test]
    fn both_mandatory_key_material_steps_run_on_every_burn() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let tpm = Arc::new(AtomicUsize::new(0));
        let keyring = Arc::new(AtomicUsize::new(0));
        let tpm_for_wipe = Arc::clone(&tpm);
        let keyring_for_wipe = Arc::clone(&keyring);
        let wipe = KeyMaterialWipe {
            evict_tpm_key: Box::new(move || {
                tpm_for_wipe.fetch_add(1, Ordering::SeqCst);
                KeyMaterialOutcome::Wiped
            }),
            purge_keyring_entry: Box::new(move || {
                keyring_for_wipe.fetch_add(1, Ordering::SeqCst);
                KeyMaterialOutcome::Wiped
            }),
        };
        let mut removed = Vec::new();
        let mut failed = Vec::new();
        run_key_material_wipe(&wipe, &mut removed, &mut failed);

        assert_eq!(tpm.load(Ordering::SeqCst), 1, "TPM eviction did not run");
        assert_eq!(
            keyring.load(Ordering::SeqCst),
            1,
            "keyring purge did not run"
        );
        assert!(failed.is_empty());
        assert!(removed.iter().any(|id| id == "tpm_persisted_key"));
        assert!(removed.iter().any(|id| id == "os_keyring_credential"));
    }

    /// Fail-closed: a mandatory step that FAILS must reach `failed_targets`, so
    /// `local_cleanup_complete` is false and Fresh Start cannot show the
    /// success line. This is the property the closed list could not have —
    /// key material was never a candidate for failure at all.
    #[test]
    fn a_failed_mandatory_key_material_step_refuses_to_report_complete() {
        let _serial = crate::global_keystore_test_lock();
        let config = temp_root("keymat-config");
        let local = temp_root("keymat-local");
        std::fs::create_dir_all(config.join(HUB_CORE_DIR)).unwrap();
        std::fs::create_dir_all(&local).unwrap();
        keystore::set_base_dir_override(Some(config.join(HUB_CORE_DIR)));
        ipc::main_password::set_file_storage_key(Some([0x21; 32]));

        let core = crate::core_bridge::HubCoreState::default();
        let failing = KeyMaterialWipe {
            evict_tpm_key: Box::new(|| KeyMaterialOutcome::Failed),
            purge_keyring_entry: Box::new(|| KeyMaterialOutcome::Wiped),
        };
        let result =
            execute_full_hub_cleanup_with_key_material_wipe(&core, &config, &local, true, &failing)
                .expect("cleanup runs");
        assert!(
            result
                .failed_targets
                .iter()
                .any(|id| id == "tpm_persisted_key"),
            "a failed TPM eviction was not reported: {:?}",
            result.failed_targets
        );
        assert!(
            !result.local_cleanup_complete,
            "cleanup reported complete while a MANDATORY wipe step had failed"
        );

        // The same run with both steps succeeding does complete, so the
        // assertion above is about the failure and not about the fixture.
        let succeeding = KeyMaterialWipe {
            evict_tpm_key: Box::new(|| KeyMaterialOutcome::Wiped),
            purge_keyring_entry: Box::new(|| KeyMaterialOutcome::Wiped),
        };
        ipc::main_password::set_file_storage_key(Some([0x21; 32]));
        std::fs::create_dir_all(config.join(HUB_CORE_DIR)).unwrap();
        let clean = execute_full_hub_cleanup_with_key_material_wipe(
            &core,
            &config,
            &local,
            true,
            &succeeding,
        )
        .expect("cleanup runs");
        assert!(
            clean.local_cleanup_complete,
            "the control run did not complete: {:?}",
            clean.failed_targets
        );

        ipc::main_password::set_file_storage_key(None);
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
        let _ = std::fs::remove_dir_all(config);
        let _ = std::fs::remove_dir_all(local);
    }

    /// A target that was never there was reported as `removed`, so the list
    /// shown at `fresh-start.ts:26` overstated what the burn found.
    #[test]
    fn a_target_that_was_never_there_is_not_reported_as_removed() {
        let config = temp_root("absent-config");
        let local = temp_root("absent-local");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::create_dir_all(&local).unwrap();
        // Nothing seeded: every registered target is absent.
        let (removed, failed) = purge_fixed_targets(&config, &local);
        assert!(failed.is_empty());
        assert!(
            removed.is_empty(),
            "absent targets were reported as removed: {removed:?}"
        );

        // Seed exactly one and it becomes the only thing claimed.
        std::fs::write(config.join("tor-preference.json"), b"{}").unwrap();
        let (removed, failed) = purge_fixed_targets(&config, &local);
        assert!(failed.is_empty());
        assert_eq!(removed, vec!["tor_preference".to_owned()]);

        let _ = std::fs::remove_dir_all(config);
        let _ = std::fs::remove_dir_all(local);
    }

    /// `collect_identities` used to `break` at `MAX_IDENTITIES` without
    /// incrementing `unreadable`, so a 17th identity was never
    /// remote-unregistered, never counted, and the frontend's
    /// `unconfirmedRemote == 0` check still showed the success line.
    /// `collect_identities` used to `break` at `MAX_IDENTITIES` without
    /// incrementing `unreadable`, so a 17th identity was never
    /// remote-unregistered, never counted, and the frontend's
    /// `unconfirmedRemote == 0` check still showed the success line.
    ///
    /// The identities here are sealed with a `MemorySealer` and genuinely
    /// OPEN, which is the only way the bound is reached at all. Seeding
    /// unopenable bytes makes every slot `unreadable` and the loop never gets
    /// there — that version of this test passed with the defect restored.
    #[test]
    fn identities_past_the_enumeration_bound_are_counted_as_unconfirmed() {
        let _serial = crate::global_keystore_test_lock();
        let config = temp_root("identities-config");
        let base = config.join(HUB_CORE_DIR);
        let slots = base.join("hub-identities");
        std::fs::create_dir_all(&slots).unwrap();
        let sealer = keystore::MemorySealer::new();
        let overflow = 3usize;
        for index in 0..(MAX_IDENTITIES + overflow) {
            let slot = slots.join(format!("slot-{index:02}"));
            std::fs::create_dir_all(&slot).unwrap();
            let identity = keystore::generate_identity(format!("owner-{index:02}"));
            keystore::save_identity(&slot.join("identity.json"), &identity, &sealer).unwrap();
        }
        keystore::set_base_dir_override(Some(base.clone()));

        let core = crate::core_bridge::HubCoreState::default();
        let (scanned, beyond) = identity_paths(&base);
        let paths = scanned.len() + beyond;
        assert_eq!(paths, MAX_IDENTITIES + overflow);
        let (identities, unreadable) = collect_identities_with_sealer(&core, &config, move || {
            Ok(Box::new(sealer) as Box<dyn keystore::Sealer>)
        })
        .expect("enumerate");

        assert_eq!(
            identities.len(),
            MAX_IDENTITIES,
            "the bound was never reached, so this test cannot see the defect"
        );
        assert_eq!(
            identities.len() + unreadable,
            paths,
            "{unreadable} counted unconfirmed, but {} identities were walked away from",
            paths - identities.len()
        );

        keystore::set_base_dir_override(None);
        let _ = std::fs::remove_dir_all(config);
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
        // Overrides the process-wide keystore base dir below, so it must serialise
        // against every other test that touches those statics.
        let _serial = crate::global_keystore_test_lock();
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

        let foreign_config = temp_root("gate-foreign-config");
        let foreign_local = temp_root("gate-foreign-local");
        std::fs::create_dir_all(foreign_config.join(HUB_CORE_DIR)).unwrap();
        std::fs::create_dir_all(foreign_local.join(PROFILE_DIR)).unwrap();
        std::fs::write(
            foreign_config.join(HUB_CORE_DIR).join("identity.json"),
            b"foreign sealed",
        )
        .unwrap();
        std::fs::write(foreign_local.join(PROFILE_DIR).join("cache"), b"foreign").unwrap();
        write_gate_burn_journal(&foreign_config).unwrap();
        assert!(resume_interrupted_gate_burn(&foreign_config, &foreign_local).is_err());
        assert!(foreign_config
            .join(HUB_CORE_DIR)
            .join("identity.json")
            .exists());
        assert!(foreign_local.join(PROFILE_DIR).join("cache").exists());

        keystore::set_active_account_dir(Some(config.join("accounts").join("stale")));
        ipc::main_password::set_file_storage_key(Some([0x44; 32]));
        write_gate_burn_journal(&config).unwrap();
        assert!(resume_interrupted_gate_burn(&config, &local).unwrap());
        assert!(!config.join(HUB_CORE_DIR).exists());
        assert!(!local.join(PROFILE_DIR).exists());
        assert!(!config.join(GATE_BURN_JOURNAL).exists());
        assert!(ipc::main_password::get_file_storage_key().is_none());
        assert!(keystore::active_account_dir().is_none());
        // A completed recovery is a harmless no-op on the next launch.
        assert!(!resume_interrupted_gate_burn(&config, &local).unwrap());

        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
        let _ = std::fs::remove_dir_all(config);
        let _ = std::fs::remove_dir_all(local);
        let _ = std::fs::remove_dir_all(foreign_config);
        let _ = std::fs::remove_dir_all(foreign_local);
    }
}
