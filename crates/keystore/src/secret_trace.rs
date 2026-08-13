//! TASK 5402 — the runtime writer/read trace for secret material.
//!
//! The finish line for 5402 needs two independent views of where secrets live:
//! this one, recorded from inside the shipping writers and readers as they run,
//! and a filesystem/registry/backup/export inventory that walks the disk
//! afterwards holding no knowledge of the code. They then have to agree. A
//! surface the inventory finds but the trace never wrote is an unattributed
//! secret; a path the trace wrote but the inventory never listed is a surface
//! the audit is blind to. Either one is a defect, and only two views can tell
//! them apart.
//!
//! The recorder is compiled into the shipping build on purpose — a trace that
//! only exists under `cfg(test)` proves what the test build does, not what
//! ships. It is inert until [`arm`] is called: every `record` call behind a
//! disarmed flag is one relaxed atomic load, so the production cost is a
//! predictable branch on a never-written cache line.
//!
//! Nothing here records secret VALUES. An entry is the class, the protection
//! the writer applied, the writer's own name, the exact path, and a length.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// The secret classes TASK 5402 enumerates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SecretClass {
    /// An account password or unlock verifier.
    UnlockVerifier,
    /// A recovery phrase — the password-gate phrase or an identity phrase.
    RecoveryPhrase,
    /// A recovery authority: the private seed a recovery kit signs with.
    RecoveryAuthority,
    /// Imported or restored identity private material.
    IdentityPrivateMaterial,
    /// A recovery package or kit: a transfer bundle, export archive, or key
    /// file that travels off the device.
    RecoveryPackage,
    /// A cached provider credential or token.
    CachedProviderCredential,
    /// A data key that sits beside the data it opens.
    AdjacentDataKey,
}

impl SecretClass {
    pub const ALL: [Self; 7] = [
        Self::UnlockVerifier,
        Self::RecoveryPhrase,
        Self::RecoveryAuthority,
        Self::IdentityPrivateMaterial,
        Self::RecoveryPackage,
        Self::CachedProviderCredential,
        Self::AdjacentDataKey,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnlockVerifier => "unlock_verifier",
            Self::RecoveryPhrase => "recovery_phrase",
            Self::RecoveryAuthority => "recovery_authority",
            Self::IdentityPrivateMaterial => "identity_private_material",
            Self::RecoveryPackage => "recovery_package",
            Self::CachedProviderCredential => "cached_provider_credential",
            Self::AdjacentDataKey => "adjacent_data_key",
        }
    }
}

impl std::fmt::Display for SecretClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SecretOp {
    Write,
    Read,
}

impl SecretOp {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Write => "write",
            Self::Read => "read",
        }
    }
}

/// What the writer actually applied to the bytes it put on disk.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Protection {
    /// A salted, memory-hard verifier. Never reversible to the password.
    SaltedMemoryHardVerifier,
    /// Authenticated ciphertext under a key derived from user input.
    UserDerivedAead,
    /// Authenticated ciphertext under the platform sealer (TPM, OS credential
    /// store, or the encrypted process-ephemeral fallback).
    DeviceSealedAead,
    /// Recorded only by a deliberate starvation of the bar. A shipping writer
    /// that reports this is the defect 5402 exists to catch.
    Plaintext,
}

