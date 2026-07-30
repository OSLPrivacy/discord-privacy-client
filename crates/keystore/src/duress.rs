//! Duress-engine primitives.
//!
//! Spec: `docs/design/unlock-and-duress.md` "Duress flow — full
//! specification" + `docs/design/build-order.md` Layer B3.
//!
//! IPC constructs a production [`DuressEngine`] in application state and the
//! Hub password gate invokes it for duress outcomes. The separate burn-password
//! path still uses `startup_gate` and `cleanup`, not this engine.
//!
//! The engine contract, when explicitly driven, has four phases:
//!
//! 1. **Apparent unlock** — an intended UI integration concern, not driven
//!    from this engine.
//! 2. **Local burn** — the engine attempts its configured synchronous wipe
//!    steps. Each step is idempotent within this engine.
//! 3. **Strip OPSEC features** — a caller-supplied callback can delete
//!    injection/config files. With no callback, this step is reported as
//!    `Skipped`.
//! 4. **Stripped state** — an intended runtime integration concern, not
//!    driven from this engine.
//!
//! ## Idempotency + journal
//!
//! After each step attempt, the engine records its `Wiped`, `AlreadyClean`,
//! `Skipped`, or `Failed` outcome in the on-disk journal. When an integration
//! explicitly calls
//! [`DuressEngine::resume_if_pending`], the engine reads the journal and
//! re-runs steps not yet completed. Startup resume remains a caller
//! responsibility.
//!
//! ## Wipe set status (v1 alpha)
//!
//! Implemented inside this engine when it is explicitly invoked:
//! - TPM key eviction (B1's `evict_tpm_key`).
//! - Keyring purge (B1's `KeyringSealer::purge_keyring_entry`).
//! - Identity-blob file deletion.
//! - Password-record file deletion.
//! - **Prekey-bundle file deletion** (B4: `prekeys.json` holds the
//!   sealed `PrekeyState`).
//! - In-memory zeroize (caller responsibility — the design's
//!   "Phase 2 step 9" is a process-exit / drop concern, not
//!   on-disk).
//! - Local-cache directory deletion (caller-supplied path).
//! - OPSEC-file deletion (caller-supplied paths — injection scripts,
//!   encryption config, etc.).
//!
//! Deferred — explicit non-stub callbacks reserved on
//! [`DuressHandlers`] so future layers (in-memory `PrekeyState`
//! wipe at the integration layer that owns it, future ratchet
//! registry, future sender-keys registry, v2.3+ anonymous-credential
//! tokens) wire in by setting one field. Each handler defaults to
//! `None`; when `None` the engine writes a `Skipped` entry in the
//! journal AND in [`DuressReport::skipped_steps`] with a reason
//! pointing at the integration layer responsible. **No
//! `unimplemented!()` / `todo!()` is used.**

use crate::sealer::{evict_tpm_key, KeyringSealer, SealerError, TpmEvictOutcome};
use crate::{Error as KeystoreError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// One wipe operation. Steps run in this enum's declaration order
/// (Phase 2 first, then Phase 3). Order matches the design doc's
/// "Local burn (synchronous, completes before strip)" list followed
/// by "Phase 3 — Strip OPSEC features".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum WipeStep {
    // ---- Phase 2: local burn ----
    /// Step 1 — `TPM2_EvictControl` on identity-key blobs.
    TpmEvict,
    /// Step 2 — delete keyring-fallback identity-data key.
    KeyringPurge,
    /// Step 3 — wipe encrypted local cache (caller-supplied dir).
    LocalCacheDir,
    /// Step 4 (v2.3+) — wipe anonymous-credential token store.
    AnonymousCredentials,
    /// Step 5a — delete the on-disk `prekeys.json` (sealed
    /// `PrekeyState`). Wired automatically when the caller supplies
    /// `DuressPaths::prekey_file`.
    PrekeyFile,
    /// Step 5b — wipe in-memory `PrekeyState`. Wired by the
    /// integration layer that owns the live state (the Tauri shell
    /// holds it in app state once layers 9–11 land).
    Prekeys,
    /// Step 6 — wipe Double Ratchet sessions + skipped-key cache.
    DoubleRatchet,
    /// Step 7 — wipe per-channel sender keys.
    SenderKeys,
    /// Step 8 — wipe per-peer ratchet states (subset of step 6 in
    /// some readings; kept distinct per design doc list).
    PeerRatchets,
    /// Step 9 — zeroize in-memory key material (caller drop handler;
    /// no-op here unless a handler is supplied).
    InMemoryZeroize,
    /// Step 10 — wipe stored unlock/duress password hashes.
    PasswordHashes,
    /// Identity blob file deletion (the on-disk identity.json that
    /// holds sealed keys). Listed under TPM eviction in the design
    /// but the file itself is separate from the TPM blob.
    IdentityFile,

    // ---- Phase 3: strip OPSEC features ----
    /// Delete injection-layer JS scripts + OPSEC config files.
    StripOpsecFiles,
}

