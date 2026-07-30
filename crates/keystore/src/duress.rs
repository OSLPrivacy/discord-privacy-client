//! Legacy duress-engine primitives (implemented-unwired).
//!
//! Spec: `docs/design/unlock-and-duress.md` "Duress flow — full
//! specification" + `docs/design/build-order.md` Layer B3.
//!
//! Current Hub/IPC and legacy Tauri production sources neither construct a
//! [`DuressEngine`] nor call its execute/resume methods. The current Hub's
//! separately implemented burn-password path uses `startup_gate` and
//! `cleanup`, not this engine.
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
//! re-runs steps not yet completed. No production startup path currently
//! makes that call.
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
//! - Account unregister callback (caller-supplied; normally signs and sends
//!   the keyserver unregister request before local identity material is gone).
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
    /// Account-burn unregister. The engine cannot infer keyserver authority;
    /// production must supply an explicit signed unregister callback.
    UnregisterAccount,
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
            WipeStep::UnregisterAccount,
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
    pub unregister_account: Option<WipeFn>,
}

/// Production-owned adapter for wiring the duress engine's callback slots.
///
/// This type is intentionally additive over [`DuressHandlers`]: the engine's
/// low-level tests can continue constructing `DuressHandlers` directly, while
/// production callers get one named boundary for explicit wipe wiring. A missing
/// callback remains `None`, which the engine reports as `Skipped`; absence of a
/// production binding never grants permission to run a wipe by inference.
#[derive(Default)]
pub struct ProductionDuressHandlers {
    handlers: DuressHandlers,
    unregister_remote_identity: Option<WipeFn>,
}

impl ProductionDuressHandlers {
    /// Start with no production callbacks wired. Each missing handler is a
    /// refusal-by-omission and becomes a skipped step at execution time.
    pub fn new() -> Self {
        Self::default()
    }

    /// Wrap an already-assembled handler set. Intended for integration layers
    /// that own concrete wipe resources and can prove those resources are
    /// explicitly bound before invoking the duress engine.
    pub fn from_handlers(handlers: DuressHandlers) -> Self {
        Self {
            handlers,
            unregister_remote_identity: None,
        }
    }

    pub fn with_purge_keyring(mut self, handler: WipeFn) -> Self {
        self.handlers.purge_keyring = Some(handler);
        self
    }

    pub fn with_wipe_local_cache_dir(mut self, handler: WipeFn) -> Self {
        self.handlers.wipe_local_cache_dir = Some(handler);
        self
    }

    pub fn with_wipe_local_cache_dir_path<P>(mut self, path: P) -> Self
    where
        P: Into<PathBuf>,
    {
        self.handlers.wipe_local_cache_dir = Some(remove_bound_paths_handler(
            [path.into()],
            "local cache wipe requires an explicitly bound directory path",
        ));
        self
    }

    pub fn with_wipe_anonymous_credentials(mut self, handler: WipeFn) -> Self {
        self.handlers.wipe_anonymous_credentials = Some(handler);
        self
    }

    pub fn with_wipe_anonymous_credentials_paths<I, P>(mut self, paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        self.handlers.wipe_anonymous_credentials = Some(remove_bound_paths_handler(
            paths,
            "anonymous credentials wipe requires at least one explicitly bound path",
        ));
        self
    }

    pub fn with_wipe_prekeys(mut self, handler: WipeFn) -> Self {
        self.handlers.wipe_prekeys = Some(handler);
        self
    }

    pub fn with_wipe_double_ratchet(mut self, handler: WipeFn) -> Self {
        self.handlers.wipe_double_ratchet = Some(handler);
        self
    }

    pub fn with_wipe_sender_keys(mut self, handler: WipeFn) -> Self {
        self.handlers.wipe_sender_keys = Some(handler);
        self
    }

    pub fn with_wipe_peer_ratchets(mut self, handler: WipeFn) -> Self {
        self.handlers.wipe_peer_ratchets = Some(handler);
        self
    }

    pub fn with_zeroize_in_memory(mut self, handler: WipeFn) -> Self {
        self.handlers.zeroize_in_memory = Some(handler);
        self
    }

    pub fn with_strip_opsec_files(mut self, handler: WipeFn) -> Self {
        self.handlers.strip_opsec_files = Some(handler);
        self
    }

