//! Unlock + duress password handling.
//!
//! Spec: `docs/design/unlock-and-duress.md`. Two passwords, both
//! optional:
//!
//! - **Unlock password**: gates app access.
//! - **Duress password**: appears to unlock normally, then triggers
//!   the duress flow (B3) which silently burns and strips the app.
//!
//! Both stored as Argon2id hashes alongside the identity blob and
//! sealed under the same [`crate::sealer::Sealer`]. Failed-attempt
//! tracking, threshold, and inactivity-timer settings live in the
//! same record.
//!
//! ## Cryptographic role (per design doc)
//!
//! Password is a **UX gate**, not part of identity-key derivation.
//! TPM-sealed identity unseals regardless of password input — the
//! password just decides which flow runs (normal unlock vs duress
//! vs failed-with-retry vs failed-threshold-exceeded).
//!
//! ## Argon2id parameters
//!
//! Default ([`Argon2Params::production`]):
//! - `m_cost` (memory): 65 536 KiB = 64 MiB. **Floor — do not
//!   lower** (this is the GPU-resistance property).
//! - `t_cost` (iterations): 3. Tune to ~250 ms target on
//!   representative hardware (i5-class 2020 laptop).
//! - `p_cost` (parallelism): 1.
//! - Output: 32 bytes.
//!
//! Tests ([`Argon2Params::fast_for_tests`]):
//! - `m_cost`: 8 KiB, `t_cost`: 1, `p_cost`: 1, output: 32. Hashes
//!   in milliseconds.
//!
//! Tests MUST NOT use production params — running 64 MiB Argon2id
//! per test makes the suite take minutes. Production code MUST NOT
//! use the test params — they have no GPU-resistance.

use crate::Result;
use argon2::{Algorithm, Argon2, Params, Version};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use zeroize::Zeroize;

#[derive(Debug, Error)]
pub enum PasswordError {
    #[error("password too short: {got} chars, minimum {min}")]
    TooShort { got: usize, min: usize },

    #[error("unlock and duress passwords must not be identical")]
    PasswordsIdentical,

    #[error("Argon2id error: {0}")]
    Argon2(String),
}

/// Minimum password length per design doc — 6 digits / chars.
pub const MIN_PASSWORD_LENGTH: usize = 6;

/// Default failed-attempt threshold. Configurable; per design doc.
pub const DEFAULT_FAILED_ATTEMPT_THRESHOLD: u32 = 10;

/// Default inactivity re-prompt window. Configurable; per design doc
/// (1 min – 60 min range, default 15 min).
pub const DEFAULT_INACTIVITY_SECONDS: u64 = 15 * 60;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Argon2Params {
    pub m_cost: u32,
    pub t_cost: u32,
    pub p_cost: u32,
    pub output_len: u32,
}

impl Argon2Params {
    /// Production parameters per design doc.
    pub fn production() -> Self {
        Argon2Params {
            m_cost: 65_536,
            t_cost: 3,
            p_cost: 1,
            output_len: 32,
        }
    }

    /// Test parameters — fast, **NOT secure**. Used by unit tests.
    pub fn fast_for_tests() -> Self {
        Argon2Params {
            m_cost: 8,
            t_cost: 1,
            p_cost: 1,
            output_len: 32,
        }
    }

    fn to_argon_params(self) -> std::result::Result<Params, PasswordError> {
        Params::new(
            self.m_cost,
            self.t_cost,
            self.p_cost,
            Some(self.output_len as usize),
        )
        .map_err(|e| PasswordError::Argon2(format!("Params::new: {e}")))
    }
}

/// One Argon2id hash + its salt + the parameters used.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PasswordHash {
    pub salt: [u8; 16],
    pub hash: [u8; 32],
    pub params: Argon2Params,
}

impl PasswordHash {
    /// Hash `plaintext` with `params` and a fresh random 16-byte salt.
    pub fn create(
        plaintext: &str,
        params: Argon2Params,
    ) -> std::result::Result<Self, PasswordError> {
        let mut salt = [0u8; 16];
        let bytes = crypto::random::random_bytes(16);
        salt.copy_from_slice(&bytes);
        let mut hash = [0u8; 32];
        hash_into(plaintext.as_bytes(), &salt, params, &mut hash)?;
        Ok(PasswordHash { salt, hash, params })
    }

