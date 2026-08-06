use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

const DB_FILE: &str = "allowed_places.sqlite";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllowedPlaceRecord {
    pub app: String,
    pub account: String,
    pub kind: String,
    pub stable_id: String,
}

impl AllowedPlaceRecord {
    pub fn discord_direct_message(
        account: impl Into<String>,
        recipient: impl Into<String>,
    ) -> Self {
        let account = account.into();
        let recipient = recipient.into();
        Self {
            app: "discord".to_owned(),
            stable_id: format!("discord:{account}:direct_message:{recipient}"),
            account,
            kind: "direct_message".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllowedPlaceQuery {
    pub app: String,
    pub account: String,
    pub kind: String,
    pub stable_id: String,
}

impl From<AllowedPlaceRecord> for AllowedPlaceQuery {
    fn from(record: AllowedPlaceRecord) -> Self {
        Self {
            app: record.app,
            account: record.account,
            kind: record.kind,
            stable_id: record.stable_id,
        }
    }
}

#[derive(Debug, Error)]
pub enum AllowedPlaceStoreError {
    #[error("allowed-place filesystem error: {0}")]
    Fs(#[from] std::io::Error),

    #[error("allowed-place sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, AllowedPlaceStoreError>;

pub fn allowed_places_db_path(app_data_dir: impl AsRef<Path>) -> PathBuf {
    app_data_dir.as_ref().join(DB_FILE)
}

pub fn add_allowed_place_record(
    app_data_dir: impl AsRef<Path>,
    record: AllowedPlaceRecord,
) -> Result<AllowedPlaceRecord> {
    validate_record(&record)?;
    std::fs::create_dir_all(app_data_dir.as_ref())?;
    let conn = Connection::open(allowed_places_db_path(app_data_dir))?;
    ensure_schema(&conn)?;
    conn.execute(
        "INSERT OR REPLACE INTO allowed_places (app, account, kind, stable_id)
         VALUES (?1, ?2, ?3, ?4)",
        params![record.app, record.account, record.kind, record.stable_id],
    )?;
    Ok(record)
}

pub fn remove_allowed_place_record(
    app_data_dir: impl AsRef<Path>,
    stable_id: impl AsRef<str>,
) -> Result<bool> {
    validate_field(stable_id.as_ref(), "stable_id")?;
    std::fs::create_dir_all(app_data_dir.as_ref())?;
    let conn = Connection::open(allowed_places_db_path(app_data_dir))?;
    ensure_schema(&conn)?;
    let removed = conn.execute(
        "DELETE FROM allowed_places WHERE stable_id = ?1",
        params![stable_id.as_ref()],
    )?;
    Ok(removed == 1)
}

pub fn list_allowed_place_records(
    app_data_dir: impl AsRef<Path>,
) -> Result<Vec<AllowedPlaceRecord>> {
    std::fs::create_dir_all(app_data_dir.as_ref())?;
    let conn = Connection::open(allowed_places_db_path(app_data_dir))?;
    ensure_schema(&conn)?;
    let mut stmt = conn.prepare(
        "SELECT app, account, kind, stable_id
         FROM allowed_places
         ORDER BY app ASC, account ASC, kind ASC, stable_id ASC",
    )?;
    let records = stmt
        .query_map([], |row| {
            Ok(AllowedPlaceRecord {
                app: row.get(0)?,
                account: row.get(1)?,
                kind: row.get(2)?,
                stable_id: row.get(3)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(records)
}

pub fn allowed_place_is_allowed(
    app_data_dir: impl AsRef<Path>,
    query: &AllowedPlaceQuery,
) -> Result<bool> {
    validate_query(query)?;
    std::fs::create_dir_all(app_data_dir.as_ref())?;
    let conn = Connection::open(allowed_places_db_path(app_data_dir))?;
    ensure_schema(&conn)?;
    let found = conn
        .query_row(
            "SELECT 1
             FROM allowed_places
             WHERE app = ?1 AND account = ?2 AND kind = ?3 AND stable_id = ?4
             LIMIT 1",
            params![query.app, query.account, query.kind, query.stable_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    Ok(found)
}

fn ensure_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS allowed_places (
            stable_id TEXT PRIMARY KEY,
            app TEXT NOT NULL,
            account TEXT NOT NULL,
            kind TEXT NOT NULL
        );",
    )?;
    Ok(())
}

fn validate_record(record: &AllowedPlaceRecord) -> Result<()> {
    validate_field(&record.app, "app")?;
    validate_field(&record.account, "account")?;
    validate_field(&record.kind, "kind")?;
    validate_field(&record.stable_id, "stable_id")?;
    let prefix = format!("{}:{}:{}:", record.app, record.account, record.kind);
    if !record.stable_id.starts_with(&prefix) {
        return Err(AllowedPlaceStoreError::Invalid(
            "stable_id must be scoped by app, account, and kind".to_owned(),
        ));
    }
    Ok(())
}

fn validate_query(query: &AllowedPlaceQuery) -> Result<()> {
    validate_record(&AllowedPlaceRecord {
        app: query.app.clone(),
        account: query.account.clone(),
        kind: query.kind.clone(),
        stable_id: query.stable_id.clone(),
    })
}

fn validate_field(value: &str, name: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 512
        || value.contains('\0')
        || value.chars().any(char::is_whitespace)
    {
        return Err(AllowedPlaceStoreError::Invalid(format!(
            "allowed-place {name} is invalid"
        )));
    }
    Ok(())
}
