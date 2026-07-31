//! Fail-closed Signal Desktop destination and composer binding contract.
//!
//! This is deliberately a pure state machine. It does not inspect Signal,
//! read its private storage, or grant authority from window geometry. A future
//! Windows adapter must supply a fresh live-UIA observation of the exact
//! account, conversation, participant set, and message composer. Until that
//! happens, no attestation exists and protected send remains unavailable.
//!
//! Provider-derived values are represented only by one-way digests and never
//! appear in returned receipts. Receipts contain fixed semantic enums and
//! counters only, so callers can log them without disclosing account names,
//! conversation names, recipients, window text, or message contents.

use serde::Serialize;
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

const DEFAULT_ATTESTATION_TTL_MS: u64 = 2_000;
const MAX_ATTESTATION_TTL_MS: u64 = 5_000;
const MAX_REPLAY_ENTRIES: usize = 4_096;

type Digest = [u8; 32];

/// Opaque live evidence produced by the trusted Signal UIA adapter.
///
/// This type intentionally does not implement `Debug` or `Serialize`: raw
/// binding digests must not accidentally cross IPC or enter diagnostic logs.
#[derive(Clone, Eq, PartialEq)]
pub struct SignalDestinationEvidence {
    pub host_generation: u64,
    pub window_identity_sha256: Digest,
    pub account_binding_sha256: Digest,
    pub conversation_binding_sha256: Digest,
    pub participant_set_sha256: Digest,
    pub composer_identity_sha256: Digest,
    pub attestation_nonce_sha256: Digest,
    pub observed_at_ms: u64,
    pub window_foreground: bool,
    pub composer_focused: bool,
    pub conversation_stable: bool,
}

/// Exact destination selected for a single protected-send attempt.
///
/// Like evidence, the intent is deliberately not serializable or debuggable.
#[derive(Clone, Eq, PartialEq)]
pub struct SignalProtectedSendIntent {
    pub account_binding_sha256: Digest,
    pub conversation_binding_sha256: Digest,
    pub participant_set_sha256: Digest,
    pub composer_identity_sha256: Digest,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SignalBindingStatus {
    Accepted,
    Rejected,
    Invalidated,
    Authorized,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SignalBindingReason {
    None,
    NoClaimedWindow,
    MalformedEvidence,
    WindowGenerationChanged,
    WindowIdentityChanged,
    WindowUnavailable,
    WindowNotForeground,
    ComposerNotFocused,
    ConversationUnstable,
    ConversationChanged,
    ComposerChanged,
    StaleEvidence,
    ReplayedEvidence,
    ReplayJournalFull,
    NoFreshAttestation,
    AttestationExpired,
    AttestationSuperseded,
    DestinationMismatch,
    AlreadyConsumed,
    StateUnavailable,
}

/// Safe diagnostic result. It never contains provider-derived identifiers.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalBindingReceipt {
    pub status: SignalBindingStatus,
    pub reason: SignalBindingReason,
    pub lifecycle_generation: u64,
    pub attestation_generation: u64,
    pub valid_for_ms: u64,
}

struct ClaimedWindow {
    host_generation: u64,
    window_identity_sha256: Digest,
}

struct FreshAttestation {
    lifecycle_generation: u64,
    attestation_generation: u64,
    expires_at_ms: u64,
    evidence: SignalDestinationEvidence,
    consumed: bool,
}

/// Owns all ephemeral Signal send authority for the current process.
///
/// The state is intentionally memory-only. Reconnects and process restarts
/// require new live observations; no authority is restored from disk.
pub struct SignalDestinationBindingGuard {
    claimed_window: Option<ClaimedWindow>,
    lifecycle_generation: u64,
    next_attestation_generation: u64,
    ttl_ms: u64,
    active: Option<FreshAttestation>,
    seen_nonces: HashSet<Digest>,
}

/// Tauri-managed, process-local holder for the native Signal proof state.
/// Renderer code can query readiness but cannot inject evidence or mint send
/// authority through IPC.
pub struct SignalDestinationBindingState {
    guard: Mutex<SignalDestinationBindingGuard>,
}

impl Default for SignalDestinationBindingState {
    fn default() -> Self {
        Self {
            guard: Mutex::new(SignalDestinationBindingGuard::default()),
        }
    }
}

impl SignalDestinationBindingState {
    /// Native host hook. No equivalent renderer command is registered.
    pub fn claim_native_window(
        &self,
        host_generation: u64,
        window_identity_sha256: Digest,
    ) -> SignalBindingReceipt {
        self.with_guard(|guard| guard.claim_window(host_generation, window_identity_sha256))
    }

