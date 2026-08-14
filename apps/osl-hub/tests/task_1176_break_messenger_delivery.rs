//! Negative Messenger direct-message delivery proof for TASK 1176.
//!
//! The first marked cover establishes the two-account control through the
//! shared fixed-origin web adapter. For the second attempt, placement finishes
//! while the browser driver is live, then the driver is stopped immediately
//! before commit. The adapter must refuse that commit without adding another
//! receiver copy.

use osl_privacy_hub::adapters::*;
use osl_privacy_hub::web_surface_adapter::{WebSurfaceAdapter, WebSurfaceBackend};
use std::env;
use std::sync::{Arc, Mutex};

const COVER: &str = "messenger-cover-1176";
const SCOPE: &str = "task-1176-messenger-direct-message";
const DRIVER_CHANGED_VALUE: &str = "stopped";
const DRIVER_REFUSAL: &str = "browser driver stopped";
const GENERATION: u64 = 1_176;

#[derive(Default)]
struct MessengerMachineState {
    browser_driver_running: bool,
    composer: String,
    received_covers: Vec<String>,
    last_driver_refusal: Option<&'static str>,
}

struct MessengerMachine {
    state: Mutex<MessengerMachineState>,
}

impl MessengerMachine {
    fn two_accounts() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(MessengerMachineState {
                browser_driver_running: true,
                ..MessengerMachineState::default()
            }),
        })
    }

    fn stop_browser_driver(&self) {
        // Mutation hook for the deliberate red proof. Ignoring this transition
        // must make the unchanged-receiver/refusal assertions fail.
        if env::var_os("OSL_TASK_1176_IGNORE_BROWSER_STOP").is_none() {
            self.state
                .lock()
                .expect("Messenger machine lock")
                .browser_driver_running = false;
        }
    }

    fn require_running(&self) -> Result<(), AdapterRefusal> {
        let mut state = self.state.lock().expect("Messenger machine lock");
        if state.browser_driver_running {
            return Ok(());
        }
        state.last_driver_refusal = Some(DRIVER_REFUSAL);
        Err(AdapterRefusal::AccessibilityUnavailable)
    }

    fn received_covers(&self) -> Vec<String> {
        self.state
            .lock()
            .expect("Messenger machine lock")
            .received_covers
            .clone()
    }

    fn last_driver_refusal(&self) -> Option<&'static str> {
        self.state
            .lock()
            .expect("Messenger machine lock")
            .last_driver_refusal
    }
}

#[derive(Clone)]
struct MessengerBrowserDriver {
    machine: Arc<MessengerMachine>,
}

impl WebSurfaceBackend for MessengerBrowserDriver {
    fn capabilities(&self, _: &adapter_profile::ProfilePayload, _: u64) -> CapabilitySet {
        [
            adapter_profile::Capability::PlaceProtectedPayload,
            adapter_profile::Capability::SendProtectedPayload,
        ]
        .into_iter()
        .collect()
    }

    fn is_current_generation(&self, generation: u64) -> bool {
        generation == GENERATION
    }

    fn wake_accessibility(&self) -> Result<(), AdapterRefusal> {
        self.machine.require_running()
    }

    fn locate(
        &self,
        _: &adapter_profile::ProfilePayload,
        target: &SurfaceTarget,
    ) -> Result<SurfaceBinding, AdapterRefusal> {
        self.machine.require_running()?;
        Ok(SurfaceBinding::for_claimed_surface(
            AdapterAppId::Messenger,
            target.generation,
            BindingEvidence::Accessibility {
                tree: A11yTree::WebAx,
            },
            NodeRef::for_claimed_node(1),
            Some(NodeRef::for_claimed_node(2)),
            Bounds {
                x: 0,
                y: 0,
                width: 640,
                height: 480,
            },
            GENERATION,
            SCOPE,
        ))
    }

    fn read_state(&self, _: &SurfaceBinding) -> Result<SurfaceState, AdapterRefusal> {
        self.machine.require_running()?;
        let state = self.machine.state.lock().expect("Messenger machine lock");
        Ok(SurfaceState {
            composer_text_sha256: format!("messenger-composer-bytes-{}", state.composer.len()),
            composer_is_empty: state.composer.is_empty(),
            composer_is_password_field: false,
            focused: true,
            occluded: false,
            read_was_complete: true,
        })
    }

    fn destination(&self, binding: &SurfaceBinding) -> Result<DestinationIdentity, AdapterRefusal> {
        self.machine.require_running()?;
        Ok(DestinationIdentity {
            status: DestinationStatus::Attested,
            account_digest: "task-1176-alice-messenger".into(),
            conversation_digest: "task-1176-alice-to-bob".into(),
            recipients_digest: "task-1176-bob-messenger".into(),
            scope_binding_hash: SCOPE.into(),
            evidence: binding.evidence.clone(),
            attested_at_ms: GENERATION,
            ttl_ms: 30_000,
        })
    }

