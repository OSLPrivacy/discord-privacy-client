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

/// The minimal, provider-specific result of finding an active X browser.
///
/// This is intentionally distinct from the general accessibility snapshot:
/// callers that need to bind a web surface must first prove that the focused
/// browser is the reviewed X origin, then may use the returned place and
/// composer records to construct an adapter binding.  Browser titles and
/// accessible names are kept only at this driver boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XActiveBrowserSurface {
    pub browser_title: String,
    pub origin: String,
    pub place_kind: String,
    pub composer: String,
}

/// The three records a caller needs after the active X browser is found.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XFoundBrowserPlaceComposer {
    pub browser_title: String,
    pub place_kind: String,
    pub composer: String,
}

/// A named control returned by the focused X browser surface.
///
/// The name is the driver-facing identifier.  The label is only a human
/// readable description of the one control that was found.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XNamedControl {
    pub name: String,
    pub label: String,
}

/// The only platform-specific seam used by the X adapter.
///
/// Implementors must obtain an isolated-VM attestation before either write
/// method performs native input.  The backend never synthesizes input itself.
pub trait XSurfaceDriver: Send + Sync {
    fn capabilities(&self) -> CapabilitySet;
    fn is_current_generation(&self, generation: u64) -> bool;
    fn wake_accessibility(&self) -> Result<(), AdapterRefusal>;
    fn snapshot(&self) -> Result<XSurfaceSnapshot, AdapterRefusal>;

    /// Find the focused browser surface before the normal X selector walk.
    /// Implementations must return only the fixed X messages origin.
    fn active_browser_surface(&self) -> Result<XActiveBrowserSurface, AdapterRefusal> {
        Err(AdapterRefusal::PlatformUnsupported)
    }

    /// Ask the focused X surface for the exact named controls requested by the
    /// caller.  A driver must refuse unknown names rather than treating them as
    /// generic browser controls.
    fn named_controls(
        &self,
        _names: &[&str],
    ) -> Result<Vec<XNamedControl>, AdapterRefusal> {
        Err(AdapterRefusal::PlatformUnsupported)
    }

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

    /// Locate the active X browser and expose its title, classified place, and
    /// composer name.  This is deliberately read-only; placement remains at
    /// the VM-attested method on [`XSurfaceDriver`].
    pub fn find_active_browser_place_and_composer(
        &self,
    ) -> Result<XFoundBrowserPlaceComposer, AdapterRefusal> {
        let surface = self.driver.active_browser_surface()?;
        if surface.origin != "https://x.com/messages"
            || !matches!(surface.place_kind.as_str(), "direct_message" | "public_post")
            || surface.browser_title.trim().is_empty()
            || surface.composer.trim().is_empty()
        {
            return Err(AdapterRefusal::WindowGone);
        }
        Ok(XFoundBrowserPlaceComposer {
            browser_title: surface.browser_title,
            place_kind: surface.place_kind,
            composer: surface.composer,
        })
    }

