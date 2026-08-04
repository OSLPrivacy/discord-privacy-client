//! A bounded resident model slot for background carrier generation.
//!
//! The send path never calls [`WarmModel::load`]: it only observes whether a
//! model is already resident and falls through to the word bank when it is
//! not.  Callers schedule `load` on their background worker after an idle
//! release, which prevents a cold weight load from becoming send latency.

use std::time::{Duration, Instant};

/// A loaded model together with the working set it actually occupies.
pub trait ResidentModel {
    fn working_set_bytes(&self) -> u64;
}

/// The result available to a latency-sensitive caller.
#[derive(Debug, PartialEq, Eq)]
pub enum Resident<'a, M> {
    Ready(&'a mut M),
    Loading,
    Unavailable,
}

/// Holds at most one already-loaded local model.  It intentionally contains no
/// loader callback: invoking arbitrary I/O from `for_generation` would make a
/// future refactor accidentally cold-load during a send.
pub struct WarmModel<M> {
    model: Option<M>,
    max_working_set_bytes: u64,
    idle_after: Duration,
    last_used: Option<Instant>,
    loading: bool,
}

impl<M: ResidentModel> WarmModel<M> {
    pub fn new(max_working_set_bytes: u64, idle_after: Duration) -> Self {
        Self {
            model: None,
            max_working_set_bytes,
            idle_after,
            last_used: None,
            loading: false,
        }
    }

    /// Mark that a background worker has begun loading.  A send sees Loading
    /// and must use a ready pool entry or the word-bank fallback.
    pub fn begin_load(&mut self) -> bool {
        if self.model.is_some() || self.loading {
            return false;
        }
        self.loading = true;
        true
    }

    /// Install a model only after the background loader has completed. Models
    /// above the declared pack working-set ceiling are rejected before they can
    /// become resident.
    pub fn finish_load(&mut self, model: M, now: Instant) -> Result<(), M> {
        self.loading = false;
        if model.working_set_bytes() > self.max_working_set_bytes {
            return Err(model);
        }
        self.model = Some(model);
        self.last_used = Some(now);
        Ok(())
    }

    pub fn fail_load(&mut self) {
        self.loading = false;
    }

    /// This is the only access generation has. It cannot load weights.
    pub fn for_generation(&mut self, now: Instant) -> Resident<'_, M> {
        if let Some(model) = self.model.as_mut() {
            self.last_used = Some(now);
            return Resident::Ready(model);
        }
        if self.loading {
            Resident::Loading
        } else {
            Resident::Unavailable
        }
    }

    /// Drop a dormant model so an unused optional feature does not retain its
    /// working set indefinitely. Returns whether memory was released.
    pub fn release_if_idle(&mut self, now: Instant) -> bool {
        let idle = self
            .last_used
            .is_some_and(|used| now.saturating_duration_since(used) >= self.idle_after);
        if idle && self.model.take().is_some() {
            self.last_used = None;
            true
        } else {
            false
        }
    }

    pub fn is_resident(&self) -> bool {
        self.model.is_some()
    }
}
