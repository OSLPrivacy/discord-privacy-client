//! Negative X direct-message delivery proof for TASK 1112.
//!
//! The first marked cover establishes the live two-account control.  For the
//! second attempt, placement finishes while the browser driver is live, then
//! the driver is stopped immediately before the direct-message commit.  The
//! adapter must refuse that commit without adding a second receiver copy.

use osl_privacy_hub::adapters::*;
use osl_privacy_hub::web_surface_adapter::x::{
    XConversationHeader, XSurfaceDriver, XSurfaceSnapshot, XTranscriptRow, XWebBackend,
};
use osl_privacy_hub::web_surface_adapter::WebSurfaceAdapter;
use std::env;
use std::sync::{Arc, Mutex};

const COVER: &str = "x-cover-1112";
const SCOPE: &str = "task-1112-x-direct-message";
const DRIVER_CHANGED_VALUE: &str = "stopped";
const DRIVER_REFUSAL: &str = "browser driver stopped";
const GENERATION: u64 = 1_112;

struct XMachine {
    sender: Mutex<XSurfaceSnapshot>,
    received_covers: Mutex<Vec<String>>,
    browser_driver_running: Mutex<bool>,
    last_driver_refusal: Mutex<Option<&'static str>>,
}

impl XMachine {
    fn two_accounts() -> Arc<Self> {
        Arc::new(Self {
            sender: Mutex::new(snapshot()),
            received_covers: Mutex::new(Vec::new()),
            browser_driver_running: Mutex::new(true),
            last_driver_refusal: Mutex::new(None),
        })
    }

    fn stop_browser_driver(&self) {
        // Mutation hook used only for the deliberate red proof.  It simulates
        // deleting the stop transition while leaving the verifier unchanged.
        if env::var_os("OSL_TASK_1112_IGNORE_BROWSER_STOP").is_none() {
            *self
                .browser_driver_running
                .lock()
                .expect("browser driver state lock") = false;
        }
    }

    fn require_running(&self) -> Result<(), AdapterRefusal> {
        if *self
            .browser_driver_running
            .lock()
            .expect("browser driver state lock")
        {
            return Ok(());
        }

        *self
            .last_driver_refusal
            .lock()
            .expect("browser driver refusal lock") = Some(DRIVER_REFUSAL);
        Err(AdapterRefusal::AccessibilityUnavailable)
    }

    fn received_covers(&self) -> Vec<String> {
        self.received_covers
            .lock()
            .expect("received covers lock")
            .clone()
    }

    fn last_driver_refusal(&self) -> Option<&'static str> {
        *self
            .last_driver_refusal
            .lock()
            .expect("browser driver refusal lock")
    }
}

#[derive(Clone)]
struct XBrowserDriver {
    machine: Arc<XMachine>,
}

