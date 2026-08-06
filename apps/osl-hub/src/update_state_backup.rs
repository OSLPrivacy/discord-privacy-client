//! Pre-update account-state snapshot.
//!
//! The signed updater may replace application files. Before that boundary runs,
//! OSL keeps a same-host copy of the local account state whose row counts can be
//! checked without reading message plaintext.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

const HUB_CORE_DIR: &str = "osl-core";
const IDENTITIES_DIR: &str = "hub-identities";
const PEOPLE_FILE: &str = "hub_people.json";
const ALLOWED_PLACES_DB: &str = "allowed_places.sqlite";
const MESSAGE_STORE_DB: &str = "messages.sqlite";
const UPDATE_BACKUPS_DIR: &str = "update-state-backups";
const RECORD_FILE: &str = "update-state-copy-record.json";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UpdateStateCopyCounts {
    pub identity_count: usize,
    pub friend_count: usize,
    pub allowed_place_count: usize,
    pub message_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UpdateStateCopyRecord {
    pub schema_version: u32,
    pub update_version: String,
    pub copied_at_unix_seconds: u64,
    pub backup_dir: String,
    pub live_counts: UpdateStateCopyCounts,
    pub copied_counts: UpdateStateCopyCounts,
    pub copied_paths: BTreeMap<String, Vec<String>>,
}

pub fn copy_identity_and_history_before_update(
    app_config_dir: &Path,
    update_version: &str,
) -> Result<UpdateStateCopyRecord, String> {
    let copied_at_unix_seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "OSL update state backup clock is unavailable".to_owned())?
        .as_secs();
    let backup_dir = unique_backup_dir(app_config_dir, update_version, copied_at_unix_seconds)?;
    copy_identity_and_history_before_update_to(
        app_config_dir,
        &backup_dir,
        update_version,
        copied_at_unix_seconds,
    )
}

#[cfg(test)]
pub fn copy_identity_and_history_before_update_to(
    app_config_dir: &Path,
    backup_dir: &Path,
    update_version: &str,
    copied_at_unix_seconds: u64,
) -> Result<UpdateStateCopyRecord, String> {
    copy_state_to_backup(
        app_config_dir,
        backup_dir,
        update_version,
        copied_at_unix_seconds,
    )
}

#[cfg(not(test))]
fn copy_identity_and_history_before_update_to(
    app_config_dir: &Path,
    backup_dir: &Path,
    update_version: &str,
    copied_at_unix_seconds: u64,
) -> Result<UpdateStateCopyRecord, String> {
    copy_state_to_backup(
        app_config_dir,
        backup_dir,
        update_version,
        copied_at_unix_seconds,
    )
}

fn copy_state_to_backup(
    app_config_dir: &Path,
    backup_dir: &Path,
    update_version: &str,
    copied_at_unix_seconds: u64,
) -> Result<UpdateStateCopyRecord, String> {
    if backup_dir.exists() {
        return Err("OSL update state backup destination already exists".to_owned());
    }
    fs::create_dir_all(backup_dir)
        .map_err(|_| "OSL update state backup directory could not be created".to_owned())?;

    let core_dir = app_config_dir.join(HUB_CORE_DIR);
    let backup_core_dir = backup_dir.join(HUB_CORE_DIR);
    fs::create_dir_all(&backup_core_dir)
        .map_err(|_| "OSL update state backup core directory could not be created".to_owned())?;

    let live_counts = counts_for_core_dir(&core_dir)?;
    let mut copied_paths: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let identity_paths = identity_paths(&core_dir);
    for path in &identity_paths {
        copy_relative_file(
            &core_dir,
            path,
            &backup_core_dir,
            "identity",
            &mut copied_paths,
        )?;
    }

    let people_path = core_dir.join(PEOPLE_FILE);
    if people_path.exists() {
        copy_relative_file(
            &core_dir,
            &people_path,
            &backup_core_dir,
            "friends",
            &mut copied_paths,
        )?;
    }

    let allowed_places_path = core_dir.join(ALLOWED_PLACES_DB);
    if allowed_places_path.exists() {
        copy_sqlite_database(
            &allowed_places_path,
            &backup_core_dir.join(ALLOWED_PLACES_DB),
            "allowed places",
            &mut copied_paths,
        )?;
    }

    let messages_path = core_dir.join("store").join(MESSAGE_STORE_DB);
    if messages_path.exists() {
        copy_sqlite_database(
            &messages_path,
            &backup_core_dir.join("store").join(MESSAGE_STORE_DB),
            "message history",
            &mut copied_paths,
        )?;
    }

    let copied_counts = counts_for_core_dir(&backup_core_dir)?;
    if live_counts != copied_counts {
        let _ = fs::remove_dir_all(backup_dir);
        return Err("OSL update state backup count verification failed".to_owned());
    }

    let record = UpdateStateCopyRecord {
        schema_version: 1,
        update_version: update_version.to_owned(),
        copied_at_unix_seconds,
        backup_dir: backup_dir.display().to_string(),
        live_counts,
        copied_counts,
        copied_paths,
    };
    let record_bytes = serde_json::to_vec_pretty(&record)
        .map_err(|_| "OSL update state backup record could not be encoded".to_owned())?;
    fs::write(backup_dir.join(RECORD_FILE), record_bytes)
        .map_err(|_| "OSL update state backup record could not be written".to_owned())?;
    Ok(record)
}

