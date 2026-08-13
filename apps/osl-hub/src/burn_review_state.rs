use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const BURN_REVIEW_STATE_VERSION: u8 = 1;
const MAX_BURN_REVIEW_STATE_BYTES: u64 = 16 * 1024;

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BurnReviewSelection {
    pub selected_scope: String,
    pub selected_chat: String,
    pub hide_other_people: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BurnReviewBackResult {
    pub status: String,
    pub local_removal_count: usize,
    pub remote_removal_count: usize,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BurnReviewDocument {
    version: u8,
    selection: Option<BurnReviewSelection>,
}

impl Default for BurnReviewDocument {
    fn default() -> Self {
        Self {
            version: BURN_REVIEW_STATE_VERSION,
            selection: None,
        }
    }
}

#[derive(Default)]
struct BurnReviewMemory {
    selection: Option<BurnReviewSelection>,
}

pub struct BurnReviewState {
    path: PathBuf,
    inner: Mutex<BurnReviewMemory>,
}

impl BurnReviewState {
    pub fn load(path: PathBuf) -> Self {
        let selection = read_state(&path).and_then(|document| document.selection);
        Self {
            path,
            inner: Mutex::new(BurnReviewMemory { selection }),
        }
    }

    pub fn save_command(
        &self,
        selected_scope: String,
        selected_chat: String,
        hide_other_people: bool,
    ) -> Result<BurnReviewSelection, String> {
        let selection = BurnReviewSelection {
            selected_scope,
            selected_chat,
            hide_other_people,
        };
        validate_selection(&selection)?;
        self.replace_selection(Some(selection.clone()))?;
        Ok(selection)
    }

    pub fn get_command(&self) -> Result<Option<BurnReviewSelection>, String> {
        self.inner
            .lock()
            .map(|state| state.selection.clone())
            .map_err(|_| "burn review state lock is unavailable".to_owned())
    }

    pub fn back_command(&self) -> Result<BurnReviewBackResult, String> {
        self.back_command_checked(|| 1, || 1)
    }

    pub fn back_command_checked<LocalBurn, RemoteBurn>(
        &self,
        mut issue_local_burn: LocalBurn,
        mut issue_remote_burn: RemoteBurn,
    ) -> Result<BurnReviewBackResult, String>
    where
        LocalBurn: FnMut() -> usize,
        RemoteBurn: FnMut() -> usize,
    {
        let _ = (&mut issue_local_burn, &mut issue_remote_burn);
        self.replace_selection(None)?;
        Ok(BurnReviewBackResult {
            status: "cancelled".to_owned(),
            local_removal_count: 0,
            remote_removal_count: 0,
        })
    }

    pub fn summary(&self) -> Result<String, String> {
        match self.get_command()? {
            Some(selection) => Ok(format!(
                "selected_scope={} selected_chat={} hide_other_people={}",
                selection.selected_scope, selection.selected_chat, selection.hide_other_people
            )),
            None => Ok("none".to_owned()),
        }
    }

    fn replace_selection(&self, selection: Option<BurnReviewSelection>) -> Result<(), String> {
        let document = BurnReviewDocument {
            version: BURN_REVIEW_STATE_VERSION,
            selection: selection.clone(),
        };
        write_state(&self.path, &document)?;
        let mut state = self
            .inner
            .lock()
            .map_err(|_| "burn review state lock is unavailable".to_owned())?;
        state.selection = selection;
        Ok(())
    }
}

fn validate_selection(selection: &BurnReviewSelection) -> Result<(), String> {
    if !valid_opaque(&selection.selected_scope, 64) || !valid_chat(&selection.selected_chat) {
        return Err("burn review selection is invalid".to_owned());
    }
    Ok(())
}

fn valid_opaque(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

fn valid_chat(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b':' | b'-' | b'_')
        })
}

fn read_state(path: &Path) -> Option<BurnReviewDocument> {
    let bytes = crate::atomic_file::read_recoverable_bounded(
        path,
        MAX_BURN_REVIEW_STATE_BYTES,
        "burn review state",
    )
    .ok()
    .flatten()?;
    let document = serde_json::from_slice::<BurnReviewDocument>(&bytes).ok()?;
    (document.version == BURN_REVIEW_STATE_VERSION).then_some(document)
}

fn write_state(path: &Path, document: &BurnReviewDocument) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(document)
        .map_err(|_| "burn review state could not be encoded".to_owned())?;
    if bytes.len() as u64 > MAX_BURN_REVIEW_STATE_BYTES {
        return Err("burn review state exceeds the size limit".to_owned());
    }
    crate::atomic_file::write_recoverable(path, &bytes, "burn review state")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn temporary_file() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        std::env::temp_dir()
            .join(format!(
                "osl-burn-review-state-{}-{nonce}",
                std::process::id()
            ))
            .join("state.json")
    }

    #[test]
    fn direct_state_query_returns_selected_scope_chat_and_hide_choice() {
        let path = temporary_file();
        let state = BurnReviewState::load(path.clone());
        state
            .save_command(
                "your_side".to_owned(),
                "chat:burn-review-0520".to_owned(),
                true,
            )
            .unwrap();

        let summary = state.summary().unwrap();
        println!("burn review state {summary}");

        assert_eq!(
            summary,
            "selected_scope=your_side selected_chat=chat:burn-review-0520 hide_other_people=true"
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn back_discards_review_state_and_issues_no_burn_command() {
        let path = temporary_file();
        let state = BurnReviewState::load(path.clone());
        state
            .save_command(
                "your_side".to_owned(),
                "chat:burn-review-0521".to_owned(),
                true,
            )
            .unwrap();
        assert_ne!(state.summary().unwrap(), "none");

        let local_commands = AtomicUsize::new(0);
        let remote_commands = AtomicUsize::new(0);
        let result = state
            .back_command_checked(
                || {
                    local_commands.fetch_add(1, Ordering::SeqCst);
                    99
                },
                || {
                    remote_commands.fetch_add(1, Ordering::SeqCst);
                    99
                },
            )
            .unwrap();

        println!(
            "BACK status={} local_removal_count={} remote_removal_count={} local_burn_commands={} remote_burn_commands={} state_after={}",
            result.status,
            result.local_removal_count,
            result.remote_removal_count,
            local_commands.load(Ordering::SeqCst),
            remote_commands.load(Ordering::SeqCst),
            state.summary().unwrap()
        );

        assert_eq!(result.status, "cancelled");
        assert_eq!(result.local_removal_count, 0);
        assert_eq!(result.remote_removal_count, 0);
        assert_eq!(local_commands.load(Ordering::SeqCst), 0);
        assert_eq!(remote_commands.load(Ordering::SeqCst), 0);
        assert_eq!(state.get_command().unwrap(), None);
        assert_eq!(
            BurnReviewState::load(path.clone()).get_command().unwrap(),
            None
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BurnReviewFinalChoiceResult {
    pub status: String,
    pub reviewed_message: String,
    pub burn_mark: String,
    pub local_removal_count: usize,
    pub remote_removal_count: usize,
}

fn valid_label(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value.bytes().all(|byte| {
            byte.is_ascii_uppercase()
                || byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'-' | b'_' | b':')
        })
}
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
