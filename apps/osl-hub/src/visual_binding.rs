//! Provider-neutral user-confirmed visual destination binding.
//!
//! This is the final binding tier, for adapters that have already exhausted
//! accessibility and Win32 structural evidence. It never reads provider text:
//! callers provide only opaque SHA-256 commitments and native-window facts.

use crate::adapters::{
    AdapterAppId, AdapterRefusal, BindingEvidence, DestinationIdentity, DestinationStatus,
};
use sha2::{Digest, Sha256};

pub const DEFAULT_VISUAL_BINDING_TTL_MS: u64 = 2_000;
pub const MAX_VISUAL_BINDING_TTL_MS: u64 = 5_000;

/// Opaque, freshly observed facts about the surface the user confirmed.
///
/// `context_binding_sha256` is the adapter's content commitment. It must be
/// re-observed immediately before and after every protected operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VisualBindingObservation {
    pub native_window_id: u64,
    pub host_generation: u64,
    pub window_rect: [i32; 4],
    pub composer_binding_sha256: String,
    pub context_binding_sha256: String,
    pub foreground: bool,
    pub minimized: bool,
    pub occluded: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VisualBindingTarget {
    pub app: AdapterAppId,
    /// Host-derived scope binding; it is never provider identity text.
    pub scope_binding_hash: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VisualBindingTransition {
    FocusLost,
    WindowMovedOrResized,
    Minimized,
    Occluded,
    ConversationChanged,
    ComposerChanged,
}

struct PendingBinding {
    ceremony_id: String,
    observation: VisualBindingObservation,
}

struct VerifiedBinding {
    ceremony_id: String,
    confirmed_at_ms: u64,
    expires_at_ms: u64,
    observation: VisualBindingObservation,
    target: VisualBindingTarget,
}

/// A memory-only binding. Recreating it after a process restart requires a
/// new user confirmation, rather than silently restoring authority.
pub struct VisualBindingGuard {
    ttl_ms: u64,
    pending: Option<PendingBinding>,
    binding: Option<VerifiedBinding>,
}

impl Default for VisualBindingGuard {
    fn default() -> Self {
        Self::new(DEFAULT_VISUAL_BINDING_TTL_MS).expect("the built-in visual binding TTL is valid")
    }
}

impl VisualBindingGuard {
    pub fn new(ttl_ms: u64) -> Result<Self, AdapterRefusal> {
        if ttl_ms == 0 || ttl_ms > MAX_VISUAL_BINDING_TTL_MS {
            return Err(AdapterRefusal::DestinationUnattested);
        }
        Ok(Self {
            ttl_ms,
            pending: None,
            binding: None,
        })
    }

    /// Starts an explicit, UI-recorded confirmation ceremony. The ceremony id
    /// is an opaque gesture id minted by trusted UI code, never provider text.
    pub fn begin(
        &mut self,
        ceremony_id: String,
        observation: VisualBindingObservation,
    ) -> Result<(), AdapterRefusal> {
        self.clear();
        if !valid_ceremony_id(&ceremony_id) || !valid_observation(&observation) {
            return Err(AdapterRefusal::DestinationUnattested);
        }
        self.pending = Some(PendingBinding {
            ceremony_id,
            observation,
        });
        Ok(())
    }

    /// Completes the ceremony only if the user-confirmed surface remained
    /// exactly unchanged between capture and confirmation.
    pub fn confirm(
        &mut self,
        ceremony_id: &str,
        target: VisualBindingTarget,
        observation: VisualBindingObservation,
        now_ms: u64,
    ) -> Result<DestinationIdentity, AdapterRefusal> {
        let Some(pending) = self.pending.take() else {
            return Err(AdapterRefusal::DestinationUnattested);
        };
        if pending.ceremony_id != ceremony_id
            || !valid_target(&target)
            || !same_surface(&pending.observation, &observation)
        {
            self.clear();
            return Err(AdapterRefusal::DestinationChanged);
        }
        self.binding = Some(VerifiedBinding {
            ceremony_id: pending.ceremony_id,
            confirmed_at_ms: now_ms,
            expires_at_ms: now_ms.saturating_add(self.ttl_ms),
            observation,
            target,
        });
        self.destination(now_ms)
    }

    /// Must be called immediately before a protected operation.
    pub fn before_protected_operation(
        &mut self,
        observation: VisualBindingObservation,
        now_ms: u64,
    ) -> Result<DestinationIdentity, AdapterRefusal> {
        self.revalidate(observation, now_ms)
    }

    /// Must be called immediately after a protected operation. A changed
    /// commitment invalidates the binding even if the operation has completed.
    pub fn after_protected_operation(
        &mut self,
        observation: VisualBindingObservation,
        now_ms: u64,
    ) -> Result<DestinationIdentity, AdapterRefusal> {
        self.revalidate(observation, now_ms)
    }

    pub fn destination(&mut self, now_ms: u64) -> Result<DestinationIdentity, AdapterRefusal> {
        let Some(binding) = self.binding.as_ref() else {
            return Err(AdapterRefusal::DestinationUnattested);
        };
        if now_ms > binding.expires_at_ms {
            self.clear();
            return Err(AdapterRefusal::DestinationUnattested);
        }
        Ok(destination_identity(binding, self.ttl_ms))
    }

    /// Call these hooks for every observable §6.5 transition. The guard is
    /// deliberately conservative: transition identity is diagnostic only; all
    /// transitions have the same fail-closed effect.
    pub fn invalidate(&mut self, _transition: VisualBindingTransition) {
        self.clear();
    }

    pub fn focus_lost(&mut self) {
        self.invalidate(VisualBindingTransition::FocusLost);
    }

    pub fn window_moved_or_resized(&mut self) {
        self.invalidate(VisualBindingTransition::WindowMovedOrResized);
    }

    pub fn minimized(&mut self) {
        self.invalidate(VisualBindingTransition::Minimized);
    }

    pub fn occluded(&mut self) {
        self.invalidate(VisualBindingTransition::Occluded);
    }

    pub fn conversation_changed(&mut self) {
        self.invalidate(VisualBindingTransition::ConversationChanged);
    }

    pub fn composer_changed(&mut self) {
        self.invalidate(VisualBindingTransition::ComposerChanged);
    }

    fn revalidate(
        &mut self,
        observation: VisualBindingObservation,
        now_ms: u64,
    ) -> Result<DestinationIdentity, AdapterRefusal> {
        let Some(binding) = self.binding.as_ref() else {
            return Err(AdapterRefusal::DestinationUnattested);
        };
        if now_ms > binding.expires_at_ms {
            self.clear();
            return Err(AdapterRefusal::DestinationUnattested);
        }
        if !same_surface(&binding.observation, &observation) {
            self.clear();
            return Err(AdapterRefusal::DestinationChanged);
        }
        Ok(destination_identity(binding, self.ttl_ms))
    }

    fn clear(&mut self) {
        self.pending = None;
        self.binding = None;
    }
}

fn destination_identity(binding: &VerifiedBinding, ttl_ms: u64) -> DestinationIdentity {
    let context = binding.observation.context_binding_sha256.as_bytes();
    DestinationIdentity {
        status: DestinationStatus::Attested,
        account_digest: digest(b"OSL/visual-binding/account/v1", context),
        conversation_digest: digest(b"OSL/visual-binding/conversation/v1", context),
        recipients_digest: digest(b"OSL/visual-binding/recipients/v1", context),
        scope_binding_hash: binding.target.scope_binding_hash.clone(),
        evidence: BindingEvidence::UserConfirmedVisualBinding {
            ceremony_id: binding.ceremony_id.clone(),
            confirmed_at_ms: binding.confirmed_at_ms,
        },
        attested_at_ms: binding.confirmed_at_ms,
        ttl_ms,
    }
}

fn same_surface(left: &VisualBindingObservation, right: &VisualBindingObservation) -> bool {
    valid_observation(right)
        && left.native_window_id == right.native_window_id
        && left.host_generation == right.host_generation
        && left.window_rect == right.window_rect
        && left.composer_binding_sha256 == right.composer_binding_sha256
        && left.context_binding_sha256 == right.context_binding_sha256
}

fn valid_observation(value: &VisualBindingObservation) -> bool {
    value.native_window_id != 0
        && value.host_generation != 0
        && value.foreground
        && !value.minimized
        && !value.occluded
        && valid_sha256(&value.composer_binding_sha256)
        && valid_sha256(&value.context_binding_sha256)
        && value.window_rect[2] > value.window_rect[0]
        && value.window_rect[3] > value.window_rect[1]
}

fn valid_target(target: &VisualBindingTarget) -> bool {
    valid_sha256(&target.scope_binding_hash)
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_ceremony_id(value: &str) -> bool {
    (16..=128).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn digest(domain: &[u8], value: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update((domain.len() as u32).to_be_bytes());
    hasher.update(domain);
    hasher.update((value.len() as u32).to_be_bytes());
    hasher.update(value);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_000;

    fn digest_byte(byte: char) -> String {
        std::iter::repeat(byte).take(64).collect()
    }

    fn observation() -> VisualBindingObservation {
        VisualBindingObservation {
            native_window_id: 44,
            host_generation: 8,
            window_rect: [100, 100, 900, 700],
            composer_binding_sha256: digest_byte('a'),
            context_binding_sha256: digest_byte('b'),
            foreground: true,
            minimized: false,
            occluded: false,
        }
    }

    fn target() -> VisualBindingTarget {
        VisualBindingTarget {
            app: AdapterAppId::Telegram,
            scope_binding_hash: digest_byte('c'),
        }
    }

    fn confirmed() -> VisualBindingGuard {
        let mut guard = VisualBindingGuard::default();
        guard
            .begin("ceremony-id-1234".to_owned(), observation())
            .unwrap();
        guard
            .confirm("ceremony-id-1234", target(), observation(), NOW)
            .unwrap();
        guard
    }

    #[test]
    fn every_required_transition_invalidates_the_confirmation() {
        let transitions: [fn(&mut VisualBindingGuard); 6] = [
            VisualBindingGuard::focus_lost,
            VisualBindingGuard::window_moved_or_resized,
            VisualBindingGuard::minimized,
            VisualBindingGuard::occluded,
            VisualBindingGuard::conversation_changed,
            VisualBindingGuard::composer_changed,
        ];
        for transition in transitions {
            let mut guard = confirmed();
            transition(&mut guard);
            assert_eq!(
                guard.before_protected_operation(observation(), NOW + 1),
                Err(AdapterRefusal::DestinationUnattested)
            );
        }
    }

    #[test]
    fn content_commitment_mismatch_after_an_operation_invalidates() {
        let mut guard = confirmed();
        assert!(guard
            .before_protected_operation(observation(), NOW + 1)
            .is_ok());
        let mut changed = observation();
        changed.context_binding_sha256 = digest_byte('d');
        assert_eq!(
            guard.after_protected_operation(changed, NOW + 2),
            Err(AdapterRefusal::DestinationChanged)
        );
        assert_eq!(
            guard.destination(NOW + 2),
            Err(AdapterRefusal::DestinationUnattested)
        );
    }

    #[test]
    fn expiry_requires_a_new_confirmation() {
        let mut guard = confirmed();
        assert_eq!(
            guard
                .before_protected_operation(observation(), NOW + DEFAULT_VISUAL_BINDING_TTL_MS + 1),
            Err(AdapterRefusal::DestinationUnattested)
        );
    }

    #[test]
    fn destination_contains_only_derived_digests_and_the_visual_evidence() {
        let mut guard = confirmed();
        let destination = guard.destination(NOW + 1).unwrap();
        assert_eq!(destination.status, DestinationStatus::Attested);
        assert_ne!(
            destination.account_digest,
            observation().context_binding_sha256
        );
        assert!(matches!(
            destination.evidence,
            BindingEvidence::UserConfirmedVisualBinding { .. }
        ));
    }
}