pub fn read_update_state_copy_record(path: &Path) -> Result<UpdateStateCopyRecord, String> {
    let bytes = fs::read(path)
        .map_err(|_| "OSL update state backup record could not be read".to_owned())?;
    serde_json::from_slice(&bytes)
        .map_err(|_| "OSL update state backup record is malformed".to_owned())
}

pub fn update_state_copy_record_path(backup_dir: &Path) -> PathBuf {
    backup_dir.join(RECORD_FILE)
}

pub fn identity_and_friend_counts_for_config(
    app_config_dir: &Path,
) -> Result<(usize, usize), String> {
    let counts = counts_for_core_dir(&app_config_dir.join(HUB_CORE_DIR))?;
    Ok((counts.identity_count, counts.friend_count))
}

pub fn restore_identity_and_friends_from_copy(
    app_config_dir: &Path,
    record: &UpdateStateCopyRecord,
) -> Result<UpdateStateCopyCounts, String> {
    restore_state_from_copy(app_config_dir, record, false)
}

pub fn restore_identity_friends_and_messages_from_copy(
    app_config_dir: &Path,
    record: &UpdateStateCopyRecord,
) -> Result<UpdateStateCopyCounts, String> {
    restore_state_from_copy(app_config_dir, record, true)
}

fn restore_state_from_copy(
    app_config_dir: &Path,
    record: &UpdateStateCopyRecord,
    include_message_history: bool,
) -> Result<UpdateStateCopyCounts, String> {
    let live_core_dir = app_config_dir.join(HUB_CORE_DIR);
    let copied_core_dir = Path::new(&record.backup_dir).join(HUB_CORE_DIR);
    if !copied_core_dir.is_dir() {
        return Err("OSL update state backup copy is unavailable".to_owned());
    }
    fs::create_dir_all(&live_core_dir)
        .map_err(|_| "OSL update state restore core directory could not be created".to_owned())?;

    replace_optional_file(
        &copied_core_dir.join("identity.json"),
        &live_core_dir.join("identity.json"),
        "identity",
    )?;
    replace_optional_dir(
        &copied_core_dir.join(IDENTITIES_DIR),
        &live_core_dir.join(IDENTITIES_DIR),
        "identity",
    )?;
    replace_optional_file(
        &copied_core_dir.join(PEOPLE_FILE),
        &live_core_dir.join(PEOPLE_FILE),
        "friends",
    )?;
    if include_message_history {
        replace_optional_file(
            &copied_core_dir.join(ALLOWED_PLACES_DB),
            &live_core_dir.join(ALLOWED_PLACES_DB),
            "allowed places",
        )?;
        replace_optional_file(
            &copied_core_dir.join("store").join(MESSAGE_STORE_DB),
            &live_core_dir.join("store").join(MESSAGE_STORE_DB),
            "message history",
        )?;
    }

    let restored = counts_for_core_dir(&live_core_dir)?;
    if restored.identity_count != record.copied_counts.identity_count
        || restored.friend_count != record.copied_counts.friend_count
        || (include_message_history && restored.message_count != record.copied_counts.message_count)
    {
        return Err("OSL update state restore count verification failed".to_owned());
    }
    Ok(restored)
}

fn counts_for_core_dir(core_dir: &Path) -> Result<UpdateStateCopyCounts, String> {
    Ok(UpdateStateCopyCounts {
        identity_count: identity_paths(core_dir).len(),
        friend_count: friend_count(&core_dir.join(PEOPLE_FILE))?,
        allowed_place_count: sqlite_count(&core_dir.join(ALLOWED_PLACES_DB), "allowed_places")?,
        message_count: sqlite_count(&core_dir.join("store").join(MESSAGE_STORE_DB), "messages")?,
    })
}