    /// Revalidates the currently claimed native window without granting any
    /// composer or destination authority.
    pub fn verify_native_window(
        &self,
        host_generation: u64,
        window_identity_sha256: Digest,
    ) -> SignalBindingReceipt {
        self.with_guard(|guard| {
            guard.verify_claimed_window(host_generation, window_identity_sha256)
        })
    }

    pub fn invalidate_focus_transition(&self) -> SignalBindingReceipt {
        self.with_guard(SignalDestinationBindingGuard::focus_lost)
    }

    pub fn invalidate_window(&self) -> SignalBindingReceipt {
        self.with_guard(SignalDestinationBindingGuard::window_unavailable)
    }

    /// Returns only a redacted semantic readiness receipt.
    pub fn readiness(&self) -> SignalBindingReceipt {
        self.with_guard(|guard| guard.readiness(monotonic_now_ms()))
    }

    fn with_guard(
        &self,
        operation: impl FnOnce(&mut SignalDestinationBindingGuard) -> SignalBindingReceipt,
    ) -> SignalBindingReceipt {
        self.guard.lock().map_or_else(
            |_| SignalBindingReceipt {
                status: SignalBindingStatus::Rejected,
                reason: SignalBindingReason::StateUnavailable,
                lifecycle_generation: 0,
                attestation_generation: 0,
                valid_for_ms: 0,
            },
            |mut guard| operation(&mut guard),
        )
    }
}

impl Default for SignalDestinationBindingGuard {
    fn default() -> Self {
        Self::new(DEFAULT_ATTESTATION_TTL_MS).expect("the built-in Signal attestation TTL is valid")
    }
}

impl SignalDestinationBindingGuard {
    pub fn new(ttl_ms: u64) -> Result<Self, SignalBindingReason> {
        if ttl_ms == 0 || ttl_ms > MAX_ATTESTATION_TTL_MS {
            return Err(SignalBindingReason::MalformedEvidence);
        }
        Ok(Self {
            claimed_window: None,
            lifecycle_generation: 0,
            next_attestation_generation: 0,
            ttl_ms,
            active: None,
            seen_nonces: HashSet::new(),
        })
    }

    /// Starts a new claimed-window lifecycle. Calling this for a reconnect,
    /// even with the same exact window identity, invalidates prior authority.
    pub fn claim_window(
        &mut self,
        host_generation: u64,
        window_identity_sha256: Digest,
    ) -> SignalBindingReceipt {
        if host_generation == 0 || !nonzero(window_identity_sha256) {
            self.invalidate(SignalBindingReason::MalformedEvidence);
            self.claimed_window = None;
            return self.receipt(
                SignalBindingStatus::Rejected,
                SignalBindingReason::MalformedEvidence,
                0,
            );
        }
        self.bump_lifecycle();
        self.active = None;
        self.claimed_window = Some(ClaimedWindow {
            host_generation,
            window_identity_sha256,
        });
        self.receipt(
            SignalBindingStatus::Invalidated,
            SignalBindingReason::None,
            0,
        )
    }

    pub fn window_unavailable(&mut self) -> SignalBindingReceipt {
        self.claimed_window = None;
        self.invalidate(SignalBindingReason::WindowUnavailable)
    }

    pub fn verify_claimed_window(
        &mut self,
        host_generation: u64,
        window_identity_sha256: Digest,
    ) -> SignalBindingReceipt {
        let Some(claimed) = self.claimed_window.as_ref() else {
            return self.receipt(
                SignalBindingStatus::Rejected,
                SignalBindingReason::NoClaimedWindow,
                0,
            );
        };
        let reason = if host_generation != claimed.host_generation {
            Some(SignalBindingReason::WindowGenerationChanged)
        } else if window_identity_sha256 != claimed.window_identity_sha256 {
            Some(SignalBindingReason::WindowIdentityChanged)
        } else {
            None
        };
        if let Some(reason) = reason {
            self.claimed_window = None;
            return self.invalidate(reason);
        }
        self.readiness(monotonic_now_ms())
    }

    pub fn focus_lost(&mut self) -> SignalBindingReceipt {
        self.invalidate(SignalBindingReason::WindowNotForeground)
    }

