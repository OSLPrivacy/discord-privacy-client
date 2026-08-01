//! Durable, payload-free message tombstones.
//!
//! Tombstones are deliberately a separate encrypted state file: destroying a
//! message payload must not destroy the receipt- and acknowledgement-addressable
//! record of that message. The shared [`message_lifecycle::Tombstone`] type has
//! no plaintext, key, pointer, or server-management capability.

use message_lifecycle::Tombstone;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

pub const TOMBSTONE_FILE: &str = "tombstones.json";

#[derive(Debug, Clone, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TombstoneFile {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub entries: Vec<Tombstone>,
}

/// Set after an existing tombstone file cannot be decrypted or parsed.
///
/// An empty `TombstoneFile` is only a container returned to keep callers from
/// treating I/O failure as a normal data shape. `is_message_destroyed` and
/// `write_tombstones` consult this latch, so no unreadable record becomes
/// available and no incomplete replacement is persisted over it.
static TOMBSTONES_UNREADABLE: AtomicBool = AtomicBool::new(false);

pub fn tombstones_unreadable() -> bool {
    TOMBSTONES_UNREADABLE.load(Ordering::SeqCst)
}

/// Test-only escape hatch for the process-global fail-closed latch.
pub fn reset_tombstones_unreadable_for_tests() {
    TOMBSTONES_UNREADABLE.store(false, Ordering::SeqCst);
}

impl TombstoneFile {
    /// Inserts or replaces the terminal record for one message without ever
    /// retaining payload material alongside it.
    pub fn record(&mut self, tombstone: Tombstone) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.message_id == tombstone.message_id)
        {
            *entry = tombstone;
        } else {
            self.entries.push(tombstone);
        }
        self.version = 1;
    }

    /// Fail closed: if the durable record was unreadable, every message is
    /// treated as destroyed until the application can safely recover it.
    pub fn is_message_destroyed(&self, message_id: &[u8; 32]) -> bool {
        tombstones_unreadable()
            || self
                .entries
                .iter()
                .any(|entry| &entry.message_id == message_id)
    }
}

/// Loads tombstones from their encrypted at-rest file.
///
/// A missing file is a fresh-install state. An existing file which cannot be
/// opened, decrypted, or parsed latches fail-closed; it never means that no
/// messages were destroyed.
pub fn load_tombstones(path: &Path) -> TombstoneFile {
    let blob = match std::fs::read(path) {
        Ok(blob) => blob,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return TombstoneFile::default();
        }
        Err(error) => {
            tracing::error!(error = %error, "OSL: tombstones unreadable — treating all messages as destroyed");
            TOMBSTONES_UNREADABLE.store(true, Ordering::SeqCst);
            return TombstoneFile::default();
        }
    };
    let plain = match crate::main_password::maybe_decrypt_file(path, &blob) {
        Ok(plain) => plain,
        Err(error) => {
            tracing::error!(error = %error, "OSL: tombstones unreadable — treating all messages as destroyed");
            TOMBSTONES_UNREADABLE.store(true, Ordering::SeqCst);
            return TombstoneFile::default();
        }
    };
    match serde_json::from_slice(&plain) {
        Ok(file) => {
            TOMBSTONES_UNREADABLE.store(false, Ordering::SeqCst);
            file
        }
        Err(error) => {
            tracing::error!(error = %error, "OSL: tombstones unparseable — treating all messages as destroyed");
            TOMBSTONES_UNREADABLE.store(true, Ordering::SeqCst);
            TombstoneFile::default()
        }
    }
}

/// Persists tombstones atomically through the standard encrypted-at-rest path.
pub fn write_tombstones(path: &Path, file: &TombstoneFile) -> Result<(), String> {
    if tombstones_unreadable() {
        return Err(
            "OSL: refusing to write tombstones — the existing destruction record could not be read"
                .to_owned(),
        );
    }
    let body = serde_json::to_vec_pretty(file)
        .map_err(|error| format!("OSL: serialize tombstones: {error}"))?;
    let encrypted = crate::main_password::maybe_encrypt(&body)
        .map_err(|error| format!("OSL: encrypt tombstones: {error}"))?;
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, encrypted)
        .map_err(|error| format!("OSL: write {}: {error}", temporary.display()))?;
    std::fs::rename(&temporary, path)
        .map_err(|error| format!("OSL: rename {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use message_lifecycle::{AckOutcome, DestructReason, MessageDirection};
    use tempfile::tempdir;

    struct FileKeyReset;

    impl Drop for FileKeyReset {
        fn drop(&mut self) {
            crate::main_password::set_file_storage_key(None);
            reset_tombstones_unreadable_for_tests();
        }
    }

    fn tombstone(message_byte: u8) -> Tombstone {
        Tombstone {
            message_id: [message_byte; 32],
            peer_id: [2; 32],
            conversation_id: [3; 32],
            direction: MessageDirection::Incoming,
            created_at: 10,
            destroyed_at: 20,
            reason: DestructReason::Burn,
            delivered_at: Some(15),
            opened_at: None,
            destruction_ack: AckOutcome::Destroyed,
        }
    }

    #[test]
    fn tf_02_unreadable_tombstones_latch_fail_closed_and_cannot_be_overwritten() {
        let _serial = crate::test_process_globals::serialize();
        let _reset = FileKeyReset;
        reset_tombstones_unreadable_for_tests();
        let dir = tempdir().unwrap();
        let path = dir.path().join(TOMBSTONE_FILE);
        std::fs::write(&path, b"not a tombstone file").unwrap();
        let original = std::fs::read(&path).unwrap();

        let loaded = load_tombstones(&path);

        assert!(tombstones_unreadable());
        assert!(loaded.is_message_destroyed(&[99; 32]));
        assert!(write_tombstones(&path, &loaded).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }

    #[test]
    fn tombstones_round_trip_encrypted_and_replace_by_message_id() {
        let _serial = crate::test_process_globals::serialize();
        let _reset = FileKeyReset;
        reset_tombstones_unreadable_for_tests();
        crate::main_password::set_file_storage_key(Some([0x41; 32]));
        let dir = tempdir().unwrap();
        let path = dir.path().join(TOMBSTONE_FILE);
        let mut file = TombstoneFile::default();
        file.record(tombstone(1));
        let mut replacement = tombstone(1);
        replacement.reason = DestructReason::ViewOnceConsumed;
        file.record(replacement);

        write_tombstones(&path, &file).unwrap();
        assert!(crate::main_password::has_enc_magic(
            &std::fs::read(&path).unwrap()
        ));
        assert_eq!(load_tombstones(&path), file);
        assert!(file.is_message_destroyed(&[1; 32]));
        assert!(!file.is_message_destroyed(&[2; 32]));
        assert_eq!(file.entries.len(), 1);
    }
}
