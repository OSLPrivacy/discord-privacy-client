//! Crash-recoverable file replacement that also works on Windows.
//!
//! `std::fs::rename(tmp, destination)` cannot replace an existing destination
//! on Windows. Security state is rewritten frequently, so preserve the last
//! committed file as a sibling backup until the new file is in place.

use std::io::Write as _;
use std::path::Path;

pub(crate) fn read_recoverable(path: &Path, label: &str) -> Result<Option<Vec<u8>>, String> {
    match std::fs::read(path) {
        // A backup can coexist with the primary when a process exits after
        // commit but before cleanup. Leave it in place until the next write;
        // callers may still need the last committed copy if decoding the new
        // primary detects corruption.
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let backup = backup_path(path);
            match std::fs::read(&backup) {
                Ok(bytes) => {
                    // Copy rather than rename so a failed recovery still leaves
                    // the last committed bytes available on the next launch.
                    std::fs::copy(&backup, path)
                        .map_err(|_| format!("{label} backup could not be recovered"))?;
                    remove_if_present(&backup, label)?;
                    Ok(Some(bytes))
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(_) => Err(format!("{label} backup could not be read")),
            }
        }
        Err(_) => Err(format!("{label} could not be read")),
    }
}

pub(crate) fn read_recoverable_bounded(
    path: &Path,
    max_bytes: u64,
    label: &str,
) -> Result<Option<Vec<u8>>, String> {
    for candidate in [path.to_path_buf(), backup_path(path)] {
        match std::fs::symlink_metadata(candidate) {
            Ok(metadata)
                if metadata.file_type().is_symlink()
                    || !metadata.is_file()
                    || metadata.len() > max_bytes =>
            {
                return Err(format!("{label} is not a bounded regular file"));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(format!("{label} metadata could not be read")),
        }
    }
    read_recoverable(path, label)
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecoverableWriteFault {
    TemporaryCreate,
    TemporaryWrite,
    TemporarySync,
    CommitRename,
}

#[cfg_attr(not(test), allow(dead_code))]
impl RecoverableWriteFault {
    pub(crate) const fn write_points() -> [Self; 4] {
        [
            Self::TemporaryCreate,
            Self::TemporaryWrite,
            Self::TemporarySync,
            Self::CommitRename,
        ]
    }

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::TemporaryCreate => "temporary_create",
            Self::TemporaryWrite => "temporary_write",
            Self::TemporarySync => "temporary_sync",
            Self::CommitRename => "commit_rename",
        }
    }
}

pub(crate) fn write_recoverable(path: &Path, bytes: &[u8], label: &str) -> Result<(), String> {
    write_recoverable_inner(path, bytes, label, None)
}

#[cfg(test)]
pub(crate) fn write_recoverable_failing_at(
    path: &Path,
    bytes: &[u8],
    label: &str,
    fault: RecoverableWriteFault,
) -> Result<(), String> {
    write_recoverable_inner(path, bytes, label, Some(fault))
}

fn write_recoverable_inner(
    path: &Path,
    bytes: &[u8],
    label: &str,
    fault: Option<RecoverableWriteFault>,
) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{label} path is invalid"))?;
    std::fs::create_dir_all(parent)
        .map_err(|_| format!("{label} directory could not be created"))?;

    let temporary = temporary_path(path);
    let backup = backup_path(path);
    remove_if_present(&temporary, label)?;
    {
        fail_if_requested(fault, RecoverableWriteFault::TemporaryCreate, label)?;
        let mut file = std::fs::File::create(&temporary)
            .map_err(|_| format!("{label} temporary file could not be created"))?;
        fail_if_requested(fault, RecoverableWriteFault::TemporaryWrite, label)?;
        file.write_all(bytes)
            .map_err(|_| format!("{label} temporary file could not be written"))?;
        fail_if_requested(fault, RecoverableWriteFault::TemporarySync, label)?;
        file.sync_all()
            .map_err(|_| format!("{label} temporary file could not be synchronized"))?;
    }

    remove_if_present(&backup, label)?;
    let had_previous = path.exists();
    if had_previous {
        std::fs::rename(path, &backup)
            .map_err(|_| format!("{label} prior file could not be preserved"))?;
    }

    if fault == Some(RecoverableWriteFault::CommitRename) {
        if had_previous {
            let _ = std::fs::rename(&backup, path);
        }
        let _ = std::fs::remove_file(&temporary);
        return Err(format!(
            "{label} could not be committed (simulated full disk at {})",
            RecoverableWriteFault::CommitRename.label()
        ));
    }
    if std::fs::rename(&temporary, path).is_err() {
        if had_previous {
            let _ = std::fs::rename(&backup, path);
        }
        let _ = std::fs::remove_file(&temporary);
        return Err(format!("{label} could not be committed"));
    }
    if had_previous {
        remove_if_present(&backup, label)?;
    }
    Ok(())
}

fn fail_if_requested(
    fault: Option<RecoverableWriteFault>,
    point: RecoverableWriteFault,
    label: &str,
) -> Result<(), String> {
    if fault == Some(point) {
        return Err(format!("{label} simulated full disk at {}", point.label()));
    }
    Ok(())
}

fn temporary_path(path: &Path) -> std::path::PathBuf {
    path.with_extension("tmp")
}

fn backup_path(path: &Path) -> std::path::PathBuf {
    path.with_extension("bak")
}

fn remove_if_present(path: &Path, label: &str) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(format!("{label} stale recovery file could not be removed")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_path(label: &str) -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "osl-hub-atomic-{label}-{}-{}",
            std::process::id(),
            nonce
        ))
    }

    #[test]
    fn repeated_replacement_keeps_latest_committed_bytes() {
        let dir = test_path("replace");
        let path = dir.join("state.json");
        write_recoverable(&path, b"one", "test state").unwrap();
        write_recoverable(&path, b"two", "test state").unwrap();
        write_recoverable(&path, b"three", "test state").unwrap();
        assert_eq!(
            read_recoverable(&path, "test state").unwrap(),
            Some(b"three".to_vec())
        );
        assert!(!path.with_extension("bak").exists());
        assert!(!path.with_extension("tmp").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_primary_recovers_last_committed_backup() {
        let dir = test_path("recover");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.json");
        std::fs::write(path.with_extension("bak"), b"safe").unwrap();
        std::fs::write(path.with_extension("tmp"), b"incomplete").unwrap();
        assert_eq!(
            read_recoverable(&path, "test state").unwrap(),
            Some(b"safe".to_vec())
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"safe");
        assert_eq!(
            std::fs::read(path.with_extension("tmp")).unwrap(),
            b"incomplete"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn bounded_read_refuses_oversized_recovery_input() {
        let dir = test_path("bounded");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.json");
        std::fs::write(path.with_extension("bak"), b"oversized").unwrap();
        assert!(read_recoverable_bounded(&path, 4, "test state").is_err());
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
