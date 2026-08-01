//! Fail-closed friction handling for a hosted Scrub batch.
//!
//! Any sign that the provider UI is no longer safe to drive ends this run.
//! A caller must create a new guard for an owner-initiated later run; this
//! type deliberately has no operation that clears a recorded stop.

/// Provider UI conditions that make a hosted Scrub action ambiguous or unsafe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostedFriction {
    Captcha,
    Challenge,
    RateLimit,
    SignedOut,
    AccountChanged,
    SchemaDrift,
    Unknown,
}

/// The only honest batch result after hosted UI friction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostedBatchOutcome {
    Unknown,
}

/// A permanent stop recorded for the current batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostedFrictionStop {
    pub reason: HostedFriction,
    pub batch_outcome: HostedBatchOutcome,
}

/// Latches the first friction signal for one hosted Scrub batch.
///
/// The latch is intentionally monotonic: once stopped, no later signal can
/// turn the batch back into an executable run. In particular, rate limiting is
/// not a cue to wait and retry within this batch.
#[derive(Debug, Default)]
pub struct HostedFrictionGuard {
    stopped: Option<HostedFrictionStop>,
}

impl HostedFrictionGuard {
    /// Starts an executable hosted Scrub batch.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records friction and permanently stops this batch.
    ///
    /// The first reason is retained so a later, less precise failure cannot
    /// overwrite the reason the owner needs to see before starting another run.
    pub fn stop(&mut self, reason: HostedFriction) -> HostedFrictionStop {
        *self.stopped.get_or_insert(HostedFrictionStop {
            reason,
            batch_outcome: HostedBatchOutcome::Unknown,
        })
    }

    /// Refuses another provider action after any friction signal.
    pub fn before_action(&self) -> Result<(), HostedFrictionStop> {
        self.stopped.map_or(Ok(()), Err)
    }

    /// Returns the permanent stop, if this batch has encountered friction.
    pub fn stop_record(&self) -> Option<HostedFrictionStop> {
        self.stopped
    }
}

#[cfg(test)]
mod tests {
    use super::{HostedBatchOutcome, HostedFriction, HostedFrictionGuard};

    #[test]
    fn scr_h4_every_friction_stops_the_run_and_leaves_the_batch_unknown() {
        for reason in [
            HostedFriction::Captcha,
            HostedFriction::Challenge,
            HostedFriction::RateLimit,
            HostedFriction::SignedOut,
            HostedFriction::AccountChanged,
            HostedFriction::SchemaDrift,
            HostedFriction::Unknown,
        ] {
            let mut guard = HostedFrictionGuard::new();

            let stop = guard.stop(reason);

            assert_eq!(stop.reason, reason);
            assert_eq!(stop.batch_outcome, HostedBatchOutcome::Unknown);
            assert_eq!(guard.before_action(), Err(stop));
            assert_eq!(guard.stop_record(), Some(stop));
        }
    }

    #[test]
    fn scr_h4_rate_limit_never_backs_off_and_continues_in_the_same_run() {
        let mut guard = HostedFrictionGuard::new();
        let rate_limit_stop = guard.stop(HostedFriction::RateLimit);

        // A later signal cannot clear or replace the rate-limit stop.
        assert_eq!(guard.stop(HostedFriction::Unknown), rate_limit_stop);
        assert_eq!(guard.before_action(), Err(rate_limit_stop));
    }
}