impl WipeStep {
    /// All steps in canonical execution order.
    pub fn ordered() -> &'static [WipeStep] {
        &[
            WipeStep::TpmEvict,
            WipeStep::KeyringPurge,
            WipeStep::IdentityFile,
            WipeStep::PasswordHashes,
            WipeStep::PrekeyFile,
            WipeStep::LocalCacheDir,
            WipeStep::AnonymousCredentials,
            WipeStep::Prekeys,
            WipeStep::DoubleRatchet,
            WipeStep::SenderKeys,
            WipeStep::PeerRatchets,
            WipeStep::InMemoryZeroize,
            WipeStep::StripOpsecFiles,
        ]
    }
}

/// Per-step result: did the engine actually run a wipe (`Wiped`),
/// run an idempotent re-run that found nothing left to do
/// (`AlreadyClean`), or skip the step because no handler /
/// dependency was wired (`Skipped { reason }`)?
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepOutcome {
    Wiped,
    AlreadyClean,
    Skipped { reason: String },
    Failed { error: String },
}

/// Caller-supplied callback shape for each deferred wipe step.
/// `Send + Sync + 'static` so the engine can drive them from any
/// thread.
pub type WipeFn = Box<dyn Fn() -> std::result::Result<(), DuressError> + Send + Sync + 'static>;

/// Optional handlers and overrides for wipe steps. Deferred steps
/// become `Skipped` when their handler is unset; the keyring purge
/// override falls back to the production [`KeyringSealer`] purge.
#[derive(Default)]
pub struct DuressHandlers {
    /// Override for Step 2. When unset, the engine purges the
    /// production OS keyring entry through [`KeyringSealer`].
    pub purge_keyring: Option<WipeFn>,
    pub wipe_local_cache_dir: Option<WipeFn>,
    pub wipe_anonymous_credentials: Option<WipeFn>,
    pub wipe_prekeys: Option<WipeFn>,
    pub wipe_double_ratchet: Option<WipeFn>,
    pub wipe_sender_keys: Option<WipeFn>,
    pub wipe_peer_ratchets: Option<WipeFn>,
    pub zeroize_in_memory: Option<WipeFn>,
    pub strip_opsec_files: Option<WipeFn>,
}

/// On-disk paths the engine deletes directly.
pub struct DuressPaths {
    pub identity_file: PathBuf,
    pub password_file: PathBuf,
    /// Path to the sealed `prekeys.json` (B4). When `None` the
    /// engine reports the [`WipeStep::PrekeyFile`] step as
    /// `Skipped`; when `Some` it deletes the file idempotently.
    pub prekey_file: Option<PathBuf>,
}

/// Production duress-engine wiring that is independent of the UI or IPC
/// layer. `account_dir` is the active account directory; `password_dir` is the
/// device-level password directory.
pub struct ProductionDuressConfig {
    pub account_dir: PathBuf,
    pub password_dir: PathBuf,
    pub journal_path: PathBuf,
    pub local_cache_dir: PathBuf,
    pub anonymous_credentials_paths: Vec<PathBuf>,
    pub opsec_paths: Vec<PathBuf>,
    pub purge_keyring: Option<WipeFn>,
    pub wipe_prekeys: Option<WipeFn>,
    pub wipe_double_ratchet: Option<WipeFn>,
    pub wipe_sender_keys: Option<WipeFn>,
    pub wipe_peer_ratchets: Option<WipeFn>,
    pub zeroize_in_memory: Option<WipeFn>,
}

