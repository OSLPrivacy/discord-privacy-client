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
pub const STOPPED_ACCOUNT_RETENTION_DAYS: i64 = 7;
const STOPPED_ACCOUNT_RETENTION_SECONDS: i64 = STOPPED_ACCOUNT_RETENTION_DAYS * SECONDS_PER_DAY;
pub const DELETE_ACCOUNT_CONFIRMATION: &str = "ERASE";
pub const DELETE_ACCOUNT_SUMMARY: &str = "Upload, download and send stop now. Messages, including relay-held undelivered ciphertext, files, server-held keys, settings, sessions and the account row purge seven days later. OSL stores no payment data. A permanent lost-key name tombstone is the sole exception.";
/// TASK 3120 owner-approved claimant wording. Keep this byte-for-byte stable.
pub const LOST_KEY_NAME_CLAIM_REFUSAL: &str = "This name belongs to an account whose key was lost. It stays reserved permanently and cannot be claimed by anyone, including its original owner.";
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
    #[error("{0}")]
    LostKeyNamePermanentlyReserved(&'static str),
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

/// The six service-data kinds removed seven days after an account is stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServiceDataKind {
    Messages,
    Files,
    Keys,
    Settings,
    Sessions,
    AccountRecords,
}

/// One shipping writer and the ruled service-data class containing its rows.
/// This manifest is intentionally public so release checks can reconcile it
/// against the independently discovered SQLite storage locations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccountDataWriter {
    pub writer: &'static str,
    pub location: &'static str,
    pub kind: ServiceDataKind,
    pub summary_label: &'static str,
}

pub const SHIPPING_ACCOUNT_DATA_WRITERS: [AccountDataWriter; 7] = [
    AccountDataWriter {
        writer: "store_local_message",
        location: "service_messages",
        kind: ServiceDataKind::Messages,
        summary_label: "Messages",
    },
    AccountDataWriter {
        writer: "store_relay_held_undelivered_ciphertext",
        location: "relay_held_undelivered_ciphertexts",
        kind: ServiceDataKind::Messages,
        summary_label: "relay-held undelivered ciphertext",
    },
    AccountDataWriter {
        writer: "store_file",
        location: "stored_files",
        kind: ServiceDataKind::Files,
        summary_label: "files",
    },
    AccountDataWriter {
        writer: "store_service_data/keys",
        location: "service_keys",
        kind: ServiceDataKind::Keys,
        summary_label: "server-held keys",
    },
    AccountDataWriter {
        writer: "store_service_data/settings",
        location: "service_settings",
        kind: ServiceDataKind::Settings,
        summary_label: "settings",
    },
    AccountDataWriter {
        writer: "store_service_data/sessions",
        location: "service_sessions",
        kind: ServiceDataKind::Sessions,
        summary_label: "sessions",
    },
    AccountDataWriter {
        writer: "store_service_data/account_records",
        location: "service_account_records",
        kind: ServiceDataKind::AccountRecords,
        summary_label: "account row",
    },
];

/// The destructive Account-screen choice. Cancel is represented explicitly;
/// an absent input is a confirmation attempt with an empty string and cannot
/// stop the account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteAccountChoice<'a> {
    Cancel,
    Confirm(&'a str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeleteAccountOutcome {
    Cancelled,
    ConfirmationRequired,
    Stopped(StoppedAccount),
}

/// Account > Delete account is always the retained stopped-account route.
/// It must never dispatch the separate immediate Remove everything action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteAccountRoute {
    StoppedAccountRetention,
}

pub const DELETE_ACCOUNT_ROUTE: DeleteAccountRoute = DeleteAccountRoute::StoppedAccountRetention;

impl ServiceDataKind {
    pub const ALL: [Self; 6] = [
        Self::Messages,
        Self::Files,
        Self::Keys,
        Self::Settings,
        Self::Sessions,
        Self::AccountRecords,
    ];

