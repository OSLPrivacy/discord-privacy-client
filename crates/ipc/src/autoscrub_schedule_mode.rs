//! TASK 1469: independent AutoScrub schedule modes.
//!
//! The schedule cadence and due-account decision are owned by Tasks 1463 and
//! 1464 on separate build lanes. This module owns the choice made *after* an
//! account becomes due: either "Find only" or "Find and delete". Keeping that
//! choice in a closed enum prevents a find-only run from inheriting deletion
//! behavior from a deletion agreement or from a previous run.
//!
//! A due run always records the exact matches returned by the service port.
//! The find-only arm intentionally never calls [`AutoScrubRunPort::delete`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const FIND_ONLY_LABEL: &str = "Find only";
pub const FIND_AND_DELETE_LABEL: &str = "Find and delete";
pub const MAX_ACCOUNT_ID_LEN: usize = 128;
pub const MAX_RUN_ID_LEN: usize = 128;
pub const MAX_MATCH_ID_LEN: usize = 256;
pub const MAX_MATCHES_PER_RUN: usize = 500;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoScrubScheduleMode {
    FindOnly,
    FindAndDelete,
}

impl AutoScrubScheduleMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::FindOnly => FIND_ONLY_LABEL,
            Self::FindAndDelete => FIND_AND_DELETE_LABEL,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedAutoScrubScheduleMode {
    pub account_id: String,
    pub mode: AutoScrubScheduleMode,
    pub label: &'static str,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutoScrubMatch {
    pub item_id: String,
}

impl AutoScrubMatch {
    pub fn new(item_id: impl Into<String>) -> Self {
        Self {
            item_id: item_id.into(),
        }
    }
}

/// Explicit scheduler handoff. `due_at_unix_secs` is the saved next-run time
/// and `now_unix_secs` is the runner's current tick; this module reads no live
/// clock and therefore cannot turn an early tick into a run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DueAutoScrubRun {
    pub run_id: String,
    pub account_id: String,
    pub due_at_unix_secs: i64,
    pub now_unix_secs: i64,
}

/// The completed activity record. Match ids are retained, not merely counted,
/// so "recorded matches" remains inspectable after the run returns.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubDueRunRecord {
    pub run_id: String,
    pub account_id: String,
    pub mode: AutoScrubScheduleMode,
    pub mode_label: &'static str,
    pub due_at_unix_secs: i64,
    pub ran_at_unix_secs: i64,
    pub matches: Vec<AutoScrubMatch>,
    pub matched_count: usize,
    pub delete_call_count: usize,
    pub deleted_count: usize,
}

#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub enum AutoScrubScheduleModeError {
    #[error("account id must be 1..={MAX_ACCOUNT_ID_LEN} bytes")]
    InvalidAccountId,
    #[error("run id must be 1..={MAX_RUN_ID_LEN} bytes")]
    InvalidRunId,
    #[error("no AutoScrub schedule mode is saved for this account")]
    ModeNotSaved,
    #[error("AutoScrub schedule is not due")]
    NotDue,
    #[error("AutoScrub returned an invalid or duplicate match id")]
    InvalidMatches,
    #[error("AutoScrub service operation failed: {0}")]
    Service(String),
}

/// The only service operations a scheduled mode can request. Tests can count
/// calls at this exact boundary, including proving that a nonempty find-only
/// result performs zero deletes.
pub trait AutoScrubRunPort {
    fn find_matches(
        &mut self,
        account_id: &str,
    ) -> Result<Vec<AutoScrubMatch>, AutoScrubScheduleModeError>;

    fn delete(
        &mut self,
        account_id: &str,
        matched: &AutoScrubMatch,
    ) -> Result<(), AutoScrubScheduleModeError>;
}

#[derive(Default)]
pub struct AutoScrubScheduleModeStore {
    modes: BTreeMap<String, AutoScrubScheduleMode>,
    activity: Vec<AutoScrubDueRunRecord>,
}

impl AutoScrubScheduleModeStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create or replace only this account's execution mode. In particular,
    /// saving Find only never creates a Find-and-delete choice.
    pub fn save_mode(
        &mut self,
        account_id: &str,
        mode: AutoScrubScheduleMode,
    ) -> Result<SavedAutoScrubScheduleMode, AutoScrubScheduleModeError> {
        validate_identifier(account_id, MAX_ACCOUNT_ID_LEN)
            .map_err(|_| AutoScrubScheduleModeError::InvalidAccountId)?;
        self.modes.insert(account_id.to_owned(), mode);
        Ok(SavedAutoScrubScheduleMode {
            account_id: account_id.to_owned(),
            mode,
            label: mode.label(),
        })
    }

    pub fn mode_for(
        &self,
        account_id: &str,
    ) -> Result<SavedAutoScrubScheduleMode, AutoScrubScheduleModeError> {
        let mode = self
            .modes
            .get(account_id)
            .copied()
            .ok_or(AutoScrubScheduleModeError::ModeNotSaved)?;
        Ok(SavedAutoScrubScheduleMode {
            account_id: account_id.to_owned(),
            mode,
            label: mode.label(),
        })
    }

    pub fn activity(&self) -> &[AutoScrubDueRunRecord] {
        &self.activity
    }

    pub fn run_due<P: AutoScrubRunPort>(
        &mut self,
        due: DueAutoScrubRun,
        port: &mut P,
    ) -> Result<AutoScrubDueRunRecord, AutoScrubScheduleModeError> {
        validate_identifier(&due.run_id, MAX_RUN_ID_LEN)
            .map_err(|_| AutoScrubScheduleModeError::InvalidRunId)?;
        validate_identifier(&due.account_id, MAX_ACCOUNT_ID_LEN)
            .map_err(|_| AutoScrubScheduleModeError::InvalidAccountId)?;
        if due.now_unix_secs < due.due_at_unix_secs {
            return Err(AutoScrubScheduleModeError::NotDue);
        }
        let mode = self.mode_for(&due.account_id)?.mode;
        let matches = port.find_matches(&due.account_id)?;
        validate_matches(&matches)?;

        let mut delete_call_count = 0usize;
        let mut deleted_count = 0usize;
        if mode == AutoScrubScheduleMode::FindAndDelete {
            for matched in &matches {
                delete_call_count += 1;
                port.delete(&due.account_id, matched)?;
                deleted_count += 1;
            }
        }

        let record = AutoScrubDueRunRecord {
            run_id: due.run_id,
            account_id: due.account_id,
            mode,
            mode_label: mode.label(),
            due_at_unix_secs: due.due_at_unix_secs,
            ran_at_unix_secs: due.now_unix_secs,
            matched_count: matches.len(),
            matches,
            delete_call_count,
            deleted_count,
        };
        self.activity.push(record.clone());
        Ok(record)
    }
}