impl ProductionDuressConfig {
    pub fn new(account_dir: PathBuf, password_dir: PathBuf) -> Self {
        Self {
            journal_path: account_dir.join("duress.journal"),
            local_cache_dir: account_dir.join("store"),
            anonymous_credentials_paths: vec![
                account_dir.join("anonymous_credentials.json"),
                account_dir.join("anonymous_credentials"),
            ],
            opsec_paths: Vec::new(),
            account_dir,
            password_dir,
            purge_keyring: None,
            wipe_prekeys: None,
            wipe_double_ratchet: None,
            wipe_sender_keys: None,
            wipe_peer_ratchets: None,
            zeroize_in_memory: None,
        }
    }
}

pub struct ProductionDuressParts {
    pub journal_path: PathBuf,
    pub paths: DuressPaths,
    pub handlers: DuressHandlers,
}

pub fn build_production_duress_handlers(config: ProductionDuressConfig) -> ProductionDuressParts {
    let local_cache_dir = config.local_cache_dir;
    let anonymous_credentials_paths = config.anonymous_credentials_paths;
    let opsec_paths = config.opsec_paths;

    ProductionDuressParts {
        journal_path: config.journal_path,
        paths: DuressPaths {
            identity_file: config.account_dir.join("identity.json"),
            password_file: config.password_dir.join("password_marker.json"),
            prekey_file: Some(config.account_dir.join("prekeys.json")),
        },
        handlers: DuressHandlers {
            purge_keyring: config.purge_keyring,
            wipe_local_cache_dir: Some(Box::new(move || remove_path_idempotent(&local_cache_dir))),
            wipe_anonymous_credentials: Some(Box::new(move || {
                remove_paths_idempotent(&anonymous_credentials_paths)
            })),
            wipe_prekeys: config.wipe_prekeys,
            wipe_double_ratchet: config.wipe_double_ratchet,
            wipe_sender_keys: config.wipe_sender_keys,
            wipe_peer_ratchets: config.wipe_peer_ratchets,
            zeroize_in_memory: config.zeroize_in_memory,
            strip_opsec_files: Some(Box::new(move || remove_paths_idempotent(&opsec_paths))),
        },
    }
}

#[derive(Debug, Error)]
pub enum DuressError {
    #[error("io: {0}")]
    Io(String),

    #[error("sealer: {0}")]
    Sealer(String),

    #[error("journal: {0}")]
    Journal(String),

    #[error("handler: {0}")]
    Handler(String),
}

impl From<std::io::Error> for DuressError {
    fn from(e: std::io::Error) -> Self {
        DuressError::Io(e.to_string())
    }
}

impl From<DuressError> for KeystoreError {
    fn from(e: DuressError) -> Self {
        KeystoreError::Transport(format!("duress: {e}"))
    }
}

/// On-disk journal of attempted step outcomes. Read when an integration calls
/// [`DuressEngine::resume_if_pending`].
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DuressJournal {
    pub completed: Vec<(WipeStep, StepOutcome)>,
    pub started_at_unix_seconds: u64,
}

/// Top-level run report. Returned by [`DuressEngine::execute`] and
/// [`DuressEngine::resume_if_pending`]. The engine never panics — a
/// failed step yields a `Failed` outcome so the caller can decide
/// whether to abort the strip or push on (the design's stance: push
/// on regardless; partial wipe is better than none).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DuressReport {
    pub steps: Vec<(WipeStep, StepOutcome)>,
    pub completed: bool,
}

impl DuressReport {
    pub fn skipped_steps(&self) -> Vec<WipeStep> {
        self.steps
            .iter()
            .filter_map(|(s, o)| matches!(o, StepOutcome::Skipped { .. }).then_some(*s))
            .collect()
    }