    fn place(&self, _: &SurfaceBinding, carrier: &Carrier) -> PlacementReceipt {
        if self.machine.require_running().is_err() {
            return not_placed();
        }
        self.machine
            .state
            .lock()
            .expect("Messenger machine lock")
            .composer = carrier.0.clone();
        PlacementReceipt {
            status: PlacementStatus::Placed,
            placed_sha256: Some(format!("messenger-cover-bytes-{}", carrier.0.len())),
            elapsed_ms: 0,
        }
    }

    fn commit(&self, _: &SurfaceBinding, placed: &PlacementReceipt) -> SendReceipt {
        if self.machine.require_running().is_err()
            || placed.status != PlacementStatus::Placed
            || placed.placed_sha256.is_none()
        {
            return not_sent();
        }
        let mut state = self.machine.state.lock().expect("Messenger machine lock");
        let cover = std::mem::take(&mut state.composer);
        state.received_covers.push(cover);
        SendReceipt {
            outcome: SendOutcome::Sent,
            elapsed_ms: 0,
        }
    }

    fn paint_targets(&self, _: &SurfaceBinding) -> Result<Vec<PaintTarget>, AdapterRefusal> {
        self.machine.require_running()?;
        Ok(Vec::new())
    }
}

fn not_placed() -> PlacementReceipt {
    PlacementReceipt {
        status: PlacementStatus::NotPlaced,
        placed_sha256: None,
        elapsed_ms: 0,
    }
}

fn not_sent() -> SendReceipt {
    SendReceipt {
        outcome: SendOutcome::NotSent,
        elapsed_ms: 0,
    }
}

fn profile() -> adapter_profile::ProfilePayload {
    adapter_profile::ProfilePayload {
        domain: "osl/adapter-profile/v1".into(),
        schema_version: 1,
        adapter_id: "messenger.web.task-1176".into(),
        app: adapter_profile::AppDescriptor {
            stable_id: "messenger".into(),
            display_name: "Messenger".into(),
            service_family: "messaging".into(),
            min_app_version: None,
        },
        revision: adapter_profile::ProfileRevision {
            number: 1,
            label: "task-1176".into(),
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
            expected_text: "Messenger".into(),
            max_age_seconds: 1,
        },
    }
}

fn place(
    sender: &WebSurfaceAdapter<MessengerBrowserDriver>,
    binding: &SurfaceBinding,
) -> PlacementReceipt {
    sender.place(
        binding,
        &PlacementAuthorization::for_scope(SCOPE),
        &Carrier(COVER.to_owned()),
    )
}

fn commit(
    sender: &WebSurfaceAdapter<MessengerBrowserDriver>,
    binding: &SurfaceBinding,
    placed: &PlacementReceipt,
) -> SendReceipt {
    sender.commit(binding, &SendAuthorization::for_scope(SCOPE), placed)
}

#[test]
fn task_1176_stopped_messenger_browser_driver_refuses_send_and_keeps_one_cover() {
    let machine = MessengerMachine::two_accounts();
    let sender = WebSurfaceAdapter::new(
        AdapterAppId::Messenger,
        profile(),
        MessengerBrowserDriver {
            machine: machine.clone(),
        },
    );
    let target = SurfaceTarget {
        app: AdapterAppId::Messenger,
        surface: SurfaceKind::FixedOfficialWebOrigin,
        generation: GENERATION,
    };
    let binding = sender
        .locate(&target)
        .expect("live Messenger browser driver is bound");

    let good_placement = place(&sender, &binding);
    assert_eq!(good_placement.status, PlacementStatus::Placed);
    let good_send = commit(&sender, &binding, &good_placement);
    assert_eq!(good_send.outcome, SendOutcome::Sent);

    let good_received = machine.received_covers();
    println!(
        "TASK1176 good_cover={COVER} received_cover_count={} received_cover={}",
        good_received.len(),
        good_received
            .first()
            .map(String::as_str)
            .unwrap_or("<none>")
    );
    assert_eq!(good_received, vec![COVER]);

    // Place the marked cover while live, stop the browser driver, then call
    // the exact same commit seam used by the good control.
    let stopped_placement = place(&sender, &binding);
    assert_eq!(stopped_placement.status, PlacementStatus::Placed);
    machine.stop_browser_driver();
    let stopped_send = commit(&sender, &binding, &stopped_placement);
    let refusal = machine.last_driver_refusal();
    let final_received = machine.received_covers();

    println!(
        "TASK1176 changed_browser_driver={DRIVER_CHANGED_VALUE} refused_by_name={DRIVER_CHANGED_VALUE} driver_refusal={} send_outcome={:?}",
        refusal.unwrap_or("<none>"),
        stopped_send.outcome
    );
    println!(
        "TASK1176 final_received_cover_count={} final_received_cover={} unchanged={}",
        final_received.len(),
        final_received
            .first()
            .map(String::as_str)
            .unwrap_or("<none>"),
        final_received == good_received
    );

    assert_eq!(refusal, Some(DRIVER_REFUSAL));
    assert_eq!(stopped_send.outcome, SendOutcome::NotSent);
    assert_eq!(final_received, good_received);
    assert_eq!(final_received, vec![COVER]);
}
