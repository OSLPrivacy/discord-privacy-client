//! The on-device byte-meter record.
//!
//! TASK 4601 chose anonymous vouchers rather than a server-side per-account
//! ledger. This DTO therefore carries only local arithmetic: a calendar month,
//! a byte count, one closed byte class, and an opaque source id. Strict serde
//! decoding prevents message text, file names, account ids, or other fields
//! from being smuggled into the record.

use rusqlite::{params, Connection};
use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};
use std::path::Path;

// Keep the persisted byte-class vocabulary in one canonical definition. The
// enum, serializer, parser, fixtures, and later metering hooks all derive their
// names from this array.
const BYTE_CLASS_NAMES: [&str; 6] = [
    "background connection",
    "messages",
    "attachments",
    "stories and posts",
    "voice",
    "multi-device sync",
];

/// The complete, closed set of traffic classes counted by the on-device meter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MeteredByteClass {
    BackgroundConnection,
    Messages,
    Attachments,
    StoriesAndPosts,
    Voice,
    MultiDeviceSync,
}

impl MeteredByteClass {
    /// Every class, in the stable order used by itemised meter displays.
    pub const ALL: [Self; 6] = [
        Self::BackgroundConnection,
        Self::Messages,
        Self::Attachments,
        Self::StoriesAndPosts,
        Self::Voice,
        Self::MultiDeviceSync,
    ];

    /// The stable persisted and displayed name of this class.
    pub const fn name(self) -> &'static str {
        BYTE_CLASS_NAMES[self.index()]
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|byte_class| byte_class.name() == name)
    }

    const fn index(self) -> usize {
        match self {
            Self::BackgroundConnection => 0,
            Self::Messages => 1,
            Self::Attachments => 2,
            Self::StoriesAndPosts => 3,
            Self::Voice => 4,
            Self::MultiDeviceSync => 5,
        }
    }
}

impl Serialize for MeteredByteClass {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.name())
    }
}

impl<'de> Deserialize<'de> for MeteredByteClass {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let name = String::deserialize(deserializer)?;
        Self::from_name(&name).ok_or_else(|| D::Error::unknown_variant(&name, &BYTE_CLASS_NAMES))
    }
}

/// One contribution to the person's on-device monthly byte arithmetic.
///
/// `byte_count` is unsigned, so negative JSON input is rejected by serde. The
/// strict four-field shape is intentional: payload text and identifying file
/// metadata do not belong in usage accounting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeteredByteRecord {
    pub month: String,
    pub byte_count: u64,
    pub byte_class: MeteredByteClass,
    pub source_id: String,
}

impl MeteredByteRecord {
    pub fn new(
        month: impl Into<String>,
        byte_count: u64,
        byte_class: MeteredByteClass,
        source_id: impl Into<String>,
    ) -> Self {
        Self {
            month: month.into(),
            byte_count,
            byte_class,
            source_id: source_id.into(),
        }
    }
}

/// The measured byte counters at a release client's media-interface boundary.
///
/// These are deliberately byte counters, not duration or codec estimates. A
/// caller snapshots this boundary immediately before and after a call, then
/// records the two deltas in the same allowance pool as every other class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoiceInterfaceCounters {
    pub sent_bytes: u64,
    pub received_bytes: u64,
}

/// The only counter source accepted by the allowance writer. Keeping the
/// source explicit makes a fixture or a guessed bitrate unable to look like a
/// release-client observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceCounterSource {
    ReleaseClientMediaInterface,
    Synthetic,
}

/// A voice accounting refusal identifies the missing side rather than silently
/// putting a zero-valued Voice row into the meter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceAccountingError {
    MissingSentBytes,
    MissingReceivedBytes,
    SyntheticCounters,
    CounterWentBackwards,
    Storage(String),
}

impl std::fmt::Display for VoiceAccountingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::MissingSentBytes => "missing sent bytes",
            Self::MissingReceivedBytes => "missing received bytes",
            Self::SyntheticCounters => "synthetic counters are not accepted for voice accounting",
            Self::CounterWentBackwards => "voice interface counter went backwards",
            Self::Storage(error) => error,
        })
    }
}

impl std::error::Error for VoiceAccountingError {}

/// A persistent monthly allowance ledger. Its rows are byte contributions and
/// its total is always calculated from the rows; there is intentionally no
/// separate voice-minute counter or cached voice total to drift after restart.
pub struct MonthlyAllowanceStore {
    connection: Connection,
}

