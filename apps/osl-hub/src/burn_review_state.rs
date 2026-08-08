use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const BURN_REVIEW_STATE_VERSION: u8 = 1;
const MAX_BURN_REVIEW_STATE_BYTES: u64 = 16 * 1024;
const BURN_SCREEN_SCOPES: [&str; 3] = ["chat", "app", "account"];

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
    #[serde(default)]
    screen_states: BTreeMap<String, BurnScreenState>,
}

impl Default for BurnReviewDocument {
    fn default() -> Self {
        Self {
            version: BURN_REVIEW_STATE_VERSION,
            selection: None,
            screen_states: BTreeMap::new(),
        }
    }
}

#[derive(Default)]
struct BurnReviewMemory {
    selection: Option<BurnReviewSelection>,
    screen_states: BTreeMap<String, BurnScreenState>,
}

pub struct BurnReviewState {
    path: PathBuf,
    inner: Mutex<BurnReviewMemory>,
}

impl BurnReviewState {
    pub fn load(path: PathBuf) -> Self {
        let document = read_state(&path).unwrap_or_default();
        Self {
            path,
            inner: Mutex::new(BurnReviewMemory {
                selection: document.selection,
                screen_states: document.screen_states,
            }),
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

    /// Save the presentation state for one of the named burn screens.  This is
    /// deliberately separate from the burn action: opening or changing a
    /// screen never authorizes deletion.
    pub fn save_burn_screen_command(
        &self,
        command: BurnScreenStateCommand,
    ) -> Result<BurnScreenState, String> {
        validate_burn_screen_scope(&command.scope)?;
        let state = BurnScreenState {
            selected_burn_time: normalize_screen_field(
                "selected burn time",
                command.selected_burn_time,
            )?,
            warning_text: normalize_screen_field("warning text", command.warning_text)?,
        };
        let mut memory = self
            .inner
            .lock()
            .map_err(|_| "burn review state lock is unavailable".to_owned())?;
        memory.screen_states.insert(command.scope, state.clone());
        self.write_memory(&memory)?;
        Ok(state)
    }

    /// Read a named burn screen without making any burn request.
    pub fn get_burn_screen_command(&self, scope: &str) -> Result<Option<BurnScreenState>, String> {
        validate_burn_screen_scope(scope)?;
        self.inner
            .lock()
            .map(|state| state.screen_states.get(scope).cloned())
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
        let mut state = self
            .inner
            .lock()
            .map_err(|_| "burn review state lock is unavailable".to_owned())?;
        state.selection = selection;
        self.write_memory(&state)?;
        Ok(())
    }

    fn write_memory(&self, memory: &BurnReviewMemory) -> Result<(), String> {
        write_state(
            &self.path,
            &BurnReviewDocument {
                version: BURN_REVIEW_STATE_VERSION,
                selection: memory.selection.clone(),
                screen_states: memory.screen_states.clone(),
            },
        )
    }
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BurnScreenStateCommand {
    pub scope: String,
    pub selected_burn_time: String,
    pub warning_text: String,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BurnScreenState {
    pub selected_burn_time: String,
    pub warning_text: String,
}

fn validate_selection(selection: &BurnReviewSelection) -> Result<(), String> {
    if !valid_opaque(&selection.selected_scope, 64) || !valid_chat(&selection.selected_chat) {
        return Err("burn review selection is invalid".to_owned());
    }
    Ok(())
}

fn validate_burn_screen_scope(scope: &str) -> Result<(), String> {
    if BURN_SCREEN_SCOPES.contains(&scope) {
        Ok(())
    } else {
        Err(format!("unknown burn screen scope: {scope}"))
    }
}

fn normalize_screen_field(label: &str, value: String) -> Result<String, String> {
    let normalized = value.trim();
    if normalized.is_empty()
        || normalized.len() > MAX_REVIEW_FIELD_BYTES
        || normalized.chars().any(char::is_control)
    {
        return Err(format!(
            "Burn screen {label} must be bounded printable text"
        ));
    }
    Ok(normalized.to_owned())
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
