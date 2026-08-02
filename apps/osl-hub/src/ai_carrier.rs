//! Tauri boundary for the optional AI-selected carrier.
//!
//! This state never holds plaintext, a capability, or a transport handle. The
//! bundled UI can only learn whether local AI selection is currently ready.

use crate::ai_consent::AiCloudConsent;
use crate::credits::{unavailable_balance, BalanceDisplay};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Default)]
pub struct AiCarrierState {
    local_model_ready: AtomicBool,
    cloud_consent: AiCloudConsent,
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
}
