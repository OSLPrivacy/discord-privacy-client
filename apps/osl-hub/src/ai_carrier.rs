//! Tauri boundary for the optional AI-selected carrier.
//!
//! This state never holds plaintext, a capability, or a transport handle. The
//! bundled UI can only learn whether local AI selection is currently ready.

use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Default)]
pub struct AiCarrierState {
    local_model_ready: AtomicBool,
}

impl AiCarrierState {
    pub fn set_local_model_ready(&self, ready: bool) {
        self.local_model_ready.store(ready, Ordering::Release);
    }

    fn status(&self) -> AiCarrierStatus {
        let local_model_ready = self.local_model_ready.load(Ordering::Acquire);
        AiCarrierStatus {
            local_model_ready,
            word_bank_fallback: !local_model_ready,
        }
    }
}

/// Bounded, non-secret carrier availability for the bundled UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiCarrierStatus {
    pub local_model_ready: bool,
    pub word_bank_fallback: bool,
}

#[tauri::command]
pub fn ai_carrier_status(state: tauri::State<'_, AiCarrierState>) -> AiCarrierStatus {
    state.status()
}

#[cfg(test)]
mod tests {
    use super::AiCarrierState;

    #[test]
    fn status_truthfully_falls_back_until_a_local_model_is_ready() {
        let state = AiCarrierState::default();
        assert!(state.status().word_bank_fallback);
        state.set_local_model_ready(true);
        assert!(state.status().local_model_ready);
        assert!(!state.status().word_bank_fallback);
    }
}
