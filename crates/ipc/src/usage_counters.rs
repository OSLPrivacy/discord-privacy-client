//! Durable, per-person usage accounting for attachment storage and daily use.
//!
//! Stored-byte accounting is file-idempotent: writing a file with the same id
//! replaces its previous size instead of double-counting a retry. Daily values
//! are partitioned by UTC day, so a read after midnight naturally returns zero
//! until new activity is recorded.

use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use sha2::{Digest, Sha256};
use std::fmt;
use std::path::Path;
use thiserror::Error;

const DATABASE_FILE: &str = "person_usage.sqlite";
const SECONDS_PER_DAY: i64 = 86_400;
const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;

/// The four usage ceilings checked before an upload may accept bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsageCeilings {
    pub stored_bytes: u64,
    pub bytes_sent_today: u64,
    pub bytes_fetched_today: u64,
    pub messages_sent_today: u64,
}

impl UsageCeilings {
    /// Free limits approved in task 0575. Storage units follow the existing
    /// attachment code's binary-byte convention (MiB/GiB).
    pub const FREE: Self = Self {
        stored_bytes: GIB,
        bytes_sent_today: 250 * MIB,
        bytes_fetched_today: 250 * MIB,
        messages_sent_today: 200,
    };

    /// Pro limits approved in task 0575.
    pub const PRO: Self = Self {
        stored_bytes: 150 * GIB,
        bytes_sent_today: 5 * GIB,
        bytes_fetched_today: 5 * GIB,
        messages_sent_today: 5_000,
    };
}

/// A stable, user-facing name for the counter that stopped an upload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageCeiling {
    StoredBytes,
    BytesSentToday,
    BytesFetchedToday,
    MessagesSentToday,
}

impl fmt::Display for UsageCeiling {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::StoredBytes => "stored bytes ceiling",
            Self::BytesSentToday => "bytes sent today ceiling",
            Self::BytesFetchedToday => "bytes fetched today ceiling",
            Self::MessagesSentToday => "messages sent today ceiling",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadRefusal {
    pub person_id: String,
    pub ceiling: UsageCeiling,
    pub current: u64,
    pub limit: u64,
}

impl fmt::Display for UploadRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "upload for person {} refused: {} reached (current {}, limit {})",
            self.person_id, self.ceiling, self.current, self.limit
        )
    }
}

/// Failure from the upload-start boundary. `Refused` and `Counter` happen
/// before the upload callback can accept a byte.
#[derive(Debug)]
pub enum UploadStartError<E> {
    Refused(UploadRefusal),
    Counter(UsageCounterError),
    Upload(E),
}

impl<E: fmt::Display> fmt::Display for UploadStartError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(refusal) => refusal.fmt(formatter),
            Self::Counter(error) => error.fmt(formatter),
            Self::Upload(error) => error.fmt(formatter),
        }
    }
}

impl<E> From<UsageCounterError> for UploadStartError<E> {
    fn from(error: UsageCounterError) -> Self {
        Self::Counter(error)
    }
}

impl<E> From<rusqlite::Error> for UploadStartError<E> {
    fn from(error: rusqlite::Error) -> Self {
        Self::Counter(UsageCounterError::Storage(error))
    }
}

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
    #[error("{action} refused: account {person_id} is stopped")]
    AccountStopped {
        person_id: String,
        action: AccountAction,
    },
    #[error("file not found")]
    FileNotFound,
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

/// Operations disabled by the stop-one-account command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountAction {
    Upload,
    Download,
    Send,
}

impl fmt::Display for AccountAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Upload => "upload",
            Self::Download => "download",
            Self::Send => "send",
        })
    }
}

