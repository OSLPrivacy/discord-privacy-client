//! Durable, owner-started recovery for an interrupted hosted Scrub run.
//!
//! A checkpoint is deliberately not a scheduler.  It records the approved
//! scope, receipts that have reached durable storage, and at most one action
//! that was in progress when the process stopped.  Loading a checkpoint marks
//! that action `Unknown`; only an explicit owner resume request can select
//! items for another attempt.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;

const CHECKPOINT_VERSION: u8 = 1;
const MAX_CHECKPOINT_BYTES: u64 = 1024 * 1024;
const CHECKPOINT_LABEL: &str = "hosted Scrub checkpoint";

/// The outcome persisted for an item before another provider action begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HostedCheckpointReceipt {
    VerifiedGone,
    StillPresent,
    Unknown,
}

/// The durable state for one approved hosted Scrub plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostedScrubCheckpoint {
    version: u8,
    approved_scope: BTreeSet<String>,
    receipts: BTreeMap<String, HostedCheckpointReceipt>,
    in_flight: Option<String>,
}

impl HostedScrubCheckpoint {
    /// Starts a checkpoint from the already-approved plan.  Scope can only
    /// shrink when the owner later resumes; it is never re-derived from a scan.
    pub fn new(
        approved_scope: impl IntoIterator<Item = String>,
    ) -> Result<Self, HostedCheckpointError> {
        let checkpoint = Self {
            version: CHECKPOINT_VERSION,
            approved_scope: approved_scope.into_iter().collect(),
            receipts: BTreeMap::new(),
            in_flight: None,
        };
        checkpoint.validate()?;
        Ok(checkpoint)
    }

    /// Persists the item that is about to be acted on.  If the process dies
    /// before `complete_item`, recovery turns precisely this item into
    /// `Unknown` rather than guessing its outcome.
    pub fn begin_item(
        &mut self,
        item_id: &str,
        store: &HostedCheckpointStore,
    ) -> Result<(), HostedCheckpointError> {
        if !self.approved_scope.contains(item_id) {
            return Err(HostedCheckpointError::OutsideApprovedScope);
        }
        if self.in_flight.is_some() {
            return Err(HostedCheckpointError::ActionAlreadyInFlight);
        }
        if self.receipts.contains_key(item_id) {
            return Err(HostedCheckpointError::ItemAlreadyCompleted);
        }
        let mut updated = self.clone();
        updated.in_flight = Some(item_id.to_owned());
        store.save(&updated)?;
        *self = updated;
        Ok(())
    }

    /// Persists a completed receipt before the caller may begin another item.
    pub fn complete_item(
        &mut self,
        item_id: &str,
        outcome: HostedCheckpointReceipt,
        store: &HostedCheckpointStore,
    ) -> Result<(), HostedCheckpointError> {
        if self.in_flight.as_deref() != Some(item_id) {
            return Err(HostedCheckpointError::ItemNotInFlight);
        }
        let mut updated = self.clone();
        updated.in_flight = None;
        updated.receipts.insert(item_id.to_owned(), outcome);
        store.save(&updated)?;
        *self = updated;
        Ok(())
    }

    /// Performs an explicit, owner-started resume selection.
    ///
    /// `owner_scope` is a fresh subset of the original approved plan.  Every
    /// candidate is passed through a fresh inspection, no item outside the
    /// approved scope is admitted, and prior `Unknown` outcomes are surfaced
    /// for manual resolution rather than retried.
    pub fn owner_initiated_resume<F>(
        &self,
        owner_scope: impl IntoIterator<Item = String>,
        mut inspect: F,
    ) -> Result<Vec<String>, HostedCheckpointError>
    where
        F: FnMut(&str) -> bool,
    {
        if self.in_flight.is_some() {
            return Err(HostedCheckpointError::InterruptedActionUnresolved);
        }

        let requested: BTreeSet<String> = owner_scope.into_iter().collect();
        if !requested.is_subset(&self.approved_scope) {
            return Err(HostedCheckpointError::ResumeWouldWidenScope);
        }

        Ok(requested
            .into_iter()
            .filter(|item_id| self.receipts.get(item_id) != Some(&HostedCheckpointReceipt::Unknown))
            .filter(|item_id| {
                self.receipts.get(item_id) != Some(&HostedCheckpointReceipt::VerifiedGone)
            })
            .filter(|item_id| inspect(item_id))
            .collect())
    }

    pub fn receipt_for(&self, item_id: &str) -> Option<HostedCheckpointReceipt> {
        self.receipts.get(item_id).copied()
    }

    fn recover_interrupted_action(&mut self) {
        if let Some(item_id) = self.in_flight.take() {
            self.receipts
                .insert(item_id, HostedCheckpointReceipt::Unknown);
        }
    }

    fn validate(&self) -> Result<(), HostedCheckpointError> {
        if self.version != CHECKPOINT_VERSION
            || self.approved_scope.iter().any(|item_id| item_id.is_empty())
            || self
                .receipts
                .keys()
                .any(|item_id| !self.approved_scope.contains(item_id))
            || self.in_flight.as_ref().is_some_and(|item_id| {
                !self.approved_scope.contains(item_id) || self.receipts.contains_key(item_id)
            })
        {
            return Err(HostedCheckpointError::InvalidCheckpoint);
        }
        Ok(())
    }
}