    pub fn conversation_changed(&mut self) -> SignalBindingReceipt {
        self.invalidate(SignalBindingReason::ConversationChanged)
    }

    pub fn composer_changed(&mut self) -> SignalBindingReceipt {
        self.invalidate(SignalBindingReason::ComposerChanged)
    }

    /// Accepts only a fresh exact observation for the currently claimed
    /// Signal window. Acceptance creates one short-lived, one-shot capability.
    pub fn attest(
        &mut self,
        evidence: SignalDestinationEvidence,
        now_ms: u64,
    ) -> SignalBindingReceipt {
        let Some(window) = self.claimed_window.as_ref() else {
            self.active = None;
            return self.receipt(
                SignalBindingStatus::Rejected,
                SignalBindingReason::NoClaimedWindow,
                0,
            );
        };

        let malformed = evidence.host_generation == 0
            || !nonzero(evidence.window_identity_sha256)
            || !nonzero(evidence.account_binding_sha256)
            || !nonzero(evidence.conversation_binding_sha256)
            || !nonzero(evidence.participant_set_sha256)
            || !nonzero(evidence.composer_identity_sha256)
            || !nonzero(evidence.attestation_nonce_sha256)
            || evidence.observed_at_ms > now_ms;
        if malformed {
            self.active = None;
            return self.receipt(
                SignalBindingStatus::Rejected,
                SignalBindingReason::MalformedEvidence,
                0,
            );
        }
        if evidence.host_generation != window.host_generation {
            self.active = None;
            return self.receipt(
                SignalBindingStatus::Rejected,
                SignalBindingReason::WindowGenerationChanged,
                0,
            );
        }
        if evidence.window_identity_sha256 != window.window_identity_sha256 {
            self.active = None;
            return self.receipt(
                SignalBindingStatus::Rejected,
                SignalBindingReason::WindowIdentityChanged,
                0,
            );
        }
        if !evidence.window_foreground {
            self.active = None;
            return self.receipt(
                SignalBindingStatus::Rejected,
                SignalBindingReason::WindowNotForeground,
                0,
            );
        }
        if !evidence.composer_focused {
            self.active = None;
            return self.receipt(
                SignalBindingStatus::Rejected,
                SignalBindingReason::ComposerNotFocused,
                0,
            );
        }
        if !evidence.conversation_stable {
            self.active = None;
            return self.receipt(
                SignalBindingStatus::Rejected,
                SignalBindingReason::ConversationUnstable,
                0,
            );
        }
        let age_ms = now_ms - evidence.observed_at_ms;
        if age_ms > self.ttl_ms {
            self.active = None;
            return self.receipt(
                SignalBindingStatus::Rejected,
                SignalBindingReason::StaleEvidence,
                0,
            );
        }
        if self
            .seen_nonces
            .contains(&evidence.attestation_nonce_sha256)
        {
            self.active = None;
            return self.receipt(
                SignalBindingStatus::Rejected,
                SignalBindingReason::ReplayedEvidence,
                0,
            );
        }
        if self.seen_nonces.len() >= MAX_REPLAY_ENTRIES {
            self.active = None;
            return self.receipt(
                SignalBindingStatus::Rejected,
                SignalBindingReason::ReplayJournalFull,
                0,
            );
        }

        self.seen_nonces.insert(evidence.attestation_nonce_sha256);
        self.next_attestation_generation = self
            .next_attestation_generation
            .checked_add(1)
            .unwrap_or(u64::MAX);
        if self.next_attestation_generation == u64::MAX {
            self.active = None;
            return self.receipt(
                SignalBindingStatus::Rejected,
                SignalBindingReason::AttestationSuperseded,
                0,
            );
        }
        let generation = self.next_attestation_generation;
        let expires_at_ms = evidence.observed_at_ms.saturating_add(self.ttl_ms);
        self.active = Some(FreshAttestation {
            lifecycle_generation: self.lifecycle_generation,
            attestation_generation: generation,
            expires_at_ms,
            evidence,
            consumed: false,
        });
        self.receipt(
            SignalBindingStatus::Accepted,
            SignalBindingReason::None,
            expires_at_ms.saturating_sub(now_ms),
        )
    }

