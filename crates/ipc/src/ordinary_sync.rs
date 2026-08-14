use crate::wire_v2::{encrypt_v3, RecipientV3, V2Error, MSG_TYPE_CONTENT, WIRE_VERSION_V3};
use crypto::x25519;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Server-visible message kinds added for ordinary cross-device sync.
///
/// Sync records are encrypted as ordinary content messages. Any sync-specific
/// routing or payload kind lives inside the encrypted body.
pub const ADDED_SERVER_VISIBLE_MESSAGE_KINDS: [u8; 0] = [];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionStamp {
    pub counter: u64,
    pub device_id: String,
}

impl VersionStamp {
    pub fn new(counter: u64, device_id: impl Into<String>) -> Self {
        Self {
            counter,
            device_id: device_id.into(),
        }
    }
}

impl Ord for VersionStamp {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.counter
            .cmp(&other.counter)
            .then_with(|| self.device_id.cmp(&other.device_id))
    }
}

impl PartialOrd for VersionStamp {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListRow {
    pub row_id: String,
    pub value: Value,
    pub stamp: VersionStamp,
}

impl ListRow {
    pub fn new(row_id: impl Into<String>, value: Value, stamp: VersionStamp) -> Self {
        Self {
            row_id: row_id.into(),
            value,
            stamp,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteMarker {
    pub row_id: String,
    pub stamp: VersionStamp,
}

impl DeleteMarker {
    pub fn new(row_id: impl Into<String>, stamp: VersionStamp) -> Self {
        Self {
            row_id: row_id.into(),
            stamp,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct OrdinaryListState {
    rows: BTreeMap<String, ListRow>,
    delete_markers: BTreeMap<String, DeleteMarker>,
}

impl OrdinaryListState {
    pub fn from_rows(rows: impl IntoIterator<Item = ListRow>) -> Self {
        let mut state = Self::default();
        for row in rows {
            state.apply(ListChange::Upsert(row));
        }
        state
    }

    pub fn apply(&mut self, change: ListChange) {
        match change {
            ListChange::Upsert(row) => {
                if self.delete_markers.contains_key(&row.row_id) {
                    return;
                }
                self.rows
                    .entry(row.row_id.clone())
                    .and_modify(|current| {
                        if row.stamp > current.stamp {
                            *current = row.clone();
                        }
                    })
                    .or_insert(row);
            }
            ListChange::Delete(marker) => {
                self.rows.remove(&marker.row_id);
                self.delete_markers
                    .entry(marker.row_id.clone())
                    .and_modify(|current| {
                        if marker.stamp > current.stamp {
                            *current = marker.clone();
                        }
                    })
                    .or_insert(marker);
            }
        }
    }

    pub fn merge(&self, other: &Self) -> Self {
        let mut merged = self.clone();
        for row in other.rows.values().cloned() {
            merged.apply(ListChange::Upsert(row));
        }
        for marker in other.delete_markers.values().cloned() {
            merged.apply(ListChange::Delete(marker));
        }
        merged
    }

    pub fn live_row_count(&self) -> usize {
        self.rows.len()
    }

    pub fn delete_marker_count(&self) -> usize {
        self.delete_markers.len()
    }

    pub fn contains_row(&self, row_id: &str) -> bool {
        self.rows.contains_key(row_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ListChange {
    Upsert(ListRow),
    Delete(DeleteMarker),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldValue {
    pub value: Value,
    pub stamp: VersionStamp,
}

impl FieldValue {
    pub fn new(value: Value, stamp: VersionStamp) -> Self {
        Self { value, stamp }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FieldWiseState {
    fields: BTreeMap<String, FieldValue>,
}

impl FieldWiseState {
    pub fn from_fields(fields: impl IntoIterator<Item = (String, FieldValue)>) -> Self {
        Self {
            fields: fields.into_iter().collect(),
        }
    }

    pub fn set_field(&mut self, name: impl Into<String>, value: Value, stamp: VersionStamp) {
        let name = name.into();
        self.fields
            .entry(name)
            .and_modify(|current| {
                if stamp > current.stamp {
                    *current = FieldValue::new(value.clone(), stamp.clone());
                }
            })
            .or_insert_with(|| FieldValue::new(value, stamp));
    }

    pub fn merge(&self, other: &Self) -> Self {
        let mut merged = self.clone();
        for (name, field) in &other.fields {
            merged.set_field(name.clone(), field.value.clone(), field.stamp.clone());
        }
        merged
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        self.fields.get(name).map(|field| &field.value)
    }
    pub fn field_count(&self) -> usize {
        self.fields.len()
    }

    pub fn reverted_field_count(&self, expected: &BTreeMap<String, Value>) -> usize {
        expected
            .iter()
            .filter(|(name, value)| self.get(name.as_str()) != Some(*value))
            .count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrdinarySyncPayload {
    pub source_device_id: String,
    pub target_device_id: String,
    pub body: Value,
}

impl OrdinarySyncPayload {
    pub fn new(
        source_device_id: impl Into<String>,
        target_device_id: impl Into<String>,
        body: Value,
    ) -> Self {
        Self {
            source_device_id: source_device_id.into(),
            target_device_id: target_device_id.into(),
            body,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityMergeRuleKind {
    SaferWins,
}

impl SecurityMergeRuleKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SaferWins => "safer-wins",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecurityMergeRule {
    pub field_name: &'static str,
    pub safer_value: &'static str,
    pub more_access_value: &'static str,
    pub kind: SecurityMergeRuleKind,
}

pub const SECURITY_MERGE_RULES: [SecurityMergeRule; 4] = [
    SecurityMergeRule {
        field_name: "permission_state",
        safer_value: "revoked",
        more_access_value: "allowed",
        kind: SecurityMergeRuleKind::SaferWins,
    },
    SecurityMergeRule {
        field_name: "identity_verification",
        safer_value: "unverified",
        more_access_value: "verified",
        kind: SecurityMergeRuleKind::SaferWins,
    },
    SecurityMergeRule {
        field_name: "person_block",
        safer_value: "blocked",
        more_access_value: "unblocked",
        kind: SecurityMergeRuleKind::SaferWins,
    },
    SecurityMergeRule {
        field_name: "request_disposition",
        safer_value: "refused",
        more_access_value: "permitted",
        kind: SecurityMergeRuleKind::SaferWins,
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityFieldValue {
    pub field_name: String,
    pub value: String,
    pub stamp: VersionStamp,
}

impl SecurityFieldValue {
    pub fn new(
        field_name: impl Into<String>,
        value: impl Into<String>,
        stamp: VersionStamp,
    ) -> Self {
        Self {
            field_name: field_name.into(),
            value: value.into(),
            stamp,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerVisibleHeader {
    pub wire_version: u8,
    pub message_kind: u8,
    pub sender_x25519: [u8; 32],
    pub recipient_count: u8,
    pub header_bytes: usize,
}

pub fn encrypt_sync_payload_v3(
    payload: &OrdinarySyncPayload,
    sender_ik_sk: &x25519::SecretKey,
    sender_ik_pub: &x25519::PublicKey,
    recipients: &[RecipientV3],
) -> Result<String, V2Error> {
    let plaintext = serde_json::to_vec(payload)
        .map_err(|err| V2Error::Crypto(format!("ordinary sync serialize: {err}")))?;
    encrypt_v3(
        sender_ik_sk,
        sender_ik_pub,
        recipients,
        MSG_TYPE_CONTENT,
        &plaintext,
    )
}

pub fn server_visible_v3_header(wire: &str) -> Result<ServerVisibleHeader, V2Error> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine as _;

    let raw = STANDARD
        .decode(wire.strip_prefix("DPC0::").ok_or(V2Error::BadPrefix)?)
        .map_err(|err| V2Error::Base64(err.to_string()))?;
    if raw.len() < 35 {
        return Err(V2Error::TooShort {
            got: raw.len(),
            expected: 35,
        });
    }
    if raw[0] != WIRE_VERSION_V3 {
        return Err(V2Error::WrongVersion {
            got: raw[0],
            expected: WIRE_VERSION_V3,
        });
    }
    let mut sender_x25519 = [0u8; 32];
    sender_x25519.copy_from_slice(&raw[2..34]);
    let recipient_count = raw[34];
    Ok(ServerVisibleHeader {
        wire_version: raw[0],
        message_kind: raw[1],
        sender_x25519,
        recipient_count,
        header_bytes: 35 + usize::from(recipient_count) * crate::wire_v2::SLOT_V3_BYTES,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityMergeDecision {
    pub field_name: String,
    pub rule: SecurityMergeRuleKind,
    pub winner: String,
}

impl SecurityMergeDecision {
    pub fn route_name(&self) -> &'static str {
        self.rule.as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityMergeResult {
    pub merged: BTreeMap<String, SecurityFieldValue>,
    pub decisions: Vec<SecurityMergeDecision>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecurityMergeError {
    MissingRule { field_name: String },
    UnknownValue { field_name: String, value: String },
}

impl std::fmt::Display for SecurityMergeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingRule { field_name } => {
                write!(f, "missing security merge rule for field {field_name}")
            }
            Self::UnknownValue { field_name, value } => {
                write!(f, "unknown security value {value} for field {field_name}")
            }
        }
    }
}

impl std::error::Error for SecurityMergeError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityFieldCoverage {
    pub field_name: String,
    pub route: SecurityMergeRuleKind,
}

pub fn check_security_field_coverage(
    field_names: impl IntoIterator<Item = impl Into<String>>,
) -> Result<Vec<SecurityFieldCoverage>, SecurityMergeError> {
    let mut fields: Vec<String> = field_names.into_iter().map(Into::into).collect();
    fields.sort();
    fields.dedup();

    let rules: BTreeSet<&str> = SECURITY_MERGE_RULES
        .iter()
        .map(|rule| rule.field_name)
        .collect();

    let mut coverage = Vec::with_capacity(fields.len());
    for field_name in fields {
        if !rules.contains(field_name.as_str()) {
            return Err(SecurityMergeError::MissingRule { field_name });
        }
        coverage.push(SecurityFieldCoverage {
            field_name,
            route: SecurityMergeRuleKind::SaferWins,
        });
    }
    Ok(coverage)
}

pub fn merge_security_fields(
    left: impl IntoIterator<Item = SecurityFieldValue>,
    right: impl IntoIterator<Item = SecurityFieldValue>,
) -> Result<SecurityMergeResult, SecurityMergeError> {
    let mut by_field: BTreeMap<String, Vec<SecurityFieldValue>> = BTreeMap::new();
    for field in left.into_iter().chain(right) {
        by_field
            .entry(field.field_name.clone())
            .or_default()
            .push(field);
    }
    check_security_field_coverage(by_field.keys().cloned())?;

    let mut merged = BTreeMap::new();
    let mut decisions = Vec::new();
    for (field_name, values) in by_field {
        let rule = SECURITY_MERGE_RULES
            .iter()
            .find(|rule| rule.field_name == field_name)
            .expect("coverage checked");
        let mut winning_value: Option<SecurityFieldValue> = None;
        for value in values {
            validate_security_value(rule, &value.value)?;
            if winning_value
                .as_ref()
                .is_none_or(|winner| security_value_is_safer(rule, &value, winner))
            {
                winning_value = Some(value);
            }
        }
        let winner = winning_value.expect("field has at least one value");
        decisions.push(SecurityMergeDecision {
            field_name: field_name.clone(),
            rule: rule.kind,
            winner: winner.value.clone(),
        });
        merged.insert(field_name, winner);
    }
    Ok(SecurityMergeResult { merged, decisions })
}

fn validate_security_value(
    rule: &SecurityMergeRule,
    value: &str,
) -> Result<(), SecurityMergeError> {
    if value == rule.safer_value || value == rule.more_access_value {
        return Ok(());
    }
    Err(SecurityMergeError::UnknownValue {
        field_name: rule.field_name.to_owned(),
        value: value.to_owned(),
    })
}

fn security_value_is_safer(
    rule: &SecurityMergeRule,
    candidate: &SecurityFieldValue,
    current: &SecurityFieldValue,
) -> bool {
    if candidate.value == rule.safer_value && current.value != rule.safer_value {
        return true;
    }
    if candidate.value != rule.safer_value && current.value == rule.safer_value {
        return false;
    }
    candidate.stamp > current.stamp
}
