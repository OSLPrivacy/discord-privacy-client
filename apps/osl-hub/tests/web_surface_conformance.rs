//! T4-W3 / Adapter ABI §12 conformance gate for fixed-origin web adapters.
//!
//! This fixture deliberately uses the shipping `WebSurfaceAdapter`, not a
//! parallel mock implementation.  Service tasks parameterise the same adapter
//! with their signed profile and backend, so weakening one of these rules makes
//! the gate fail before an L2/L3 grant can ship.

use osl_privacy_hub::adapters::*;
use osl_privacy_hub::web_surface_adapter::{
    WebPageControlRefusal, WebPageControls, WebPlacementCommandAction, WebPlacementCommandRefusal,
    WebSurfaceAdapter, WebSurfaceBackend,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

#[derive(Clone, Copy)]
enum DestinationMode {
    Attested,
    Changed,
    Unknown,
}

struct Fixture {
    writes: AtomicUsize,
    wakeups: AtomicUsize,
    password: bool,
    pixel: bool,
    destination: DestinationMode,
    unknown_send: bool,
    empty_exact_target: bool,
    wake_fails: bool,
}

fn profile() -> adapter_profile::ProfilePayload {
    adapter_profile::ProfilePayload {
        domain: "osl/adapter-profile/v1".into(),
        schema_version: 1,
        adapter_id: "fixture.web".into(),
        app: adapter_profile::AppDescriptor {
            stable_id: "x".into(),
            display_name: "X".into(),
            service_family: "messaging".into(),
            min_app_version: None,
        },
        revision: adapter_profile::ProfileRevision {
            number: 1,
            label: "fixture".into(),
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
            expected_text: "Messages".into(),
            max_age_seconds: 1,
        },
    }
}

fn binding(generation: u64, pixel: bool) -> SurfaceBinding {
    SurfaceBinding::for_claimed_surface(
        AdapterAppId::X,
        generation,
        if pixel {
            BindingEvidence::Pixel
        } else {
            BindingEvidence::Accessibility {
                tree: A11yTree::WebAx,
            }
        },
        NodeRef::for_claimed_node(1),
        Some(NodeRef::for_claimed_node(2)),
        Bounds {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        },
        1,
        "opaque-scope",
    )
}

impl WebSurfaceBackend for Fixture {
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
        self.wakeups.fetch_add(1, Ordering::SeqCst);
        if self.wake_fails {
            Err(AdapterRefusal::AccessibilityUnavailable)
        } else {
            Ok(())
        }
    }
    fn locate(
        &self,
        _: &adapter_profile::ProfilePayload,
        target: &SurfaceTarget,
    ) -> Result<SurfaceBinding, AdapterRefusal> {
        Ok(binding(target.generation, self.pixel))
    }
    fn read_state(&self, _: &SurfaceBinding) -> Result<SurfaceState, AdapterRefusal> {
        Ok(SurfaceState {
            composer_text_sha256: "a".repeat(64),
            composer_is_empty: true,
            composer_is_password_field: self.password,
            focused: true,
            occluded: false,
            read_was_complete: true,
        })
    }
    fn destination(&self, b: &SurfaceBinding) -> Result<DestinationIdentity, AdapterRefusal> {
        Ok(DestinationIdentity {
            status: match self.destination {
                DestinationMode::Attested => DestinationStatus::Attested,
                DestinationMode::Changed => DestinationStatus::Changed,
                DestinationMode::Unknown => DestinationStatus::Unknown,
            },
            account_digest: "a".repeat(64),
            conversation_digest: "b".repeat(64),
            recipients_digest: "c".repeat(64),
            scope_binding_hash: "opaque-scope".into(),
            evidence: b.evidence.clone(),
            attested_at_ms: 1,
            ttl_ms: 1,
        })
    }
    fn place(&self, _: &SurfaceBinding, _: &Carrier) -> PlacementReceipt {
        self.writes.fetch_add(1, Ordering::SeqCst);
        PlacementReceipt {
            status: PlacementStatus::Placed,
            placed_sha256: Some("d".repeat(64)),
            elapsed_ms: 1,
        }
    }
    fn commit(&self, _: &SurfaceBinding, _: &PlacementReceipt) -> SendReceipt {
        self.writes.fetch_add(1, Ordering::SeqCst);
        SendReceipt {
            outcome: if self.unknown_send {
                SendOutcome::Unknown
            } else {
                SendOutcome::Sent
            },
            elapsed_ms: 1,
        }
    }
    fn paint_targets(&self, _: &SurfaceBinding) -> Result<Vec<PaintTarget>, AdapterRefusal> {
        Ok(vec![PaintTarget {
            carrier_sha256: if self.empty_exact_target {
                String::new()
            } else {
                "d".repeat(64)
            },
            rect: Bounds {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            clipped_by: None,
            confidence: PaintConfidence::Exact,
        }])
    }
}

fn adapter<B>(fixture: B) -> WebSurfaceAdapter<B> {
    WebSurfaceAdapter::new(AdapterAppId::X, profile(), fixture)
}
fn current_target() -> SurfaceTarget {
    SurfaceTarget {
        app: AdapterAppId::X,
        surface: SurfaceKind::FixedOfficialWebOrigin,
        generation: 7,
    }
}
fn placed() -> PlacementReceipt {
    PlacementReceipt {
        status: PlacementStatus::Placed,
        placed_sha256: Some("d".repeat(64)),
        elapsed_ms: 1,
    }
}

struct Task1206Page {
    body_present: bool,
    body_editable: bool,
    send_present: bool,
    body_text: String,
}

struct Task1206Fixture {
    page: Mutex<Task1206Page>,
    placements: AtomicUsize,
    sends: AtomicUsize,
    last_control_refusal: Mutex<Option<WebPageControlRefusal>>,
}

impl Task1206Fixture {
    fn new() -> Self {
        Self {
            page: Mutex::new(Task1206Page {
                body_present: true,
                body_editable: true,
                send_present: true,
                body_text: String::new(),
            }),
            placements: AtomicUsize::new(0),
            sends: AtomicUsize::new(0),
            last_control_refusal: Mutex::new(None),
        }
    }

    fn placement_count(&self) -> usize {
        self.placements.load(Ordering::SeqCst)
    }

    fn body_text(&self) -> String {
        self.page.lock().unwrap().body_text.clone()
    }

    fn send_count(&self) -> usize {
        self.sends.load(Ordering::SeqCst)
    }

    fn set_send_present(&self, present: bool) {
        self.page.lock().unwrap().send_present = present;
    }

    fn set_body_editable(&self, editable: bool) {
        self.page.lock().unwrap().body_editable = editable;
    }

    fn last_control_refusal(&self) -> Option<WebPageControlRefusal> {
        *self.last_control_refusal.lock().unwrap()
    }
}

impl WebSurfaceBackend for Task1206Fixture {
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
        Ok(binding(target.generation, false))
    }

    fn read_state(&self, _: &SurfaceBinding) -> Result<SurfaceState, AdapterRefusal> {
        Ok(SurfaceState {
            composer_text_sha256: "a".repeat(64),
            composer_is_empty: true,
            composer_is_password_field: false,
            focused: true,
            occluded: false,
            read_was_complete: true,
        })
    }

    fn page_controls(
        &self,
        _: &adapter_profile::ProfilePayload,
        _: &SurfaceBinding,
    ) -> WebPageControls {
        let page = self.page.lock().unwrap();
        let controls = WebPageControls {
            body_present: page.body_present,
            body_editable: page.body_editable,
            send_present: page.send_present,
        };
        *self.last_control_refusal.lock().unwrap() = controls.validate().err();
        controls
    }

    fn destination(&self, b: &SurfaceBinding) -> Result<DestinationIdentity, AdapterRefusal> {
        Ok(DestinationIdentity {
            status: DestinationStatus::Attested,
            account_digest: "a".repeat(64),
            conversation_digest: "b".repeat(64),
            recipients_digest: "c".repeat(64),
            scope_binding_hash: "opaque-scope".into(),
            evidence: b.evidence.clone(),
            attested_at_ms: 1,
            ttl_ms: 1,
        })
    }

    fn place(&self, _: &SurfaceBinding, carrier: &Carrier) -> PlacementReceipt {
        self.placements.fetch_add(1, Ordering::SeqCst);
        self.page.lock().unwrap().body_text = carrier.0.clone();
        PlacementReceipt {
            status: PlacementStatus::Placed,
            placed_sha256: Some("d".repeat(64)),
            elapsed_ms: 1,
        }
    }

    fn commit(&self, _: &SurfaceBinding, _: &PlacementReceipt) -> SendReceipt {
        self.sends.fetch_add(1, Ordering::SeqCst);
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
fn web_w3_c1_to_c10_conformance() {
    // C1 + C6: stale binding writes nothing; place itself never commits.
    let a = adapter(Fixture {
        writes: AtomicUsize::new(0),
        wakeups: AtomicUsize::new(0),
        password: false,
        pixel: false,
        destination: DestinationMode::Attested,
        unknown_send: false,
        empty_exact_target: false,
        wake_fails: false,
    });
    let stale = binding(6, false);
    assert_eq!(
        a.place(
            &stale,
            &PlacementAuthorization::for_scope("opaque-scope"),
            &Carrier("carrier".into())
        )
        .status,
        PlacementStatus::NotPlaced
    );
    assert_eq!(a.backend().writes.load(Ordering::SeqCst), 0);
    let b = a.locate(&current_target()).unwrap();
    assert_eq!(
        a.place(
            &b,
            &PlacementAuthorization::for_scope("opaque-scope"),
            &Carrier("carrier".into())
        )
        .status,
        PlacementStatus::Placed
    );
    assert_eq!(
        a.backend().writes.load(Ordering::SeqCst),
        1,
        "place must not send"
    );

    // C2: a password composer never accepts a carrier.
    let a = adapter(Fixture {
        writes: AtomicUsize::new(0),
        wakeups: AtomicUsize::new(0),
        password: true,
        pixel: false,
        destination: DestinationMode::Attested,
        unknown_send: false,
        empty_exact_target: false,
        wake_fails: false,
    });
    let b = a.locate(&current_target()).unwrap();
    assert_eq!(
        a.place(
            &b,
            &PlacementAuthorization::for_scope("opaque-scope"),
            &Carrier("carrier".into())
        )
        .status,
        PlacementStatus::NotPlaced
    );

    // C3/C4: neither Unknown nor Changed may commit.
    for mode in [DestinationMode::Unknown, DestinationMode::Changed] {
        let a = adapter(Fixture {
            writes: AtomicUsize::new(0),
            wakeups: AtomicUsize::new(0),
            password: false,
            pixel: false,
            destination: mode,
            unknown_send: false,
            empty_exact_target: false,
            wake_fails: false,
        });
        let b = a.locate(&current_target()).unwrap();
        assert_eq!(
            a.commit(&b, &SendAuthorization::for_scope("opaque-scope"), &placed())
                .outcome,
            SendOutcome::NotSent
        );
    }

    // C5/C8: Pixel cannot commit or yield an Exact paint target; exact needs a carrier digest.
    let a = adapter(Fixture {
        writes: AtomicUsize::new(0),
        wakeups: AtomicUsize::new(0),
        password: false,
        pixel: true,
        destination: DestinationMode::Attested,
        unknown_send: false,
        empty_exact_target: false,
        wake_fails: false,
    });
    let b = a.locate(&current_target()).unwrap();
    assert_eq!(
        a.commit(&b, &SendAuthorization::for_scope("opaque-scope"), &placed())
            .outcome,
        SendOutcome::NotSent
    );
    assert_eq!(
        a.paint_targets(&b),
        Err(AdapterRefusal::AccessibilityUnavailable)
    );
    let a = adapter(Fixture {
        writes: AtomicUsize::new(0),
        wakeups: AtomicUsize::new(0),
        password: false,
        pixel: false,
        destination: DestinationMode::Attested,
        unknown_send: false,
        empty_exact_target: true,
        wake_fails: false,
    });
    let b = a.locate(&current_target()).unwrap();
    assert_eq!(
        a.paint_targets(&b),
        Err(AdapterRefusal::AccessibilityUnavailable)
    );

    // C7: Unknown is preserved and one commit means one backend invocation.
    let a = adapter(Fixture {
        writes: AtomicUsize::new(0),
        wakeups: AtomicUsize::new(0),
        password: false,
        pixel: false,
        destination: DestinationMode::Attested,
        unknown_send: true,
        empty_exact_target: false,
        wake_fails: false,
    });
    let b = a.locate(&current_target()).unwrap();
    assert_eq!(
        a.commit(&b, &SendAuthorization::for_scope("opaque-scope"), &placed())
            .outcome,
        SendOutcome::Unknown
    );
    assert_eq!(a.backend().writes.load(Ordering::SeqCst), 1);

    // C9: wake precedes selector work and unavailable a11y is surfaced.
    let a = adapter(Fixture {
        writes: AtomicUsize::new(0),
        wakeups: AtomicUsize::new(0),
        password: false,
        pixel: false,
        destination: DestinationMode::Attested,
        unknown_send: false,
        empty_exact_target: false,
        wake_fails: true,
    });
    assert_eq!(
        a.locate(&current_target()),
        Err(AdapterRefusal::AccessibilityUnavailable)
    );
    assert_eq!(a.backend().wakeups.load(Ordering::SeqCst), 1);

    // C10 is representation-level: adapter public results are enum states,
    // hashes and geometry; this distinctive provider input cannot escape.
    let marker = "provider-title::must-not-leak";
    assert!(!format!("{:?}", placed()).contains(marker));
    assert!(!format!("{:?}", AdapterRefusal::ComposerNotFound).contains(marker));
}

#[test]
fn task_1206_missing_send_control_refuses_without_replacing_body() {
    let adapter = adapter(Task1206Fixture::new());
    let binding = adapter.locate(&current_target()).unwrap();
    let carrier = Carrier("MAPLE-4172".into());

    println!(
        "TASK1206_PLACEMENT_COUNT_BEFORE_GOOD={}",
        adapter.backend().placement_count()
    );
    assert_eq!(adapter.backend().placement_count(), 0);

    let good = adapter.place(
        &binding,
        &PlacementAuthorization::for_scope("opaque-scope"),
        &carrier,
    );
    assert_eq!(good.status, PlacementStatus::Placed);
    assert_eq!(adapter.backend().placement_count(), 1);
    assert_eq!(adapter.backend().body_text(), "MAPLE-4172");
    println!("TASK1206_BODY_AFTER_GOOD={}", adapter.backend().body_text());
    println!(
        "TASK1206_PLACEMENT_COUNT_AFTER_GOOD={}",
        adapter.backend().placement_count()
    );

    adapter.backend().set_send_present(false);
    let refused = adapter.place(
        &binding,
        &PlacementAuthorization::for_scope("opaque-scope"),
        &carrier,
    );
    assert_eq!(refused.status, PlacementStatus::NotPlaced);
    let refusal = adapter
        .backend()
        .last_control_refusal()
        .expect("missing Send must be the named page-control refusal");
    assert_eq!(refusal, WebPageControlRefusal::MissingSend);
    assert_eq!(refusal.to_string(), "missing Send");
    assert_eq!(adapter.backend().body_text(), "MAPLE-4172");
    assert_eq!(adapter.backend().placement_count(), 1);
    println!("TASK1206_MISSING_SEND_REFUSAL={refusal}");
    println!(
        "TASK1206_BODY_AFTER_MISSING_SEND={}",
        adapter.backend().body_text()
    );
    println!(
        "TASK1206_PLACEMENT_COUNT_AFTER_MISSING_SEND={}",
        adapter.backend().placement_count()
    );
}

#[test]
fn task_1209_check_placement_does_not_submit() {
    let adapter = adapter(Task1206Fixture::new());
    let binding = adapter.locate(&current_target()).unwrap();
    let authorization = PlacementAuthorization::for_scope("opaque-scope");
    let carrier = Carrier("MAPLE-4172".into());

    println!(
        "TASK1209_PLACEMENT_COUNT_BEFORE={}",
        adapter.backend().placement_count()
    );
    println!(
        "TASK1209_SEND_COUNT_BEFORE={}",
        adapter.backend().send_count()
    );
    assert_eq!(adapter.backend().placement_count(), 0);
    assert_eq!(adapter.backend().send_count(), 0);

    let placed = adapter
        .run_placement_command(
            &binding,
            &authorization,
            &carrier,
            WebPlacementCommandAction::Place,
        )
        .expect("place action should run through the placement command");
    assert_eq!(placed.status, PlacementStatus::Placed);
    assert_eq!(adapter.backend().placement_count(), 1);
    assert_eq!(adapter.backend().body_text(), "MAPLE-4172");
    assert_eq!(adapter.backend().send_count(), 0);
    println!("TASK1209_REQUESTED_ACTION_BEFORE=place");
    println!(
        "TASK1209_BODY_READS_AFTER_PLACE={}",
        adapter.backend().body_text()
    );
    println!(
        "TASK1209_PLACEMENT_COUNT_AFTER_PLACE={}",
        adapter.backend().placement_count()
    );
    println!(
        "TASK1209_SEND_COUNT_AFTER_PLACE={}",
        adapter.backend().send_count()
    );

    let refused = adapter
        .run_placement_command(
            &binding,
            &authorization,
            &carrier,
            WebPlacementCommandAction::Submit,
        )
        .expect_err("submit action must be refused by the placement command");
    assert_eq!(refused, WebPlacementCommandRefusal::PlacementCannotSubmit);
    assert_eq!(refused.to_string(), "placement cannot submit");
    assert_eq!(adapter.backend().body_text(), "MAPLE-4172");
    assert_eq!(adapter.backend().placement_count(), 1);
    assert_eq!(adapter.backend().send_count(), 0);
    println!("TASK1209_REQUESTED_ACTION_AFTER=submit");
    println!("TASK1209_SUBMIT_REFUSAL={refused}");
    println!(
        "TASK1209_BODY_READS_AFTER_SUBMIT_REFUSAL={}",
        adapter.backend().body_text()
    );
    println!(
        "TASK1209_PLACEMENT_COUNT_AFTER_SUBMIT_REFUSAL={}",
        adapter.backend().placement_count()
    );
    println!(
        "TASK1209_SEND_COUNT_AFTER_SUBMIT_REFUSAL={}",
        adapter.backend().send_count()
    );
}

#[test]
fn task_1263_body_editable_no_refuses_second_placement() {
    let adapter = adapter(Task1206Fixture::new());
    let binding = adapter.locate(&current_target()).unwrap();
    let carrier = Carrier("MAPLE-4172".into());

    println!(
        "TASK1263_PLACEMENT_COUNT_BEFORE={}",
        adapter.backend().placement_count()
    );
    assert_eq!(adapter.backend().placement_count(), 0);

    let good = adapter.place(
        &binding,
        &PlacementAuthorization::for_scope("opaque-scope"),
        &carrier,
    );
    assert_eq!(good.status, PlacementStatus::Placed);
    assert_eq!(adapter.backend().placement_count(), 1);
    assert_eq!(adapter.backend().body_text(), "MAPLE-4172");
    println!(
        "TASK1263_BODY_AFTER_EDITABLE_YES={}",
        adapter.backend().body_text()
    );
    println!(
        "TASK1263_PLACEMENT_COUNT_AFTER_EDITABLE_YES={}",
        adapter.backend().placement_count()
    );

    adapter.backend().set_body_editable(false);
    let refused = adapter.place(
        &binding,
        &PlacementAuthorization::for_scope("opaque-scope"),
        &carrier,
    );
    assert_eq!(refused.status, PlacementStatus::NotPlaced);
    let refusal = adapter
        .backend()
        .last_control_refusal()
        .expect("non-editable Body must be the named page-control refusal");
    assert_eq!(refusal, WebPageControlRefusal::BodyNotEditable);
    assert_eq!(refusal.to_string(), "Body not editable");
    assert_eq!(adapter.backend().body_text(), "MAPLE-4172");
    assert_eq!(adapter.backend().placement_count(), 1);
    println!("TASK1263_EDITABLE_NO_REFUSAL={refusal}");
    println!(
        "TASK1263_BODY_AFTER_EDITABLE_NO={}",
        adapter.backend().body_text()
    );
    println!(
        "TASK1263_PLACEMENT_COUNT_AFTER_EDITABLE_NO={}",
        adapter.backend().placement_count()
    );
}
