//! Stable-place authorization for hosted Scrub-visible actions.
//!
//! A provider adapter may read, draft, place, or scrub only inside the stable
//! place IDs the owner approved. Refusals happen before any action record is
//! appended so an unlisted place stays silent.

use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostedPlaceList {
    allowed_place_ids: BTreeSet<String>,
}

impl HostedPlaceList {
    pub fn new(allowed_place_ids: impl IntoIterator<Item = String>) -> Self {
        Self {
            allowed_place_ids: allowed_place_ids.into_iter().collect(),
        }
    }

    pub fn is_readable(&self, stable_place_id: &str) -> bool {
        self.allowed_place_ids.contains(stable_place_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostedPlaceActionRequest {
    pub stable_place_id: String,
    pub item_id: String,
}

impl HostedPlaceActionRequest {
    pub fn new(stable_place_id: impl Into<String>, item_id: impl Into<String>) -> Self {
        Self {
            stable_place_id: stable_place_id.into(),
            item_id: item_id.into(),
        }
    }

    pub fn with_stable_place_id(&self, stable_place_id: impl Into<String>) -> Self {
        Self {
            stable_place_id: stable_place_id.into(),
            item_id: self.item_id.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostedPlaceActionKind {
    Read,
    Draft,
    SentItem,
    ScrubItem,
}

impl HostedPlaceActionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Draft => "draft",
            Self::SentItem => "sent item",
            Self::ScrubItem => "Scrub item",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostedPlaceActionRecord {
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

    pub fn action_names(&self) -> Vec<&'static str> {
        self.records
            .iter()
            .map(|record| record.kind.as_str())
            .collect()
    }

    pub fn records(&self) -> &[HostedPlaceActionRecord] {
        &self.records
    }

    pub fn record_fingerprint(&self) -> String {
        let mut hasher = Sha256::new();
        for record in &self.records {
            write_len_prefixed(&mut hasher, record.stable_place_id.as_bytes());
            write_len_prefixed(&mut hasher, record.item_id.as_bytes());
            write_len_prefixed(&mut hasher, record.kind.as_str().as_bytes());
        }
        hex(&hasher.finalize())
    }

    fn push(&mut self, request: &HostedPlaceActionRequest, kind: HostedPlaceActionKind) {
        self.records.push(HostedPlaceActionRecord {
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

pub fn place_hosted_sent_item(
    places: &HostedPlaceList,
    log: &mut HostedPlaceActionLog,
    request: &HostedPlaceActionRequest,
) -> Result<HostedPlaceActionRecord, HostedPlaceActionError> {
    record_action(places, log, request, HostedPlaceActionKind::SentItem)
}

pub fn scrub_hosted_item(
    places: &HostedPlaceList,
    log: &mut HostedPlaceActionLog,
    request: &HostedPlaceActionRequest,
) -> Result<HostedPlaceActionRecord, HostedPlaceActionError> {
    record_action(places, log, request, HostedPlaceActionKind::ScrubItem)
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
    if places.is_readable(&request.stable_place_id) {
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