    pub fn with_strip_opsec_file_paths<I, P>(mut self, paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        self.handlers.strip_opsec_files = Some(remove_bound_paths_handler(
            paths,
            "OPSEC strip requires at least one explicitly bound path",
        ));
        self
    }

    /// Wire the production caller's keyserver unregister operation.
    ///
    /// This deliberately lives beside, not inside, [`DuressHandlers`]: the
    /// remote unregister must run while the old identity key still exists, so a
    /// caller performs it before handing the local wipe handlers to
    /// [`DuressEngine`]. If no callback is bound, production gets an explicit
    /// skipped outcome rather than permission inferred from absence.
    pub fn with_unregister_remote_identity(mut self, handler: WipeFn) -> Self {
        self.unregister_remote_identity = Some(handler);
        self
    }

    pub fn unregister_remote_identity(&self) -> StepOutcome {
        match self.unregister_remote_identity.as_ref() {
            Some(handler) => match handler() {
                Ok(()) => StepOutcome::Wiped,
                Err(error) => StepOutcome::Failed {
                    error: error.to_string(),
                },
            },
            None => StepOutcome::Skipped {
                reason: "remote identity unregister handler not wired — caller must unregister before local identity files are wiped".to_string(),
            },
        }
    }

    pub fn with_unregister_account(mut self, handler: WipeFn) -> Self {
        self.handlers.unregister_account = Some(handler);
        self
    }

    pub fn into_handlers(self) -> DuressHandlers {
        self.handlers
    }
}

impl From<ProductionDuressHandlers> for DuressHandlers {
    fn from(production: ProductionDuressHandlers) -> Self {
        production.into_handlers()
    }
}

/// Convert explicitly-bound production wipe callbacks into the engine's
/// complete handler table.
///
/// This function is the production assembly boundary: callers must supply each
/// callback they have authority to run through [`ProductionDuressHandlers`].
/// Any missing callback remains absent and is reported by [`DuressEngine`] as a
/// skipped step; absence is never treated as permission.
pub fn build_production_duress_handlers(production: ProductionDuressHandlers) -> DuressHandlers {
    production.into_handlers()
}

