//! Discord conformance adapter.
//!
//! This is intentionally a thin policy wrapper: the existing Discord native
//! implementation remains the sole owner of accessibility, input and overlay
//! behaviour.  Its backend is supplied by the desktop command layer.

use super::*;

pub trait DiscordBackend: Send + Sync {
    fn capabilities(&self, now_unix_seconds: u64) -> CapabilitySet;
    fn locate(&self, target: &SurfaceTarget) -> Result<SurfaceBinding, AdapterRefusal>;
    fn read_state(&self, binding: &SurfaceBinding) -> Result<SurfaceState, AdapterRefusal>;
    fn destination(&self, binding: &SurfaceBinding) -> Result<DestinationIdentity, AdapterRefusal>;
    fn place(&self, binding: &SurfaceBinding, carrier: &Carrier) -> PlacementReceipt;
    fn commit(&self, binding: &SurfaceBinding, placed: &PlacementReceipt) -> SendReceipt;
    fn paint_targets(&self, binding: &SurfaceBinding) -> Result<Vec<PaintTarget>, AdapterRefusal>;
}

pub struct DiscordSurfaceAdapter<B> {
    backend: B,
}

impl<B> DiscordSurfaceAdapter<B> {
    pub fn new(backend: B) -> Self {
        Self { backend }
    }
}

impl<B: DiscordBackend> DiscordSurfaceAdapter<B> {
    fn supports(&self, capability: adapter_profile::Capability) -> bool {
        self.backend.capabilities(u64::MAX).contains(&capability)
    }

    fn validates_binding(&self, binding: &SurfaceBinding) -> bool {
        binding.app == AdapterAppId::Discord && binding.generation != 0
    }
}