    /// Consumes the exact fresh attestation for one protected send.
    /// Destination mismatch also consumes it, preventing retry-based probing.
    pub fn authorize_once(
        &mut self,
        attestation_generation: u64,
        intent: &SignalProtectedSendIntent,
        now_ms: u64,
    ) -> SignalBindingReceipt {
        let Some(active) = self.active.as_mut() else {
            return self.receipt(
                SignalBindingStatus::Rejected,
                SignalBindingReason::NoFreshAttestation,
                0,
            );
        };
        if active.attestation_generation != attestation_generation {
            return self.receipt(
                SignalBindingStatus::Rejected,
                SignalBindingReason::AttestationSuperseded,
                0,
            );
        }
        if active.consumed {
            return self.receipt(
                SignalBindingStatus::Rejected,
                SignalBindingReason::AlreadyConsumed,
                0,
            );
        }
        active.consumed = true;
        if active.lifecycle_generation != self.lifecycle_generation {
            return self.receipt(
                SignalBindingStatus::Rejected,
                SignalBindingReason::AttestationSuperseded,
                0,
            );
        }
        if now_ms > active.expires_at_ms {
            return self.receipt(
                SignalBindingStatus::Rejected,
                SignalBindingReason::AttestationExpired,
                0,
            );
        }
        let exact = intent.account_binding_sha256 == active.evidence.account_binding_sha256
            && intent.conversation_binding_sha256 == active.evidence.conversation_binding_sha256
            && intent.participant_set_sha256 == active.evidence.participant_set_sha256
            && intent.composer_identity_sha256 == active.evidence.composer_identity_sha256;
        if !exact {
            return self.receipt(
                SignalBindingStatus::Rejected,
                SignalBindingReason::DestinationMismatch,
                0,
            );
        }
        self.receipt(
            SignalBindingStatus::Authorized,
            SignalBindingReason::None,
            0,
        )
    }

    /// Non-consuming readiness view for trusted UI surfaces. This never
    /// implies that a later send will succeed; authorization still rechecks
    /// freshness and consumes the exact attestation atomically.
    pub fn readiness(&mut self, now_ms: u64) -> SignalBindingReceipt {
        let Some(active) = self.active.as_ref() else {
            let reason = if self.claimed_window.is_some() {
                SignalBindingReason::NoFreshAttestation
            } else {
                SignalBindingReason::NoClaimedWindow
            };
            return self.receipt(SignalBindingStatus::Rejected, reason, 0);
        };
        if active.lifecycle_generation != self.lifecycle_generation {
            self.active = None;
            return self.receipt(
                SignalBindingStatus::Rejected,
                SignalBindingReason::AttestationSuperseded,
                0,
            );
        }
        if active.consumed {
            return self.receipt(
                SignalBindingStatus::Rejected,
                SignalBindingReason::AlreadyConsumed,
                0,
            );
        }
        if now_ms > active.expires_at_ms {
            self.active = None;
            return self.receipt(
                SignalBindingStatus::Rejected,
                SignalBindingReason::AttestationExpired,
                0,
            );
        }
        self.receipt(
            SignalBindingStatus::Accepted,
            SignalBindingReason::None,
            active.expires_at_ms.saturating_sub(now_ms),
        )
    }

    fn invalidate(&mut self, reason: SignalBindingReason) -> SignalBindingReceipt {
        self.bump_lifecycle();
        self.active = None;
        self.receipt(SignalBindingStatus::Invalidated, reason, 0)
    }

    fn bump_lifecycle(&mut self) {
        self.lifecycle_generation = self.lifecycle_generation.saturating_add(1);
        if self.lifecycle_generation == 0 {
            self.lifecycle_generation = 1;
        }
    }

    fn receipt(
        &self,
        status: SignalBindingStatus,
        reason: SignalBindingReason,
        valid_for_ms: u64,
    ) -> SignalBindingReceipt {
        SignalBindingReceipt {
            status,
            reason,
            lifecycle_generation: self.lifecycle_generation,
            attestation_generation: self
                .active
                .as_ref()
                .map_or(0, |active| active.attestation_generation),
            valid_for_ms,
        }
    }
}

fn nonzero(value: Digest) -> bool {
    value.iter().any(|byte| *byte != 0)
}

/// Single process-relative clock shared by native evidence capture and
/// readiness queries. It reveals no wall-clock or provider information.
pub fn monotonic_now_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START
        .get_or_init(Instant::now)
        .elapsed()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 10_000;

    fn digest(value: u8) -> Digest {
        [value; 32]
    }

