//! Live accessibility-node source for the pure Signal selectors.
//!
//! This boundary owns only the conversion from an already-claimed Signal UIA
//! tree into `SignalNode`s.  It deliberately does not discover windows, read
//! accessible names or text, place input, or attest a destination.

use super::*;
use crate::native_signal_adapter::{
    discover_signal_composer, discover_signal_transcript, SignalNode, SignalRect, SignalRole,
    SignalSelectorError,
};
use crate::signal_destination_binding::{
    SignalBindingStatus, SignalDestinationBindingState, SignalDestinationEvidence,
};
use std::sync::Arc;

const MAX_SIGNAL_A11Y_NODES: usize = 4_096;
const MAX_SIGNAL_A11Y_DEPTH: usize = 64;

/// The L2-only operations supplied by Signal's native accessibility bridge.
///
/// Deliberately omits any send/submit operation: Signal placement writes a
/// draft, while committing it is reserved for the later L3 task.
pub trait SignalBackend: Send + Sync {
    fn capabilities(&self, now_unix_seconds: u64) -> CapabilitySet;
    fn locate(&self, target: &SurfaceTarget) -> Result<SurfaceBinding, AdapterRefusal>;
    fn read_state(&self, binding: &SurfaceBinding) -> Result<SurfaceState, AdapterRefusal>;
    /// Fresh native accessibility evidence for the exact currently selected
    /// Signal account, conversation, recipients, and composer.
    fn destination_evidence(
        &self,
        binding: &SurfaceBinding,
    ) -> Result<SignalDestinationEvidence, AdapterRefusal>;
    fn place_without_submit(&self, binding: &SurfaceBinding, carrier: &Carrier)
        -> PlacementReceipt;
}

/// Signal's native surface adapter through L2 placement.
pub struct SignalSurfaceAdapter<B> {
    backend: B,
    destination_binding: Arc<SignalDestinationBindingState>,
}

impl<B> SignalSurfaceAdapter<B> {
    pub fn new(backend: B) -> Self {
        Self::with_destination_binding(backend, Arc::new(SignalDestinationBindingState::default()))
    }

    pub fn with_destination_binding(
        backend: B,
        destination_binding: Arc<SignalDestinationBindingState>,
    ) -> Self {
        Self {
            backend,
            destination_binding,
        }
    }
}

impl<B: SignalBackend> SignalSurfaceAdapter<B> {
    fn supports(&self, capability: adapter_profile::Capability) -> bool {
        self.backend.capabilities(u64::MAX).contains(&capability)
    }

    fn validates_binding(&self, binding: &SurfaceBinding) -> bool {
        binding.app == AdapterAppId::Signal && binding.generation != 0
    }
}

impl<B: SignalBackend> SurfaceAdapter for SignalSurfaceAdapter<B> {
    fn abi_version(&self) -> u32 {
        ADAPTER_ABI_VERSION
    }

    fn app(&self) -> AdapterAppId {
        AdapterAppId::Signal
    }

    fn surface(&self) -> SurfaceKind {
        SurfaceKind::InstalledNativeClient
    }

    fn capabilities(&self, now_unix_seconds: u64) -> CapabilitySet {
        self.backend.capabilities(now_unix_seconds)
    }

    fn locate(&self, target: &SurfaceTarget) -> Result<SurfaceBinding, AdapterRefusal> {
        if target.app != self.app() || target.surface != self.surface() || target.generation == 0 {
            return Err(AdapterRefusal::WindowGone);
        }
        let binding = self.backend.locate(target)?;
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
        if !self.validates_binding(binding)
            || !matches!(
                binding.evidence,
                BindingEvidence::Accessibility {
                    tree: A11yTree::Uia | A11yTree::Msaa | A11yTree::Both
                }
            )
        {
            return Err(AdapterRefusal::DestinationUnattested);
        }
        let evidence = self.backend.destination_evidence(binding)?;
        let receipt = self
            .destination_binding
            .attest_native_observation(evidence.clone());
        if receipt.status != SignalBindingStatus::Accepted {
            return Err(AdapterRefusal::DestinationUnattested);
        }
        Ok(DestinationIdentity {
            status: DestinationStatus::Attested,
            account_digest: hex_digest(evidence.account_binding_sha256),
            conversation_digest: hex_digest(evidence.conversation_binding_sha256),
            recipients_digest: hex_digest(evidence.participant_set_sha256),
            scope_binding_hash: binding.scope_binding_hash.clone(),
            evidence: binding.evidence.clone(),
            attested_at_ms: evidence.observed_at_ms,
            ttl_ms: receipt.valid_for_ms,
        })
    }

