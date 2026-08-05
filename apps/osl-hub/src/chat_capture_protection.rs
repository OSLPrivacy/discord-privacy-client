//! Bilateral capture-protection consent for an OSL Chat conversation.
//!
//! A peer's preference is input, never a command.  The caller persists the
//! two authored booleans and sends `effective_changed` to both peers through
//! the authenticated chat-control lane when present.

use std::{collections::HashMap, sync::Mutex};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EffectiveCaptureProtection {
    Off,
    On,
}

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
        Self {
            local_opt_in: false,
            peer_opt_in: false,
            effective: EffectiveCaptureProtection::Off,
        }
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
            Some(if local_opt_in {
                EffectiveCaptureProtection::On
            } else {
                EffectiveCaptureProtection::Off
            })
        } else {
            None
        };
        // Matched on the variants rather than using `!=` and `unwrap_or`:
        // PartialEq and Option::unwrap_or are not const, so those forms do not
        // compile inside a `const fn` and broke the desktop build.
        let changed = match (requested, self.effective) {
            (Some(EffectiveCaptureProtection::On), EffectiveCaptureProtection::Off) => {
                Some(EffectiveCaptureProtection::On)
            }
            (Some(EffectiveCaptureProtection::Off), EffectiveCaptureProtection::On) => {
                Some(EffectiveCaptureProtection::Off)
            }
            _ => None,
        };
        let effective = match changed {
            Some(next) => next,
            None => self.effective,
        };
        ConsentTransition {
            state: Self {
                local_opt_in,
                peer_opt_in,
                effective,
            },
            effective_changed: changed,
        }
    }

    /// Commit only after platform enforcement succeeded. A failed platform
    /// change leaves the previous effective state visible to both parties.
    pub const fn commit_if_enforced(
        self,
        transition: ConsentTransition,
        enforced: bool,
    ) -> ConsentTransition {
        if transition.effective_changed.is_some() && !enforced {
            ConsentTransition {
                state: self,
                effective_changed: None,
            }
        } else {
            transition
        }
    }
}

impl Default for CaptureConsent {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-conversation state owned by the shipping chat runtime. A conversation is
/// entered with both authored preferences off; peer control messages can only
/// update this state through the transition methods above.
#[derive(Default)]
pub struct ChatCaptureProtectionState {
    conversations: Mutex<HashMap<String, CaptureConsent>>,
}

impl ChatCaptureProtectionState {
    pub fn ensure_conversation(&self, conversation_id: &str) -> CaptureConsent {
        let mut conversations = self
            .conversations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *conversations
            .entry(conversation_id.to_owned())
            .or_insert_with(CaptureConsent::new)
    }

    pub fn local_preference_transition(
        &self,
        conversation_id: &str,
        local_opt_in: bool,
    ) -> ConsentTransition {
        self.ensure_conversation(conversation_id)
            .with_local_preference(local_opt_in)
    }

    pub fn peer_preference_transition(
        &self,
        conversation_id: &str,
        peer_opt_in: bool,
    ) -> ConsentTransition {
        self.ensure_conversation(conversation_id)
            .with_peer_preference(peer_opt_in)
    }

    pub fn commit_transition_if_enforced(
        &self,
        conversation_id: &str,
        transition: ConsentTransition,
        enforced: bool,
    ) -> ConsentTransition {
        let mut conversations = self
            .conversations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let current = *conversations
            .entry(conversation_id.to_owned())
            .or_insert_with(CaptureConsent::new);
        let committed = current.commit_if_enforced(transition, enforced);
        *conversations
            .get_mut(conversation_id)
            .expect("conversation was inserted above") = committed.state;
        committed
    }

    pub fn apply_local_preference(
        &self,
        conversation_id: &str,
        local_opt_in: bool,
        enforced: bool,
    ) -> ConsentTransition {
        let transition = self.local_preference_transition(conversation_id, local_opt_in);
        self.commit_transition_if_enforced(conversation_id, transition, enforced)
    }

    pub fn apply_peer_preference(
        &self,
        conversation_id: &str,
        peer_opt_in: bool,
        enforced: bool,
    ) -> ConsentTransition {
        let transition = self.peer_preference_transition(conversation_id, peer_opt_in);
        self.commit_transition_if_enforced(conversation_id, transition, enforced)
    }
}

#[cfg(test)]
mod state_tests {
    use super::{ChatCaptureProtectionState, EffectiveCaptureProtection};

    #[test]
    fn shipping_conversation_state_starts_each_new_chat_with_both_sides_off() {
        let state = ChatCaptureProtectionState::default();
        let consent = state.ensure_conversation("person-a");
        assert!(!consent.local_opt_in);
        assert!(!consent.peer_opt_in);
        assert_eq!(consent.effective, EffectiveCaptureProtection::Off);
    }

    #[test]
    fn shipping_state_commits_only_matching_two_party_transitions() {
        let state = ChatCaptureProtectionState::default();

        let local_only = state.apply_local_preference("person-a", true, true);
        assert_eq!(local_only.effective_changed, None);
        assert!(local_only.state.local_opt_in);
        assert!(!local_only.state.peer_opt_in);
        assert_eq!(local_only.state.effective, EffectiveCaptureProtection::Off);

        let peer_match = state.apply_peer_preference("person-a", true, true);
        assert_eq!(
            peer_match.effective_changed,
            Some(EffectiveCaptureProtection::On)
        );
        assert_eq!(peer_match.state.effective, EffectiveCaptureProtection::On);

        let local_mismatch = state.apply_local_preference("person-a", false, true);
        assert_eq!(local_mismatch.effective_changed, None);
        assert_eq!(
            local_mismatch.state.effective,
            EffectiveCaptureProtection::On
        );

        let peer_match_off = state.apply_peer_preference("person-a", false, true);
        assert_eq!(
            peer_match_off.effective_changed,
            Some(EffectiveCaptureProtection::Off)
        );
        assert_eq!(peer_match_off.state.effective, EffectiveCaptureProtection::Off);
    }

    #[test]
    fn shipping_state_rolls_back_effective_changes_when_platform_enforcement_fails() {
        let state = ChatCaptureProtectionState::default();
        state.apply_local_preference("person-a", true, true);

        let failed = state.apply_peer_preference("person-a", true, false);
        assert_eq!(failed.effective_changed, None);
        assert_eq!(failed.state, state.ensure_conversation("person-a"));
        assert!(failed.state.local_opt_in);
        assert!(!failed.state.peer_opt_in);
        assert_eq!(failed.state.effective, EffectiveCaptureProtection::Off);
    }
}