    /// Verify `plaintext` against this hash. Constant-time comparison
    /// (Argon2id's internal compare is CT; we only re-derive the
    /// expected hash and compare via `subtle`).
    pub fn verify(&self, plaintext: &str) -> std::result::Result<bool, PasswordError> {
        let mut candidate = [0u8; 32];
        hash_into(
            plaintext.as_bytes(),
            &self.salt,
            self.params,
            &mut candidate,
        )?;
        let eq = subtle_eq(&candidate, &self.hash);
        candidate.zeroize();
        Ok(eq)
    }
}

fn hash_into(
    password: &[u8],
    salt: &[u8],
    params: Argon2Params,
    out: &mut [u8; 32],
) -> std::result::Result<(), PasswordError> {
    let argon = Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        params.to_argon_params()?,
    );
    argon
        .hash_password_into(password, salt, out)
        .map_err(|e| PasswordError::Argon2(format!("hash_password_into: {e}")))
}

fn subtle_eq(a: &[u8; 32], b: &[u8; 32]) -> bool {
    use subtle::ConstantTimeEq;
    bool::from(a.ct_eq(b))
}

// Re-export `subtle` from the workspace via the crypto crate's
// dependency tree. We use `subtle` directly here for the CT compare.
use subtle as _;

/// Validate password against the policy: length >= MIN_PASSWORD_LENGTH.
/// (The design also accepts "longer alphanumeric passphrases at user
/// choice"; min length is the floor in either case.)
pub fn validate_password(plaintext: &str) -> std::result::Result<(), PasswordError> {
    if plaintext.chars().count() < MIN_PASSWORD_LENGTH {
        return Err(PasswordError::TooShort {
            got: plaintext.chars().count(),
            min: MIN_PASSWORD_LENGTH,
        });
    }
    Ok(())
}

/// Validate that `unlock` and `duress` (if present) both pass the
/// policy AND differ from each other.
pub fn validate_setup_pair(
    unlock: &str,
    duress: Option<&str>,
) -> std::result::Result<(), PasswordError> {
    validate_password(unlock)?;
    if let Some(d) = duress {
        validate_password(d)?;
        if d == unlock {
            return Err(PasswordError::PasswordsIdentical);
        }
    }
    Ok(())
}

/// Persisted record for unlock + (optional) duress passwords plus
/// failed-attempt counters and inactivity-timer config. Sealed via
/// the active [`crate::sealer::Sealer`] (same as identity).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PasswordRecord {
    pub unlock_hash: PasswordHash,
    pub duress_hash: Option<PasswordHash>,
    /// Cumulative failed-attempt counter. Reset on successful unlock.
    pub failed_attempts: u32,
    /// Threshold above which the next failed attempt auto-triggers
    /// the duress flow. Configurable; design default 10.
    pub failed_attempt_threshold: u32,
    /// Inactivity re-prompt window in seconds. Configurable; design
    /// default 900 (15 min).
    pub inactivity_seconds: u64,
}

impl PasswordRecord {
    pub fn new(
        unlock: &str,
        duress: Option<&str>,
        params: Argon2Params,
    ) -> std::result::Result<Self, PasswordError> {
        validate_setup_pair(unlock, duress)?;
        let unlock_hash = PasswordHash::create(unlock, params)?;
        let duress_hash = match duress {
            Some(d) => Some(PasswordHash::create(d, params)?),
            None => None,
        };
        Ok(PasswordRecord {
            unlock_hash,
            duress_hash,
            failed_attempts: 0,
            failed_attempt_threshold: DEFAULT_FAILED_ATTEMPT_THRESHOLD,
            inactivity_seconds: DEFAULT_INACTIVITY_SECONDS,
        })
    }
}

/// What happened when a user-typed password was checked against the
/// record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerifyOutcome {
    /// Unlock password matched. Caller resets `failed_attempts`.
    Unlock,
    /// Duress password matched. Caller triggers duress (B3) silently.
    Duress,
    /// Neither matched, but the failed-attempt threshold has not yet
    /// been exceeded. Caller increments and persists the counter.
    Wrong { attempts: u32 },
    /// Neither matched AND the threshold is now reached. Caller
    /// triggers duress (B3) silently — same effect as Duress, just a
    /// different cause.
    DuressByThreshold,
}