    fn place(
        &self,
        binding: &SurfaceBinding,
        authorization: &PlacementAuthorization,
        carrier: &Carrier,
    ) -> PlacementReceipt {
        let refused = || PlacementReceipt {
            status: PlacementStatus::NotPlaced,
            placed_sha256: None,
            elapsed_ms: 0,
        };
        if !self.validates_binding(binding)
            || !same_scope(
                &binding.scope_binding_hash,
                &authorization.scope_binding_hash,
            )
            || !self.supports(adapter_profile::Capability::PlaceProtectedPayload)
        {
            return refused();
        }
        match self.read_state(binding) {
            Ok(state)
                if state.composer_is_empty
                    && !state.composer_is_password_field
                    && state.focused
                    && !state.occluded =>
            {
                self.backend.place_without_submit(binding, carrier)
            }
            _ => refused(),
        }
    }

    fn commit(
        &self,
        _: &SurfaceBinding,
        _: &SendAuthorization,
        _: &PlacementReceipt,
    ) -> SendReceipt {
        SendReceipt {
            outcome: SendOutcome::NotSent,
            elapsed_ms: 0,
        }
    }

    fn paint_targets(&self, _: &SurfaceBinding) -> Result<Vec<PaintTarget>, AdapterRefusal> {
        Err(AdapterRefusal::AccessibilityUnavailable)
    }
}

