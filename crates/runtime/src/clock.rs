//! Clock abstraction for deterministic testing.
//!
//! `Instant` arithmetic is opaque (no public constructor that takes a
//! point in time), so we model the clock as "a base instant captured
//! at construction time + a freely-mutable offset". The base is fixed
//! once; tests advance the offset.

use std::sync::Mutex;
use std::time::{Duration, Instant};

pub trait Clock: Send + Sync {
    fn now(&self) -> Instant;
}

/// Production clock — defers straight to [`Instant::now`].
#[derive(Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// Test clock. Constructed at "epoch zero" (whatever wall-clock time
/// happens at construction); [`MockClock::advance`] moves the
/// reported `now()` forward by the given duration.
#[derive(Debug)]
pub struct MockClock {
    base: Instant,
    offset: Mutex<Duration>,
}

impl Default for MockClock {
    fn default() -> Self {
        Self::new()
    }
}

impl MockClock {
    pub fn new() -> Self {
        MockClock {
            base: Instant::now(),
            offset: Mutex::new(Duration::ZERO),
        }
    }

    pub fn advance(&self, by: Duration) {
        let mut g = self.offset.lock().expect("MockClock offset poisoned");
        *g += by;
    }

    /// Current offset from `base` (test introspection).
    pub fn offset(&self) -> Duration {
        *self.offset.lock().expect("MockClock offset poisoned")
    }
}

impl Clock for MockClock {
    fn now(&self) -> Instant {
        self.base + self.offset()
    }
}
