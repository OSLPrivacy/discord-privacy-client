//! Pure validation helpers for the trusted OSL Privacy updater boundary.

use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use sha2::{Digest, Sha256};

pub const RELEASES_URL: &str = "https://github.com/OSLPrivacy/discord-privacy-client/releases";
pub const SOURCE_REPOSITORY_URL: &str = "https://github.com/OSLPrivacy/discord-privacy-client";
pub const MAX_RELEASE_NOTES_CHARS: usize = 2_000;
pub const UPDATE_DOWNLOAD_READY_TO_INSTALL: &str = "ready to install";
pub const UPDATE_DOWNLOAD_FINGERPRINT_MISMATCH: &str = "fingerprint mismatch";

pub fn bounded_plain_notes(input: Option<&str>) -> String {
    let input = input.unwrap_or_default();
    input
        .chars()
        .filter(|character| !character.is_control() && *character != '<' && *character != '>')
        .take(MAX_RELEASE_NOTES_CHARS)
        .collect()
}

pub fn bounded_version(input: &str) -> Option<String> {
    if input.is_empty()
        || input.len() > 64
        || !input
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ".+-".contains(character))
    {
        return None;
    }
    Some(input.to_owned())
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedUpdateDownload {
    pub status: &'static str,
    pub staged_path: PathBuf,
    pub bytes: u64,
    pub expected_fingerprint: String,
    pub actual_fingerprint: String,
}

#[derive(Debug, Eq, PartialEq)]
pub enum UpdateDownloadGateError {
    InvalidExpectedFingerprint,
    CreateStagingDirectory,
    CreateStagingFile,
    DownloadFailed,
    FlushFailed,
    ReadBackFailed,
    FingerprintMismatch {
        expected_fingerprint: String,
        actual_fingerprint: String,
        staged_path: PathBuf,
        deleted_downloaded_file: bool,
    },
}

impl fmt::Display for UpdateDownloadGateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidExpectedFingerprint => {
                formatter.write_str("expected update fingerprint is invalid")
            }
            Self::CreateStagingDirectory => {
                formatter.write_str("update download staging directory could not be created")
            }
            Self::CreateStagingFile => {
                formatter.write_str("update download staging file could not be created")
            }
            Self::DownloadFailed => formatter.write_str("update download failed"),
            Self::FlushFailed => formatter.write_str("update download could not be flushed"),
            Self::ReadBackFailed => formatter.write_str("update download could not be read back"),
            Self::FingerprintMismatch { .. } => {
                formatter.write_str(UPDATE_DOWNLOAD_FINGERPRINT_MISMATCH)
            }
        }
    }
}

impl std::error::Error for UpdateDownloadGateError {}

pub fn expected_download_fingerprint(
    raw_manifest: &serde_json::Value,
    target: &str,
) -> Option<String> {
    let platform = raw_manifest
        .get("platforms")
        .and_then(serde_json::Value::as_object)
        .and_then(|platforms| platforms.get(target));
    platform
        .and_then(fingerprint_from_object)
        .or_else(|| fingerprint_from_object(raw_manifest))
}

pub fn prepare_verified_update_download(
    staging_root: &Path,
    expected_fingerprint: &str,
    download: impl FnOnce(&mut dyn Write) -> io::Result<()>,
) -> Result<VerifiedUpdateDownload, UpdateDownloadGateError> {
    let Some(expected_fingerprint) = normalize_sha256(expected_fingerprint) else {
        return Err(UpdateDownloadGateError::InvalidExpectedFingerprint);
    };
    let staging_dir = staging_root.join("update-downloads");
    fs::create_dir_all(&staging_dir)
        .map_err(|_| UpdateDownloadGateError::CreateStagingDirectory)?;
    let (staged_path, mut file) = create_staging_file(&staging_dir)?;
    if download(&mut file).is_err() {
        let _ = fs::remove_file(&staged_path);
        return Err(UpdateDownloadGateError::DownloadFailed);
    }
    file.flush()
        .and_then(|()| file.sync_all())
        .map_err(|_| UpdateDownloadGateError::FlushFailed)?;
    drop(file);

    let payload = fs::read(&staged_path).map_err(|_| {
        let _ = fs::remove_file(&staged_path);
        UpdateDownloadGateError::ReadBackFailed
    })?;
    let bytes = u64::try_from(payload.len()).unwrap_or(u64::MAX);
    let actual_fingerprint = sha256_hex(&payload);
    if actual_fingerprint != expected_fingerprint {
        let deleted_downloaded_file = fs::remove_file(&staged_path).is_ok();
        return Err(UpdateDownloadGateError::FingerprintMismatch {
            expected_fingerprint,
            actual_fingerprint,
            staged_path,
            deleted_downloaded_file,
        });
    }

    Ok(VerifiedUpdateDownload {
        status: UPDATE_DOWNLOAD_READY_TO_INSTALL,
        staged_path,
        bytes,
        expected_fingerprint,
        actual_fingerprint,
    })
}

