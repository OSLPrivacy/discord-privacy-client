//! Crash-recoverable file replacement that also works on Windows.
//!
//! `std::fs::rename(tmp, destination)` cannot replace an existing destination
//! on Windows. Security state is rewritten frequently, so preserve the last
//! committed file as a sibling backup until the new file is in place and the
//! replacement can be opened again.

use std::io::Write as _;
use std::path::Path;
#[cfg(test)]
use std::sync::Mutex;

#[cfg(test)]
static FAIL_NEXT_WRITE_LABEL: Mutex<Option<String>> = Mutex::new(None);

#[cfg(test)]
pub(crate) fn fail_next_write_with_label(label: &str) {
    *FAIL_NEXT_WRITE_LABEL
        .lock()
        .expect("atomic write failure hook lock") = Some(label.to_owned());
}

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
    #[cfg(test)]
    if FAIL_NEXT_WRITE_LABEL
        .lock()
        .expect("atomic write failure hook lock")
        .take()
        .is_some_and(|expected| expected == label)
    {
        return Err(format!("{label} simulated disk full during write"));
    }

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
        match std::fs::read(path) {
            Ok(read_back) if read_back == bytes => {}
            Ok(_) | Err(_) => {
                let _ = std::fs::remove_file(path);
                let _ = std::fs::rename(&backup, path);
                return Err(format!("{label} replacement could not be verified"));
            }
        }
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
    }


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

    fn fingerprint(bytes: &[u8]) -> String {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        format!("fnv1a64:{hash:016x}:len:{}", bytes.len())
    }

    fn restart_read_fingerprint(path: &Path) -> String {
        let bytes = read_recoverable(path, "test state")
            .unwrap()
            .expect("restart read returns a committed item");
        fingerprint(&bytes)
    }

    #[test]
    #[test]
    fn task_3609g_local_save_retains_known_good_previous_copy() {
        let dir = test_path("task-3609g");
        let path = dir.join("state.json");
        let old = br#"{"generation":"old","value":1}"#;
        let new = br#"{"generation":"new","value":2}"#;
        let old_fingerprint = fingerprint(old);
        let new_fingerprint = fingerprint(new);

        write_recoverable(&path, old, "test state").unwrap();
        std::fs::write(temporary_path(&path), new).unwrap();
        let stopped_before_replacement = fingerprint(&std::fs::read(&path).unwrap());
        assert_eq!(stopped_before_replacement, old_fingerprint);

        write_recoverable(&path, new, "test state").unwrap();
        let finished_replacement = fingerprint(
            &read_recoverable(&path, "test state")
                .unwrap()
                .expect("replacement remains readable"),
        );
        let previous_copy = std::fs::read(backup_path(&path)).expect("previous copy remains");
        let previous_copy_fingerprint = fingerprint(&previous_copy);

        assert_eq!(finished_replacement, new_fingerprint);
        assert_eq!(previous_copy_fingerprint, old_fingerprint);
        assert_eq!(previous_copy, old);
        assert!(!temporary_path(&path).exists());

        println!("old_fingerprint={old_fingerprint}");
        println!("new_fingerprint={new_fingerprint}");
        println!("stopped_before_replacement={stopped_before_replacement}");
        println!("finished_replacement={finished_replacement}");
        println!("previous_copy_fingerprint={previous_copy_fingerprint}");
        println!("previous_copy_readable_bytes={}", previous_copy.len());

        let _ = std::fs::remove_dir_all(dir);
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

    #[test]
    fn task_3648_tears_local_replacement_and_restart_reads_exact_fingerprint() {
        let old = br#"{"item":"exact-old-local-write","generation":1}"#;
        let new = br#"{"item":"exact-new-local-write","generation":2}"#;
        let control = br#"{"control":"unrelated","generation":77}"#;
        let old_fingerprint = fingerprint(old);
        let new_fingerprint = fingerprint(new);
        let control_fingerprint = fingerprint(control);
        let mut restart_fingerprints = Vec::new();
        let mut control_fingerprints = Vec::new();

        println!("TASK3648_OLD_FINGERPRINT={old_fingerprint}");
        println!("TASK3648_NEW_FINGERPRINT={new_fingerprint}");
        println!("TASK3648_CONTROL_FINGERPRINT={control_fingerprint}");

        for stop_point in [
            "before_replacement",
            "during_replacement",
            "after_replacement",
            "completed_write",
        ] {
            let dir = test_path(stop_point);
            let path = dir.join("state.json");
            let control_path = dir.join("control.json");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(&path, old).unwrap();
            std::fs::write(&control_path, control).unwrap();

            match stop_point {
                "before_replacement" => {
                    std::fs::write(temporary_path(&path), new).unwrap();
                    assert_eq!(std::fs::read(&path).unwrap(), old);
                }
                "during_replacement" => {
                    std::fs::write(temporary_path(&path), new).unwrap();
                    std::fs::rename(&path, backup_path(&path)).unwrap();
                    assert_eq!(std::fs::read(backup_path(&path)).unwrap(), old);
                    assert!(!path.exists());
                }
                "after_replacement" => {
                    std::fs::write(backup_path(&path), old).unwrap();
                    std::fs::write(&path, new).unwrap();
                    assert_eq!(std::fs::read(backup_path(&path)).unwrap(), old);
                }
                "completed_write" => {
                    write_recoverable(&path, new, "test state").unwrap();
                    assert_eq!(std::fs::read(backup_path(&path)).unwrap(), old);
                }
                _ => unreachable!(),
            }

            let restart_fingerprint = restart_read_fingerprint(&path);
            let control_after_restart = fingerprint(&std::fs::read(&control_path).unwrap());
            let allowed =
                restart_fingerprint == old_fingerprint || restart_fingerprint == new_fingerprint;
            let control_unchanged = control_after_restart == control_fingerprint;
            println!(
                "TASK3648_RUN stop_point={stop_point} restart_fingerprint={restart_fingerprint} allowed_old_or_new={allowed} control_fingerprint={control_after_restart} control_unchanged={control_unchanged}"
            );
            assert!(allowed);
            assert!(control_unchanged);
            restart_fingerprints.push(restart_fingerprint);
            control_fingerprints.push(control_after_restart);
            let _ = std::fs::remove_dir_all(dir);
        }

        let mixed_fingerprint_count = restart_fingerprints
            .iter()
            .filter(|fingerprint| {
                **fingerprint != old_fingerprint && **fingerprint != new_fingerprint
            })
            .count();
        let control_unchanged_run_count = control_fingerprints
            .iter()
            .filter(|fingerprint| **fingerprint == control_fingerprint)
            .count();
        println!("TASK3648_RUN_COUNT={}", restart_fingerprints.len());
        println!("TASK3648_MIXED_FINGERPRINT_COUNT={mixed_fingerprint_count}");
        println!("TASK3648_CONTROL_UNCHANGED_RUN_COUNT={control_unchanged_run_count}");
        assert_eq!(restart_fingerprints.len(), 4);
        assert_eq!(mixed_fingerprint_count, 0);
        assert_eq!(control_unchanged_run_count, restart_fingerprints.len());
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
