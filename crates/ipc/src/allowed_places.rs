use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

const DB_FILE: &str = "allowed_places.sqlite";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
            app: "discord".to_string(),
            stable_id: format!("discord:{account}:direct_message:{recipient}"),
            account,
            kind: "direct_message".to_string(),
        }
    }
}

#[derive(Debug, Error)]
pub enum AllowedPlaceStoreError {
    #[error("filesystem: {0}")]
    Fs(#[from] std::io::Error),

    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("not allowed: {stable_id}")]
    NotAllowed { stable_id: String },
}

pub type Result<T> = std::result::Result<T, AllowedPlaceStoreError>;

pub fn allowed_places_db_path(app_data_dir: impl AsRef<Path>) -> PathBuf {
    app_data_dir.as_ref().join(DB_FILE)
}

pub fn add_allowed_place_record(
    app_data_dir: impl AsRef<Path>,
    record: AllowedPlaceRecord,
) -> Result<()> {
    std::fs::create_dir_all(app_data_dir.as_ref())?;
    let conn = Connection::open(allowed_places_db_path(app_data_dir))?;
    ensure_schema(&conn)?;
    conn.execute(
        "INSERT INTO allowed_places (app, account, kind, stable_id) VALUES (?1, ?2, ?3, ?4)",
        params![
            &record.app,
            &record.account,
            &record.kind,
            &record.stable_id
        ],
    )?;
    Ok(())
}

pub fn remove_allowed_place_record(
    app_data_dir: impl AsRef<Path>,
    stable_id: impl AsRef<str>,
) -> Result<bool> {
    std::fs::create_dir_all(app_data_dir.as_ref())?;
    let conn = Connection::open(allowed_places_db_path(app_data_dir))?;
    ensure_schema(&conn)?;
    let removed = conn.execute(
        "DELETE FROM allowed_places WHERE stable_id = ?1",
        params![stable_id.as_ref()],
    )?;
    Ok(removed == 1)
}

pub fn is_allowed_place_record(
    app_data_dir: impl AsRef<Path>,
    record: &AllowedPlaceRecord,
) -> Result<bool> {
    std::fs::create_dir_all(app_data_dir.as_ref())?;
    let conn = Connection::open(allowed_places_db_path(app_data_dir))?;
    ensure_schema(&conn)?;
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM allowed_places
         WHERE app = ?1 AND account = ?2 AND kind = ?3 AND stable_id = ?4",
        params![
            &record.app,
            &record.account,
            &record.kind,
            &record.stable_id
        ],
        |row| row.get(0),
    )?;
    Ok(count == 1)
}

pub fn require_allowed_place_record(
    app_data_dir: impl AsRef<Path>,
    record: &AllowedPlaceRecord,
) -> Result<()> {
    if is_allowed_place_record(app_data_dir, record)? {
        Ok(())
    } else {
        Err(AllowedPlaceStoreError::NotAllowed {
            stable_id: record.stable_id.clone(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AllowedPlaceSummary {
    pub distinct_apps: usize,
    pub places: usize,
}

pub fn allowed_place_summary(app_data_dir: impl AsRef<Path>) -> Result<AllowedPlaceSummary> {
    let path = allowed_places_db_path(app_data_dir);
    if !path.exists() {
        return Ok(AllowedPlaceSummary {
            distinct_apps: 0,
            places: 0,
        });
    }
    let conn = Connection::open(path)?;
    ensure_schema(&conn)?;
    let distinct_apps: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT app) FROM allowed_places",
        [],
        |row| row.get(0),
    )?;
    let places: i64 =
        conn.query_row("SELECT COUNT(*) FROM allowed_places", [], |row| row.get(0))?;
    Ok(AllowedPlaceSummary {
        distinct_apps: distinct_apps.max(0) as usize,
        places: places.max(0) as usize,
    })
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
