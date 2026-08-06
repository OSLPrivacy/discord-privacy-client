use rusqlite::{params, Connection, OptionalExtension};
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

fn open_allowed_places_db(app_data_dir: &Path) -> Result<Connection, AllowedPlaceStoreError> {
    std::fs::create_dir_all(app_data_dir)?;
    let conn = Connection::open(app_data_dir.join(ALLOWED_PLACES_DB))?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS allowed_places (
            app TEXT NOT NULL,
            account TEXT NOT NULL,
            kind TEXT NOT NULL,
            stable_id TEXT PRIMARY KEY NOT NULL,
            place_name TEXT NOT NULL DEFAULT '',
            person_name TEXT NOT NULL DEFAULT ''
        )",
        [],
    )?;

    let has_place_name: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM pragma_table_info('allowed_places') WHERE name = 'place_name'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if has_place_name.is_none() {
        conn.execute(
            "ALTER TABLE allowed_places ADD COLUMN place_name TEXT NOT NULL DEFAULT ''",
            [],
        )?;
    }

    let has_person_name: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM pragma_table_info('allowed_places') WHERE name = 'person_name'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if has_person_name.is_none() {
        conn.execute(
            "ALTER TABLE allowed_places ADD COLUMN person_name TEXT NOT NULL DEFAULT ''",
            [],
        )?;
    }

    Ok(conn)
}

pub fn add_allowed_place_record(
    app_data_dir: &Path,
    record: &AllowedPlaceRecord,
) -> Result<(), AllowedPlaceStoreError> {
    record.validate()?;
    let conn = open_allowed_places_db(app_data_dir)?;
    conn.execute(
        "INSERT INTO allowed_places (app, account, kind, stable_id, place_name, person_name)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            &record.app,
            &record.account,
            &record.kind,
            &record.stable_id,
            &record.place_name,
            &record.person_name,
        ],
    )?;
    Ok(())
}

pub fn search_allowed_place_records(
    app_data_dir: &Path,
    query: &str,
) -> Result<Vec<AllowedPlaceRecord>, AllowedPlaceStoreError> {
    let conn = open_allowed_places_db(app_data_dir)?;
    let needle = query.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return Ok(Vec::new());
    }
    let pattern = format!("%{needle}%");
    let mut statement = conn.prepare(
        "SELECT app, account, kind, stable_id, place_name, person_name
         FROM allowed_places
         WHERE lower(place_name) LIKE ?1 OR lower(person_name) LIKE ?1
         ORDER BY place_name COLLATE NOCASE, person_name COLLATE NOCASE, stable_id",
    )?;
    let rows = statement.query_map(params![pattern], |row| {
        Ok(AllowedPlaceRecord {
            app: row.get(0)?,
            account: row.get(1)?,
            kind: row.get(2)?,
            stable_id: row.get(3)?,
            place_name: row.get(4)?,
            person_name: row.get(5)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(AllowedPlaceStoreError::from)
}
