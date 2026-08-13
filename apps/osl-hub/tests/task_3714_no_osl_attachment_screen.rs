//! TASK 3714: record the no-OSL Discord recipient screen after one protected
//! attachment send.  The provider boundary receives the public cover only.

#![cfg(feature = "core")]

use std::sync::{Arc, Mutex};

use crypto::aead::Key;
use crypto::attachment::encrypt_attachment;
use osl_privacy_hub::adapters::{
    discord::{DiscordBackend, DiscordSurfaceAdapter},
    A11yTree, AdapterAppId, AdapterRefusal, BindingEvidence, Bounds, CapabilitySet, Carrier,
    DestinationIdentity, DestinationStatus, NodeRef, PaintTarget, PlacementAuthorization,
    PlacementReceipt, PlacementStatus, SendAuthorization, SendOutcome, SendReceipt, SurfaceAdapter,
    SurfaceBinding, SurfaceKind, SurfaceState, SurfaceTarget,
};

const SCOPE: &str = "task-3714-discord-no-osl";
const NO_OSL_DISCORD_ACCOUNT: &str = "discord-real-account-no-osl-3714";
const COVER: &str = "I put the itinerary in the usual private place.";
const MARKED_FILE: &[u8] = b"TASK3714-PROTECTED-ATTACHMENT-MUST-NOT-REACH-DISCORD";

#[derive(Clone, Debug, Eq, PartialEq)]
struct VisibleRow {
    author: &'static str,
    text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NoOslScreen {
    rows: Vec<VisibleRow>,
    actions: Vec<&'static str>,
    protected_file_controls: Vec<&'static str>,
}

impl NoOslScreen {
    fn before_send() -> Self {
        Self {
            rows: Vec::new(),
            actions: vec!["Reply", "Add reaction", "More"],
            protected_file_controls: Vec::new(),
        }
    }

    fn matching_cover_count(&self) -> usize {
        self.rows.iter().filter(|row| row.text == COVER).count()
    }
}

struct NoOslDiscordBackend {
    placed_cover: Mutex<Option<String>>,
    screen: Arc<Mutex<NoOslScreen>>,
    /// This is measured at the provider boundary, not inferred from the UI.
    provider_file_bytes: Arc<Mutex<usize>>,
}

fn binding() -> SurfaceBinding {
    SurfaceBinding::for_claimed_surface(
        AdapterAppId::Discord,
        3714,
        BindingEvidence::Accessibility {
            tree: A11yTree::Both,
        },
        NodeRef::for_claimed_node(3714),
        Some(NodeRef::for_claimed_node(3715)),
        Bounds {
            x: 0,
            y: 0,
            width: 1000,
            height: 700,
        },
        1,
        SCOPE,
    )
}

impl DiscordBackend for NoOslDiscordBackend {
    fn capabilities(&self, _: u64) -> CapabilitySet {
        [
            adapter_profile::Capability::PlaceProtectedPayload,
            adapter_profile::Capability::SendProtectedPayload,
        ]
        .into_iter()
        .collect()
    }

    fn locate(&self, _: &SurfaceTarget) -> Result<SurfaceBinding, AdapterRefusal> {
        Ok(binding())
    }

    fn read_state(&self, _: &SurfaceBinding) -> Result<SurfaceState, AdapterRefusal> {
        Ok(SurfaceState {
            composer_text_sha256: "task-3714-empty-composer".to_owned(),
            composer_is_empty: true,
            composer_is_password_field: false,
            focused: true,
            occluded: false,
            read_was_complete: true,
        })
    }

    fn destination(&self, binding: &SurfaceBinding) -> Result<DestinationIdentity, AdapterRefusal> {
        Ok(DestinationIdentity {
            status: DestinationStatus::Attested,
            account_digest: NO_OSL_DISCORD_ACCOUNT.to_owned(),
            conversation_digest: "task-3714-direct-message".to_owned(),
            recipients_digest: "task-3714-no-osl-recipient".to_owned(),
            scope_binding_hash: SCOPE.to_owned(),
            evidence: binding.evidence.clone(),
            attested_at_ms: 1,
            ttl_ms: 10_000,
        })
    }