    pub fn failed_steps(&self) -> Vec<(WipeStep, String)> {
        self.steps
            .iter()
            .filter_map(|(s, o)| match o {
                StepOutcome::Failed { error } => Some((*s, error.clone())),
                _ => None,
            })
            .collect()
    }
}

/// The engine itself. Cheap to construct; expensive when [`execute`]
/// drives the wipes.
pub struct DuressEngine {
    journal_path: PathBuf,
    paths: DuressPaths,
    handlers: DuressHandlers,
}

impl DuressEngine {
    pub fn new(journal_path: PathBuf, paths: DuressPaths, handlers: DuressHandlers) -> Self {
        DuressEngine {
            journal_path,
            paths,
            handlers,
        }
    }

    /// Run the full duress sequence. Phase 2 + 3 are synchronous and
    /// run in canonical order. Returns when the journal is removed
    /// (every step finished) or every remaining step has logged a
    /// failure outcome.
    pub fn execute(&self) -> Result<DuressReport> {
        self.execute_with_tpm_evict(evict_tpm_key)
    }

    fn execute_with_tpm_evict<F>(&self, tpm_evict: F) -> Result<DuressReport>
    where
        F: Fn() -> std::result::Result<TpmEvictOutcome, SealerError>,
    {
        let mut journal = self.read_or_init_journal()?;
        let already_done: std::collections::HashSet<WipeStep> =
            journal.completed.iter().map(|(s, _)| *s).collect();

        let mut report_steps = journal.completed.clone();
        for &step in WipeStep::ordered() {
            if already_done.contains(&step) {
                continue;
            }
            let outcome = self.run_step(step, &tpm_evict);
            report_steps.push((step, outcome.clone()));
            journal.completed.push((step, outcome));
            self.write_journal(&journal)?;
        }

        // Sweep: did all steps finish? If yes, remove the journal.
        let all_terminal = report_steps.iter().all(|(_, o)| {
            matches!(
                o,
                StepOutcome::Wiped
                    | StepOutcome::AlreadyClean
                    | StepOutcome::Skipped { .. }
                    | StepOutcome::Failed { .. },
            )
        });
        let any_failed = report_steps
            .iter()
            .any(|(_, o)| matches!(o, StepOutcome::Failed { .. }));
        if all_terminal && !any_failed {
            // Clean run — remove journal.
            self.remove_journal_if_present()?;
        }

        Ok(DuressReport {
            steps: report_steps,
            completed: all_terminal,
        })
    }

    /// If the journal exists at `journal_path`, resume the run.
    /// Returns `Ok(Some(report))` if a resume happened, `Ok(None)`
    /// if there was nothing to resume.
    pub fn resume_if_pending(&self) -> Result<Option<DuressReport>> {
        if !self.journal_path.exists() {
            return Ok(None);
        }
        Ok(Some(self.execute()?))
    }

