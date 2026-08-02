//! Durable, sealed per-peer OSL-RN session health.
//!
//! This state deliberately lives beside, not inside, the ratchet session
//! blob. A recovery deletes the session in order to re-handshake, while a
//! `Desynced` diagnosis must survive that deletion and a subsequent restart.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::wire_rn::{RnError, RN_SESSION_DIR};

/// Return the stable reset-ledger key for a complete, newly observed TOFU
/// bundle.  This is deliberately distinct from `TofuOutcome`: an unaccepted
/// `Changed` outcome can recur for every send, while its bundle digest must
/// cause only one session reset.
pub fn tofu_bundle_digest(bundle: &crate::tofu::KeyBundle) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"OSL-RN/v1/tofu-reset-bundle/");
    for component in [
        Some(bundle.ed25519_pub.as_bytes()),
        Some(bundle.x25519_pub.as_bytes()),
        Some(bundle.mlkem768_pub.as_bytes()),
        bundle.ratchet_initial_pub.as_deref().map(str::as_bytes),
    ] {
        match component {
            Some(bytes) => {
                hasher.update([1]);
                hasher.update((bytes.len() as u64).to_be_bytes());
                hasher.update(bytes);
            }
            None => hasher.update([0]),
        }
    }
    hasher.finalize().into()
}

/// The pending key-change alert is the reset-once ledger.  It stores the
/// digest that has already invalidated the old session; repeated observations
/// of that digest must leave a freshly bootstrapped session intact.
pub fn tofu_change_needs_session_reset(
    pending: Option<&crate::tofu::KeyBundle>,
    fetched: &crate::tofu::KeyBundle,
) -> bool {
    pending.is_none_or(|bundle| tofu_bundle_digest(bundle) != tofu_bundle_digest(fetched))
}

const HEALTH_BLOB_VERSION: u32 = 1;
const MAX_HEALTH_FILE_BYTES: u64 = 16 * 1024;
static NEXT_TEMP_SUFFIX: AtomicU64 = AtomicU64::new(0);

/// The user-visible health of one peer's OSL-RN session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RnSessionHealth {
    Healthy,
    Degraded,
    Desynced,
    Unrecoverable,
}

/// A receive-side symptom that can prove an RN-pinned session has diverged.
///
/// Callers may report these only after the peer's RN pin has been verified.
/// An authentication failure needs a small threshold because one tampered
/// packet is not proof of a lost session; the other two symptoms are local,
/// unambiguous contradictions of the pinned session state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RnDesyncSymptom {
    AuthFailed,
    MissingSession,
    MaxSkipPerMessageRefused,
}

/// Durable health record for one peer.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RnPeerHealth {
    health: RnSessionHealth,
    consecutive_auth_failures: u8,
}

impl Default for RnPeerHealth {
    fn default() -> Self {
        Self {
            health: RnSessionHealth::Healthy,
            consecutive_auth_failures: 0,
        }
    }
}

impl RnPeerHealth {
    pub fn health(&self) -> RnSessionHealth {
        self.health
    }

    pub fn consecutive_auth_failures(&self) -> u8 {
        self.consecutive_auth_failures
    }

    /// A successful authenticated decrypt is the sole way to clear a
    /// desynchronisation diagnosis.
    pub fn successful_decrypt(&mut self) {
        if self.health == RnSessionHealth::Unrecoverable {
            return;
        }
        self.health = RnSessionHealth::Healthy;
        self.consecutive_auth_failures = 0;
    }

    /// Record a real receive-side symptom from an RN-pinned peer.
    ///
    /// The detector's authentication-failure threshold is frozen at three.
    /// A missing session or a per-message skip-bound refusal are immediate
    /// proofs that the pinned session cannot process this wire message.
    /// Once desynchronised, later symptoms retain that state; they cannot make
    /// the UI look healthy again.
    pub fn observe_pinned_symptom(&mut self, symptom: RnDesyncSymptom) {
        if matches!(
            self.health,
            RnSessionHealth::Unrecoverable | RnSessionHealth::Desynced
        ) {
            return;
        }
        match symptom {
            RnDesyncSymptom::AuthFailed => {
                self.consecutive_auth_failures = self.consecutive_auth_failures.saturating_add(1);
                self.health = if self.consecutive_auth_failures >= 3 {
                    RnSessionHealth::Desynced
                } else {
                    RnSessionHealth::Degraded
                };
            }
            RnDesyncSymptom::MissingSession | RnDesyncSymptom::MaxSkipPerMessageRefused => {
                self.health = RnSessionHealth::Desynced;
            }
        }
    }

