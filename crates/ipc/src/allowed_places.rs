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
    validate_allowed_place_record(record)?;
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

fn validate_allowed_place_record(record: &AllowedPlaceRecord) -> Result<(), String> {
    require_present("app_kind", &record.app_kind)?;
    require_present("place_kind", &record.place_kind)?;
    require_present("place_id", &record.place_id)?;
    let Some(display_name) = record.display_name.as_deref() else {
        return Err("OSL: allowed place display_name is missing".to_string());
    };
    require_present("display_name", display_name)?;
    Ok(())
}

fn require_present(field: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("OSL: allowed place {field} is missing"));
    }
    Ok(())
}

pub fn count_allowed_place_records(app_data_dir: &Path) -> Result<i64, String> {
    let conn = open_allowed_places(app_data_dir)?;
    conn.query_row("SELECT COUNT(*) FROM allowed_places", [], |row| row.get(0))
        .map_err(|e| format!("OSL: count allowed places: {e}"))
}

pub fn count_allowed_place_records_for_kind(
    app_data_dir: &Path,
    app_kind: &str,
    place_kind: &str,
) -> Result<i64, String> {
    let conn = open_allowed_places(app_data_dir)?;
    conn.query_row(
        "SELECT COUNT(*) FROM allowed_places WHERE app_kind = ?1 AND place_kind = ?2",
        params![app_kind, place_kind],
        |row| row.get(0),
    )
    .map_err(|e| format!("OSL: count allowed places for kind: {e}"))
}

pub fn get_allowed_place_record(
    app_data_dir: &Path,
    app_kind: &str,
    place_kind: &str,
    place_id: &str,
) -> Result<Option<AllowedPlaceRecord>, String> {
    let conn = open_allowed_places(app_data_dir)?;
    let mut stmt = conn
        .prepare(
            "SELECT app_kind, place_kind, place_id, display_name, found_at_unix_secs \
             FROM allowed_places \
             WHERE app_kind = ?1 AND place_kind = ?2 AND place_id = ?3 \
             ORDER BY id DESC \
             LIMIT 1",
        )
        .map_err(|e| format!("OSL: prepare allowed place lookup: {e}"))?;
    let mut rows = stmt
        .query(params![app_kind, place_kind, place_id])
        .map_err(|e| format!("OSL: query allowed place lookup: {e}"))?;
    let Some(row) = rows
        .next()
        .map_err(|e| format!("OSL: read allowed place lookup: {e}"))?
    else {
        return Ok(None);
    };
    Ok(Some(AllowedPlaceRecord {
        app_kind: row
            .get(0)
            .map_err(|e| format!("OSL: read allowed place app_kind: {e}"))?,
        place_kind: row
            .get(1)
            .map_err(|e| format!("OSL: read allowed place place_kind: {e}"))?,
        place_id: row
            .get(2)
            .map_err(|e| format!("OSL: read allowed place place_id: {e}"))?,
        display_name: row
            .get(3)
            .map_err(|e| format!("OSL: read allowed place display_name: {e}"))?,
        found_at_unix_secs: row
            .get(4)
            .map_err(|e| format!("OSL: read allowed place found_at_unix_secs: {e}"))?,
    }))
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