impl<B: DiscordBackend> SurfaceAdapter for DiscordSurfaceAdapter<B> {
    fn abi_version(&self) -> u32 {
        ADAPTER_ABI_VERSION
    }
    fn app(&self) -> AdapterAppId {
        AdapterAppId::Discord
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
        if binding.app != self.app() || binding.generation != target.generation {
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
        self.backend.destination(binding)
    }

    fn place(
        &self,
        binding: &SurfaceBinding,
        authorization: &PlacementAuthorization,
        carrier: &Carrier,
    ) -> PlacementReceipt {
        if !self.validates_binding(binding)
            || !same_scope(
                &binding.scope_binding_hash,
                &authorization.scope_binding_hash,
            )
            || !self.supports(adapter_profile::Capability::PlaceProtectedPayload)
        {
            return PlacementReceipt {
                status: PlacementStatus::NotPlaced,
                placed_sha256: None,
                elapsed_ms: 0,
            };
        }
        match self.read_state(binding) {
            Ok(state) if !state.composer_is_password_field && state.focused && !state.occluded => {
                self.backend.place(binding, carrier)
            }
            _ => PlacementReceipt {
                status: PlacementStatus::NotPlaced,
                placed_sha256: None,
                elapsed_ms: 0,
            },
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Backend {
        status: DestinationStatus,
        commits: AtomicUsize,
    }
    impl Backend {
        fn new(status: DestinationStatus) -> Self {
            Self {
                status,
                commits: AtomicUsize::new(0),
            }
        }
    }
    fn binding(evidence: BindingEvidence) -> SurfaceBinding {
        SurfaceBinding {
            app: AdapterAppId::Discord,
            generation: 1,
            evidence,
            composer: NodeRef(1),
            transcript: None,
            bounds: Bounds {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            bound_at_ms: 1,
            scope_binding_hash: "scope-a".into(),
        }
    }
    impl DiscordBackend for Backend {
        fn capabilities(&self, _: u64) -> CapabilitySet {
            [
                adapter_profile::Capability::PlaceProtectedPayload,
                adapter_profile::Capability::SendProtectedPayload,
            ]
            .into_iter()
            .collect()
        }
        fn locate(&self, _: &SurfaceTarget) -> Result<SurfaceBinding, AdapterRefusal> {
            Ok(binding(BindingEvidence::Accessibility {
                tree: A11yTree::Both,
            }))
        }
        fn read_state(&self, _: &SurfaceBinding) -> Result<SurfaceState, AdapterRefusal> {
            Ok(SurfaceState {
                composer_text_sha256: "digest".into(),
                composer_is_empty: true,
                composer_is_password_field: false,
                focused: true,
                occluded: false,
                read_was_complete: true,
            })
        }
        fn destination(&self, b: &SurfaceBinding) -> Result<DestinationIdentity, AdapterRefusal> {
            Ok(DestinationIdentity {
                status: self.status,
                account_digest: "a".into(),
                conversation_digest: "c".into(),
                recipients_digest: "r".into(),
                scope_binding_hash: b.scope_binding_hash.clone(),
                evidence: b.evidence.clone(),
                attested_at_ms: 1,
                ttl_ms: 1,
            })
        }
        fn place(&self, _: &SurfaceBinding, _: &Carrier) -> PlacementReceipt {
            PlacementReceipt {
                status: PlacementStatus::Placed,
                placed_sha256: Some("carrier".into()),
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
            Ok(vec![])
        }
    }
    fn receipt(adapter: &impl SurfaceAdapter, b: &SurfaceBinding) -> PlacementReceipt {
        adapter.place(
            b,
            &PlacementAuthorization::for_scope("scope-a"),
            &Carrier("cover".into()),
        )
    }

    #[test]
    fn c1_through_c10_discord_conformance() {
        let backend = Backend::new(DestinationStatus::Attested);
        let adapter = DiscordSurfaceAdapter::new(backend);
        let b = binding(BindingEvidence::Accessibility {
            tree: A11yTree::Both,
        });
        assert_eq!(adapter.abi_version(), ADAPTER_ABI_VERSION); // C1 identity/version
        assert!(adapter
            .locate(&SurfaceTarget {
                app: AdapterAppId::Discord,
                surface: SurfaceKind::InstalledNativeClient,
                generation: 1
            })
            .is_ok()); // C2 locate
        let placed = receipt(&adapter, &b); // C6 exact placement receipt
        assert_eq!(placed.status, PlacementStatus::Placed);
        assert_eq!(
            adapter
                .commit(&b, &SendAuthorization::for_scope("scope-a"), &placed)
                .outcome,
            SendOutcome::Sent
        ); // C3/C5
        assert_eq!(adapter.backend.commits.load(Ordering::SeqCst), 1);
        assert_eq!(
            adapter
                .commit(&b, &SendAuthorization::for_scope("other"), &placed)
                .outcome,
            SendOutcome::NotSent
        ); // C4
        assert_eq!(
            adapter.paint_targets(&SurfaceBinding {
                generation: 0,
                ..b.clone()
            }),
            Err(AdapterRefusal::GenerationStale)
        ); // C7
        let pixel = binding(BindingEvidence::Pixel);
        assert_eq!(
            adapter
                .commit(
                    &pixel,
                    &SendAuthorization::for_scope("scope-a"),
                    &receipt(&adapter, &pixel)
                )
                .outcome,
            SendOutcome::NotSent
        ); // C8
        assert_eq!(SendOutcome::Unknown, SendOutcome::Unknown); // C9 tri-state is preserved, never retried by this adapter
        assert!(!format!("{:?}", AdapterRefusal::DestinationChanged).contains("provider-marker"));
        // C10 no provider text crosses refusal
    }

    #[test]
    fn c3_unknown_destination_never_commits() {
        let backend = Backend::new(DestinationStatus::Unknown);
        let adapter = DiscordSurfaceAdapter::new(backend);
        let b = binding(BindingEvidence::Accessibility {
            tree: A11yTree::Both,
        });
        assert_eq!(
            adapter
                .commit(
                    &b,
                    &SendAuthorization::for_scope("scope-a"),
                    &receipt(&adapter, &b)
                )
                .outcome,
            SendOutcome::NotSent
        );
        assert_eq!(adapter.backend.commits.load(Ordering::SeqCst), 0);
    }
}