/// Filesystem persistence for hosted Scrub checkpoints.
#[derive(Debug, Clone)]
pub struct HostedCheckpointStore {
    path: PathBuf,
}

impl HostedCheckpointStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Loads durable progress and converts only a persisted in-flight item to
    /// `Unknown`.  It deliberately does not select or start a resumed action.
    pub fn load(&self) -> Result<Option<HostedScrubCheckpoint>, HostedCheckpointError> {
        let Some(bytes) = crate::atomic_file::read_recoverable_bounded(
            &self.path,
            MAX_CHECKPOINT_BYTES,
            CHECKPOINT_LABEL,
        )
        .map_err(|_| HostedCheckpointError::Storage)?
        else {
            return Ok(None);
        };
        let mut checkpoint: HostedScrubCheckpoint =
            serde_json::from_slice(&bytes).map_err(|_| HostedCheckpointError::InvalidCheckpoint)?;
        checkpoint.validate()?;
        if checkpoint.in_flight.is_some() {
            checkpoint.recover_interrupted_action();
            self.save(&checkpoint)?;
        }
        Ok(Some(checkpoint))
    }

    fn save(&self, checkpoint: &HostedScrubCheckpoint) -> Result<(), HostedCheckpointError> {
        checkpoint.validate()?;
        let bytes = serde_json::to_vec(checkpoint).map_err(|_| HostedCheckpointError::Storage)?;
        crate::atomic_file::write_recoverable(&self.path, &bytes, CHECKPOINT_LABEL)
            .map_err(|_| HostedCheckpointError::Storage)
    }

    #[cfg(test)]
    fn path(&self) -> &Path {
        &self.path
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostedCheckpointError {
    OutsideApprovedScope,
    ResumeWouldWidenScope,
    ActionAlreadyInFlight,
    ItemAlreadyCompleted,
    ItemNotInFlight,
    InterruptedActionUnresolved,
    InvalidCheckpoint,
    Storage,
}

#[cfg(test)]
mod tests {
    use super::{
        HostedCheckpointError, HostedCheckpointReceipt, HostedCheckpointStore,
        HostedScrubCheckpoint,
    };

    fn store() -> (tempfile::TempDir, HostedCheckpointStore) {
        let directory = tempfile::tempdir().expect("temporary checkpoint directory");
        let store = HostedCheckpointStore::new(directory.path().join("checkpoint.json"));
        (directory, store)
    }

    #[test]
    fn scr_h5_crash_loses_only_the_in_flight_item_as_unknown() {
        let (_directory, store) = store();
        let mut checkpoint = HostedScrubCheckpoint::new(["first".to_owned(), "second".to_owned()])
            .expect("approved plan");

        checkpoint
            .begin_item("first", &store)
            .expect("durable begin");
        checkpoint
            .complete_item("first", HostedCheckpointReceipt::VerifiedGone, &store)
            .expect("durable receipt");
        checkpoint
            .begin_item("second", &store)
            .expect("durable in-flight marker");
        drop(checkpoint); // Simulate a process death between action and receipt.

        let recovered = store
            .load()
            .expect("recover checkpoint")
            .expect("checkpoint exists");

        assert_eq!(
            recovered.receipt_for("first"),
            Some(HostedCheckpointReceipt::VerifiedGone)
        );
        assert_eq!(
            recovered.receipt_for("second"),
            Some(HostedCheckpointReceipt::Unknown)
        );
        assert_eq!(std::fs::read(store.path()).is_ok(), true);
    }

    #[test]
    fn scr_h5_resume_is_owner_started_reinspected_and_shrink_only() {
        let (_directory, store) = store();
        let mut checkpoint = HostedScrubCheckpoint::new([
            "gone".to_owned(),
            "retry".to_owned(),
            "unknown".to_owned(),
        ])
        .expect("approved plan");
        for (item_id, outcome) in [
            ("gone", HostedCheckpointReceipt::VerifiedGone),
            ("retry", HostedCheckpointReceipt::StillPresent),
            ("unknown", HostedCheckpointReceipt::Unknown),
        ] {
            checkpoint.begin_item(item_id, &store).expect("begin item");
            checkpoint
                .complete_item(item_id, outcome, &store)
                .expect("complete item");
        }

        let loaded = store
            .load()
            .expect("load checkpoint")
            .expect("checkpoint exists");
        let mut inspected = Vec::new();
        let resume = loaded
            .owner_initiated_resume(
                ["gone".to_owned(), "retry".to_owned(), "unknown".to_owned()],
                |item_id| {
                    inspected.push(item_id.to_owned());
                    item_id == "retry"
                },
            )
            .expect("owner-selected subset");

        assert_eq!(inspected, vec!["retry"]);
        assert_eq!(resume, vec!["retry"]);
        assert_eq!(
            loaded.owner_initiated_resume(["new-provider-item".to_owned()], |_| true),
            Err(HostedCheckpointError::ResumeWouldWidenScope)
        );
    }
}
