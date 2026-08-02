//! X (`x.com/messages`) implementation of the fixed-origin web adapter.
//!
//! The operating-system accessibility bridge supplies [`XSurfaceDriver`].
//! This module deliberately has no window discovery, activation, or native
//! input calls.  In particular, the write operations below are hooks for the
//! T3-F4 VM-attested input boundary, never direct `SendInput` calls.

use super::WebSurfaceBackend;
use crate::adapters::*;
use sha2::{Digest, Sha256};

/// A conversation header observed by the accessibility driver.  These values
/// are consumed only to create domain-separated digests and never cross the
/// `SurfaceAdapter` ABI as provider text.
#[derive(Clone, Debug, Default)]
pub struct XConversationHeader {
    pub account_label: Option<String>,
    pub conversation_label: Option<String>,
    pub recipient_labels: Vec<String>,
}

/// One visible transcript row.  `carrier` is present only when the driver can
/// prove this row is the carrier that OSL placed or sent.
#[derive(Clone, Debug)]
pub struct XTranscriptRow {
    pub rect: Bounds,
    pub carrier: Option<String>,
}

/// A fresh accessibility snapshot. `scope_binding_hash` is host-derived when
/// the web surface is claimed; it is intentionally not derived from labels in
/// this module.
#[derive(Clone, Debug)]
pub struct XSurfaceSnapshot {
    pub generation: u64,
    pub composer: NodeRef,
    pub transcript: Option<NodeRef>,
    pub bounds: Bounds,
    pub bound_at_ms: u64,
    pub scope_binding_hash: String,
    pub composer_text: String,
    pub composer_is_password_field: bool,
    pub focused: bool,
    pub occluded: bool,
    pub read_was_complete: bool,
    pub header: XConversationHeader,
    pub transcript_epoch: u64,
    pub rows: Vec<XTranscriptRow>,
}

/// The only platform-specific seam used by the X adapter.
///
/// Implementors must obtain an isolated-VM attestation before either write
/// method performs native input.  The backend never synthesizes input itself.
pub trait XSurfaceDriver: Send + Sync {
    fn capabilities(&self) -> CapabilitySet;
    fn is_current_generation(&self, generation: u64) -> bool;
    fn snapshot(&self) -> Result<XSurfaceSnapshot, AdapterRefusal>;

    fn place_with_vm_attestation(&self, _carrier: &str) -> Result<(), AdapterRefusal> {
        Err(AdapterRefusal::PlatformUnsupported)
    }

    fn commit_with_vm_attestation(&self) -> Result<SendOutcome, AdapterRefusal> {
        Err(AdapterRefusal::PlatformUnsupported)
    }
}

/// X-specific backend wired into [`super::WebSurfaceAdapter`].
pub struct XWebBackend<D> {
    driver: D,
}

impl<D> XWebBackend<D> {
    pub fn new(driver: D) -> Self {
        Self { driver }
    }
}

impl<D: XSurfaceDriver> XWebBackend<D> {
    fn snapshot(&self) -> Result<XSurfaceSnapshot, AdapterRefusal> {
        self.driver.snapshot()
    }

    fn destination_from(snapshot: XSurfaceSnapshot) -> DestinationIdentity {
        let header = snapshot.header;
        let complete = nonempty(&header.account_label)
            && nonempty(&header.conversation_label)
            && !header.recipient_labels.is_empty()
            && header
                .recipient_labels
                .iter()
                .all(|value| !value.trim().is_empty());

        if !complete {
            return DestinationIdentity {
                status: DestinationStatus::Unknown,
                account_digest: String::new(),
                conversation_digest: String::new(),
                recipients_digest: String::new(),
                scope_binding_hash: snapshot.scope_binding_hash,
                evidence: BindingEvidence::Accessibility {
                    tree: A11yTree::WebAx,
                },
                attested_at_ms: snapshot.bound_at_ms,
                ttl_ms: 0,
            };
        }

        DestinationIdentity {
            status: DestinationStatus::Attested,
            account_digest: digest(
                "x-account",
                header.account_label.as_deref().unwrap_or_default(),
            ),
            conversation_digest: digest(
                "x-conversation",
                header.conversation_label.as_deref().unwrap_or_default(),
            ),
            recipients_digest: digest("x-recipients", &header.recipient_labels.join("\u{1f}")),
            scope_binding_hash: snapshot.scope_binding_hash,
            evidence: BindingEvidence::Accessibility {
                tree: A11yTree::WebAx,
            },
            attested_at_ms: snapshot.bound_at_ms,
            ttl_ms: 30_000,
        }
    }
}

impl<D: XSurfaceDriver> WebSurfaceBackend for XWebBackend<D> {
    fn capabilities(&self, _: &adapter_profile::ProfilePayload, _: u64) -> CapabilitySet {
        self.driver.capabilities()
    }

    fn is_current_generation(&self, generation: u64) -> bool {
        self.driver.is_current_generation(generation)
    }

