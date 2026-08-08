//! Process-wide fence between destructive Burn and background work.
//!
//! Deleting persisted jobs is insufficient while their workers are alive: an
//! old worker could write state back or send after Burn reports completion.
//! Protected jobs therefore own an epoch lease and put every state-creating or
//! sending side effect behind [`BurnJobLease::run_if_live`]. Burn takes the
//! exclusive side-effect gate, revokes the epoch, and only then deletes state.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BurnJobKind {
    Upload,
    TimedDelete,
    ServiceDelete,
}

impl BurnJobKind {
    const fn index(self) -> usize {
        match self {
            Self::Upload => 0,
            Self::TimedDelete => 1,
            Self::ServiceDelete => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BurnJobCounts {
    pub upload: usize,
    pub timed_delete: usize,
    pub service_delete: usize,
}

impl BurnJobCounts {
    pub const fn total(self) -> usize {
        self.upload + self.timed_delete + self.service_delete
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BurnRevoked;

impl std::fmt::Display for BurnRevoked {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("background job was revoked by Burn")
    }
}

impl std::error::Error for BurnRevoked {}

struct Inner {
    burning: AtomicBool,
    epoch: AtomicU64,
    active: [AtomicUsize; 3],
    side_effect_gate: RwLock<()>,
}

#[derive(Clone)]
pub struct BurnJobFence {
    inner: Arc<Inner>,
}

impl Default for BurnJobFence {
    fn default() -> Self {
        Self {
            inner: Arc::new(Inner {
                burning: AtomicBool::new(false),
                epoch: AtomicU64::new(0),
                active: std::array::from_fn(|_| AtomicUsize::new(0)),
                side_effect_gate: RwLock::new(()),
            }),
        }
    }
}

impl BurnJobFence {
    /// Start one protected job. No new lease is issued after Burn begins.
    pub fn start(&self, kind: BurnJobKind) -> Result<BurnJobLease, BurnRevoked> {
        let gate = self
            .inner
            .side_effect_gate
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if self.inner.burning.load(Ordering::Acquire) {
            return Err(BurnRevoked);
        }
        let epoch = self.inner.epoch.load(Ordering::Acquire);
        self.inner.active[kind.index()].fetch_add(1, Ordering::AcqRel);
        drop(gate);
        Ok(BurnJobLease {
            inner: Arc::clone(&self.inner),
            kind,
            epoch,
        })
    }

    /// Revoke every existing lease before Burn removes persisted state.
    ///
    /// Taking the write gate waits for a side effect already in progress and
    /// prevents a later one from entering while the epoch changes.
    pub fn begin_burn(&self) {
        let _gate = self
            .inner
            .side_effect_gate
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !self.inner.burning.swap(true, Ordering::AcqRel) {
            self.inner.epoch.fetch_add(1, Ordering::AcqRel);
        }
    }

    pub fn burning(&self) -> bool {
        self.inner.burning.load(Ordering::Acquire)
    }

    pub fn active_counts(&self) -> BurnJobCounts {
        BurnJobCounts {
            upload: self.inner.active[BurnJobKind::Upload.index()].load(Ordering::Acquire),
            timed_delete: self.inner.active[BurnJobKind::TimedDelete.index()]
                .load(Ordering::Acquire),
            service_delete: self.inner.active[BurnJobKind::ServiceDelete.index()]
                .load(Ordering::Acquire),
        }
    }
}

pub struct BurnJobLease {
    inner: Arc<Inner>,
    kind: BurnJobKind,
    epoch: u64,
}

impl BurnJobLease {
    pub fn kind(&self) -> BurnJobKind {
        self.kind
    }

    pub fn is_live(&self) -> bool {
        !self.inner.burning.load(Ordering::Acquire)
            && self.inner.epoch.load(Ordering::Acquire) == self.epoch
    }

    /// Run one state-creating or sending side effect only while this lease is
    /// still in the live pre-Burn epoch.
    pub fn run_if_live<T>(&self, action: impl FnOnce() -> T) -> Result<T, BurnRevoked> {
        let _gate = self
            .inner
            .side_effect_gate
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !self.is_live() {
            return Err(BurnRevoked);
        }
        Ok(action())
    }
}

impl Drop for BurnJobLease {
    fn drop(&mut self) {
        self.inner.active[self.kind.index()].fetch_sub(1, Ordering::AcqRel);
    }
}
