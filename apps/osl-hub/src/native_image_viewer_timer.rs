//! Monotonic auto-close and shred sequencing for the protected image viewer.
//!
//! The native viewer starts this timer at its first visible frame.  The timer
//! is deliberately independent of wall-clock time: changing the system clock
//! must never buy extra time with plaintext pixels on screen.

use std::time::{Duration, Instant};

/// The action that removes plaintext when a protected view ends.
///
/// Implementations must make `shred_staging` remove every decrypted staging
/// artifact owned by the view.  It runs even when closing the native window
/// reports an error: a failed close is never a reason to retain plaintext.
pub(crate) trait ProtectedViewerCloseEffects {
    type Error;

    fn close_viewer(&mut self) -> Result<(), Self::Error>;
    fn shred_staging(&mut self) -> Result<(), Self::Error>;
}

/// Result of checking a protected viewer's one-shot deadline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AutoCloseVerdict {
    /// The view remains within its exact lifetime.
    Active,
    /// The deadline was reached and the close/shred sequence ran.
    Closed,
}

/// A non-resettable deadline measured from the first visible frame.
#[derive(Debug)]
pub(crate) struct ProtectedViewerTimer {
    deadline: Instant,
    fired: bool,
}

impl ProtectedViewerTimer {
    /// Start the timer when, and only when, the viewer begins rendering.
    pub(crate) fn from_first_render(lifetime: Duration) -> Self {
        Self {
            deadline: Instant::now() + lifetime,
            fired: false,
        }
    }

    /// Close and shred exactly once once the monotonic deadline is reached.
    pub(crate) fn poll_at<E: ProtectedViewerCloseEffects>(
        &mut self,
        now: Instant,
        effects: &mut E,
    ) -> Result<AutoCloseVerdict, E::Error> {
        if self.fired || now < self.deadline {
            return Ok(AutoCloseVerdict::Active);
        }
        self.fired = true;
        close_and_shred(effects)?;
        Ok(AutoCloseVerdict::Closed)
    }

    #[cfg(test)]
    fn deadline(&self) -> Instant {
        self.deadline
    }
}

/// Perform terminal cleanup in the required order.
///
/// Closing first removes plaintext pixels from the display.  Shredding is
/// attempted regardless of close success, and a close failure remains the
/// returned error if both operations fail.
pub(crate) fn close_and_shred<E: ProtectedViewerCloseEffects>(
    effects: &mut E,
) -> Result<(), E::Error> {
    let close = effects.close_viewer();
    let shred = effects.shred_staging();
    close.and(shred)
}

#[cfg(test)]
mod tests {
    use super::{AutoCloseVerdict, ProtectedViewerCloseEffects, ProtectedViewerTimer};
    use std::time::{Duration, Instant};

    #[derive(Default)]
    struct TestEffects {
        viewer_closed: bool,
        decrypted_staging: Option<Vec<u8>>,
        close_fails: bool,
    }

    impl ProtectedViewerCloseEffects for TestEffects {
        type Error = &'static str;

        fn close_viewer(&mut self) -> Result<(), Self::Error> {
            self.viewer_closed = true;
            (!self.close_fails).then_some(()).ok_or("close failed")
        }

        fn shred_staging(&mut self) -> Result<(), Self::Error> {
            self.decrypted_staging = None;
            Ok(())
        }
    }

    #[test]
    fn deadline_closes_and_shreds_decrypted_staging_once() {
        let mut timer = ProtectedViewerTimer::from_first_render(Duration::from_secs(5));
        let mut effects = TestEffects {
            decrypted_staging: Some(b"decrypted image bytes".to_vec()),
            ..TestEffects::default()
        };
        let deadline = timer.deadline();

        assert_eq!(
            timer.poll_at(deadline - Duration::from_nanos(1), &mut effects),
            Ok(AutoCloseVerdict::Active)
        );
        assert!(!effects.viewer_closed);
        assert!(effects.decrypted_staging.is_some());

        assert_eq!(
            timer.poll_at(deadline, &mut effects),
            Ok(AutoCloseVerdict::Closed)
        );
        assert!(effects.viewer_closed);
        assert_eq!(effects.decrypted_staging, None);

        assert_eq!(
            timer.poll_at(deadline + Duration::from_secs(60), &mut effects),
            Ok(AutoCloseVerdict::Active)
        );
    }

    #[test]
    fn failed_close_does_not_leave_decrypted_staging_behind() {
        let mut timer = ProtectedViewerTimer::from_first_render(Duration::ZERO);
        let mut effects = TestEffects {
            decrypted_staging: Some(b"decrypted image bytes".to_vec()),
            close_fails: true,
            ..TestEffects::default()
        };

        assert_eq!(
            timer.poll_at(Instant::now(), &mut effects),
            Err("close failed")
        );
        assert!(effects.viewer_closed);
        assert_eq!(effects.decrypted_staging, None);
    }
}
