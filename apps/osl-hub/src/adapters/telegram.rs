//! Telegram Desktop adapter through the native accessibility ABI.
//!
//! Destination attestation is deliberately bounded
//! to live account, conversation, and participant-set evidence: a window
//! title alone cannot be adapted into a destination identity.

use super::*;
use crate::signal_destination_binding::{
    monotonic_now_ms, SignalBindingStatus, SignalDestinationBindingGuard, SignalDestinationEvidence,
};
use std::sync::Mutex;

/// Opaque, live identity evidence captured from Telegram's claimed surface.
///
/// All provider-derived values are already SHA-256 digests. The backend must
/// provide the account, conversation, and exact recipient-set bindings; the
/// adapter intentionally has no title-only input.
#[derive(Clone, Eq, PartialEq)]
pub struct TelegramDestinationEvidence {
    pub window_identity_sha256: [u8; 32],
    pub account_binding_sha256: [u8; 32],
    pub conversation_binding_sha256: [u8; 32],
    pub participant_set_sha256: [u8; 32],
    pub composer_identity_sha256: [u8; 32],
    pub attestation_nonce_sha256: [u8; 32],
    pub observed_at_ms: u64,
    pub window_foreground: bool,
    pub composer_focused: bool,
    pub conversation_stable: bool,
}

pub trait TelegramBackend: Send + Sync {
    fn capabilities(&self, now_unix_seconds: u64) -> CapabilitySet;
    fn locate(&self, target: &SurfaceTarget) -> Result<SurfaceBinding, AdapterRefusal>;
    fn read_state(&self, binding: &SurfaceBinding) -> Result<SurfaceState, AdapterRefusal>;
    fn destination_evidence(
        &self,
        binding: &SurfaceBinding,
    ) -> Result<TelegramDestinationEvidence, AdapterRefusal>;
    /// Placement remains separate from the explicit L3 send commit.
    fn place(&self, binding: &SurfaceBinding, carrier: &Carrier) -> PlacementReceipt;
    fn commit(&self, binding: &SurfaceBinding, placed: &PlacementReceipt) -> SendReceipt;
    fn paint_targets(&self, binding: &SurfaceBinding) -> Result<Vec<PaintTarget>, AdapterRefusal>;
}

pub struct TelegramSurfaceAdapter<B> {
    backend: B,
    destination_guard: Mutex<SignalDestinationBindingGuard>,
}

impl<B> TelegramSurfaceAdapter<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            destination_guard: Mutex::new(SignalDestinationBindingGuard::default()),
        }
    }
}

impl<B: TelegramBackend> TelegramSurfaceAdapter<B> {
    fn supports(&self, capability: adapter_profile::Capability) -> bool {
        self.backend.capabilities(u64::MAX).contains(&capability)
    }

    fn validates_binding(&self, binding: &SurfaceBinding) -> bool {
        binding.app == AdapterAppId::Telegram && binding.generation != 0
    }
}

impl<B: TelegramBackend> SurfaceAdapter for TelegramSurfaceAdapter<B> {
    fn abi_version(&self) -> u32 {
        ADAPTER_ABI_VERSION
    }
    fn app(&self) -> AdapterAppId {
        AdapterAppId::Telegram
    }
    fn surface(&self) -> SurfaceKind {
        SurfaceKind::InstalledNativeClient
    }
    fn capabilities(&self, now_unix_seconds: u64) -> CapabilitySet {
        self.backend.capabilities(now_unix_seconds)
    }

    fn locate(&self, target: &SurfaceTarget) -> Result<SurfaceBinding, AdapterRefusal> {
        if target.app != self.app() || target.surface != self.surface() || target.generation == 0 {
            return Err(AdapterRefusal::WindowGone);
        }
        let binding = self.backend.locate(target)?;
        if !self.validates_binding(&binding) || binding.generation != target.generation {
            return Err(AdapterRefusal::GenerationStale);
        }
        Ok(binding)
    }

    fn read_state(&self, binding: &SurfaceBinding) -> Result<SurfaceState, AdapterRefusal> {
        if !self.validates_binding(binding) {
            return Err(AdapterRefusal::GenerationStale);
        }
        self.backend.read_state(binding)
    }