    fn locate(
        &self,
        _: &adapter_profile::ProfilePayload,
        target: &SurfaceTarget,
    ) -> Result<SurfaceBinding, AdapterRefusal> {
        let snapshot = self.snapshot()?;
        if snapshot.generation != target.generation {
            return Err(AdapterRefusal::GenerationStale);
        }
        if snapshot.scope_binding_hash.is_empty() {
            return Err(AdapterRefusal::DestinationUnattested);
        }
        Ok(SurfaceBinding::for_claimed_surface(
            AdapterAppId::X,
            snapshot.generation,
            BindingEvidence::Accessibility {
                tree: A11yTree::WebAx,
            },
            snapshot.composer,
            snapshot.transcript,
            snapshot.bounds,
            snapshot.transcript_epoch,
            snapshot.scope_binding_hash,
        ))
    }

    fn read_state(&self, _: &SurfaceBinding) -> Result<SurfaceState, AdapterRefusal> {
        let snapshot = self.snapshot()?;
        Ok(SurfaceState {
            composer_text_sha256: digest("x-composer", &snapshot.composer_text),
            composer_is_empty: snapshot.composer_text.is_empty(),
            composer_is_password_field: snapshot.composer_is_password_field,
            focused: snapshot.focused,
            occluded: snapshot.occluded,
            read_was_complete: snapshot.read_was_complete,
        })
    }

    fn destination(&self, _: &SurfaceBinding) -> Result<DestinationIdentity, AdapterRefusal> {
        Ok(Self::destination_from(self.snapshot()?))
    }

    // T4-P4 replaces these fail-closed stubs with the VM-guarded write path.
    fn place(&self, _: &SurfaceBinding, _: &Carrier) -> PlacementReceipt {
        PlacementReceipt {
            status: PlacementStatus::NotPlaced,
            placed_sha256: None,
            elapsed_ms: 0,
        }
    }

    fn commit(&self, _: &SurfaceBinding, _: &PlacementReceipt) -> SendReceipt {
        SendReceipt {
            outcome: SendOutcome::NotSent,
            elapsed_ms: 0,
        }
    }

    fn paint_targets(&self, _: &SurfaceBinding) -> Result<Vec<PaintTarget>, AdapterRefusal> {
        Ok(Vec::new())
    }
}

fn nonempty(value: &Option<String>) -> bool {
    value
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
}

fn digest(domain: &str, value: &str) -> String {
    format!(
        "{:x}",
        Sha256::digest([domain.as_bytes(), b"\0", value.as_bytes()].concat())
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Fixture(Mutex<XSurfaceSnapshot>);

    impl XSurfaceDriver for Fixture {
        fn capabilities(&self) -> CapabilitySet {
            [adapter_profile::Capability::PlaceProtectedPayload]
                .into_iter()
                .collect()
        }

        fn is_current_generation(&self, generation: u64) -> bool {
            generation == 7
        }

        fn snapshot(&self) -> Result<XSurfaceSnapshot, AdapterRefusal> {
            Ok(self.0.lock().unwrap().clone())
        }
    }

    fn snapshot(scope: &str, conversation: Option<&str>) -> XSurfaceSnapshot {
        XSurfaceSnapshot {
            generation: 7,
            composer: NodeRef::for_claimed_node(1),
            transcript: Some(NodeRef::for_claimed_node(2)),
            bounds: Bounds {
                x: 1,
                y: 2,
                width: 3,
                height: 4,
            },
            bound_at_ms: 10,
            scope_binding_hash: scope.into(),
            composer_text: String::new(),
            composer_is_password_field: false,
            focused: true,
            occluded: false,
            read_was_complete: true,
            header: XConversationHeader {
                account_label: Some("owner".into()),
                conversation_label: conversation.map(str::to_owned),
                recipient_labels: vec!["recipient".into()],
            },
            transcript_epoch: 10,
            rows: Vec::new(),
        }
    }

    #[test]
    fn web_p3_switching_conversations_changes_scope_and_unknown_never_attests() {
        let fixture = Fixture(Mutex::new(snapshot("scope-a", Some("Alice"))));
        let backend = XWebBackend::new(fixture);
        let binding = SurfaceBinding::for_claimed_surface(
            AdapterAppId::X,
            7,
            BindingEvidence::Accessibility {
                tree: A11yTree::WebAx,
            },
            NodeRef::for_claimed_node(1),
            Some(NodeRef::for_claimed_node(2)),
            Bounds {
                x: 1,
                y: 2,
                width: 3,
                height: 4,
            },
            10,
            "scope-a",
        );
        let first = backend.destination(&binding).unwrap();
        assert_eq!(first.status, DestinationStatus::Attested);
        assert_eq!(first.scope_binding_hash, "scope-a");
        assert!(!first.conversation_digest.contains("Alice"));

        *backend.driver.0.lock().unwrap() = snapshot("scope-b", Some("Bob"));
        let second = backend.destination(&binding).unwrap();
        assert_eq!(second.status, DestinationStatus::Attested);
        assert_ne!(first.scope_binding_hash, second.scope_binding_hash);

        *backend.driver.0.lock().unwrap() = snapshot("scope-c", None);
        let unknown = backend.destination(&binding).unwrap();
        assert_eq!(unknown.status, DestinationStatus::Unknown);
        assert!(unknown.conversation_digest.is_empty());
    }
}
