use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use thiserror::Error;

const DB_FILE: &str = "allowed_places.sqlite";

#[derive(Debug, Clone, PartialEq, Eq)]
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
        params![record.app, record.account, record.kind, record.stable_id],
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