    fn destination(&self, binding: &SurfaceBinding) -> Result<DestinationIdentity, AdapterRefusal> {
        if !self.validates_binding(binding) {
            return Err(AdapterRefusal::GenerationStale);
        }
        if !matches!(
            &binding.evidence,
            BindingEvidence::Accessibility { .. } | BindingEvidence::Win32Structural
        ) {
            return Err(AdapterRefusal::DestinationUnattested);
        }
        let evidence = self.backend.destination_evidence(binding)?;
        let mut guard = self
            .destination_guard
            .lock()
            .map_err(|_| AdapterRefusal::DestinationUnattested)?;
        guard.claim_window(binding.generation, evidence.window_identity_sha256);
        let receipt = guard.attest(
            SignalDestinationEvidence {
                host_generation: binding.generation,
                window_identity_sha256: evidence.window_identity_sha256,
                account_binding_sha256: evidence.account_binding_sha256,
                conversation_binding_sha256: evidence.conversation_binding_sha256,
                participant_set_sha256: evidence.participant_set_sha256,
                composer_identity_sha256: evidence.composer_identity_sha256,
                attestation_nonce_sha256: evidence.attestation_nonce_sha256,
                observed_at_ms: evidence.observed_at_ms,
                window_foreground: evidence.window_foreground,
                composer_focused: evidence.composer_focused,
                conversation_stable: evidence.conversation_stable,
            },
            monotonic_now_ms(),
        );
        if receipt.status != SignalBindingStatus::Accepted {
            return Err(AdapterRefusal::DestinationUnattested);
        }
        Ok(DestinationIdentity {
            status: DestinationStatus::Attested,
            account_digest: hex_digest(evidence.account_binding_sha256),
            conversation_digest: hex_digest(evidence.conversation_binding_sha256),
            recipients_digest: hex_digest(evidence.participant_set_sha256),
            scope_binding_hash: binding.scope_binding_hash.clone(),
            evidence: binding.evidence.clone(),
            attested_at_ms: evidence.observed_at_ms,
            ttl_ms: receipt.valid_for_ms,
        })
    }

    fn place(
        &self,
        binding: &SurfaceBinding,
        authorization: &PlacementAuthorization,
        carrier: &Carrier,
    ) -> PlacementReceipt {
        let refused = || PlacementReceipt {
            status: PlacementStatus::NotPlaced,
            placed_sha256: None,
            elapsed_ms: 0,
        };
        if !self.validates_binding(binding)
            || !same_scope(
                &binding.scope_binding_hash,
                &authorization.scope_binding_hash,
            )
            || !self.supports(adapter_profile::Capability::PlaceProtectedPayload)
        {
            return refused();
        }
        match self.read_state(binding) {
            Ok(state)
                if state.composer_is_empty
                    && !state.composer_is_password_field
                    && state.focused
                    && !state.occluded =>
            {
                self.backend.place(binding, carrier)
            }
            _ => refused(),
        }
    }

    fn commit(
        &self,
        binding: &SurfaceBinding,
        authorization: &SendAuthorization,
        placed: &PlacementReceipt,
    ) -> SendReceipt {
        let refused = || SendReceipt {
            outcome: SendOutcome::NotSent,
            elapsed_ms: 0,
        };
        if !self.validates_binding(binding)
            || !is_send_evidence_admissible(&binding.evidence)
            || !same_scope(
                &binding.scope_binding_hash,
                &authorization.scope_binding_hash,
            )
            || !self.supports(adapter_profile::Capability::SendProtectedPayload)
            || placed.status != PlacementStatus::Placed
            || placed.placed_sha256.is_none()
        {
            return refused();
        }
        let destination = match self.destination(binding) {
            Ok(destination) => destination,
            Err(_) => return refused(),
        };
        if destination.status != DestinationStatus::Attested
            || !same_scope(&binding.scope_binding_hash, &destination.scope_binding_hash)
            || !is_send_evidence_admissible(&destination.evidence)
        {
            return refused();
        }
        self.backend.commit(binding, placed)
    }

