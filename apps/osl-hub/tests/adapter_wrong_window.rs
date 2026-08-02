//! T3-T18: a carrier authorized for one claimed conversation must never be
//! committed after the host has bound the adapter to another one.

use osl_privacy_hub::adapters::{
    discord::{DiscordBackend, DiscordSurfaceAdapter},
    telegram::{TelegramBackend, TelegramDestinationEvidence, TelegramSurfaceAdapter},
    A11yTree, AdapterAppId, AdapterRefusal, BindingEvidence, Bounds, CapabilitySet, Carrier,
    DestinationIdentity, DestinationStatus, NodeRef, PlacementAuthorization, PlacementReceipt,
    PlacementStatus, SendAuthorization, SendOutcome, SendReceipt, SurfaceAdapter, SurfaceBinding,
    SurfaceKind, SurfaceState, SurfaceTarget,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

fn binding(app: AdapterAppId, scope: &str) -> SurfaceBinding {
    SurfaceBinding::for_claimed_surface(
        app,
        7,
        BindingEvidence::Accessibility {
            tree: A11yTree::Uia,
        },
        NodeRef::for_claimed_node(4),
        Some(NodeRef::for_claimed_node(8)),
        Bounds {
            x: 0,
            y: 0,
            width: 1200,
            height: 900,
        },
        1,
        scope,
    )
}

fn state() -> SurfaceState {
    SurfaceState {
        composer_text_sha256: "carrier-before".into(),
        composer_is_empty: true,
        composer_is_password_field: false,
        focused: true,
        occluded: false,
        read_was_complete: true,
    }
}

struct DiscordWrongWindowBackend {
    commits: Arc<AtomicUsize>,
}
impl DiscordBackend for DiscordWrongWindowBackend {
    fn capabilities(&self, _: u64) -> CapabilitySet {
        [
            adapter_profile::Capability::PlaceProtectedPayload,
            adapter_profile::Capability::SendProtectedPayload,
        ]
        .into_iter()
        .collect()
    }
    fn locate(&self, target: &SurfaceTarget) -> Result<SurfaceBinding, AdapterRefusal> {
        Ok(binding(
            AdapterAppId::Discord,
            if target.generation == 7 {
                "conversation-a"
            } else {
                "conversation-b"
            },
        ))
    }
    fn read_state(&self, _: &SurfaceBinding) -> Result<SurfaceState, AdapterRefusal> {
        Ok(state())
    }
    fn destination(&self, b: &SurfaceBinding) -> Result<DestinationIdentity, AdapterRefusal> {
        Ok(DestinationIdentity {
            status: DestinationStatus::Attested,
            account_digest: "account".into(),
            conversation_digest: "conversation".into(),
            recipients_digest: "recipients".into(),
            scope_binding_hash: "conversation-a".into(),
            evidence: b.evidence.clone(),
            attested_at_ms: 1,
            ttl_ms: 1,
        })
    }
    fn place(&self, _: &SurfaceBinding, _: &Carrier) -> PlacementReceipt {
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
    fn paint_targets(
        &self,
        _: &SurfaceBinding,
    ) -> Result<Vec<osl_privacy_hub::adapters::PaintTarget>, AdapterRefusal> {
        Ok(vec![])
    }
}

struct TelegramWrongWindowBackend {
    commits: Arc<AtomicUsize>,
}
impl TelegramBackend for TelegramWrongWindowBackend {
    fn capabilities(&self, _: u64) -> CapabilitySet {
        [
            adapter_profile::Capability::PlaceProtectedPayload,
            adapter_profile::Capability::SendProtectedPayload,
        ]
        .into_iter()
        .collect()
    }
    fn locate(&self, target: &SurfaceTarget) -> Result<SurfaceBinding, AdapterRefusal> {
        Ok(binding(
            AdapterAppId::Telegram,
            if target.generation == 7 {
                "conversation-a"
            } else {
                "conversation-b"
            },
        ))
    }
    fn read_state(&self, _: &SurfaceBinding) -> Result<SurfaceState, AdapterRefusal> {
        Ok(state())
    }
    fn destination_evidence(
        &self,
        _: &SurfaceBinding,
    ) -> Result<TelegramDestinationEvidence, AdapterRefusal> {
        Ok(TelegramDestinationEvidence {
            window_identity_sha256: [1; 32],
            account_binding_sha256: [2; 32],
            conversation_binding_sha256: [3; 32],
            participant_set_sha256: [4; 32],
            composer_identity_sha256: [5; 32],
            attestation_nonce_sha256: [6; 32],
            observed_at_ms: 1,
            window_foreground: true,
            composer_focused: true,
            conversation_stable: true,
        })
    }
    fn place(&self, _: &SurfaceBinding, _: &Carrier) -> PlacementReceipt {
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
    fn paint_targets(
        &self,
        _: &SurfaceBinding,
    ) -> Result<Vec<osl_privacy_hub::adapters::PaintTarget>, AdapterRefusal> {
        Ok(vec![])
    }
}

fn assert_wrong_window_refused(
    adapter: &impl SurfaceAdapter,
    first: SurfaceBinding,
    swapped: SurfaceBinding,
) {
    let placed = adapter.place(
        &first,
        &PlacementAuthorization::for_scope("conversation-a"),
        &Carrier("carrier".into()),
    );
    assert_eq!(placed.status, PlacementStatus::Placed);
    assert_eq!(
        adapter
            .commit(
                &swapped,
                &SendAuthorization::for_scope("conversation-a"),
                &placed
            )
            .outcome,
        SendOutcome::NotSent
    );
}

#[test]
fn t3_t18_wrong_window_plaintext_is_refused_for_each_sending_native_adapter() {
    let discord_commits = Arc::new(AtomicUsize::new(0));
    let discord = DiscordSurfaceAdapter::new(DiscordWrongWindowBackend {
        commits: Arc::clone(&discord_commits),
    });
    assert_wrong_window_refused(
        &discord,
        binding(AdapterAppId::Discord, "conversation-a"),
        binding(AdapterAppId::Discord, "conversation-b"),
    );
    assert_eq!(discord_commits.load(Ordering::SeqCst), 0);

    let telegram_commits = Arc::new(AtomicUsize::new(0));
    let telegram = TelegramSurfaceAdapter::new(TelegramWrongWindowBackend {
        commits: Arc::clone(&telegram_commits),
    });
    assert_wrong_window_refused(
        &telegram,
        binding(AdapterAppId::Telegram, "conversation-a"),
        binding(AdapterAppId::Telegram, "conversation-b"),
    );
    assert_eq!(telegram_commits.load(Ordering::SeqCst), 0);
}
