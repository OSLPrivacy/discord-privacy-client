//! Durable, per-person usage accounting for attachment storage and daily use.
//!
//! Stored-byte accounting is file-idempotent: writing a file with the same id
//! replaces its previous size instead of double-counting a retry. Daily values
//! are partitioned by UTC day, so a read after midnight naturally returns zero
//! until new activity is recorded.

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::path::Path;
use thiserror::Error;

const DATABASE_FILE: &str = "person_usage.sqlite";
const SECONDS_PER_DAY: i64 = 86_400;

#[derive(Debug, Error)]
pub enum UsageCounterError {
    #[error("usage counter storage error: {0}")]
    Storage(#[from] rusqlite::Error),
    #[error("usage counter filesystem error: {0}")]
    Filesystem(#[from] std::io::Error),
    #[error("invalid {0}")]
    Invalid(&'static str),
    #[error("usage counter overflow")]
    Overflow,
}

pub type Result<T> = std::result::Result<T, UsageCounterError>;

/// A person's current storage total and their three UTC-day counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PersonUsageCounters {
    pub stored_bytes: u64,
    pub bytes_sent_today: u64,
    pub bytes_fetched_today: u64,
    pub messages_sent_today: u64,
}

/// Open a counter ledger rooted at an account's application-data directory.
pub struct UsageCounterStore {
    conn: Connection,
}

impl UsageCounterStore {
    pub fn open(app_data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(app_data_dir)?;
        let conn = Connection::open(app_data_dir.join(DATABASE_FILE))?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS person_usage (
                person_id TEXT PRIMARY KEY NOT NULL,
                stored_bytes INTEGER NOT NULL DEFAULT 0 CHECK (stored_bytes >= 0),
                sent_day INTEGER,
                bytes_sent INTEGER NOT NULL DEFAULT 0 CHECK (bytes_sent >= 0),
                fetched_day INTEGER,
                bytes_fetched INTEGER NOT NULL DEFAULT 0 CHECK (bytes_fetched >= 0),
                messages_day INTEGER,
                messages_sent INTEGER NOT NULL DEFAULT 0 CHECK (messages_sent >= 0)
             );
             CREATE TABLE IF NOT EXISTS stored_files (
                person_id TEXT NOT NULL,
                file_id TEXT NOT NULL,
                byte_len INTEGER NOT NULL CHECK (byte_len >= 0),
                PRIMARY KEY (person_id, file_id),
                FOREIGN KEY (person_id) REFERENCES person_usage(person_id) ON DELETE CASCADE
             );",
        )?;
        Ok(Self { conn })
    }

    /// Read the four values for `person_id` using the current UTC day.
    pub fn read(&self, person_id: &str) -> Result<PersonUsageCounters> {
        self.read_at(person_id, now_unix_seconds())
    }

    /// Deterministic counterpart of [`Self::read`] for callers/tests that
    /// already have an event timestamp.
    pub fn read_at(&self, person_id: &str, unix_seconds: i64) -> Result<PersonUsageCounters> {
        validate_id(person_id, "person id")?;
        let day = utc_day(unix_seconds)?;
        let row: Option<(i64, Option<i64>, i64, Option<i64>, i64, Option<i64>, i64)> = self
            .conn
            .query_row(
                "SELECT stored_bytes, sent_day, bytes_sent, fetched_day, bytes_fetched,
                        messages_day, messages_sent
                   FROM person_usage WHERE person_id = ?1",
                params![person_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .optional()?;
        let Some((stored, sent_day, sent, fetched_day, fetched, messages_day, messages)) = row
        else {
            return Ok(PersonUsageCounters::default());
        };
        Ok(PersonUsageCounters {
            stored_bytes: as_u64(stored)?,
            bytes_sent_today: if sent_day == Some(day) {
                as_u64(sent)?
            } else {
                0
            },
            bytes_fetched_today: if fetched_day == Some(day) {
                as_u64(fetched)?
            } else {
                0
            },
            messages_sent_today: if messages_day == Some(day) {
                as_u64(messages)?
            } else {
                0
            },
        })
    }

    /// Record that `file_id` is now held for the person. Replacing the same
    /// id adjusts the total by the exact difference; upload retries therefore
    /// cannot inflate held storage.
    pub fn store_file(&mut self, person_id: &str, file_id: &str, byte_len: u64) -> Result<()> {
        validate_id(person_id, "person id")?;
        validate_id(file_id, "file id")?;
        let byte_len = as_i64(byte_len)?;
        let tx = self.conn.transaction()?;
        ensure_person(&tx, person_id)?;
        let old: Option<i64> = tx
            .query_row(
                "SELECT byte_len FROM stored_files WHERE person_id = ?1 AND file_id = ?2",
                params![person_id, file_id],
                |row| row.get(0),
            )
            .optional()?;
        let current: i64 = tx.query_row(
            "SELECT stored_bytes FROM person_usage WHERE person_id = ?1",
            params![person_id],
            |row| row.get(0),
        )?;
        let next = current
            .checked_sub(old.unwrap_or(0))
            .and_then(|value| value.checked_add(byte_len))
            .ok_or(UsageCounterError::Overflow)?;
        tx.execute(
            "INSERT INTO stored_files (person_id, file_id, byte_len) VALUES (?1, ?2, ?3)
             ON CONFLICT(person_id, file_id) DO UPDATE SET byte_len = excluded.byte_len",
            params![person_id, file_id, byte_len],
        )?;
        tx.execute(
            "UPDATE person_usage SET stored_bytes = ?2 WHERE person_id = ?1",
            params![person_id, next],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn remove_file(&mut self, person_id: &str, file_id: &str) -> Result<()> {
        validate_id(person_id, "person id")?;
        validate_id(file_id, "file id")?;
        let tx = self.conn.transaction()?;
        let old: Option<i64> = tx
            .query_row(
                "SELECT byte_len FROM stored_files WHERE person_id = ?1 AND file_id = ?2",
                params![person_id, file_id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(old) = old {
            tx.execute(
                "DELETE FROM stored_files WHERE person_id = ?1 AND file_id = ?2",
                params![person_id, file_id],
            )?;
            tx.execute(
                "UPDATE person_usage SET stored_bytes = stored_bytes - ?2 WHERE person_id = ?1",
                params![person_id, old],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn record_bytes_sent(
        &mut self,
        person_id: &str,
        bytes: u64,
        unix_seconds: i64,
    ) -> Result<()> {
        self.record_daily(person_id, bytes, unix_seconds, DailyCounter::Sent)
    }

    pub fn record_bytes_fetched(
        &mut self,
        person_id: &str,
        bytes: u64,
        unix_seconds: i64,
    ) -> Result<()> {
        self.record_daily(person_id, bytes, unix_seconds, DailyCounter::Fetched)
    }

    pub fn record_message_sent(&mut self, person_id: &str, unix_seconds: i64) -> Result<()> {
        self.record_daily(person_id, 1, unix_seconds, DailyCounter::Messages)
    }

    fn record_daily(
        &mut self,
        person_id: &str,
        amount: u64,
        unix_seconds: i64,
        counter: DailyCounter,
    ) -> Result<()> {
        validate_id(person_id, "person id")?;
        let amount = as_i64(amount)?;
        let day = utc_day(unix_seconds)?;
        let (day_column, value_column) = match counter {
            DailyCounter::Sent => ("sent_day", "bytes_sent"),
            DailyCounter::Fetched => ("fetched_day", "bytes_fetched"),
            DailyCounter::Messages => ("messages_day", "messages_sent"),
        };
        let tx = self.conn.transaction()?;
        ensure_person(&tx, person_id)?;
        let sql = format!(
            "UPDATE person_usage SET {value_column} = CASE WHEN {day_column} = ?2 THEN {value_column} + ?3 ELSE ?3 END, {day_column} = ?2 WHERE person_id = ?1"
        );
        tx.execute(&sql, params![person_id, day, amount])?;
        tx.commit()?;
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum DailyCounter {
    Sent,
    Fetched,
    Messages,
}

fn ensure_person(tx: &Transaction<'_>, person_id: &str) -> Result<()> {
    tx.execute(
        "INSERT INTO person_usage (person_id) VALUES (?1) ON CONFLICT(person_id) DO NOTHING",
        params![person_id],
    )?;
    Ok(())
}

fn validate_id(value: &str, label: &'static str) -> Result<()> {
    if value.trim().is_empty() || value.contains('\0') {
        return Err(UsageCounterError::Invalid(label));
    }
    Ok(())
}

fn now_unix_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn utc_day(unix_seconds: i64) -> Result<i64> {
    unix_seconds
        .checked_div(SECONDS_PER_DAY)
        .ok_or(UsageCounterError::Overflow)
}

fn as_i64(value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| UsageCounterError::Overflow)
}
fn as_u64(value: i64) -> Result<u64> {
    u64::try_from(value).map_err(|_| UsageCounterError::Overflow)
}