impl Protection {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SaltedMemoryHardVerifier => "salted_memory_hard_verifier",
            Self::UserDerivedAead => "user_derived_aead",
            Self::DeviceSealedAead => "device_sealed_aead",
            Self::Plaintext => "plaintext",
        }
    }

    pub const fn is_protected(self) -> bool {
        !matches!(self, Self::Plaintext)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecretAccess {
    pub op: SecretOp,
    pub class: SecretClass,
    pub protection: Protection,
    /// The shipping function that performed the access.
    pub writer: &'static str,
    pub path: PathBuf,
    pub bytes: usize,
}

impl SecretAccess {
    /// One stable line per access, for the evidence transcript.
    pub fn line(&self) -> String {
        format!(
            "op={} class={} protection={} writer={} bytes={} path={}",
            self.op.as_str(),
            self.class.as_str(),
            self.protection.as_str(),
            self.writer,
            self.bytes,
            self.path.display()
        )
    }
}

static ARMED: AtomicBool = AtomicBool::new(false);
static LOG: Mutex<Vec<SecretAccess>> = Mutex::new(Vec::new());

/// Start recording. Clears anything a previous run left behind.
pub fn arm() {
    if let Ok(mut log) = LOG.lock() {
        log.clear();
    }
    ARMED.store(true, Ordering::SeqCst);
}

pub fn is_armed() -> bool {
    ARMED.load(Ordering::Relaxed)
}

/// Stop recording and take everything observed.
pub fn disarm_and_take() -> Vec<SecretAccess> {
    ARMED.store(false, Ordering::SeqCst);
    LOG.lock()
        .map(|mut log| std::mem::take(&mut *log))
        .unwrap_or_default()
}

/// Everything observed so far, without stopping.
pub fn snapshot() -> Vec<SecretAccess> {
    LOG.lock().map(|log| log.clone()).unwrap_or_default()
}

/// Record one access. Inert (a single relaxed load) when disarmed.
pub fn record(
    op: SecretOp,
    class: SecretClass,
    protection: Protection,
    writer: &'static str,
    path: &Path,
    bytes: usize,
) {
    if !ARMED.load(Ordering::Relaxed) {
        return;
    }
    if let Ok(mut log) = LOG.lock() {
        log.push(SecretAccess {
            op,
            class,
            protection,
            writer,
            path: path.to_path_buf(),
            bytes,
        });
    }
}

/// Record a write plus the recoverable-write companions the same primitive
/// creates (`<stem>.tmp` and `<stem>.bak`).
///
/// The companions are not an implementation detail the audit may skip: the
/// `.bak` file is a full previous copy of the record and outlives the write on
/// every path that had a previous version, so it is exactly as much of an
/// at-rest surface as the file it backs up.
pub fn record_recoverable_write(
    class: SecretClass,
    protection: Protection,
    writer: &'static str,
    path: &Path,
    bytes: usize,
) {
    if !ARMED.load(Ordering::Relaxed) {
        return;
    }
    record(SecretOp::Write, class, protection, writer, path, bytes);
    for extension in ["tmp", "bak"] {
        let companion = path.with_extension(extension);
        record(
            SecretOp::Write,
            class,
            protection,
            writer,
            &companion,
            bytes,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serialises this module's own tests against each other. `arm`/`record`
    /// operate on process-global state, so two of these running concurrently
    /// would read each other's entries.
    static TRACE_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn disarmed_recording_is_inert_and_arming_captures_companions() {
        let _guard = TRACE_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _ = disarm_and_take();
        record(
            SecretOp::Write,
            SecretClass::UnlockVerifier,
            Protection::SaltedMemoryHardVerifier,
            "test",
            Path::new("/tmp/ignored.json"),
            1,
        );
        assert!(snapshot().is_empty(), "a disarmed trace records nothing");

        arm();
        record_recoverable_write(
            SecretClass::IdentityPrivateMaterial,
            Protection::DeviceSealedAead,
            "test::save",
            Path::new("/tmp/osl/identity.json"),
            42,
        );
        let observed = disarm_and_take();
        let paths: Vec<String> = observed
            .iter()
            .map(|entry| entry.path.display().to_string())
            .collect();
        assert_eq!(
            paths,
            vec![
                "/tmp/osl/identity.json".to_string(),
                "/tmp/osl/identity.tmp".to_string(),
                "/tmp/osl/identity.bak".to_string(),
            ]
        );
        assert!(observed.iter().all(|entry| entry.protection.is_protected()));
    }
}
