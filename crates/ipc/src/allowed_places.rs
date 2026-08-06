use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
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

impl AllowedPlaceMachineRecord {
    fn validate_filled(&self) -> std::result::Result<(), AllowedPlaceRecordReadError> {
        if self.app.trim().is_empty() {
            return Err(AllowedPlaceRecordReadError::EmptyPart { part: "app" });
        }
        if self.exact_place_name.trim().is_empty() {
            return Err(AllowedPlaceRecordReadError::EmptyPart {
                part: "exactPlaceName",
            });
        }
        if self.source_task == 0 {
            return Err(AllowedPlaceRecordReadError::EmptyPart { part: "sourceTask" });
        }
        if self.typing_box_check.trim().is_empty() {
            return Err(AllowedPlaceRecordReadError::EmptyPart {
                part: "typingBoxCheck",
            });
        }
        if self.person_can.trim().is_empty() {
            return Err(AllowedPlaceRecordReadError::EmptyPart { part: "personCan" });
        }
        Ok(())
    }
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

    pub fn email_address(account: impl Into<String>, address: impl Into<String>) -> Self {
        let account = account.into();
        let address = address.into();
        Self {
            app: "email".to_string(),
            stable_id: format!("email:{account}:email_address:{address}"),
            account,
            kind: "email_address".to_string(),
        }
    }

    pub fn email_domain(account: impl Into<String>, domain: impl Into<String>) -> Self {
        let account = account.into();
        let domain = domain.into();
        Self {
            app: "email".to_string(),
            stable_id: format!("email:{account}:email_domain:{domain}"),
            account,
            kind: "email_domain".to_string(),
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