    fn evidence(nonce: u8) -> SignalDestinationEvidence {
        SignalDestinationEvidence {
            host_generation: 7,
            window_identity_sha256: digest(1),
            account_binding_sha256: digest(2),
            conversation_binding_sha256: digest(3),
            participant_set_sha256: digest(4),
            composer_identity_sha256: digest(5),
            attestation_nonce_sha256: digest(nonce),
            observed_at_ms: NOW,
            window_foreground: true,
            composer_focused: true,
            conversation_stable: true,
        }
    }

    fn intent() -> SignalProtectedSendIntent {
        SignalProtectedSendIntent {
            account_binding_sha256: digest(2),
            conversation_binding_sha256: digest(3),
            participant_set_sha256: digest(4),
            composer_identity_sha256: digest(5),
        }
    }

    fn claimed() -> SignalDestinationBindingGuard {
        let mut guard = SignalDestinationBindingGuard::default();
        let receipt = guard.claim_window(7, digest(1));
        assert_eq!(receipt.status, SignalBindingStatus::Invalidated);
        guard
    }

    #[test]
    fn protected_send_requires_fresh_exact_one_shot_attestation() {
        let mut guard = claimed();
        assert_eq!(
            guard.authorize_once(1, &intent(), NOW).reason,
            SignalBindingReason::NoFreshAttestation
        );
        let accepted = guard.attest(evidence(9), NOW);
        assert_eq!(accepted.status, SignalBindingStatus::Accepted);
        assert_eq!(accepted.valid_for_ms, DEFAULT_ATTESTATION_TTL_MS);
        assert_eq!(
            guard
                .authorize_once(accepted.attestation_generation, &intent(), NOW + 1)
                .status,
            SignalBindingStatus::Authorized
        );
        assert_eq!(
            guard
                .authorize_once(accepted.attestation_generation, &intent(), NOW + 1)
                .reason,
            SignalBindingReason::AlreadyConsumed
        );
    }

    #[test]
    fn reconnect_invalidates_even_when_window_identity_is_unchanged() {
        let mut guard = claimed();
        let accepted = guard.attest(evidence(9), NOW);
        let first_lifecycle = accepted.lifecycle_generation;
        guard.claim_window(7, digest(1));
        let denied = guard.authorize_once(accepted.attestation_generation, &intent(), NOW + 1);
        assert_eq!(denied.reason, SignalBindingReason::NoFreshAttestation);
        assert!(denied.lifecycle_generation > first_lifecycle);
    }

    #[test]
    fn native_revalidation_invalidates_changed_claim_before_readiness() {
        let mut guard = claimed();
        let accepted = guard.attest(evidence(9), NOW);
        let receipt = guard.verify_claimed_window(7, digest(8));
        assert_eq!(receipt.status, SignalBindingStatus::Invalidated);
        assert_eq!(receipt.reason, SignalBindingReason::WindowIdentityChanged);
        assert_eq!(
            guard
                .authorize_once(accepted.attestation_generation, &intent(), NOW + 1)
                .reason,
            SignalBindingReason::NoFreshAttestation
        );
    }

    #[test]
    fn accepted_nonce_cannot_be_replayed_after_reconnect() {
        let mut guard = claimed();
        assert_eq!(
            guard.attest(evidence(9), NOW).status,
            SignalBindingStatus::Accepted
        );
        guard.claim_window(7, digest(1));
        assert_eq!(
            guard.attest(evidence(9), NOW + 1).reason,
            SignalBindingReason::ReplayedEvidence
        );
    }

    #[test]
    fn malformed_and_stale_evidence_fail_closed() {
        let mut guard = claimed();
        let mut malformed = evidence(9);
        malformed.conversation_binding_sha256 = [0; 32];
        assert_eq!(
            guard.attest(malformed, NOW).reason,
            SignalBindingReason::MalformedEvidence
        );
        let mut future = evidence(10);
        future.observed_at_ms = NOW + 1;
        assert_eq!(
            guard.attest(future, NOW).reason,
            SignalBindingReason::MalformedEvidence
        );
        let mut stale = evidence(11);
        stale.observed_at_ms = NOW - DEFAULT_ATTESTATION_TTL_MS - 1;
        assert_eq!(
            guard.attest(stale, NOW).reason,
            SignalBindingReason::StaleEvidence
        );
    }

