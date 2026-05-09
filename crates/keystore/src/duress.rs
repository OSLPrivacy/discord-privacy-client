//! Duress flow execution.
//!
//! Spec: `docs/design/unlock-and-duress.md` "Duress flow — full
//! specification" + `docs/design/build-order.md` Layer B3.
//!
//! Four phases:
//!
//! 1. **Apparent unlock** — UI concern, not driven from this engine.
//!    The caller (Tauri shell) plays the normal unlock animation
//!    while the engine runs phases 2 + 3 concurrently in the
//!    background.
//! 2. **Local burn** — synchronous wipe of every key-bearing piece
//!    of state. Each step is idempotent so a crash mid-burn resumes
//!    cleanly on relaunch.
//! 3. **Strip OPSEC features** — delete injection scripts +
//!    encryption-module config so future launches fall through to a
//!    "stub" mode (plain Discord webview shell, no privacy
//!    features).
//! 4. **Stripped state** — runtime concern, not driven from this
//!    engine. The next process launch sees the absence of the
//!    OPSEC files and operates as a stub.
//!
//! ## Idempotency + journal
//!
//! Each wipe step writes its completion into the on-disk journal
//! AFTER it succeeds. On relaunch, [`DuressEngine::resume_if_pending`]
//! reads the journal and re-runs any steps not yet completed — every
//! step is idempotent so re-running a completed step is a no-op.
//! Once all steps complete the journal is removed and the engine
//! reports [`DuressOutcome::Completed`].
//!
//! ## Wipe set status (v1 alpha)
//!
//! Implemented today:
//! - TPM key eviction (B1's `evict_tpm_key`).
//! - Keyring purge (B1's `KeyringSealer::purge_keyring_entry`).
//! - Identity-blob file deletion.
//! - Password-record file deletion.
//! - In-memory zeroize (caller responsibility — the design's
//!   "Phase 2 step 9" is a process-exit / drop concern, not
//!   on-disk).
//! - Local-cache directory deletion (caller-supplied path).
//! - OPSEC-file deletion (caller-supplied paths — injection scripts,
//!   encryption config, etc.).
//!
//! Deferred — explicit non-stub callbacks reserved on
//! [`DuressHandlers`] so future layers (B4 prekeys, future ratchet
//! registry, future sender-keys registry, v2.3+ anonymous-credential
//! tokens) wire in by setting one field. Each handler defaults to
//! `None`; when `None` the engine writes a `Skipped` entry in the
//! journal AND in [`DuressReport::skipped_steps`]. **No
//! `unimplemented!()` / `todo!()` is used.**

use crate::sealer::{evict_tpm_key, KeyringSealer};
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
    /// Step 5 — wipe prekey state (own + cached). Lands in B4.
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

/// Optional handlers for wipe steps that aren't yet self-contained
/// in the keystore crate. Each `None` becomes a `Skipped` step at
/// run time with a reason string in the report — never an
/// `unimplemented!()`.
#[derive(Default)]
pub struct DuressHandlers {
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

/// On-disk journal: lists which steps have been completed so far.
/// Read on relaunch by [`DuressEngine::resume_if_pending`].
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
    pub fn new(
        journal_path: PathBuf,
        paths: DuressPaths,
        handlers: DuressHandlers,
    ) -> Self {
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
        let mut journal = self.read_or_init_journal()?;
        let already_done: std::collections::HashSet<WipeStep> =
            journal.completed.iter().map(|(s, _)| *s).collect();

        let mut report_steps = journal.completed.clone();
        for &step in WipeStep::ordered() {
            if already_done.contains(&step) {
                continue;
            }
            let outcome = self.run_step(step);
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

    fn run_step(&self, step: WipeStep) -> StepOutcome {
        match step {
            WipeStep::TpmEvict => evict_tpm_key()
                .map(|_| StepOutcome::Wiped)
                .unwrap_or_else(|e| StepOutcome::Failed {
                    error: e.to_string(),
                }),
            WipeStep::KeyringPurge => KeyringSealer::purge_keyring_entry()
                .map(|_| StepOutcome::Wiped)
                .unwrap_or_else(|e| StepOutcome::Failed {
                    error: e.to_string(),
                }),
            WipeStep::IdentityFile => self.delete_file_idempotent(&self.paths.identity_file),
            WipeStep::PasswordHashes => {
                self.delete_file_idempotent(&self.paths.password_file)
            }
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
                "prekey wipe deferred — handler wired by Layer B4 once \
                 prekey infrastructure exists",
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

    fn run_handler(
        &self,
        handler: Option<&WipeFn>,
        skip_reason: &'static str,
    ) -> StepOutcome {
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
            serde_json::from_slice(&bytes)
                .map_err(|e| DuressError::Journal(format!("parse: {e}")))
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
