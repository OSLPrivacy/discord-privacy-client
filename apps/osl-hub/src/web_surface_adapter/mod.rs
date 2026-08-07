//! Profile-driven adapter shell for origin-pinned official web surfaces.
//!
//! Service-specific selector logic deliberately lives behind `WebSurfaceBackend`.
//! This type owns the ABI guardrails shared by every fixed-origin web profile,
//! especially refusing a generation that the host no longer claims.

use crate::adapters::*;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebPageControlRefusal {
    MissingBody,
    MissingSend,
}

impl fmt::Display for WebPageControlRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            WebPageControlRefusal::MissingBody => "missing Body",
            WebPageControlRefusal::MissingSend => "missing Send",
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WebPageControls {
    pub body_present: bool,
    pub send_present: bool,
}

impl WebPageControls {
    pub const fn complete() -> Self {
        Self {
            body_present: true,
            send_present: true,
        }
    }

    pub fn validate(self) -> Result<(), WebPageControlRefusal> {
        if !self.body_present {
            return Err(WebPageControlRefusal::MissingBody);
        }
        if !self.send_present {
            return Err(WebPageControlRefusal::MissingSend);
        }
        Ok(())
    }
}

/// GMX Mail target map. Kept as service-local data until a live GMX backend
/// proves the fixed-origin controls against a signed-in account.
pub mod gmx;
/// Outlook on the web target map.  Kept separate from the installed Outlook
/// Win32 driver so web selectors cannot be mistaken for desktop controls.
pub mod outlook;
/// X's reviewed DM implementation.  Keeping this behind the common adapter
/// means it receives the same generation, scope, and authorization checks as
/// every other fixed-origin web surface.
pub mod x;

/// Accessibility and input implementation for one verified web profile.
///
/// The backend is intentionally supplied by later service tasks. It may use the
/// profile selectors, but it cannot bypass this adapter's binding-generation or
/// authorization checks.
pub trait WebSurfaceBackend: Send + Sync {
    fn capabilities(
        &self,
        profile: &adapter_profile::ProfilePayload,
        now_unix_seconds: u64,
    ) -> CapabilitySet;
    fn is_current_generation(&self, generation: u64) -> bool;
    /// Wake Chromium/WebView accessibility before a selector walk. A backend
    /// that cannot populate a complete tree must fail closed, never fall back.
    fn wake_accessibility(&self) -> Result<(), AdapterRefusal>;
    fn locate(
        &self,
        profile: &adapter_profile::ProfilePayload,
        target: &SurfaceTarget,
    ) -> Result<SurfaceBinding, AdapterRefusal>;
    fn read_state(&self, binding: &SurfaceBinding) -> Result<SurfaceState, AdapterRefusal>;
    fn page_controls(
        &self,
        _profile: &adapter_profile::ProfilePayload,
        _binding: &SurfaceBinding,
    ) -> WebPageControls {
        WebPageControls::complete()
    }
    fn destination(&self, binding: &SurfaceBinding) -> Result<DestinationIdentity, AdapterRefusal>;
    fn place(&self, binding: &SurfaceBinding, carrier: &Carrier) -> PlacementReceipt;
    fn commit(&self, binding: &SurfaceBinding, placed: &PlacementReceipt) -> SendReceipt;
    fn paint_targets(&self, binding: &SurfaceBinding) -> Result<Vec<PaintTarget>, AdapterRefusal>;
}

/// A single adapter implementation for all fixed, official web origins.
///
/// `app` is host-derived from the claimed service route. `profile` supplies
/// the per-service selectors and may be replaced without introducing a new
/// adapter type.
pub struct WebSurfaceAdapter<B> {
    app: AdapterAppId,
    profile: adapter_profile::ProfilePayload,
    backend: B,
}

impl<B> WebSurfaceAdapter<B> {
    /// Read-only view of the backend, so conformance tests can assert on what
    /// the adapter did without the field becoming publicly mutable.
    pub fn backend(&self) -> &B {
        &self.backend
    }

    pub fn new(app: AdapterAppId, profile: adapter_profile::ProfilePayload, backend: B) -> Self {
        Self {
            app,
            profile,
            backend,
        }
    }

    pub fn profile(&self) -> &adapter_profile::ProfilePayload {
        &self.profile
    }
}

impl<B: WebSurfaceBackend> WebSurfaceAdapter<B> {
    fn supports(&self, capability: adapter_profile::Capability) -> bool {
        self.backend
            .capabilities(&self.profile, u64::MAX)
            .contains(&capability)
    }

    fn validates_binding(&self, binding: &SurfaceBinding) -> bool {
        binding.app == self.app
            && binding.generation != 0
            && self.backend.is_current_generation(binding.generation)
    }