    fn run_step<F>(&self, step: WipeStep, tpm_evict: &F) -> StepOutcome
    where
        F: Fn() -> std::result::Result<TpmEvictOutcome, SealerError>,
    {
        match step {
            WipeStep::TpmEvict => map_tpm_evict_result(tpm_evict()),
            WipeStep::KeyringPurge => self.run_keyring_purge(),
            WipeStep::IdentityFile => self.delete_file_idempotent(&self.paths.identity_file),
            WipeStep::PasswordHashes => self.delete_file_idempotent(&self.paths.password_file),
            WipeStep::PrekeyFile => match self.paths.prekey_file.as_deref() {
                Some(path) => self.delete_file_idempotent(path),
                None => StepOutcome::Skipped {
                    reason: "DuressPaths::prekey_file not supplied — \
                             caller must set the path to the sealed \
                             `prekeys.json` for B4's prekey-bundle blob \
                             to be deleted at duress time"
                        .to_string(),
                },
            },
            WipeStep::LocalCacheDir => self.run_handler(
                self.handlers.wipe_local_cache_dir.as_ref(),
                "local cache wipe handler not wired (caller passes \
                 dir path via DuressHandlers::wipe_local_cache_dir)",
            ),
            WipeStep::AnonymousCredentials => self.run_handler(
                self.handlers.wipe_anonymous_credentials.as_ref(),
                "anonymous credentials wipe deferred — feature lands in v2.3+",
            ),
            WipeStep::Prekeys => self.run_handler(
                self.handlers.wipe_prekeys.as_ref(),
                "in-memory PrekeyState wipe handler not wired — caller \
                 sets DuressHandlers::wipe_prekeys to drop the live \
                 state (the on-disk file is handled by the PrekeyFile \
                 step)",
            ),
            WipeStep::DoubleRatchet => self.run_handler(
                self.handlers.wipe_double_ratchet.as_ref(),
                "Double Ratchet wipe deferred — wired by future ratchet \
                 registry (per-peer state has no top-level home in v1 \
                 alpha)",
            ),
            WipeStep::SenderKeys => self.run_handler(
                self.handlers.wipe_sender_keys.as_ref(),
                "sender-keys wipe deferred — wired by future per-channel \
                 sender-keys registry",
            ),
            WipeStep::PeerRatchets => self.run_handler(
                self.handlers.wipe_peer_ratchets.as_ref(),
                "per-peer ratchet wipe deferred — same registry as \
                 DoubleRatchet step",
            ),
            WipeStep::InMemoryZeroize => self.run_handler(
                self.handlers.zeroize_in_memory.as_ref(),
                "in-memory zeroize handler not wired — caller's drop \
                 handlers are the canonical path; this is the documented \
                 last-resort hook",
            ),
            WipeStep::StripOpsecFiles => self.run_handler(
                self.handlers.strip_opsec_files.as_ref(),
                "OPSEC file strip not wired — caller passes injection \
                 script paths via DuressHandlers::strip_opsec_files",
            ),
        }
    }

    fn run_keyring_purge(&self) -> StepOutcome {
        match self.handlers.purge_keyring.as_ref() {
            Some(handler) => match handler() {
                Ok(_) => StepOutcome::Wiped,
                Err(e) => StepOutcome::Failed {
                    error: e.to_string(),
                },
            },
            None => KeyringSealer::purge_keyring_entry()
                .map(|_| StepOutcome::Wiped)
                .unwrap_or_else(|e| StepOutcome::Failed {
                    error: e.to_string(),
                }),
        }
    }

    fn run_handler(&self, handler: Option<&WipeFn>, skip_reason: &'static str) -> StepOutcome {
        match handler {
            Some(f) => match f() {
                Ok(_) => StepOutcome::Wiped,
                Err(e) => StepOutcome::Failed {
                    error: e.to_string(),
                },
            },
            None => StepOutcome::Skipped {
                reason: skip_reason.to_string(),
            },
        }
    }

    fn delete_file_idempotent(&self, path: &Path) -> StepOutcome {
        match std::fs::remove_file(path) {
            Ok(_) => StepOutcome::Wiped,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => StepOutcome::AlreadyClean,
            Err(e) => StepOutcome::Failed {
                error: format!("remove_file {}: {e}", path.display()),
            },
        }
    }

    fn read_or_init_journal(&self) -> std::result::Result<DuressJournal, DuressError> {
        if self.journal_path.exists() {
            let bytes = std::fs::read(&self.journal_path)?;
            serde_json::from_slice(&bytes).map_err(|e| DuressError::Journal(format!("parse: {e}")))
        } else {
            if let Some(parent) = self.journal_path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)?;
                }
            }
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let initial = DuressJournal {
                completed: Vec::new(),
                started_at_unix_seconds: now,
            };
            self.write_journal(&initial)?;
            Ok(initial)
        }
    }

    fn write_journal(&self, journal: &DuressJournal) -> std::result::Result<(), DuressError> {
        let json = serde_json::to_vec_pretty(journal)
            .map_err(|e| DuressError::Journal(format!("serialize: {e}")))?;
        std::fs::write(&self.journal_path, &json)?;
        Ok(())
    }

    fn remove_journal_if_present(&self) -> std::result::Result<(), DuressError> {
        match std::fs::remove_file(&self.journal_path) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(DuressError::Io(e.to_string())),
        }
    }
}