    /// A non-retryable local state/sealer failure must remain visible until
    /// its named remediation; it may never silently become healthy.
    pub fn unrecoverable(&mut self) {
        self.health = RnSessionHealth::Unrecoverable;
    }

    /// Complete the explicit remediation for a non-retryable local failure.
    /// This is intentionally separate from message processing so an ordinary
    /// inbound packet can never clear the user-visible failure.
    pub fn remediation_completed(&mut self) {
        self.health = RnSessionHealth::Healthy;
        self.consecutive_auth_failures = 0;
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SealedHealthBlob {
    version: u32,
    method: String,
    sealed_b64: String,
}

/// Sealed per-peer health records stored in the canonical RN directory.
#[derive(Clone)]
pub struct RnHealthStore {
    dir: PathBuf,
}

impl RnHealthStore {
    pub fn for_config_dir(config_dir: impl AsRef<Path>) -> Self {
        Self::new(config_dir.as_ref().join(RN_SESSION_DIR))
    }

    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn load(&self, peer: &[u8; 32]) -> Result<RnPeerHealth, RnError> {
        let sealer = keystore::select_best_sealer();
        self.load_with_sealer(peer, sealer.as_ref())
    }

    pub fn load_with_sealer(
        &self,
        peer: &[u8; 32],
        sealer: &dyn keystore::sealer::Sealer,
    ) -> Result<RnPeerHealth, RnError> {
        let path = self.health_path(peer);
        let bytes = match read_bounded(&path)? {
            Some(bytes) => bytes,
            None => return Ok(RnPeerHealth::default()),
        };
        let blob: SealedHealthBlob = serde_json::from_slice(&bytes)
            .map_err(|e| RnError::Storage(format!("parse RN health blob: {e}")))?;
        if blob.version != HEALTH_BLOB_VERSION {
            return Err(RnError::Storage(format!(
                "RN health blob version {} != {HEALTH_BLOB_VERSION}",
                blob.version
            )));
        }
        if blob.method != sealer.method_label() {
            return Err(RnError::Storage(
                "RN health blob was sealed by a different sealer".into(),
            ));
        }
        let sealed =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, blob.sealed_b64)
                .map_err(|e| RnError::Storage(format!("decode RN health blob: {e}")))?;
        let plain = sealer
            .unseal(&sealed)
            .map_err(|e| RnError::Storage(format!("unseal RN health blob: {e}")))?;
        serde_json::from_slice(&plain)
            .map_err(|e| RnError::Storage(format!("parse RN health record: {e}")))
    }

    pub fn save(&self, peer: &[u8; 32], record: &RnPeerHealth) -> Result<(), RnError> {
        let sealer = keystore::select_best_sealer();
        self.save_with_sealer(peer, record, sealer.as_ref())
    }

    pub fn save_with_sealer(
        &self,
        peer: &[u8; 32],
        record: &RnPeerHealth,
        sealer: &dyn keystore::sealer::Sealer,
    ) -> Result<(), RnError> {
        if sealer.requires_insecure_banner() {
            return Err(RnError::PlaintextSealerRefused);
        }
        let plain = serde_json::to_vec(record)
            .map_err(|e| RnError::Storage(format!("serialize RN health record: {e}")))?;
        let sealed = sealer
            .seal(&plain)
            .map_err(|e| RnError::Storage(format!("seal RN health record: {e}")))?;
        let blob = SealedHealthBlob {
            version: HEALTH_BLOB_VERSION,
            method: sealer.method_label().to_owned(),
            sealed_b64: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, sealed),
        };
        let bytes = serde_json::to_vec(&blob)
            .map_err(|e| RnError::Storage(format!("serialize RN health blob: {e}")))?;
        if bytes.len() as u64 > MAX_HEALTH_FILE_BYTES {
            return Err(RnError::StateTooLarge {
                got: bytes.len(),
                max: MAX_HEALTH_FILE_BYTES,
            });
        }
        atomic_write(&self.health_path(peer), &bytes)
    }

