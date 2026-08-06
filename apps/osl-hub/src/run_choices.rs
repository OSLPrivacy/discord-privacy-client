//! Persistent choices for a local run plan.
//!
//! These choices are intentionally separate from account scan selections. A
//! plan is built only from the saved run choices, so account scanning cannot
//! invent a downloaded file path or change the selected run view.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const STORE_DIR: &str = "run-choices-v1";
const MAX_RUN_ID_BYTES: usize = 64;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OslRunViewChoice {
    WatchLive,
    RunInBackground,
}

impl OslRunViewChoice {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WatchLive => "watch-live",
            Self::RunInBackground => "run-in-background",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OslRunChoices {
    pub run_id: String,
    pub chosen_view: OslRunViewChoice,
    pub downloaded_file_path: Option<PathBuf>,
}

impl OslRunChoices {
    pub fn watch_live(run_id: impl Into<String>) -> Result<Self, String> {
        Self::new(run_id, OslRunViewChoice::WatchLive, None)
    }

    pub fn run_in_background(run_id: impl Into<String>) -> Result<Self, String> {
        Self::new(run_id, OslRunViewChoice::RunInBackground, None)
    }

    pub fn with_downloaded_file(mut self, downloaded_file_path: impl Into<PathBuf>) -> Self {
        self.downloaded_file_path = Some(downloaded_file_path.into());
        self
    }

    pub fn new(
        run_id: impl Into<String>,
        chosen_view: OslRunViewChoice,
        downloaded_file_path: Option<PathBuf>,
    ) -> Result<Self, String> {
        let choices = Self {
            run_id: run_id.into(),
            chosen_view,
            downloaded_file_path,
        };
        choices.validate()?;
        Ok(choices)
    }

    pub fn validate(&self) -> Result<(), String> {
        validate_run_id(&self.run_id)?;
        if let Some(path) = &self.downloaded_file_path {
            validate_downloaded_file_path(path)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OslRunChoiceSaveReceipt {
    pub run_id: String,
    pub chosen_view: OslRunViewChoice,
    pub downloaded_file_selected: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OslRunPlan {
    pub run_id: String,
    pub chosen_view: OslRunViewChoice,
    pub downloaded_file_path: Option<PathBuf>,
}

impl OslRunPlan {
    pub fn downloaded_file_status(&self) -> &'static str {
        if self.downloaded_file_path.is_some() {
            "selected"
        } else {
            "none"
        }
    }
}

pub fn save_osl_run_choices(
    root: &Path,
    choices: &OslRunChoices,
) -> Result<OslRunChoiceSaveReceipt, String> {
    choices.validate()?;
    let path = choices_path(root, &choices.run_id)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create OSL run choices store: {error}"))?;
    }
    let body = serde_json::to_vec_pretty(choices)
        .map_err(|error| format!("serialize OSL run choices: {error}"))?;
    let sealed = ipc::main_password::maybe_encrypt(&body)
        .map_err(|error| format!("encrypt OSL run choices: {error}"))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, sealed)
        .map_err(|error| format!("write OSL run choices temp file: {error}"))?;
    std::fs::rename(&tmp, &path)
        .map_err(|error| format!("commit OSL run choices file: {error}"))?;

    let read_back = read_osl_run_choices(root, &choices.run_id)?
        .ok_or_else(|| "OSL run choices were not readable after save".to_owned())?;
    Ok(OslRunChoiceSaveReceipt {
        run_id: choices.run_id.clone(),
        chosen_view: read_back.chosen_view,
        downloaded_file_selected: read_back.downloaded_file_path.is_some(),
    })
}

pub fn read_osl_run_choices(root: &Path, run_id: &str) -> Result<Option<OslRunChoices>, String> {
    let path = choices_path(root, run_id)?;
    let Ok(sealed) = std::fs::read(&path) else {
        return Ok(None);
    };
    let plain = ipc::main_password::maybe_decrypt_file(&path, &sealed)
        .map_err(|error| format!("decrypt OSL run choices: {error}"))?;
    let choices: OslRunChoices = serde_json::from_slice(&plain)
        .map_err(|error| format!("parse OSL run choices: {error}"))?;
    choices.validate()?;
    Ok(Some(choices))
}

pub fn build_osl_run_plan(root: &Path, run_id: &str) -> Result<OslRunPlan, String> {
    let choices = read_osl_run_choices(root, run_id)?
        .ok_or_else(|| "OSL run choices are missing".to_owned())?;
    Ok(OslRunPlan {
        run_id: choices.run_id,
        chosen_view: choices.chosen_view,
        downloaded_file_path: choices.downloaded_file_path,
    })
}

fn choices_path(root: &Path, run_id: &str) -> Result<PathBuf, String> {
    validate_run_id(run_id)?;
    Ok(root.join(STORE_DIR).join(format!("{run_id}.json")))
}

fn validate_run_id(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > MAX_RUN_ID_BYTES
        || value
            .bytes()
            .any(|byte| !(byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')))
    {
        return Err("OSL run id is invalid".to_owned());
    }
    Ok(())
}

fn validate_downloaded_file_path(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Err("OSL downloaded-file path is invalid".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_1416_run_plan_returns_chosen_view_and_file_only_when_selected() {
        let temp = tempfile::tempdir().expect("tempdir");
        ipc::main_password::set_file_storage_key(Some([0x16; 32]));

        let watch_live = OslRunChoices::watch_live("task-1416-watch-live").expect("watch choices");
        save_osl_run_choices(temp.path(), &watch_live).expect("save watch-live");
        let watch_plan =
            build_osl_run_plan(temp.path(), "task-1416-watch-live").expect("watch plan");

        let downloaded = temp.path().join("task-1416-report.pdf");
        std::fs::write(&downloaded, b"report").expect("downloaded fixture");
        let background =
            OslRunChoices::run_in_background("task-1416-background").expect("background choices");
        save_osl_run_choices(temp.path(), &background.with_downloaded_file(&downloaded))
            .expect("save background");
        let background_plan =
            build_osl_run_plan(temp.path(), "task-1416-background").expect("background plan");

        println!(
            "TASK1416_TEST watch_view={} watch_file_status={} background_view={} background_file_status={}",
            watch_plan.chosen_view.as_str(),
            watch_plan.downloaded_file_status(),
            background_plan.chosen_view.as_str(),
            background_plan.downloaded_file_status()
        );
        assert_eq!(watch_plan.chosen_view, OslRunViewChoice::WatchLive);
        assert_eq!(watch_plan.downloaded_file_path, None);
        assert_eq!(
            background_plan.chosen_view,
            OslRunViewChoice::RunInBackground
        );
        assert_eq!(background_plan.downloaded_file_path, Some(downloaded));
    }
}