    fn table(self) -> &'static str {
        match self {
            Self::Messages => "service_messages",
            Self::Files => "stored_files",
            Self::Keys => "service_keys",
            Self::Settings => "service_settings",
            Self::Sessions => "service_sessions",
            Self::AccountRecords => "service_account_records",
        }
    }
}

/// Account-scoped counts for every data kind covered by the stopped-account
/// retention promise.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ServiceDataCounts {
    pub messages: u64,
    pub files: u64,
    pub keys: u64,
    pub settings: u64,
    pub sessions: u64,
    pub account_records: u64,
}

/// Audit detail for one account removed by a due purge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PurgedStoppedAccount {
    pub person_id: String,
    /// Counts observed immediately before the account-scoped cascade.
    pub deleted: ServiceDataCounts,
    /// Independent counts observed in the same transaction after the cascade.
    pub remaining: ServiceDataCounts,
    /// Permanent non-account tombstones observed after the cascade.
    pub lost_key_name_tombstones: u64,
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
             );
             CREATE INDEX IF NOT EXISTS stopped_accounts_due
                ON stopped_accounts(stopped_at, person_id);
             CREATE TABLE IF NOT EXISTS service_messages (
                person_id TEXT NOT NULL,
                data_id TEXT NOT NULL,
                PRIMARY KEY (person_id, data_id),
                FOREIGN KEY (person_id) REFERENCES person_usage(person_id) ON DELETE CASCADE
             );
             CREATE TABLE IF NOT EXISTS relay_held_undelivered_ciphertexts (
                person_id TEXT NOT NULL,
                data_id TEXT NOT NULL,
                PRIMARY KEY (person_id, data_id),
                FOREIGN KEY (person_id) REFERENCES person_usage(person_id) ON DELETE CASCADE
             );
             CREATE TABLE IF NOT EXISTS service_keys (
                person_id TEXT NOT NULL,
                data_id TEXT NOT NULL,
                PRIMARY KEY (person_id, data_id),
                FOREIGN KEY (person_id) REFERENCES person_usage(person_id) ON DELETE CASCADE
             );
             CREATE TABLE IF NOT EXISTS service_settings (
                person_id TEXT NOT NULL,
                data_id TEXT NOT NULL,
                PRIMARY KEY (person_id, data_id),
                FOREIGN KEY (person_id) REFERENCES person_usage(person_id) ON DELETE CASCADE
             );
             CREATE TABLE IF NOT EXISTS service_sessions (
                person_id TEXT NOT NULL,
                data_id TEXT NOT NULL,
                PRIMARY KEY (person_id, data_id),
                FOREIGN KEY (person_id) REFERENCES person_usage(person_id) ON DELETE CASCADE
             );
             CREATE TABLE IF NOT EXISTS service_account_records (
                person_id TEXT NOT NULL,
                data_id TEXT NOT NULL,
                PRIMARY KEY (person_id, data_id),
                FOREIGN KEY (person_id) REFERENCES person_usage(person_id) ON DELETE CASCADE
             );
             CREATE TABLE IF NOT EXISTS lost_key_name_tombstones (
                public_name TEXT PRIMARY KEY NOT NULL,
                locked_at INTEGER NOT NULL
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
        self.stop_account_at(person_id, now_unix_seconds())
    }

    /// Account-screen Delete account boundary. Only the exact, case-sensitive
    /// confirmation word dispatches the stopped-account command. Cancel,
    /// missing text, and every other spelling leave the account active.
    pub fn delete_account_at(
        &mut self,
        person_id: &str,
        choice: DeleteAccountChoice<'_>,
        stopped_at_unix_seconds: i64,
    ) -> Result<DeleteAccountOutcome> {
        match choice {
            DeleteAccountChoice::Cancel => Ok(DeleteAccountOutcome::Cancelled),
            DeleteAccountChoice::Confirm(DELETE_ACCOUNT_CONFIRMATION) => self
                .stop_account_at(person_id, stopped_at_unix_seconds)
                .map(DeleteAccountOutcome::Stopped),
            DeleteAccountChoice::Confirm(_) => Ok(DeleteAccountOutcome::ConfirmationRequired),
        }
    }

    /// Deterministic counterpart of [`Self::stop_account`]. Repeating the
    /// command preserves the first stop time.
    pub fn stop_account_at(
        &mut self,
        person_id: &str,
        stopped_at_unix_seconds: i64,
    ) -> Result<StoppedAccount> {
        validate_id(person_id, "person id")?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_person(&tx, person_id)?;
        tx.execute(
            "INSERT INTO stopped_accounts (person_id, stopped_at) VALUES (?1, ?2)
             ON CONFLICT(person_id) DO NOTHING",
            params![person_id, stopped_at_unix_seconds],
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

    /// Store one opaque service-data record for an active account. Files use
    /// the existing stored-file ledger; all kinds are idempotent by data id.
    pub fn store_service_data(
        &mut self,
        person_id: &str,
        kind: ServiceDataKind,
        data_id: &str,
    ) -> Result<()> {
        validate_id(person_id, "person id")?;
        validate_id(data_id, "service data id")?;
        if kind == ServiceDataKind::Files {
            return self.store_file(person_id, data_id, 1);
        }

        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        reject_if_stopped(&tx, person_id, AccountAction::Upload)?;
        ensure_person(&tx, person_id)?;
        let sql = format!(
            "INSERT INTO {} (person_id, data_id) VALUES (?1, ?2) \
             ON CONFLICT(person_id, data_id) DO NOTHING",
            kind.table()
        );
        tx.execute(&sql, params![person_id, data_id])?;
        tx.commit()?;
        Ok(())
    }

    /// Write through one independently inventoried shipping location. This is
    /// intentionally location-specific because the Messages class has two
    /// distinct writers: local messages and relay-held undelivered ciphertext.
    pub fn store_account_data_at_location(
        &mut self,
        person_id: &str,
        location: &str,
        data_id: &str,
    ) -> Result<()> {
        validate_id(person_id, "person id")?;
        validate_id(data_id, "service data id")?;
        let writer = SHIPPING_ACCOUNT_DATA_WRITERS
            .iter()
            .find(|writer| writer.location == location)
            .ok_or(UsageCounterError::Invalid("account data location"))?;
        if writer.kind == ServiceDataKind::Files {
            return self.store_file(person_id, data_id, 1);
        }

        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        reject_if_stopped(&tx, person_id, AccountAction::Upload)?;
        ensure_person(&tx, person_id)?;
        let sql = format!(
            "INSERT INTO {} (person_id, data_id) VALUES (?1, ?2) \
             ON CONFLICT(person_id, data_id) DO NOTHING",
            writer.location
        );
        tx.execute(&sql, params![person_id, data_id])?;
        tx.commit()?;
        Ok(())
    }

    /// Count only the named account's six service-data kinds.
    pub fn service_data_counts(&self, person_id: &str) -> Result<ServiceDataCounts> {
        validate_id(person_id, "person id")?;
        service_data_counts(&self.conn, person_id)
    }

    /// Record the one ruled non-account exception. It has no account id,
    /// identity key, recovery secret, or finite expiry and therefore cannot be
    /// reached by the stopped-account foreign-key cascade.
    pub fn store_lost_key_name_tombstone(
        &mut self,
        public_name: &str,
        locked_at_unix_seconds: i64,
    ) -> Result<()> {
        validate_id(public_name, "public name")?;
        self.conn.execute(
            "INSERT INTO lost_key_name_tombstones (public_name, locked_at)
             VALUES (?1, ?2) ON CONFLICT(public_name) DO NOTHING",
            params![public_name, locked_at_unix_seconds],
        )?;
        Ok(())
    }

    pub fn lost_key_name_tombstone_count(&self) -> Result<u64> {
        let count: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM lost_key_name_tombstones", [], |row| {
                    row.get(0)
                })?;
        as_u64(count)
    }

    /// Claim-path preflight for the permanent ruling 3120 reservation.
    pub fn claim_public_name(&self, public_name: &str) -> Result<()> {
        validate_id(public_name, "public name")?;
        let locked = self
            .conn
            .query_row(
                "SELECT 1 FROM lost_key_name_tombstones WHERE public_name = ?1",
                params![public_name],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if locked {
            return Err(UsageCounterError::LostKeyNamePermanentlyReserved(
                LOST_KEY_NAME_CLAIM_REFUSAL,
            ));
        }
        Ok(())
    }

    /// Purge every account that has been stopped for seven full days.
    pub fn purge_due_stopped_accounts(&mut self) -> Result<Vec<PurgedStoppedAccount>> {
        self.purge_due_stopped_accounts_at(now_unix_seconds())
    }

    /// Deterministic counterpart of [`Self::purge_due_stopped_accounts`]. The
    /// delete is one transaction and is scoped to ids selected by the cutoff.
    pub fn purge_due_stopped_accounts_at(
        &mut self,
        now_unix_seconds: i64,
    ) -> Result<Vec<PurgedStoppedAccount>> {
        let cutoff = now_unix_seconds
            .checked_sub(STOPPED_ACCOUNT_RETENTION_SECONDS)
            .ok_or(UsageCounterError::Overflow)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let person_ids = {
            let mut statement = tx.prepare(
                "SELECT person_id FROM stopped_accounts \
                 WHERE stopped_at <= ?1 ORDER BY person_id",
            )?;
            let selected = statement
                .query_map(params![cutoff], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            selected
        };

        let mut purged = Vec::with_capacity(person_ids.len());
        for person_id in person_ids {
            let deleted = service_data_counts(&tx, &person_id)?;
            tx.execute(
                "DELETE FROM person_usage WHERE person_id = ?1",
                params![person_id],
            )?;
            let remaining = service_data_counts(&tx, &person_id)?;
            let lost_key_name_tombstones: i64 =
                tx.query_row("SELECT COUNT(*) FROM lost_key_name_tombstones", [], |row| {
                    row.get(0)
                })?;
            purged.push(PurgedStoppedAccount {
                person_id,
                deleted,
                remaining,
                lost_key_name_tombstones: as_u64(lost_key_name_tombstones)?,
            });
        }
        tx.commit()?;
        Ok(purged)
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

fn service_data_counts(conn: &Connection, person_id: &str) -> Result<ServiceDataCounts> {
    let counts: (i64, i64, i64, i64, i64, i64) = conn.query_row(
        "SELECT
            ((SELECT COUNT(*) FROM service_messages WHERE person_id = ?1) +
             (SELECT COUNT(*) FROM relay_held_undelivered_ciphertexts WHERE person_id = ?1)),
            (SELECT COUNT(*) FROM stored_files WHERE person_id = ?1),
            (SELECT COUNT(*) FROM service_keys WHERE person_id = ?1),
            (SELECT COUNT(*) FROM service_settings WHERE person_id = ?1),
            (SELECT COUNT(*) FROM service_sessions WHERE person_id = ?1),
            (SELECT COUNT(*) FROM service_account_records WHERE person_id = ?1)",
        params![person_id],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            ))
        },
    )?;
    Ok(ServiceDataCounts {
        messages: as_u64(counts.0)?,
        files: as_u64(counts.1)?,
        keys: as_u64(counts.2)?,
        settings: as_u64(counts.3)?,
        sessions: as_u64(counts.4)?,
        account_records: as_u64(counts.5)?,
    })
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

#[cfg(test)]
mod task_3709_tests {
    use super::*;

    const MINUTE_SECONDS: i64 = 60;
    const STOPPED_AT: i64 = 30_000 * SECONDS_PER_DAY;
    const STOPPED_ACCOUNT: &str = "task-3709-stopped-account";
    const OTHER_ACCOUNT: &str = "task-3709-other-account";

    fn seed_two_of_each_kind(store: &mut UsageCounterStore, account: &str) {
        for kind in ServiceDataKind::ALL {
            for data_id in ["first", "second"] {
                store
                    .store_service_data(account, kind, data_id)
                    .expect("seed service-data item");
            }
        }
    }

    fn two_of_each_kind() -> ServiceDataCounts {
        ServiceDataCounts {
            messages: 2,
            files: 2,
            keys: 2,
            settings: 2,
            sessions: 2,
            account_records: 2,
        }
    }

    fn total(counts: ServiceDataCounts) -> u64 {
        counts.messages
            + counts.files
            + counts.keys
            + counts.settings
            + counts.sessions
            + counts.account_records
    }

    #[test]
    fn task_3709_keeps_twelve_items_until_after_the_stopped_account_deadline() {
        let dir = tempfile::tempdir().expect("temporary service-data directory");
        let mut store = UsageCounterStore::open(dir.path()).expect("open service-data store");

        seed_two_of_each_kind(&mut store, STOPPED_ACCOUNT);
        seed_two_of_each_kind(&mut store, OTHER_ACCOUNT);
        store
            .stop_account_at(STOPPED_ACCOUNT, STOPPED_AT)
            .expect("stop the account at the test clock");

        let stopped_before = store
            .service_data_counts(STOPPED_ACCOUNT)
            .expect("count stopped account before cleanup");
        let other_before = store
            .service_data_counts(OTHER_ACCOUNT)
            .expect("count other account before cleanup");
        assert_eq!(stopped_before, two_of_each_kind());
        assert_eq!(other_before, two_of_each_kind());
        assert_eq!(total(stopped_before), 12, "stopped total before cleanup");

        let deadline = STOPPED_AT + STOPPED_ACCOUNT_RETENTION_DAYS * SECONDS_PER_DAY;
        let early_purge = store
            .purge_due_stopped_accounts_at(deadline - MINUTE_SECONDS)
            .expect("run cleanup one minute before the deadline");
        assert!(
            early_purge.is_empty(),
            "cleanup must not purge before the deadline"
        );
        let stopped_one_minute_before = store
            .service_data_counts(STOPPED_ACCOUNT)
            .expect("count stopped account one minute before deadline");
        assert_eq!(stopped_one_minute_before, two_of_each_kind());
        assert_eq!(
            total(stopped_one_minute_before),
            12,
            "stopped total one minute before deadline"
        );

        let late_purge = store
            .purge_due_stopped_accounts_at(deadline + MINUTE_SECONDS)
            .expect("run cleanup one minute after the deadline");
        assert_eq!(late_purge.len(), 1, "exactly the due account is purged");
        assert_eq!(late_purge[0].person_id, STOPPED_ACCOUNT);
        assert_eq!(late_purge[0].deleted, two_of_each_kind());
        assert_eq!(total(late_purge[0].deleted), 12, "purge deleted total");

        let stopped_one_minute_after = store
            .service_data_counts(STOPPED_ACCOUNT)
            .expect("count stopped account one minute after deadline");
        let other_after = store
            .service_data_counts(OTHER_ACCOUNT)
            .expect("count other account after both cleanup runs");
        assert_eq!(stopped_one_minute_after, ServiceDataCounts::default());
        assert_eq!(other_after, two_of_each_kind());
        assert_eq!(
            total(stopped_one_minute_after),
            0,
            "stopped total one minute after deadline"
        );
        assert_eq!(total(other_after), 12, "other account total after cleanup");

        println!(
            "TASK3709 before={} one_minute_before={} one_minute_after={} other={}",
            total(stopped_before),
            total(stopped_one_minute_before),
            total(stopped_one_minute_after),
            total(other_after),
        );
    }
}
