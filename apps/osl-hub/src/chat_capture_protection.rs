//! Bilateral capture-protection consent for an OSL Chat conversation.
//!
//! A peer's preference is input, never a command.  The caller persists the
//! two authored booleans and sends `effective_changed` to both peers through
//! the authenticated chat-control lane when present.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EffectiveCaptureProtection { Off, On }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptureConsent {
    pub local_opt_in: bool,
    pub peer_opt_in: bool,
    pub effective: EffectiveCaptureProtection,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConsentTransition {
    pub state: CaptureConsent,
    pub effective_changed: Option<EffectiveCaptureProtection>,
}

impl CaptureConsent {
    /// Whitelisting never enables protection by itself: each side starts off.
    pub const fn new() -> Self {
        Self { local_opt_in: false, peer_opt_in: false, effective: EffectiveCaptureProtection::Off }
    }

    /// Apply one independently authored boolean.  A mismatched pair preserves
    /// the current effective state; only a matching new pair may transition.
    pub const fn with_local_preference(self, local_opt_in: bool) -> ConsentTransition {
        self.transition(local_opt_in, self.peer_opt_in)
    }

    pub const fn with_peer_preference(self, peer_opt_in: bool) -> ConsentTransition {
        self.transition(self.local_opt_in, peer_opt_in)
    }

    const fn transition(self, local_opt_in: bool, peer_opt_in: bool) -> ConsentTransition {
        let requested = if local_opt_in == peer_opt_in {
            Some(if local_opt_in { EffectiveCaptureProtection::On } else { EffectiveCaptureProtection::Off })
        } else { None };
        let changed = match requested {
            Some(next) if next != self.effective => Some(next),
            _ => None,
        };
        ConsentTransition {
            state: Self { local_opt_in, peer_opt_in, effective: changed.unwrap_or(self.effective) },
            effective_changed: changed,
        }
    }

    /// Commit only after platform enforcement succeeded. A failed platform
    /// change leaves the previous effective state visible to both parties.
    pub const fn commit_if_enforced(self, transition: ConsentTransition, enforced: bool) -> ConsentTransition {
        if transition.effective_changed.is_some() && !enforced {
            ConsentTransition { state: self, effective_changed: None }
        } else { transition }
    }
}

impl Default for CaptureConsent { fn default() -> Self { Self::new() } }
