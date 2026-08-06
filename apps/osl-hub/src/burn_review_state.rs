use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

const BURN_REVIEW_STATE_VERSION: u8 = 1;
const MAX_BURN_REVIEW_STATE_BYTES: u64 = 4 * 1024;
const MAX_REVIEW_FIELD_BYTES: usize = 256;

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BurnReviewStateCommand {
    pub selected_scope: String,
    pub selected_chat: String,
    pub hide_other_people: bool,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BurnReviewStateSummary {
    pub selected_scope: String,
    pub selected_chat: String,
    pub hide_other_people: bool,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BurnReviewStateDocument {
    version: u8,
    selected_scope: String,
    selected_chat: String,
    hide_other_people: bool,
}

impl Default for BurnReviewStateDocument {
    fn default() -> Self {
        Self {
            version: BURN_REVIEW_STATE_VERSION,
            selected_scope: String::new(),
            selected_chat: String::new(),
            hide_other_people: false,
        }
    }
}

pub struct BurnReviewState {
    path: PathBuf,
    document: Mutex<BurnReviewStateDocument>,
}

impl BurnReviewState {
    pub fn load(path: PathBuf) -> Self {
        Self {
            document: Mutex::new(read_document(&path).unwrap_or_default()),
            path,
        }
    }

    pub fn save_command(
        &self,
        command: BurnReviewStateCommand,
    ) -> Result<BurnReviewStateSummary, String> {
        let document = document_from_command(command)?;
        write_document(&self.path, &document)?;
        let summary = summary(&document);
        let mut current = self
            .document
            .lock()
            .map_err(|_| "Burn review state is unavailable".to_owned())?;
        *current = document;
        Ok(summary)
    }

    pub fn summary(&self) -> Result<BurnReviewStateSummary, String> {
        self.document
            .lock()
            .map(|document| summary(&document))
            .map_err(|_| "Burn review state is unavailable".to_owned())
    }
}

fn document_from_command(
    command: BurnReviewStateCommand,
) -> Result<BurnReviewStateDocument, String> {
    Ok(BurnReviewStateDocument {
        version: BURN_REVIEW_STATE_VERSION,
        selected_scope: normalize_review_field("selected scope", command.selected_scope)?,
        selected_chat: normalize_review_field("selected chat", command.selected_chat)?,
        hide_other_people: command.hide_other_people,
    })
}

fn normalize_review_field(label: &str, value: String) -> Result<String, String> {
    let normalized = value.trim();
    if normalized.is_empty()
        || normalized.len() > MAX_REVIEW_FIELD_BYTES
        || normalized.chars().any(|character| character.is_control())
    {
        return Err(format!(
            "Burn review {label} must be a bounded printable identifier"
        ));
    }
    Ok(normalized.to_owned())
}

fn summary(document: &BurnReviewStateDocument) -> BurnReviewStateSummary {
    BurnReviewStateSummary {
        selected_scope: document.selected_scope.clone(),
        selected_chat: document.selected_chat.clone(),
        hide_other_people: document.hide_other_people,
    }
}

fn read_document(path: &Path) -> Option<BurnReviewStateDocument> {
    let bytes = crate::atomic_file::read_recoverable_bounded(
        path,
        MAX_BURN_REVIEW_STATE_BYTES,
        "Burn review state",
    )
    .ok()
    .flatten()?;
    let document = serde_json::from_slice::<BurnReviewStateDocument>(&bytes).ok()?;
    (document.version == BURN_REVIEW_STATE_VERSION).then_some(document)
}

fn write_document(path: &Path, document: &BurnReviewStateDocument) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(document)
        .map_err(|_| "Burn review state could not be encoded".to_owned())?;
    if bytes.len() as u64 > MAX_BURN_REVIEW_STATE_BYTES {
        return Err("Burn review state exceeds the size limit".to_owned());
    }
    crate::atomic_file::write_recoverable(path, &bytes, "Burn review state")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_file() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        std::env::temp_dir()
            .join(format!(
                "osl-hub-burn-review-state-{}-{nonce}",
                std::process::id()
            ))
            .join("burn-review-state.json")
    }

    #[test]
    fn direct_state_query_returns_selected_scope_chat_and_hide_choice() {
        let path = temporary_file();
        let state = BurnReviewState::load(path.clone());

        state
            .save_command(BurnReviewStateCommand {
                selected_scope: "your_side".to_owned(),
                selected_chat: "chat:burn-review-0520".to_owned(),
                hide_other_people: true,
            })
            .expect("burn review state saves");

        let queried = state.summary().expect("direct state query");
        assert_eq!(queried.selected_scope, "your_side");
        assert_eq!(queried.selected_chat, "chat:burn-review-0520");
        assert!(queried.hide_other_people);
        assert_eq!(
            BurnReviewState::load(path.clone()).summary().unwrap(),
            queried
        );
        println!(
            "burn review state selected_scope={} selected_chat={} hide_other_people={}",
            queried.selected_scope, queried.selected_chat, queried.hide_other_people
        );

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