/// Verify `plaintext` against `record`. Mutates `record.failed_attempts`
/// on the wrong / threshold paths so the caller can persist it. The
/// caller is responsible for actually persisting the updated record
/// + invoking duress on the appropriate outcomes.
pub fn verify_against_record(
    record: &mut PasswordRecord,
    plaintext: &str,
) -> std::result::Result<VerifyOutcome, PasswordError> {
    // Always check both hashes (constant-ish-time across paths;
    // matches the design's "duress unlocks normally" UX — a watcher
    // shouldn't be able to time-distinguish unlock vs duress).
    let unlock_match = record.unlock_hash.verify(plaintext)?;
    let duress_match = match &record.duress_hash {
        Some(h) => h.verify(plaintext)?,
        None => false,
    };
    if unlock_match {
        record.failed_attempts = 0;
        return Ok(VerifyOutcome::Unlock);
    }
    if duress_match {
        // Duress branches deliberately do NOT reset the counter —
        // duress is irreversible; counter state is moot.
        return Ok(VerifyOutcome::Duress);
    }
    record.failed_attempts = record.failed_attempts.saturating_add(1);
    if record.failed_attempts >= record.failed_attempt_threshold {
        Ok(VerifyOutcome::DuressByThreshold)
    } else {
        Ok(VerifyOutcome::Wrong {
            attempts: record.failed_attempts,
        })
    }
}

/// In-memory inactivity timer. Caller calls
/// [`Self::mark_activity`] on every OS input event (or a coarser
/// signal — focus changes, mouse moves) and
/// [`Self::should_reprompt`] on each tick / ipc command.
///
/// Source per design doc: **OS idle (last input device event)**, not
/// app-focus idle — the user might be working in another window with
/// the app in the background.
pub struct InactivityTimer {
    threshold: std::time::Duration,
    last_activity: std::time::Instant,
}

impl InactivityTimer {
    pub fn from_seconds(seconds: u64) -> Self {
        InactivityTimer {
            threshold: std::time::Duration::from_secs(seconds),
            last_activity: std::time::Instant::now(),
        }
    }

    /// Test/diagnostic constructor: lets the caller seed the
    /// last-activity instant explicitly.
    pub fn with_last_activity(seconds: u64, last_activity: std::time::Instant) -> Self {
        InactivityTimer {
            threshold: std::time::Duration::from_secs(seconds),
            last_activity,
        }
    }

    pub fn mark_activity_at(&mut self, now: std::time::Instant) {
        self.last_activity = now;
    }

    pub fn mark_activity(&mut self) {
        self.last_activity = std::time::Instant::now();
    }

    pub fn should_reprompt_at(&self, now: std::time::Instant) -> bool {
        now.duration_since(self.last_activity) >= self.threshold
    }

    pub fn should_reprompt(&self) -> bool {
        self.should_reprompt_at(std::time::Instant::now())
    }
}

// ---- storage hookup ----

/// Save `record` to `path` sealed under `sealer`. Mirrors the
/// identity on-disk format (versioned outer JSON + sealed inner
/// payload) but stores a separate file alongside `identity.json`.
pub fn save_password_record(
    path: &std::path::Path,
    record: &PasswordRecord,
    sealer: &dyn crate::sealer::Sealer,
) -> Result<()> {
    let inner = serde_json::to_vec(record)?;
    let sealed = sealer.seal(&inner)?;
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    let on_disk = PasswordRecordOnDisk {
        version: 1,
        method: sealer.method_label().to_string(),
        sealed_b64: STANDARD.encode(&sealed),
        insecure_banner: if sealer.requires_insecure_banner() {
            Some(
                "INSECURE prototype storage — plain JSON, no TPM. \
                 v1 stable replaces with TPM-sealed blob; do NOT use \
                 with real users."
                    .to_string(),
            )
        } else {
            None
        },
    };
    let json = serde_json::to_vec_pretty(&on_disk)?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, &json)?;
    Ok(())
}

/// Load a [`PasswordRecord`] from `path` using `sealer`.
pub fn load_password_record(
    path: &std::path::Path,
    sealer: &dyn crate::sealer::Sealer,
) -> Result<PasswordRecord> {
    let bytes = std::fs::read(path)?;
    let on_disk: PasswordRecordOnDisk = serde_json::from_slice(&bytes)?;
    if on_disk.version != 1 {
        return Err(crate::Error::BlobVersionMismatch {
            got: on_disk.version,
            expected: 1,
        });
    }
    if on_disk.method != sealer.method_label() {
        return Err(crate::Error::BlobMethodMismatch {
            got: on_disk.method,
            expected: sealer.method_label().to_string(),
        });
    }
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    let sealed = STANDARD.decode(&on_disk.sealed_b64)?;
    let inner = sealer.unseal(&sealed)?;
    let record: PasswordRecord = serde_json::from_slice(&inner)?;
    Ok(record)
}

#[derive(Serialize, Deserialize)]
struct PasswordRecordOnDisk {
    version: u32,
    method: String,
    sealed_b64: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    insecure_banner: Option<String>,
}
