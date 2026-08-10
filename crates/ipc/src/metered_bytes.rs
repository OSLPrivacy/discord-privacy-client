//! The on-device byte-meter record.
//!
//! TASK 4601 chose anonymous vouchers rather than a server-side per-account
//! ledger. This DTO therefore carries only local arithmetic: a calendar month,
//! a byte count, one closed byte class, and an opaque source id. Strict serde
//! decoding prevents message text, file names, account ids, or other fields
//! from being smuggled into the record.

use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};

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