impl MonthlyAllowanceStore {
    pub fn open(directory: impl AsRef<Path>) -> Result<Self, VoiceAccountingError> {
        let path = directory.as_ref().join("monthly_allowance.sqlite");
        let connection = Connection::open(path)
            .map_err(|error| VoiceAccountingError::Storage(error.to_string()))?;
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS monthly_allowance_records (
                    month TEXT NOT NULL,
                    source_id TEXT NOT NULL,
                    byte_class TEXT NOT NULL,
                    byte_count INTEGER NOT NULL CHECK(byte_count >= 0),
                    PRIMARY KEY (month, source_id)
                );",
            )
            .map_err(|error| VoiceAccountingError::Storage(error.to_string()))?;
        Ok(Self { connection })
    }

    /// Record a general byte contribution. A source id is idempotent so a
    /// reconnect cannot double-charge a person's allowance.
    pub fn record(&self, record: &MeteredByteRecord) -> Result<(), VoiceAccountingError> {
        self.connection
            .execute(
                "INSERT INTO monthly_allowance_records (month, source_id, byte_class, byte_count)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(month, source_id) DO UPDATE SET
                   byte_class = excluded.byte_class, byte_count = excluded.byte_count",
                params![
                    record.month,
                    record.source_id,
                    record.byte_class.name(),
                    i64::try_from(record.byte_count).map_err(|_| VoiceAccountingError::Storage(
                        "byte count exceeds SQLite integer range".into()
                    ))?,
                ],
            )
            .map_err(|error| VoiceAccountingError::Storage(error.to_string()))?;
        Ok(())
    }

    /// Compute the sent-plus-received media-interface delta and persist it as
    /// the one Voice row for this call. Both sides must move: an absent
    /// transmit or receive observation is a failure, never a 0-byte success.
    pub fn record_voice_call(
        &self,
        month: impl Into<String>,
        call_id: impl AsRef<str>,
        before: VoiceInterfaceCounters,
        after: VoiceInterfaceCounters,
        source: VoiceCounterSource,
    ) -> Result<MeteredByteRecord, VoiceAccountingError> {
        if source != VoiceCounterSource::ReleaseClientMediaInterface {
            return Err(VoiceAccountingError::SyntheticCounters);
        }
        let sent = after
            .sent_bytes
            .checked_sub(before.sent_bytes)
            .ok_or(VoiceAccountingError::CounterWentBackwards)?;
        let received = after
            .received_bytes
            .checked_sub(before.received_bytes)
            .ok_or(VoiceAccountingError::CounterWentBackwards)?;
        if sent == 0 {
            return Err(VoiceAccountingError::MissingSentBytes);
        }
        if received == 0 {
            return Err(VoiceAccountingError::MissingReceivedBytes);
        }
        let bytes = sent
            .checked_add(received)
            .ok_or_else(|| VoiceAccountingError::Storage("voice byte total overflow".into()))?;
        let record =
            MeteredByteRecord::new(month, bytes, MeteredByteClass::Voice, call_id.as_ref());
        self.record(&record)?;
        Ok(record)
    }

    /// The six stable Data-this-month rows, including an absent feature's zero.
    pub fn itemised_rows(
        &self,
        month: &str,
    ) -> Result<Vec<(MeteredByteClass, u64)>, VoiceAccountingError> {
        let mut totals = [0_u64; 6];
        let mut statement = self
            .connection
            .prepare(
                "SELECT byte_class, byte_count FROM monthly_allowance_records WHERE month = ?1",
            )
            .map_err(|error| VoiceAccountingError::Storage(error.to_string()))?;
        let rows = statement
            .query_map([month], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|error| VoiceAccountingError::Storage(error.to_string()))?;
        for row in rows {
            let (name, bytes) =
                row.map_err(|error| VoiceAccountingError::Storage(error.to_string()))?;
            let class = MeteredByteClass::from_name(&name).ok_or_else(|| {
                VoiceAccountingError::Storage(format!("unknown persisted byte class {name}"))
            })?;
            let bytes = u64::try_from(bytes).map_err(|_| {
                VoiceAccountingError::Storage("negative persisted byte count".into())
            })?;
            totals[class.index()] = totals[class.index()].checked_add(bytes).ok_or_else(|| {
                VoiceAccountingError::Storage("monthly byte total overflow".into())
            })?;
        }
        Ok(MeteredByteClass::ALL
            .into_iter()
            .map(|class| (class, totals[class.index()]))
            .collect())
    }

    pub fn total(&self, month: &str) -> Result<u64, VoiceAccountingError> {
        self.itemised_rows(month)?
            .into_iter()
            .try_fold(0_u64, |total, (_, bytes)| {
                total.checked_add(bytes).ok_or_else(|| {
                    VoiceAccountingError::Storage("monthly byte total overflow".into())
                })
            })
    }
}
