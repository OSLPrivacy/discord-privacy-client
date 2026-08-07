//! Update application receipts.
//!
//! The signed updater owns signature verification and download. This module
//! owns the local handoff semantics OSL depends on: built files are overwritten,
//! account state stays in the config directory, and the update is not closed
//! until the replacement build has reached startup once.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::update_state_backup::{
    read_update_state_copy_record, restore_identity_and_friends_from_copy,
};

const APPLY_RECORD_FILE: &str = "update-apply-record.json";
    read_update_state_copy_record, restore_identity_friends_and_messages_from_copy,
};

const APPLY_RECORD_FILE: &str = "update-apply-record.json";
const RECOVERY_RECORD_FILE: &str = "update-recovery-record.json";
const HUB_CORE_DIR: &str = "osl-core";
const IDENTITIES_DIR: &str = "hub-identities";
const MESSAGE_STORE_DB: &str = "messages.sqlite";
const OLD_BUILD_BACKUPS_DIR: &str = "update-old-build-backups";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateApplyStatus {
    PendingRestart,
    Finished,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UpdateApplyRecord {
    pub schema_version: u32,
    pub previous_version: String,
    pub applied_version: String,
    pub status: UpdateApplyStatus,
    pub applied_at_unix_seconds: u64,
    pub finished_at_unix_seconds: Option<u64>,
    pub failed_at_unix_seconds: Option<u64>,
    pub failure_reason: Option<String>,
    pub successful_start_count: u32,
    pub install_dir: String,
    pub staged_build_dir: String,
    pub old_build_dir: String,
    pub state_copy_record_path: String,
    pub protected_state_dir: String,
    pub built_files: Vec<String>,
    pub identity_sha256_before: String,
    pub identity_sha256_after_apply: String,
    pub message_history_sha256_before: String,
    pub message_history_sha256_after_apply: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterruptedUpdateRecoveryStatus {
    NoInterruptedUpdate,
    RestoredOldBeforeStart,
    CompleteNewPendingRestart,
    FailedCannotStart,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InterruptedUpdateRecoveryReport {
    pub schema_version: u32,
    pub status: InterruptedUpdateRecoveryStatus,
    pub complete_version: Option<String>,
    pub stopped_step_count: usize,
    pub mixed_version_run: bool,
    pub failed_reason: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InterruptedUpdateStepRecord {
    pub step: String,
    pub detail: String,
    pub recorded_at_unix_seconds: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct UpdateRecoveryRecord {
    schema_version: u32,
    previous_version: String,
    applied_version: String,
    reserved_at_unix_seconds: u64,
    install_dir: String,
    staged_build_dir: String,
    old_build_dir: String,
    state_copy_record_path: String,
    protected_state_dir: String,
    built_files: Vec<String>,
    old_build_sha256: BTreeMap<String, String>,
    desired_build_sha256: BTreeMap<String, String>,
    identity_sha256_before: String,
    message_history_sha256_before: String,
    stopped_steps: Vec<InterruptedUpdateStepRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingUpdateApply {
    app_config_dir: PathBuf,
    install_dir: PathBuf,
    previous_version: String,
    applied_version: String,
    identity_sha256_before: String,
    message_history_sha256_before: String,
    old_build_dir: PathBuf,
    state_copy_record_path: Option<PathBuf>,
}

pub fn begin_update_apply(
    app_config_dir: &Path,
    install_dir: &Path,
    previous_version: &str,
    applied_version: &str,
) -> Result<PendingUpdateApply, String> {
    begin_update_apply_with_old_files_at(
        app_config_dir,
        install_dir,
        previous_version,
        applied_version,
        Vec::new(),
        None,
        current_unix_seconds()?,
    )
}

pub fn begin_update_apply_with_old_files(
    app_config_dir: &Path,
    install_dir: &Path,
    previous_version: &str,
    applied_version: &str,
    old_built_files: Vec<String>,
    state_copy_record_path: Option<&Path>,
) -> Result<PendingUpdateApply, String> {
    begin_update_apply_with_old_files_at(
        app_config_dir,
        install_dir,
        previous_version,
        applied_version,
        old_built_files,
        state_copy_record_path,
        current_unix_seconds()?,
    )
}

pub fn begin_update_apply_with_old_files_at(
    app_config_dir: &Path,
    install_dir: &Path,
    previous_version: &str,
    applied_version: &str,
    old_built_files: Vec<String>,
    state_copy_record_path: Option<&Path>,
    reserved_at_unix_seconds: u64,
) -> Result<PendingUpdateApply, String> {
    if previous_version == applied_version {
        return Err("OSL update apply requires a changed version".to_owned());
    }
    fs::create_dir_all(app_config_dir)
        .map_err(|_| "OSL update apply state directory could not be created".to_owned())?;
    let old_build_dir = if old_built_files.is_empty() {
        PathBuf::new()
    } else {
        let old_build_dir =
            old_build_backup_dir(app_config_dir, applied_version, reserved_at_unix_seconds)?;
        fs::create_dir_all(&old_build_dir).map_err(|_| {
            "OSL update rollback old build directory could not be created".to_owned()
        })?;
        for relative in &old_built_files {
            backup_old_built_file(install_dir, &old_build_dir, relative)?;
        }
        old_build_dir
    };
    Ok(PendingUpdateApply {
    let pending = PendingUpdateApply {
        app_config_dir: app_config_dir.to_path_buf(),
        install_dir: install_dir.to_path_buf(),
        previous_version: previous_version.to_owned(),
        applied_version: applied_version.to_owned(),
        identity_sha256_before: identity_state_sha256(app_config_dir)?,
        message_history_sha256_before: message_history_sha256(app_config_dir)?,
        old_build_dir,
        state_copy_record_path: state_copy_record_path.map(Path::to_path_buf),
    })
    };
    write_recovery_record(&pending, old_built_files, reserved_at_unix_seconds)?;
    Ok(pending)
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
        failed_at_unix_seconds: None,
        failure_reason: None,
        successful_start_count: 0,
        install_dir: pending.install_dir.display().to_string(),
        staged_build_dir: String::new(),
        old_build_dir: pending.old_build_dir.display().to_string(),
        state_copy_record_path: pending
            .state_copy_record_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default(),
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

pub fn record_interrupted_update_data_step_at(
    app_config_dir: &Path,
    detail: &str,
    recorded_at_unix_seconds: u64,
) -> Result<(), String> {
    append_recovery_step(
        app_config_dir,
        "data_changed",
        detail,
        recorded_at_unix_seconds,
    )
}

pub fn record_interrupted_update_built_file_replaced_at(
    app_config_dir: &Path,
    relative_built_file: &str,
    recorded_at_unix_seconds: u64,
) -> Result<(), String> {
    let _ = validated_relative_built_file(relative_built_file)?;
    append_recovery_step(
        app_config_dir,
        "built_file_replaced",
        relative_built_file,
        recorded_at_unix_seconds,
    )
}

pub fn recover_interrupted_update_before_start_at(
    app_config_dir: &Path,
    recovered_at_unix_seconds: u64,
) -> Result<Option<InterruptedUpdateRecoveryReport>, String> {
    let recovery_path = app_config_dir.join(RECOVERY_RECORD_FILE);
    if !recovery_path.exists() {
        return Ok(None);
    }
    let recovery = read_recovery_record(&recovery_path)?;
    if app_config_dir.join(APPLY_RECORD_FILE).exists() {
        let classification = classify_installed_version(&recovery)?;
        if classification.as_deref() == Some(recovery.applied_version.as_str()) {
            return Ok(Some(InterruptedUpdateRecoveryReport {
                schema_version: 1,
                status: InterruptedUpdateRecoveryStatus::CompleteNewPendingRestart,
                complete_version: classification,
                stopped_step_count: recovery.stopped_steps.len(),
                mixed_version_run: false,
                failed_reason: None,
            }));
        }
    }

    match restore_old_from_recovery(&recovery) {
        Ok(()) => {
            if !recovery.state_copy_record_path.is_empty() {
                let state_record =
                    read_update_state_copy_record(Path::new(&recovery.state_copy_record_path))?;
                restore_identity_friends_and_messages_from_copy(app_config_dir, &state_record)?;
            }
            let classification = classify_installed_version(&recovery)?;
            if classification.as_deref() != Some(recovery.previous_version.as_str()) {
                let reason = "interrupted update recovery could not produce a complete old build";
                let report = fail_interrupted_recovery(
                    app_config_dir,
                    &recovery,
                    reason,
                    recovered_at_unix_seconds,
                )?;
                return Ok(Some(report));
            }
            remove_recovery_record(app_config_dir)?;
            Ok(Some(InterruptedUpdateRecoveryReport {
                schema_version: 1,
                status: InterruptedUpdateRecoveryStatus::RestoredOldBeforeStart,
                complete_version: Some(recovery.previous_version),
                stopped_step_count: recovery.stopped_steps.len(),
                mixed_version_run: false,
                failed_reason: None,
            }))
        }
        Err(error) => {
            let report = fail_interrupted_recovery(
                app_config_dir,
                &recovery,
                &error,
                recovered_at_unix_seconds,
            )?;
            Ok(Some(report))
        }
    }
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
    let staged_files = staged_built_files(staged_build_dir)?;
    if staged_files.is_empty() {
        return Err("OSL update apply staged build is empty".to_owned());
    }
    fs::create_dir_all(install_dir)
        .map_err(|_| "OSL update apply install directory could not be created".to_owned())?;
    let pending = begin_update_apply_with_old_files_at(
        app_config_dir,
        install_dir,
        previous_version,
        applied_version,
        staged_files
            .iter()
            .map(|path| path.display().to_string())
            .collect(),
        None,
        applied_at_unix_seconds,
    )?;
    record_recovery_desired_build(app_config_dir, staged_build_dir)?;

    for relative in &staged_files {
        let source = staged_build_dir.join(relative);
        let destination = install_dir.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .map_err(|_| "OSL update apply build directory could not be created".to_owned())?;
        }
        fs::copy(&source, &destination)
            .map_err(|_| "OSL update apply built file could not be replaced".to_owned())?;
        append_recovery_step(
            app_config_dir,
            "built_file_replaced",
            &relative.display().to_string(),
            applied_at_unix_seconds,
        )?;
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
    append_recovery_step(
        app_config_dir,
        "apply_record_written",
        APPLY_RECORD_FILE,
        applied_at_unix_seconds,
    )?;
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
    if record.status == UpdateApplyStatus::Failed {
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
    remove_recovery_record(app_config_dir)?;
    Ok(Some(record))
}

pub fn rollback_failed_update_after_start_failure(
    app_config_dir: &Path,
    failure_reason: &str,
) -> Result<Option<UpdateApplyRecord>, String> {
    rollback_failed_update_after_start_failure_at(
        app_config_dir,
        failure_reason,
        current_unix_seconds()?,
    )
}

pub fn rollback_failed_update_after_start_failure_at(
    app_config_dir: &Path,
    failure_reason: &str,
    failed_at_unix_seconds: u64,
) -> Result<Option<UpdateApplyRecord>, String> {
    let reason = failure_reason.trim();
    if reason.is_empty() {
        return Err("OSL update rollback failure reason is required".to_owned());
    }
    let path = app_config_dir.join(APPLY_RECORD_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let mut record = read_update_apply_record(&path)?;
    if record.status != UpdateApplyStatus::PendingRestart {
        return Ok(Some(record));
    }

    restore_old_built_files(&record)?;
    let state_record_path = Path::new(&record.state_copy_record_path);
    if state_record_path.as_os_str().is_empty() {
        return Err("OSL update rollback state copy record is missing".to_owned());
    }
    let state_record = read_update_state_copy_record(state_record_path)?;
    restore_identity_and_friends_from_copy(app_config_dir, &state_record)?;
    restore_identity_friends_and_messages_from_copy(app_config_dir, &state_record)?;

    record.status = UpdateApplyStatus::Failed;
    record.failed_at_unix_seconds = Some(failed_at_unix_seconds);
    record.failure_reason = Some(reason.to_owned());
    write_apply_record(app_config_dir, &record)?;
    Ok(Some(record))
}

pub fn read_update_apply_record(path: &Path) -> Result<UpdateApplyRecord, String> {
    let bytes =
        fs::read(path).map_err(|_| "OSL update apply record could not be read".to_owned())?;
    serde_json::from_slice(&bytes).map_err(|_| "OSL update apply record is malformed".to_owned())
}

fn current_unix_seconds() -> Result<u64, String> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "OSL update apply clock is unavailable".to_owned())
        .map(|duration| duration.as_secs())
}

fn write_recovery_record(
    pending: &PendingUpdateApply,
    built_files: Vec<String>,
    reserved_at_unix_seconds: u64,
) -> Result<(), String> {
    if built_files.is_empty() {
        return Ok(());
    }
    let mut old_build_sha256 = BTreeMap::new();
    for relative in &built_files {
        let relative_path = validated_relative_built_file(relative)?;
        old_build_sha256.insert(
            relative.clone(),
            GetPathSha256::sha256(&pending.old_build_dir.join(relative_path))?,
        );
    }
    let mut stopped_steps = vec![InterruptedUpdateStepRecord {
        step: "old_build_backed_up".to_owned(),
        detail: built_files.join(","),
        recorded_at_unix_seconds: reserved_at_unix_seconds,
    }];
    if let Some(path) = &pending.state_copy_record_path {
        stopped_steps.push(InterruptedUpdateStepRecord {
            step: "state_copy_recorded".to_owned(),
            detail: path.display().to_string(),
            recorded_at_unix_seconds: reserved_at_unix_seconds,
        });
    }
    let record = UpdateRecoveryRecord {
        schema_version: 1,
        previous_version: pending.previous_version.clone(),
        applied_version: pending.applied_version.clone(),
        reserved_at_unix_seconds,
        install_dir: pending.install_dir.display().to_string(),
        staged_build_dir: String::new(),
        old_build_dir: pending.old_build_dir.display().to_string(),
        state_copy_record_path: pending
            .state_copy_record_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default(),
        protected_state_dir: pending.app_config_dir.display().to_string(),
        built_files,
        old_build_sha256,
        desired_build_sha256: BTreeMap::new(),
        identity_sha256_before: pending.identity_sha256_before.clone(),
        message_history_sha256_before: pending.message_history_sha256_before.clone(),
        stopped_steps,
    };
    write_recovery_record_file(&pending.app_config_dir, &record)
}

fn record_recovery_desired_build(
    app_config_dir: &Path,
    staged_build_dir: &Path,
) -> Result<(), String> {
    let path = app_config_dir.join(RECOVERY_RECORD_FILE);
    if !path.exists() {
        return Ok(());
    }
    let mut record = read_recovery_record(&path)?;
    record.staged_build_dir = staged_build_dir.display().to_string();
    let mut desired = BTreeMap::new();
    for relative in &record.built_files {
        let relative_path = validated_relative_built_file(relative)?;
        desired.insert(
            relative.clone(),
            GetPathSha256::sha256(&staged_build_dir.join(relative_path))?,
        );
    }
    record.desired_build_sha256 = desired;
    write_recovery_record_file(app_config_dir, &record)
}

fn append_recovery_step(
    app_config_dir: &Path,
    step: &str,
    detail: &str,
    recorded_at_unix_seconds: u64,
) -> Result<(), String> {
    let path = app_config_dir.join(RECOVERY_RECORD_FILE);
    if !path.exists() {
        return Ok(());
    }
    let mut record = read_recovery_record(&path)?;
    record.stopped_steps.push(InterruptedUpdateStepRecord {
        step: step.to_owned(),
        detail: detail.to_owned(),
        recorded_at_unix_seconds,
    });
    write_recovery_record_file(app_config_dir, &record)
}

fn read_recovery_record(path: &Path) -> Result<UpdateRecoveryRecord, String> {
    let bytes =
        fs::read(path).map_err(|_| "OSL update recovery record could not be read".to_owned())?;
    let record: UpdateRecoveryRecord = serde_json::from_slice(&bytes)
        .map_err(|_| "OSL update recovery record is malformed".to_owned())?;
    if record.schema_version != 1 || record.built_files.is_empty() {
        return Err("OSL update recovery record is invalid".to_owned());
    }
    Ok(record)
}

fn write_recovery_record_file(
    app_config_dir: &Path,
    record: &UpdateRecoveryRecord,
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(record)
        .map_err(|_| "OSL update recovery record could not be encoded".to_owned())?;
    fs::write(app_config_dir.join(RECOVERY_RECORD_FILE), bytes)
        .map_err(|_| "OSL update recovery record could not be written".to_owned())
}

fn remove_recovery_record(app_config_dir: &Path) -> Result<(), String> {
    match fs::remove_file(app_config_dir.join(RECOVERY_RECORD_FILE)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("OSL update recovery record could not be removed".to_owned()),
    }
}

fn restore_old_from_recovery(record: &UpdateRecoveryRecord) -> Result<(), String> {
    let install_dir = Path::new(&record.install_dir);
    let old_build_dir = Path::new(&record.old_build_dir);
    for relative in &record.built_files {
        let relative_path = validated_relative_built_file(relative)?;
        let source = old_build_dir.join(&relative_path);
        if !source.is_file() {
            return Err("OSL update recovery old build file is missing".to_owned());
        }
        let destination = install_dir.join(&relative_path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .map_err(|_| "OSL update recovery install directory failed".to_owned())?;
        }
        fs::copy(source, destination)
            .map_err(|_| "OSL update recovery old build restore failed".to_owned())?;
    }
    Ok(())
}

fn classify_installed_version(record: &UpdateRecoveryRecord) -> Result<Option<String>, String> {
    let install_dir = Path::new(&record.install_dir);
    let mut old_count = 0usize;
    let mut new_count = 0usize;
    for relative in &record.built_files {
        let relative_path = validated_relative_built_file(relative)?;
        let current = GetPathSha256::sha256(&install_dir.join(relative_path))?;
        if record.old_build_sha256.get(relative) == Some(&current) {
            old_count += 1;
        }
        if record.desired_build_sha256.get(relative) == Some(&current) {
            new_count += 1;
        }
    }
    if old_count == record.built_files.len() {
        return Ok(Some(record.previous_version.clone()));
    }
    if record.desired_build_sha256.len() == record.built_files.len()
        && new_count == record.built_files.len()
    {
        return Ok(Some(record.applied_version.clone()));
    }
    Ok(None)
}

fn fail_interrupted_recovery(
    app_config_dir: &Path,
    recovery: &UpdateRecoveryRecord,
    reason: &str,
    failed_at_unix_seconds: u64,
) -> Result<InterruptedUpdateRecoveryReport, String> {
    let record = UpdateApplyRecord {
        schema_version: 1,
        previous_version: recovery.previous_version.clone(),
        applied_version: recovery.applied_version.clone(),
        status: UpdateApplyStatus::Failed,
        applied_at_unix_seconds: recovery.reserved_at_unix_seconds,
        finished_at_unix_seconds: None,
        failed_at_unix_seconds: Some(failed_at_unix_seconds),
        failure_reason: Some(reason.to_owned()),
        successful_start_count: 0,
        install_dir: recovery.install_dir.clone(),
        staged_build_dir: recovery.staged_build_dir.clone(),
        old_build_dir: recovery.old_build_dir.clone(),
        state_copy_record_path: recovery.state_copy_record_path.clone(),
        protected_state_dir: recovery.protected_state_dir.clone(),
        built_files: recovery.built_files.clone(),
        identity_sha256_before: recovery.identity_sha256_before.clone(),
        identity_sha256_after_apply: identity_state_sha256(app_config_dir)
            .unwrap_or_else(|_| recovery.identity_sha256_before.clone()),
        message_history_sha256_before: recovery.message_history_sha256_before.clone(),
        message_history_sha256_after_apply: message_history_sha256(app_config_dir)
            .unwrap_or_else(|_| recovery.message_history_sha256_before.clone()),
    };
    write_apply_record(app_config_dir, &record)?;
    Ok(InterruptedUpdateRecoveryReport {
        schema_version: 1,
        status: InterruptedUpdateRecoveryStatus::FailedCannotStart,
        complete_version: None,
        stopped_step_count: recovery.stopped_steps.len(),
        mixed_version_run: false,
        failed_reason: Some(reason.to_owned()),
    })
}

struct GetPathSha256;

impl GetPathSha256 {
    fn sha256(path: &Path) -> Result<String, String> {
        fs::read(path)
            .map_err(|_| "OSL update recovery could not read built file".to_owned())
            .map(|bytes| {
                let digest = Sha256::digest(bytes);
                hex_sha256(digest.as_slice())
            })
    }
}

fn old_build_backup_dir(
    app_config_dir: &Path,
    applied_version: &str,
    reserved_at_unix_seconds: u64,
) -> Result<PathBuf, String> {
    let suffix = sanitize_backup_component(applied_version);
    for attempt in 0..100u32 {
        let candidate = app_config_dir.join(OLD_BUILD_BACKUPS_DIR).join(format!(
            "pre-update-{reserved_at_unix_seconds}-{suffix}-{attempt}"
        ));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err("OSL update rollback old build directory could not be reserved".to_owned())
}

fn backup_old_built_file(
    install_dir: &Path,
    old_build_dir: &Path,
    relative: &str,
) -> Result<(), String> {
    let relative = validated_relative_built_file(relative)?;
    let source = install_dir.join(&relative);
    if !source.is_file() {
        return Err("OSL update rollback old build file is missing".to_owned());
    }
    let destination = old_build_dir.join(&relative);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|_| {
            "OSL update rollback old build directory could not be created".to_owned()
        })?;
    }
    fs::copy(source, destination)
        .map_err(|_| "OSL update rollback old build file could not be copied".to_owned())?;
    Ok(())
}

fn restore_old_built_files(record: &UpdateApplyRecord) -> Result<(), String> {
    if record.old_build_dir.is_empty() || record.built_files.is_empty() {
        return Err("OSL update rollback old build copy is missing".to_owned());
    }
    let install_dir = Path::new(&record.install_dir);
    let old_build_dir = Path::new(&record.old_build_dir);
    for relative in &record.built_files {
        let relative = validated_relative_built_file(relative)?;
        let source = old_build_dir.join(&relative);
        if !source.is_file() {
            return Err("OSL update rollback old build file is missing".to_owned());
        }
        let destination = install_dir.join(&relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|_| {
                "OSL update rollback build directory could not be created".to_owned()
            })?;
        }
        fs::copy(source, destination)
            .map_err(|_| "OSL update rollback built file could not be restored".to_owned())?;
    }
    Ok(())
}

fn validated_relative_built_file(relative: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(relative);
    if relative.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err("OSL update apply built file path is invalid".to_owned());
    }
    Ok(path)
}

fn sanitize_backup_component(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '.' || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect()
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
