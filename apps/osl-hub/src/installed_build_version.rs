//! Installed-build version record.
//!
//! This records the package version together with the SHA-256 of the installed
//! file the app is actually starting from. It is local diagnostic state, not a
//! trust decision: integrity policy remains in `build_integrity`.

use crate::atomic_file;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const RECORD_FILE: &str = "installed-build-version.json";
const RECORD_LABEL: &str = "OSL installed-build version record";
const RECORD_MAX_BYTES: u64 = 16 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledBuildVersionRecord {
    pub schema_version: u8,
    pub installed_version: String,
    pub built_file_fingerprint: String,
}

pub fn installed_build_version_record_path(config_dir: &Path) -> PathBuf {
    config_dir.join(RECORD_FILE)
}

pub fn record_current_executable_at_start(
    config_dir: &Path,
) -> Result<InstalledBuildVersionRecord, String> {
    let executable = std::env::current_exe()
        .map_err(|_| "OSL installed-build executable path could not be resolved".to_owned())?;
    let record_path = installed_build_version_record_path(config_dir);
    write_installed_build_version_record_at_start(&record_path, &executable)?;
    cmd_read_installed_build_version_record(&record_path)
}

pub fn write_installed_build_version_record_at_start(
    record_path: &Path,
    built_file: &Path,
) -> Result<InstalledBuildVersionRecord, String> {
    let record = InstalledBuildVersionRecord {
        schema_version: 1,
        installed_version: env!("CARGO_PKG_VERSION").to_owned(),
        built_file_fingerprint: fingerprint_built_file(built_file)?,
    };
    let bytes = serde_json::to_vec_pretty(&record)
        .map_err(|_| "OSL installed-build version record could not be encoded".to_owned())?;
    atomic_file::write_recoverable(record_path, &bytes, RECORD_LABEL)?;
    cmd_read_installed_build_version_record(record_path)
}

pub fn cmd_read_installed_build_version_record(
    record_path: &Path,
) -> Result<InstalledBuildVersionRecord, String> {
    let bytes = atomic_file::read_recoverable_bounded(record_path, RECORD_MAX_BYTES, RECORD_LABEL)?
        .ok_or_else(|| "OSL installed-build version record is missing".to_owned())?;
    let record: InstalledBuildVersionRecord = serde_json::from_slice(&bytes)
        .map_err(|_| "OSL installed-build version record could not be decoded".to_owned())?;
    if record.schema_version != 1 {
        return Err("OSL installed-build version record version is unsupported".to_owned());
    }
    if record.installed_version != env!("CARGO_PKG_VERSION") {
        return Err(
            "OSL installed-build version record does not match this app version".to_owned(),
        );
    }
    if !valid_sha256_hex(&record.built_file_fingerprint) {
        return Err("OSL installed-build version record fingerprint is invalid".to_owned());
    }
    Ok(record)
}

fn fingerprint_built_file(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|_| {
        format!(
            "OSL installed-build file could not be read: {}",
            path.display()
        )
    })?;
    Ok(hex_digest(Sha256::digest(bytes)))
}

fn hex_digest(digest: impl AsRef<[u8]>) -> String {
    digest
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn valid_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value.bytes().all(|byte| {
            byte.is_ascii_digit() || (byte.is_ascii_hexdigit() && byte.is_ascii_lowercase())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_record_shape_is_refused() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join(RECORD_FILE);
        std::fs::write(
            &path,
            format!(
                r#"{{"schemaVersion":1,"installedVersion":"{}","builtFileFingerprint":"not-sha"}}"#,
                env!("CARGO_PKG_VERSION")
            ),
        )
        .unwrap();
        assert_eq!(
            cmd_read_installed_build_version_record(&path).unwrap_err(),
            "OSL installed-build version record fingerprint is invalid"
        );
    }
}