impl XSurfaceDriver for XBrowserDriver {
    fn capabilities(&self) -> CapabilitySet {
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

    fn snapshot(&self) -> Result<XSurfaceSnapshot, AdapterRefusal> {
        self.machine.require_running()?;
        Ok(self
            .machine
            .sender
            .lock()
            .expect("sender snapshot lock")
            .clone())
    }

    fn place_with_vm_attestation(&self, carrier: &str) -> Result<(), AdapterRefusal> {
        self.machine.require_running()?;
        self.machine
            .sender
            .lock()
            .expect("sender snapshot lock")
            .composer_text = carrier.to_owned();
        Ok(())
    }

    fn commit_with_vm_attestation(&self) -> Result<SendOutcome, AdapterRefusal> {
        self.machine.require_running()?;
        let carrier = {
            let mut sender = self.machine.sender.lock().expect("sender snapshot lock");
            let carrier = std::mem::take(&mut sender.composer_text);
            sender.rows.push(XTranscriptRow {
                rect: Bounds {
                    x: 4,
                    y: 20,
                    width: 320,
                    height: 24,
                },
                carrier: Some(carrier.clone()),
            });
            carrier
        };
        self.machine
            .received_covers
            .lock()
            .expect("received covers lock")
            .push(carrier);
        Ok(SendOutcome::Sent)
    }
}

fn profile() -> adapter_profile::ProfilePayload {
    adapter_profile::ProfilePayload {
        domain: "osl/adapter-profile/v1".into(),
        schema_version: 1,
        adapter_id: "x.web.task-1112".into(),
        app: adapter_profile::AppDescriptor {
            stable_id: "x".into(),
            display_name: "X".into(),
            service_family: "messaging".into(),
            min_app_version: None,
        },
        revision: adapter_profile::ProfileRevision {
            number: 1,
            label: "task-1112".into(),
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

fn snapshot() -> XSurfaceSnapshot {
    XSurfaceSnapshot {
        generation: GENERATION,
        composer: NodeRef::for_claimed_node(1),
        transcript: Some(NodeRef::for_claimed_node(2)),
        bounds: Bounds {
            x: 0,
            y: 0,
            width: 640,
            height: 480,
        },
        bound_at_ms: GENERATION,
        scope_binding_hash: SCOPE.into(),
        composer_text: String::new(),
        composer_is_password_field: false,
        focused: true,
        occluded: false,
        read_was_complete: true,
        header: XConversationHeader {
            account_label: Some("task-1112-alice-x-test".into()),
            conversation_label: Some("DM with task-1112-bob-x-test".into()),
            recipient_labels: vec!["task-1112-bob-x-test".into()],
        },
        transcript_epoch: GENERATION,
        rows: Vec::new(),
    }
}

fn place(
    sender: &WebSurfaceAdapter<XWebBackend<XBrowserDriver>>,
    binding: &SurfaceBinding,
) -> PlacementReceipt {
    sender.place(
        binding,
        &PlacementAuthorization::for_scope(SCOPE),
        &Carrier(COVER.to_owned()),
    )
}

fn commit(
    sender: &WebSurfaceAdapter<XWebBackend<XBrowserDriver>>,
    binding: &SurfaceBinding,
    placed: &PlacementReceipt,
) -> SendReceipt {
    sender.commit(binding, &SendAuthorization::for_scope(SCOPE), placed)
}

#[test]
fn task_1112_stopped_x_browser_driver_refuses_send_and_preserves_one_received_cover() {
    let machine = XMachine::two_accounts();
    let sender = WebSurfaceAdapter::new(
        AdapterAppId::X,
        profile(),
        XWebBackend::new(XBrowserDriver {
            machine: machine.clone(),
        }),
    );
    let target = SurfaceTarget {
        app: AdapterAppId::X,
        surface: SurfaceKind::FixedOfficialWebOrigin,
        generation: GENERATION,
    };
    let binding = sender.locate(&target).expect("live X browser is bound");

    let good_placement = place(&sender, &binding);
    assert_eq!(good_placement.status, PlacementStatus::Placed);
    let good_send = commit(&sender, &binding, &good_placement);
    assert_eq!(good_send.outcome, SendOutcome::Sent);

    let good_received = machine.received_covers();
    println!(
        "TASK1112 good_cover={COVER} received_cover_count={} received_cover={}",
        good_received.len(),
        good_received
            .first()
            .map(String::as_str)
            .unwrap_or("<none>")
    );
    assert_eq!(good_received, vec![COVER]);

    // Marked direct-message attempt: place while live, stop the browser driver,
    // then call the exact same one-shot commit seam used by the good control.
    let stopped_placement = place(&sender, &binding);
    assert_eq!(stopped_placement.status, PlacementStatus::Placed);
    machine.stop_browser_driver();
    let stopped_send = commit(&sender, &binding, &stopped_placement);
    let refusal = machine.last_driver_refusal();
    let final_received = machine.received_covers();

    println!(
        "TASK1112 changed_browser_driver={DRIVER_CHANGED_VALUE} refused_by_name={DRIVER_CHANGED_VALUE} driver_refusal={} send_outcome={:?}",
        refusal.unwrap_or("<none>"),
        stopped_send.outcome
    );
    println!(
        "TASK1112 final_received_cover_count={} final_received_cover={} unchanged={}",
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
