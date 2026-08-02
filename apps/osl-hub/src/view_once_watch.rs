//! Capture-protection watch for an already-rendered view-once viewer.
//!
//! A successful protection readback before rendering is only a snapshot. The
//! viewer must keep checking while pixels are present: another process can
//! change the HWND's display affinity after it has been shown.

/// The platform operations used to watch a rendered view-once viewer.
///
/// `protection_is_current` must read the platform's current affinity (on
/// Windows, `GetWindowDisplayAffinity`) rather than returning the result of
/// the earlier setup request. `close_viewer` must remove plaintext pixels
/// before it returns.
pub trait ViewOnceWatchEffects {
    type Error;

    /// Read whether capture protection is still active on the rendered viewer.
    fn protection_is_current(&mut self) -> Result<bool, Self::Error>;

    /// Close the rendered viewer after protection is lost.
    fn close_viewer(&mut self) -> Result<(), Self::Error>;
}

/// Result of one re-verification tick while a view-once viewer is rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewOnceWatchOutcome {
    /// Protection remains active; rendering may continue until the next tick.
    StillProtected,
    /// Protection was lost and the viewer was closed before returning.
    ClosedForProtectionLoss,
}

/// Re-verify capture protection for a rendered view-once viewer.
///
/// This must be called repeatedly for as long as plaintext pixels are
/// rendered. A failed readback is deliberately not treated as protected: the
/// error is propagated so the owner can close the viewer through its
/// fail-closed error path.
pub fn reverify_while_rendered<E: ViewOnceWatchEffects>(
    effects: &mut E,
) -> Result<ViewOnceWatchOutcome, E::Error> {
    if effects.protection_is_current()? {
        Ok(ViewOnceWatchOutcome::StillProtected)
    } else {
        effects.close_viewer()?;
        Ok(ViewOnceWatchOutcome::ClosedForProtectionLoss)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::{reverify_while_rendered, ViewOnceWatchEffects, ViewOnceWatchOutcome};

    struct TestEffects {
        affinity_readbacks: VecDeque<bool>,
        rendered: bool,
        close_calls: usize,
    }

    impl ViewOnceWatchEffects for TestEffects {
        type Error = &'static str;

        fn protection_is_current(&mut self) -> Result<bool, Self::Error> {
            self.affinity_readbacks
                .pop_front()
                .ok_or("missing affinity readback")
        }

        fn close_viewer(&mut self) -> Result<(), Self::Error> {
            self.close_calls += 1;
            self.rendered = false;
            Ok(())
        }
    }

    #[test]
    fn affinity_flip_mid_render_closes_the_viewer() {
        let mut effects = TestEffects {
            affinity_readbacks: VecDeque::from([true, false]),
            rendered: true,
            close_calls: 0,
        };

        assert_eq!(
            reverify_while_rendered(&mut effects),
            Ok(ViewOnceWatchOutcome::StillProtected)
        );
        assert!(effects.rendered);
        assert_eq!(effects.close_calls, 0);

        assert_eq!(
            reverify_while_rendered(&mut effects),
            Ok(ViewOnceWatchOutcome::ClosedForProtectionLoss)
        );
        assert!(!effects.rendered);
        assert_eq!(effects.close_calls, 1);
    }

    #[test]
    fn readback_failure_does_not_claim_the_viewer_is_protected() {
        let mut effects = TestEffects {
            affinity_readbacks: VecDeque::new(),
            rendered: true,
            close_calls: 0,
        };

        assert_eq!(
            reverify_while_rendered(&mut effects),
            Err("missing affinity readback")
        );
        assert!(effects.rendered);
        assert_eq!(effects.close_calls, 0);
    }
}