fn remove_bound_paths_handler<I, P>(paths: I, empty_binding_error: &'static str) -> WipeFn
where
    I: IntoIterator<Item = P>,
    P: Into<PathBuf>,
{
    let paths: Vec<PathBuf> = paths.into_iter().map(Into::into).collect();
    Box::new(move || remove_bound_paths(&paths, empty_binding_error))
}

fn remove_bound_paths(
    paths: &[PathBuf],
    empty_binding_error: &'static str,
) -> std::result::Result<(), DuressError> {
    if paths.is_empty() {
        return Err(DuressError::Handler(empty_binding_error.to_string()));
    }

    for (idx, path) in paths.iter().enumerate() {
        match std::fs::symlink_metadata(path) {
            Ok(meta) if meta.file_type().is_dir() && !meta.file_type().is_symlink() => {
                std::fs::remove_dir_all(path)
                    .map_err(|e| DuressError::Io(format!("remove bound directory #{idx}: {e}")))?;
            }
            Ok(_) => {
                std::fs::remove_file(path)
                    .map_err(|e| DuressError::Io(format!("remove bound file #{idx}: {e}")))?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(DuressError::Io(format!("inspect bound path #{idx}: {e}")));
            }
        }
    }

    Ok(())
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
            WipeStep::UnregisterAccount => self.run_handler(
                self.handlers.unregister_account.as_ref(),
                "account unregister not wired — caller must bind a signed \
                 keyserver unregister callback before local identity material \
                 is burned",
            ),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
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

    fn counted_handler(counter: Arc<AtomicUsize>) -> WipeFn {
        Box::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    }

    fn record_handler(calls: Arc<Mutex<Vec<&'static str>>>, label: &'static str) -> WipeFn {
        Box::new(move || {
            calls.lock().unwrap().push(label);
            Ok(())
        })
    }

    #[test]
    fn production_duress_handlers_compose_concrete_handlers() {
        let dir = TempDir::new().unwrap();
        let (paths, journal_path) = test_paths(&dir);
        std::fs::write(&paths.identity_file, b"identity").unwrap();
        std::fs::write(&paths.password_file, b"password").unwrap();
        if let Some(prekey_file) = paths.prekey_file.as_ref() {
            std::fs::write(prekey_file, b"prekeys").unwrap();
        }
        let anonymous_store = dir.path().join("anonymous_credentials.json");
        let strip_file = dir.path().join("opsec.js");
        std::fs::write(&anonymous_store, b"anonymous").unwrap();
        std::fs::write(&strip_file, b"opsec").unwrap();
        let remote_unregister = Arc::new(AtomicUsize::new(0));

        let calls = Arc::new(Mutex::new(Vec::new()));
        let production = ProductionDuressHandlers::new()
            .with_unregister_remote_identity(counted_handler(remote_unregister.clone()))
            .with_purge_keyring(record_handler(Arc::clone(&calls), "purge_keyring"))
            .with_wipe_local_cache_dir(record_handler(Arc::clone(&calls), "local_cache"))
            .with_wipe_anonymous_credentials_paths([anonymous_store.clone()])
            .with_wipe_prekeys(record_handler(Arc::clone(&calls), "prekeys"))
            .with_wipe_double_ratchet(record_handler(Arc::clone(&calls), "double_ratchet"))
            .with_wipe_sender_keys(record_handler(Arc::clone(&calls), "sender_keys"))
            .with_wipe_peer_ratchets(record_handler(Arc::clone(&calls), "peer_ratchets"))
            .with_zeroize_in_memory(record_handler(Arc::clone(&calls), "zeroize"))
            .with_strip_opsec_file_paths([strip_file.clone()])
            .with_unregister_account(record_handler(Arc::clone(&calls), "unregister"));

        assert_eq!(
            production.unregister_remote_identity(),
            StepOutcome::Wiped,
            "production remote unregister must be an explicit concrete callback"
        );
        assert_eq!(remote_unregister.load(Ordering::SeqCst), 1);

        let handlers = production.into_handlers();
        let engine = DuressEngine::new(journal_path, paths, handlers);

        let report = engine
            .execute_with_tpm_evict(|| Ok(TpmEvictOutcome::NoTpmNothingToEvict))
            .unwrap();

        for step in [
            WipeStep::KeyringPurge,
            WipeStep::IdentityFile,
            WipeStep::PasswordHashes,
            WipeStep::UnregisterAccount,
            WipeStep::PrekeyFile,
            WipeStep::LocalCacheDir,
            WipeStep::AnonymousCredentials,
            WipeStep::Prekeys,
            WipeStep::DoubleRatchet,
            WipeStep::SenderKeys,
            WipeStep::PeerRatchets,
            WipeStep::InMemoryZeroize,
            WipeStep::StripOpsecFiles,
        ] {
            assert_eq!(outcome_for(&report.steps, step), &StepOutcome::Wiped);
        }
        assert_eq!(
            calls.lock().unwrap().as_slice(),
            &[
                "purge_keyring",
                "unregister",
                "local_cache",
                "prekeys",
                "double_ratchet",
                "sender_keys",
                "peer_ratchets",
                "zeroize",
            ]
        );
        assert!(!anonymous_store.exists());
        assert!(!strip_file.exists());
    }

    #[test]
    fn build_production_duress_handlers() {
        let dir = TempDir::new().unwrap();
        let (paths, journal_path) = test_paths(&dir);
        std::fs::write(&paths.identity_file, b"identity").unwrap();
        std::fs::write(&paths.password_file, b"password").unwrap();
        if let Some(prekey_file) = paths.prekey_file.as_ref() {
            std::fs::write(prekey_file, b"prekeys").unwrap();
        }
        let calls = Arc::new(Mutex::new(Vec::new()));
        let handlers = super::build_production_duress_handlers(
            ProductionDuressHandlers::new()
                .with_purge_keyring(record_handler(Arc::clone(&calls), "purge_keyring"))
                .with_unregister_account(record_handler(Arc::clone(&calls), "unregister"))
                .with_wipe_local_cache_dir(record_handler(Arc::clone(&calls), "local_cache"))
                .with_wipe_anonymous_credentials(record_handler(
                    Arc::clone(&calls),
                    "anonymous_credentials",
                ))
                .with_wipe_prekeys(record_handler(Arc::clone(&calls), "prekeys"))
                .with_wipe_double_ratchet(record_handler(Arc::clone(&calls), "double_ratchet"))
                .with_wipe_sender_keys(record_handler(Arc::clone(&calls), "sender_keys"))
                .with_wipe_peer_ratchets(record_handler(Arc::clone(&calls), "peer_ratchets"))
                .with_zeroize_in_memory(record_handler(Arc::clone(&calls), "zeroize"))
                .with_strip_opsec_files(record_handler(Arc::clone(&calls), "strip_opsec")),
        );
        let engine = DuressEngine::new(journal_path, paths, handlers);

        let report = engine
            .execute_with_tpm_evict(|| Ok(TpmEvictOutcome::NoTpmNothingToEvict))
            .unwrap();

        assert!(report.completed);
        assert!(report.failed_steps().is_empty());
        assert!(report.skipped_steps().is_empty());
        assert_eq!(
            outcome_for(&report.steps, WipeStep::TpmEvict),
            &StepOutcome::AlreadyClean
        );
        for step in [
            WipeStep::KeyringPurge,
            WipeStep::IdentityFile,
            WipeStep::PasswordHashes,
            WipeStep::UnregisterAccount,
            WipeStep::PrekeyFile,
            WipeStep::LocalCacheDir,
            WipeStep::AnonymousCredentials,
            WipeStep::Prekeys,
            WipeStep::DoubleRatchet,
            WipeStep::SenderKeys,
            WipeStep::PeerRatchets,
            WipeStep::InMemoryZeroize,
            WipeStep::StripOpsecFiles,
        ] {
            assert_eq!(outcome_for(&report.steps, step), &StepOutcome::Wiped);
        }
        assert_eq!(
            calls.lock().unwrap().as_slice(),
            &[
                "purge_keyring",
                "unregister",
                "local_cache",
                "anonymous_credentials",
                "prekeys",
                "double_ratchet",
                "sender_keys",
                "peer_ratchets",
                "zeroize",
                "strip_opsec",
            ],
            "production handler assembly must preserve every concrete callback"
        );
    }

    #[test]
    fn production_duress_wipes_password_hashes_strips_opsec_and_unregisters() {
        let dir = TempDir::new().unwrap();
        let (paths, journal_path) = test_paths(&dir);
        let password_file = paths.password_file.clone();
        let strip_file = dir.path().join("injection.js");
        let strip_dir = dir.path().join("opsec");
        let anonymous_file = dir.path().join("anonymous-credentials.json");
        std::fs::write(&password_file, b"password hash record").unwrap();
        std::fs::write(&strip_file, b"boot script").unwrap();
        std::fs::create_dir(&strip_dir).unwrap();
        std::fs::write(strip_dir.join("config.json"), b"{}").unwrap();
        std::fs::write(&anonymous_file, b"anonymous credential").unwrap();
        let unregister_calls = Arc::new(Mutex::new(0usize));
        let unregister_calls_for_handler = Arc::clone(&unregister_calls);

        let handlers = ProductionDuressHandlers::new()
            .with_purge_keyring(Box::new(|| Ok(())))
            .with_wipe_anonymous_credentials_paths([anonymous_file.clone()])
            .with_strip_opsec_file_paths([strip_file.clone(), strip_dir.clone()])
            .with_unregister_account(Box::new(move || {
                *unregister_calls_for_handler.lock().unwrap() += 1;
                Ok(())
            }))
            .into_handlers();
        let engine = DuressEngine::new(journal_path, paths, handlers);

        let report = engine
            .execute_with_tpm_evict(|| Ok(TpmEvictOutcome::NoTpmNothingToEvict))
            .unwrap();

        assert_eq!(
            outcome_for(&report.steps, WipeStep::PasswordHashes),
            &StepOutcome::Wiped
        );
        assert_eq!(
            outcome_for(&report.steps, WipeStep::AnonymousCredentials),
            &StepOutcome::Wiped
        );
        assert_eq!(
            outcome_for(&report.steps, WipeStep::StripOpsecFiles),
            &StepOutcome::Wiped
        );
        assert_eq!(
            outcome_for(&report.steps, WipeStep::UnregisterAccount),
            &StepOutcome::Wiped
        );
        assert!(!password_file.exists());
        assert!(!anonymous_file.exists());
        assert!(!strip_file.exists());
        assert!(!strip_dir.exists());
        assert_eq!(*unregister_calls.lock().unwrap(), 1);
    }

    #[cfg(not(windows))]
    #[test]
    fn production_handlers_absence_keeps_remaining_callbacks_skipped() {
        let dir = TempDir::new().unwrap();
        let (paths, journal_path) = test_paths(&dir);
        let handlers = ProductionDuressHandlers::new().into_handlers();
        let engine = DuressEngine::new(journal_path, paths, handlers);

        let report = engine
            .execute_with_tpm_evict(|| Ok(TpmEvictOutcome::NoTpmNothingToEvict))
            .unwrap();

        for step in [
            WipeStep::UnregisterAccount,
            WipeStep::AnonymousCredentials,
            WipeStep::InMemoryZeroize,
            WipeStep::StripOpsecFiles,
        ] {
            assert!(
                matches!(
                    outcome_for(&report.steps, step),
                    StepOutcome::Skipped { .. }
                ),
                "unbound production callback {step:?} must fail closed as Skipped"
            );
        }
    }

    #[test]
    fn production_path_handlers_wipe_anonymous_credentials_and_strip_files() {
        let dir = TempDir::new().unwrap();
        let (paths, journal_path) = test_paths(&dir);
        let anonymous_store = dir.path().join("anonymous_credentials.json");
        let strip_file = dir.path().join("boot.js");
        let strip_dir = dir.path().join("opsec_config");
        std::fs::write(&anonymous_store, b"credential-token").unwrap();
        std::fs::write(&strip_file, b"injection").unwrap();
        std::fs::create_dir(&strip_dir).unwrap();
        std::fs::write(strip_dir.join("config.json"), b"{}").unwrap();

        let handlers = ProductionDuressHandlers::new()
            .with_wipe_anonymous_credentials_paths([anonymous_store.clone()])
            .with_strip_opsec_file_paths([strip_file.clone(), strip_dir.clone()])
            .into_handlers();
        let engine = DuressEngine::new(journal_path, paths, handlers);
        let report = engine
            .execute_with_tpm_evict(|| Ok(TpmEvictOutcome::NoTpmNothingToEvict))
            .unwrap();

        assert_eq!(
            outcome_for(&report.steps, WipeStep::AnonymousCredentials),
            &StepOutcome::Wiped
        );
        assert_eq!(
            outcome_for(&report.steps, WipeStep::StripOpsecFiles),
            &StepOutcome::Wiped
        );
        assert!(!anonymous_store.exists());
        assert!(!strip_file.exists());
        assert!(!strip_dir.exists());
    }

    #[test]
    fn production_path_handlers_with_empty_bindings_fail_closed() {
        let dir = TempDir::new().unwrap();
        let (paths, journal_path) = test_paths(&dir);
        let handlers = ProductionDuressHandlers::new()
            .with_wipe_anonymous_credentials_paths(Vec::<PathBuf>::new())
            .with_strip_opsec_file_paths(Vec::<PathBuf>::new())
            .into_handlers();
        let engine = DuressEngine::new(journal_path, paths, handlers);

        let report = engine
            .execute_with_tpm_evict(|| Ok(TpmEvictOutcome::NoTpmNothingToEvict))
            .unwrap();

        assert!(matches!(
            outcome_for(&report.steps, WipeStep::AnonymousCredentials),
            StepOutcome::Failed { error } if error.contains("explicitly bound path")
        ));
        assert!(matches!(
            outcome_for(&report.steps, WipeStep::StripOpsecFiles),
            StepOutcome::Failed { error } if error.contains("explicitly bound path")
        ));
    }

    #[test]
    fn production_password_hashes_wipe_is_bound_by_duress_paths() {
        let dir = TempDir::new().unwrap();
        let (paths, journal_path) = test_paths(&dir);
        std::fs::write(&paths.password_file, b"password-record").unwrap();
        let password_file = paths.password_file.clone();
        let handlers = ProductionDuressHandlers::new().into_handlers();
        let engine = DuressEngine::new(journal_path, paths, handlers);

        let report = engine
            .execute_with_tpm_evict(|| Ok(TpmEvictOutcome::NoTpmNothingToEvict))
            .unwrap();

        assert_eq!(
            outcome_for(&report.steps, WipeStep::PasswordHashes),
            &StepOutcome::Wiped
        );
        assert!(!password_file.exists());
    }

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
}
