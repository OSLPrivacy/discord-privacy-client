//! Durable store for places the user has allowed OSL to protect.

use rusqlite::{
    Connection,
    OptionalExtension,
    params,
};
use serde::{
    Deserialize,
    Serialize,
};
use std::path::{
    Path,
    PathBuf,
};
use thiserror::Error;
use std::path::Path;
use std::collections::BTreeMap;

const DB_FILE: &str = "allowed_places.sqlite";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
//!
//! Merge note (lane n): task 0104 and task 0137 both introduced this module in
//! parallel lanes. `AllowedPlaceRecord` is the 0104 wire record used by the
//! hub restart path; `StoredAllowedPlace` is the 0137 sqlite row (named
//! `AllowedPlaceRecord` in lane f) used by the auto-whitelist rule commands.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// One local place the user has explicitly allowed.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
            app: "discord".to_owned(),
            stable_id: format!("discord:{account}:direct_message:{recipient}"),
            account,
            kind: "direct_message".to_owned(),
        }
    }

    pub fn signal(
        account: impl Into<String>,
        kind: crate::auto_whitelist_rules::SignalWhitelistKind,
        place: impl Into<String>,
    ) -> Self {
        let account = account.into();
        let place = place.into();
        let kind = kind.allowed_place_kind();
        Self {
            app: "signal".to_owned(),
            stable_id: format!("signal:{account}:{kind}:{place}"),
            account,
            kind: kind.to_owned(),
        }
    }

    pub fn whatsapp(
        account: impl Into<String>,
        kind: crate::auto_whitelist_rules::WhatsAppWhitelistKind,
        place: impl Into<String>,
    ) -> Self {
        let account = account.into();
        let place = place.into();
        let kind = kind.allowed_place_kind();
        Self {
            app: "whatsapp".to_owned(),
            stable_id: format!("whatsapp:{account}:{kind}:{place}"),
            account,
            kind: kind.to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
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
impl AllowedPlaceRecord {
    pub fn discord_direct_message(account: &str, conversation_id: &str) -> Self {
        Self {
            app: "discord".to_owned(),
            account: account.to_owned(),
            kind: "direct_message".to_owned(),
            stable_id: format!("discord:{account}:direct_message:{conversation_id}"),
            app: "discord".to_string(),
            stable_id: format!("discord:{account}:direct_message:{recipient}"),
            account,
            kind: "direct_message".to_string(),
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
) -> Result<AllowedPlaceRecord> {
    validate_record(&record)?;
) -> Result<()> {
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

pub fn remove_allowed_place_records_for_person(
    app_data_dir: impl AsRef<Path>,
    person_id: impl AsRef<str>,
) -> Result<usize> {
    let person_id = person_id.as_ref();
    validate_field(person_id, "person_id")?;
    let records = list_allowed_place_records(&app_data_dir)?;
    let matching = records
        .iter()
        .filter(|record| allowed_place_record_belongs_to_person(record, person_id))
        .map(|record| record.stable_id.clone())
        .collect::<Vec<_>>();
    let conn = Connection::open(allowed_places_db_path(app_data_dir))?;
    ensure_schema(&conn)?;
    let mut removed = 0usize;
    for stable_id in matching {
        removed += conn.execute(
            "DELETE FROM allowed_places WHERE stable_id = ?1",
            params![stable_id],
        )?;
    }
    Ok(removed)
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

pub fn count_allowed_place_records_for_person(
    app_data_dir: impl AsRef<Path>,
    person_id: impl AsRef<str>,
) -> Result<usize> {
    let person_id = person_id.as_ref();
    validate_field(person_id, "person_id")?;
    Ok(list_allowed_place_records(app_data_dir)?
        .iter()
        .filter(|record| allowed_place_record_belongs_to_person(record, person_id))
        .count())
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

fn allowed_place_record_belongs_to_person(record: &AllowedPlaceRecord, person_id: &str) -> bool {
    record.stable_id == person_id || record.stable_id.rsplit(':').next() == Some(person_id)
        "INSERT INTO allowed_places (app, account, kind, stable_id) VALUES (?1, ?2, ?3, ?4)",
        params![record.app, record.account, record.kind, record.stable_id],
    )?;
    Ok(())
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

pub fn read_allowed_place_record(
    app_data_dir: impl AsRef<Path>,
    stable_id: impl AsRef<str>,
) -> Result<Option<AllowedPlaceRecord>> {
    validate_stable_id_shape(stable_id.as_ref())?;
    std::fs::create_dir_all(app_data_dir.as_ref())?;
    let conn = Connection::open(allowed_places_db_path(app_data_dir))?;
    ensure_schema(&conn)?;
    let mut stmt = conn
        .prepare("SELECT app, account, kind, stable_id FROM allowed_places WHERE stable_id = ?1")?;
    let mut rows = stmt.query(params![stable_id.as_ref()])?;
    if let Some(row) = rows.next()? {
        Ok(Some(AllowedPlaceRecord {
            app: row.get(0)?,
            account: row.get(1)?,
            kind: row.get(2)?,
            stable_id: row.get(3)?,
        }))
    } else {
        Ok(None)
    }
}

pub fn is_allowed_place_record(
    app_data_dir: impl AsRef<Path>,
    record: &AllowedPlaceRecord,
) -> Result<bool> {
    validate_allowed_place_stable_id(record)?;
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

fn validate_allowed_place_stable_id(record: &AllowedPlaceRecord) -> Result<()> {
    validate_stable_id_shape(&record.stable_id)?;
    let parts: Vec<_> = record.stable_id.split(':').collect();
    if parts[0] == record.app && parts[1] == record.account && parts[2] == record.kind {
        Ok(())
    } else {
        Err(AllowedPlaceStoreError::InvalidStableId {
            stable_id: record.stable_id.clone(),
        })
    }
}

fn validate_stable_id_shape(stable_id: &str) -> Result<()> {
    let mut parts = stable_id.split(':');
    let valid = matches!(
        (parts.next(), parts.next(), parts.next(), parts.next(), parts.next()),
        (Some(app), Some(account), Some(kind), Some(place), None)
            if app == "discord"
                && !account.trim().is_empty()
                && !kind.trim().is_empty()
                && !place.trim().is_empty()
                && account == account.trim()
                && kind == kind.trim()
                && place == place.trim()
    );
    if valid {
        Ok(())
    } else {
        Err(AllowedPlaceStoreError::InvalidStableId {
            stable_id: stable_id.to_string(),
        })
    }
}

pub const ALLOWED_PLACES_DB: &str = "allowed_places.sqlite";

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
pub const ALLOWED_PLACES_DB: &str = "allowed_places.sqlite";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredAllowedPlace {
    pub app_kind: String,
    pub place_kind: String,
    pub place_id: String,
    pub display_name: Option<String>,
    pub found_at_unix_secs: i64,
}

impl StoredAllowedPlace {
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
    record: &StoredAllowedPlace,
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
) -> Result<Option<StoredAllowedPlace>, String> {
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
    Ok(Some(StoredAllowedPlace {
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

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AllowedPlaceAccess {
    Approved,
    LookOnly,
}

impl AllowedPlaceAccess {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::LookOnly => "look-only",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AllowedPlaceMachineRecord {
    pub app: String,
    pub exact_place_name: String,
    pub access: AllowedPlaceAccess,
    pub source_task: u32,
    pub typing_box_check: String,
    pub person_can: String,
}

impl AllowedPlaceMachineRecord {
    pub const REQUIRED_PARTS: [&'static str; 6] = [
        "app",
        "exactPlaceName",
        "access",
        "sourceTask",
        "typingBoxCheck",
        "personCan",
    ];

    pub fn filled_part_count(&self) -> usize {
        [
            !self.app.trim().is_empty(),
            !self.exact_place_name.trim().is_empty(),
            !self.access.as_str().trim().is_empty(),
            self.source_task != 0,
            !self.typing_box_check.trim().is_empty(),
            !self.person_can.trim().is_empty(),
        ]
        .into_iter()
        .filter(|filled| *filled)
        .count()
    }
}

pub const TASK_4202_EXAMPLE_ALLOWED_PLACE_RECORD: &str = r#"{
  "app": "Telegram",
  "exactPlaceName": "Saved Messages",
  "access": "approved",
  "sourceTask": 4202,
  "typingBoxCheck": "native_telegram_adapter::tests::drive_the_real_telegram_composer",
  "personCan": "Send protected text to the user's own Telegram saved chat."
}"#;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AllowedPlaceRecordReadError {
    #[error("missing allowed-place record part: {part}")]
    MissingPart { part: &'static str },

    #[error("empty allowed-place record part: {part}")]
    EmptyPart { part: &'static str },

    #[error("invalid allowed-place record JSON: {0}")]
    Json(String),
}

pub fn read_allowed_place_machine_record_json(
    input: &str,
) -> std::result::Result<AllowedPlaceMachineRecord, AllowedPlaceRecordReadError> {
    let value: serde_json::Value = serde_json::from_str(input)
        .map_err(|error| AllowedPlaceRecordReadError::Json(error.to_string()))?;
    let object = value.as_object().ok_or_else(|| {
        AllowedPlaceRecordReadError::Json("allowed-place record must be an object".to_owned())
    })?;

    for part in AllowedPlaceMachineRecord::REQUIRED_PARTS {
        if !object.contains_key(part) {
            return Err(AllowedPlaceRecordReadError::MissingPart { part });
        }
    }

    let record: AllowedPlaceMachineRecord = serde_json::from_value(value)
        .map_err(|error| AllowedPlaceRecordReadError::Json(error.to_string()))?;
    record.validate_filled()?;
    Ok(record)
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

const ALLOWED_PLACES_FILE: &str = "allowed_places.json";

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
    let bytes = serde_json::to_vec_pretty(records)?;
    crate::recoverable_file::write_recoverable(&allowed_places_path(app_data_dir), &bytes)?;
    Ok(())
}

pub const APP_DISCORD: &str = "discord";
pub const APP_TELEGRAM: &str = "telegram";
pub const KIND_DIRECT_MESSAGE: &str = "direct_message";
pub const KIND_GROUP_CHAT: &str = "group_chat";
pub const KIND_CHANNEL: &str = "channel";
pub const KIND_PUBLIC_POST: &str = "public_post";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AllowedPlaceKind {
    pub app: String,
    pub name: String,
}

fn validate_allowed_place_field(value: &str, label: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 512 || value.contains('\0') {
        return Err(format!("OSL: allowed-place {label} is invalid"));
    }
    Ok(())
}

pub fn telegram_whitelist_kinds() -> Vec<AllowedPlaceKind> {
    [
        KIND_DIRECT_MESSAGE,
        KIND_GROUP_CHAT,
        KIND_CHANNEL,
        KIND_PUBLIC_POST,
    ]
    .into_iter()
    .map(|name| AllowedPlaceKind {
        app: APP_TELEGRAM.to_string(),
        name: name.to_string(),
    })
    .collect()
}

pub fn normalize_telegram_whitelist_kind(input: &str) -> Result<String, String> {
    let normalized = input.trim().to_ascii_lowercase().replace('-', "_");
    if [
        KIND_DIRECT_MESSAGE,
        KIND_GROUP_CHAT,
        KIND_CHANNEL,
        KIND_PUBLIC_POST,
    ]
    .contains(&normalized.as_str())
    {
        Ok(normalized)
    } else {
        Err(format!("OSL: unknown Telegram whitelist kind '{input}'"))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedAllowedPlaces {
    #[serde(default)]
    places: BTreeMap<String, AllowedPlaceRecord>,
}

impl SavedAllowedPlaces {
    pub fn save(&mut self, record: AllowedPlaceRecord) -> AllowedPlaceRecord {
        self.places.insert(record.stable_id.clone(), record.clone());
        record
    }

    pub fn query(&self, stable_id: &str) -> Option<AllowedPlaceQuery> {
        self.places.get(stable_id).cloned().map(Into::into)
    }
}

pub fn load_allowed_places(path: &Path) -> SavedAllowedPlaces {
    let Ok(blob) = std::fs::read(path) else {
        return SavedAllowedPlaces::default();
    };
    let plain = match crate::main_password::maybe_decrypt_file(path, &blob) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "OSL: load allowed_places.json decrypt failed");
            return SavedAllowedPlaces::default();
        }
    };
    serde_json::from_slice(&plain).unwrap_or_default()
}
