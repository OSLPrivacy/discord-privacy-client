//! Update application receipts.
//!
//! The signed updater owns signature verification and download. This module
//! owns the local handoff semantics OSL depends on: built files are overwritten,
//! account state stays in the config directory, and the update is not closed
//! until the replacement build has reached startup once.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const APPLY_RECORD_FILE: &str = "update-apply-record.json";
const HUB_CORE_DIR: &str = "osl-core";
const IDENTITIES_DIR: &str = "hub-identities";
const MESSAGE_STORE_DB: &str = "messages.sqlite";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateApplyStatus {
    PendingRestart,
    Finished,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UpdateApplyRecord {
    pub schema_version: u32,
    pub previous_version: String,
    pub applied_version: String,
    pub status: UpdateApplyStatus,
    pub applied_at_unix_seconds: u64,
    pub finished_at_unix_seconds: Option<u64>,
    pub successful_start_count: u32,
    pub install_dir: String,
    pub staged_build_dir: String,
    pub protected_state_dir: String,
    pub built_files: Vec<String>,
    pub identity_sha256_before: String,
    pub identity_sha256_after_apply: String,
    pub message_history_sha256_before: String,
    pub message_history_sha256_after_apply: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingUpdateApply {
    app_config_dir: PathBuf,
    install_dir: PathBuf,
    previous_version: String,
    applied_version: String,
    identity_sha256_before: String,
    message_history_sha256_before: String,
}

pub fn begin_update_apply(
    app_config_dir: &Path,
    install_dir: &Path,
    previous_version: &str,
    applied_version: &str,
) -> Result<PendingUpdateApply, String> {
    if previous_version == applied_version {
        return Err("OSL update apply requires a changed version".to_owned());
    }
    fs::create_dir_all(app_config_dir)
        .map_err(|_| "OSL update apply state directory could not be created".to_owned())?;
    Ok(PendingUpdateApply {
        app_config_dir: app_config_dir.to_path_buf(),
        install_dir: install_dir.to_path_buf(),
        previous_version: previous_version.to_owned(),
        applied_version: applied_version.to_owned(),
        identity_sha256_before: identity_state_sha256(app_config_dir)?,
        message_history_sha256_before: message_history_sha256(app_config_dir)?,
    })
}

pub fn record_replaced_built_files(
    pending: PendingUpdateApply,
    built_files: Vec<String>,
) -> Result<UpdateApplyRecord, String> {
    let applied_at_unix_seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "OSL update apply clock is unavailable".to_owned())?
        .as_secs();
    record_replaced_built_files_at(pending, built_files, applied_at_unix_seconds)
}

pub fn record_replaced_built_files_at(
    pending: PendingUpdateApply,
    built_files: Vec<String>,
    applied_at_unix_seconds: u64,
) -> Result<UpdateApplyRecord, String> {
    if built_files.is_empty() {
        return Err("OSL update apply replaced no built files".to_owned());
    }
    let identity_after = identity_state_sha256(&pending.app_config_dir)?;
    let history_after = message_history_sha256(&pending.app_config_dir)?;
    if pending.identity_sha256_before != identity_after
        || pending.message_history_sha256_before != history_after
    {
        return Err("OSL update apply changed protected account state".to_owned());
    }

    let record = UpdateApplyRecord {
        schema_version: 1,
        previous_version: pending.previous_version,
        applied_version: pending.applied_version,
        status: UpdateApplyStatus::PendingRestart,
        applied_at_unix_seconds,
        finished_at_unix_seconds: None,
        successful_start_count: 0,
        install_dir: pending.install_dir.display().to_string(),
        staged_build_dir: String::new(),
        protected_state_dir: pending.app_config_dir.display().to_string(),
        built_files,
        identity_sha256_before: pending.identity_sha256_before,
        identity_sha256_after_apply: identity_after,
        message_history_sha256_before: pending.message_history_sha256_before,
        message_history_sha256_after_apply: history_after,
    };
    write_apply_record(&pending.app_config_dir, &record)?;
    Ok(record)
}

pub fn apply_staged_build_update(
    install_dir: &Path,
    staged_build_dir: &Path,
    app_config_dir: &Path,
    previous_version: &str,
    applied_version: &str,
) -> Result<UpdateApplyRecord, String> {
    let applied_at_unix_seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "OSL update apply clock is unavailable".to_owned())?
        .as_secs();
    apply_staged_build_update_at(
        install_dir,
        staged_build_dir,
        app_config_dir,
        previous_version,
        applied_version,
        applied_at_unix_seconds,
    )
}

