//! Monotonic lifetime enforcement for content that is currently rendered.
//!
//! `view_lifetime` begins when pixels are first shown, never when a message is
//! sent or materialized.  It deliberately accepts only a monotonic timestamp:
//! wall-clock time is not part of this API, so changing either device's date
//! cannot extend a live view.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

static PROCESS_MONOTONIC_ORIGIN: OnceLock<Instant> = OnceLock::new();

/// A process-local monotonic timestamp suitable for [`ViewLifetime`].
///
/// This value is intentionally not serializable and must not be persisted as a
/// message deadline.  Durable expiry is enforced by the sealed lifecycle
/// ledger; this clock protects the interval after a renderer exposes content.
pub fn monotonic_now() -> Duration {
    PROCESS_MONOTONIC_ORIGIN.get_or_init(Instant::now).elapsed()
}

/// Whether a renderer may continue showing a message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderLifetimeVerdict {
    /// The first render was accepted and started the timer.
    Started,
    /// The message remains within its already-started lifetime.
    Active,
    /// The message must be removed from the renderer immediately.
    Expired,
}

/// A non-resettable timer that starts at the first render.
///
/// `now` values are monotonic durations from one shared origin. A regressing
/// input is treated as expired rather than saturating or restarting the timer:
/// accepting it would turn a faulty clock into extra viewing time.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ViewLifetime {
    lifetime: Duration,
    first_rendered_at: Option<Duration>,
    last_observed_at: Option<Duration>,
}

impl ViewLifetime {
    pub fn new(lifetime: Duration) -> Self {
        Self {
            lifetime,
            first_rendered_at: None,
            last_observed_at: None,
        }
    }

    /// Start the timer only if this is the first actual render.
    pub fn on_render(&mut self, now: Duration) -> RenderLifetimeVerdict {
        if self.regressed(now) {
            return RenderLifetimeVerdict::Expired;
        }
        if self.first_rendered_at.is_none() {
            self.first_rendered_at = Some(now);
            self.last_observed_at = Some(now);
            return RenderLifetimeVerdict::Started;
        }
        self.verdict_at(now)
    }

    /// Recheck the timer before a later frame or interaction.
    pub fn verdict_at(&mut self, now: Duration) -> RenderLifetimeVerdict {
        if self.regressed(now) {
            return RenderLifetimeVerdict::Expired;
        }
        self.last_observed_at = Some(now);
        match self.first_rendered_at {
            Some(started_at) if now.saturating_sub(started_at) < self.lifetime => {
                RenderLifetimeVerdict::Active
            }
            _ => RenderLifetimeVerdict::Expired,
        }
    }

    pub fn has_started(&self) -> bool {
        self.first_rendered_at.is_some()
    }

    fn regressed(&self, now: Duration) -> bool {
        self.last_observed_at.is_some_and(|previous| now < previous)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifetime_starts_at_first_render_not_before() {
        let mut lifetime = ViewLifetime::new(Duration::from_secs(5));

        assert!(!lifetime.has_started());
        assert_eq!(
            lifetime.on_render(Duration::from_secs(100)),
            RenderLifetimeVerdict::Started
        );
        assert_eq!(
            lifetime.verdict_at(Duration::from_secs(104)),
            RenderLifetimeVerdict::Active
        );
        assert_eq!(
            lifetime.verdict_at(Duration::from_secs(105)),
            RenderLifetimeVerdict::Expired
        );
    }

    #[test]
    fn a_second_render_cannot_restart_the_lifetime() {
        let mut lifetime = ViewLifetime::new(Duration::from_secs(5));

        assert_eq!(
            lifetime.on_render(Duration::from_secs(100)),
            RenderLifetimeVerdict::Started
        );
        assert_eq!(
            lifetime.on_render(Duration::from_secs(104)),
            RenderLifetimeVerdict::Active
        );
        assert_eq!(
            lifetime.verdict_at(Duration::from_secs(105)),
            RenderLifetimeVerdict::Expired
        );
    }

    #[test]
    fn clock_set_backwards_cannot_extend_a_live_view() {
        let mut lifetime = ViewLifetime::new(Duration::from_secs(5));

        assert_eq!(
            lifetime.on_render(Duration::from_secs(100)),
            RenderLifetimeVerdict::Started
        );
        assert_eq!(
            lifetime.verdict_at(Duration::from_secs(104)),
            RenderLifetimeVerdict::Active
        );
        assert_eq!(
            lifetime.verdict_at(Duration::from_secs(99)),
            RenderLifetimeVerdict::Expired
        );
    }
}