    fn placement_refused() -> PlacementReceipt {
        PlacementReceipt {
            status: PlacementStatus::NotPlaced,
            placed_sha256: None,
            elapsed_ms: 0,
        }
    }

    fn send_refused() -> SendReceipt {
        SendReceipt {
            outcome: SendOutcome::NotSent,
            elapsed_ms: 0,
        }
    }
}

impl<B: WebSurfaceBackend> SurfaceAdapter for WebSurfaceAdapter<B> {
    fn abi_version(&self) -> u32 {
        ADAPTER_ABI_VERSION
    }

    fn app(&self) -> AdapterAppId {
        self.app.clone()
    }

    fn surface(&self) -> SurfaceKind {
        SurfaceKind::FixedOfficialWebOrigin
    }

    fn capabilities(&self, now_unix_seconds: u64) -> CapabilitySet {
        self.backend.capabilities(&self.profile, now_unix_seconds)
    }

    fn locate(&self, target: &SurfaceTarget) -> Result<SurfaceBinding, AdapterRefusal> {
        if target.app != self.app || target.surface != self.surface() || target.generation == 0 {
            return Err(AdapterRefusal::WindowGone);
        }
        if !self.backend.is_current_generation(target.generation) {
            return Err(AdapterRefusal::GenerationStale);
        }
        // C9: Chromium can expose a partial accessibility tree until it has
        // received an accessibility wake-up.  Do this before every selector
        // walk; never disguise an unavailable tree as a geometry fallback.
        self.backend.wake_accessibility()?;
        let binding = self.backend.locate(&self.profile, target)?;
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
        self.backend.destination(binding)
    }

    fn place(
        &self,
        binding: &SurfaceBinding,
        authorization: &PlacementAuthorization,
        carrier: &Carrier,
    ) -> PlacementReceipt {
        if !self.validates_binding(binding)
            || !same_scope_and_message_box(binding, authorization)
            || !self.supports(adapter_profile::Capability::PlaceProtectedPayload)
        {
            return Self::placement_refused();
        }
        match self.read_state(binding) {
            Ok(state) if !state.composer_is_password_field && state.focused && !state.occluded => {
                if self
                    .backend
                    .page_controls(&self.profile, binding)
                    .validate()
                    .is_err()
                {
                    Self::placement_refused()
                } else {
                    self.backend.place(binding, carrier)
                }
            }
            _ => Self::placement_refused(),
        }
    }

    fn commit(
        &self,
        binding: &SurfaceBinding,
        authorization: &SendAuthorization,
        placed: &PlacementReceipt,
    ) -> SendReceipt {
        if !self.validates_binding(binding)
            || !is_send_evidence_admissible(&binding.evidence)
            || !same_scope(
                &binding.scope_binding_hash,
                authorization.scope_binding_hash(),
            )
            || !self.supports(adapter_profile::Capability::SendProtectedPayload)
            || placed.status != PlacementStatus::Placed
            || placed.placed_sha256.is_none()
        {
            return Self::send_refused();
        }
        let destination = match self.destination(binding) {
            Ok(destination) => destination,
            Err(_) => return Self::send_refused(),
        };
        if destination.status != DestinationStatus::Attested
            || !same_scope(&binding.scope_binding_hash, &destination.scope_binding_hash)
            || !is_send_evidence_admissible(&destination.evidence)
        {
            return Self::send_refused();
        }
        self.backend.commit(binding, placed)
    }

    fn paint_targets(&self, binding: &SurfaceBinding) -> Result<Vec<PaintTarget>, AdapterRefusal> {
        if !self.validates_binding(binding) {
            return Err(AdapterRefusal::GenerationStale);
        }
        let targets = self.backend.paint_targets(binding)?;
        if targets.iter().any(|target| {
            target.confidence == PaintConfidence::Exact && target.carrier_sha256.is_empty()
        }) {
            return Err(AdapterRefusal::AccessibilityUnavailable);
        }
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
        writes: AtomicUsize,
    }

    fn profile() -> adapter_profile::ProfilePayload {
        adapter_profile::ProfilePayload {
            domain: "osl/adapter-profile/v1".into(),
            schema_version: 1,
            adapter_id: "x.web".into(),
            app: adapter_profile::AppDescriptor {
                stable_id: "x".into(),
                display_name: "X".into(),
                service_family: "messaging".into(),
                min_app_version: None,
            },
            revision: adapter_profile::ProfileRevision {
                number: 1,
                label: "test".into(),
            },
            issued_at_unix_seconds: 1,
            expires_at_unix_seconds: u64::MAX,
            support: adapter_profile::SupportLevel::Supported,
            authority: adapter_profile::AuthorityRequirements {
                user_consent_required: true,
                account_binding_required: true,
                release_authority_required: true,
                harmless_canary_required: true,
            },
            selectors: vec![],
            fallbacks: vec![],
            canary: adapter_profile::HarmlessCanary {
                selector: adapter_profile::SelectorKind::AppRoot,
                expected_text: "X".into(),
                max_age_seconds: 1,
            },
        }
    }

