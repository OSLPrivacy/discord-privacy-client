//! Tauri boundary for the optional AI-selected carrier.
//!
//! This state never holds plaintext, a capability, or a transport handle. The
//! bundled UI can only learn whether local AI selection is currently ready.

use crate::ai_consent::AiCloudConsent;
use crate::credits::{unavailable_balance, BalanceDisplay};
use cover_ai::fallback::{select_carrier, CarrierCapabilities, CarrierDecision};
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Default)]
pub struct AiCarrierState {
    local_model_ready: AtomicBool,
    cloud_consent: AiCloudConsent,
    // Previewing is deliberately an opt-in held only for this running session.
    // A restart must fail closed rather than unexpectedly resuming typing into
    // a third-party composer.
    preview_enabled_scopes: Mutex<HashSet<String>>,
}

impl AiCarrierState {
    pub fn set_local_model_ready(&self, ready: bool) {
        self.local_model_ready.store(ready, Ordering::Release);
    }

    /// This record remains separate from entitlement and is read whenever the
    /// shipping status boundary is queried, so revocation takes effect without
    /// restarting the app.
    pub fn cloud_consent(&self) -> &AiCloudConsent {
        &self.cloud_consent
    }

    /// Enable or revoke carrier preview for one opaque conversation scope.
    ///
    /// Scope ids are identifiers, never message content.  The actual adapter
    /// must also pass its trusted-process result to [`can_preview_for`] at the
    /// point where it would type, so a UI preference can never bypass that
    /// boundary.
    pub fn set_preview_enabled(&self, scope_id: String, enabled: bool) -> Result<(), String> {
        if scope_id.is_empty() || scope_id.len() > 128 {
            return Err("Preview scope identifier is invalid".to_owned());
        }
        let mut scopes = self
            .preview_enabled_scopes
            .lock()
            .map_err(|_| "Preview preference state is unavailable".to_owned())?;
        if enabled {
            scopes.insert(scope_id);
        } else {
            scopes.remove(&scope_id);
        }
        Ok(())
    }

    /// Last-moment gate for any future native preview writer.  The preference
    /// is per scope and revocation is observed on the very next call.
    pub fn can_preview_for(&self, scope_id: &str, trusted_process: bool) -> bool {
        trusted_process
            && self
                .preview_enabled_scopes
                .lock()
                .is_ok_and(|scopes| scopes.contains(scope_id))
    }

    /// Resolve the optional carrier policy at the protected-send boundary.
    ///
    /// This is deliberately local-only. Cloud generation is deferred, so a
    /// consent record can never become an egress route. An unavailable local
    /// model resolves to the word-bank floor and never prevents encryption.
    pub fn select_for_shipping_send(&self) -> CarrierDecision {
        select_carrier(CarrierCapabilities {
            ai_model_available: self.local_model_ready.load(Ordering::Acquire),
            word_bank_selection_available: true,
        })
    }

    fn status(&self) -> AiCarrierStatus {
        let local_model_ready = self.local_model_ready.load(Ordering::Acquire);
        AiCarrierStatus {
            local_model_ready,
            word_bank_fallback: !local_model_ready,
            cloud_consent_granted: self.cloud_consent.is_granted(),
            cloud_credit_balance: match unavailable_balance() {
                BalanceDisplay::Current(balance) | BalanceDisplay::Stale(balance) => Some(balance.0),
                BalanceDisplay::Unknown => None,
            },
        }
    }
}

/// Bounded, non-secret carrier availability for the bundled UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiCarrierStatus {
    pub local_model_ready: bool,
    pub word_bank_fallback: bool,
    /// Separate from Pro entitlement. A future cloud sender must still check
    /// `AiCloudConsent::permits_cloud_send` at the send boundary.
    pub cloud_consent_granted: bool,
    /// `None` is an explicit unknown, never a fabricated zero balance.
    pub cloud_credit_balance: Option<u64>,
}

// The #[tauri::command] wrapper lives in main.rs with every other command:
// the macro generates __cmd__* helpers that must be in the same crate as the
// invoke_handler, so declaring it here left main.rs unable to resolve them.
pub fn ai_carrier_status_for(state: &AiCarrierState) -> AiCarrierStatus {
    state.status()
}

pub fn set_ai_carrier_preview_enabled_for(
    state: &AiCarrierState,
    scope_id: String,
    enabled: bool,
) -> Result<(), String> {
    state.set_preview_enabled(scope_id, enabled)
}

#[cfg(test)]
mod tests {
    use super::AiCarrierState;
    use crate::cloud_autoscrub_consent::CloudAutoScrubHighSensitivityAcknowledgement;

    #[test]
    fn status_truthfully_falls_back_until_a_local_model_is_ready() {
        let state = AiCarrierState::default();
        assert!(state.status().word_bank_fallback);
        state.set_local_model_ready(true);
        assert!(state.status().local_model_ready);
        assert!(!state.status().word_bank_fallback);
    }

    #[test]
    fn cloud_consent_status_reads_the_separate_record_on_every_shipping_status_request() {
        let state = AiCarrierState::default();
        assert!(!state.status().cloud_consent_granted);

        assert!(state
            .cloud_consent()
            .grant(CloudAutoScrubHighSensitivityAcknowledgement::all()));
        assert!(state.status().cloud_consent_granted);

        state.cloud_consent().revoke();
        assert!(
            !state.status().cloud_consent_granted,
            "revocation must be visible through the shipping carrier status without restart"
        );
    }

    #[test]
    fn shipping_carrier_status_reports_an_unavailable_credit_ledger_as_unknown() {
        assert_eq!(AiCarrierState::default().status().cloud_credit_balance, None);
    }

    #[test]
    fn t13_th3_preview_is_off_by_default_scoped_and_revocable() {
        let state = AiCarrierState::default();
        assert!(!state.can_preview_for("conversation-a", true));

        state
            .set_preview_enabled("conversation-a".to_owned(), true)
            .expect("a bounded opaque scope is accepted");
        assert!(state.can_preview_for("conversation-a", true));
        assert!(!state.can_preview_for("conversation-b", true));
        assert!(
            !state.can_preview_for("conversation-a", false),
            "the preference must never bypass the native adapter's trusted-process check"
        );

        state
            .set_preview_enabled("conversation-a".to_owned(), false)
            .expect("revocation is valid");
        assert!(
            !state.can_preview_for("conversation-a", true),
            "revocation takes effect before the next potential native write"
        );
    }

    #[test]
    fn t13_carrier_policy_is_called_by_every_shipping_send_before_encoding() {
        let source = include_str!("broker.rs");
        let send_boundary = source
            .find("fn prepare_peer_inbox_text(")
            .expect("shipping protected-send boundary");
        let policy = source[send_boundary..]
            .find("ai_carrier.select_for_shipping_send()")
            .map(|offset| send_boundary + offset)
            .expect("shipping send must resolve the carrier policy");
        let encode = source[send_boundary..]
            .find("prepare_peer_prose_text_inner_with_chunk(")
            .map(|offset| send_boundary + offset)
            .expect("shipping send must encode its pointer carrier");
        assert!(policy < encode, "policy must precede carrier encoding");
    }
}