    fn health_path(&self, peer: &[u8; 32]) -> PathBuf {
        let mut hash = Sha256::new();
        hash.update(b"OSL-RN/v1/health-file/");
        hash.update(peer);
        let name: String = hash
            .finalize()
            .iter()
            .take(16)
            .map(|byte| format!("{byte:02x}"))
            .collect();
        self.dir.join(format!("{name}.health"))
    }
}

fn read_bounded(path: &Path) -> Result<Option<Vec<u8>>, RnError> {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(RnError::Storage(format!("inspect RN health file: {error}"))),
    };
    if metadata.len() > MAX_HEALTH_FILE_BYTES {
        return Err(RnError::StateTooLarge {
            got: metadata.len() as usize,
            max: MAX_HEALTH_FILE_BYTES,
        });
    }
    std::fs::read(path)
        .map(Some)
        .map_err(|error| RnError::Storage(format!("read RN health file: {error}")))
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), RnError> {
    use std::io::Write as _;

    let parent = path
        .parent()
        .ok_or_else(|| RnError::Storage("RN health path has no parent".into()))?;
    std::fs::create_dir_all(parent)
        .map_err(|error| RnError::Storage(format!("create RN health directory: {error}")))?;
    let suffix = NEXT_TEMP_SUFFIX.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_extension(format!("health.{}.{}.tmp", std::process::id(), suffix));
    let result = (|| {
        let mut file = std::fs::File::create(&tmp)
            .map_err(|error| RnError::Storage(format!("create RN health temp file: {error}")))?;
        file.write_all(bytes)
            .map_err(|error| RnError::Storage(format!("write RN health temp file: {error}")))?;
        file.sync_all()
            .map_err(|error| RnError::Storage(format!("fsync RN health temp file: {error}")))?;
        std::fs::rename(&tmp, path)
            .map_err(|error| RnError::Storage(format!("rename RN health file: {error}")))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use keystore::sealer::MemorySealer;

    #[test]
    fn desync_is_durable_and_only_successful_decrypt_clears_it() {
        let directory = tempfile::tempdir().expect("temporary health directory");
        let store = RnHealthStore::new(directory.path());
        let sealer = MemorySealer::new();
        let peer = [42; 32];

        let mut record = store
            .load_with_sealer(&peer, &sealer)
            .expect("empty record");
        record.observe_pinned_symptom(RnDesyncSymptom::MissingSession);
        store
            .save_with_sealer(&peer, &record, &sealer)
            .expect("persist desync");

        let mut reloaded = store
            .load_with_sealer(&peer, &sealer)
            .expect("reload desync");
        assert_eq!(reloaded.health(), RnSessionHealth::Desynced);
        reloaded.observe_pinned_symptom(RnDesyncSymptom::AuthFailed);
        assert_eq!(reloaded.health(), RnSessionHealth::Desynced);

        reloaded.successful_decrypt();
        assert_eq!(reloaded.health(), RnSessionHealth::Healthy);
        assert_eq!(reloaded.consecutive_auth_failures(), 0);
    }

    #[test]
    fn health_transitions_are_explicit_and_never_silent() {
        let mut record = RnPeerHealth::default();
        assert_eq!(record.health(), RnSessionHealth::Healthy);

        record.observe_pinned_symptom(RnDesyncSymptom::AuthFailed);
        assert_eq!(record.health(), RnSessionHealth::Degraded);
        assert_eq!(record.consecutive_auth_failures(), 1);
        record.observe_pinned_symptom(RnDesyncSymptom::AuthFailed);
        assert_eq!(record.health(), RnSessionHealth::Degraded);
        record.observe_pinned_symptom(RnDesyncSymptom::AuthFailed);
        assert_eq!(record.health(), RnSessionHealth::Desynced);

        record.successful_decrypt();
        assert_eq!(record.health(), RnSessionHealth::Healthy);

        record.observe_pinned_symptom(RnDesyncSymptom::MaxSkipPerMessageRefused);
        assert_eq!(record.health(), RnSessionHealth::Desynced);
        record.unrecoverable();
        record.successful_decrypt();
        assert_eq!(record.health(), RnSessionHealth::Unrecoverable);
        record.remediation_completed();
        assert_eq!(record.health(), RnSessionHealth::Healthy);
    }
}