    fn binding(generation: u64) -> SurfaceBinding {
        SurfaceBinding {
            app: AdapterAppId::X,
            generation,
            evidence: BindingEvidence::Accessibility {
                tree: A11yTree::Both,
            },
            composer: NodeRef(1),
            transcript: Some(NodeRef(2)),
            bounds: Bounds {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            bound_at_ms: 1,
            scope_binding_hash: "scope".into(),
        }
    }

    impl WebSurfaceBackend for Backend {
        fn capabilities(&self, _: &adapter_profile::ProfilePayload, _: u64) -> CapabilitySet {
            [
                adapter_profile::Capability::PlaceProtectedPayload,
                adapter_profile::Capability::SendProtectedPayload,
            ]
            .into_iter()
            .collect()
        }

        fn is_current_generation(&self, generation: u64) -> bool {
            generation == 7
        }

        fn wake_accessibility(&self) -> Result<(), AdapterRefusal> {
            Ok(())
        }

        fn locate(
            &self,
            _: &adapter_profile::ProfilePayload,
            target: &SurfaceTarget,
        ) -> Result<SurfaceBinding, AdapterRefusal> {
            Ok(binding(target.generation))
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

        fn destination(
            &self,
            binding: &SurfaceBinding,
        ) -> Result<DestinationIdentity, AdapterRefusal> {
            Ok(DestinationIdentity {
                status: DestinationStatus::Attested,
                account_digest: "account".into(),
                conversation_digest: "conversation".into(),
                recipients_digest: "recipients".into(),
                scope_binding_hash: binding.scope_binding_hash.clone(),
                evidence: binding.evidence.clone(),
                attested_at_ms: 1,
                ttl_ms: 1,
            })
        }

        fn place(&self, _: &SurfaceBinding, _: &Carrier) -> PlacementReceipt {
            self.writes.fetch_add(1, Ordering::SeqCst);
            PlacementReceipt {
                status: PlacementStatus::Placed,
                placed_sha256: Some("digest".into()),
                elapsed_ms: 1,
            }
        }

        fn commit(&self, _: &SurfaceBinding, _: &PlacementReceipt) -> SendReceipt {
            self.writes.fetch_add(1, Ordering::SeqCst);
            SendReceipt {
                outcome: SendOutcome::Sent,
                elapsed_ms: 1,
            }
        }

        fn paint_targets(&self, _: &SurfaceBinding) -> Result<Vec<PaintTarget>, AdapterRefusal> {
            Ok(vec![])
        }
    }

    #[test]
    fn web_w1_stale_generation_refuses_every_verb_without_writing() {
        let adapter = WebSurfaceAdapter::new(
            AdapterAppId::X,
            profile(),
            Backend {
                writes: AtomicUsize::new(0),
            },
        );
        let stale = binding(6);
        let placed = PlacementReceipt {
            status: PlacementStatus::Placed,
            placed_sha256: Some("digest".into()),
            elapsed_ms: 1,
        };

        assert_eq!(
            adapter.locate(&SurfaceTarget {
                app: AdapterAppId::X,
                surface: SurfaceKind::FixedOfficialWebOrigin,
                generation: 6,
            }),
            Err(AdapterRefusal::GenerationStale)
        );
        assert_eq!(
            adapter.read_state(&stale),
            Err(AdapterRefusal::GenerationStale)
        );
        assert_eq!(
            adapter.destination(&stale),
            Err(AdapterRefusal::GenerationStale)
        );
        assert_eq!(
            adapter
                .place(
                    &stale,
                    &PlacementAuthorization::for_scope_and_provider("scope", "gmail").unwrap(),
                    &Carrier("carrier".into()),
                )
                .status,
            PlacementStatus::NotPlaced
        );
        assert_eq!(
            adapter
                .commit(&stale, &SendAuthorization::for_scope("scope"), &placed)
                .outcome,
            SendOutcome::NotSent
        );
        assert_eq!(
            adapter.paint_targets(&stale),
            Err(AdapterRefusal::GenerationStale)
        );
        assert_eq!(adapter.backend.writes.load(Ordering::SeqCst), 0);
    }
}