fn fingerprint_from_object(value: &serde_json::Value) -> Option<String> {
    let object = value.as_object()?;
    for field in [
        "fingerprint",
        "sha256",
        "expectedSha256",
        "expectedFingerprint",
        "candidateSha256",
        "installerSha256",
    ] {
        if let Some(normalized) = object
            .get(field)
            .and_then(serde_json::Value::as_str)
            .and_then(normalize_sha256)
        {
            return Some(normalized);
        }
    }
    None
}

fn create_staging_file(staging_dir: &Path) -> Result<(PathBuf, File), UpdateDownloadGateError> {
    let process_id = std::process::id();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    for attempt in 0..128u8 {
        let path = staging_dir.join(format!(
            "osl-update-download-{process_id}-{now}-{attempt}.bin"
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(UpdateDownloadGateError::CreateStagingFile),
        }
    }
    Err(UpdateDownloadGateError::CreateStagingFile)
}

fn normalize_sha256(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.len() == 64
        && trimmed
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        Some(trimmed.to_ascii_lowercase())
    } else {
        None
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_metadata_is_plain_bounded_text() {
        assert_eq!(
            bounded_plain_notes(Some("safe <b>notes</b>\0")),
            "safe bnotes/b"
        );
        assert_eq!(bounded_plain_notes(Some(&"x".repeat(2_100))).len(), 2_000);
    }

    #[test]
    fn versions_are_bounded_and_never_markup() {
        assert_eq!(
            bounded_version("0.2.0-beta.1").as_deref(),
            Some("0.2.0-beta.1")
        );
        assert!(bounded_version("<script>").is_none());
        assert!(bounded_version(&"1".repeat(65)).is_none());
    }

    #[test]
    fn external_pages_are_exact_compiled_in_github_destinations() {
        assert_eq!(
            SOURCE_REPOSITORY_URL,
            "https://github.com/OSLPrivacy/discord-privacy-client"
        );
        assert_eq!(
            RELEASES_URL,
            "https://github.com/OSLPrivacy/discord-privacy-client/releases"
        );
    }

    #[test]
    fn task_3170_update_download_fingerprint_gate_reports_ready_and_deletes_mismatch() {
        let root = tempfile::tempdir().expect("create update gate temp root");
        let payload = b"new OSL build bytes";
        let expected = sha256_hex(payload);

        let ready = prepare_verified_update_download(root.path(), &expected, |writer| {
            writer.write_all(payload)
        })
        .expect("matching update download should be ready");
        assert_eq!(ready.status, UPDATE_DOWNLOAD_READY_TO_INSTALL);
        assert_eq!(ready.bytes, payload.len() as u64);
        assert_eq!(ready.expected_fingerprint, expected);
        assert_eq!(ready.actual_fingerprint, expected);
        assert!(ready.staged_path.exists());

        let mut changed = payload.to_vec();
        changed[0] ^= 0x01;
        let mismatch = prepare_verified_update_download(root.path(), &expected, |writer| {
            writer.write_all(&changed)
        })
        .expect_err("one changed byte must produce a fingerprint mismatch");
        let UpdateDownloadGateError::FingerprintMismatch {
            actual_fingerprint,
            staged_path,
            deleted_downloaded_file,
            ..
        } = mismatch
        else {
            panic!("one changed byte must be a fingerprint mismatch");
        };
        assert_ne!(actual_fingerprint, expected);
        assert!(deleted_downloaded_file);
        assert!(!staged_path.exists());

        println!(
            "TASK3170_MATCHING_DOWNLOAD_STATUS=\"{}\"",
            UPDATE_DOWNLOAD_READY_TO_INSTALL
        );
        println!("TASK3170_MATCHING_DOWNLOAD_BYTES={}", ready.bytes);
        println!(
            "TASK3170_EXPECTED_FINGERPRINT={}",
            ready.expected_fingerprint
        );
        println!(
            "TASK3170_MATCHING_DOWNLOAD_ACTUAL_FINGERPRINT={}",
            ready.actual_fingerprint
        );
        println!(
            "TASK3170_ONE_BYTE_CHANGED_MISMATCH=\"{}\"",
            UPDATE_DOWNLOAD_FINGERPRINT_MISMATCH
        );
        println!("TASK3170_MISMATCH_ACTUAL_FINGERPRINT={actual_fingerprint}");
        println!(
            "TASK3170_MISMATCH_DELETED_DOWNLOADED_FILE={}",
            deleted_downloaded_file
        );
        println!(
            "TASK3170_MISMATCH_STAGED_FILE_EXISTS_AFTER={}",
            staged_path.exists()
        );
    }
}
