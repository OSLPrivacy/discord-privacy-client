use crate::secret_trace::{self, Protection, SecretClass};
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// [`write_recoverable`] for a payload that carries secret material.
///
/// TASK 5402: the caller declares the class and the protection it already
/// applied, and this records the committed file AND the two companions this
/// primitive creates. The `.bak` companion is a complete previous copy of the
/// record, so an at-rest audit that only ever looked at `identity.json` would
/// be blind to a full second copy of the same secret sitting beside it.
pub(crate) fn write_recoverable_secret(
    path: &Path,
    bytes: &[u8],
    class: SecretClass,
    protection: Protection,
    writer: &'static str,
) -> std::io::Result<()> {
    let result = write_recoverable(path, bytes);
    if result.is_ok() {
        secret_trace::record_recoverable_write(class, protection, writer, path, bytes.len());
    }
    result
}

pub(crate) fn write_recoverable(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("recoverable path has no parent"))?;
    if !parent.as_os_str().is_empty() {
        std::fs::create_dir_all(parent)?;
    }

    let temporary = companion_path(path, "tmp");
    let backup = companion_path(path, "bak");
    remove_if_present(&temporary)?;
    {
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }

    remove_if_present(&backup)?;
    let had_previous = path.exists();
    if had_previous {
        std::fs::rename(path, &backup)?;
    }
    if let Err(error) = std::fs::rename(&temporary, path) {
        if had_previous {
            let _ = std::fs::rename(&backup, path);
        }
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    match std::fs::read(path) {
        Ok(committed) if committed == bytes => Ok(()),
        Ok(_) | Err(_) => {
            if had_previous {
                let _ = std::fs::rename(&backup, path);
            }
            Err(std::io::Error::other(
                "recoverable write verification failed",
            ))
        }
    }
}

fn companion_path(path: &Path, extension: &str) -> PathBuf {
    path.with_extension(extension)
}

fn remove_if_present(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
