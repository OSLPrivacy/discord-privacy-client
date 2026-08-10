//! The on-device byte-meter record.
//!
//! TASK 4601 chose anonymous vouchers rather than a server-side per-account
//! ledger. This DTO therefore carries only local arithmetic: a calendar month,
//! a byte count, one closed byte class, and an opaque source id. Strict serde
//! decoding prevents message text, file names, account ids, or other fields
//! from being smuggled into the record.

use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

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

/// Every shipping byte-producing path covered by ruling A7.
///
/// Voice intentionally is not a variant here: no voice client ships in this
/// release. It remains a required byte class (and therefore a visible zero in
/// totals) so adding voice later cannot silently bypass allowance accounting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MeteredSendPath {
    HeldMessageBytes,
    Attachments,
    StoryAndPostMedia,
    MultiDeviceSyncTraffic,
    BackgroundCoverTick,
    PlainText,
}

const METERED_SEND_PATH_HOOKS: [(MeteredSendPath, Option<MeteredByteClass>); 6] = [
    (
        MeteredSendPath::HeldMessageBytes,
        Some(MeteredByteClass::Messages),
    ),
    (
        MeteredSendPath::Attachments,
        Some(MeteredByteClass::Attachments),
    ),
    (
        MeteredSendPath::StoryAndPostMedia,
        Some(MeteredByteClass::StoriesAndPosts),
    ),
    (
        MeteredSendPath::MultiDeviceSyncTraffic,
        Some(MeteredByteClass::MultiDeviceSync),
    ),
    (
        MeteredSendPath::BackgroundCoverTick,
        Some(MeteredByteClass::BackgroundConnection),
    ),
    (MeteredSendPath::PlainText, Some(MeteredByteClass::Messages)),
];

impl MeteredSendPath {
    pub const ALL: [Self; 6] = [
        Self::HeldMessageBytes,
        Self::Attachments,
        Self::StoryAndPostMedia,
        Self::MultiDeviceSyncTraffic,
        Self::BackgroundCoverTick,
        Self::PlainText,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::HeldMessageBytes => "held message bytes",
            Self::Attachments => "attachments",
            Self::StoryAndPostMedia => "story and post media",
            Self::MultiDeviceSyncTraffic => "multi-device sync traffic",
            Self::BackgroundCoverTick => "constant background cover tick",
            Self::PlainText => "plain text",
        }
    }

    pub fn byte_class(self) -> Option<MeteredByteClass> {
        METERED_SEND_PATH_HOOKS
            .into_iter()
            .find_map(|(path, byte_class)| (path == self).then_some(byte_class))
            .flatten()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MissingByteClassHook {
    pub path: MeteredSendPath,
}

impl fmt::Display for MissingByteClassHook {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "metered send path has no byte-class hook: {}",
            self.path.name()
        )
    }
}

impl std::error::Error for MissingByteClassHook {}

/// Production enables all hooks. `without` is a fault-injection seam proving
/// that each class contributes exactly its measured bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeteredByteHooks {
    enabled: [bool; 6],
}

impl MeteredByteHooks {
    pub const fn all() -> Self {
        Self { enabled: [true; 6] }
    }

    pub fn without(mut self, byte_class: MeteredByteClass) -> Self {
        self.enabled[byte_class.index()] = false;
        self
    }

    pub const fn is_enabled(self, byte_class: MeteredByteClass) -> bool {
        self.enabled[byte_class.index()]
    }
}

impl Default for MeteredByteHooks {
    fn default() -> Self {
        Self::all()
    }
}

/// The on-device meter before any allowance top-up arithmetic is applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeteredByteMeter {
    hooks: MeteredByteHooks,
    records: Vec<MeteredByteRecord>,
}

impl Default for MeteredByteMeter {
    fn default() -> Self {
        Self::new()
    }
}

impl MeteredByteMeter {
    pub fn new() -> Self {
        Self::with_hooks(MeteredByteHooks::all())
    }

    pub fn with_hooks(hooks: MeteredByteHooks) -> Self {
        Self {
            hooks,
            records: Vec::new(),
        }
    }

    pub fn record_send(
        &mut self,
        month: impl Into<String>,
        path: MeteredSendPath,
        byte_count: u64,
        source_id: impl Into<String>,
    ) -> Result<bool, MissingByteClassHook> {
        let byte_class = path.byte_class().ok_or(MissingByteClassHook { path })?;
        if !self.hooks.is_enabled(byte_class) {
            return Ok(false);
        }
        self.records.push(MeteredByteRecord::new(
            month, byte_count, byte_class, source_id,
        ));
        Ok(true)
    }

    /// Six rows are always returned, including zero-valued absent features.
    pub fn class_totals(&self) -> Vec<(MeteredByteClass, u64)> {
        let mut totals = [0_u64; 6];
        for record in &self.records {
            totals[record.byte_class.index()] = totals[record.byte_class.index()]
                .checked_add(record.byte_count)
                .expect("metered byte total overflow");
        }
        MeteredByteClass::ALL
            .into_iter()
            .map(|byte_class| (byte_class, totals[byte_class.index()]))
            .collect()
    }

    pub fn total_before_top_ups(&self) -> u64 {
        self.class_totals()
            .into_iter()
            .map(|(_, bytes)| bytes)
            .sum()
    }

    pub fn records(&self) -> &[MeteredByteRecord] {
        &self.records
    }
}

pub fn unhooked_metered_send_path_count() -> usize {
    MeteredSendPath::ALL
        .into_iter()
        .filter(|path| path.byte_class().is_none())
        .count()
}
