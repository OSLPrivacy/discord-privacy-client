//! WhatsApp native adapter.  The backend owns the live, already-claimed
//! accessibility snapshot; this wrapper supplies the common ABI policy.

use super::*;

pub trait WhatsAppBackend: Send + Sync {
    fn capabilities(&self, now_unix_seconds: u64) -> CapabilitySet;
    fn locate(&self, target: &SurfaceTarget) -> Result<SurfaceBinding, AdapterRefusal>;
    fn read_state(&self, binding: &SurfaceBinding) -> Result<SurfaceState, AdapterRefusal>;
    fn destination(&self, binding: &SurfaceBinding) -> Result<DestinationIdentity, AdapterRefusal>;
    /// Must write the carrier and then re-read the exact composer/row proof.
    fn place(&self, binding: &SurfaceBinding, carrier: &Carrier) -> PlacementReceipt;
    fn commit(&self, binding: &SurfaceBinding, placed: &PlacementReceipt) -> SendReceipt;
    fn paint_targets(&self, binding: &SurfaceBinding) -> Result<Vec<PaintTarget>, AdapterRefusal>;
}

pub struct WhatsAppSurfaceAdapter<B> { backend: B }
impl<B> WhatsAppSurfaceAdapter<B> { pub fn new(backend: B) -> Self { Self { backend } } }

impl<B: WhatsAppBackend> WhatsAppSurfaceAdapter<B> {
    fn valid(&self, binding: &SurfaceBinding) -> bool { binding.app == AdapterAppId::Whatsapp && binding.generation != 0 }
    fn supports(&self, capability: adapter_profile::Capability) -> bool { self.backend.capabilities(u64::MAX).contains(&capability) }
    fn refused() -> PlacementReceipt { PlacementReceipt { status: PlacementStatus::NotPlaced, placed_sha256: None, elapsed_ms: 0 } }
}

