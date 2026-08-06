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

pub(crate) fn write_recoverable(path: &Path, bytes: &[u8], label: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{label} path is invalid"))?;
    std::fs::create_dir_all(parent)
        .map_err(|_| format!("{label} directory could not be created"))?;

    let temporary = temporary_path(path);
    let backup = backup_path(path);
    remove_if_present(&temporary, label)?;
    {
        let mut file = std::fs::File::create(&temporary)
            .map_err(|_| format!("{label} temporary file could not be created"))?;
        file.write_all(bytes)
            .map_err(|_| format!("{label} temporary file could not be written"))?;
        file.sync_all()
            .map_err(|_| format!("{label} temporary file could not be synchronized"))?;
    }

    remove_if_present(&backup, label)?;
    let had_previous = path.exists();
    if had_previous {
        std::fs::rename(path, &backup)
            .map_err(|_| format!("{label} prior file could not be preserved"))?;
    }

    if std::fs::rename(&temporary, path).is_err() {
        if had_previous {
            let _ = std::fs::rename(&backup, path);
        }
        let _ = std::fs::remove_file(&temporary);
        return Err(format!("{label} could not be committed"));
    }

    match std::fs::read(path) {
        Ok(committed) if committed == bytes => Ok(()),
        Ok(_) | Err(_) => {
            if had_previous {
                let _ = std::fs::rename(&backup, path);
            }
            Err(format!("{label} committed file could not be verified"))
        }
    }
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
    fn repeated_replacement_keeps_latest_committed_and_one_previous_copy() {
        let dir = test_path("replace");
        let path = dir.join("state.json");
        write_recoverable(&path, b"one", "test state").unwrap();
        write_recoverable(&path, b"two", "test state").unwrap();
        write_recoverable(&path, b"three", "test state").unwrap();
        assert_eq!(
            read_recoverable(&path, "test state").unwrap(),
            Some(b"three".to_vec())
        );
        assert_eq!(std::fs::read(path.with_extension("bak")).unwrap(), b"two");
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

    fn fingerprint(bytes: &[u8]) -> String {
        let mut hash = 0xcbf29ce484222325_u64;
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        format!("fnv1a64:{hash:016x}:len:{}", bytes.len())
    }

    #[test]
    fn task_3609g_local_save_retains_known_good_previous_copy() {
        let dir = test_path("task-3609g");
        let path = dir.join("state.json");
        let old = br#"{"state":"known-good-old"}"#;
        let new = br#"{"state":"known-good-new"}"#;
        write_recoverable(&path, old, "test state").unwrap();
        let old_fingerprint = fingerprint(old);
        let new_fingerprint = fingerprint(new);
        println!("old_fingerprint={old_fingerprint}");
        println!("new_fingerprint={new_fingerprint}");

        std::fs::rename(&path, path.with_extension("bak")).unwrap();
        let stopped_before_replacement =
            fingerprint(&std::fs::read(path.with_extension("bak")).unwrap());
        println!("stopped_before_replacement={stopped_before_replacement}");
        assert_eq!(stopped_before_replacement, old_fingerprint);
        std::fs::rename(path.with_extension("bak"), &path).unwrap();

        write_recoverable(&path, new, "test state").unwrap();
        let finished_replacement = fingerprint(&std::fs::read(&path).unwrap());
        let previous_copy = std::fs::read(path.with_extension("bak")).unwrap();
        let previous_copy_fingerprint = fingerprint(&previous_copy);
        println!("finished_replacement={finished_replacement}");
        println!("previous_copy_fingerprint={previous_copy_fingerprint}");
        println!("previous_copy_readable_bytes={}", previous_copy.len());
        assert_eq!(finished_replacement, new_fingerprint);
        assert_eq!(previous_copy_fingerprint, old_fingerprint);
        assert_eq!(previous_copy.len(), old.len());
        let _ = std::fs::remove_dir_all(dir);
    }
}