fn friend_count(path: &Path) -> Result<usize, String> {
    if !path.exists() {
        return Ok(0);
    }
    let mut bytes =
        fs::read(path).map_err(|_| "OSL update state backup could not read friends".to_owned())?;
    if ipc::main_password::has_enc_magic(&bytes) {
        let key = ipc::main_password::get_file_storage_key()
            .ok_or_else(|| "Unlock OSL before installing an update".to_owned())?;
        bytes = ipc::main_password::decrypt_at_rest(&bytes, &key)
            .map_err(|_| "OSL update state backup could not open friends".to_owned())?;
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| "OSL update state backup friends file is malformed".to_owned())?;
    let Some(people) = value.get("people").and_then(serde_json::Value::as_object) else {
        return Err("OSL update state backup friends file is malformed".to_owned());
    };
    Ok(people.len())
}

fn sqlite_count(path: &Path, table: &str) -> Result<usize, String> {
    if !path.exists() {
        return Ok(0);
    }
    let conn = Connection::open(path)
        .map_err(|_| "OSL update state backup could not open copied database".to_owned())?;
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |row| row.get(0),
        )
        .map_err(|_| "OSL update state backup could not inspect copied database".to_owned())?;
    if exists == 0 {
        return Ok(0);
    }
    let sql = format!("SELECT COUNT(*) FROM {table}");
    let count: i64 = conn
        .query_row(&sql, [], |row| row.get(0))
        .map_err(|_| "OSL update state backup could not count copied database".to_owned())?;
    usize::try_from(count).map_err(|_| "OSL update state backup count is invalid".to_owned())
}

fn copy_relative_file(
    root: &Path,
    source: &Path,
    destination_root: &Path,
    label: &str,
    copied_paths: &mut BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    let relative = source
        .strip_prefix(root)
        .map_err(|_| "OSL update state backup source escaped root".to_owned())?;
    let destination = destination_root.join(relative);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|_| "OSL update state backup destination could not be created".to_owned())?;
    }
    fs::copy(source, &destination)
        .map_err(|_| "OSL update state backup file could not be copied".to_owned())?;
    copied_paths
        .entry(label.to_owned())
        .or_default()
        .push(relative.display().to_string());
    Ok(())
}

fn copy_sqlite_database(
    source: &Path,
    destination: &Path,
    label: &str,
    copied_paths: &mut BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|_| {
            "OSL update state backup database directory could not be created".to_owned()
        })?;
    }
    let conn = Connection::open(source)
        .map_err(|_| "OSL update state backup could not open source database".to_owned())?;
    let destination_text = destination.to_string_lossy().into_owned();
    conn.execute("VACUUM main INTO ?1", params![destination_text])
        .map_err(|_| "OSL update state backup database could not be copied".to_owned())?;
    copied_paths.entry(label.to_owned()).or_default().push(
        destination
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
    );
    Ok(())
}

fn replace_optional_file(source: &Path, destination: &Path, label: &str) -> Result<(), String> {
    if source.is_file() {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .map_err(|_| format!("OSL update state restore {label} directory failed"))?;
        }
        fs::copy(source, destination)
            .map_err(|_| format!("OSL update state restore {label} file failed"))?;
    } else {
        remove_file_if_present(destination, label)?;
    }
    Ok(())
}

fn replace_optional_dir(source: &Path, destination: &Path, label: &str) -> Result<(), String> {
    if destination.exists() {
        fs::remove_dir_all(destination)
            .map_err(|_| format!("OSL update state restore {label} directory failed"))?;
    }
    if source.is_dir() {
        copy_dir(source, destination, label)?;
    }
    Ok(())
}

fn copy_dir(source: &Path, destination: &Path, label: &str) -> Result<(), String> {
    fs::create_dir_all(destination)
        .map_err(|_| format!("OSL update state restore {label} directory failed"))?;
    for entry in fs::read_dir(source)
        .map_err(|_| format!("OSL update state restore {label} directory failed"))?
    {
        let entry =
            entry.map_err(|_| format!("OSL update state restore {label} directory failed"))?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry
            .file_type()
            .map_err(|_| format!("OSL update state restore {label} directory failed"))?;
        if file_type.is_dir() {
            copy_dir(&source_path, &destination_path, label)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &destination_path)
                .map_err(|_| format!("OSL update state restore {label} file failed"))?;
        }
    }
    Ok(())
}

fn remove_file_if_present(path: &Path, label: &str) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(format!("OSL update state restore {label} file failed")),
    }
}

fn identity_paths(core_dir: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let flat = core_dir.join("identity.json");
    if flat.is_file() {
        paths.push(flat);
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
        paths.extend(slots);
    }
    paths
}

fn unique_backup_dir(
    app_config_dir: &Path,
    update_version: &str,
    copied_at_unix_seconds: u64,
) -> Result<PathBuf, String> {
    let suffix = update_version
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '.' || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    for attempt in 0..100u32 {
        let candidate = app_config_dir.join(UPDATE_BACKUPS_DIR).join(format!(
            "pre-update-{copied_at_unix_seconds}-{suffix}-{attempt}"
        ));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err("OSL update state backup destination could not be reserved".to_owned())
}
