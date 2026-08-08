//! Whole-service transfer stop at the configured money ceiling.
//!
//! The stop applies before every new upload and download.  It deliberately
//! does not remove already-held bytes: operators can lower the fixture (or
//! costs) and resume service without losing a person's stored file.

use crate::{
    cost_alarm::{check_cost_alarm, CostAlarmError, MoneyCeiling},
    usage_counters::{UsageCounterError, UsageCounterStore},
};
use std::collections::BTreeMap;
use thiserror::Error;

/// One plain, shared explanation for every ceiling refusal.
pub const SERVICE_PAUSED_MESSAGE: &str = "The service has paused.";

#[derive(Debug, Error)]
pub enum AutomaticStopError {
    #[error("{SERVICE_PAUSED_MESSAGE}")]
    ServicePaused,
    #[error("stored file not found")]
    NotFound,
    #[error("usage counter failed: {0}")]
    Usage(#[from] UsageCounterError),
    #[error("cost alarm failed: {0}")]
    Cost(#[from] CostAlarmError),
}

/// Minimal backend file service used by the transfer boundary.
///
/// The byte map represents retained stored files. `read_stored` is purposely
/// not a transfer endpoint: it keeps retained bytes inspectable while the
/// service-wide transfer stop is in force.
pub struct AutomaticStopStore {
    counters: UsageCounterStore,
    ceiling: MoneyCeiling,
    files: BTreeMap<(String, String), Vec<u8>>,
}

impl AutomaticStopStore {
    pub fn new(counters: UsageCounterStore, ceiling: MoneyCeiling) -> Self {
        Self {
            counters,
            ceiling,
            files: BTreeMap::new(),
        }
    }

    /// Change the active ceiling without discarding retained files.
    pub fn set_ceiling(&mut self, ceiling: MoneyCeiling) {
        self.ceiling = ceiling;
    }

    /// Bring a pre-existing retained file under accounting without treating the
    /// migration as a new upload.  This is used when the stop is introduced to
    /// a service that already has stored files.
    pub fn import_retained_file(
        &mut self,
        person_id: &str,
        file_name: &str,
        bytes: Vec<u8>,
    ) -> Result<(), AutomaticStopError> {
        self.counters
            .store_file(person_id, file_name, bytes.len() as u64)?;
        self.files
            .insert((person_id.to_owned(), file_name.to_owned()), bytes);
        Ok(())
    }

    /// Admit a new upload only while the whole service remains below its cap.
    pub fn upload(
        &mut self,
        person_id: &str,
        file_name: &str,
        bytes: Vec<u8>,
        unix_seconds: i64,
    ) -> Result<(), AutomaticStopError> {
        self.require_running(unix_seconds)?;
        self.counters
            .store_file(person_id, file_name, bytes.len() as u64)?;
        self.counters
            .record_bytes_sent(person_id, bytes.len() as u64, unix_seconds)?;
        self.files
            .insert((person_id.to_owned(), file_name.to_owned()), bytes);
        Ok(())
    }

    /// Admit a new download only while the whole service remains below its cap.
    pub fn download(
        &mut self,
        person_id: &str,
        file_name: &str,
        unix_seconds: i64,
    ) -> Result<Vec<u8>, AutomaticStopError> {
        self.require_running(unix_seconds)?;
        let bytes = self
            .files
            .get(&(person_id.to_owned(), file_name.to_owned()))
            .cloned()
            .ok_or(AutomaticStopError::NotFound)?;
        self.counters
            .record_bytes_fetched(person_id, bytes.len() as u64, unix_seconds)?;
        Ok(bytes)
    }

    /// Read retained storage without starting a new download.
    pub fn read_stored(
        &self,
        person_id: &str,
        file_name: &str,
    ) -> Result<&[u8], AutomaticStopError> {
        self.files
            .get(&(person_id.to_owned(), file_name.to_owned()))
            .map(Vec::as_slice)
            .ok_or(AutomaticStopError::NotFound)
    }

    /// Delete one retained file and remove its live-storage charge.
    pub fn remove_stored(
        &mut self,
        person_id: &str,
        file_name: &str,
    ) -> Result<(), AutomaticStopError> {
        self.counters.remove_file(person_id, file_name)?;
        self.files
            .remove(&(person_id.to_owned(), file_name.to_owned()));
        Ok(())
    }

    fn require_running(&self, unix_seconds: i64) -> Result<(), AutomaticStopError> {
        let report = check_cost_alarm(&self.counters, self.ceiling, unix_seconds)?;
        if report.projected_cost_micros >= self.ceiling.ceiling_micros {
            return Err(AutomaticStopError::ServicePaused);
        }
        Ok(())
    }
}