    /// Read only the explicitly requested controls from the X driver.
    ///
    /// The backend rejects a driver result whose identifier was not requested,
    /// so a story-only control cannot be substituted for a reviewed composer.
    pub fn find_named_controls(
        &self,
        names: &[&str],
    ) -> Result<Vec<XNamedControl>, AdapterRefusal> {
        if names.is_empty() || names.iter().any(|name| name.trim().is_empty()) {
            return Err(AdapterRefusal::WindowGone);
        }
        let controls = self.driver.named_controls(names)?;
        if controls.is_empty()
            || controls.iter().any(|control| {
                control.name.trim().is_empty()
                    || control.label.trim().is_empty()
                    || !names.iter().any(|name| *name == control.name)
            })
        {
            return Err(AdapterRefusal::WindowGone);
        }
        Ok(controls)
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

    fn wake_accessibility(&self) -> Result<(), AdapterRefusal> {
        self.driver.wake_accessibility()
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

    fn place(&self, _: &SurfaceBinding, carrier: &Carrier) -> PlacementReceipt {
        let before = match self.snapshot() {
            Ok(snapshot) => snapshot,
            Err(_) => return not_placed(),
        };
        let expected = digest("x-carrier", &carrier.0);
        if self.driver.place_with_vm_attestation(&carrier.0).is_err() {
            return not_placed();
        }
        let after = match self.snapshot() {
            Ok(snapshot) => snapshot,
            Err(_) => return not_placed(),
        };

        // Placement is an edit only.  A newly visible row means the platform
        // submitted while placing, so do not issue a successful receipt.
        if after.rows.len() != before.rows.len()
            || after.generation != before.generation
            || after.composer_text != carrier.0
        {
            return not_placed();
        }
        PlacementReceipt {
            status: PlacementStatus::Placed,
            placed_sha256: Some(expected),
            elapsed_ms: 0,
        }
    }

    fn commit(&self, _: &SurfaceBinding, placed: &PlacementReceipt) -> SendReceipt {
        if placed.status != PlacementStatus::Placed || placed.placed_sha256.is_none() {
            return not_sent();
        }
        let before = match self.snapshot() {
            Ok(snapshot) => snapshot,
            Err(_) => return not_sent(),
        };
        let outcome = match self.driver.commit_with_vm_attestation() {
            Ok(outcome) => outcome,
            Err(_) => return not_sent(),
        };
        if outcome != SendOutcome::Sent {
            return SendReceipt {
                outcome,
                elapsed_ms: 0,
            };
        }
        let after = match self.snapshot() {
            Ok(snapshot) => snapshot,
            Err(_) => {
                return SendReceipt {
                    outcome: SendOutcome::Unknown,
                    elapsed_ms: 0,
                }
            }
        };
        // The send command is one-shot: any ambiguity is reported as Unknown
        // and is never retried by this adapter.
        let outcome = if after.rows.len() == before.rows.len() + 1 {
            SendOutcome::Sent
        } else {
            SendOutcome::Unknown
        };
        SendReceipt {
            outcome,
            elapsed_ms: 0,
        }
    }

    fn paint_targets(&self, binding: &SurfaceBinding) -> Result<Vec<PaintTarget>, AdapterRefusal> {
        let snapshot = self.snapshot()?;
        // Virtualised X rows are recycled when scrolled.  Returning an old
        // rectangle would paint plaintext over a different message, so a
        // changed transcript epoch invalidates the request outright.
        if snapshot.transcript_epoch != binding.bound_at_ms {
            return Err(AdapterRefusal::ReadIncomplete);
        }
        Ok(snapshot
            .rows
            .into_iter()
            .filter_map(|row| {
                let carrier = row.carrier?;
                if carrier.is_empty() {
                    return None;
                }
                Some(PaintTarget {
                    carrier_sha256: digest("x-carrier", &carrier),
                    rect: row.rect,
                    clipped_by: None,
                    confidence: PaintConfidence::Exact,
                })
            })
            .collect())
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

        fn wake_accessibility(&self) -> Result<(), AdapterRefusal> {
            Ok(())
        }

        fn snapshot(&self) -> Result<XSurfaceSnapshot, AdapterRefusal> {
            Ok(self.0.lock().unwrap().clone())
        }

        fn place_with_vm_attestation(&self, carrier: &str) -> Result<(), AdapterRefusal> {
            // The fixture models the T3-F4-owned, VM-attested write boundary.
            // It deliberately changes only the composer; no Enter/submit occurs.
            self.0.lock().unwrap().composer_text = carrier.to_owned();
            Ok(())
        }

        fn commit_with_vm_attestation(&self) -> Result<SendOutcome, AdapterRefusal> {
            let mut snapshot = self.0.lock().unwrap();
            let carrier = std::mem::take(&mut snapshot.composer_text);
            let rect = Bounds {
                x: 1,
                y: 20,
                width: 30,
                height: 10,
            };
            snapshot.rows.push(XTranscriptRow {
                rect,
                carrier: Some(carrier),
            });
            Ok(SendOutcome::Sent)
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

    #[test]
    fn web_p4_place_never_sends_and_commit_adds_exactly_one_row() {
        let backend = XWebBackend::new(Fixture(Mutex::new(snapshot("scope-a", Some("Alice")))));
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
        let carrier = Carrier("osl1_carrier".into());
        let rows_before = backend.snapshot().unwrap().rows.len();
        let placed = backend.place(&binding, &carrier);
        let after_place = backend.snapshot().unwrap();
        assert_eq!(placed.status, PlacementStatus::Placed);
        assert_eq!(placed.placed_sha256, Some(digest("x-carrier", &carrier.0)));
        assert_eq!(after_place.rows.len(), rows_before);
        assert_eq!(after_place.composer_text, carrier.0);

        let sent = backend.commit(&binding, &placed);
        assert_eq!(sent.outcome, SendOutcome::Sent);
        assert_eq!(backend.snapshot().unwrap().rows.len(), rows_before + 1);
    }

    #[test]
    fn web_p5_exact_targets_have_carrier_digests_and_scroll_invalidates_them() {
        let mut initial = snapshot("scope-a", Some("Alice"));
        initial.rows.push(XTranscriptRow {
            rect: Bounds {
                x: 1,
                y: 20,
                width: 30,
                height: 10,
            },
            carrier: Some("osl1_carrier".into()),
        });
        let backend = XWebBackend::new(Fixture(Mutex::new(initial)));
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
        let targets = backend.paint_targets(&binding).unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].confidence, PaintConfidence::Exact);
        assert_eq!(
            targets[0].carrier_sha256,
            digest("x-carrier", "osl1_carrier")
        );

        // A scroll may recycle the same accessibility node at a new position;
        // it must invalidate instead of moving the old target.
        backend.driver.0.lock().unwrap().transcript_epoch = 11;
        assert_eq!(
            backend.paint_targets(&binding),
            Err(AdapterRefusal::ReadIncomplete)
        );
    }
}