fn validate_identifier(value: &str, max_len: usize) -> Result<(), ()> {
    if value.is_empty()
        || value.len() > max_len
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'@')
        })
    {
        return Err(());
    }
    Ok(())
}

fn validate_matches(matches: &[AutoScrubMatch]) -> Result<(), AutoScrubScheduleModeError> {
    if matches.len() > MAX_MATCHES_PER_RUN {
        return Err(AutoScrubScheduleModeError::InvalidMatches);
    }
    let mut seen = std::collections::BTreeSet::new();
    if matches.iter().any(|matched| {
        validate_identifier(&matched.item_id, MAX_MATCH_ID_LEN).is_err()
            || !seen.insert(matched.item_id.as_str())
    }) {
        return Err(AutoScrubScheduleModeError::InvalidMatches);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct CountingPort {
        matches: Vec<AutoScrubMatch>,
        delete_calls: usize,
    }

    impl AutoScrubRunPort for CountingPort {
        fn find_matches(
            &mut self,
            _account_id: &str,
        ) -> Result<Vec<AutoScrubMatch>, AutoScrubScheduleModeError> {
            Ok(self.matches.clone())
        }

        fn delete(
            &mut self,
            _account_id: &str,
            _matched: &AutoScrubMatch,
        ) -> Result<(), AutoScrubScheduleModeError> {
            self.delete_calls += 1;
            Ok(())
        }
    }

    fn due(account_id: &str) -> DueAutoScrubRun {
        DueAutoScrubRun {
            run_id: format!("run-{account_id}"),
            account_id: account_id.to_owned(),
            due_at_unix_secs: 100,
            now_unix_secs: 100,
        }
    }

    #[test]
    fn find_only_records_nonempty_matches_without_deleting() {
        let mut store = AutoScrubScheduleModeStore::new();
        store
            .save_mode("acct-find", AutoScrubScheduleMode::FindOnly)
            .unwrap();
        let mut port = CountingPort {
            matches: vec![
                AutoScrubMatch::new("message-1"),
                AutoScrubMatch::new("message-2"),
            ],
            ..CountingPort::default()
        };

        let record = store.run_due(due("acct-find"), &mut port).unwrap();

        assert_eq!(record.matched_count, 2);
        assert_eq!(record.matches, port.matches);
        assert_eq!(record.delete_call_count, 0);
        assert_eq!(record.deleted_count, 0);
        assert_eq!(port.delete_calls, 0);
        assert_eq!(store.activity(), &[record]);
    }

    #[test]
    fn find_and_delete_is_a_distinct_saved_mode_and_reaches_the_delete_port() {
        let mut store = AutoScrubScheduleModeStore::new();
        store
            .save_mode("acct-find", AutoScrubScheduleMode::FindOnly)
            .unwrap();
        store
            .save_mode("acct-delete", AutoScrubScheduleMode::FindAndDelete)
            .unwrap();
        assert_eq!(
            store.mode_for("acct-find").unwrap().mode,
            AutoScrubScheduleMode::FindOnly
        );
        assert_eq!(
            store.mode_for("acct-delete").unwrap().mode,
            AutoScrubScheduleMode::FindAndDelete
        );

        let mut port = CountingPort {
            matches: vec![
                AutoScrubMatch::new("message-1"),
                AutoScrubMatch::new("message-2"),
            ],
            ..CountingPort::default()
        };
        let record = store.run_due(due("acct-delete"), &mut port).unwrap();
        assert_eq!(record.delete_call_count, 2);
        assert_eq!(record.deleted_count, 2);
        assert_eq!(port.delete_calls, 2);
    }

    #[test]
    fn an_early_tick_does_not_find_or_delete() {
        let mut store = AutoScrubScheduleModeStore::new();
        store
            .save_mode("acct-find", AutoScrubScheduleMode::FindOnly)
            .unwrap();
        let mut request = due("acct-find");
        request.now_unix_secs = request.due_at_unix_secs - 1;
        let mut port = CountingPort {
            matches: vec![AutoScrubMatch::new("message-1")],
            ..CountingPort::default()
        };

        assert_eq!(
            store.run_due(request, &mut port),
            Err(AutoScrubScheduleModeError::NotDue)
        );
        assert_eq!(port.delete_calls, 0);
        assert!(store.activity().is_empty());
    }
}