fn hex_digest(digest: [u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        encoded.push(HEX[usize::from(byte >> 4)] as char);
        encoded.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    encoded
}

/// The result of one bounded read of the exact Signal accessibility root.
#[derive(Clone)]
pub struct SignalNodeSnapshot {
    pub nodes: Vec<SignalNode>,
    pub window_bounds: SignalRect,
}

impl SignalNodeSnapshot {
    /// Feed the live node source to the selector layer without adding any
    /// Signal-specific selection policy at this boundary.
    pub fn locate(&self) -> Result<SignalLocatedNodes, AdapterRefusal> {
        let composer =
            discover_signal_composer(&self.nodes, self.window_bounds).map_err(composer_refusal)?;
        let transcript = discover_signal_transcript(&self.nodes, composer, self.window_bounds)
            .map_err(transcript_refusal)?;
        Ok(SignalLocatedNodes {
            composer,
            transcript,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SignalLocatedNodes {
    pub composer: usize,
    pub transcript: usize,
}

fn composer_refusal(error: SignalSelectorError) -> AdapterRefusal {
    match error {
        SignalSelectorError::Missing => AdapterRefusal::ComposerNotFound,
        SignalSelectorError::Ambiguous => AdapterRefusal::ComposerAmbiguous,
        SignalSelectorError::Invalid
        | SignalSelectorError::LimitExceeded
        | SignalSelectorError::ProofMismatch => AdapterRefusal::AccessibilityUnavailable,
    }
}

fn transcript_refusal(error: SignalSelectorError) -> AdapterRefusal {
    match error {
        SignalSelectorError::Missing => AdapterRefusal::TranscriptNotFound,
        SignalSelectorError::Ambiguous
        | SignalSelectorError::Invalid
        | SignalSelectorError::LimitExceeded
        | SignalSelectorError::ProofMismatch => AdapterRefusal::AccessibilityUnavailable,
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn snapshot_claimed_signal_nodes(
    target: crate::native_window_host::NativeDiscordAccessibilityTarget,
    process_is_trusted: &dyn Fn(u32) -> bool,
) -> Result<SignalNodeSnapshot, AdapterRefusal> {
    windows::snapshot_claimed_signal_nodes(target, process_is_trusted)
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use ::windows::Win32::Foundation::HWND;
    use ::windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    };
    use ::windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTreeWalker,
        IUIAutomationValuePattern, UIA_ButtonControlTypeId, UIA_EditControlTypeId,
        UIA_CONTROLTYPE_ID, UIA_ListControlTypeId, UIA_PaneControlTypeId,
        UIA_TextControlTypeId, UIA_ValuePatternId, UIA_WindowControlTypeId,
    };

    struct ComGuard(bool);

    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    pub(super) fn snapshot_claimed_signal_nodes(
        target: crate::native_window_host::NativeDiscordAccessibilityTarget,
        process_is_trusted: &dyn Fn(u32) -> bool,
    ) -> Result<SignalNodeSnapshot, AdapterRefusal> {
        let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let _com = ComGuard(initialized.is_ok());
        let automation: IUIAutomation =
            unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }
                .map_err(|_| AdapterRefusal::AccessibilityUnavailable)?;
        let root = unsafe { automation.ElementFromHandle(HWND(target.window as _)) }
            .map_err(|_| AdapterRefusal::AccessibilityUnavailable)?;
        let root_node = project_node(&root, target.process_id, process_is_trusted)?;
        let window_bounds = root_node.bounds;
        if !window_bounds.valid() {
            return Err(AdapterRefusal::AccessibilityUnavailable);
        }
        let walker = unsafe { automation.RawViewWalker() }
            .map_err(|_| AdapterRefusal::AccessibilityUnavailable)?;
        let mut nodes = vec![root_node];
        let mut stack = vec![(0usize, root, 0usize)];
        while let Some((parent_index, parent, depth)) = stack.pop() {
            if depth >= MAX_SIGNAL_A11Y_DEPTH {
                return Err(AdapterRefusal::AccessibilityUnavailable);
            }
            let children = child_elements(&walker, &parent)?;
            for child in children.into_iter().rev() {
                if nodes.len() >= MAX_SIGNAL_A11Y_NODES {
                    return Err(AdapterRefusal::AccessibilityUnavailable);
                }
                let child_node = project_node(&child, target.process_id, process_is_trusted)?;
                let child_index = nodes.len();
                nodes.push(child_node);
                nodes[parent_index].children.push(child_index);
                stack.push((child_index, child, depth + 1));
            }
        }
        Ok(SignalNodeSnapshot {
            nodes,
            window_bounds,
        })
    }

    fn child_elements(
        walker: &IUIAutomationTreeWalker,
        parent: &IUIAutomationElement,
    ) -> Result<Vec<IUIAutomationElement>, AdapterRefusal> {
        let mut children = Vec::new();
        // UI Automation denotes the end of a sibling list with a successful
        // call and a null COM pointer. `windows` 0.56 projects that as an
        // empty (S_OK) error instead of `Option<IUIAutomationElement>`.
        let mut current = match unsafe { walker.GetFirstChildElement(parent) } {
            Ok(element) => Some(element),
            Err(error) if error.code().0 == 0 => None,
            Err(_) => return Err(AdapterRefusal::AccessibilityUnavailable),
        };
        while let Some(element) = current {
            if children.len() >= MAX_SIGNAL_A11Y_NODES {
                return Err(AdapterRefusal::AccessibilityUnavailable);
            }
            current = match unsafe { walker.GetNextSiblingElement(&element) } {
                Ok(next) => Some(next),
                Err(error) if error.code().0 == 0 => None,
                Err(_) => return Err(AdapterRefusal::AccessibilityUnavailable),
            };
            children.push(element);
        }
        Ok(children)
    }

    fn project_node(
        element: &IUIAutomationElement,
        expected_process_id: u32,
        process_is_trusted: &dyn Fn(u32) -> bool,
    ) -> Result<SignalNode, AdapterRefusal> {
        let process_id = unsafe { element.CurrentProcessId() }
            .map_err(|_| AdapterRefusal::AccessibilityUnavailable)?;
        if process_id <= 0
            || process_id as u32 != expected_process_id
            || !process_is_trusted(process_id as u32)
        {
            return Err(AdapterRefusal::AccessibilityUnavailable);
        }
        let rect = unsafe { element.CurrentBoundingRectangle() }
            .map_err(|_| AdapterRefusal::AccessibilityUnavailable)?;
        let control_type = unsafe { element.CurrentControlType() }
            .map_err(|_| AdapterRefusal::AccessibilityUnavailable)?;
        let mut node = SignalNode::structural(
            role_from_control_type(control_type),
            SignalRect {
                left: rect.left,
                top: rect.top,
                right: rect.right,
                bottom: rect.bottom,
            },
        );
        node.visible = !unsafe { element.CurrentIsOffscreen() }
            .map_err(|_| AdapterRefusal::AccessibilityUnavailable)?
            .as_bool();
        node.enabled = unsafe { element.CurrentIsEnabled() }
            .map_err(|_| AdapterRefusal::AccessibilityUnavailable)?
            .as_bool();
        node.focusable = unsafe { element.CurrentIsKeyboardFocusable() }
            .map_err(|_| AdapterRefusal::AccessibilityUnavailable)?
            .as_bool();
        node.editable = control_type == UIA_EditControlTypeId;
        node.read_only = if node.editable {
            let value_pattern = unsafe {
                element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
            }
            .map_err(|_| AdapterRefusal::AccessibilityUnavailable)?;
            unsafe { value_pattern.CurrentIsReadOnly() }
                .map_err(|_| AdapterRefusal::AccessibilityUnavailable)?
                .as_bool()
        } else {
            true
        };
        Ok(node)
    }

    fn role_from_control_type(control_type: UIA_CONTROLTYPE_ID) -> SignalRole {
        match control_type {
            value if value == UIA_WindowControlTypeId => SignalRole::Window,
            value if value == UIA_PaneControlTypeId => SignalRole::Pane,
            value if value == UIA_ListControlTypeId => SignalRole::List,
            value if value == UIA_TextControlTypeId => SignalRole::Text,
            value if value == UIA_EditControlTypeId => SignalRole::EditableText,
            value if value == UIA_ButtonControlTypeId => SignalRole::Button,
            _ => SignalRole::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_signal_adapter::SignalNode;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    };

    struct PlacementBackend {
        placements: AtomicUsize,
    }

    fn binding(generation: u64) -> SurfaceBinding {
        SurfaceBinding {
            app: AdapterAppId::Signal,
            generation,
            evidence: BindingEvidence::Accessibility {
                tree: A11yTree::Uia,
            },
            composer: NodeRef(4),
            transcript: Some(NodeRef(8)),
            bounds: Bounds {
                x: 0,
                y: 0,
                width: 1200,
                height: 900,
            },
            bound_at_ms: 1,
            scope_binding_hash: "scope-a".into(),
        }
    }

    impl SignalBackend for PlacementBackend {
        fn capabilities(&self, _: u64) -> CapabilitySet {
            [
                adapter_profile::Capability::InspectVisibleComposer,
                adapter_profile::Capability::InspectVisibleTranscript,
                adapter_profile::Capability::PlaceProtectedPayload,
            ]
            .into_iter()
            .collect()
        }

        fn locate(&self, target: &SurfaceTarget) -> Result<SurfaceBinding, AdapterRefusal> {
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

        fn destination_evidence(
            &self,
            binding: &SurfaceBinding,
        ) -> Result<SignalDestinationEvidence, AdapterRefusal> {
            Ok(destination_evidence(binding.generation, 3))
        }

        fn place_without_submit(&self, _: &SurfaceBinding, _: &Carrier) -> PlacementReceipt {
            self.placements.fetch_add(1, Ordering::SeqCst);
            PlacementReceipt {
                status: PlacementStatus::Placed,
                placed_sha256: Some("carrier-digest".into()),
                elapsed_ms: 1,
            }
        }
    }

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> SignalRect {
        SignalRect {
            left,
            top,
            right,
            bottom,
        }
    }

    fn editable(bounds: SignalRect) -> SignalNode {
        let mut node = SignalNode::structural(SignalRole::EditableText, bounds);
        node.focusable = true;
        node.editable = true;
        node.read_only = false;
        node
    }

    fn destination_evidence(generation: u64, conversation: u8) -> SignalDestinationEvidence {
        SignalDestinationEvidence {
            host_generation: generation,
            window_identity_sha256: [1; 32],
            account_binding_sha256: [2; 32],
            conversation_binding_sha256: [conversation; 32],
            participant_set_sha256: [4; 32],
            composer_identity_sha256: [5; 32],
            attestation_nonce_sha256: [conversation.saturating_add(10); 32],
            observed_at_ms: crate::signal_destination_binding::monotonic_now_ms(),
            window_foreground: true,
            composer_focused: true,
            conversation_stable: true,
        }
    }

    struct AttestationBackend {
        conversation: Mutex<u8>,
    }

    impl SignalBackend for AttestationBackend {
        fn capabilities(&self, _: u64) -> CapabilitySet {
            CapabilitySet::new()
        }

        fn locate(&self, target: &SurfaceTarget) -> Result<SurfaceBinding, AdapterRefusal> {
            Ok(binding(target.generation))
        }

        fn read_state(&self, _: &SurfaceBinding) -> Result<SurfaceState, AdapterRefusal> {
            unreachable!("destination attestation does not read the composer state")
        }

        fn destination_evidence(
            &self,
            binding: &SurfaceBinding,
        ) -> Result<SignalDestinationEvidence, AdapterRefusal> {
            Ok(destination_evidence(
                binding.generation,
                *self.conversation.lock().unwrap(),
            ))
        }

        fn place_without_submit(&self, _: &SurfaceBinding, _: &Carrier) -> PlacementReceipt {
            unreachable!("destination attestation never places a carrier")
        }
    }

    #[test]
    fn t3_t14_live_nodes_feed_the_existing_signal_selectors() {
        let window_bounds = rect(0, 0, 1200, 900);
        let snapshot = SignalNodeSnapshot {
            nodes: vec![
                SignalNode::structural(SignalRole::Window, window_bounds),
                SignalNode::structural(SignalRole::List, rect(430, 90, 1130, 710)),
                editable(rect(455, 735, 1125, 820)),
            ],
            window_bounds,
        };

        assert_eq!(
            snapshot.locate(),
            Ok(SignalLocatedNodes {
                composer: 2,
                transcript: 1,
            })
        );
    }

    #[test]
    fn t3_t14_empty_live_tree_refuses_instead_of_locating_a_composer() {
        let snapshot = SignalNodeSnapshot {
            nodes: Vec::new(),
            window_bounds: rect(0, 0, 1200, 900),
        };

        assert_eq!(snapshot.locate(), Err(AdapterRefusal::ComposerNotFound));
    }

    #[test]
    fn t3_t15_places_only_a_focused_empty_draft_and_never_submits() {
        let adapter = SignalSurfaceAdapter::new(PlacementBackend {
            placements: AtomicUsize::new(0),
        });
        let binding = binding(1);

        let placed = adapter.place(
            &binding,
            &PlacementAuthorization::for_scope("scope-a"),
            &Carrier("carrier".into()),
        );
        assert_eq!(placed.status, PlacementStatus::Placed);
        assert_eq!(placed.placed_sha256.as_deref(), Some("carrier-digest"));
        assert_eq!(adapter.backend.placements.load(Ordering::SeqCst), 1);

        assert_eq!(
            adapter
                .commit(&binding, &SendAuthorization::for_scope("scope-a"), &placed)
                .outcome,
            SendOutcome::NotSent
        );

        let refused = adapter.place(
            &binding,
            &PlacementAuthorization::for_scope("other-scope"),
            &Carrier("carrier".into()),
        );
        assert_eq!(refused.status, PlacementStatus::NotPlaced);
        assert_eq!(adapter.backend.placements.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn t3_t16_conversation_change_invalidates_prior_signal_attestation() {
        let state = Arc::new(SignalDestinationBindingState::default());
        let adapter = SignalSurfaceAdapter::with_destination_binding(
            AttestationBackend {
                conversation: Mutex::new(3),
            },
            Arc::clone(&state),
        );
        let binding = binding(7);

        let first = adapter.destination(&binding).unwrap();
        assert_eq!(first.status, DestinationStatus::Attested);
        let first_readiness = state.readiness();
        assert_eq!(first_readiness.status, SignalBindingStatus::Accepted);

        *adapter.backend.conversation.lock().unwrap() = 8;
        let second = adapter.destination(&binding).unwrap();

        assert_eq!(second.status, DestinationStatus::Attested);
        assert_ne!(first.conversation_digest, second.conversation_digest);
        let second_readiness = state.readiness();
        assert_eq!(second_readiness.status, SignalBindingStatus::Accepted);
        assert!(second_readiness.lifecycle_generation > first_readiness.lifecycle_generation);
    }
}
