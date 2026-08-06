//! Stable-place authorization for hosted Scrub-visible app actions.
//!
//! Provider adapters may read, draft, or place items only inside stable place
//! IDs the owner marked as allowed. Refusals happen before any action record is
//! appended, so an unlisted place stays silent.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostedAllowedPlaceRecord {
    pub app: String,
    pub stable_place_id: String,
    pub exact_place_name: String,
    pub source_task: u32,
}

impl HostedAllowedPlaceRecord {
    pub fn new(
        app: impl Into<String>,
        stable_place_id: impl Into<String>,
        exact_place_name: impl Into<String>,
        source_task: u32,
    ) -> Self {
        Self {
            app: app.into(),
            stable_place_id: stable_place_id.into(),
            exact_place_name: exact_place_name.into(),
            source_task,
        }
    }

    pub fn fingerprint(&self) -> String {
        let mut hasher = Sha256::new();
        write_len_prefixed(&mut hasher, self.app.as_bytes());
        write_len_prefixed(&mut hasher, self.stable_place_id.as_bytes());
        write_len_prefixed(&mut hasher, self.exact_place_name.as_bytes());
        hasher.update(self.source_task.to_be_bytes());
        hex(&hasher.finalize())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostedPlaceList {
    allowed_places: BTreeMap<(String, String), HostedAllowedPlaceRecord>,
}

impl HostedPlaceList {
    pub fn new(records: impl IntoIterator<Item = HostedAllowedPlaceRecord>) -> Self {
        Self {
            allowed_places: records
                .into_iter()
                .map(|record| ((record.app.clone(), record.stable_place_id.clone()), record))
                .collect(),
        }
    }

    pub fn is_readable(&self, app: &str, stable_place_id: &str) -> bool {
        self.allowed_places
            .contains_key(&(app.to_owned(), stable_place_id.to_owned()))
    }

    pub fn readable_count(&self) -> usize {
        self.allowed_places
            .values()
            .filter(|record| self.is_readable(&record.app, &record.stable_place_id))
            .count()
    }

    pub fn record_fingerprints(&self) -> Vec<String> {
        self.allowed_places
            .values()
            .map(HostedAllowedPlaceRecord::fingerprint)
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostedPlaceActionRequest {
    pub app: String,
    pub stable_place_id: String,
    pub item_id: String,
}

impl HostedPlaceActionRequest {
    pub fn new(
        app: impl Into<String>,
        stable_place_id: impl Into<String>,
        item_id: impl Into<String>,
    ) -> Self {
        Self {
            app: app.into(),
            stable_place_id: stable_place_id.into(),
            item_id: item_id.into(),
        }
    }

    pub fn with_stable_place_id(&self, stable_place_id: impl Into<String>) -> Self {
        Self {
            app: self.app.clone(),
            stable_place_id: stable_place_id.into(),
            item_id: self.item_id.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostedPlaceActionKind {
    Read,
    Draft,
    PlacedItem,
}

impl HostedPlaceActionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Draft => "draft",
            Self::PlacedItem => "placed item",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostedPlaceActionRecord {
    pub app: String,
    pub stable_place_id: String,
    pub item_id: String,
    pub kind: HostedPlaceActionKind,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct HostedPlaceActionLog {
    records: Vec<HostedPlaceActionRecord>,
}

impl HostedPlaceActionLog {
    pub fn action_count(&self) -> usize {
        self.records.len()
    }

    pub fn records(&self) -> &[HostedPlaceActionRecord] {
        &self.records
    }

    fn push(&mut self, request: &HostedPlaceActionRequest, kind: HostedPlaceActionKind) {
        self.records.push(HostedPlaceActionRecord {
            app: request.app.clone(),
            stable_place_id: request.stable_place_id.clone(),
            item_id: request.item_id.clone(),
            kind,
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostedPlaceActionError {
    PlaceNotAllowed,
}

impl fmt::Display for HostedPlaceActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PlaceNotAllowed => f.write_str("place not allowed"),
        }
    }
}

impl std::error::Error for HostedPlaceActionError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostedPlaceRead {
    pub app: String,
    pub stable_place_id: String,
    pub item_id: String,
    pub readable: bool,
}

pub fn read_hosted_place(
    places: &HostedPlaceList,
    log: &mut HostedPlaceActionLog,
    request: &HostedPlaceActionRequest,
) -> Result<HostedPlaceRead, HostedPlaceActionError> {
    authorize_place(places, request)?;
    log.push(request, HostedPlaceActionKind::Read);
    Ok(HostedPlaceRead {
        app: request.app.clone(),
        stable_place_id: request.stable_place_id.clone(),
        item_id: request.item_id.clone(),
        readable: true,
    })
}

pub fn prepare_hosted_place_draft(
    places: &HostedPlaceList,
    log: &mut HostedPlaceActionLog,
    request: &HostedPlaceActionRequest,
) -> Result<HostedPlaceActionRecord, HostedPlaceActionError> {
    record_action(places, log, request, HostedPlaceActionKind::Draft)
}

pub fn place_hosted_item(
    places: &HostedPlaceList,
    log: &mut HostedPlaceActionLog,
    request: &HostedPlaceActionRequest,
) -> Result<HostedPlaceActionRecord, HostedPlaceActionError> {
    record_action(places, log, request, HostedPlaceActionKind::PlacedItem)
}

fn record_action(
    places: &HostedPlaceList,
    log: &mut HostedPlaceActionLog,
    request: &HostedPlaceActionRequest,
    kind: HostedPlaceActionKind,
) -> Result<HostedPlaceActionRecord, HostedPlaceActionError> {
    authorize_place(places, request)?;
    log.push(request, kind);
    Ok(log.records.last().expect("just pushed action").clone())
}

fn authorize_place(
    places: &HostedPlaceList,
    request: &HostedPlaceActionRequest,
) -> Result<(), HostedPlaceActionError> {
    if places.is_readable(&request.app, &request.stable_place_id) {
        Ok(())
    } else {
        Err(HostedPlaceActionError::PlaceNotAllowed)
    }
}

fn write_len_prefixed(hasher: &mut Sha256, value: &[u8]) {
    hasher.update(value.len().to_be_bytes());
    hasher.update(value);
}

fn hex(bytes: &[u8]) -> String {
    const TABLE: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(TABLE[(byte >> 4) as usize] as char);
        output.push(TABLE[(byte & 0x0f) as usize] as char);
    }
    output
}