fn map_tpm_evict_result(result: std::result::Result<TpmEvictOutcome, SealerError>) -> StepOutcome {
    match result {
        Ok(TpmEvictOutcome::Evicted) => StepOutcome::Wiped,
        Ok(TpmEvictOutcome::NoTpmNothingToEvict) => StepOutcome::AlreadyClean,
        Err(e) => StepOutcome::Failed {
            error: e.to_string(),
        },
    }
}

fn remove_paths_idempotent(paths: &[PathBuf]) -> std::result::Result<(), DuressError> {
    for path in paths {
        remove_path_idempotent(path)?;
    }
    Ok(())
}

fn remove_path_idempotent(path: &Path) -> std::result::Result<(), DuressError> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(path).map_err(DuressError::from),
        Ok(_) => std::fs::remove_file(path).map_err(DuressError::from),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(DuressError::from(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_paths(dir: &TempDir) -> (DuressPaths, PathBuf) {
        (
            DuressPaths {
                identity_file: dir.path().join("identity.json"),
                password_file: dir.path().join("password.json"),
                prekey_file: Some(dir.path().join("prekeys.json")),
            },
            dir.path().join("duress.journal"),
        )
    }

    fn write_journal_with_all_steps_except(journal_path: &Path, except: WipeStep) {
        let completed = WipeStep::ordered()
            .iter()
            .copied()
            .filter(|step| *step != except)
            .map(|step| {
                (
                    step,
                    StepOutcome::Skipped {
                        reason: "prefilled test step".to_string(),
                    },
                )
            })
            .collect();
        let journal = DuressJournal {
            completed,
            started_at_unix_seconds: 0,
        };
        std::fs::write(journal_path, serde_json::to_vec_pretty(&journal).unwrap()).unwrap();
    }

    fn outcome_for(steps: &[(WipeStep, StepOutcome)], target: WipeStep) -> &StepOutcome {
        &steps
            .iter()
            .find(|(step, _)| *step == target)
            .unwrap_or_else(|| panic!("step {target:?} missing from report"))
            .1
    }

    #[cfg(not(windows))]
    #[test]
    fn duress_tpm_no_tpm_nothing_to_evict_completes_and_removes_journal() {
        let dir = TempDir::new().unwrap();
        let (paths, journal_path) = test_paths(&dir);
        write_journal_with_all_steps_except(&journal_path, WipeStep::TpmEvict);

        let engine = DuressEngine::new(journal_path.clone(), paths, DuressHandlers::default());
        let report = engine.execute().unwrap();

        assert!(report.completed);
        assert_eq!(
            outcome_for(&report.steps, WipeStep::TpmEvict),
            &StepOutcome::AlreadyClean
        );
        assert!(
            !journal_path.exists(),
            "NoTpmNothingToEvict is terminal and must remove the journal"
        );
    }

    #[test]
    fn duress_tpm_evict_error_retains_journal_and_reports_failed() {
        let dir = TempDir::new().unwrap();
        let (paths, journal_path) = test_paths(&dir);
        write_journal_with_all_steps_except(&journal_path, WipeStep::TpmEvict);

        let engine = DuressEngine::new(journal_path.clone(), paths, DuressHandlers::default());
        let report = engine
            .execute_with_tpm_evict(|| Err(SealerError::Tpm("DeleteKey: access denied".into())))
            .unwrap();

        assert!(report.completed);
        assert!(matches!(
            outcome_for(&report.steps, WipeStep::TpmEvict),
            StepOutcome::Failed { error } if error.contains("DeleteKey")
        ));
        assert!(
            journal_path.exists(),
            "failed TPM deletion must retain the journal for resume"
        );
    }

    #[test]
    fn already_clean_is_not_conflated_with_wiped_in_report() {
        let dir = TempDir::new().unwrap();
        let (paths, journal_path) = test_paths(&dir);
        write_journal_with_all_steps_except(&journal_path, WipeStep::TpmEvict);

        let engine = DuressEngine::new(journal_path, paths, DuressHandlers::default());
        let report = engine
            .execute_with_tpm_evict(|| Ok(TpmEvictOutcome::NoTpmNothingToEvict))
            .unwrap();
        let tpm_outcome = outcome_for(&report.steps, WipeStep::TpmEvict);

        assert_eq!(tpm_outcome, &StepOutcome::AlreadyClean);
        assert_ne!(tpm_outcome, &StepOutcome::Wiped);
    }

    #[test]
    fn build_production_duress_handlers() {
        let dir = TempDir::new().unwrap();
        let account_dir = dir.path().join("account");
        let password_dir = dir.path().join("base");
        std::fs::create_dir_all(account_dir.join("store")).unwrap();
        std::fs::create_dir_all(account_dir.join("anonymous_credentials")).unwrap();
        std::fs::write(account_dir.join("identity.json"), b"identity").unwrap();
        std::fs::write(account_dir.join("prekeys.json"), b"prekeys").unwrap();
        std::fs::write(
            account_dir.join("anonymous_credentials.json"),
            b"anonymous credentials",
        )
        .unwrap();
        std::fs::write(account_dir.join("store").join("messages.sqlite"), b"cache").unwrap();
        std::fs::create_dir_all(&password_dir).unwrap();
        std::fs::write(password_dir.join("password_marker.json"), b"marker").unwrap();
        let opsec_script = account_dir.join("boot.js");
        let opsec_config = account_dir.join("opsec.json");
        std::fs::write(&opsec_script, b"script").unwrap();
        std::fs::write(&opsec_config, b"config").unwrap();

        let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let callback = |name: &'static str| {
            let calls = calls.clone();
            Box::new(move || {
                calls.lock().unwrap().push(name);
                Ok(())
            }) as WipeFn
        };

        let mut config = ProductionDuressConfig::new(account_dir.clone(), password_dir.clone());
        config.purge_keyring = Some(callback("keyring"));
        config.wipe_prekeys = Some(callback("prekeys"));
        config.wipe_double_ratchet = Some(callback("double_ratchet"));
        config.wipe_sender_keys = Some(callback("sender_keys"));
        config.wipe_peer_ratchets = Some(callback("peer_ratchets"));
        config.zeroize_in_memory = Some(callback("zeroize"));
        config.opsec_paths = vec![opsec_script.clone(), opsec_config.clone()];

        let parts = super::build_production_duress_handlers(config);
        assert_eq!(parts.journal_path, account_dir.join("duress.journal"));
        assert_eq!(parts.paths.identity_file, account_dir.join("identity.json"));
        assert_eq!(
            parts.paths.password_file,
            password_dir.join("password_marker.json")
        );
        let expected_prekey_file = account_dir.join("prekeys.json");
        assert_eq!(
            parts.paths.prekey_file.as_deref(),
            Some(expected_prekey_file.as_path())
        );

        let engine = DuressEngine::new(parts.journal_path.clone(), parts.paths, parts.handlers);
        let report = engine
            .execute_with_tpm_evict(|| Ok(TpmEvictOutcome::NoTpmNothingToEvict))
            .unwrap();

        assert!(report.completed);
        assert!(report.failed_steps().is_empty());
        assert!(
            report.skipped_steps().is_empty(),
            "production assembly must not leave handler-backed steps skipped"
        );
        assert!(!account_dir.join("identity.json").exists());
        assert!(!account_dir.join("prekeys.json").exists());
        assert!(!account_dir.join("store").exists());
        assert!(!account_dir.join("anonymous_credentials").exists());
        assert!(!account_dir.join("anonymous_credentials.json").exists());
        assert!(!opsec_script.exists());
        assert!(!opsec_config.exists());
        assert!(!password_dir.join("password_marker.json").exists());
        assert!(!parts.journal_path.exists());
        assert_eq!(
            calls.lock().unwrap().as_slice(),
            [
                "keyring",
                "prekeys",
                "double_ratchet",
                "sender_keys",
                "peer_ratchets",
                "zeroize",
            ]
        );
    }
}
