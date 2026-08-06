//! Durable store for places the user has allowed OSL to protect.

use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

pub const ALLOWED_PLACES_DB: &str = "allowed_places.sqlite";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllowedPlaceRecord {
    pub app_kind: String,
    pub place_kind: String,
    pub place_id: String,
    pub display_name: Option<String>,
    pub found_at_unix_secs: i64,
}

impl AllowedPlaceRecord {
    pub fn discord_direct_message(peer_discord_id: impl Into<String>) -> Self {
        let place_id = peer_discord_id.into();
        Self {
            app_kind: "discord".to_string(),
            place_kind: "direct_message".to_string(),
            display_name: Some(place_id.clone()),
            place_id,
            found_at_unix_secs: 0,
        }
    }
}

pub fn allowed_places_db_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join(ALLOWED_PLACES_DB)
}

pub fn add_allowed_place_record(
    app_data_dir: &Path,
    record: &AllowedPlaceRecord,
) -> Result<(), String> {
    let conn = open_allowed_places(app_data_dir)?;
    conn.execute(
        "INSERT INTO allowed_places \
         (app_kind, place_kind, place_id, display_name, found_at_unix_secs) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            record.app_kind,
            record.place_kind,
            record.place_id,
            record.display_name,
            record.found_at_unix_secs
        ],
    )
    .map_err(|e| format!("OSL: insert allowed place: {e}"))?;
    Ok(())
}

pub fn count_allowed_place_records(app_data_dir: &Path) -> Result<i64, String> {
    let conn = open_allowed_places(app_data_dir)?;
    conn.query_row("SELECT COUNT(*) FROM allowed_places", [], |row| row.get(0))
        .map_err(|e| format!("OSL: count allowed places: {e}"))
}

fn open_allowed_places(app_data_dir: &Path) -> Result<Connection, String> {
    std::fs::create_dir_all(app_data_dir)
        .map_err(|e| format!("OSL: create allowed places dir: {e}"))?;
    let path = allowed_places_db_path(app_data_dir);
    let conn = Connection::open(&path)
        .map_err(|e| format!("OSL: open allowed places {}: {e}", path.display()))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS allowed_places (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            app_kind TEXT NOT NULL,
            place_kind TEXT NOT NULL,
            place_id TEXT NOT NULL,
            display_name TEXT,
            found_at_unix_secs INTEGER NOT NULL
        );",
    )
    .map_err(|e| format!("OSL: initialize allowed places: {e}"))?;
    Ok(conn)
}
