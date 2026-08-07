//! Durable progress for one both-sides burn.
//!
//! The burn action itself spans three independently retryable removals:
//! local records, service-held wrapped keys, and the peer/other-side removal
//! instruction. This journal stores only phase progress for an already
//! selected burn; `completed` is derived from all three phases.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const VERSION: u8 = 1;
const MAX_BURN_ID_LEN: usize = 128;
const MAX_MESSAGE_ID_LEN: usize = 160;
const MAX_SELECTED_MESSAGES: usize = 512;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum BothSidesBurnRemovalStep {
    LocalRemoval,
    ServiceRemoval,
    OtherSideRemoval,
}

impl BothSidesBurnRemovalStep {
    pub const ORDERED: [Self; 3] = [
        Self::LocalRemoval,
        Self::ServiceRemoval,
        Self::OtherSideRemoval,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::LocalRemoval => "local removal",
            Self::ServiceRemoval => "service removal",
            Self::OtherSideRemoval => "other-side removal",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "local removal" => Ok(Self::LocalRemoval),
            "service removal" => Ok(Self::ServiceRemoval),
            "other-side removal" => Ok(Self::OtherSideRemoval),
            _ => Err(format!(
                "OSL: unknown both-sides burn removal step: {value}"
            )),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct BothSidesBurnProgressStepDto {
    pub name: String,
    pub finished: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct BothSidesBurnProgressDto {
    pub burn_id: String,
    pub selected_message_count: usize,
    pub steps: Vec<BothSidesBurnProgressStepDto>,
    pub completed: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BurnProgressRecord {
    selected_message_ids: Vec<String>,
    finished_steps: BTreeMap<String, bool>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BurnProgressDocument {
    version: u8,
    burns: BTreeMap<String, BurnProgressRecord>,
}

pub struct BothSidesBurnProgressStore {
    path: PathBuf,
}

impl BothSidesBurnProgressStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn begin_or_resume(
        &self,
        burn_id: &str,
        selected_message_ids: Vec<String>,
    ) -> Result<BothSidesBurnProgressDto, String> {
        validate_burn_id(burn_id)?;
        validate_selected_message_ids(&selected_message_ids)?;
        let mut document = load_document(&self.path)?;
        let dto =
            {
                let record = document.burns.entry(burn_id.to_owned()).or_insert_with(|| {
                    BurnProgressRecord {
                        selected_message_ids: selected_message_ids.clone(),
                        finished_steps: initial_steps(),
                    }
                });
                if record.selected_message_ids != selected_message_ids {
                    return Err("OSL: both-sides burn progress selection changed".to_owned());
                }
                normalize_record(record)?;
                report(burn_id, record)?
            };
        persist_document(&self.path, &document)?;
        Ok(dto)
    }

    pub fn mark_finished(
        &self,
        burn_id: &str,
        step: BothSidesBurnRemovalStep,
    ) -> Result<BothSidesBurnProgressDto, String> {
        validate_burn_id(burn_id)?;
        let mut document = load_document(&self.path)?;
        let dto = {
            let record = document
                .burns
                .get_mut(burn_id)
                .ok_or_else(|| "OSL: both-sides burn progress is missing".to_owned())?;
            normalize_record(record)?;
            record.finished_steps.insert(step.as_str().to_owned(), true);
            report(burn_id, record)?
        };
        persist_document(&self.path, &document)?;
        Ok(dto)
    }

    pub fn report(&self, burn_id: &str) -> Result<BothSidesBurnProgressDto, String> {
        validate_burn_id(burn_id)?;
        let mut document = load_document(&self.path)?;
        let record = document
            .burns
            .get_mut(burn_id)
            .ok_or_else(|| "OSL: both-sides burn progress is missing".to_owned())?;
        normalize_record(record)?;
        report(burn_id, record)
    }
}

fn initial_steps() -> BTreeMap<String, bool> {
    BothSidesBurnRemovalStep::ORDERED
        .into_iter()
        .map(|step| (step.as_str().to_owned(), false))
        .collect()
}

fn normalize_record(record: &mut BurnProgressRecord) -> Result<(), String> {
    validate_selected_message_ids(&record.selected_message_ids)?;
    for step in BothSidesBurnRemovalStep::ORDERED {
        record
            .finished_steps
            .entry(step.as_str().to_owned())
            .or_insert(false);
    }
    if record.finished_steps.len() != BothSidesBurnRemovalStep::ORDERED.len()
        || record
            .finished_steps
            .keys()
            .any(|name| BothSidesBurnRemovalStep::parse(name).is_err())
    {
        return Err("OSL: both-sides burn progress has unknown removal steps".to_owned());
    }
    Ok(())
}

fn report(burn_id: &str, record: &BurnProgressRecord) -> Result<BothSidesBurnProgressDto, String> {
    let steps: Vec<_> = BothSidesBurnRemovalStep::ORDERED
        .into_iter()
        .map(|step| {
            let name = step.as_str().to_owned();
            BothSidesBurnProgressStepDto {
                finished: record.finished_steps.get(&name) == Some(&true),
                name,
            }
        })
        .collect();
    let completed = steps.iter().all(|step| step.finished);
    Ok(BothSidesBurnProgressDto {
        burn_id: burn_id.to_owned(),
        selected_message_count: record.selected_message_ids.len(),
        steps,
        completed,
    })
}

fn validate_burn_id(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > MAX_BURN_ID_LEN
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("OSL: both-sides burn id is invalid".to_owned());
    }
    Ok(())
}

fn validate_selected_message_ids(values: &[String]) -> Result<(), String> {
    if values.is_empty() || values.len() > MAX_SELECTED_MESSAGES {
        return Err("OSL: both-sides burn progress needs selected messages".to_owned());
    }
    let mut seen = std::collections::BTreeSet::new();
    for value in values {
        if value.is_empty() || value.len() > MAX_MESSAGE_ID_LEN || !seen.insert(value.as_str()) {
            return Err("OSL: both-sides burn progress message ids are invalid".to_owned());
        }
    }
    Ok(())
}

fn load_document(path: &Path) -> Result<BurnProgressDocument, String> {
    let Some(bytes) = read_optional(path)? else {
        return Ok(BurnProgressDocument {
            version: VERSION,
            ..BurnProgressDocument::default()
        });
    };
    let document: BurnProgressDocument = serde_json::from_slice(&bytes)
        .map_err(|_| "OSL: both-sides burn progress is malformed".to_owned())?;
    if document.version != VERSION {
        return Err("OSL: both-sides burn progress version is unsupported".to_owned());
    }
    Ok(document)
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("OSL: both-sides burn progress could not be read".to_owned()),
    }
}

fn persist_document(path: &Path, document: &BurnProgressDocument) -> Result<(), String> {
    let bytes = serde_json::to_vec(document)
        .map_err(|_| "OSL: both-sides burn progress could not be encoded".to_owned())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|_| "OSL: both-sides burn progress directory is unavailable".to_owned())?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)
        .map_err(|_| "OSL: both-sides burn progress could not be written".to_owned())?;
    std::fs::rename(&tmp, path)
        .map_err(|_| "OSL: both-sides burn progress could not be committed".to_owned())
}