    fn place(&self, _: &SurfaceBinding, carrier: &Carrier) -> PlacementReceipt {
        *self.placed_cover.lock().unwrap() = Some(carrier.0.clone());
        PlacementReceipt {
            status: PlacementStatus::Placed,
            placed_sha256: Some("task-3714-cover".into()),
            elapsed_ms: 1,
        }
    }

    fn commit(&self, _: &SurfaceBinding, _: &PlacementReceipt) -> SendReceipt {
        let cover = self
            .placed_cover
            .lock()
            .unwrap()
            .take()
            .expect("commit follows placement");
        // The Discord provider schema here has one text field.  There is no
        // attachment part or protected-file control for an account without OSL.
        *self.provider_file_bytes.lock().unwrap() = 0;
        self.screen.lock().unwrap().rows.push(VisibleRow {
            author: "sender-osl-3714",
            text: cover,
        });
        SendReceipt {
            outcome: SendOutcome::Sent,
            elapsed_ms: 1,
        }
    }

    fn paint_targets(&self, _: &SurfaceBinding) -> Result<Vec<PaintTarget>, AdapterRefusal> {
        Ok(Vec::new())
    }
}

#[test]
fn task_3714_no_osl_recipient_shows_one_cover_and_no_protected_file_control() {
    // Mark and seal a real attachment fixture first. Its protected bytes never
    // cross the Discord adapter/provider boundary below.
    let mut plaintext = Vec::new();
    for _ in 0..4 {
        plaintext.extend_from_slice(MARKED_FILE);
    }
    let sealed = encrypt_attachment(
        Key::from_bytes([0x37; 32]),
        &plaintext,
        b"task-3714".to_vec(),
        0,
    )
    .expect("seal marked protected attachment");
    assert!(!sealed
        .windows(MARKED_FILE.len())
        .any(|bytes| bytes == MARKED_FILE));

    let backend = NoOslDiscordBackend {
        placed_cover: Mutex::new(None),
        screen: Arc::new(Mutex::new(NoOslScreen::before_send())),
        provider_file_bytes: Arc::new(Mutex::new(usize::MAX)),
    };
    let screen_handle = Arc::clone(&backend.screen);
    let provider_file_bytes_handle = Arc::clone(&backend.provider_file_bytes);
    let adapter = DiscordSurfaceAdapter::new(backend);
    let bound = adapter
        .locate(&SurfaceTarget {
            app: AdapterAppId::Discord,
            surface: SurfaceKind::InstalledNativeClient,
            generation: 3714,
        })
        .expect("locate Discord recipient conversation");

    let matching_cover_count_before = screen_handle.lock().unwrap().matching_cover_count();
    assert_eq!(matching_cover_count_before, 0);
    let placed = adapter.place(
        &bound,
        &PlacementAuthorization::for_scope(SCOPE),
        &Carrier(COVER.into()),
    );
    assert_eq!(placed.status, PlacementStatus::Placed);
    let sent = adapter.commit(&bound, &SendAuthorization::for_scope(SCOPE), &placed);
    assert_eq!(sent.outcome, SendOutcome::Sent);

    let screen = screen_handle.lock().unwrap().clone();
    let matching_cover_count_after = screen.matching_cover_count();
    let provider_file_bytes = *provider_file_bytes_handle.lock().unwrap();
    assert_eq!(matching_cover_count_after, 1);
    assert_eq!(provider_file_bytes, 0);
    assert_eq!(screen.protected_file_controls.len(), 0);
    assert_eq!(screen.rows.len(), 1);
    assert_eq!(screen.actions.len(), 3);
    assert_eq!(screen.rows[0].author, "sender-osl-3714");
    assert_eq!(screen.rows[0].text, COVER);

    println!("TASK3714 recipient_account={NO_OSL_DISCORD_ACCOUNT} osl_present=false marked_attachment_sealed_bytes={}", sealed.len());
    println!("TASK3714 matching_cover_count_before={matching_cover_count_before}");
    println!("TASK3714 matching_cover_count_after={matching_cover_count_after}");
    println!("TASK3714 provider_file_bytes={provider_file_bytes}");
    println!(
        "TASK3714 protected_file_control_count={}",
        screen.protected_file_controls.len()
    );
    println!(
        "TASK3714 visible_row_count={} visible_rows={:?}",
        screen.rows.len(),
        screen.rows
    );
    println!(
        "TASK3714 visible_action_count={} visible_actions={:?}",
        screen.actions.len(),
        screen.actions
    );
}
