//! Tauri boundary for the optional AI-selected carrier.
//!
//! This state never holds plaintext, a capability, or a transport handle. The
//! bundled UI can only learn whether local AI selection is currently ready.

use crate::bundled_model_pack::{
    ensure_bundled_model_pack, BundledCoverWriter, BundledModelPackError, BundledModelPackStatus,
    CoverShapeConstraints,
};
use cover_ai::fallback::{select_carrier, CarrierCapabilities, CarrierDecision};
use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

#[derive(Default)]
pub struct AiCarrierState {
    local_model_ready: AtomicBool,
    // The shared Covertext button chooses the built-in layered wordbank for
    // this running session.  It is deliberately separate from model
    // availability: an unpressed choice keeps the established cover writer,
    // while pressing the plain Covertext button selects the local wordbank.
    wordbank_writer_selected: AtomicBool,
    ai_covertext_selected: AtomicBool,
    local_writer: Mutex<Option<BundledCoverWriter>>,
    // Previewing is deliberately an opt-in held only for this running session.
    // A restart must fail closed rather than unexpectedly resuming typing into
    // a third-party composer.
    preview_enabled_scopes: Mutex<HashSet<String>>,
}

impl AiCarrierState {
    pub fn set_local_model_ready(&self, ready: bool) {
        self.local_model_ready.store(ready, Ordering::Release);
        if !ready {
            self.ai_covertext_selected.store(false, Ordering::Release);
        }
    }

    pub fn set_wordbank_writer_selected(&self, selected: bool) {
        self.wordbank_writer_selected
            .store(selected, Ordering::Release);
    }

    pub fn wordbank_writer_selected(&self) -> bool {
        self.wordbank_writer_selected.load(Ordering::Acquire)
    }

    pub fn ensure_bundled_local_model(
        &self,
        install_root: &Path,
    ) -> Result<BundledModelPackStatus, BundledModelPackError> {
        match ensure_bundled_model_pack(install_root) {
            Ok(status) => {
                let writer = BundledCoverWriter::load(&status.artifact_path)?;
                *self
                    .local_writer
                    .lock()
                    .map_err(|_| BundledModelPackError::InvalidModelFile)? = Some(writer);
                self.set_local_model_ready(true);
                Ok(status)
            }
            Err(error) => {
                self.set_local_model_ready(false);
                Err(error)
            }
        }
    }

    /// Select the AI writer for subsequent carriers. This is the backend
    /// action behind the visible AI Covertext button; it refuses only when the
    /// verified bundled pack is genuinely unavailable.
    pub fn set_ai_covertext_selected(&self, selected: bool) -> Result<AiCarrierStatus, String> {
        if selected && !self.local_model_ready.load(Ordering::Acquire) {
            return Err("AI Covertext needs the verified local model pack".to_owned());
        }
        self.ai_covertext_selected
            .store(selected, Ordering::Release);
        Ok(self.status())
    }

    /// Ask the local writer for fresh cover entropy. Its input type contains
    /// only length and line-shape counts, so private words cannot cross this
    /// boundary accidentally.
    pub fn next_cover_entropy(
        &self,
        shape: &CoverShapeConstraints,
    ) -> Result<Option<[u8; 32]>, String> {
        if !self.ai_covertext_selected.load(Ordering::Acquire) {
            return Ok(None);
        }
        let mut writer = self
            .local_writer
            .lock()
            .map_err(|_| "The local AI cover writer is unavailable".to_owned())?;
        let writer = writer
            .as_mut()
            .ok_or_else(|| "The local AI cover writer is unavailable".to_owned())?;
        writer
            .generate_cover_entropy(shape)
            .map(|entropy| Some(entropy.into_bytes()))
            .map_err(|_| "The local AI cover writer could not produce a cover".to_owned())
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
    /// This is deliberately local-only. An unavailable local model resolves
    /// to the word-bank floor and never prevents encryption.
    pub fn select_for_shipping_send(&self) -> CarrierDecision {
        select_carrier(CarrierCapabilities {
            ai_model_available: self.local_model_ready.load(Ordering::Acquire)
                && self.ai_covertext_selected.load(Ordering::Acquire),
            word_bank_selection_available: true,
        })
    }

    fn status(&self) -> AiCarrierStatus {
        let local_model_ready = self.local_model_ready.load(Ordering::Acquire);
        AiCarrierStatus {
            local_model_ready,
            ai_covertext_selected: self.ai_covertext_selected.load(Ordering::Acquire),
            word_bank_fallback: !local_model_ready,
        }
    }
}

/// Bounded, non-secret carrier availability for the bundled UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiCarrierStatus {
    pub local_model_ready: bool,
    pub ai_covertext_selected: bool,
    pub word_bank_fallback: bool,
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

pub fn set_ai_covertext_selected_for(
    state: &AiCarrierState,
    selected: bool,
) -> Result<AiCarrierStatus, String> {
    state.set_ai_covertext_selected(selected)
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

    #[test]
    fn task_3520_plain_covertext_button_selects_the_wordbank_writer() {
        let state = AiCarrierState::default();
        assert!(!state.wordbank_writer_selected());
        state.set_wordbank_writer_selected(true);
        assert!(state.wordbank_writer_selected());
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
