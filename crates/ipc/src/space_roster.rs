//! Durable encrypted storage for the client-authoritative Space roster.
//!
//! The roster schema is deliberately owned by the Space lifecycle work. This
//! module owns only its at-rest boundary: a roster is never created or
//! overwritten without the unlocked file-storage key, and writes are atomic.

use std::path::Path;

/// The single account-relative path for the Space roster.
///
/// Keep this constant as the source of truth for every lifecycle sweep. A
/// Space roster contains membership state and must move with an identity,
/// survive password changes, and be present in an encrypted data export.
pub const SPACE_ROSTER_FILE: &str = "space_roster.json";

#[derive(Debug, thiserror::Error)]
pub enum SpaceRosterFileError {
    #[error("space roster not found at {0}")]
    NotFound(String),
    #[error("space roster read failed at {path}: {source}")]
    ReadFailed {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("space roster decrypt failed at {path}: {reason}")]
    DecryptFailed { path: String, reason: String },
    #[error("space roster write failed at {path}: {source}")]
    WriteFailed {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

/// Loads the decrypted serialized roster. Interpretation belongs to the
/// roster-state owner so persistence does not freeze a schema ahead of it.
pub fn load_space_roster(path: &Path) -> Result<Vec<u8>, SpaceRosterFileError> {
    let encrypted = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            return Err(SpaceRosterFileError::NotFound(path.display().to_string()));
        }
        Err(source) => {
            return Err(SpaceRosterFileError::ReadFailed {
                path: path.display().to_string(),
                source,
            });
        }
    };

    crate::main_password::maybe_decrypt_file(path, &encrypted).map_err(|reason| {
        SpaceRosterFileError::DecryptFailed {
            path: path.display().to_string(),
            reason,
        }
    })
}

/// Encrypts and atomically replaces the serialized roster.
///
/// A locked client must refuse rather than create plaintext state or overwrite
/// the encrypted roster with data it cannot protect.
pub fn write_space_roster(
    path: &Path,
    serialized_roster: &[u8],
) -> Result<(), SpaceRosterFileError> {
    let key = crate::main_password::get_file_storage_key().ok_or_else(|| {
        SpaceRosterFileError::WriteFailed {
            path: path.display().to_string(),
            source: std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "OSL: refusing to write plaintext space roster without file_storage_key",
            ),
        }
    })?;
    let encrypted =
        crate::main_password::encrypt_at_rest(serialized_roster, &key).map_err(|source| {
            SpaceRosterFileError::WriteFailed {
                path: path.display().to_string(),
                source: std::io::Error::other(source),
            }
        })?;
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, encrypted).map_err(|source| SpaceRosterFileError::WriteFailed {
        path: temporary.display().to_string(),
        source,
    })?;
    std::fs::rename(&temporary, path).map_err(|source| SpaceRosterFileError::WriteFailed {
        path: path.display().to_string(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::main_password::{has_enc_magic, set_file_storage_key};
    use tempfile::tempdir;

    #[test]
    fn roster_persistence_is_encrypted_atomic_and_refuses_a_locked_write() {
        let _guard = crate::test_process_globals::serialize();
        let directory = tempdir().unwrap();
        let path = directory.path().join(SPACE_ROSTER_FILE);
        let roster = br#"{"version":1,"spaces":[]}"#;

        set_file_storage_key(None);
        let refused = write_space_roster(&path, roster).unwrap_err();
        assert!(matches!(
            refused,
            SpaceRosterFileError::WriteFailed { source, .. }
                if source.kind() == std::io::ErrorKind::PermissionDenied
        ));
        assert!(
            !path.exists(),
            "a locked write must not create plaintext state"
        );

        set_file_storage_key(Some([0x6d; 32]));
        write_space_roster(&path, roster).unwrap();
        assert!(has_enc_magic(&std::fs::read(&path).unwrap()));
        assert_eq!(load_space_roster(&path).unwrap(), roster);

        set_file_storage_key(None);
    }
}