pub fn apply_staged_build_update_at(
    install_dir: &Path,
    staged_build_dir: &Path,
    app_config_dir: &Path,
    previous_version: &str,
    applied_version: &str,
    applied_at_unix_seconds: u64,
) -> Result<UpdateApplyRecord, String> {
    if previous_version == applied_version {
        return Err("OSL update apply requires a changed version".to_owned());
    }
    if !staged_build_dir.is_dir() {
        return Err("OSL update apply staged build directory is unavailable".to_owned());
    }
    fs::create_dir_all(install_dir)
        .map_err(|_| "OSL update apply install directory could not be created".to_owned())?;
    let pending = begin_update_apply(
        app_config_dir,
        install_dir,
        previous_version,
        applied_version,
    )?;
    let staged_files = staged_built_files(staged_build_dir)?;
    if staged_files.is_empty() {
        return Err("OSL update apply staged build is empty".to_owned());
    }

    for relative in &staged_files {
        let source = staged_build_dir.join(relative);
        let destination = install_dir.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .map_err(|_| "OSL update apply build directory could not be created".to_owned())?;
        }
        fs::copy(&source, &destination)
            .map_err(|_| "OSL update apply built file could not be replaced".to_owned())?;
    }

    let mut record = record_replaced_built_files_at(
        pending,
        staged_files
            .iter()
            .map(|path| path.display().to_string())
            .collect(),
        applied_at_unix_seconds,
    )?;
    record.staged_build_dir = staged_build_dir.display().to_string();
    write_apply_record(app_config_dir, &record)?;
    Ok(record)
}

pub fn mark_update_finished_after_successful_start(
    app_config_dir: &Path,
    running_version: &str,
) -> Result<Option<UpdateApplyRecord>, String> {
    let started_at_unix_seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "OSL update apply clock is unavailable".to_owned())?
        .as_secs();
    mark_update_finished_after_successful_start_at(
        app_config_dir,
        running_version,
        started_at_unix_seconds,
    )
}

pub fn mark_update_finished_after_successful_start_at(
    app_config_dir: &Path,
    running_version: &str,
    started_at_unix_seconds: u64,
) -> Result<Option<UpdateApplyRecord>, String> {
    let path = app_config_dir.join(APPLY_RECORD_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let mut record = read_update_apply_record(&path)?;
    if record.status == UpdateApplyStatus::Finished {
        return Ok(Some(record));
    }
    if running_version != record.applied_version {
        return Ok(Some(record));
    }

    let identity_now = identity_state_sha256(app_config_dir)?;
    let history_now = message_history_sha256(app_config_dir)?;
    if identity_now != record.identity_sha256_before
        || history_now != record.message_history_sha256_before
    {
        return Err("OSL update apply cannot finish after account state changed".to_owned());
    }

    record.status = UpdateApplyStatus::Finished;
    record.finished_at_unix_seconds = Some(started_at_unix_seconds);
    record.successful_start_count = 1;
    write_apply_record(app_config_dir, &record)?;
    Ok(Some(record))
}

pub fn read_update_apply_record(path: &Path) -> Result<UpdateApplyRecord, String> {
    let bytes =
        fs::read(path).map_err(|_| "OSL update apply record could not be read".to_owned())?;
    serde_json::from_slice(&bytes).map_err(|_| "OSL update apply record is malformed".to_owned())
}

fn write_apply_record(app_config_dir: &Path, record: &UpdateApplyRecord) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(record)
        .map_err(|_| "OSL update apply record could not be encoded".to_owned())?;
    fs::write(app_config_dir.join(APPLY_RECORD_FILE), bytes)
        .map_err(|_| "OSL update apply record could not be written".to_owned())
}

fn staged_built_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_files(root: &Path, current: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(current)
        .map_err(|_| "OSL update apply staged build could not be listed".to_owned())?;
    for entry in entries {
        let entry =
            entry.map_err(|_| "OSL update apply staged build could not be listed".to_owned())?;
        let path = entry.path();
        let kind = entry
            .file_type()
            .map_err(|_| "OSL update apply staged build could not be inspected".to_owned())?;
        if kind.is_dir() {
            collect_files(root, &path, files)?;
        } else if kind.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| "OSL update apply staged build escaped root".to_owned())?;
            files.push(relative.to_path_buf());
        }
    }
    Ok(())
}

fn identity_state_sha256(app_config_dir: &Path) -> Result<String, String> {
    let core_dir = app_config_dir.join(HUB_CORE_DIR);
    let mut files = Vec::new();
    let flat = core_dir.join("identity.json");
    if flat.is_file() {
        files.push(flat);
    }
    let identities_dir = core_dir.join(IDENTITIES_DIR);
    if let Ok(entries) = fs::read_dir(identities_dir) {
        let mut slots = entries
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .map(|entry| entry.path().join("identity.json"))
            .filter(|path| path.is_file())
            .collect::<Vec<_>>();
        slots.sort();
        files.extend(slots);
    }
    digest_files(&core_dir, files, "identity")
}

fn message_history_sha256(app_config_dir: &Path) -> Result<String, String> {
    let core_dir = app_config_dir.join(HUB_CORE_DIR);
    let messages = core_dir.join("store").join(MESSAGE_STORE_DB);
    let files = if messages.is_file() {
        vec![messages]
    } else {
        Vec::new()
    };
    digest_files(&core_dir, files, "message history")
}

fn digest_files(root: &Path, files: Vec<PathBuf>, label: &str) -> Result<String, String> {
    let mut hasher = Sha256::new();
    for path in files {
        let relative = path
            .strip_prefix(root)
            .map_err(|_| format!("OSL update apply {label} escaped root"))?;
        hasher.update(relative.display().to_string().as_bytes());
        hasher.update([0]);
        let bytes = fs::read(&path)
            .map_err(|_| format!("OSL update apply could not read {label} state"))?;
        hasher.update(bytes);
        hasher.update([0xff]);
    }
    Ok(hex_sha256(hasher.finalize().as_slice()))
}

fn hex_sha256(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}
