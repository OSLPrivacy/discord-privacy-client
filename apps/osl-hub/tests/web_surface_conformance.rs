//! T4-W3 / Adapter ABI §12 conformance gate for fixed-origin web adapters.

use osl_privacy_hub::adapters::*;
use osl_privacy_hub::web_surface_adapter::{WebSurfaceAdapter, WebSurfaceBackend};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Clone, Copy)]
enum DestinationMode { Attested, Changed, Unknown }

struct Fixture { writes: AtomicUsize, wakeups: AtomicUsize, password: bool, pixel: bool, destination: DestinationMode, unknown_send: bool, empty_exact_target: bool, wake_fails: bool }

fn profile() -> adapter_profile::ProfilePayload {
    adapter_profile::ProfilePayload { domain: "osl/adapter-profile/v1".into(), schema_version: 1, adapter_id: "fixture.web".into(), app: adapter_profile::AppDescriptor { stable_id: "x".into(), display_name: "X".into(), service_family: "messaging".into(), min_app_version: None }, revision: adapter_profile::ProfileRevision { number: 1, label: "fixture".into() }, issued_at_unix_seconds: 1, expires_at_unix_seconds: u64::MAX, support: adapter_profile::SupportLevel::Supported, authority: adapter_profile::AuthorityRequirements { user_consent_required: true, account_binding_required: true, release_authority_required: true, harmless_canary_required: true }, selectors: vec![], fallbacks: vec![], canary: adapter_profile::HarmlessCanary { selector: adapter_profile::SelectorKind::AppRoot, expected_text: "Messages".into(), max_age_seconds: 1 } }
}
fn binding(generation: u64, pixel: bool) -> SurfaceBinding {
    SurfaceBinding::for_claimed_surface(AdapterAppId::X, generation, if pixel { BindingEvidence::Pixel } else { BindingEvidence::Accessibility { tree: A11yTree::WebAx } }, NodeRef::for_claimed_node(1), Some(NodeRef::for_claimed_node(2)), Bounds { x: 0, y: 0, width: 10, height: 10 }, 1, "opaque-scope")
}
impl WebSurfaceBackend for Fixture {
    fn capabilities(&self, _: &adapter_profile::ProfilePayload, _: u64) -> CapabilitySet { [adapter_profile::Capability::PlaceProtectedPayload, adapter_profile::Capability::SendProtectedPayload].into_iter().collect() }
    fn is_current_generation(&self, generation: u64) -> bool { generation == 7 }
    fn wake_accessibility(&self) -> Result<(), AdapterRefusal> { self.wakeups.fetch_add(1, Ordering::SeqCst); if self.wake_fails { Err(AdapterRefusal::AccessibilityUnavailable) } else { Ok(()) } }
    fn locate(&self, _: &adapter_profile::ProfilePayload, target: &SurfaceTarget) -> Result<SurfaceBinding, AdapterRefusal> { Ok(binding(target.generation, self.pixel)) }
    fn read_state(&self, _: &SurfaceBinding) -> Result<SurfaceState, AdapterRefusal> { Ok(SurfaceState { composer_text_sha256: "a".repeat(64), composer_is_empty: true, composer_is_password_field: self.password, focused: true, occluded: false, read_was_complete: true }) }
    fn destination(&self, b: &SurfaceBinding) -> Result<DestinationIdentity, AdapterRefusal> { Ok(DestinationIdentity { status: match self.destination { DestinationMode::Attested => DestinationStatus::Attested, DestinationMode::Changed => DestinationStatus::Changed, DestinationMode::Unknown => DestinationStatus::Unknown }, account_digest: "a".repeat(64), conversation_digest: "b".repeat(64), recipients_digest: "c".repeat(64), scope_binding_hash: b.scope_binding_hash.clone(), evidence: b.evidence.clone(), attested_at_ms: 1, ttl_ms: 1 }) }
    fn place(&self, _: &SurfaceBinding, _: &Carrier) -> PlacementReceipt { self.writes.fetch_add(1, Ordering::SeqCst); PlacementReceipt { status: PlacementStatus::Placed, placed_sha256: Some("d".repeat(64)), elapsed_ms: 1 } }
    fn commit(&self, _: &SurfaceBinding, _: &PlacementReceipt) -> SendReceipt { self.writes.fetch_add(1, Ordering::SeqCst); SendReceipt { outcome: if self.unknown_send { SendOutcome::Unknown } else { SendOutcome::Sent }, elapsed_ms: 1 } }
    fn paint_targets(&self, _: &SurfaceBinding) -> Result<Vec<PaintTarget>, AdapterRefusal> { Ok(vec![PaintTarget { carrier_sha256: if self.empty_exact_target { String::new() } else { "d".repeat(64) }, rect: Bounds { x: 0, y: 0, width: 1, height: 1 }, clipped_by: None, confidence: PaintConfidence::Exact }]) }
}
fn fixture(destination: DestinationMode, pixel: bool, password: bool, unknown_send: bool, empty_exact_target: bool, wake_fails: bool) -> WebSurfaceAdapter<Fixture> { WebSurfaceAdapter::new(AdapterAppId::X, profile(), Fixture { writes: AtomicUsize::new(0), wakeups: AtomicUsize::new(0), password, pixel, destination, unknown_send, empty_exact_target, wake_fails }) }
fn target() -> SurfaceTarget { SurfaceTarget { app: AdapterAppId::X, surface: SurfaceKind::FixedOfficialWebOrigin, generation: 7 } }
fn placed() -> PlacementReceipt { PlacementReceipt { status: PlacementStatus::Placed, placed_sha256: Some("d".repeat(64)), elapsed_ms: 1 } }

