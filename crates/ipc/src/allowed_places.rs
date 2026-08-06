use serde::{Deserialize, Serialize};
use std::path::Path;

const ALLOWED_PLACES_FILE: &str = "allowed_places.json";

#[derive(Debug, thiserror::Error)]
pub enum AllowedPlaceStoreError {
    #[error("allowed-place record is invalid: {0}")]
    InvalidRecord(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllowedPlaceRecord {
    pub app: String,
    pub account: String,
    pub kind: String,
    pub stable_id: String,
    #[serde(default)]
    pub place_name: String,
    #[serde(default)]
    pub person_name: String,
}

impl AllowedPlaceRecord {
    pub fn discord_direct_message(account: impl Into<String>, peer: impl Into<String>) -> Self {
        let account = account.into();
        let peer = peer.into();
        Self {
            app: "discord".to_owned(),
            account: account.clone(),
            kind: "direct_message".to_owned(),
            stable_id: format!("discord:{account}:direct_message:{peer}"),
            place_name: format!("Direct message with {peer}"),
            person_name: peer,
        }
    }

    pub fn validate(&self) -> Result<(), AllowedPlaceStoreError> {
        for (field, value) in [
            ("app", self.app.as_str()),
            ("account", self.account.as_str()),
            ("kind", self.kind.as_str()),
            ("stable_id", self.stable_id.as_str()),
            ("place_name", self.place_name.as_str()),
            ("person_name", self.person_name.as_str()),
        ] {
            if matches!(field, "app" | "account" | "kind" | "stable_id") && value.is_empty() {
                return Err(AllowedPlaceStoreError::InvalidRecord(format!(
                    "{field} may not be empty"
                )));
            }
            if value.contains('\0') {
                return Err(AllowedPlaceStoreError::InvalidRecord(format!(
                    "{field} may not contain NUL"
                )));
            }
        }
        Ok(())
    }
}

fn allowed_places_path(app_data_dir: &Path) -> std::path::PathBuf {
    app_data_dir.join(ALLOWED_PLACES_FILE)
}

fn read_allowed_places(
    app_data_dir: &Path,
) -> Result<Vec<AllowedPlaceRecord>, AllowedPlaceStoreError> {
    std::fs::create_dir_all(app_data_dir)?;
    let path = allowed_places_path(app_data_dir);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let bytes = std::fs::read(path)?;
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_slice(&bytes).map_err(AllowedPlaceStoreError::from)
}

fn write_allowed_places(
    app_data_dir: &Path,
    records: &[AllowedPlaceRecord],
) -> Result<(), AllowedPlaceStoreError> {
    std::fs::create_dir_all(app_data_dir)?;
    let bytes = serde_json::to_vec_pretty(records)?;
    std::fs::write(allowed_places_path(app_data_dir), bytes)?;
    Ok(())
}

pub fn add_allowed_place_record(
    app_data_dir: &Path,
    record: &AllowedPlaceRecord,
) -> Result<(), AllowedPlaceStoreError> {
    record.validate()?;
    let mut records = read_allowed_places(app_data_dir)?;
    if !records
        .iter()
        .any(|existing| existing.stable_id == record.stable_id)
    {
        records.push(record.clone());
        write_allowed_places(app_data_dir, &records)?;
    }
    Ok(())
}

pub fn search_allowed_place_records(
    app_data_dir: &Path,
    query: &str,
) -> Result<Vec<AllowedPlaceRecord>, AllowedPlaceStoreError> {
    let mut records = read_allowed_places(app_data_dir)?;
    let needle = query.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return Ok(Vec::new());
    }
    records.retain(|record| {
        record.place_name.to_ascii_lowercase().contains(&needle)
            || record.person_name.to_ascii_lowercase().contains(&needle)
    });
    records.sort_by(|a, b| {
        a.place_name
            .to_ascii_lowercase()
            .cmp(&b.place_name.to_ascii_lowercase())
            .then_with(|| {
                a.person_name
                    .to_ascii_lowercase()
                    .cmp(&b.person_name.to_ascii_lowercase())
            })
            .then_with(|| a.stable_id.cmp(&b.stable_id))
    });
    Ok(records)
}