/// Result of the idempotent stop command. The byte count is informational and
/// is never changed by stopping an account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoppedAccount {
    pub person_id: String,
    pub stopped_at_unix_seconds: i64,
    pub stored_bytes: u64,
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
             PRAGMA foreign_keys = ON;
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
             );
             CREATE TABLE IF NOT EXISTS stopped_accounts (
                person_id TEXT PRIMARY KEY NOT NULL,
                stopped_at INTEGER NOT NULL,
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

    /// Check all four counters and, only if they are below their ceilings,
    /// invoke the operation that accepts the upload bytes.
    ///
    /// The immediate transaction serializes admission across store handles,
    /// so two uploads cannot both spend the same last byte. A refused upload
    /// never invokes `upload`; a failed upload rolls the reservation back.
    /// Successful uploads atomically add the file to stored bytes and add its
    /// length to today's sent-byte counter. Message and fetched-byte accounting
    /// remain owned by their actual send/fetch completion paths.
    pub fn with_upload_admission<T, E>(
        &mut self,
        person_id: &str,
        file_id: &str,
        byte_len: u64,
        ceilings: UsageCeilings,
        unix_seconds: i64,
        upload: impl FnOnce() -> std::result::Result<T, E>,
    ) -> std::result::Result<T, UploadStartError<E>> {
        validate_id(person_id, "person id")?;
        validate_id(file_id, "file id")?;
        let byte_len_i64 = as_i64(byte_len)?;
        let day = utc_day(unix_seconds)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        reject_if_stopped(&tx, person_id, AccountAction::Upload)?;
        ensure_person(&tx, person_id)?;
        let usage = read_at_tx(&tx, person_id, day)?;
        let old: Option<i64> = tx
            .query_row(
                "SELECT byte_len FROM stored_files WHERE person_id = ?1 AND file_id = ?2",
                params![person_id, file_id],
                |row| row.get(0),
            )
            .optional()?;
        let old_byte_len = as_u64(old.unwrap_or(0))?;
        let projected_stored = usage
            .stored_bytes
            .checked_sub(old_byte_len)
            .and_then(|value| value.checked_add(byte_len));

        if projected_stored.is_none_or(|next| next > ceilings.stored_bytes) {
            return Err(UploadStartError::Refused(UploadRefusal {
                person_id: person_id.to_owned(),
                ceiling: UsageCeiling::StoredBytes,
                current: usage.stored_bytes,
                limit: ceilings.stored_bytes,
            }));
        }
        if usage
            .bytes_sent_today
            .checked_add(byte_len)
            .is_none_or(|next| next > ceilings.bytes_sent_today)
        {
            return Err(UploadStartError::Refused(UploadRefusal {
                person_id: person_id.to_owned(),
                ceiling: UsageCeiling::BytesSentToday,
                current: usage.bytes_sent_today,
                limit: ceilings.bytes_sent_today,
            }));
        }
        if usage.bytes_fetched_today >= ceilings.bytes_fetched_today {
            return Err(UploadStartError::Refused(UploadRefusal {
                person_id: person_id.to_owned(),
                ceiling: UsageCeiling::BytesFetchedToday,
                current: usage.bytes_fetched_today,
                limit: ceilings.bytes_fetched_today,
            }));
        }
        if usage.messages_sent_today >= ceilings.messages_sent_today {
            return Err(UploadStartError::Refused(UploadRefusal {
                person_id: person_id.to_owned(),
                ceiling: UsageCeiling::MessagesSentToday,
                current: usage.messages_sent_today,
                limit: ceilings.messages_sent_today,
            }));
        }

        let uploaded = upload().map_err(UploadStartError::Upload)?;
        let next_stored = projected_stored.ok_or(UsageCounterError::Overflow)?;
        let next_sent = usage
            .bytes_sent_today
            .checked_add(byte_len)
            .ok_or(UsageCounterError::Overflow)?;
        tx.execute(
            "INSERT INTO stored_files (person_id, file_id, byte_len) VALUES (?1, ?2, ?3)
             ON CONFLICT(person_id, file_id) DO UPDATE SET byte_len = excluded.byte_len",
            params![person_id, file_id, byte_len_i64],
        )?;
        tx.execute(
            "UPDATE person_usage
                SET stored_bytes = ?2, sent_day = ?3, bytes_sent = ?4
              WHERE person_id = ?1",
            params![person_id, as_i64(next_stored)?, day, as_i64(next_sent)?],
        )?;
        tx.commit()?;
        Ok(uploaded)
    }

    /// Record that `file_id` is now held for the person. Replacing the same
    /// id adjusts the total by the exact difference; upload retries therefore
    /// cannot inflate held storage.
    pub fn store_file(&mut self, person_id: &str, file_id: &str, byte_len: u64) -> Result<()> {
        validate_id(person_id, "person id")?;
        validate_id(file_id, "file id")?;
        let byte_len = as_i64(byte_len)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        reject_if_stopped(&tx, person_id, AccountAction::Upload)?;
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

    /// Account-scoped upload boundary. The returned fingerprint is SHA-256 of
    /// the exact bytes accepted by this call.
    pub fn upload_file(&mut self, person_id: &str, file_id: &str, bytes: &[u8]) -> Result<String> {
        let fingerprint = format!("{:x}", Sha256::digest(bytes));
        self.store_file(person_id, file_id, bytes.len() as u64)?;
        Ok(fingerprint)
    }

    /// Stop one account without removing its stored files or changing usage.
    /// Repeating the command preserves the original stop time.
    pub fn stop_account(&mut self, person_id: &str) -> Result<StoppedAccount> {
        validate_id(person_id, "person id")?;
        let stopped_at = now_unix_seconds();
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_person(&tx, person_id)?;
        tx.execute(
            "INSERT INTO stopped_accounts (person_id, stopped_at) VALUES (?1, ?2)
             ON CONFLICT(person_id) DO NOTHING",
            params![person_id, stopped_at],
        )?;
        let (stopped_at_unix_seconds, stored_bytes): (i64, i64) = tx.query_row(
            "SELECT stopped_accounts.stopped_at, person_usage.stored_bytes
               FROM stopped_accounts
               JOIN person_usage USING (person_id)
              WHERE stopped_accounts.person_id = ?1",
            params![person_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        tx.commit()?;
        Ok(StoppedAccount {
            person_id: person_id.to_owned(),
            stopped_at_unix_seconds,
            stored_bytes: as_u64(stored_bytes)?,
        })
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
        self.record_daily(
            person_id,
            bytes,
            unix_seconds,
            DailyCounter::Sent,
            AccountAction::Send,
        )
    }

    pub fn record_bytes_fetched(
        &mut self,
        person_id: &str,
        bytes: u64,
        unix_seconds: i64,
    ) -> Result<()> {
        self.record_daily(
            person_id,
            bytes,
            unix_seconds,
            DailyCounter::Fetched,
            AccountAction::Download,
        )
    }

    pub fn record_message_sent(&mut self, person_id: &str, unix_seconds: i64) -> Result<()> {
        self.record_daily(
            person_id,
            1,
            unix_seconds,
            DailyCounter::Messages,
            AccountAction::Send,
        )
    }

    /// Download one known file in accounting terms and return its exact byte
    /// length. The stop check and fetched-byte increment share one transaction.
    pub fn download_file(
        &mut self,
        person_id: &str,
        file_id: &str,
        unix_seconds: i64,
    ) -> Result<u64> {
        validate_id(person_id, "person id")?;
        validate_id(file_id, "file id")?;
        let day = utc_day(unix_seconds)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        reject_if_stopped(&tx, person_id, AccountAction::Download)?;
        let byte_len: i64 = tx
            .query_row(
                "SELECT byte_len FROM stored_files WHERE person_id = ?1 AND file_id = ?2",
                params![person_id, file_id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(UsageCounterError::FileNotFound)?;
        update_daily(&tx, person_id, byte_len, day, DailyCounter::Fetched)?;
        tx.commit()?;
        as_u64(byte_len)
    }

    /// Record one completed send and its bytes atomically.
    pub fn send_message(&mut self, person_id: &str, bytes: u64, unix_seconds: i64) -> Result<()> {
        validate_id(person_id, "person id")?;
        let bytes = as_i64(bytes)?;
        let day = utc_day(unix_seconds)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        reject_if_stopped(&tx, person_id, AccountAction::Send)?;
        ensure_person(&tx, person_id)?;
        update_daily(&tx, person_id, bytes, day, DailyCounter::Sent)?;
        update_daily(&tx, person_id, 1, day, DailyCounter::Messages)?;
        tx.commit()?;
        Ok(())
    }

    fn record_daily(
        &mut self,
        person_id: &str,
        amount: u64,
        unix_seconds: i64,
        counter: DailyCounter,
        action: AccountAction,
    ) -> Result<()> {
        validate_id(person_id, "person id")?;
        let amount = as_i64(amount)?;
        let day = utc_day(unix_seconds)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        reject_if_stopped(&tx, person_id, action)?;
        ensure_person(&tx, person_id)?;
        update_daily(&tx, person_id, amount, day, counter)?;
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

fn read_at_tx(tx: &Transaction<'_>, person_id: &str, day: i64) -> Result<PersonUsageCounters> {
    let (stored, sent_day, sent, fetched_day, fetched, messages_day, messages): (
        i64,
        Option<i64>,
        i64,
        Option<i64>,
        i64,
        Option<i64>,
        i64,
    ) = tx.query_row(
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
    )?;
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

fn reject_if_stopped(tx: &Transaction<'_>, person_id: &str, action: AccountAction) -> Result<()> {
    let stopped = tx
        .query_row(
            "SELECT 1 FROM stopped_accounts WHERE person_id = ?1",
            params![person_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if stopped {
        return Err(UsageCounterError::AccountStopped {
            person_id: person_id.to_owned(),
            action,
        });
    }
    Ok(())
}

fn update_daily(
    tx: &Transaction<'_>,
    person_id: &str,
    amount: i64,
    day: i64,
    counter: DailyCounter,
) -> Result<()> {
    let (day_column, value_column) = match counter {
        DailyCounter::Sent => ("sent_day", "bytes_sent"),
        DailyCounter::Fetched => ("fetched_day", "bytes_fetched"),
        DailyCounter::Messages => ("messages_day", "messages_sent"),
    };
    let sql = format!(
        "UPDATE person_usage SET {value_column} = CASE WHEN {day_column} = ?2 THEN {value_column} + ?3 ELSE ?3 END, {day_column} = ?2 WHERE person_id = ?1"
    );
    tx.execute(&sql, params![person_id, day, amount])?;
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