    fn paint_targets(&self, binding: &SurfaceBinding) -> Result<Vec<PaintTarget>, AdapterRefusal> {
        if !self.validates_binding(binding) {
            return Err(AdapterRefusal::GenerationStale);
        }
        let targets = self.backend.paint_targets(binding)?;
        if matches!(binding.evidence, BindingEvidence::Pixel)
            && targets
                .iter()
                .any(|target| target.confidence == PaintConfidence::Exact)
        {
            return Err(AdapterRefusal::AccessibilityUnavailable);
        }
        Ok(targets)
    }
}

fn hex_digest(digest: [u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Backend {
        complete: bool,
        placements: AtomicUsize,
        commits: AtomicUsize,
        destination_evidence: TelegramDestinationEvidence,
        paint_targets: Vec<PaintTarget>,
    }

    fn binding(generation: u64) -> SurfaceBinding {
        SurfaceBinding {
            app: AdapterAppId::Telegram,
            generation,
            evidence: BindingEvidence::Accessibility {
                tree: A11yTree::Uia,
            },
            composer: NodeRef(4),
            transcript: Some(NodeRef(8)),
            bounds: Bounds {
                x: 0,
                y: 0,
                width: 1200,
                height: 900,
            },
            bound_at_ms: 1,
            scope_binding_hash: "scope-a".into(),
        }
    }

    impl TelegramBackend for Backend {
        fn capabilities(&self, _: u64) -> CapabilitySet {
            [
                adapter_profile::Capability::InspectVisibleComposer,
                adapter_profile::Capability::InspectVisibleTranscript,
                adapter_profile::Capability::PlaceProtectedPayload,
                adapter_profile::Capability::SendProtectedPayload,
            ]
            .into_iter()
            .collect()
        }
        fn locate(&self, target: &SurfaceTarget) -> Result<SurfaceBinding, AdapterRefusal> {
            Ok(binding(target.generation))
        }
        fn read_state(&self, _: &SurfaceBinding) -> Result<SurfaceState, AdapterRefusal> {
            Ok(SurfaceState {
                composer_text_sha256: "digest".into(),
                composer_is_empty: true,
                composer_is_password_field: false,
                focused: true,
                occluded: false,
                read_was_complete: self.complete,
            })
        }
        fn destination_evidence(
            &self,
            _: &SurfaceBinding,
        ) -> Result<TelegramDestinationEvidence, AdapterRefusal> {
            Ok(self.destination_evidence.clone())
        }
        fn place(&self, _: &SurfaceBinding, _: &Carrier) -> PlacementReceipt {
            self.placements.fetch_add(1, Ordering::SeqCst);
            PlacementReceipt {
                status: PlacementStatus::Placed,
                placed_sha256: Some("carrier-digest".into()),
                elapsed_ms: 1,
            }
        }
        fn commit(&self, _: &SurfaceBinding, _: &PlacementReceipt) -> SendReceipt {
            self.commits.fetch_add(1, Ordering::SeqCst);
            SendReceipt {
                outcome: SendOutcome::Sent,
                elapsed_ms: 1,
            }
        }
        fn paint_targets(&self, _: &SurfaceBinding) -> Result<Vec<PaintTarget>, AdapterRefusal> {
            Ok(self.paint_targets.clone())
        }
    }

    fn destination_evidence() -> TelegramDestinationEvidence {
        TelegramDestinationEvidence {
            window_identity_sha256: [1; 32],
            account_binding_sha256: [2; 32],
            conversation_binding_sha256: [3; 32],
            participant_set_sha256: [4; 32],
            composer_identity_sha256: [5; 32],
            attestation_nonce_sha256: [6; 32],
            observed_at_ms: monotonic_now_ms(),
            window_foreground: true,
            composer_focused: true,
            conversation_stable: true,
        }
    }

    #[test]
    fn t3_t21_locate_and_read_preserve_an_incomplete_empty_transcript() {
        let adapter = TelegramSurfaceAdapter::new(Backend {
            complete: false,
            placements: AtomicUsize::new(0),
            commits: AtomicUsize::new(0),
            destination_evidence: destination_evidence(),
            paint_targets: vec![],
        });
        let target = SurfaceTarget {
            app: AdapterAppId::Telegram,
            surface: SurfaceKind::InstalledNativeClient,
            generation: 7,
        };
        let located = adapter.locate(&target).unwrap();
        assert_eq!(located.generation, 7);
        assert!(!adapter.read_state(&located).unwrap().read_was_complete);
        assert_eq!(
            adapter.read_state(&binding(0)),
            Err(AdapterRefusal::GenerationStale)
        );
    }

    #[test]
    fn t3_t22_places_only_with_a_focused_empty_composer_and_never_exposes_send() {
        let adapter = TelegramSurfaceAdapter::new(Backend {
            complete: true,
            placements: AtomicUsize::new(0),
            commits: AtomicUsize::new(0),
            destination_evidence: destination_evidence(),
            paint_targets: vec![],
        });
        let binding = binding(1);
        let placed = adapter.place(
            &binding,
            &PlacementAuthorization::for_scope("scope-a"),
            &Carrier("carrier".into()),
        );

        assert_eq!(placed.status, PlacementStatus::Placed);
        assert_eq!(placed.placed_sha256.as_deref(), Some("carrier-digest"));
        assert_eq!(adapter.backend.placements.load(Ordering::SeqCst), 1);

        let refused = adapter.place(
            &binding,
            &PlacementAuthorization::for_scope("other-scope"),
            &Carrier("carrier".into()),
        );
        assert_eq!(refused.status, PlacementStatus::NotPlaced);
        assert_eq!(adapter.backend.placements.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn t3_t23_attests_only_a_complete_live_destination_identity() {
        let adapter = TelegramSurfaceAdapter::new(Backend {
            complete: true,
            placements: AtomicUsize::new(0),
            commits: AtomicUsize::new(0),
            destination_evidence: destination_evidence(),
            paint_targets: vec![],
        });
        let identity = adapter.destination(&binding(1)).unwrap();
        assert_eq!(identity.status, DestinationStatus::Attested);
        assert_eq!(identity.account_digest, "02".repeat(32));
        assert_eq!(identity.conversation_digest, "03".repeat(32));
        assert_eq!(identity.recipients_digest, "04".repeat(32));
        assert!(identity.ttl_ms > 0);

        let adapter = TelegramSurfaceAdapter::new(Backend {
            complete: true,
            placements: AtomicUsize::new(0),
            commits: AtomicUsize::new(0),
            destination_evidence: TelegramDestinationEvidence {
                participant_set_sha256: [0; 32],
                ..destination_evidence()
            },
            paint_targets: vec![],
        });
        assert_eq!(
            adapter.destination(&binding(1)),
            Err(AdapterRefusal::DestinationUnattested)
        );
    }

    #[test]
    fn t3_t24_commits_only_an_attested_accessibility_binding_and_rejects_exact_pixel_paint() {
        let target = PaintTarget {
            carrier_sha256: "carrier-digest".into(),
            rect: Bounds {
                x: 10,
                y: 20,
                width: 300,
                height: 40,
            },
            clipped_by: None,
            confidence: PaintConfidence::Exact,
        };
        let adapter = TelegramSurfaceAdapter::new(Backend {
            complete: true,
            placements: AtomicUsize::new(0),
            commits: AtomicUsize::new(0),
            destination_evidence: destination_evidence(),
            paint_targets: vec![target.clone()],
        });
        let accessibility = binding(1);
        let placed = adapter.place(
            &accessibility,
            &PlacementAuthorization::for_scope("scope-a"),
            &Carrier("carrier".into()),
        );

        assert_eq!(
            adapter
                .commit(
                    &accessibility,
                    &SendAuthorization::for_scope("scope-a"),
                    &placed,
                )
                .outcome,
            SendOutcome::Sent
        );
        assert_eq!(adapter.backend.commits.load(Ordering::SeqCst), 1);
        assert_eq!(adapter.paint_targets(&accessibility), Ok(vec![target]));

        let pixel = SurfaceBinding {
            evidence: BindingEvidence::Pixel,
            ..accessibility
        };
        assert_eq!(
            adapter.paint_targets(&pixel),
            Err(AdapterRefusal::AccessibilityUnavailable)
        );
        assert_eq!(
            adapter
                .commit(&pixel, &SendAuthorization::for_scope("scope-a"), &placed,)
                .outcome,
            SendOutcome::NotSent
        );
        assert_eq!(adapter.backend.commits.load(Ordering::SeqCst), 1);
    }
}
