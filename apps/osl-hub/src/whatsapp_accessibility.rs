//! Fail-closed WhatsApp accessibility and protected-control contracts.
//!
//! No selector is guessed here. Until exact anchors are calibrated against a
//! reviewed WhatsApp build, the production probe returns
//! `SelectorContractUnverified`. Synthetic snapshots exist only in unit tests.
//! Receipts contain commitments and geometry, never account names, chat names,
//! recipients, message text, credentials, or provider storage.

use crate::burn_contract::{ExternalCopiesEffect, LocalBurnPlan, NativeCarrierHistoryEffect};
use crate::control_contract::{
    ControlContractError, ExpiryPlan, OpenedReceiptStatus, TimedMessageDisposition,
};
#[cfg(test)]
use crate::external_overlay::{ComposerCalibration, ExternalContextBinding, VerifiedFieldKind};
use crate::external_overlay::{
    ComposerOverlayDecision, ComposerOverlayGuard, DecryptionOverlayGuard, ScreenRect,
    WindowObservation,
};
use serde::Serialize;
#[cfg(test)]
use sha2::{Digest, Sha256};
#[cfg(test)]
use std::collections::BTreeSet;

#[cfg(test)]
const MAX_RECIPIENTS: usize = 512;
#[cfg(test)]
const MAX_RUNTIME_ID: usize = 32;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WhatsAppVerificationStatus {
    Verified,
    PlatformUnsupported,
    HostUnavailable,
    SelectorContractUnverified,
    ContextIncomplete,
    RecipientSetAmbiguous,
    GeometryRejected,
    ContextChanged,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WhatsAppVerificationReceipt {
    pub status: WhatsAppVerificationStatus,
    pub provider: &'static str,
    pub selector_revision: Option<&'static str>,
    pub account_verified: bool,
    pub chat_verified: bool,
    pub recipient_set_verified: bool,
    pub composer_verified: bool,
    pub transcript_verified: bool,
    pub context_binding_sha256: Option<String>,
    pub recipient_set_sha256: Option<String>,
    pub window_generation: Option<u64>,
    pub composer_rect: Option<[i32; 4]>,
    pub transcript_rect: Option<[i32; 4]>,
    pub protected_controls_available: bool,
}

impl WhatsAppVerificationReceipt {
    fn unavailable(status: WhatsAppVerificationStatus) -> Self {
        Self {
            status,
            provider: "whatsapp",
            selector_revision: None,
            account_verified: false,
            chat_verified: false,
            recipient_set_verified: false,
            composer_verified: false,
            transcript_verified: false,
            context_binding_sha256: None,
            recipient_set_sha256: None,
            window_generation: None,
            composer_rect: None,
            transcript_rect: None,
            protected_controls_available: false,
        }
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
enum AnchorKind {
    Account,
    Chat,
    RecipientSet,
    Composer,
    Transcript,
}

#[cfg(test)]
#[derive(Debug, Clone)]
struct AnchorEvidence {
    kind: AnchorKind,
    runtime_id: Vec<i32>,
    bounds: ScreenRect,
    stable_fingerprint: [u8; 32],
}

#[cfg(test)]
#[derive(Debug, Clone)]
struct UiaSnapshot {
    selector_revision: Option<&'static str>,
    generation: u64,
    native_window_id: u64,
    window: ScreenRect,
    process_fingerprint: [u8; 32],
    account_commitment: [u8; 32],
    chat_commitment: [u8; 32],
    recipient_commitments: Vec<[u8; 32]>,
    anchors: Vec<AnchorEvidence>,
}

#[derive(Clone)]
#[allow(dead_code)]
struct VerifiedBinding {
    #[cfg(test)]
    context: ExternalContextBinding,
    context_hash: [u8; 32],
    #[cfg(test)]
    window: ScreenRect,
    composer: ScreenRect,
    transcript: ScreenRect,
}

#[derive(Default)]
pub struct WhatsAppAccessibilityState {
    binding: Option<VerifiedBinding>,
    composer_guard: ComposerOverlayGuard,
    transcript_guard: DecryptionOverlayGuard,
}

impl WhatsAppAccessibilityState {
    pub fn clear(&mut self) {
        self.binding = None;
        self.composer_guard.clear();
        self.transcript_guard.clear();
    }

    /// Production entrypoint. It deliberately does not enumerate UIA until a
    /// reviewed selector revision is available. Host identity is still checked
    /// first so an unavailable/changed borrowed window is not mislabeled.
    pub fn verify_current(
        &mut self,
        host: &crate::whatsapp_qa_host::WhatsAppQaHostState,
    ) -> WhatsAppVerificationReceipt {
        self.clear();
        #[cfg(target_os = "windows")]
        {
            return host
                .with_current_accessibility_target(|target| {
                    let _bounded_target = (
                        target.generation,
                        target.window,
                        target.process_id,
                        target.window_rect,
                    );
                    Ok(WhatsAppVerificationReceipt::unavailable(
                        WhatsAppVerificationStatus::SelectorContractUnverified,
                    ))
                })
                .unwrap_or_else(|_| {
                    WhatsAppVerificationReceipt::unavailable(
                        WhatsAppVerificationStatus::HostUnavailable,
                    )
                });
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = host;
            WhatsAppVerificationReceipt::unavailable(
                WhatsAppVerificationStatus::PlatformUnsupported,
            )
        }
    }

    pub fn composer_decision(&self, observation: &WindowObservation) -> ComposerOverlayDecision {
        self.composer_guard.observe(observation)
    }

    pub fn protected_controls(&self) -> WhatsAppProtectedControlState {
        WhatsAppProtectedControlState {
            binding: self.binding.clone(),
        }
    }

    #[cfg(test)]
    fn verify_snapshot(&mut self, snapshot: UiaSnapshot) -> WhatsAppVerificationReceipt {
        self.clear();
        let Some(revision) = snapshot.selector_revision else {
            return WhatsAppVerificationReceipt::unavailable(
                WhatsAppVerificationStatus::SelectorContractUnverified,
            );
        };
        if revision != "whatsapp-uia-test-v1" {
            return WhatsAppVerificationReceipt::unavailable(
                WhatsAppVerificationStatus::SelectorContractUnverified,
            );
        }
        let invalid_commitment = |value: &[u8; 32]| value.iter().all(|byte| *byte == 0);
        if snapshot.generation == 0
            || snapshot.native_window_id == 0
            || invalid_commitment(&snapshot.process_fingerprint)
            || invalid_commitment(&snapshot.account_commitment)
            || invalid_commitment(&snapshot.chat_commitment)
            || snapshot.recipient_commitments.is_empty()
            || snapshot.recipient_commitments.len() > MAX_RECIPIENTS
        {
            return WhatsAppVerificationReceipt::unavailable(
                WhatsAppVerificationStatus::ContextIncomplete,
            );
        }
        let mut recipients = snapshot.recipient_commitments.clone();
        recipients.sort_unstable();
        if recipients.iter().any(invalid_commitment)
            || recipients.windows(2).any(|pair| pair[0] == pair[1])
        {
            return WhatsAppVerificationReceipt::unavailable(
                WhatsAppVerificationStatus::RecipientSetAmbiguous,
            );
        }
        let kinds = snapshot
            .anchors
            .iter()
            .map(|anchor| anchor.kind)
            .collect::<BTreeSet<_>>();
        let runtime_ids = snapshot
            .anchors
            .iter()
            .map(|anchor| anchor.runtime_id.clone())
            .collect::<BTreeSet<_>>();
        if snapshot.anchors.len() != 5
            || kinds.len() != 5
            || runtime_ids.len() != 5
            || snapshot.anchors.iter().any(|anchor| {
                anchor.runtime_id.is_empty()
                    || anchor.runtime_id.len() > MAX_RUNTIME_ID
                    || invalid_commitment(&anchor.stable_fingerprint)
                    || !rect_contains(snapshot.window, anchor.bounds)
            })
        {
            return WhatsAppVerificationReceipt::unavailable(
                WhatsAppVerificationStatus::ContextIncomplete,
            );
        }
        let anchor = |kind| snapshot.anchors.iter().find(|anchor| anchor.kind == kind);
        let (Some(composer), Some(transcript)) =
            (anchor(AnchorKind::Composer), anchor(AnchorKind::Transcript))
        else {
            return WhatsAppVerificationReceipt::unavailable(
                WhatsAppVerificationStatus::ContextIncomplete,
            );
        };
        if composer.bounds.width < 240
            || composer.bounds.height < 36
            || transcript.bounds.height < 100
            || rects_overlap(composer.bounds, transcript.bounds)
            || transcript.bounds.y >= composer.bounds.y
        {
            return WhatsAppVerificationReceipt::unavailable(
                WhatsAppVerificationStatus::GeometryRejected,
            );
        }
        let recipient_set_hash = hash_recipient_set(&recipients);
        let context_hash = stable_hash(&[
            b"OSL/whatsapp-context/v1".as_slice(),
            &snapshot.account_commitment,
            &snapshot.chat_commitment,
            &recipient_set_hash,
            &snapshot.generation.to_be_bytes(),
        ]);
        let context = ExternalContextBinding {
            service_id: "whatsapp".to_owned(),
            account_id: format!("wa-{}", short_hex(&snapshot.account_commitment, 24)),
            context_id: format!("wa-{}", short_hex(&context_hash, 48)),
            native_window_id: snapshot.native_window_id,
            native_window_generation: snapshot.generation,
            process_fingerprint_sha256: snapshot.process_fingerprint,
        };
        if self
            .composer_guard
            .calibrate(ComposerCalibration {
                binding: context.clone(),
                target_window: snapshot.window,
                composer: composer.bounds,
                field_kind: VerifiedFieldKind::MessageComposer,
            })
            .is_err()
            || self
                .transcript_guard
                .bind_context(context.clone(), snapshot.window)
                .is_err()
        {
            self.clear();
            return WhatsAppVerificationReceipt::unavailable(
                WhatsAppVerificationStatus::GeometryRejected,
            );
        }
        self.binding = Some(VerifiedBinding {
            context,
            context_hash,
            window: snapshot.window,
            composer: composer.bounds,
            transcript: transcript.bounds,
        });
        WhatsAppVerificationReceipt {
            status: WhatsAppVerificationStatus::Verified,
            provider: "whatsapp",
            selector_revision: Some(revision),
            account_verified: true,
            chat_verified: true,
            recipient_set_verified: true,
            composer_verified: true,
            transcript_verified: true,
            context_binding_sha256: Some(hex(&context_hash)),
            recipient_set_sha256: Some(hex(&recipient_set_hash)),
            window_generation: Some(snapshot.generation),
            composer_rect: Some(rect_array(composer.bounds)),
            transcript_rect: Some(rect_array(transcript.bounds)),
            protected_controls_available: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WhatsAppProtectedAction {
    Text,
    MultilineUtf8,
    Covertext,
    Burn,
    Expiry,
    ReplayRejection,
    MalformedPayloadRejection,
    Media,
    Caption,
    OpenedReceipt,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WhatsAppProtectedActionStatus {
    ReadyForExplicitPlacement,
    LocalLifecycleApplied,
    RejectedSafely,
    ReceiptReady,
    ReceiptPendingOffline,
    ReceiptUnavailable,
    ContextUnverified,
    EvidenceMismatch,
}

pub enum WhatsAppPrimitiveEvidence {
    CiphertextPrepared {
        encrypted: bool,
        valid_utf8: bool,
        utf8_bytes: usize,
        hard_line_count: usize,
        covertext: bool,
        media: bool,
        caption: bool,
    },
    LocalBurn(LocalBurnPlan),
    Expiry(TimedMessageDisposition),
    Replay(ControlContractError),
    MalformedPayloadRejected {
        uniform_error: bool,
    },
    OpenedReceipt(OpenedReceiptStatus),
}

pub struct WhatsAppProtectedActionEvidence {
    pub action: WhatsAppProtectedAction,
    pub context_binding_sha256: [u8; 32],
    pub message_commitment: [u8; 32],
    pub primitive: WhatsAppPrimitiveEvidence,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WhatsAppProtectedActionReceipt {
    pub action: WhatsAppProtectedAction,
    pub status: WhatsAppProtectedActionStatus,
    pub provider: &'static str,
    pub context_binding_sha256: Option<String>,
    pub message_commitment_sha256: Option<String>,
    pub composer_rect: Option<[i32; 4]>,
    pub transcript_rect: Option<[i32; 4]>,
    pub real_message_sent: bool,
    pub provider_history_changed: bool,
    pub provider_storage_read: bool,
}

#[derive(Default)]
pub struct WhatsAppProtectedControlState {
    binding: Option<VerifiedBinding>,
}

impl WhatsAppProtectedControlState {
    pub fn evaluate(
        &self,
        evidence: WhatsAppProtectedActionEvidence,
    ) -> WhatsAppProtectedActionReceipt {
        let Some(binding) = self.binding.as_ref() else {
            return action_receipt(
                evidence.action,
                WhatsAppProtectedActionStatus::ContextUnverified,
                None,
                None,
            );
        };
        if evidence.context_binding_sha256 != binding.context_hash
            || evidence.message_commitment.iter().all(|byte| *byte == 0)
        {
            return action_receipt(
                evidence.action,
                WhatsAppProtectedActionStatus::EvidenceMismatch,
                Some(binding),
                None,
            );
        }
        let status = match (evidence.action, evidence.primitive) {
            (
                WhatsAppProtectedAction::Text,
                WhatsAppPrimitiveEvidence::CiphertextPrepared {
                    encrypted: true,
                    valid_utf8: true,
                    utf8_bytes: 1..=262_144,
                    hard_line_count: 1,
                    covertext: false,
                    media: false,
                    caption: false,
                },
            ) => WhatsAppProtectedActionStatus::ReadyForExplicitPlacement,
            (
                WhatsAppProtectedAction::MultilineUtf8,
                WhatsAppPrimitiveEvidence::CiphertextPrepared {
                    encrypted: true,
                    valid_utf8: true,
                    utf8_bytes: 1..=262_144,
                    hard_line_count: 2..=4096,
                    covertext: false,
                    media: false,
                    caption: false,
                },
            )
            | (
                WhatsAppProtectedAction::Covertext,
                WhatsAppPrimitiveEvidence::CiphertextPrepared {
                    encrypted: true,
                    valid_utf8: true,
                    utf8_bytes: 1..=262_144,
                    hard_line_count: 1..=4096,
                    covertext: true,
                    media: false,
                    caption: false,
                },
            )
            | (
                WhatsAppProtectedAction::Media,
                WhatsAppPrimitiveEvidence::CiphertextPrepared {
                    encrypted: true,
                    valid_utf8: true,
                    utf8_bytes: 1..=262_144,
                    hard_line_count: 1..=4096,
                    covertext: false,
                    media: true,
                    caption: false,
                },
            )
            | (
                WhatsAppProtectedAction::Caption,
                WhatsAppPrimitiveEvidence::CiphertextPrepared {
                    encrypted: true,
                    valid_utf8: true,
                    utf8_bytes: 1..=262_144,
                    hard_line_count: 1..=4096,
                    covertext: false,
                    media: true,
                    caption: true,
                },
            ) => WhatsAppProtectedActionStatus::ReadyForExplicitPlacement,
            (WhatsAppProtectedAction::Burn, WhatsAppPrimitiveEvidence::LocalBurn(plan))
                if local_burn_is_truthful(&plan) =>
            {
                WhatsAppProtectedActionStatus::LocalLifecycleApplied
            }
            (
                WhatsAppProtectedAction::Expiry,
                WhatsAppPrimitiveEvidence::Expiry(TimedMessageDisposition::Expired(plan)),
            ) if expiry_is_truthful(&plan) => WhatsAppProtectedActionStatus::LocalLifecycleApplied,
            (
                WhatsAppProtectedAction::ReplayRejection,
                WhatsAppPrimitiveEvidence::Replay(ControlContractError::ReplayRejected),
            )
            | (
                WhatsAppProtectedAction::MalformedPayloadRejection,
                WhatsAppPrimitiveEvidence::MalformedPayloadRejected {
                    uniform_error: true,
                },
            ) => WhatsAppProtectedActionStatus::RejectedSafely,
            (
                WhatsAppProtectedAction::OpenedReceipt,
                WhatsAppPrimitiveEvidence::OpenedReceipt(OpenedReceiptStatus::Ready),
            ) => WhatsAppProtectedActionStatus::ReceiptReady,
            (
                WhatsAppProtectedAction::OpenedReceipt,
                WhatsAppPrimitiveEvidence::OpenedReceipt(OpenedReceiptStatus::PendingOffline),
            ) => WhatsAppProtectedActionStatus::ReceiptPendingOffline,
            (
                WhatsAppProtectedAction::OpenedReceipt,
                WhatsAppPrimitiveEvidence::OpenedReceipt(_),
            ) => WhatsAppProtectedActionStatus::ReceiptUnavailable,
            _ => WhatsAppProtectedActionStatus::EvidenceMismatch,
        };
        action_receipt(
            evidence.action,
            status,
            Some(binding),
            Some(evidence.message_commitment),
        )
    }
}

fn action_receipt(
    action: WhatsAppProtectedAction,
    status: WhatsAppProtectedActionStatus,
    binding: Option<&VerifiedBinding>,
    message: Option<[u8; 32]>,
) -> WhatsAppProtectedActionReceipt {
    WhatsAppProtectedActionReceipt {
        action,
        status,
        provider: "whatsapp",
        context_binding_sha256: binding.map(|binding| hex(&binding.context_hash)),
        message_commitment_sha256: message.map(|message| hex(&message)),
        composer_rect: binding.map(|binding| rect_array(binding.composer)),
        transcript_rect: binding.map(|binding| rect_array(binding.transcript)),
        real_message_sent: false,
        provider_history_changed: false,
        provider_storage_read: false,
    }
}

fn local_burn_is_truthful(plan: &LocalBurnPlan) -> bool {
    plan.destroy_local_decrypt_capability
        && plan.destroy_local_key_mappings
        && plan.clear_local_caches
        && plan.native_carrier_history == NativeCarrierHistoryEffect::Unchanged
        && plan.screenshots_exports_and_external_copies == ExternalCopiesEffect::NotControllable
}

fn expiry_is_truthful(plan: &ExpiryPlan) -> bool {
    plan.destroy_local_decrypt_keys
        && plan.clear_plaintext_and_local_caches
        && plan.native_carrier_history == NativeCarrierHistoryEffect::Unchanged
        && plan.screenshots_exports_and_external_copies == ExternalCopiesEffect::NotControllable
}

#[cfg(test)]
fn rect_contains(parent: ScreenRect, child: ScreenRect) -> bool {
    let parent_right = parent.x.checked_add_unsigned(parent.width);
    let parent_bottom = parent.y.checked_add_unsigned(parent.height);
    let child_right = child.x.checked_add_unsigned(child.width);
    let child_bottom = child.y.checked_add_unsigned(child.height);
    parent.width > 0
        && parent.height > 0
        && child.width > 0
        && child.height > 0
        && child.x >= parent.x
        && child.y >= parent.y
        && child_right
            .zip(parent_right)
            .is_some_and(|(child, parent)| child <= parent)
        && child_bottom
            .zip(parent_bottom)
            .is_some_and(|(child, parent)| child <= parent)
}

#[cfg(test)]
fn rects_overlap(left: ScreenRect, right: ScreenRect) -> bool {
    let left_right = left.x.saturating_add_unsigned(left.width);
    let right_right = right.x.saturating_add_unsigned(right.width);
    let left_bottom = left.y.saturating_add_unsigned(left.height);
    let right_bottom = right.y.saturating_add_unsigned(right.height);
    left.x < right_right && right.x < left_right && left.y < right_bottom && right.y < left_bottom
}

fn rect_array(rect: ScreenRect) -> [i32; 4] {
    [
        rect.x,
        rect.y,
        rect.x.saturating_add_unsigned(rect.width),
        rect.y.saturating_add_unsigned(rect.height),
    ]
}

#[cfg(test)]
fn hash_recipient_set(recipients: &[[u8; 32]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"OSL/whatsapp-recipient-set/v1");
    hasher.update((recipients.len() as u32).to_be_bytes());
    for recipient in recipients {
        hasher.update(recipient);
    }
    hasher.finalize().into()
}

#[cfg(test)]
fn stable_hash(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update((part.len() as u32).to_be_bytes());
        hasher.update(part);
    }
    hasher.finalize().into()
}

fn hex(value: &[u8]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
fn short_hex(value: &[u8], chars: usize) -> String {
    hex(value).chars().take(chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::burn_contract::{BurnScopeCommitment, BurnScopeLevel};

    fn bytes(value: u8) -> [u8; 32] {
        [value; 32]
    }

    fn rect(x: i32, y: i32, width: u32, height: u32) -> ScreenRect {
        ScreenRect {
            x,
            y,
            width,
            height,
        }
    }

    fn snapshot() -> UiaSnapshot {
        let anchor = |kind, runtime, bounds, fingerprint| AnchorEvidence {
            kind,
            runtime_id: vec![42, runtime],
            bounds,
            stable_fingerprint: bytes(fingerprint),
        };
        UiaSnapshot {
            selector_revision: Some("whatsapp-uia-test-v1"),
            generation: 7,
            native_window_id: 11,
            window: rect(100, 100, 1000, 800),
            process_fingerprint: bytes(1),
            account_commitment: bytes(2),
            chat_commitment: bytes(3),
            recipient_commitments: vec![bytes(4), bytes(5)],
            anchors: vec![
                anchor(AnchorKind::Account, 1, rect(120, 120, 200, 40), 11),
                anchor(AnchorKind::Chat, 2, rect(350, 120, 300, 40), 12),
                anchor(AnchorKind::RecipientSet, 3, rect(660, 120, 200, 40), 13),
                anchor(AnchorKind::Transcript, 4, rect(340, 180, 730, 600), 14),
                anchor(AnchorKind::Composer, 5, rect(340, 800, 730, 70), 15),
            ],
        }
    }

    fn verified() -> (WhatsAppAccessibilityState, WhatsAppVerificationReceipt) {
        let mut state = WhatsAppAccessibilityState::default();
        let receipt = state.verify_snapshot(snapshot());
        (state, receipt)
    }

    fn action(
        kind: WhatsAppProtectedAction,
        context: [u8; 32],
        primitive: WhatsAppPrimitiveEvidence,
    ) -> WhatsAppProtectedActionEvidence {
        WhatsAppProtectedActionEvidence {
            action: kind,
            context_binding_sha256: context,
            message_commitment: bytes(99),
            primitive,
        }
    }

    #[test]
    fn production_without_reviewed_selectors_is_explicitly_unverified() {
        let mut state = WhatsAppAccessibilityState::default();
        let mut unknown = snapshot();
        unknown.selector_revision = None;
        let receipt = state.verify_snapshot(unknown);
        assert_eq!(
            receipt.status,
            WhatsAppVerificationStatus::SelectorContractUnverified
        );
        assert!(!receipt.protected_controls_available);
        assert!(state.binding.is_none());
    }

    #[test]
    fn exact_account_chat_recipient_composer_and_transcript_bind_geometry() {
        let (state, receipt) = verified();
        assert_eq!(receipt.status, WhatsAppVerificationStatus::Verified);
        assert!(receipt.account_verified && receipt.recipient_set_verified);
        assert_eq!(receipt.composer_rect, Some([340, 800, 1070, 870]));
        let binding = state.binding.as_ref().unwrap();
        let observation = WindowObservation {
            binding: binding.context.clone(),
            target_window: binding.window,
            foreground: true,
            minimized: false,
            geometry_certain: true,
            focused_field: VerifiedFieldKind::MessageComposer,
            focused_field_bounds: Some(binding.composer),
        };
        assert_eq!(
            state.composer_decision(&observation),
            ComposerOverlayDecision::Show(binding.composer)
        );
    }

    #[test]
    fn duplicate_recipients_changed_runtime_ids_and_overlap_fail_closed() {
        let mut state = WhatsAppAccessibilityState::default();
        let mut duplicate = snapshot();
        duplicate.recipient_commitments = vec![bytes(4), bytes(4)];
        assert_eq!(
            state.verify_snapshot(duplicate).status,
            WhatsAppVerificationStatus::RecipientSetAmbiguous
        );

        let mut runtime = snapshot();
        runtime.anchors[1].runtime_id = runtime.anchors[0].runtime_id.clone();
        assert_eq!(
            state.verify_snapshot(runtime).status,
            WhatsAppVerificationStatus::ContextIncomplete
        );

        let mut overlap = snapshot();
        overlap.anchors[4].bounds = rect(340, 700, 730, 100);
        assert_eq!(
            state.verify_snapshot(overlap).status,
            WhatsAppVerificationStatus::GeometryRejected
        );
    }

    #[test]
    fn text_multiline_utf8_covertext_media_and_caption_only_become_ready() {
        let (state, _) = verified();
        let binding = state.binding.as_ref().unwrap().context_hash;
        let controls = state.protected_controls();
        for (kind, lines, covertext, media, caption) in [
            (WhatsAppProtectedAction::Text, 1, false, false, false),
            (
                WhatsAppProtectedAction::MultilineUtf8,
                3,
                false,
                false,
                false,
            ),
            (WhatsAppProtectedAction::Covertext, 2, true, false, false),
            (WhatsAppProtectedAction::Media, 1, false, true, false),
            (WhatsAppProtectedAction::Caption, 2, false, true, true),
        ] {
            let receipt = controls.evaluate(action(
                kind,
                binding,
                WhatsAppPrimitiveEvidence::CiphertextPrepared {
                    encrypted: true,
                    valid_utf8: true,
                    utf8_bytes: 21,
                    hard_line_count: lines,
                    covertext,
                    media,
                    caption,
                },
            ));
            assert_eq!(
                receipt.status,
                WhatsAppProtectedActionStatus::ReadyForExplicitPlacement
            );
            assert!(!receipt.real_message_sent && !receipt.provider_storage_read);
        }
    }

    #[test]
    fn generic_burn_expiry_replay_malformed_and_receipt_truth_is_preserved() {
        let (state, _) = verified();
        let binding = state.binding.as_ref().unwrap().context_hash;
        let controls = state.protected_controls();
        let scope = BurnScopeCommitment {
            level: BurnScopeLevel::CurrentChat,
            digest: bytes(6),
        };
        let burn = LocalBurnPlan {
            scope,
            destroy_local_decrypt_capability: true,
            destroy_local_key_mappings: true,
            clear_local_caches: true,
            forget_incoming_and_member_messages: false,
            native_carrier_history: NativeCarrierHistoryEffect::Unchanged,
            screenshots_exports_and_external_copies: ExternalCopiesEffect::NotControllable,
            may_offer_separate_uninstall_after_completion: false,
        };
        assert_eq!(
            controls
                .evaluate(action(
                    WhatsAppProtectedAction::Burn,
                    binding,
                    WhatsAppPrimitiveEvidence::LocalBurn(burn)
                ))
                .status,
            WhatsAppProtectedActionStatus::LocalLifecycleApplied
        );
        let expiry = ExpiryPlan {
            message_commitment: bytes(99),
            destroy_local_decrypt_keys: true,
            clear_plaintext_and_local_caches: true,
            request_encrypted_blob_deletion: true,
            may_destroy_unread_content: false,
            native_carrier_history: NativeCarrierHistoryEffect::Unchanged,
            screenshots_exports_and_external_copies: ExternalCopiesEffect::NotControllable,
        };
        assert_eq!(
            controls
                .evaluate(action(
                    WhatsAppProtectedAction::Expiry,
                    binding,
                    WhatsAppPrimitiveEvidence::Expiry(TimedMessageDisposition::Expired(expiry))
                ))
                .status,
            WhatsAppProtectedActionStatus::LocalLifecycleApplied
        );
        assert_eq!(
            controls
                .evaluate(action(
                    WhatsAppProtectedAction::ReplayRejection,
                    binding,
                    WhatsAppPrimitiveEvidence::Replay(ControlContractError::ReplayRejected)
                ))
                .status,
            WhatsAppProtectedActionStatus::RejectedSafely
        );
        assert_eq!(
            controls
                .evaluate(action(
                    WhatsAppProtectedAction::MalformedPayloadRejection,
                    binding,
                    WhatsAppPrimitiveEvidence::MalformedPayloadRejected {
                        uniform_error: true
                    }
                ))
                .status,
            WhatsAppProtectedActionStatus::RejectedSafely
        );
        assert_eq!(
            controls
                .evaluate(action(
                    WhatsAppProtectedAction::OpenedReceipt,
                    binding,
                    WhatsAppPrimitiveEvidence::OpenedReceipt(OpenedReceiptStatus::PendingOffline)
                ))
                .status,
            WhatsAppProtectedActionStatus::ReceiptPendingOffline
        );
    }

    #[test]
    fn changed_context_and_mismatched_evidence_never_reach_ready() {
        let (state, _) = verified();
        let controls = state.protected_controls();
        let receipt = controls.evaluate(action(
            WhatsAppProtectedAction::Text,
            bytes(88),
            WhatsAppPrimitiveEvidence::CiphertextPrepared {
                encrypted: true,
                valid_utf8: true,
                utf8_bytes: 10,
                hard_line_count: 1,
                covertext: false,
                media: false,
                caption: false,
            },
        ));
        assert_eq!(
            receipt.status,
            WhatsAppProtectedActionStatus::EvidenceMismatch
        );
        assert!(!receipt.real_message_sent);
    }
}