#[test]
fn web_w3_c1_to_c10_conformance() {
    let adapter = fixture(DestinationMode::Attested, false, false, false, false, false);
    let stale = binding(6, false);
    assert_eq!(adapter.place(&stale, &PlacementAuthorization::for_scope("opaque-scope"), &Carrier("carrier".into())).status, PlacementStatus::NotPlaced);
    assert_eq!(adapter.backend.writes.load(Ordering::SeqCst), 0);
    let live = adapter.locate(&target()).unwrap();
    assert_eq!(adapter.place(&live, &PlacementAuthorization::for_scope("opaque-scope"), &Carrier("carrier".into())).status, PlacementStatus::Placed);
    assert_eq!(adapter.backend.writes.load(Ordering::SeqCst), 1, "place must not send");

    let password = fixture(DestinationMode::Attested, false, true, false, false, false);
    assert_eq!(password.place(&password.locate(&target()).unwrap(), &PlacementAuthorization::for_scope("opaque-scope"), &Carrier("carrier".into())).status, PlacementStatus::NotPlaced);
    for mode in [DestinationMode::Unknown, DestinationMode::Changed] {
        let adapter = fixture(mode, false, false, false, false, false);
        assert_eq!(adapter.commit(&adapter.locate(&target()).unwrap(), &SendAuthorization::for_scope("opaque-scope"), &placed()).outcome, SendOutcome::NotSent);
    }
    let pixel = fixture(DestinationMode::Attested, true, false, false, false, false);
    let live = pixel.locate(&target()).unwrap();
    assert_eq!(pixel.commit(&live, &SendAuthorization::for_scope("opaque-scope"), &placed()).outcome, SendOutcome::NotSent);
    assert_eq!(pixel.paint_targets(&live), Err(AdapterRefusal::AccessibilityUnavailable));
    let missing_digest = fixture(DestinationMode::Attested, false, false, false, true, false);
    assert_eq!(missing_digest.paint_targets(&missing_digest.locate(&target()).unwrap()), Err(AdapterRefusal::AccessibilityUnavailable));
    let unknown = fixture(DestinationMode::Attested, false, false, true, false, false);
    assert_eq!(unknown.commit(&unknown.locate(&target()).unwrap(), &SendAuthorization::for_scope("opaque-scope"), &placed()).outcome, SendOutcome::Unknown);
    assert_eq!(unknown.backend.writes.load(Ordering::SeqCst), 1);
    let unavailable = fixture(DestinationMode::Attested, false, false, false, false, true);
    assert_eq!(unavailable.locate(&target()), Err(AdapterRefusal::AccessibilityUnavailable));
    assert_eq!(unavailable.backend.wakeups.load(Ordering::SeqCst), 1);
    let marker = "provider-title::must-not-leak";
    assert!(!format!("{:?}", placed()).contains(marker));
}