    #[test]
    fn attestation_expires_before_authorization() {
        let mut guard = claimed();
        let accepted = guard.attest(evidence(9), NOW);
        assert_eq!(
            guard
                .authorize_once(
                    accepted.attestation_generation,
                    &intent(),
                    NOW + DEFAULT_ATTESTATION_TTL_MS + 1,
                )
                .reason,
            SignalBindingReason::AttestationExpired
        );
    }

    #[test]
    fn destination_mismatch_consumes_attestation() {
        let mut guard = claimed();
        let accepted = guard.attest(evidence(9), NOW);
        let mut wrong = intent();
        wrong.participant_set_sha256 = digest(8);
        assert_eq!(
            guard
                .authorize_once(accepted.attestation_generation, &wrong, NOW + 1)
                .reason,
            SignalBindingReason::DestinationMismatch
        );
        assert_eq!(
            guard
                .authorize_once(accepted.attestation_generation, &intent(), NOW + 1)
                .reason,
            SignalBindingReason::AlreadyConsumed
        );
    }

    #[test]
    fn focus_conversation_composer_and_window_lifecycle_each_invalidate() {
        let cases: [fn(&mut SignalDestinationBindingGuard) -> SignalBindingReceipt; 4] = [
            SignalDestinationBindingGuard::focus_lost,
            SignalDestinationBindingGuard::conversation_changed,
            SignalDestinationBindingGuard::composer_changed,
            SignalDestinationBindingGuard::window_unavailable,
        ];
        for invalidate in cases {
            let mut guard = claimed();
            let accepted = guard.attest(evidence(9), NOW);
            invalidate(&mut guard);
            assert_eq!(
                guard
                    .authorize_once(accepted.attestation_generation, &intent(), NOW + 1)
                    .reason,
                SignalBindingReason::NoFreshAttestation
            );
        }
    }

    #[test]
    fn changed_window_or_unstable_focus_evidence_is_rejected() {
        let mut guard = claimed();
        let mut changed = evidence(9);
        changed.window_identity_sha256 = digest(8);
        assert_eq!(
            guard.attest(changed, NOW).reason,
            SignalBindingReason::WindowIdentityChanged
        );

        let mut background = evidence(10);
        background.window_foreground = false;
        assert_eq!(
            guard.attest(background, NOW).reason,
            SignalBindingReason::WindowNotForeground
        );
        let mut unfocused = evidence(11);
        unfocused.composer_focused = false;
        assert_eq!(
            guard.attest(unfocused, NOW).reason,
            SignalBindingReason::ComposerNotFocused
        );
        let mut unstable = evidence(12);
        unstable.conversation_stable = false;
        assert_eq!(
            guard.attest(unstable, NOW).reason,
            SignalBindingReason::ConversationUnstable
        );
    }

    #[test]
    fn receipts_serialize_without_provider_or_message_fields() {
        let mut guard = claimed();
        let receipt = guard.attest(evidence(9), NOW);
        let json = serde_json::to_string(&receipt).unwrap();
        assert_eq!(
            json,
            "{\"status\":\"accepted\",\"reason\":\"none\",\"lifecycleGeneration\":1,\"attestationGeneration\":1,\"validForMs\":2000}"
        );
        for forbidden in [
            "account",
            "conversation",
            "participant",
            "composerIdentity",
            "windowIdentity",
            "message",
        ] {
            assert!(!json.contains(forbidden));
        }
    }

    #[test]
    fn readiness_is_non_consuming_but_expires_fail_closed() {
        let mut guard = claimed();
        let accepted = guard.attest(evidence(9), NOW);
        assert_eq!(
            guard.readiness(NOW + 1).status,
            SignalBindingStatus::Accepted
        );
        assert_eq!(
            guard.readiness(NOW + 2).status,
            SignalBindingStatus::Accepted
        );
        assert_eq!(
            guard
                .authorize_once(accepted.attestation_generation, &intent(), NOW + 3)
                .status,
            SignalBindingStatus::Authorized
        );

        let mut expiring = claimed();
        expiring.attest(evidence(10), NOW);
        assert_eq!(
            expiring
                .readiness(NOW + DEFAULT_ATTESTATION_TTL_MS + 1)
                .reason,
            SignalBindingReason::AttestationExpired
        );
        assert_eq!(
            expiring
                .readiness(NOW + DEFAULT_ATTESTATION_TTL_MS + 2)
                .reason,
            SignalBindingReason::NoFreshAttestation
        );
    }
}
