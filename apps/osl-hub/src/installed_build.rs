//! Installed build version record.

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const INSTALLED_BUILD_RECORD_FILE: &str = "installed-build-record.v1.json";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledBuildRecord {
    pub schema_version: u8,
    pub package: String,
    pub version: String,
    pub built_file: String,
    pub fingerprint: String,
}

pub fn record_current_installed_build(record_dir: &Path) -> Result<InstalledBuildRecord, String> {
    let executable =
        std::env::current_exe().map_err(|error| format!("resolve current executable: {error}"))?;
    store_and_read_installed_build_record(
        &record_dir.join(INSTALLED_BUILD_RECORD_FILE),
        &executable,
    )
}

pub fn store_and_read_installed_build_record(
    record_path: &Path,
    built_file: &Path,
) -> Result<InstalledBuildRecord, String> {
    let record = installed_build_record_for_file(built_file)?;
    write_installed_build_record(record_path, &record)?;
    let read_back = read_installed_build_record(record_path)?;
    if read_back != record {
        return Err("installed build record readback did not match the written record".to_owned());
    }
    Ok(read_back)
}

pub fn installed_build_record_for_file(built_file: &Path) -> Result<InstalledBuildRecord, String> {
    let bytes = fs::read(built_file)
        .map_err(|error| format!("read built file {}: {error}", built_file.display()))?;
    let fingerprint = format!("{:x}", Sha256::digest(&bytes));
    let built_file = canonical_or_original(built_file);
    Ok(InstalledBuildRecord {
        schema_version: 1,
        package: env!("CARGO_PKG_NAME").to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        built_file: built_file.display().to_string(),
        fingerprint,
    })
}

pub fn read_installed_build_record(record_path: &Path) -> Result<InstalledBuildRecord, String> {
    let bytes = fs::read(record_path).map_err(|error| {
        format!(
            "read installed build record {}: {error}",
            record_path.display()
        )
    })?;
    serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "parse installed build record {}: {error}",
            record_path.display()
        )
    })
}

fn write_installed_build_record(
    record_path: &Path,
    record: &InstalledBuildRecord,
) -> Result<(), String> {
    if let Some(parent) = record_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "create installed build record dir {}: {error}",
                parent.display()
            )
        })?;
    }
    let bytes = serde_json::to_vec_pretty(record)
        .map_err(|error| format!("encode installed build record: {error}"))?;
    let temp_path = temp_record_path(record_path);
    fs::write(&temp_path, bytes).map_err(|error| {
        format!(
            "write installed build record temp {}: {error}",
            temp_path.display()
        )
    })?;
    fs::rename(&temp_path, record_path).map_err(|error| {
        format!(
            "replace installed build record {}: {error}",
            record_path.display()
        )
    })
}

fn temp_record_path(record_path: &Path) -> PathBuf {
    let mut name = record_path
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_else(|| "installed-build-record".into());
    name.push(format!(".{}.tmp", std::process::id()));
    record_path.with_file_name(name)
}

fn canonical_or_original(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_build_fingerprint_tracks_one_byte_change() {
        let directory = tempfile::tempdir().expect("temp dir");
        let built_file = directory.path().join("installed.bin");
        let record_path = directory.path().join(INSTALLED_BUILD_RECORD_FILE);

        fs::write(&built_file, b"built file").expect("write built file");
        let first = store_and_read_installed_build_record(&record_path, &built_file)
            .expect("write first record");

        fs::write(&built_file, b"built file!").expect("write one byte changed built file");
        let second = store_and_read_installed_build_record(&record_path, &built_file)
            .expect("write second record");

        assert_eq!(first.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(second.version, env!("CARGO_PKG_VERSION"));
        assert_ne!(first.fingerprint, second.fingerprint);
        assert_eq!(
            read_installed_build_record(&record_path).expect("read record"),
            second
        );
    }
}
