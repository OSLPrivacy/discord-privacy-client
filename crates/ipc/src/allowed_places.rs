use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;

const ALLOWED_PLACES_DB: &str = "allowed_places.sqlite";

#[derive(Debug, thiserror::Error)]
pub enum AllowedPlaceStoreError {
    #[error("allowed-place record is invalid: {0}")]
    InvalidRecord(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllowedPlaceRecord {
    pub app: String,
    pub account: String,
    pub kind: String,
    pub stable_id: String,
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
        }
    }

    fn validate(&self) -> Result<(), AllowedPlaceStoreError> {
        for (field, value) in [
            ("app", self.app.as_str()),
            ("account", self.account.as_str()),
            ("kind", self.kind.as_str()),
            ("stable_id", self.stable_id.as_str()),
        ] {
            if value.is_empty() {
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

pub fn add_allowed_place_record(
    app_data_dir: &Path,
    record: &AllowedPlaceRecord,
) -> Result<(), AllowedPlaceStoreError> {
    record.validate()?;
    std::fs::create_dir_all(app_data_dir)?;
    let conn = Connection::open(app_data_dir.join(ALLOWED_PLACES_DB))?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS allowed_places (
            app TEXT NOT NULL,
            account TEXT NOT NULL,
            kind TEXT NOT NULL,
            stable_id TEXT PRIMARY KEY NOT NULL
        )",
        [],
    )?;
    conn.execute(
        "INSERT INTO allowed_places (app, account, kind, stable_id)
         VALUES (?1, ?2, ?3, ?4)",
        params![
            &record.app,
            &record.account,
            &record.kind,
            &record.stable_id
        ],
    )?;
    Ok(())
}
