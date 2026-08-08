use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

const BURN_REVIEW_STATE_VERSION: u8 = 1;
const MAX_BURN_REVIEW_STATE_BYTES: u64 = 16 * 1024;

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BurnReviewSelection {
    pub selected_scope: String,
    pub selected_chat: String,
    pub hide_other_people: bool,
}

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

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BurnReviewBackResult {
    pub status: String,
    pub local_removal_count: usize,
    pub remote_removal_count: usize,
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

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BurnReviewDocument {
    version: u8,
    selection: Option<BurnReviewSelection>,
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

    pub fn save_state_command(
        &self,
        command: BurnReviewStateCommand,
    ) -> Result<BurnReviewStateSummary, String> {
        let selection = self.save_command(
            command.selected_scope,
            command.selected_chat,
            command.hide_other_people,
        )?;
        Ok(BurnReviewStateSummary {
            selected_scope: selection.selected_scope,
            selected_chat: selection.selected_chat,
            hide_other_people: selection.hide_other_people,
        })
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

    pub fn final_choice_command_checked<LocalBurn, RemoteBurn>(
        &self,
        final_choice: &str,
        reviewed_message: String,
        burn_mark: String,
        mut issue_local_burn: LocalBurn,
        mut issue_remote_burn: RemoteBurn,
    ) -> Result<BurnReviewFinalChoiceResult, String>
    where
        LocalBurn: FnMut(&str, &str) -> usize,
        RemoteBurn: FnMut(&str, &str) -> usize,
    {
        if !valid_label(&reviewed_message, 64) || !valid_label(&burn_mark, 64) {
            return Err("burn review final choice is invalid".to_owned());
        }
        match final_choice {
            "CONFIRM" => {
                let local_removal_count = issue_local_burn(&reviewed_message, &burn_mark);
                let remote_removal_count = issue_remote_burn(&reviewed_message, &burn_mark);
                self.replace_selection(None)?;
                Ok(BurnReviewFinalChoiceResult {
                    status: "burn confirmed".to_owned(),
                    reviewed_message,
                    burn_mark,
                    local_removal_count,
                    remote_removal_count,
                })
            }
            "BACK" => {
                self.replace_selection(None)?;
                Ok(BurnReviewFinalChoiceResult {
                    status: "burn cancelled".to_owned(),
                    reviewed_message,
                    burn_mark,
                    local_removal_count: 0,
                    remote_removal_count: 0,
                })
            }
            _ => Err("unknown burn choice".to_owned()),
        }
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