impl<B: WhatsAppBackend> SurfaceAdapter for WhatsAppSurfaceAdapter<B> {
    fn abi_version(&self) -> u32 { ADAPTER_ABI_VERSION }
    fn app(&self) -> AdapterAppId { AdapterAppId::Whatsapp }
    fn surface(&self) -> SurfaceKind { SurfaceKind::InstalledNativeClient }
    fn capabilities(&self, now: u64) -> CapabilitySet { self.backend.capabilities(now) }
    fn locate(&self, target: &SurfaceTarget) -> Result<SurfaceBinding, AdapterRefusal> {
        if target.app != self.app() || target.surface != self.surface() || target.generation == 0 { return Err(AdapterRefusal::WindowGone); }
        let binding = self.backend.locate(target)?;
        if !self.valid(&binding) || binding.generation != target.generation { return Err(AdapterRefusal::GenerationStale); }
        Ok(binding)
    }
    fn read_state(&self, binding: &SurfaceBinding) -> Result<SurfaceState, AdapterRefusal> {
        if !self.valid(binding) { return Err(AdapterRefusal::GenerationStale); }
        self.backend.read_state(binding)
    }
    fn destination(&self, binding: &SurfaceBinding) -> Result<DestinationIdentity, AdapterRefusal> {
        if !self.valid(binding) || !matches!(binding.evidence, BindingEvidence::Accessibility { .. } | BindingEvidence::UserConfirmedVisualBinding { .. }) { return Err(AdapterRefusal::DestinationUnattested); }
        self.backend.destination(binding)
    }
    fn place(&self, binding: &SurfaceBinding, auth: &PlacementAuthorization, carrier: &Carrier) -> PlacementReceipt {
        if !self.valid(binding) || !same_scope(&binding.scope_binding_hash, auth.scope_binding_hash()) || !self.supports(adapter_profile::Capability::PlaceProtectedPayload) { return Self::refused(); }
        match self.read_state(binding) {
            Ok(state) if state.composer_is_empty && !state.composer_is_password_field && state.focused && !state.occluded && state.read_was_complete => {
                let receipt = self.backend.place(binding, carrier);
                // C6: an alleged placement without an exact post-write digest is not a placement.
                if receipt.status == PlacementStatus::Placed && receipt.placed_sha256.is_some() { receipt } else { Self::refused() }
            }
            _ => Self::refused(),
        }
    }
    fn commit(&self, binding: &SurfaceBinding, auth: &SendAuthorization, placed: &PlacementReceipt) -> SendReceipt {
        let refused = || SendReceipt { outcome: SendOutcome::NotSent, elapsed_ms: 0 };
        if !self.valid(binding) || !is_send_evidence_admissible(&binding.evidence) || !same_scope(&binding.scope_binding_hash, auth.scope_binding_hash()) || !self.supports(adapter_profile::Capability::SendProtectedPayload) || placed.status != PlacementStatus::Placed || placed.placed_sha256.is_none() { return refused(); }
        match self.destination(binding) {
            Ok(destination) if destination.status == DestinationStatus::Attested && same_scope(&binding.scope_binding_hash, &destination.scope_binding_hash) && is_send_evidence_admissible(&destination.evidence) => self.backend.commit(binding, placed),
            _ => refused(),
        }
    }
    fn paint_targets(&self, binding: &SurfaceBinding) -> Result<Vec<PaintTarget>, AdapterRefusal> {
        if !self.valid(binding) { return Err(AdapterRefusal::GenerationStale); }
        let targets = self.backend.paint_targets(binding)?;
        if targets.iter().any(|target| target.carrier_sha256.len() != 64 || !target.carrier_sha256.bytes().all(|b| b.is_ascii_hexdigit())) { return Err(AdapterRefusal::AccessibilityUnavailable); }
        Ok(targets)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Backend { writes: AtomicUsize, digest: bool }
    fn binding() -> SurfaceBinding { SurfaceBinding::for_claimed_surface(AdapterAppId::Whatsapp, 1, BindingEvidence::Accessibility { tree: A11yTree::Both }, NodeRef::for_claimed_node(1), Some(NodeRef::for_claimed_node(2)), Bounds { x: 0, y: 0, width: 20, height: 20 }, 1, "scope") }
    impl WhatsAppBackend for Backend {
        fn capabilities(&self, _: u64) -> CapabilitySet { [adapter_profile::Capability::PlaceProtectedPayload, adapter_profile::Capability::SendProtectedPayload].into_iter().collect() }
        fn locate(&self, _: &SurfaceTarget) -> Result<SurfaceBinding, AdapterRefusal> { Ok(binding()) }
        fn read_state(&self, _: &SurfaceBinding) -> Result<SurfaceState, AdapterRefusal> { Ok(SurfaceState { composer_text_sha256: "before".into(), composer_is_empty: true, composer_is_password_field: false, focused: true, occluded: false, read_was_complete: true }) }
        fn destination(&self, b: &SurfaceBinding) -> Result<DestinationIdentity, AdapterRefusal> { Ok(DestinationIdentity { status: DestinationStatus::Attested, account_digest: "a".into(), conversation_digest: "c".into(), recipients_digest: "r".into(), scope_binding_hash: b.scope_binding_hash.clone(), evidence: b.evidence.clone(), attested_at_ms: 1, ttl_ms: 1 }) }
        fn place(&self, _: &SurfaceBinding, _: &Carrier) -> PlacementReceipt { self.writes.fetch_add(1, Ordering::SeqCst); PlacementReceipt { status: PlacementStatus::Placed, placed_sha256: self.digest.then(|| "a".repeat(64)), elapsed_ms: 1 } }
        fn commit(&self, _: &SurfaceBinding, _: &PlacementReceipt) -> SendReceipt { SendReceipt { outcome: SendOutcome::Sent, elapsed_ms: 1 } }
        fn paint_targets(&self, _: &SurfaceBinding) -> Result<Vec<PaintTarget>, AdapterRefusal> { Ok(vec![]) }
    }
    #[test]
    fn t3_t11_c1_through_c10_requires_a_real_post_write_digest() {
        let adapter = WhatsAppSurfaceAdapter::new(Backend { writes: AtomicUsize::new(0), digest: true }); let b = binding();
        let placed = adapter.place(&b, &PlacementAuthorization::for_scope("scope"), &Carrier("carrier".into()));
        assert_eq!(placed.status, PlacementStatus::Placed); assert_eq!(adapter.backend.writes.load(Ordering::SeqCst), 1);
        assert_eq!(adapter.commit(&b, &SendAuthorization::for_scope("scope"), &placed).outcome, SendOutcome::Sent);
        assert_eq!(adapter.commit(&b, &SendAuthorization::for_scope("other"), &placed).outcome, SendOutcome::NotSent);
    }
    #[test]
    fn c6_returning_ok_without_a_post_write_digest_is_refused() {
        let adapter = WhatsAppSurfaceAdapter::new(Backend { writes: AtomicUsize::new(0), digest: false });
        assert_eq!(adapter.place(&binding(), &PlacementAuthorization::for_scope("scope"), &Carrier("carrier".into())).status, PlacementStatus::NotPlaced);
    }
    #[test]
    fn t3_t13_c5_c8_refuses_an_unbound_paint_target_and_pixel_send() {
        let adapter = WhatsAppSurfaceAdapter::new(Backend { writes: AtomicUsize::new(0), digest: true });
        let mut pixel = binding();
        pixel.evidence = BindingEvidence::Pixel;
        let placed = adapter.place(&pixel, &PlacementAuthorization::for_scope("scope"), &Carrier("carrier".into()));
        assert_eq!(adapter.commit(&pixel, &SendAuthorization::for_scope("scope"), &placed).outcome, SendOutcome::NotSent);

        struct BadPaint;
        impl WhatsAppBackend for BadPaint {
            fn capabilities(&self, _: u64) -> CapabilitySet { std::collections::BTreeSet::new() }
            fn locate(&self, _: &SurfaceTarget) -> Result<SurfaceBinding, AdapterRefusal> { Ok(binding()) }
            fn read_state(&self, _: &SurfaceBinding) -> Result<SurfaceState, AdapterRefusal> { unreachable!() }
            fn destination(&self, _: &SurfaceBinding) -> Result<DestinationIdentity, AdapterRefusal> { unreachable!() }
            fn place(&self, _: &SurfaceBinding, _: &Carrier) -> PlacementReceipt { unreachable!() }
            fn commit(&self, _: &SurfaceBinding, _: &PlacementReceipt) -> SendReceipt { unreachable!() }
            fn paint_targets(&self, _: &SurfaceBinding) -> Result<Vec<PaintTarget>, AdapterRefusal> { Ok(vec![PaintTarget { carrier_sha256: String::new(), rect: Bounds { x: 0, y: 0, width: 1, height: 1 }, clipped_by: None, confidence: PaintConfidence::Exact }]) }
        }
        assert_eq!(WhatsAppSurfaceAdapter::new(BadPaint).paint_targets(&binding()), Err(AdapterRefusal::AccessibilityUnavailable));
    }
}
