//! Telegram Desktop adapter through the locate/read-state ABI half.
//!
//! Placement, destination attestation, commit, and paint targets are left
//! closed until their respective Telegram tasks supply their proofs.

use super::*;

pub trait TelegramBackend: Send + Sync {
    fn capabilities(&self, now_unix_seconds: u64) -> CapabilitySet;
    fn locate(&self, target: &SurfaceTarget) -> Result<SurfaceBinding, AdapterRefusal>;
    fn read_state(&self, binding: &SurfaceBinding) -> Result<SurfaceState, AdapterRefusal>;
    /// Place only. Sending is deliberately not part of the L2 backend surface.
    fn place(&self, binding: &SurfaceBinding, carrier: &Carrier) -> PlacementReceipt;
}

pub struct TelegramSurfaceAdapter<B> {
    backend: B,
}

impl<B> TelegramSurfaceAdapter<B> {
    pub fn new(backend: B) -> Self {
        Self { backend }
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

    fn destination(&self, _: &SurfaceBinding) -> Result<DestinationIdentity, AdapterRefusal> {
        Err(AdapterRefusal::DestinationUnattested)
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
        _: &SurfaceBinding,
        _: &SendAuthorization,
        _: &PlacementReceipt,
    ) -> SendReceipt {
        SendReceipt {
            outcome: SendOutcome::NotSent,
            elapsed_ms: 0,
        }
    }

    fn paint_targets(&self, _: &SurfaceBinding) -> Result<Vec<PaintTarget>, AdapterRefusal> {
        Err(AdapterRefusal::AccessibilityUnavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Backend {
        complete: bool,
        placements: AtomicUsize,
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
        fn place(&self, _: &SurfaceBinding, _: &Carrier) -> PlacementReceipt {
            self.placements.fetch_add(1, Ordering::SeqCst);
            PlacementReceipt {
                status: PlacementStatus::Placed,
                placed_sha256: Some("carrier-digest".into()),
                elapsed_ms: 1,
            }
        }
    }

    #[test]
    fn t3_t21_locate_and_read_preserve_an_incomplete_empty_transcript() {
        let adapter = TelegramSurfaceAdapter::new(Backend {
            complete: false,
            placements: AtomicUsize::new(0),
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
}
