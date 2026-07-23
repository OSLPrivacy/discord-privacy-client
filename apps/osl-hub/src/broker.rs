//! Account- and conversation-bound trusted broker for the original OSL core.
//!
//! Platform pages never receive this command surface. A trusted local adapter
//! activates one exact service/account/conversation context and gets a
//! generation-bound lease. Switching context invalidates the prior lease,
//! preventing a prepared capsule from being reused in another account or chat.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::sync::Mutex;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use ipc::scope::{ScopeInput, ScopeKind};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::core_bridge::HubCoreState;
use crate::security::{self, HubSecurityState, ManualPeerBinding};
use crate::service_host::{service_manifest, validate_opaque_id, ActiveServiceHost};
use crate::service_scope_index::ServiceScopeRegistration;
use crate::services::{service_kind_from_id, ServiceRegistryState};

const MAX_CONTEXT_ID_BYTES: usize = 160;
const MAX_PARTICIPANTS: usize = 512;
const MAX_TEXT_BYTES: usize = 1_000;
const MAX_NATIVE_OVERLAY_CHUNK_BYTES: usize = 40 * 1024;
const MAX_NATIVE_OVERLAY_LOGICAL_TEXT_BYTES: usize = 1024 * 1024;
const MAX_NATIVE_OVERLAY_TEXT_CHUNKS: usize = 32;
const MAX_NATIVE_OVERLAY_REASSEMBLY_GROUPS: usize = 8;
const MAX_NATIVE_OVERLAY_REASSEMBLY_BYTES: usize = 8 * 1024 * 1024;
const MAX_PROSE_COVER_BYTES: usize = 16 * 1024;
const MAX_ATTACHMENT_B64_BYTES: usize = 32 * 1024 * 1024;
const MAX_LOCAL_LEDGER_BYTES: usize = 2 * 1024 * 1024;
const MAX_LOCAL_LEDGER_ENTRIES: usize = 4_096;
const LOCAL_PROTECTED_VERSION: u32 = 1;
const PEER_PROTECTED_VERSION: u32 = 2;
const PEER_PROTECTED_CHUNK_VERSION: u32 = 4;
const PEER_PROTECTED_CHUNK_PREFIX: &[u8; 8] = b"OSLTXT4\0";
const PEER_ATTACHMENT_VERSION: u32 = 1;
const NATIVE_OVERLAY_RELAY_VERSION: u32 = 1;
const NATIVE_OVERLAY_RELAY_DOMAIN: &str = "osl-privacy/native-discord-overlay/relay-notice/v1";
const NATIVE_OVERLAY_ACK_VERSION: u32 = 1;
const NATIVE_OVERLAY_ACK_DOMAIN: &str = "osl-privacy/native-discord-overlay/ack/v1";
const NATIVE_OVERLAY_ATTACHMENT_VERSION: u32 = 1;
const NATIVE_OVERLAY_ATTACHMENT_DOMAIN: &str =
    "osl-privacy/native-discord-overlay/attachment-notice/v1";
const MAX_STREAMED_ATTACHMENT_BYTES: u64 = ipc::cipher_store_client::MAX_SEALED_ATTACHMENT_BYTES;
const MAX_NATIVE_OVERLAY_OPEN_BATCH: usize = 64;
const MAX_PEER_LIFETIME_SECONDS: i64 = 7 * 24 * 60 * 60;
const MAX_PEER_CLOCK_SKEW_SECONDS: i64 = 5 * 60;
const LOCAL_PROTECTED_MESSAGE_TYPE: u8 = 0x80;
const LOCAL_PROTECTED_FILE: &str = "hub_local_protected.json";
const NATIVE_OVERLAY_RECEIPTS_FILE: &str = "hub_native_overlay_receipts.json";
const LOCAL_PROTECTED_LABEL: &str = "local_protected_loopback";

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HubConversationKind {
    Dm,
    Group,
    Channel,
    Space,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HubConversationContext {
    pub service_id: String,
    pub account_id: String,
    pub conversation_kind: HubConversationKind,
    pub conversation_id: String,
    pub space_id: Option<String>,
    pub participant_osl_ids: Vec<String>,
    pub self_osl_id: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextLease {
    pub generation: u64,
    pub host_generation: u64,
    pub context_token: String,
    pub service_id: String,
    pub account_id: String,
}

#[derive(Debug, Clone)]
struct ActiveContext {
    lease: ContextLease,
    context: HubConversationContext,
    authority: ContextAuthority,
    manual_peer: Option<ManualPeerContext>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct ManualPeerContext {
    service_id: String,
    account_id: String,
    person_id: String,
    peer_osl_user_id: String,
    scope: ScopeInput,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum ContextAuthority {
    PeerMessaging,
    ManualPeer,
    LocalLoopback,
}

#[derive(Debug, Default)]
struct BrokerInner {
    generation: u64,
    active: Option<ActiveContext>,
}

#[derive(Debug, Default)]
pub struct HubBrokerState {
    inner: Mutex<BrokerInner>,
    local_protected_transition: Mutex<()>,
    native_overlay_receipt_transition: Mutex<()>,
    native_overlay_received_view_once: Mutex<BTreeMap<String, i64>>,
}

impl HubBrokerState {
    pub fn activate(
        &self,
        context: HubConversationContext,
        host_generation: u64,
    ) -> Result<ContextLease, String> {
        self.activate_with_authority(
            context,
            host_generation,
            ContextAuthority::PeerMessaging,
            None,
        )
    }

    fn activate_with_authority(
        &self,
        context: HubConversationContext,
        host_generation: u64,
        authority: ContextAuthority,
        manual_peer: Option<ManualPeerContext>,
    ) -> Result<ContextLease, String> {
        validate_context(&context)?;
        if host_generation == 0 {
            return Err("OSL broker host generation is invalid".to_owned());
        }
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "OSL broker state is unavailable".to_owned())?;
        inner.generation = inner
            .generation
            .checked_add(1)
            .ok_or_else(|| "OSL broker generation exhausted".to_owned())?;
        let lease = ContextLease {
            generation: inner.generation,
            host_generation,
            context_token: context_token(inner.generation, host_generation, &context),
            service_id: context.service_id.clone(),
            account_id: context.account_id.clone(),
        };
        inner.active = Some(ActiveContext {
            lease: lease.clone(),
            context,
            authority,
            manual_peer,
        });
        Ok(lease)
    }

    fn activate_local_loopback(
        &self,
        owner_osl_user_id: &str,
        active: &ActiveServiceHost,
        conversation_id: String,
    ) -> Result<ContextLease, String> {
        validate_loopback_conversation_id(&conversation_id)?;
        let context = HubConversationContext {
            service_id: active.service_id.clone(),
            account_id: active.account_id.clone(),
            conversation_kind: HubConversationKind::Dm,
            conversation_id,
            space_id: None,
            participant_osl_ids: vec![owner_osl_user_id.to_owned()],
            self_osl_id: owner_osl_user_id.to_owned(),
        };
        self.activate_with_authority(
            context,
            active.generation,
            ContextAuthority::LocalLoopback,
            None,
        )
    }

    fn activate_manual_peer(
        &self,
        owner_osl_user_id: &str,
        active: &ActiveServiceHost,
        binding: ManualPeerBinding,
    ) -> Result<ContextLease, String> {
        let channel_binding = manual_dm_channel_binding(
            &active.service_id,
            owner_osl_user_id,
            &binding.peer_osl_user_id,
        )?;
        let scope = ScopeInput {
            kind: ScopeKind::Dm,
            id: security::manual_peer_scope_id(
                &active.service_id,
                &active.account_id,
                &binding.person_id,
            )?,
            server_id: None,
            channel_id: Some(channel_binding.clone()),
        };
        let manual_peer = ManualPeerContext {
            service_id: active.service_id.clone(),
            account_id: active.account_id.clone(),
            person_id: binding.person_id.clone(),
            peer_osl_user_id: binding.peer_osl_user_id,
            scope,
        };
        let context = HubConversationContext {
            service_id: active.service_id.clone(),
            account_id: active.account_id.clone(),
            conversation_kind: HubConversationKind::Dm,
            conversation_id: channel_binding,
            space_id: None,
            // The original core indexes peers by the local person id. Never
            // accept this recipient set from the renderer.
            participant_osl_ids: vec![binding.person_id],
            self_osl_id: owner_osl_user_id.to_owned(),
        };
        self.activate_with_authority(
            context,
            active.generation,
            ContextAuthority::ManualPeer,
            Some(manual_peer),
        )
    }

    pub fn validate_active_host(
        &self,
        context_token: &str,
        active: &ActiveServiceHost,
    ) -> Result<(), String> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| "OSL broker state is unavailable".to_owned())?;
        let context = inner
            .active
            .as_ref()
            .ok_or_else(|| "OSL broker has no active trusted context".to_owned())?;
        if context.lease.context_token != context_token
            || context.lease.host_generation != active.generation
            || context.lease.service_id != active.service_id
            || context.lease.account_id != active.account_id
        {
            return Err(
                "OSL broker context is stale or belongs to another service host".to_owned(),
            );
        }
        Ok(())
    }

    pub fn clear(&self) -> Result<(), String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "OSL broker state is unavailable".to_owned())?;
        inner.generation = inner
            .generation
            .checked_add(1)
            .ok_or_else(|| "OSL broker generation exhausted".to_owned())?;
        inner.active = None;
        if let Ok(mut received) = self.native_overlay_received_view_once.lock() {
            received.clear();
        }
        Ok(())
    }

    fn view_once_received_was_sent(&self, message_id: &str, now: i64) -> Result<bool, String> {
        let mut received = self
            .native_overlay_received_view_once
            .lock()
            .map_err(|_| "OSL view-once receipt state is unavailable".to_owned())?;
        received.retain(|_, expires_at| *expires_at > now);
        Ok(received.contains_key(message_id))
    }

    fn record_view_once_received(
        &self,
        message_id: &str,
        expires_at: i64,
        now: i64,
    ) -> Result<(), String> {
        let mut received = self
            .native_overlay_received_view_once
            .lock()
            .map_err(|_| "OSL view-once receipt state is unavailable".to_owned())?;
        received.retain(|_, retained_until| *retained_until > now);
        if !received.contains_key(message_id) && received.len() >= MAX_LOCAL_LEDGER_ENTRIES {
            return Err("OSL view-once receipt state reached its safe limit".to_owned());
        }
        received.insert(message_id.to_owned(), expires_at);
        Ok(())
    }

    fn context_for(&self, context_token: &str) -> Result<HubConversationContext, String> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| "OSL broker state is unavailable".to_owned())?;
        let active = inner
            .active
            .as_ref()
            .ok_or_else(|| "OSL broker has no active trusted context".to_owned())?;
        if active.lease.context_token != context_token {
            return Err("OSL broker context is stale or belongs to another account".to_owned());
        }
        Ok(active.context.clone())
    }

    fn require_authority(
        &self,
        context_token: &str,
        expected: ContextAuthority,
    ) -> Result<(), String> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| "OSL broker state is unavailable".to_owned())?;
        let active = inner
            .active
            .as_ref()
            .ok_or_else(|| "OSL broker has no active trusted context".to_owned())?;
        if active.lease.context_token != context_token || active.authority != expected {
            return Err("OSL broker context does not authorize this operation".to_owned());
        }
        Ok(())
    }

    pub fn require_peer_messaging_context(&self, context_token: &str) -> Result<(), String> {
        self.require_authority(context_token, ContextAuthority::PeerMessaging)
    }

    #[cfg(test)]
    fn require_local_loopback_context(&self, context_token: &str) -> Result<(), String> {
        self.require_authority(context_token, ContextAuthority::LocalLoopback)
    }

    pub fn scope_for_context(&self, context_token: &str) -> Result<ScopeInput, String> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| "OSL broker state is unavailable".to_owned())?;
        let active = inner
            .active
            .as_ref()
            .ok_or_else(|| "OSL broker has no active trusted context".to_owned())?;
        if active.lease.context_token != context_token {
            return Err("OSL broker context is stale or belongs to another account".to_owned());
        }
        active
            .manual_peer
            .as_ref()
            .map(|manual| manual.scope.clone())
            .map(Ok)
            .unwrap_or_else(|| scope_input(&active.context))
    }

    fn manual_peer_for(&self, context_token: &str) -> Result<ManualPeerContext, String> {
        self.require_authority(context_token, ContextAuthority::ManualPeer)?;
        let inner = self
            .inner
            .lock()
            .map_err(|_| "OSL broker state is unavailable".to_owned())?;
        let active = inner
            .active
            .as_ref()
            .filter(|active| active.lease.context_token == context_token)
            .ok_or_else(|| {
                "OSL broker context is stale or belongs to another account".to_owned()
            })?;
        active
            .manual_peer
            .clone()
            .ok_or_else(|| "OSL broker context is not a manual peer conversation".to_owned())
    }

    /// Return the current native Discord manual-peer lease without accepting a
    /// renderer-provided context capability. Overlay-only commands use this to
    /// revalidate the native host generation before and after network work.
    pub fn active_native_manual_context_token(&self) -> Result<String, String> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| "OSL broker state is unavailable".to_owned())?;
        let active = inner
            .active
            .as_ref()
            .filter(|active| {
                active.authority == ContextAuthority::ManualPeer
                    && active.context.service_id == "discord"
                    && active.context.account_id.starts_with("native-discord-")
                    && active.manual_peer.is_some()
            })
            .ok_or_else(|| "OSL native Discord protection is not active".to_owned())?;
        Ok(active.lease.context_token.clone())
    }

    /// Return the current first-party OSL direct-chat lease. Unlike the
    /// native Discord path, this context is owned entirely by the trusted main
    /// window and therefore has no foreign host generation to revalidate.
    pub fn active_osl_chat_context_token(&self) -> Result<String, String> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| "OSL broker state is unavailable".to_owned())?;
        let active = inner
            .active
            .as_ref()
            .filter(|active| {
                active.authority == ContextAuthority::ManualPeer
                    && active.context.service_id == "osl-chat"
                    && active.context.account_id == "osl-main"
                    && active.manual_peer.is_some()
            })
            .ok_or_else(|| "OSL Chat is not active".to_owned())?;
        Ok(active.lease.context_token.clone())
    }

    pub fn clear_osl_chat_context(&self) -> Result<(), String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "OSL broker state is unavailable".to_owned())?;
        let is_osl_chat = inner.active.as_ref().is_some_and(|active| {
            active.authority == ContextAuthority::ManualPeer
                && active.context.service_id == "osl-chat"
                && active.context.account_id == "osl-main"
        });
        if is_osl_chat {
            inner.generation = inner
                .generation
                .checked_add(1)
                .ok_or_else(|| "OSL broker generation exhausted".to_owned())?;
            inner.active = None;
        }
        Ok(())
    }

    pub fn manual_permission_target(
        &self,
        context_token: &str,
        requested_person_id: &str,
        requested_broadened: bool,
    ) -> Result<String, String> {
        let manual = self.manual_peer_for(context_token)?;
        if manual.person_id != requested_person_id || requested_broadened {
            return Err(
                "OSL manual peer permission target does not match the active friend".to_owned(),
            );
        }
        Ok(manual.person_id)
    }

    pub fn manual_burn_target(
        &self,
        context_token: &str,
    ) -> Result<Option<ManualPeerBurnTarget>, String> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| "OSL broker state is unavailable".to_owned())?;
        let active = inner
            .active
            .as_ref()
            .filter(|active| active.lease.context_token == context_token)
            .ok_or_else(|| {
                "OSL broker context is stale or belongs to another account".to_owned()
            })?;
        Ok(active
            .manual_peer
            .as_ref()
            .map(|manual| ManualPeerBurnTarget {
                service_id: manual.service_id.clone(),
                account_id: manual.account_id.clone(),
                person_id: manual.person_id.clone(),
                scope: manual.scope.clone(),
            }))
    }

    pub fn service_scope_registration(
        &self,
        context_token: &str,
    ) -> Result<ServiceScopeRegistration, String> {
        let context = self.context_for(context_token)?;
        let scope = self.scope_for_context(context_token)?;
        let manual_peer_person_id = self
            .manual_burn_target(context_token)?
            .map(|manual| manual.person_id);
        let canonical_channel_ids = scope.channel_id.clone().into_iter().collect::<Vec<_>>();
        if canonical_channel_ids.is_empty() {
            return Err(
                "OSL cannot index a service scope without complete channel coverage".to_owned(),
            );
        }
        Ok(ServiceScopeRegistration {
            owner_osl_user_id: context.self_osl_id.clone(),
            service_id: context.service_id.clone(),
            account_id: context.account_id.clone(),
            scope,
            canonical_channel_ids,
            local_context_binding_sha256: local_context_binding(&context),
            manual_peer_person_id,
        })
    }
}

/// Create a local-only, self-recipient context from trusted backend state.
/// The caller supplies no identity or participant IDs: ownership, active host
/// identity, and host generation are all derived and checked here.
pub fn activate_owned_local_loopback_context(
    broker: &HubBrokerState,
    registry: &ServiceRegistryState,
    host: &crate::service_host::ServiceHostState,
    owner_osl_user_id: &str,
    service_id: &str,
    account_id: &str,
    conversation_id: String,
) -> Result<ContextLease, String> {
    let service_kind =
        service_kind_from_id(service_id).ok_or_else(|| "unknown service".to_owned())?;
    registry.require_owned(owner_osl_user_id, service_kind, account_id)?;
    let active = host
        .require_current_owned(owner_osl_user_id, service_id, account_id)
        .map_err(|error| error.to_string())?;
    broker.activate_local_loopback(owner_osl_user_id, &active, conversation_id)
}

#[derive(Debug, Clone)]
pub struct ActivatedManualPeerContext {
    pub lease: ContextLease,
    pub person_id: String,
    pub peer_osl_user_id: String,
    pub scope: ScopeInput,
}

#[derive(Debug, Clone)]
pub struct ManualPeerBurnTarget {
    pub service_id: String,
    pub account_id: String,
    pub person_id: String,
    pub scope: ScopeInput,
}

/// Activate one renderer-selected existing friend without accepting any
/// participant, recipient, or conversation identifier from the renderer.
pub fn activate_owned_manual_peer_context(
    broker: &HubBrokerState,
    registry: &ServiceRegistryState,
    host: &crate::service_host::ServiceHostState,
    owner_osl_user_id: &str,
    service_id: &str,
    account_id: &str,
    binding: ManualPeerBinding,
) -> Result<ActivatedManualPeerContext, String> {
    let service_kind =
        service_kind_from_id(service_id).ok_or_else(|| "unknown service".to_owned())?;
    registry.require_owned(owner_osl_user_id, service_kind, account_id)?;
    let active = host
        .require_current_owned(owner_osl_user_id, service_id, account_id)
        .map_err(|error| error.to_string())?;
    activate_manual_peer_from_trusted_host(broker, owner_osl_user_id, &active, binding)
}

/// Activate an OSL-owned protection layer over the currently trusted native
/// Discord lifecycle. The synthetic account id and generation are derived by
/// native code; Discord credentials, profile state, and page content are never
/// consulted.
pub fn activate_owned_native_manual_peer_context(
    broker: &HubBrokerState,
    owner_osl_user_id: &str,
    active: &ActiveServiceHost,
    binding: ManualPeerBinding,
) -> Result<ActivatedManualPeerContext, String> {
    if active.service_id != "discord"
        || !active.account_id.starts_with("native-discord-")
        || active.generation == 0
    {
        return Err("OSL native Discord context is unavailable".to_owned());
    }
    activate_manual_peer_from_trusted_host(broker, owner_osl_user_id, active, binding)
}

/// Activate a first-party OSL direct chat. The renderer selects only an
/// already verified friend; every service/account/conversation identifier is
/// fixed or derived inside Rust.
pub fn activate_owned_osl_chat_context(
    broker: &HubBrokerState,
    owner_osl_user_id: &str,
    binding: ManualPeerBinding,
) -> Result<ActivatedManualPeerContext, String> {
    let active = ActiveServiceHost {
        service_id: "osl-chat".to_owned(),
        account_id: "osl-main".to_owned(),
        generation: 1,
        owner_namespace: owner_osl_user_id.to_owned(),
    };
    activate_manual_peer_from_trusted_host(broker, owner_osl_user_id, &active, binding)
}

fn activate_manual_peer_from_trusted_host(
    broker: &HubBrokerState,
    owner_osl_user_id: &str,
    active: &ActiveServiceHost,
    binding: ManualPeerBinding,
) -> Result<ActivatedManualPeerContext, String> {
    let person_id = binding.person_id.clone();
    let peer_osl_user_id = binding.peer_osl_user_id.clone();
    let lease = broker.activate_manual_peer(owner_osl_user_id, active, binding)?;
    let scope = broker.scope_for_context(&lease.context_token)?;
    Ok(ActivatedManualPeerContext {
        lease,
        person_id,
        peer_osl_user_id,
        scope,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedCoreMessage {
    pub messages: Vec<String>,
    pub control_messages: Vec<String>,
    pub session_id: Option<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedPeerProseMessage {
    pub cover_text: String,
    pub expires_at: i64,
    pub person_to_person_e2ee: bool,
    pub view_once: bool,
}

/// Plaintext is intentionally not `Debug` so diagnostics cannot format it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedPeerProseMessage {
    pub plaintext: String,
    pub context_verified: bool,
    pub person_to_person_e2ee: bool,
    pub view_once_consumed: bool,
    pub require_capture_protection: bool,
}

/// Successful background delivery into the recipient's authenticated OSL
/// inbox. No cipher-store pointer or inbox capability crosses the Tauri API.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedNativeOverlayText {
    pub message_id: String,
    pub expires_at: i64,
    pub person_to_person_e2ee: bool,
    pub view_once: bool,
    pub delivered_to_osl_inbox: bool,
}

/// Plaintext is deliberately nested only in the established non-Debug DTO.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedNativeOverlayTextBatch {
    pub messages: Vec<OpenedNativeOverlayText>,
    pub pending_view_once: Vec<PendingNativeOverlayText>,
    pub acknowledgments: Vec<NativeOverlayAcknowledgment>,
    pub fetched: u32,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingNativeOverlayText {
    pub message_id: String,
    pub expires_at: i64,
    pub person_to_person_e2ee: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeOverlayAcknowledgment {
    pub message_id: String,
    pub status: NativeOverlayAcknowledgmentStatus,
    pub acknowledged_at: i64,
}

#[derive(Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeOverlayAcknowledgmentStatus {
    Received,
    Opened,
}

/// Plaintext opened for the short-lived native overlay only. The absolute
/// expiry lets that renderer remove its in-memory copy without persisting it.
/// This deliberately does not derive `Debug`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedNativeOverlayText {
    pub plaintext: String,
    pub context_verified: bool,
    pub person_to_person_e2ee: bool,
    pub view_once_consumed: bool,
    pub expires_at: i64,
}

/// A single-device protected capsule. This is intentionally not described as
/// person-to-person E2EE: the current identity encrypts to its own X25519 key
/// and the context-bound ledger is local to this OSL Privacy identity.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedLocalProtectedMessage {
    pub capsule: String,
    pub local_message_id: String,
    pub protection: &'static str,
    pub person_to_person_e2ee: bool,
    pub state_persisted: bool,
    pub view_once: bool,
}

/// Decrypted local protected content. Deliberately does not derive `Debug` so
/// an error/debug formatter cannot accidentally emit the plaintext.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecryptedLocalProtectedMessage {
    pub plaintext: String,
    pub local_message_id: String,
    pub protection: &'static str,
    pub person_to_person_e2ee: bool,
    pub context_verified: bool,
    pub view_once_consumed: bool,
}

/// Ciphertext prepared by the original OSL attachment core for manual upload.
/// The remote service page never receives plaintext or a Tauri command. A
/// platform adapter may eventually place only `sealed_b64` after validating
/// the exact active conversation.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedHubAttachment {
    pub sealed_b64: String,
    pub transport_filename: String,
    pub transport_mime_type: &'static str,
    pub original_mime_type: String,
    pub ciphertext_prepared: bool,
    pub automatic_service_upload: bool,
}

/// Plain attachment output for the bundled trusted UI. Deliberately omits
/// `Debug` because `plaintext_b64` is user content.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedHubAttachment {
    pub plaintext_b64: String,
    pub original_filename: String,
    pub mime_type: String,
    pub context_verified: bool,
}

/// Internal manual-peer attachment result. This deliberately stays off the
/// Tauri command surface: a native service adapter can transport the opaque
/// bytes and envelope without ever receiving the attachment key or plaintext.
pub struct PreparedPeerAttachment {
    pub sealed_bytes: Vec<u8>,
    pub envelope_wire: String,
    pub transport_filename: String,
    pub expires_at: i64,
    pub view_once: bool,
}

/// Plain attachment recovered inside trusted Rust. It intentionally omits
/// `Debug`/`Serialize` so logs and the remote service page cannot format it.
pub struct OpenedPeerAttachment {
    pub plaintext: Vec<u8>,
    pub original_filename: String,
    pub mime_type: String,
    pub attachment_id: String,
    pub view_once_consumed: bool,
}

#[derive(Clone, Serialize, Deserialize)]
struct LocalProtectedPayload {
    version: u32,
    local_message_id: String,
    context_binding: String,
    plaintext: String,
    #[serde(default)]
    view_once: bool,
}

/// Authenticated person-to-person content. The outer relay token is only a
/// transport capability; these fields bind the encrypted bytes to the exact
/// OSL service and identity pair so a valid Discord message cannot be replayed
/// as another app or another friend conversation.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PeerProtectedPayload {
    version: u32,
    message_id: String,
    created_at: i64,
    expires_at: i64,
    service_id: String,
    conversation_binding: String,
    sender_osl_user_id: String,
    recipient_osl_user_id: String,
    plaintext: String,
    view_once: bool,
    #[serde(default)]
    require_capture_protection: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    logical_message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    chunk_index: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    chunk_count: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    whole_sha256: Option<String>,
}

#[derive(Clone)]
struct NativeTextChunkMeta {
    logical_message_id: String,
    chunk_index: u16,
    chunk_count: u16,
    whole_sha256: String,
    created_at: i64,
    expires_at: i64,
}

struct NativeTextReassembly {
    template: PeerProtectedPayload,
    chunks: BTreeMap<u16, String>,
    inbox_ids: Vec<String>,
    bytes: usize,
    invalid: bool,
}

/// The outer envelope contains only an opaque cipher-store cover pointer and
/// the exact authenticated routing/binding facts needed before resolving it.
/// It never contains message plaintext.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeOverlayRelayNotice {
    version: u32,
    domain: String,
    created_at: i64,
    expires_at: i64,
    service_id: String,
    conversation_binding: String,
    sender_osl_user_id: String,
    recipient_osl_user_id: String,
    message_id: String,
    cover_pointer: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeOverlayAcknowledgmentPayload {
    version: u32,
    domain: String,
    message_id: String,
    status: NativeOverlayAcknowledgmentStatus,
    acknowledged_at: i64,
    expires_at: i64,
    service_id: String,
    conversation_binding: String,
    sender_osl_user_id: String,
    recipient_osl_user_id: String,
}

#[derive(Clone, Copy)]
struct PeerProtectionPolicy {
    view_once: bool,
    require_capture_protection: bool,
    created_at: i64,
    expires_at: i64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PeerAttachmentPayload {
    version: u32,
    attachment_id: String,
    created_at: i64,
    expires_at: i64,
    service_id: String,
    conversation_binding: String,
    sender_osl_user_id: String,
    recipient_osl_user_id: String,
    original_filename: String,
    mime_type: String,
    plaintext_size: u64,
    transport_filename: String,
    ciphertext_sha256: String,
    ciphertext_format: String,
    key_algorithm: String,
    attachment_key: [u8; 32],
    view_once: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeOverlayAttachmentNotice {
    version: u32,
    domain: String,
    attachment_id: String,
    created_at: i64,
    expires_at: i64,
    service_id: String,
    conversation_binding: String,
    sender_osl_user_id: String,
    recipient_osl_user_id: String,
    original_filename: String,
    mime_type: String,
    plaintext_size: u64,
    sealed_size: u64,
    ciphertext_sha256: String,
    ciphertext_format: String,
    object_id: String,
    fetch_token: String,
    attachment_key: [u8; 32],
    content_id: [u8; 16],
    view_once: bool,
}

impl Drop for NativeOverlayAttachmentNotice {
    fn drop(&mut self) {
        self.fetch_token.clear();
        self.attachment_key.fill(0);
        self.content_id.fill(0);
    }
}

pub struct NativeOverlayAttachmentSealPlan {
    pub attachment_id: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub original_filename: String,
    pub mime_type: String,
    pub plaintext_size: u64,
    pub attachment_key: [u8; 32],
    pub content_id: [u8; 16],
    pub view_once: bool,
    pub burn_scope: ScopeInput,
    service_id: String,
    account_id: String,
    person_id: String,
    peer_osl_user_id: String,
    conversation_binding: String,
    self_osl_user_id: String,
}

impl Drop for NativeOverlayAttachmentSealPlan {
    fn drop(&mut self) {
        self.attachment_key.fill(0);
        self.content_id.fill(0);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedNativeOverlayAttachment {
    pub attachment_id: String,
    pub original_filename: String,
    pub plaintext_size: u64,
    pub expires_at: i64,
    pub view_once: bool,
    pub delivered_to_osl_inbox: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingNativeOverlayAttachment {
    pub attachment_id: String,
    pub original_filename: String,
    pub mime_type: String,
    pub plaintext_size: u64,
    pub expires_at: i64,
    pub view_once: bool,
}

pub struct NativeOverlayAttachmentOpenPlan {
    pub inbox_id: String,
    pub attachment_id: String,
    pub original_filename: String,
    pub mime_type: String,
    pub plaintext_size: u64,
    pub sealed_size: u64,
    pub ciphertext_sha256: String,
    pub object_id: String,
    pub fetch_token: String,
    pub attachment_key: [u8; 32],
    pub view_once: bool,
    expires_at: i64,
}

impl Drop for NativeOverlayAttachmentOpenPlan {
    fn drop(&mut self) {
        self.attachment_key.fill(0);
        self.fetch_token.clear();
    }
}

impl Drop for PeerAttachmentPayload {
    fn drop(&mut self) {
        self.attachment_key.fill(0);
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct LocalProtectedRecord {
    context_binding: String,
    capsule_sha256: String,
    created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_opened_at: Option<i64>,
    #[serde(default)]
    view_once: bool,
}

#[derive(Default, Serialize, Deserialize)]
struct LocalProtectedLedger {
    version: u32,
    #[serde(default)]
    records: BTreeMap<String, LocalProtectedRecord>,
}

#[derive(Clone, Serialize, Deserialize)]
struct NativeOverlayReceiptRecord {
    service_id: String,
    conversation_binding: String,
    peer_osl_user_id: String,
    expires_at: i64,
    status: NativeOverlayReceiptStatus,
    #[serde(default)]
    acknowledged_at: i64,
}

#[derive(Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum NativeOverlayReceiptStatus {
    Sent,
    Received,
    Opened,
}

#[derive(Default, Serialize, Deserialize)]
struct NativeOverlayReceiptLedger {
    version: u32,
    #[serde(default)]
    records: BTreeMap<String, NativeOverlayReceiptRecord>,
}

pub fn prepare_encrypted_text(
    core: &HubCoreState,
    broker: &HubBrokerState,
    context_token: &str,
    plaintext: String,
) -> Result<PreparedCoreMessage, String> {
    broker.require_peer_messaging_context(context_token)?;
    if plaintext.is_empty() || plaintext.len() > MAX_TEXT_BYTES {
        return Err(format!(
            "OSL broker plaintext must be between 1 and {MAX_TEXT_BYTES} bytes"
        ));
    }
    let context = broker.context_for(context_token)?;
    let scope = broker.scope_for_context(context_token)?;
    let encrypted = ipc::commands::cmd_osl_encrypt_message_v2(
        &core.osl,
        plaintext,
        scope,
        context.participant_osl_ids,
        context.self_osl_id,
    )?;
    Ok(PreparedCoreMessage {
        messages: encrypted.messages,
        control_messages: encrypted.control_messages,
        session_id: encrypted.session_id,
    })
}

pub fn prepare_peer_prose_text(
    core: &HubCoreState,
    security_state: &HubSecurityState,
    broker: &HubBrokerState,
    context_token: &str,
    plaintext: String,
    view_once: bool,
) -> Result<PreparedPeerProseMessage, String> {
    prepare_peer_prose_text_with_capture(
        core,
        security_state,
        broker,
        context_token,
        plaintext,
        view_once,
        false,
    )
}

pub fn prepare_peer_prose_text_with_capture(
    core: &HubCoreState,
    security_state: &HubSecurityState,
    broker: &HubBrokerState,
    context_token: &str,
    plaintext: String,
    view_once: bool,
    require_capture_protection: bool,
) -> Result<PreparedPeerProseMessage, String> {
    prepare_peer_prose_text_inner(
        core,
        security_state,
        broker,
        context_token,
        plaintext,
        view_once,
        require_capture_protection,
    )
    .map(|(prepared, _)| prepared)
}

fn prepare_peer_prose_text_inner(
    core: &HubCoreState,
    security_state: &HubSecurityState,
    broker: &HubBrokerState,
    context_token: &str,
    plaintext: String,
    view_once: bool,
    require_capture_protection: bool,
) -> Result<(PreparedPeerProseMessage, String), String> {
    prepare_peer_prose_text_inner_with_chunk(
        core,
        security_state,
        broker,
        context_token,
        plaintext,
        view_once,
        require_capture_protection,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn prepare_peer_prose_text_inner_with_chunk(
    core: &HubCoreState,
    security_state: &HubSecurityState,
    broker: &HubBrokerState,
    context_token: &str,
    plaintext: String,
    view_once: bool,
    require_capture_protection: bool,
    chunk: Option<NativeTextChunkMeta>,
) -> Result<(PreparedPeerProseMessage, String), String> {
    let manual = broker.manual_peer_for(context_token)?;
    let verified = security::require_manual_peer_scope_approved(
        core,
        &manual.service_id,
        &manual.account_id,
        manual.person_id.clone(),
        manual.scope.clone(),
    )?;
    let context = broker.context_for(context_token)?;
    let ttl_seconds = security::scope_security(manual.scope.clone())?.ttl_seconds;
    if i64::from(ttl_seconds) > MAX_PEER_LIFETIME_SECONDS || ttl_seconds == 0 {
        return Err("OSL could not prepare a single manual peer message".to_owned());
    }
    let now = chunk
        .as_ref()
        .map_or_else(ipc::main_password::now_unix_secs_pub, |chunk| {
            chunk.created_at
        });
    let expires_at = chunk
        .as_ref()
        .map_or_else(
            || now.checked_add(i64::from(ttl_seconds)),
            |chunk| Some(chunk.expires_at),
        )
        .ok_or_else(|| "OSL could not prepare a single manual peer message".to_owned())?;
    if expires_at.saturating_sub(now) != i64::from(ttl_seconds) {
        return Err("OSL could not prepare a single manual peer message".to_owned());
    }
    let message_id = random_peer_message_id();
    let encrypted = prepare_direct_manual_v3(
        core,
        &verified,
        &manual,
        &context,
        plaintext,
        PeerProtectionPolicy {
            view_once,
            require_capture_protection,
            created_at: now,
            expires_at,
        },
        message_id.clone(),
        chunk.as_ref(),
    )?;
    if verify_manual_v3(core, &verified, &encrypted, ManualWireSender::SelfIdentity).is_err() {
        return Err("OSL could not prepare a single manual peer message".to_owned());
    }

    let dir = keystore::osl_config_dir()
        .map_err(|_| "OSL Privacy account storage is unavailable".to_owned())?;
    let uploaded = ipc::prose_token::prose_token_send(&dir, &manual.scope, &encrypted, ttl_seconds)
        .map_err(|_| "OSL could not prepare the encrypted copy text".to_owned())?;
    if security::record_peer_prose_blob(
        security_state,
        manual.scope.clone(),
        uploaded.blob_id.clone(),
    )
    .is_err()
    {
        if ipc::prose_token::prose_token_burn_id(&dir, &manual.scope, &uploaded.blob_id).is_err() {
            // A transient primary-ledger failure must not become an
            // untracked remote blob if the authenticated DELETE also fails.
            // Retry the encrypted recoverable ledger before returning failure.
            let _ = security::record_peer_prose_blob(
                security_state,
                manual.scope.clone(),
                uploaded.blob_id.clone(),
            );
        }
        return Err("OSL could not save the encrypted message safely".to_owned());
    }
    Ok((
        PreparedPeerProseMessage {
            cover_text: uploaded.cover_text,
            expires_at,
            person_to_person_e2ee: true,
            view_once,
        },
        message_id,
    ))
}

fn prepare_direct_manual_v3(
    core: &HubCoreState,
    peer: &ManualPeerBinding,
    manual: &ManualPeerContext,
    context: &HubConversationContext,
    plaintext: String,
    policy: PeerProtectionPolicy,
    message_id: String,
    chunk: Option<&NativeTextChunkMeta>,
) -> Result<String, String> {
    let maximum = if chunk.is_some() {
        MAX_NATIVE_OVERLAY_CHUNK_BYTES
    } else {
        MAX_TEXT_BYTES
    };
    if plaintext.is_empty() || plaintext.len() > maximum {
        return Err(format!(
            "Message text must be between 1 and {maximum} UTF-8 bytes"
        ));
    }
    let payload = PeerProtectedPayload {
        version: if chunk.is_some() {
            PEER_PROTECTED_CHUNK_VERSION
        } else {
            PEER_PROTECTED_VERSION
        },
        message_id,
        created_at: policy.created_at,
        expires_at: policy.expires_at,
        service_id: manual.service_id.clone(),
        conversation_binding: context.conversation_id.clone(),
        sender_osl_user_id: context.self_osl_id.clone(),
        recipient_osl_user_id: manual.peer_osl_user_id.clone(),
        plaintext,
        view_once: policy.view_once,
        require_capture_protection: policy.require_capture_protection,
        logical_message_id: chunk.map(|value| value.logical_message_id.clone()),
        chunk_index: chunk.map(|value| value.chunk_index),
        chunk_count: chunk.map(|value| value.chunk_count),
        whole_sha256: chunk.map(|value| value.whole_sha256.clone()),
    };
    let payload = if chunk.is_some() {
        encode_peer_protected_chunk(&payload)?
    } else {
        serde_json::to_vec(&payload)
            .map_err(|_| "OSL could not prepare a single manual peer message".to_owned())?
    };
    encrypt_direct_manual_v3_payload(core, peer, ipc::wire_v2::MSG_TYPE_CONTENT, &payload)
}

fn encrypt_direct_manual_v3_payload(
    core: &HubCoreState,
    peer: &ManualPeerBinding,
    message_type: u8,
    payload: &[u8],
) -> Result<String, String> {
    let identity = core
        .osl
        .identity
        .lock()
        .map_err(|_| "OSL identity state is unavailable".to_owned())?
        .clone()
        .ok_or_else(|| "OSL identity is not loaded".to_owned())?;
    if constant_time_eq_32(identity.x25519_public.as_bytes(), &peer.peer_x25519_public) {
        return Err("OSL manual peer key matches the active identity".to_owned());
    }
    let recipients = [
        ipc::wire_v2::RecipientV3 {
            x25519_pub: identity.x25519_public,
            mlkem_pub: identity.mlkem_encapsulation_key(),
        },
        ipc::wire_v2::RecipientV3 {
            x25519_pub: crypto::x25519::PublicKey::from_bytes(peer.peer_x25519_public),
            mlkem_pub: crypto::ml_kem_768::EncapsulationKey::from_bytes(&peer.peer_mlkem768_public),
        },
    ];
    ipc::wire_v2::encrypt_v3(
        &identity.x25519_secret,
        &identity.x25519_public,
        &recipients,
        message_type,
        payload,
    )
    .map_err(|_| "OSL could not prepare a single manual peer message".to_owned())
}

pub fn open_peer_prose_text(
    core: &HubCoreState,
    security_state: &HubSecurityState,
    broker: &HubBrokerState,
    context_token: &str,
    sender_person_id: String,
    cover_text: String,
) -> Result<OpenedPeerProseMessage, String> {
    let manual = broker.manual_peer_for(context_token)?;
    let display = security::scope_security(manual.scope.clone())?;
    if !display.decrypt_display_enabled {
        return Err("Turn on decrypted text for this conversation before opening it".to_owned());
    }
    let payload = authenticate_peer_prose_pointer(
        core,
        broker,
        context_token,
        &sender_person_id,
        &cover_text,
    )?;
    let now = ipc::main_password::now_unix_secs_pub();
    security::consume_peer_message(
        security_state,
        manual.scope,
        &payload.message_id,
        payload.expires_at,
        now,
    )
    .map_err(|_| "This encrypted message could not be opened".to_owned())?;
    let view_once_consumed = payload.view_once;
    Ok(OpenedPeerProseMessage {
        plaintext: payload.plaintext,
        context_verified: true,
        person_to_person_e2ee: true,
        view_once_consumed,
        require_capture_protection: payload.require_capture_protection,
    })
}

fn authenticate_peer_prose_pointer(
    core: &HubCoreState,
    broker: &HubBrokerState,
    context_token: &str,
    sender_person_id: &str,
    cover_text: &str,
) -> Result<PeerProtectedPayload, String> {
    if cover_text.is_empty() || cover_text.len() > MAX_PROSE_COVER_BYTES {
        return Err("This encrypted message could not be opened".to_owned());
    }
    let manual = broker.manual_peer_for(context_token)?;
    if sender_person_id != manual.person_id {
        return Err("This encrypted message could not be opened".to_owned());
    }
    let verified = security::require_manual_peer_scope_approved(
        core,
        &manual.service_id,
        &manual.account_id,
        manual.person_id.clone(),
        manual.scope.clone(),
    )?;
    if verified.peer_osl_user_id != manual.peer_osl_user_id {
        return Err("This encrypted message could not be opened".to_owned());
    }
    let dir = keystore::osl_config_dir()
        .map_err(|_| "OSL Privacy account storage is unavailable".to_owned())?;
    let recovered = peer_prose_token_or_generic(ipc::prose_token::prose_token_recv(
        &dir,
        &manual.scope,
        cover_text,
    ))?;
    verify_manual_v3(core, &verified, &recovered.wire, ManualWireSender::Peer)
        .map_err(|_| "This encrypted message could not be opened".to_owned())?;
    let context = broker.context_for(context_token)?;
    let payload = decrypt_direct_manual_v3(core, &recovered.wire)
        .map_err(|_| "This encrypted message could not be opened".to_owned())?;
    validate_peer_protected_payload(
        &payload,
        &manual,
        &context,
        ipc::main_password::now_unix_secs_pub(),
    )
    .map_err(|_| "This encrypted message could not be opened".to_owned())?;
    Ok(payload)
}

/// Encrypt the inner user message with the established peer-message format,
/// upload only that ciphertext to cipher-store, then deliver a second,
/// domain-separated encrypted pointer notice through the authenticated OSL
/// inbox. Discord is neither read nor written by this path.
pub fn prepare_native_discord_overlay_text(
    core: &HubCoreState,
    security_state: &HubSecurityState,
    broker: &HubBrokerState,
    plaintext: String,
    view_once: bool,
) -> Result<PreparedNativeOverlayText, String> {
    let context_token = broker.active_native_manual_context_token()?;
    prepare_peer_inbox_text(
        core,
        security_state,
        broker,
        &context_token,
        plaintext,
        view_once,
    )
}

pub fn prepare_osl_chat_text(
    core: &HubCoreState,
    security_state: &HubSecurityState,
    broker: &HubBrokerState,
    plaintext: String,
    view_once: bool,
) -> Result<PreparedNativeOverlayText, String> {
    let context_token = broker.active_osl_chat_context_token()?;
    prepare_peer_inbox_text(
        core,
        security_state,
        broker,
        &context_token,
        plaintext,
        view_once,
    )
}

fn prepare_peer_inbox_text(
    core: &HubCoreState,
    security_state: &HubSecurityState,
    broker: &HubBrokerState,
    context_token: &str,
    plaintext: String,
    view_once: bool,
) -> Result<PreparedNativeOverlayText, String> {
    if plaintext.is_empty() || plaintext.len() > MAX_NATIVE_OVERLAY_LOGICAL_TEXT_BYTES {
        return Err(format!(
            "Private message must be between 1 and {MAX_NATIVE_OVERLAY_LOGICAL_TEXT_BYTES} UTF-8 bytes"
        ));
    }
    let manual = broker.manual_peer_for(context_token)?;
    let context = broker.context_for(context_token)?;
    let now = ipc::main_password::now_unix_secs_pub();
    let ttl_seconds = security::scope_security(manual.scope.clone())?.ttl_seconds;
    let expires_at = now
        .checked_add(i64::from(ttl_seconds))
        .ok_or_else(|| "OSL could not deliver the protected message".to_owned())?;
    let history_plaintext =
        (context.service_id == "osl-chat" && !view_once).then(|| plaintext.clone());
    let chunks = split_native_overlay_text(&plaintext)?;
    let chunk_count = u16::try_from(chunks.len())
        .map_err(|_| "OSL could not deliver the protected message".to_owned())?;
    let logical_message_id = random_peer_message_id();
    let whole_sha256 = sha256_hex(plaintext.as_bytes());
    let verified = security::require_manual_peer_scope_approved(
        core,
        &manual.service_id,
        &manual.account_id,
        manual.person_id.clone(),
        manual.scope.clone(),
    )?;
    let scope_id = native_overlay_relay_scope_id(&context.conversation_id)?;
    record_native_overlay_sent(
        core,
        broker,
        &context,
        &manual,
        &logical_message_id,
        expires_at,
    )?;
    let (identity, client) = keyserver_transport(core)?;
    for (index, chunk_plaintext) in chunks.into_iter().enumerate() {
        let chunk_index = u16::try_from(index)
            .map_err(|_| "OSL could not deliver the protected message".to_owned())?;
        let meta = NativeTextChunkMeta {
            logical_message_id: logical_message_id.clone(),
            chunk_index,
            chunk_count,
            whole_sha256: whole_sha256.clone(),
            created_at: now,
            expires_at,
        };
        let (prepared, physical_message_id) = prepare_peer_prose_text_inner_with_chunk(
            core,
            security_state,
            broker,
            context_token,
            chunk_plaintext,
            view_once,
            true,
            Some(meta),
        )?;
        let notice = NativeOverlayRelayNotice {
            version: NATIVE_OVERLAY_RELAY_VERSION,
            domain: NATIVE_OVERLAY_RELAY_DOMAIN.to_owned(),
            created_at: now,
            expires_at,
            service_id: manual.service_id.clone(),
            conversation_binding: context.conversation_id.clone(),
            sender_osl_user_id: context.self_osl_id.clone(),
            recipient_osl_user_id: manual.peer_osl_user_id.clone(),
            message_id: physical_message_id,
            cover_pointer: prepared.cover_text,
        };
        let encoded = serde_json::to_vec(&notice)
            .map_err(|_| "OSL could not deliver the protected message".to_owned())?;
        let wire = encrypt_direct_manual_v3_payload(
            core,
            &verified,
            ipc::wire_v2::MSG_TYPE_NATIVE_OVERLAY_RELAY,
            &encoded,
        )?;
        verify_manual_v3_type(
            core,
            &verified,
            &wire,
            ManualWireSender::SelfIdentity,
            ipc::wire_v2::MSG_TYPE_NATIVE_OVERLAY_RELAY,
        )
        .map_err(|_| "OSL could not deliver the protected message".to_owned())?;
        let bundle = decode_overlay_relay_wire(&wire)?;
        client
            .post_control_inbox(&identity, &manual.peer_osl_user_id, &scope_id, &bundle)
            .map_err(|_| "OSL could not deliver the protected message".to_owned())?;
    }
    if let Some(history_plaintext) = history_plaintext {
        ipc::commands::cmd_osl_persist_outbound(
            &core.osl,
            context.conversation_id.clone(),
            logical_message_id.clone(),
            history_plaintext,
        )?;
    }
    Ok(PreparedNativeOverlayText {
        message_id: logical_message_id,
        expires_at,
        person_to_person_e2ee: true,
        view_once,
        delivered_to_osl_inbox: true,
    })
}

fn split_native_overlay_text(plaintext: &str) -> Result<Vec<String>, String> {
    if plaintext.is_empty() || plaintext.len() > MAX_NATIVE_OVERLAY_LOGICAL_TEXT_BYTES {
        return Err("The private message is too large".to_owned());
    }
    let mut chunks = Vec::new();
    let mut start = 0usize;
    while start < plaintext.len() {
        let mut end = start
            .saturating_add(MAX_NATIVE_OVERLAY_CHUNK_BYTES)
            .min(plaintext.len());
        while end > start && !plaintext.is_char_boundary(end) {
            end -= 1;
        }
        if end == start {
            return Err("The private message could not be split safely".to_owned());
        }
        chunks.push(plaintext[start..end].to_owned());
        start = end;
    }
    if chunks.is_empty() || chunks.len() > MAX_NATIVE_OVERLAY_TEXT_CHUNKS {
        return Err("The private message is too large".to_owned());
    }
    Ok(chunks)
}

/// Drain only native-overlay relay notices for the currently active friend and
/// exact native Discord context. Unrelated control rows and other friends'
/// notices remain untouched for their owning drain/context.
pub fn drain_native_discord_overlay_text(
    core: &HubCoreState,
    security_state: &HubSecurityState,
    broker: &HubBrokerState,
) -> Result<OpenedNativeOverlayTextBatch, String> {
    let context_token = broker.active_native_manual_context_token()?;
    drain_peer_inbox_text(
        core,
        security_state,
        broker,
        &context_token,
        None,
        true,
        true,
    )
}

pub fn reveal_native_discord_overlay_view_once(
    core: &HubCoreState,
    security_state: &HubSecurityState,
    broker: &HubBrokerState,
    message_id: &str,
) -> Result<OpenedNativeOverlayText, String> {
    if !valid_peer_attachment_id(message_id) {
        return Err("This view-once message is unavailable or expired".to_owned());
    }
    let context_token = broker.active_native_manual_context_token()?;
    let mut batch = drain_peer_inbox_text(
        core,
        security_state,
        broker,
        &context_token,
        Some(message_id),
        true,
        true,
    )?;
    if batch.messages.len() != 1
        || !batch.pending_view_once.is_empty()
        || !batch.messages[0].view_once_consumed
    {
        return Err("This view-once message is unavailable or expired".to_owned());
    }
    Ok(batch.messages.remove(0))
}

pub fn drain_osl_chat_text(
    core: &HubCoreState,
    security_state: &HubSecurityState,
    broker: &HubBrokerState,
    capture_protection_ready: bool,
) -> Result<OpenedNativeOverlayTextBatch, String> {
    let context_token = broker.active_osl_chat_context_token()?;
    drain_peer_inbox_text(
        core,
        security_state,
        broker,
        &context_token,
        None,
        false,
        capture_protection_ready,
    )
}

fn drain_peer_inbox_text(
    core: &HubCoreState,
    security_state: &HubSecurityState,
    broker: &HubBrokerState,
    context_token: &str,
    reveal_view_once: Option<&str>,
    two_phase_view_once: bool,
    capture_protection_ready: bool,
) -> Result<OpenedNativeOverlayTextBatch, String> {
    let manual = broker.manual_peer_for(context_token)?;
    let context = broker.context_for(context_token)?;
    let display = security::scope_security(manual.scope.clone())?;
    let allow_messages = display.decrypt_display_enabled;
    let scope_id = native_overlay_relay_scope_id(&context.conversation_id)
        .map_err(|_| "OSL could not receive protected messages".to_owned())?;
    let verified = security::require_manual_peer_scope_approved(
        core,
        &manual.service_id,
        &manual.account_id,
        manual.person_id.clone(),
        manual.scope.clone(),
    )?;
    let (identity, client) = keyserver_transport(core)?;
    let items = client
        .get_control_inbox(&identity)
        .map_err(|_| "OSL could not receive protected messages".to_owned())?;
    let mut messages = Vec::new();
    let mut pending_view_once = Vec::new();
    let mut acknowledgments = Vec::new();
    let mut chunk_groups = BTreeMap::<String, NativeTextReassembly>::new();
    let mut reassembly_bytes = 0usize;
    for item in items {
        // Unrelated inbox traffic must never consume this bounded display
        // budget. Stop only after 64 messages for this exact friend/scope were
        // authenticated, consumed, and made ready for the current overlay.
        if messages.len().saturating_add(pending_view_once.len()) >= MAX_NATIVE_OVERLAY_OPEN_BATCH
            && acknowledgments.len() >= MAX_NATIVE_OVERLAY_OPEN_BATCH
        {
            break;
        }
        let Ok(bundle) = STANDARD.decode(&item.bundle_b64) else {
            continue;
        };
        if item.sender_id != manual.peer_osl_user_id || item.scope_id != scope_id {
            continue;
        }
        if ipc::wire_v2::is_native_overlay_ack_bundle(&bundle) {
            if acknowledgments.len() >= MAX_NATIVE_OVERLAY_OPEN_BATCH {
                continue;
            }
            let wire = format!("DPC0::{}", STANDARD.encode(&bundle));
            if verify_manual_v3_type(
                core,
                &verified,
                &wire,
                ManualWireSender::Peer,
                ipc::wire_v2::MSG_TYPE_NATIVE_OVERLAY_ACK,
            )
            .is_err()
            {
                continue;
            }
            let Ok(plaintext) = decrypt_direct_manual_v3_payload(
                core,
                &wire,
                ipc::wire_v2::MSG_TYPE_NATIVE_OVERLAY_ACK,
            ) else {
                continue;
            };
            let Ok(acknowledgment) =
                serde_json::from_slice::<NativeOverlayAcknowledgmentPayload>(&plaintext)
            else {
                continue;
            };
            let now = ipc::main_password::now_unix_secs_pub();
            if validate_native_overlay_acknowledgment(&acknowledgment, &manual, &context, now)
                .is_err()
            {
                continue;
            }
            let Ok(receipt) = record_native_overlay_acknowledgment(
                core,
                broker,
                &context,
                &manual,
                &acknowledgment,
            ) else {
                continue;
            };
            // The authenticated inbox item is removed only after the encrypted
            // receipt ledger is durably replaced. Replays are idempotent.
            if client.delete_control_inbox(&identity, &item.id).is_ok() {
                acknowledgments.push(receipt);
            }
            continue;
        }
        if !allow_messages
            || messages.len().saturating_add(pending_view_once.len())
                >= MAX_NATIVE_OVERLAY_OPEN_BATCH
            || !ipc::wire_v2::is_native_overlay_relay_bundle(&bundle)
        {
            continue;
        }
        let wire = format!("DPC0::{}", STANDARD.encode(&bundle));
        if verify_manual_v3_type(
            core,
            &verified,
            &wire,
            ManualWireSender::Peer,
            ipc::wire_v2::MSG_TYPE_NATIVE_OVERLAY_RELAY,
        )
        .is_err()
        {
            continue;
        }
        let Ok(plaintext) = decrypt_direct_manual_v3_payload(
            core,
            &wire,
            ipc::wire_v2::MSG_TYPE_NATIVE_OVERLAY_RELAY,
        ) else {
            continue;
        };
        let Ok(notice) = serde_json::from_slice::<NativeOverlayRelayNotice>(&plaintext) else {
            continue;
        };
        let now = ipc::main_password::now_unix_secs_pub();
        if validate_native_overlay_relay_notice(&notice, &manual, &context, now).is_err() {
            continue;
        }
        let Ok(payload) = authenticate_peer_prose_pointer(
            core,
            broker,
            &context_token,
            &manual.person_id,
            &notice.cover_pointer,
        ) else {
            continue;
        };
        if !capture_policy_allows_plaintext(&payload, capture_protection_ready) {
            continue;
        }
        if payload.expires_at != notice.expires_at {
            continue;
        }
        if payload.message_id != notice.message_id {
            continue;
        }
        if payload.version == PEER_PROTECTED_CHUNK_VERSION {
            let Some(logical_message_id) = payload.logical_message_id.clone() else {
                continue;
            };
            let Some(chunk_index) = payload.chunk_index else {
                continue;
            };
            if !chunk_groups.contains_key(&logical_message_id)
                && chunk_groups.len() >= MAX_NATIVE_OVERLAY_REASSEMBLY_GROUPS
            {
                continue;
            }
            if reassembly_bytes.saturating_add(payload.plaintext.len())
                > MAX_NATIVE_OVERLAY_REASSEMBLY_BYTES
            {
                continue;
            }
            let entry =
                chunk_groups
                    .entry(logical_message_id)
                    .or_insert_with(|| NativeTextReassembly {
                        template: payload.clone(),
                        chunks: BTreeMap::new(),
                        inbox_ids: Vec::new(),
                        bytes: 0,
                        invalid: false,
                    });
            if !same_native_text_group(&entry.template, &payload) {
                entry.invalid = true;
                continue;
            }
            if let Some(existing) = entry.chunks.get(&chunk_index) {
                if existing != &payload.plaintext {
                    entry.invalid = true;
                    continue;
                }
            } else {
                entry.bytes = entry.bytes.saturating_add(payload.plaintext.len());
                reassembly_bytes = reassembly_bytes.saturating_add(payload.plaintext.len());
                entry.chunks.insert(chunk_index, payload.plaintext);
            }
            entry.inbox_ids.push(item.id);
            continue;
        }
        if payload.view_once && two_phase_view_once {
            if reveal_view_once.is_none() {
                if !broker.view_once_received_was_sent(&payload.message_id, now)?
                    && send_native_overlay_acknowledgment(
                        core,
                        &verified,
                        &identity,
                        &client,
                        &manual,
                        &context,
                        &payload,
                        &scope_id,
                        NativeOverlayAcknowledgmentStatus::Received,
                    )
                    .is_ok()
                {
                    broker.record_view_once_received(
                        &payload.message_id,
                        payload.expires_at,
                        now,
                    )?;
                }
                if pending_view_once.len() < MAX_NATIVE_OVERLAY_OPEN_BATCH {
                    pending_view_once.push(PendingNativeOverlayText {
                        message_id: payload.message_id,
                        expires_at: payload.expires_at,
                        person_to_person_e2ee: true,
                    });
                }
                continue;
            }
            if reveal_view_once != Some(payload.message_id.as_str()) {
                continue;
            }
        } else if reveal_view_once.is_some() {
            continue;
        }
        let already_consumed = security::peer_message_was_consumed(
            security_state,
            manual.scope.clone(),
            &payload.message_id,
            now,
        )
        .unwrap_or(false);
        if already_consumed {
            if send_native_overlay_acknowledgment(
                core,
                &verified,
                &identity,
                &client,
                &manual,
                &context,
                &payload,
                &scope_id,
                NativeOverlayAcknowledgmentStatus::Opened,
            )
            .is_ok()
            {
                let _ = client.delete_control_inbox(&identity, &item.id);
            }
            continue;
        }
        if security::consume_peer_message(
            security_state,
            manual.scope.clone(),
            &payload.message_id,
            payload.expires_at,
            now,
        )
        .is_err()
        {
            continue;
        }
        // The receiver acknowledges only after authenticated open and durable
        // replay consumption. If posting the receipt fails, keep the relay row
        // so a later drain can retry the receipt without redisplaying content.
        if send_native_overlay_acknowledgment(
            core,
            &verified,
            &identity,
            &client,
            &manual,
            &context,
            &payload,
            &scope_id,
            NativeOverlayAcknowledgmentStatus::Opened,
        )
        .is_ok()
        {
            let _ = client.delete_control_inbox(&identity, &item.id);
        }
        if context.service_id == "osl-chat" && !payload.view_once {
            let _ = ipc::commands::cmd_osl_persist_inbound(
                &core.osl,
                context.conversation_id.clone(),
                payload.message_id.clone(),
                manual.peer_osl_user_id.clone(),
                payload.plaintext.clone(),
            );
        }
        messages.push(OpenedNativeOverlayText {
            plaintext: payload.plaintext,
            context_verified: true,
            person_to_person_e2ee: true,
            view_once_consumed: payload.view_once,
            expires_at: payload.expires_at,
        });
    }
    for (_, group) in chunk_groups {
        if messages.len().saturating_add(pending_view_once.len()) >= MAX_NATIVE_OVERLAY_OPEN_BATCH {
            continue;
        }
        let Some(plaintext) = reassemble_native_text_group(&group) else {
            continue;
        };
        let logical_message_id = group
            .template
            .logical_message_id
            .clone()
            .unwrap_or_default();
        if group.template.view_once && two_phase_view_once {
            if reveal_view_once.is_none() {
                let now = ipc::main_password::now_unix_secs_pub();
                let mut receipt_payload = group.template.clone();
                receipt_payload.message_id = logical_message_id.clone();
                receipt_payload.plaintext.clear();
                if !broker.view_once_received_was_sent(&logical_message_id, now)?
                    && send_native_overlay_acknowledgment(
                        core,
                        &verified,
                        &identity,
                        &client,
                        &manual,
                        &context,
                        &receipt_payload,
                        &scope_id,
                        NativeOverlayAcknowledgmentStatus::Received,
                    )
                    .is_ok()
                {
                    broker.record_view_once_received(
                        &logical_message_id,
                        group.template.expires_at,
                        now,
                    )?;
                }
                if pending_view_once.len() < MAX_NATIVE_OVERLAY_OPEN_BATCH {
                    pending_view_once.push(PendingNativeOverlayText {
                        message_id: logical_message_id,
                        expires_at: group.template.expires_at,
                        person_to_person_e2ee: true,
                    });
                }
                continue;
            }
            if reveal_view_once != Some(logical_message_id.as_str()) {
                continue;
            }
        } else if reveal_view_once.is_some() {
            continue;
        }
        let already_consumed = security::peer_message_was_consumed(
            security_state,
            manual.scope.clone(),
            &logical_message_id,
            ipc::main_password::now_unix_secs_pub(),
        )
        .unwrap_or(false);
        if !already_consumed
            && security::consume_peer_message(
                security_state,
                manual.scope.clone(),
                &logical_message_id,
                group.template.expires_at,
                ipc::main_password::now_unix_secs_pub(),
            )
            .is_err()
        {
            continue;
        }
        let mut logical = group.template;
        logical.message_id = logical_message_id;
        logical.plaintext = plaintext;
        if send_native_overlay_acknowledgment(
            core,
            &verified,
            &identity,
            &client,
            &manual,
            &context,
            &logical,
            &scope_id,
            NativeOverlayAcknowledgmentStatus::Opened,
        )
        .is_ok()
        {
            for inbox_id in group.inbox_ids {
                let _ = client.delete_control_inbox(&identity, &inbox_id);
            }
        }
        if !already_consumed {
            if context.service_id == "osl-chat" && !logical.view_once {
                let _ = ipc::commands::cmd_osl_persist_inbound(
                    &core.osl,
                    context.conversation_id.clone(),
                    logical.message_id.clone(),
                    manual.peer_osl_user_id.clone(),
                    logical.plaintext.clone(),
                );
            }
            messages.push(OpenedNativeOverlayText {
                plaintext: logical.plaintext,
                context_verified: true,
                person_to_person_e2ee: true,
                view_once_consumed: logical.view_once,
                expires_at: logical.expires_at,
            });
        }
    }
    let fetched = u32::try_from(messages.len().saturating_add(pending_view_once.len()))
        .unwrap_or(MAX_NATIVE_OVERLAY_OPEN_BATCH as u32);
    Ok(OpenedNativeOverlayTextBatch {
        messages,
        pending_view_once,
        acknowledgments,
        fetched,
    })
}

pub fn load_osl_chat_history(
    core: &HubCoreState,
    broker: &HubBrokerState,
) -> Result<Vec<ipc::commands::StoredMessageDto>, String> {
    let context_token = broker.active_osl_chat_context_token()?;
    let manual = broker.manual_peer_for(&context_token)?;
    let display = security::scope_security(manual.scope.clone())?;
    if !display.decrypt_display_enabled {
        return Err("Turn on decrypted text for this conversation before opening it".to_owned());
    }
    security::require_manual_peer_scope_approved(
        core,
        &manual.service_id,
        &manual.account_id,
        manual.person_id,
        manual.scope,
    )?;
    let context = broker.context_for(&context_token)?;
    ipc::commands::cmd_osl_load_channel_history(&core.osl, context.conversation_id, Some(200))
}

pub fn begin_native_overlay_attachment(
    core: &HubCoreState,
    broker: &HubBrokerState,
    original_filename: String,
    plaintext_size: u64,
    view_once: bool,
) -> Result<NativeOverlayAttachmentSealPlan, String> {
    begin_peer_attachment(
        core,
        broker,
        original_filename,
        plaintext_size,
        view_once,
        false,
    )
}

pub fn begin_osl_chat_attachment(
    core: &HubCoreState,
    broker: &HubBrokerState,
    original_filename: String,
    plaintext_size: u64,
    view_once: bool,
) -> Result<NativeOverlayAttachmentSealPlan, String> {
    begin_peer_attachment(
        core,
        broker,
        original_filename,
        plaintext_size,
        view_once,
        true,
    )
}

fn begin_peer_attachment(
    core: &HubCoreState,
    broker: &HubBrokerState,
    original_filename: String,
    plaintext_size: u64,
    view_once: bool,
    osl_chat: bool,
) -> Result<NativeOverlayAttachmentSealPlan, String> {
    const ERROR: &str = "OSL could not prepare this private attachment";
    let context_token = if osl_chat {
        broker.active_osl_chat_context_token()?
    } else {
        broker.active_native_manual_context_token()?
    };
    let manual = broker.manual_peer_for(&context_token)?;
    let context = broker.context_for(&context_token)?;
    security::require_manual_peer_scope_approved(
        core,
        &manual.service_id,
        &manual.account_id,
        manual.person_id.clone(),
        manual.scope.clone(),
    )
    .map_err(|_| ERROR.to_owned())?;
    let mime_type =
        validate_peer_attachment_filename(&original_filename).map_err(|_| ERROR.to_owned())?;
    if plaintext_size == 0 || plaintext_size > ipc::attachment_wire::MAX_STREAMED_ATTACHMENT_BYTES {
        return Err(ERROR.to_owned());
    }
    let ttl_seconds = security::scope_security(manual.scope.clone())
        .map_err(|_| ERROR.to_owned())?
        .ttl_seconds;
    if ttl_seconds == 0 || i64::from(ttl_seconds) > MAX_PEER_LIFETIME_SECONDS {
        return Err(ERROR.to_owned());
    }
    let created_at = ipc::main_password::now_unix_secs_pub();
    let expires_at = created_at
        .checked_add(i64::from(ttl_seconds))
        .ok_or_else(|| ERROR.to_owned())?;
    let mut attachment_key = [0u8; 32];
    attachment_key.copy_from_slice(&crypto::random::random_bytes(32));
    let mut content_id = [0u8; 16];
    content_id.copy_from_slice(&crypto::random::random_bytes(16));
    Ok(NativeOverlayAttachmentSealPlan {
        attachment_id: random_peer_message_id(),
        created_at,
        expires_at,
        original_filename,
        mime_type,
        plaintext_size,
        attachment_key,
        content_id,
        view_once,
        burn_scope: manual.scope,
        service_id: manual.service_id,
        account_id: manual.account_id,
        person_id: manual.person_id,
        peer_osl_user_id: manual.peer_osl_user_id,
        conversation_binding: context.conversation_id,
        self_osl_user_id: context.self_osl_id,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn deliver_native_overlay_attachment(
    core: &HubCoreState,
    broker: &HubBrokerState,
    plan: NativeOverlayAttachmentSealPlan,
    sealed_size: u64,
    ciphertext_sha256: String,
    object_id: String,
    fetch_token: String,
) -> Result<PreparedNativeOverlayAttachment, String> {
    deliver_peer_attachment(
        core,
        broker,
        plan,
        sealed_size,
        ciphertext_sha256,
        object_id,
        fetch_token,
        false,
    )
}

pub fn deliver_osl_chat_attachment(
    core: &HubCoreState,
    broker: &HubBrokerState,
    plan: NativeOverlayAttachmentSealPlan,
    sealed_size: u64,
    ciphertext_sha256: String,
    object_id: String,
    fetch_token: String,
) -> Result<PreparedNativeOverlayAttachment, String> {
    deliver_peer_attachment(
        core,
        broker,
        plan,
        sealed_size,
        ciphertext_sha256,
        object_id,
        fetch_token,
        true,
    )
}

#[allow(clippy::too_many_arguments)]
fn deliver_peer_attachment(
    core: &HubCoreState,
    broker: &HubBrokerState,
    plan: NativeOverlayAttachmentSealPlan,
    sealed_size: u64,
    ciphertext_sha256: String,
    object_id: String,
    fetch_token: String,
    osl_chat: bool,
) -> Result<PreparedNativeOverlayAttachment, String> {
    const ERROR: &str = "OSL could not deliver this private attachment";
    let fetch_token = Zeroizing::new(fetch_token);
    let context_token = if osl_chat {
        broker.active_osl_chat_context_token()?
    } else {
        broker.active_native_manual_context_token()?
    };
    let manual = broker.manual_peer_for(&context_token)?;
    let context = broker.context_for(&context_token)?;
    let verified = security::require_manual_peer_scope_approved(
        core,
        &manual.service_id,
        &manual.account_id,
        manual.person_id.clone(),
        manual.scope.clone(),
    )
    .map_err(|_| ERROR.to_owned())?;
    if plan.service_id != manual.service_id
        || plan.account_id != manual.account_id
        || plan.person_id != manual.person_id
        || plan.peer_osl_user_id != manual.peer_osl_user_id
        || plan.conversation_binding != context.conversation_id
        || plan.self_osl_user_id != context.self_osl_id
    {
        return Err("The native Discord attachment context changed".to_owned());
    }
    if sealed_size == 0
        || sealed_size > MAX_STREAMED_ATTACHMENT_BYTES
        || !canonical_hex(&ciphertext_sha256, 64)
        || !canonical_hex(&object_id, 32)
        || !canonical_hex(&fetch_token, 32)
    {
        return Err(ERROR.to_owned());
    }
    let notice = NativeOverlayAttachmentNotice {
        version: NATIVE_OVERLAY_ATTACHMENT_VERSION,
        domain: NATIVE_OVERLAY_ATTACHMENT_DOMAIN.to_owned(),
        attachment_id: plan.attachment_id.clone(),
        created_at: plan.created_at,
        expires_at: plan.expires_at,
        service_id: manual.service_id.clone(),
        conversation_binding: context.conversation_id.clone(),
        sender_osl_user_id: context.self_osl_id.clone(),
        recipient_osl_user_id: manual.peer_osl_user_id.clone(),
        original_filename: plan.original_filename.clone(),
        mime_type: plan.mime_type.clone(),
        plaintext_size: plan.plaintext_size,
        sealed_size,
        ciphertext_sha256,
        ciphertext_format: "osl-stream-attachment-v1".to_owned(),
        object_id,
        fetch_token: fetch_token.to_string(),
        attachment_key: plan.attachment_key,
        content_id: plan.content_id,
        view_once: plan.view_once,
    };
    let mut encoded = serde_json::to_vec(&notice).map_err(|_| ERROR.to_owned())?;
    let wire = encrypt_direct_manual_v3_payload(
        core,
        &verified,
        ipc::wire_v2::MSG_TYPE_ATTACHMENT,
        &encoded,
    )
    .map_err(|_| ERROR.to_owned())?;
    encoded.fill(0);
    verify_manual_v3_type(
        core,
        &verified,
        &wire,
        ManualWireSender::SelfIdentity,
        ipc::wire_v2::MSG_TYPE_ATTACHMENT,
    )
    .map_err(|_| ERROR.to_owned())?;
    let bundle = decode_typed_manual_wire(&wire, ipc::wire_v2::MSG_TYPE_ATTACHMENT)
        .map_err(|_| ERROR.to_owned())?;
    let scope_id =
        native_overlay_relay_scope_id(&context.conversation_id).map_err(|_| ERROR.to_owned())?;
    let (identity, client) = keyserver_transport(core)?;
    client
        .post_control_inbox(&identity, &manual.peer_osl_user_id, &scope_id, &bundle)
        .map_err(|_| ERROR.to_owned())?;
    Ok(PreparedNativeOverlayAttachment {
        attachment_id: plan.attachment_id.clone(),
        original_filename: plan.original_filename.clone(),
        plaintext_size: plan.plaintext_size,
        expires_at: plan.expires_at,
        view_once: plan.view_once,
        delivered_to_osl_inbox: true,
    })
}

pub fn list_native_overlay_attachments(
    core: &HubCoreState,
    security_state: &HubSecurityState,
    broker: &HubBrokerState,
) -> Result<Vec<PendingNativeOverlayAttachment>, String> {
    list_peer_attachments(core, security_state, broker, false)
}

pub fn list_osl_chat_attachments(
    core: &HubCoreState,
    security_state: &HubSecurityState,
    broker: &HubBrokerState,
) -> Result<Vec<PendingNativeOverlayAttachment>, String> {
    list_peer_attachments(core, security_state, broker, true)
}

fn list_peer_attachments(
    core: &HubCoreState,
    security_state: &HubSecurityState,
    broker: &HubBrokerState,
    osl_chat: bool,
) -> Result<Vec<PendingNativeOverlayAttachment>, String> {
    let plans = native_overlay_attachment_plans(core, security_state, broker, None, osl_chat)?;
    Ok(plans
        .into_iter()
        .map(|plan| PendingNativeOverlayAttachment {
            attachment_id: plan.attachment_id.clone(),
            original_filename: plan.original_filename.clone(),
            mime_type: plan.mime_type.clone(),
            plaintext_size: plan.plaintext_size,
            expires_at: plan.expires_at,
            view_once: plan.view_once,
        })
        .collect())
}

pub fn take_native_overlay_attachment(
    core: &HubCoreState,
    security_state: &HubSecurityState,
    broker: &HubBrokerState,
    attachment_id: &str,
) -> Result<NativeOverlayAttachmentOpenPlan, String> {
    take_peer_attachment(core, security_state, broker, attachment_id, false)
}

pub fn take_osl_chat_attachment(
    core: &HubCoreState,
    security_state: &HubSecurityState,
    broker: &HubBrokerState,
    attachment_id: &str,
) -> Result<NativeOverlayAttachmentOpenPlan, String> {
    take_peer_attachment(core, security_state, broker, attachment_id, true)
}

fn take_peer_attachment(
    core: &HubCoreState,
    security_state: &HubSecurityState,
    broker: &HubBrokerState,
    attachment_id: &str,
    osl_chat: bool,
) -> Result<NativeOverlayAttachmentOpenPlan, String> {
    if !valid_peer_attachment_id(attachment_id) {
        return Err("This private attachment could not be opened".to_owned());
    }
    native_overlay_attachment_plans(core, security_state, broker, Some(attachment_id), osl_chat)?
        .into_iter()
        .next()
        .ok_or_else(|| "This private attachment is unavailable or expired".to_owned())
}

fn native_overlay_attachment_plans(
    core: &HubCoreState,
    security_state: &HubSecurityState,
    broker: &HubBrokerState,
    wanted_id: Option<&str>,
    osl_chat: bool,
) -> Result<Vec<NativeOverlayAttachmentOpenPlan>, String> {
    const ERROR: &str = "OSL could not receive private attachments";
    let context_token = if osl_chat {
        broker.active_osl_chat_context_token()?
    } else {
        broker.active_native_manual_context_token()?
    };
    let manual = broker.manual_peer_for(&context_token)?;
    let context = broker.context_for(&context_token)?;
    if !security::scope_security(manual.scope.clone())
        .map_err(|_| ERROR.to_owned())?
        .decrypt_display_enabled
    {
        return Ok(Vec::new());
    }
    let verified = security::require_manual_peer_scope_approved(
        core,
        &manual.service_id,
        &manual.account_id,
        manual.person_id.clone(),
        manual.scope.clone(),
    )
    .map_err(|_| ERROR.to_owned())?;
    let scope_id =
        native_overlay_relay_scope_id(&context.conversation_id).map_err(|_| ERROR.to_owned())?;
    let (identity, client) = keyserver_transport(core)?;
    let items = client
        .get_control_inbox(&identity)
        .map_err(|_| ERROR.to_owned())?;
    let now = ipc::main_password::now_unix_secs_pub();
    let limit = if wanted_id.is_some() {
        1
    } else {
        MAX_NATIVE_OVERLAY_OPEN_BATCH
    };
    Ok(collect_valid_bounded(items, limit, |item| {
        if item.sender_id != manual.peer_osl_user_id || item.scope_id != scope_id {
            return None;
        }
        let Ok(bundle) = STANDARD.decode(&item.bundle_b64) else {
            return None;
        };
        if !ipc::wire_v2::is_attachment_bundle(&bundle) {
            return None;
        }
        let wire = format!("DPC0::{}", STANDARD.encode(&bundle));
        if verify_manual_v3_type(
            core,
            &verified,
            &wire,
            ManualWireSender::Peer,
            ipc::wire_v2::MSG_TYPE_ATTACHMENT,
        )
        .is_err()
        {
            return None;
        }
        let Ok(mut plaintext) =
            decrypt_direct_manual_v3_payload(core, &wire, ipc::wire_v2::MSG_TYPE_ATTACHMENT)
        else {
            return None;
        };
        let parsed = serde_json::from_slice::<NativeOverlayAttachmentNotice>(&plaintext);
        plaintext.fill(0);
        let Ok(mut notice) = parsed else {
            return None;
        };
        if validate_native_overlay_attachment_notice(&notice, &manual, &context, now).is_err()
            || wanted_id.is_some_and(|wanted| wanted != notice.attachment_id)
        {
            return None;
        }
        let consumed = security::peer_message_was_consumed(
            security_state,
            manual.scope.clone(),
            &notice.attachment_id,
            now,
        )
        .unwrap_or(false);
        if consumed {
            let _ = client.delete_control_inbox(&identity, &item.id);
            return None;
        }
        Some(NativeOverlayAttachmentOpenPlan {
            inbox_id: item.id,
            attachment_id: std::mem::take(&mut notice.attachment_id),
            original_filename: std::mem::take(&mut notice.original_filename),
            mime_type: std::mem::take(&mut notice.mime_type),
            plaintext_size: notice.plaintext_size,
            sealed_size: notice.sealed_size,
            ciphertext_sha256: std::mem::take(&mut notice.ciphertext_sha256),
            object_id: std::mem::take(&mut notice.object_id),
            fetch_token: std::mem::take(&mut notice.fetch_token),
            attachment_key: notice.attachment_key,
            view_once: notice.view_once,
            expires_at: notice.expires_at,
        })
    }))
}

fn collect_valid_bounded<T, U>(
    items: impl IntoIterator<Item = T>,
    limit: usize,
    mut validate: impl FnMut(T) -> Option<U>,
) -> Vec<U> {
    let mut output = Vec::new();
    for item in items {
        if let Some(valid) = validate(item) {
            output.push(valid);
            if output.len() >= limit {
                break;
            }
        }
    }
    output
}

pub fn commit_native_overlay_attachment_open(
    core: &HubCoreState,
    security_state: &HubSecurityState,
    broker: &HubBrokerState,
    plan: &NativeOverlayAttachmentOpenPlan,
) -> Result<(), String> {
    commit_peer_attachment_open(core, security_state, broker, plan, false)
}

pub fn commit_osl_chat_attachment_open(
    core: &HubCoreState,
    security_state: &HubSecurityState,
    broker: &HubBrokerState,
    plan: &NativeOverlayAttachmentOpenPlan,
) -> Result<(), String> {
    commit_peer_attachment_open(core, security_state, broker, plan, true)
}

fn commit_peer_attachment_open(
    core: &HubCoreState,
    security_state: &HubSecurityState,
    broker: &HubBrokerState,
    plan: &NativeOverlayAttachmentOpenPlan,
    osl_chat: bool,
) -> Result<(), String> {
    const ERROR: &str = "This private attachment could not be opened";
    let context_token = if osl_chat {
        broker.active_osl_chat_context_token()?
    } else {
        broker.active_native_manual_context_token()?
    };
    let manual = broker.manual_peer_for(&context_token)?;
    security::require_manual_peer_scope_approved(
        core,
        &manual.service_id,
        &manual.account_id,
        manual.person_id.clone(),
        manual.scope.clone(),
    )
    .map_err(|_| ERROR.to_owned())?;
    let now = ipc::main_password::now_unix_secs_pub();
    security::consume_peer_message(
        security_state,
        manual.scope,
        &plan.attachment_id,
        plan.expires_at,
        now,
    )
    .map_err(|_| ERROR.to_owned())?;
    let (identity, client) = keyserver_transport(core)?;
    // Replay state is durable before remote deletion. A failed delete cannot
    // display plaintext twice; it is retried when the inbox is listed again.
    let _ = client.delete_control_inbox(&identity, &plan.inbox_id);
    Ok(())
}

fn decode_typed_manual_wire(wire: &str, message_type: u8) -> Result<Vec<u8>, ()> {
    let body = wire.strip_prefix("DPC0::").ok_or(())?;
    let bundle = STANDARD.decode(body).map_err(|_| ())?;
    if bundle.len() > 16 * 1024
        || match message_type {
            ipc::wire_v2::MSG_TYPE_ATTACHMENT => !ipc::wire_v2::is_attachment_bundle(&bundle),
            _ => true,
        }
    {
        return Err(());
    }
    Ok(bundle)
}

fn canonical_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_peer_attachment_id(value: &str) -> bool {
    value
        .strip_prefix("peer-")
        .is_some_and(|suffix| canonical_hex(suffix, 32))
}

fn validate_native_overlay_attachment_notice(
    notice: &NativeOverlayAttachmentNotice,
    manual: &ManualPeerContext,
    context: &HubConversationContext,
    now: i64,
) -> Result<(), ()> {
    let expected_mime = validate_peer_attachment_filename(&notice.original_filename)?;
    if notice.version != NATIVE_OVERLAY_ATTACHMENT_VERSION
        || notice.domain != NATIVE_OVERLAY_ATTACHMENT_DOMAIN
        || !valid_peer_attachment_id(&notice.attachment_id)
        || notice.created_at <= 0
        || notice.expires_at <= notice.created_at
        || notice.expires_at.saturating_sub(notice.created_at) > MAX_PEER_LIFETIME_SECONDS
        || notice.created_at > now.saturating_add(MAX_PEER_CLOCK_SKEW_SECONDS)
        || notice.created_at
            < now.saturating_sub(MAX_PEER_LIFETIME_SECONDS + MAX_PEER_CLOCK_SKEW_SECONDS)
        || notice.expires_at <= now
        || notice.service_id != manual.service_id
        || notice.conversation_binding != context.conversation_id
        || notice.sender_osl_user_id != manual.peer_osl_user_id
        || notice.recipient_osl_user_id != context.self_osl_id
        || notice.mime_type != expected_mime
        || notice.plaintext_size == 0
        || notice.plaintext_size > ipc::attachment_wire::MAX_STREAMED_ATTACHMENT_BYTES
        || notice.sealed_size == 0
        || notice.sealed_size > MAX_STREAMED_ATTACHMENT_BYTES
        || !canonical_hex(&notice.ciphertext_sha256, 64)
        || notice.ciphertext_format != "osl-stream-attachment-v1"
        || !canonical_hex(&notice.object_id, 32)
        || !canonical_hex(&notice.fetch_token, 32)
        || notice.attachment_key.iter().all(|byte| *byte == 0)
        || notice.content_id.iter().all(|byte| *byte == 0)
    {
        return Err(());
    }
    Ok(())
}

fn keyserver_transport(
    core: &HubCoreState,
) -> Result<(keystore::Identity, keystore::KeyServerClient), String> {
    let identity = core
        .osl
        .identity
        .lock()
        .map_err(|_| "OSL identity state is unavailable".to_owned())?
        .clone()
        .ok_or_else(|| "OSL identity is not loaded".to_owned())?;
    let client = core
        .osl
        .keyserver
        .lock()
        .map_err(|_| "OSL key server state is unavailable".to_owned())?
        .clone()
        .ok_or_else(|| "OSL key server is unavailable".to_owned())?;
    Ok((identity, client))
}

fn send_native_overlay_acknowledgment(
    core: &HubCoreState,
    verified: &ManualPeerBinding,
    identity: &keystore::Identity,
    client: &keystore::KeyServerClient,
    manual: &ManualPeerContext,
    context: &HubConversationContext,
    original: &PeerProtectedPayload,
    scope_id: &str,
    status: NativeOverlayAcknowledgmentStatus,
) -> Result<(), String> {
    let acknowledged_at = ipc::main_password::now_unix_secs_pub();
    if original.expires_at <= acknowledged_at {
        return Err("OSL could not acknowledge the protected message".to_owned());
    }
    let acknowledgment = NativeOverlayAcknowledgmentPayload {
        version: NATIVE_OVERLAY_ACK_VERSION,
        domain: NATIVE_OVERLAY_ACK_DOMAIN.to_owned(),
        message_id: original.message_id.clone(),
        status,
        acknowledged_at,
        expires_at: original.expires_at,
        service_id: manual.service_id.clone(),
        conversation_binding: context.conversation_id.clone(),
        sender_osl_user_id: context.self_osl_id.clone(),
        recipient_osl_user_id: manual.peer_osl_user_id.clone(),
    };
    let encoded = serde_json::to_vec(&acknowledgment)
        .map_err(|_| "OSL could not acknowledge the protected message".to_owned())?;
    let wire = encrypt_direct_manual_v3_payload(
        core,
        verified,
        ipc::wire_v2::MSG_TYPE_NATIVE_OVERLAY_ACK,
        &encoded,
    )?;
    verify_manual_v3_type(
        core,
        verified,
        &wire,
        ManualWireSender::SelfIdentity,
        ipc::wire_v2::MSG_TYPE_NATIVE_OVERLAY_ACK,
    )
    .map_err(|_| "OSL could not acknowledge the protected message".to_owned())?;
    let bundle = decode_native_overlay_ack_wire(&wire)?;
    client
        .post_control_inbox(identity, &manual.peer_osl_user_id, scope_id, &bundle)
        .map(|_| ())
        .map_err(|_| "OSL could not acknowledge the protected message".to_owned())
}

fn decode_overlay_relay_wire(wire: &str) -> Result<Vec<u8>, String> {
    let body = wire
        .strip_prefix("DPC0::")
        .ok_or_else(|| "OSL could not deliver the protected message".to_owned())?;
    let bundle = STANDARD
        .decode(body)
        .map_err(|_| "OSL could not deliver the protected message".to_owned())?;
    if !ipc::wire_v2::is_native_overlay_relay_bundle(&bundle) || bundle.len() > 16 * 1024 {
        return Err("OSL could not deliver the protected message".to_owned());
    }
    Ok(bundle)
}

fn decode_native_overlay_ack_wire(wire: &str) -> Result<Vec<u8>, String> {
    let body = wire
        .strip_prefix("DPC0::")
        .ok_or_else(|| "OSL could not acknowledge the protected message".to_owned())?;
    let bundle = STANDARD
        .decode(body)
        .map_err(|_| "OSL could not acknowledge the protected message".to_owned())?;
    if !ipc::wire_v2::is_native_overlay_ack_bundle(&bundle) || bundle.len() > 16 * 1024 {
        return Err("OSL could not acknowledge the protected message".to_owned());
    }
    Ok(bundle)
}

fn native_overlay_relay_scope_id(conversation_binding: &str) -> Result<String, String> {
    validate_opaque_id(conversation_binding)
        .map_err(|_| "OSL native Discord conversation is unavailable".to_owned())?;
    let scope_id = format!("native-overlay:{conversation_binding}");
    if scope_id.len() > MAX_CONTEXT_ID_BYTES {
        return Err("OSL native Discord conversation is unavailable".to_owned());
    }
    Ok(scope_id)
}

fn validate_native_overlay_relay_notice(
    notice: &NativeOverlayRelayNotice,
    manual: &ManualPeerContext,
    context: &HubConversationContext,
    now: i64,
) -> Result<(), ()> {
    if notice.version != NATIVE_OVERLAY_RELAY_VERSION
        || notice.domain != NATIVE_OVERLAY_RELAY_DOMAIN
        || notice.created_at <= 0
        || notice.expires_at <= notice.created_at
        || notice.expires_at.saturating_sub(notice.created_at) > MAX_PEER_LIFETIME_SECONDS
        || notice.created_at > now.saturating_add(MAX_PEER_CLOCK_SKEW_SECONDS)
        || notice.expires_at <= now
        || notice.expires_at
            > now.saturating_add(MAX_PEER_LIFETIME_SECONDS + MAX_PEER_CLOCK_SKEW_SECONDS)
        || notice.service_id != manual.service_id
        || notice.conversation_binding != context.conversation_id
        || notice.sender_osl_user_id != manual.peer_osl_user_id
        || notice.recipient_osl_user_id != context.self_osl_id
        || notice.message_id.is_empty()
        || notice.message_id.len() > 96
        || notice.cover_pointer.is_empty()
        || notice.cover_pointer.len() > MAX_PROSE_COVER_BYTES
    {
        return Err(());
    }
    Ok(())
}

fn validate_native_overlay_acknowledgment(
    acknowledgment: &NativeOverlayAcknowledgmentPayload,
    manual: &ManualPeerContext,
    context: &HubConversationContext,
    now: i64,
) -> Result<(), ()> {
    if acknowledgment.version != NATIVE_OVERLAY_ACK_VERSION
        || acknowledgment.domain != NATIVE_OVERLAY_ACK_DOMAIN
        || acknowledgment.message_id.is_empty()
        || acknowledgment.message_id.len() > 96
        || acknowledgment.acknowledged_at <= 0
        || acknowledgment.acknowledged_at > now.saturating_add(MAX_PEER_CLOCK_SKEW_SECONDS)
        || acknowledgment.expires_at <= acknowledgment.acknowledged_at
        || acknowledgment.expires_at <= now
        || acknowledgment.expires_at
            > now.saturating_add(MAX_PEER_LIFETIME_SECONDS + MAX_PEER_CLOCK_SKEW_SECONDS)
        || acknowledgment.service_id != manual.service_id
        || acknowledgment.conversation_binding != context.conversation_id
        || acknowledgment.sender_osl_user_id != manual.peer_osl_user_id
        || acknowledgment.recipient_osl_user_id != context.self_osl_id
    {
        return Err(());
    }
    Ok(())
}

fn native_overlay_receipt_path() -> Result<std::path::PathBuf, String> {
    keystore::osl_config_dir()
        .map(|dir| dir.join(NATIVE_OVERLAY_RECEIPTS_FILE))
        .map_err(|_| "OSL receipt storage is unavailable".to_owned())
}

fn load_native_overlay_receipts(
    path: &Path,
    file_key: &[u8; 32],
) -> Result<NativeOverlayReceiptLedger, String> {
    let Some(bytes) = crate::atomic_file::read_recoverable_bounded(
        path,
        MAX_LOCAL_LEDGER_BYTES as u64,
        "OSL native overlay receipt ledger",
    )?
    else {
        return Ok(NativeOverlayReceiptLedger::default());
    };
    if bytes.len() > MAX_LOCAL_LEDGER_BYTES || !ipc::main_password::has_enc_magic(&bytes) {
        return Err("OSL native overlay receipt ledger is invalid or not encrypted".to_owned());
    }
    let plaintext = ipc::main_password::decrypt_at_rest(&bytes, file_key)
        .map_err(|_| "OSL native overlay receipt ledger could not be decrypted".to_owned())?;
    let ledger: NativeOverlayReceiptLedger = serde_json::from_slice(&plaintext)
        .map_err(|_| "OSL native overlay receipt ledger is malformed".to_owned())?;
    if ledger.version != NATIVE_OVERLAY_ACK_VERSION
        || ledger.records.len() > MAX_LOCAL_LEDGER_ENTRIES
    {
        return Err("OSL native overlay receipt ledger version or size is invalid".to_owned());
    }
    Ok(ledger)
}

fn write_native_overlay_receipts(
    path: &Path,
    ledger: &NativeOverlayReceiptLedger,
    file_key: &[u8; 32],
) -> Result<(), String> {
    let plaintext = serde_json::to_vec(ledger)
        .map_err(|_| "OSL native overlay receipt ledger could not be encoded".to_owned())?;
    if plaintext.len() > MAX_LOCAL_LEDGER_BYTES {
        return Err("OSL native overlay receipt ledger exceeds its storage limit".to_owned());
    }
    let encrypted = ipc::main_password::encrypt_at_rest(&plaintext, file_key)
        .map_err(|_| "OSL native overlay receipt ledger encryption failed".to_owned())?;
    crate::atomic_file::write_recoverable(path, &encrypted, "OSL native overlay receipt ledger")
}

fn prune_native_overlay_receipts(ledger: &mut NativeOverlayReceiptLedger, now: i64) {
    ledger.records.retain(|_, record| record.expires_at > now);
    while ledger.records.len() >= MAX_LOCAL_LEDGER_ENTRIES {
        let oldest = ledger
            .records
            .iter()
            .min_by_key(|(_, record)| record.expires_at)
            .map(|(id, _)| id.clone());
        let Some(oldest) = oldest else { break };
        ledger.records.remove(&oldest);
    }
}

fn record_native_overlay_sent(
    core: &HubCoreState,
    broker: &HubBrokerState,
    context: &HubConversationContext,
    manual: &ManualPeerContext,
    message_id: &str,
    expires_at: i64,
) -> Result<(), String> {
    let (_, file_key) = local_protected_identity(core, context)?;
    let _transition = broker
        .native_overlay_receipt_transition
        .lock()
        .map_err(|_| "OSL receipt state is unavailable".to_owned())?;
    let path = native_overlay_receipt_path()?;
    let now = ipc::main_password::now_unix_secs_pub();
    let mut ledger = load_native_overlay_receipts(&path, &file_key)?;
    prune_native_overlay_receipts(&mut ledger, now);
    if ledger.records.contains_key(message_id) {
        return Err("OSL could not save the protected message receipt safely".to_owned());
    }
    ledger.version = NATIVE_OVERLAY_ACK_VERSION;
    ledger.records.insert(
        message_id.to_owned(),
        NativeOverlayReceiptRecord {
            service_id: manual.service_id.clone(),
            conversation_binding: context.conversation_id.clone(),
            peer_osl_user_id: manual.peer_osl_user_id.clone(),
            expires_at,
            status: NativeOverlayReceiptStatus::Sent,
            acknowledged_at: 0,
        },
    );
    write_native_overlay_receipts(&path, &ledger, &file_key)
}

fn record_native_overlay_acknowledgment(
    core: &HubCoreState,
    broker: &HubBrokerState,
    context: &HubConversationContext,
    manual: &ManualPeerContext,
    acknowledgment: &NativeOverlayAcknowledgmentPayload,
) -> Result<NativeOverlayAcknowledgment, String> {
    let (_, file_key) = local_protected_identity(core, context)?;
    let _transition = broker
        .native_overlay_receipt_transition
        .lock()
        .map_err(|_| "OSL receipt state is unavailable".to_owned())?;
    let path = native_overlay_receipt_path()?;
    let now = ipc::main_password::now_unix_secs_pub();
    let mut ledger = load_native_overlay_receipts(&path, &file_key)?;
    prune_native_overlay_receipts(&mut ledger, now);
    let result =
        apply_native_overlay_acknowledgment_record(&mut ledger, context, manual, acknowledgment)?;
    ledger.version = NATIVE_OVERLAY_ACK_VERSION;
    write_native_overlay_receipts(&path, &ledger, &file_key)?;
    Ok(result)
}

fn apply_native_overlay_acknowledgment_record(
    ledger: &mut NativeOverlayReceiptLedger,
    context: &HubConversationContext,
    manual: &ManualPeerContext,
    acknowledgment: &NativeOverlayAcknowledgmentPayload,
) -> Result<NativeOverlayAcknowledgment, String> {
    let record = ledger
        .records
        .get_mut(&acknowledgment.message_id)
        .ok_or_else(|| "OSL receipt does not match a sent message".to_owned())?;
    if record.service_id != manual.service_id
        || record.conversation_binding != context.conversation_id
        || record.peer_osl_user_id != manual.peer_osl_user_id
        || record.expires_at != acknowledgment.expires_at
    {
        return Err("OSL receipt does not match a sent message".to_owned());
    }
    let next = match acknowledgment.status {
        NativeOverlayAcknowledgmentStatus::Received => NativeOverlayReceiptStatus::Received,
        NativeOverlayAcknowledgmentStatus::Opened => NativeOverlayReceiptStatus::Opened,
    };
    let rank = |status| match status {
        NativeOverlayReceiptStatus::Sent => 0,
        NativeOverlayReceiptStatus::Received => 1,
        NativeOverlayReceiptStatus::Opened => 2,
    };
    if rank(next) >= rank(record.status) {
        record.status = next;
        if record.acknowledged_at == 0 {
            record.acknowledged_at = acknowledgment.acknowledged_at;
        }
    }
    Ok(NativeOverlayAcknowledgment {
        message_id: acknowledgment.message_id.clone(),
        status: acknowledgment.status,
        acknowledged_at: record.acknowledged_at,
    })
}

fn decrypt_direct_manual_v3(
    core: &HubCoreState,
    wire: &str,
) -> Result<PeerProtectedPayload, String> {
    let plaintext = decrypt_direct_manual_v3_payload(core, wire, ipc::wire_v2::MSG_TYPE_CONTENT)?;
    if plaintext.starts_with(PEER_PROTECTED_CHUNK_PREFIX) {
        decode_peer_protected_chunk(&plaintext)
    } else {
        serde_json::from_slice(&plaintext)
            .map_err(|_| "This encrypted message could not be opened".to_owned())
    }
}

fn encode_peer_protected_chunk(payload: &PeerProtectedPayload) -> Result<Vec<u8>, String> {
    let logical_message_id = payload
        .logical_message_id
        .as_deref()
        .ok_or_else(|| "OSL could not prepare a single manual peer message".to_owned())?;
    let whole_sha256 = payload
        .whole_sha256
        .as_deref()
        .ok_or_else(|| "OSL could not prepare a single manual peer message".to_owned())?;
    let chunk_index = payload
        .chunk_index
        .ok_or_else(|| "OSL could not prepare a single manual peer message".to_owned())?;
    let chunk_count = payload
        .chunk_count
        .ok_or_else(|| "OSL could not prepare a single manual peer message".to_owned())?;
    let mut encoded = Vec::with_capacity(payload.plaintext.len().saturating_add(1024));
    encoded.extend_from_slice(PEER_PROTECTED_CHUNK_PREFIX);
    encoded.extend_from_slice(&payload.created_at.to_be_bytes());
    encoded.extend_from_slice(&payload.expires_at.to_be_bytes());
    encoded.extend_from_slice(&chunk_index.to_be_bytes());
    encoded.extend_from_slice(&chunk_count.to_be_bytes());
    encoded.push(u8::from(payload.view_once));
    encoded.push(u8::from(payload.require_capture_protection));
    for value in [
        payload.message_id.as_str(),
        payload.service_id.as_str(),
        payload.conversation_binding.as_str(),
        payload.sender_osl_user_id.as_str(),
        payload.recipient_osl_user_id.as_str(),
        logical_message_id,
        whole_sha256,
        payload.plaintext.as_str(),
    ] {
        let length = u32::try_from(value.len())
            .map_err(|_| "OSL could not prepare a single manual peer message".to_owned())?;
        encoded.extend_from_slice(&length.to_be_bytes());
        encoded.extend_from_slice(value.as_bytes());
    }
    Ok(encoded)
}

fn decode_peer_protected_chunk(encoded: &[u8]) -> Result<PeerProtectedPayload, String> {
    const ERROR: &str = "This encrypted message could not be opened";
    if encoded.len() > MAX_NATIVE_OVERLAY_CHUNK_BYTES.saturating_add(2048)
        || !encoded.starts_with(PEER_PROTECTED_CHUNK_PREFIX)
    {
        return Err(ERROR.to_owned());
    }
    let mut offset = PEER_PROTECTED_CHUNK_PREFIX.len();
    let created_at = read_i64(encoded, &mut offset)?;
    let expires_at = read_i64(encoded, &mut offset)?;
    let chunk_index = read_u16(encoded, &mut offset)?;
    let chunk_count = read_u16(encoded, &mut offset)?;
    let view_once = match encoded.get(offset).copied() {
        Some(0) => false,
        Some(1) => true,
        _ => return Err(ERROR.to_owned()),
    };
    offset = offset.saturating_add(1);
    let require_capture_protection = match encoded.get(offset).copied() {
        Some(0) => false,
        Some(1) => true,
        _ => return Err(ERROR.to_owned()),
    };
    offset = offset.saturating_add(1);
    let message_id = read_bounded_utf8(encoded, &mut offset, 96)?;
    let service_id = read_bounded_utf8(encoded, &mut offset, 32)?;
    let conversation_binding = read_bounded_utf8(encoded, &mut offset, MAX_CONTEXT_ID_BYTES)?;
    let sender_osl_user_id = read_bounded_utf8(encoded, &mut offset, 160)?;
    let recipient_osl_user_id = read_bounded_utf8(encoded, &mut offset, 160)?;
    let logical_message_id = read_bounded_utf8(encoded, &mut offset, 96)?;
    let whole_sha256 = read_bounded_utf8(encoded, &mut offset, 64)?;
    let plaintext = read_bounded_utf8(encoded, &mut offset, MAX_NATIVE_OVERLAY_CHUNK_BYTES)?;
    if offset != encoded.len() {
        return Err(ERROR.to_owned());
    }
    Ok(PeerProtectedPayload {
        version: PEER_PROTECTED_CHUNK_VERSION,
        message_id,
        created_at,
        expires_at,
        service_id,
        conversation_binding,
        sender_osl_user_id,
        recipient_osl_user_id,
        plaintext,
        view_once,
        require_capture_protection,
        logical_message_id: Some(logical_message_id),
        chunk_index: Some(chunk_index),
        chunk_count: Some(chunk_count),
        whole_sha256: Some(whole_sha256),
    })
}

fn read_u16(input: &[u8], offset: &mut usize) -> Result<u16, String> {
    let end = offset.saturating_add(2);
    let bytes: [u8; 2] = input
        .get(*offset..end)
        .and_then(|value| value.try_into().ok())
        .ok_or_else(|| "This encrypted message could not be opened".to_owned())?;
    *offset = end;
    Ok(u16::from_be_bytes(bytes))
}

fn read_i64(input: &[u8], offset: &mut usize) -> Result<i64, String> {
    let end = offset.saturating_add(8);
    let bytes: [u8; 8] = input
        .get(*offset..end)
        .and_then(|value| value.try_into().ok())
        .ok_or_else(|| "This encrypted message could not be opened".to_owned())?;
    *offset = end;
    Ok(i64::from_be_bytes(bytes))
}

fn read_bounded_utf8(input: &[u8], offset: &mut usize, maximum: usize) -> Result<String, String> {
    let length_end = offset.saturating_add(4);
    let length_bytes: [u8; 4] = input
        .get(*offset..length_end)
        .and_then(|value| value.try_into().ok())
        .ok_or_else(|| "This encrypted message could not be opened".to_owned())?;
    let length = usize::try_from(u32::from_be_bytes(length_bytes))
        .map_err(|_| "This encrypted message could not be opened".to_owned())?;
    if length > maximum {
        return Err("This encrypted message could not be opened".to_owned());
    }
    let end = length_end.saturating_add(length);
    let value = std::str::from_utf8(
        input
            .get(length_end..end)
            .ok_or_else(|| "This encrypted message could not be opened".to_owned())?,
    )
    .map_err(|_| "This encrypted message could not be opened".to_owned())?
    .to_owned();
    *offset = end;
    Ok(value)
}

fn decrypt_direct_manual_v3_payload(
    core: &HubCoreState,
    wire: &str,
    expected_message_type: u8,
) -> Result<Vec<u8>, String> {
    let identity = core
        .osl
        .identity
        .lock()
        .map_err(|_| "OSL identity state is unavailable".to_owned())?
        .clone()
        .ok_or_else(|| "OSL identity is not loaded".to_owned())?;
    let opened = ipc::wire_v2::decrypt_v3(
        wire,
        &identity.x25519_secret,
        &identity.mlkem_decapsulation_key(),
    )
    .map_err(|_| "This encrypted message could not be opened".to_owned())?;
    if opened.msg_type != expected_message_type {
        return Err("This encrypted message could not be opened".to_owned());
    }
    Ok(opened.plaintext)
}

fn validate_peer_protected_payload(
    payload: &PeerProtectedPayload,
    manual: &ManualPeerContext,
    context: &HubConversationContext,
    now: i64,
) -> Result<(), String> {
    let valid_message_id = payload
        .message_id
        .strip_prefix("peer-")
        .is_some_and(|value| {
            value.len() == 32
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        });
    let chunk_valid = if payload.version == PEER_PROTECTED_CHUNK_VERSION {
        ((manual.service_id == "discord" && manual.account_id.starts_with("native-discord-"))
            || (manual.service_id == "osl-chat" && manual.account_id == "osl-main"))
            && payload.plaintext.len() <= MAX_NATIVE_OVERLAY_CHUNK_BYTES
            && payload
                .logical_message_id
                .as_deref()
                .is_some_and(valid_peer_message_id)
            && payload.chunk_count.is_some_and(|count| {
                count > 0 && usize::from(count) <= MAX_NATIVE_OVERLAY_TEXT_CHUNKS
            })
            && payload
                .chunk_index
                .zip(payload.chunk_count)
                .is_some_and(|(index, count)| index < count)
            && payload
                .whole_sha256
                .as_deref()
                .is_some_and(|value| canonical_hex(value, 64))
    } else {
        payload.version == PEER_PROTECTED_VERSION
            && payload.plaintext.len() <= MAX_TEXT_BYTES
            && payload.logical_message_id.is_none()
            && payload.chunk_index.is_none()
            && payload.chunk_count.is_none()
            && payload.whole_sha256.is_none()
    };
    if !chunk_valid
        || !valid_message_id
        || payload.created_at <= 0
        || payload.expires_at <= payload.created_at
        || payload.expires_at.saturating_sub(payload.created_at) > MAX_PEER_LIFETIME_SECONDS
        || payload.created_at > now.saturating_add(MAX_PEER_CLOCK_SKEW_SECONDS)
        || payload.created_at
            < now.saturating_sub(MAX_PEER_LIFETIME_SECONDS + MAX_PEER_CLOCK_SKEW_SECONDS)
        || payload.expires_at <= now
        || payload.expires_at
            > now.saturating_add(MAX_PEER_LIFETIME_SECONDS + MAX_PEER_CLOCK_SKEW_SECONDS)
        || payload.service_id != manual.service_id
        || payload.conversation_binding != context.conversation_id
        || payload.sender_osl_user_id != manual.peer_osl_user_id
        || payload.recipient_osl_user_id != context.self_osl_id
        || payload.plaintext.is_empty()
    {
        return Err("This encrypted message could not be opened".to_owned());
    }
    Ok(())
}

fn capture_policy_allows_plaintext(
    payload: &PeerProtectedPayload,
    capture_protection_ready: bool,
) -> bool {
    !payload.require_capture_protection || capture_protection_ready
}

fn valid_peer_message_id(value: &str) -> bool {
    value.strip_prefix("peer-").is_some_and(|suffix| {
        suffix.len() == 32
            && suffix
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn same_native_text_group(left: &PeerProtectedPayload, right: &PeerProtectedPayload) -> bool {
    left.version == right.version
        && left.created_at == right.created_at
        && left.expires_at == right.expires_at
        && left.service_id == right.service_id
        && left.conversation_binding == right.conversation_binding
        && left.sender_osl_user_id == right.sender_osl_user_id
        && left.recipient_osl_user_id == right.recipient_osl_user_id
        && left.view_once == right.view_once
        && left.require_capture_protection == right.require_capture_protection
        && left.logical_message_id == right.logical_message_id
        && left.chunk_count == right.chunk_count
        && left.whole_sha256 == right.whole_sha256
}

fn reassemble_native_text_group(group: &NativeTextReassembly) -> Option<String> {
    if group.invalid || group.bytes > MAX_NATIVE_OVERLAY_LOGICAL_TEXT_BYTES {
        return None;
    }
    let chunk_count = group.template.chunk_count?;
    if group.chunks.len() != usize::from(chunk_count) {
        return None;
    }
    let mut plaintext = String::with_capacity(group.bytes);
    for index in 0..chunk_count {
        plaintext.push_str(group.chunks.get(&index)?);
    }
    if plaintext.is_empty()
        || plaintext.len() > MAX_NATIVE_OVERLAY_LOGICAL_TEXT_BYTES
        || group.template.whole_sha256.as_deref()? != sha256_hex(plaintext.as_bytes())
    {
        return None;
    }
    Some(plaintext)
}

fn peer_prose_token_or_generic(
    result: Result<
        Option<ipc::prose_token::ProseTokenRecvOutput>,
        ipc::prose_token::ProseTokenError,
    >,
) -> Result<ipc::prose_token::ProseTokenRecvOutput, String> {
    result
        .ok()
        .flatten()
        .ok_or_else(|| "This encrypted message could not be opened".to_owned())
}

struct InspectedV3Content {
    sender_ik: [u8; 32],
    recipient_hashes: Vec<[u8; 8]>,
}

#[cfg(test)]
fn inspect_v3_content_wire(wire: &str) -> Result<InspectedV3Content, ()> {
    inspect_v3_wire(wire, ipc::wire_v2::MSG_TYPE_CONTENT)
}

fn inspect_v3_wire(wire: &str, expected_message_type: u8) -> Result<InspectedV3Content, ()> {
    let body = wire.strip_prefix("DPC0::").ok_or(())?;
    let raw = STANDARD.decode(body).map_err(|_| ())?;
    if raw.len() < 35 || raw[0] != 3 || raw[1] != expected_message_type {
        return Err(());
    }
    let recipient_count = raw[34] as usize;
    if recipient_count == 0 {
        return Err(());
    }
    let slots_bytes = recipient_count
        .checked_mul(ipc::wire_v2::SLOT_V3_BYTES)
        .ok_or(())?;
    let slots_end = 35usize.checked_add(slots_bytes).ok_or(())?;
    if raw.len() < slots_end + 12 + 16 {
        return Err(());
    }
    let mut sender_ik = [0u8; 32];
    sender_ik.copy_from_slice(&raw[2..34]);
    let mut recipient_hashes = Vec::with_capacity(recipient_count);
    for slot in 0..recipient_count {
        let start = 35 + slot * ipc::wire_v2::SLOT_V3_BYTES;
        let mut hash = [0u8; 8];
        hash.copy_from_slice(&raw[start..start + 8]);
        recipient_hashes.push(hash);
    }
    Ok(InspectedV3Content {
        sender_ik,
        recipient_hashes,
    })
}

#[derive(Clone, Copy)]
enum ManualWireSender {
    SelfIdentity,
    Peer,
}

fn verify_manual_v3(
    core: &HubCoreState,
    peer: &ManualPeerBinding,
    wire: &str,
    sender: ManualWireSender,
) -> Result<(), ()> {
    verify_manual_v3_type(core, peer, wire, sender, ipc::wire_v2::MSG_TYPE_CONTENT)
}

fn verify_manual_v3_type(
    core: &HubCoreState,
    peer: &ManualPeerBinding,
    wire: &str,
    sender: ManualWireSender,
    expected_message_type: u8,
) -> Result<(), ()> {
    let inspected = inspect_v3_wire(wire, expected_message_type)?;
    let self_public = {
        let identity = core.osl.identity.lock().map_err(|_| ())?;
        *identity.as_ref().ok_or(())?.x25519_public.as_bytes()
    };
    let expected_sender = match sender {
        ManualWireSender::SelfIdentity => &self_public,
        ManualWireSender::Peer => &peer.peer_x25519_public,
    };
    verify_inspected_manual_v3(
        &inspected,
        &self_public,
        &peer.peer_x25519_public,
        expected_sender,
    )
}

fn verify_inspected_manual_v3(
    inspected: &InspectedV3Content,
    self_public: &[u8; 32],
    peer_public: &[u8; 32],
    expected_sender: &[u8; 32],
) -> Result<(), ()> {
    if inspected.recipient_hashes.len() != 2
        || !constant_time_eq_32(&inspected.sender_ik, expected_sender)
        || constant_time_eq_32(self_public, peer_public)
    {
        return Err(());
    }
    let self_hash =
        ipc::wire_v2::pubkey_hash_prefix(&crypto::x25519::PublicKey::from_bytes(*self_public));
    let peer_hash =
        ipc::wire_v2::pubkey_hash_prefix(&crypto::x25519::PublicKey::from_bytes(*peer_public));
    let first = inspected.recipient_hashes[0];
    let second = inspected.recipient_hashes[1];
    if !((first == self_hash && second == peer_hash) || (first == peer_hash && second == self_hash))
    {
        return Err(());
    }
    Ok(())
}

fn constant_time_eq_32(left: &[u8; 32], right: &[u8; 32]) -> bool {
    let mut difference = 0u8;
    for index in 0..32 {
        difference |= left[index] ^ right[index];
    }
    difference == 0
}

/// Encrypt text to this identity's own key and persist a context-bound local
/// ledger entry. This gives the single-sided protected composer a real
/// ciphertext path before a peer is linked, while remaining explicitly
/// distinct from peer E2EE.
pub fn prepare_local_protected_text(
    core: &HubCoreState,
    broker: &HubBrokerState,
    context_token: &str,
    plaintext: String,
) -> Result<PreparedLocalProtectedMessage, String> {
    prepare_local_protected_text_with_policy(core, broker, context_token, plaintext, false)
}

pub fn prepare_local_protected_text_with_policy(
    core: &HubCoreState,
    broker: &HubBrokerState,
    context_token: &str,
    plaintext: String,
    view_once: bool,
) -> Result<PreparedLocalProtectedMessage, String> {
    let dir = keystore::osl_config_dir()
        .map_err(|_| "OSL Privacy account storage is unavailable".to_owned())?;
    prepare_local_protected_text_in_dir(core, broker, context_token, plaintext, view_once, &dir)
}

fn prepare_local_protected_text_in_dir(
    core: &HubCoreState,
    broker: &HubBrokerState,
    context_token: &str,
    plaintext: String,
    view_once: bool,
    dir: &Path,
) -> Result<PreparedLocalProtectedMessage, String> {
    if plaintext.is_empty() || plaintext.len() > MAX_TEXT_BYTES {
        return Err(format!(
            "OSL protected plaintext must be between 1 and {MAX_TEXT_BYTES} bytes"
        ));
    }
    let context = broker.context_for(context_token)?;
    let (identity, file_key) = local_protected_identity(core, &context)?;
    let _transition = broker
        .local_protected_transition
        .lock()
        .map_err(|_| "OSL protected state is unavailable".to_owned())?;
    let context_binding = local_context_binding(&context);
    let local_message_id = random_local_message_id();
    let payload = LocalProtectedPayload {
        version: LOCAL_PROTECTED_VERSION,
        local_message_id: local_message_id.clone(),
        context_binding: context_binding.clone(),
        plaintext,
        view_once,
    };
    let payload_bytes = serde_json::to_vec(&payload)
        .map_err(|_| "OSL protected payload could not be encoded".to_owned())?;
    let capsule = ipc::wire_v2::encrypt_v2(
        &payload_bytes,
        &[identity.x25519_public],
        LOCAL_PROTECTED_MESSAGE_TYPE,
        &identity.x25519_secret,
    )
    .map_err(|_| "OSL could not encrypt the protected message".to_owned())?;

    let path = dir.join(LOCAL_PROTECTED_FILE);
    let mut ledger = load_local_ledger(&path, &file_key)?;
    let ttl_scope = scope_input(&context)?
        .try_into()
        .map_err(|_| "OSL protected scope is invalid".to_owned())?;
    let ttl_key = ipc::scope::Scope::storage_key(&ttl_scope);
    let ttl_file = ipc::scope_ttl_file::load_scope_ttls(&dir.join("scope_ttl.json"));
    let ttl_seconds = ipc::scope_ttl_file::get_scope_ttl(&ttl_file, &ttl_key);
    let now = ipc::main_password::now_unix_secs_pub();
    if ttl_seconds > 0 {
        ledger
            .records
            .retain(|_, record| now.saturating_sub(record.created_at) <= i64::from(ttl_seconds));
    }
    if ledger.records.len() >= MAX_LOCAL_LEDGER_ENTRIES {
        let oldest = ledger
            .records
            .iter()
            .min_by_key(|(_, record)| record.created_at)
            .map(|(id, _)| id.clone());
        if let Some(oldest) = oldest {
            ledger.records.remove(&oldest);
        }
    }
    ledger.version = LOCAL_PROTECTED_VERSION;
    ledger.records.insert(
        local_message_id.clone(),
        LocalProtectedRecord {
            context_binding,
            capsule_sha256: sha256_hex(capsule.as_bytes()),
            created_at: now,
            last_opened_at: None,
            view_once,
        },
    );
    write_local_ledger(&path, &ledger, &file_key)?;

    Ok(PreparedLocalProtectedMessage {
        capsule,
        local_message_id,
        protection: LOCAL_PROTECTED_LABEL,
        person_to_person_e2ee: false,
        state_persisted: true,
        view_once,
    })
}

pub fn prepare_encrypted_attachment(
    core: &HubCoreState,
    broker: &HubBrokerState,
    context_token: &str,
    original_bytes_b64: String,
    original_filename: String,
) -> Result<PreparedHubAttachment, String> {
    broker.require_peer_messaging_context(context_token)?;
    if original_bytes_b64.is_empty() || original_bytes_b64.len() > MAX_ATTACHMENT_B64_BYTES {
        return Err("The selected attachment is empty or too large".to_owned());
    }
    if original_filename.is_empty()
        || original_filename.len() > ipc::attachment_wire::MAX_FILENAME_LEN
        || original_filename
            .chars()
            .any(|character| character.is_control())
    {
        return Err("The selected attachment filename is invalid".to_owned());
    }
    let original_mime = ipc::attachment_wire::mime_for_filename(&original_filename)
        .ok_or_else(|| "This attachment type is not supported".to_owned())?;
    let context = broker.context_for(context_token)?;
    let random = crypto::random::random_bytes(16);
    let transport_filename = format!("osl-{}.mp4", short_hex(&random));
    let sealed = ipc::commands::cmd_osl_seal_attachment_with_cover_v3(
        &core.osl,
        scope_input(&context)?,
        context.participant_osl_ids,
        context.self_osl_id,
        original_bytes_b64,
        original_filename,
        transport_filename.clone(),
    )?;
    Ok(PreparedHubAttachment {
        sealed_b64: sealed.sealed_b64,
        transport_filename,
        transport_mime_type: "video/mp4",
        original_mime_type: original_mime.to_owned(),
        ciphertext_prepared: true,
        automatic_service_upload: false,
    })
}

/// Prepare one attachment for an already-verified manual peer. This internal
/// Rust API intentionally uses byte vectors rather than renderer/base64 DTOs.
/// The native adapter must deliver `envelope_wire` alongside `sealed_bytes`.
pub fn prepare_peer_attachment(
    core: &HubCoreState,
    broker: &HubBrokerState,
    context_token: &str,
    original_bytes: Vec<u8>,
    original_filename: String,
    view_once: bool,
) -> Result<PreparedPeerAttachment, String> {
    prepare_peer_attachment_at(
        core,
        broker,
        context_token,
        original_bytes,
        original_filename,
        view_once,
        ipc::main_password::now_unix_secs_pub(),
    )
}

fn prepare_peer_attachment_at(
    core: &HubCoreState,
    broker: &HubBrokerState,
    context_token: &str,
    original_bytes: Vec<u8>,
    original_filename: String,
    view_once: bool,
    now: i64,
) -> Result<PreparedPeerAttachment, String> {
    const PREPARE_ERROR: &str = "OSL could not prepare a single manual peer attachment";
    let manual = broker.manual_peer_for(context_token)?;
    let verified = security::require_manual_peer_scope_approved(
        core,
        &manual.service_id,
        &manual.account_id,
        manual.person_id.clone(),
        manual.scope.clone(),
    )?;
    let context = broker.context_for(context_token)?;
    let mime_type = validate_peer_attachment_filename(&original_filename)
        .map_err(|_| PREPARE_ERROR.to_owned())?;
    if original_bytes.is_empty()
        || original_bytes.len() > ipc::attachment_wire::MAX_ATTACHMENT_BYTES
    {
        return Err(PREPARE_ERROR.to_owned());
    }
    let ttl_seconds = security::scope_security(manual.scope.clone())?.ttl_seconds;
    if ttl_seconds == 0 || i64::from(ttl_seconds) > MAX_PEER_LIFETIME_SECONDS {
        return Err(PREPARE_ERROR.to_owned());
    }
    let expires_at = now
        .checked_add(i64::from(ttl_seconds))
        .ok_or_else(|| PREPARE_ERROR.to_owned())?;
    let attachment_id = random_peer_message_id();
    let transport_filename = format!(
        "osl-{}.mp4",
        attachment_id.strip_prefix("peer-").unwrap_or("attachment")
    );
    let mut attachment_key = [0u8; 32];
    attachment_key.copy_from_slice(&crypto::random::random_bytes(32));
    let sealed_bytes = ipc::attachment_wire::seal_attachment_v3(
        crypto::aead::Key::from_bytes(attachment_key),
        &original_bytes,
        &original_filename,
        &[],
    )
    .map_err(|_| PREPARE_ERROR.to_owned())?;
    let mut payload = PeerAttachmentPayload {
        version: PEER_ATTACHMENT_VERSION,
        attachment_id,
        created_at: now,
        expires_at,
        service_id: manual.service_id.clone(),
        conversation_binding: context.conversation_id.clone(),
        sender_osl_user_id: context.self_osl_id.clone(),
        recipient_osl_user_id: manual.peer_osl_user_id.clone(),
        original_filename,
        mime_type,
        plaintext_size: original_bytes.len() as u64,
        transport_filename: transport_filename.clone(),
        ciphertext_sha256: sha256_hex(&sealed_bytes),
        ciphertext_format: "osl-attachment-v3".to_owned(),
        key_algorithm: "xchacha20-poly1305-ietf".to_owned(),
        attachment_key,
        view_once,
    };
    let mut payload_bytes = serde_json::to_vec(&payload).map_err(|_| PREPARE_ERROR.to_owned())?;
    payload.attachment_key.fill(0);
    attachment_key.fill(0);
    let envelope_wire = encrypt_direct_manual_v3_payload(
        core,
        &verified,
        ipc::wire_v2::MSG_TYPE_ATTACHMENT,
        &payload_bytes,
    )
    .map_err(|_| PREPARE_ERROR.to_owned())?;
    payload_bytes.fill(0);
    verify_manual_v3_type(
        core,
        &verified,
        &envelope_wire,
        ManualWireSender::SelfIdentity,
        ipc::wire_v2::MSG_TYPE_ATTACHMENT,
    )
    .map_err(|_| PREPARE_ERROR.to_owned())?;
    Ok(PreparedPeerAttachment {
        sealed_bytes,
        envelope_wire,
        transport_filename,
        expires_at,
        view_once,
    })
}

/// Open one manual-peer attachment. Replay state is committed only after all
/// metadata, ciphertext-hash, and AEAD checks pass, but always before plaintext
/// leaves this function (including view-once content).
pub fn open_peer_attachment(
    core: &HubCoreState,
    security_state: &HubSecurityState,
    broker: &HubBrokerState,
    context_token: &str,
    sender_person_id: String,
    envelope_wire: String,
    sealed_bytes: Vec<u8>,
) -> Result<OpenedPeerAttachment, String> {
    const OPEN_ERROR: &str = "This encrypted attachment could not be opened";
    if envelope_wire.is_empty()
        || envelope_wire.len() > 256 * 1024
        || sealed_bytes.is_empty()
        || sealed_bytes.len() > MAX_ATTACHMENT_B64_BYTES
    {
        return Err(OPEN_ERROR.to_owned());
    }
    let manual = broker.manual_peer_for(context_token)?;
    if sender_person_id != manual.person_id {
        return Err(OPEN_ERROR.to_owned());
    }
    let verified = security::require_manual_peer_scope_approved(
        core,
        &manual.service_id,
        &manual.account_id,
        manual.person_id.clone(),
        manual.scope.clone(),
    )
    .map_err(|_| OPEN_ERROR.to_owned())?;
    if verified.peer_osl_user_id != manual.peer_osl_user_id {
        return Err(OPEN_ERROR.to_owned());
    }
    if !security::scope_security(manual.scope.clone())
        .map_err(|_| OPEN_ERROR.to_owned())?
        .decrypt_display_enabled
    {
        return Err(OPEN_ERROR.to_owned());
    }
    verify_manual_v3_type(
        core,
        &verified,
        &envelope_wire,
        ManualWireSender::Peer,
        ipc::wire_v2::MSG_TYPE_ATTACHMENT,
    )
    .map_err(|_| OPEN_ERROR.to_owned())?;
    let mut payload_bytes =
        decrypt_direct_manual_v3_payload(core, &envelope_wire, ipc::wire_v2::MSG_TYPE_ATTACHMENT)
            .map_err(|_| OPEN_ERROR.to_owned())?;
    let mut payload: PeerAttachmentPayload =
        serde_json::from_slice(&payload_bytes).map_err(|_| OPEN_ERROR.to_owned())?;
    payload_bytes.fill(0);
    let context = broker.context_for(context_token)?;
    let now = ipc::main_password::now_unix_secs_pub();
    validate_peer_attachment_payload(&payload, &manual, &context, &sealed_bytes, now)
        .map_err(|_| OPEN_ERROR.to_owned())?;
    let (cover, embedded_filename, ciphertext) =
        ipc::attachment_wire::open_attachment_v3_split(&sealed_bytes)
            .map_err(|_| OPEN_ERROR.to_owned())?;
    if !cover.is_empty() || embedded_filename != payload.original_filename {
        payload.attachment_key.fill(0);
        return Err(OPEN_ERROR.to_owned());
    }
    let plaintext = crypto::attachment::decrypt_attachment(
        crypto::aead::Key::from_bytes(payload.attachment_key),
        &ciphertext,
    )
    .map_err(|_| OPEN_ERROR.to_owned())?;
    payload.attachment_key.fill(0);
    if plaintext.len() as u64 != payload.plaintext_size {
        return Err(OPEN_ERROR.to_owned());
    }
    security::consume_peer_message(
        security_state,
        manual.scope,
        &payload.attachment_id,
        payload.expires_at,
        now,
    )
    .map_err(|_| OPEN_ERROR.to_owned())?;
    let original_filename = std::mem::take(&mut payload.original_filename);
    let mime_type = std::mem::take(&mut payload.mime_type);
    let attachment_id = std::mem::take(&mut payload.attachment_id);
    Ok(OpenedPeerAttachment {
        plaintext,
        original_filename,
        mime_type,
        attachment_id,
        view_once_consumed: payload.view_once,
    })
}

fn validate_peer_attachment_filename(original_filename: &str) -> Result<String, ()> {
    if original_filename.is_empty()
        || original_filename.len() > ipc::attachment_wire::MAX_FILENAME_LEN
        || original_filename.chars().any(char::is_control)
    {
        return Err(());
    }
    ipc::attachment_wire::mime_for_filename(original_filename)
        .map(str::to_owned)
        .ok_or(())
}

fn validate_peer_attachment_payload(
    payload: &PeerAttachmentPayload,
    manual: &ManualPeerContext,
    context: &HubConversationContext,
    sealed_bytes: &[u8],
    now: i64,
) -> Result<(), ()> {
    let valid_attachment_id = payload
        .attachment_id
        .strip_prefix("peer-")
        .is_some_and(|value| {
            value.len() == 32
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        });
    let expected_mime = validate_peer_attachment_filename(&payload.original_filename)?;
    if payload.version != PEER_ATTACHMENT_VERSION
        || !valid_attachment_id
        || payload.created_at <= 0
        || payload.expires_at <= payload.created_at
        || payload.expires_at.saturating_sub(payload.created_at) > MAX_PEER_LIFETIME_SECONDS
        || payload.created_at > now.saturating_add(MAX_PEER_CLOCK_SKEW_SECONDS)
        || payload.created_at
            < now.saturating_sub(MAX_PEER_LIFETIME_SECONDS + MAX_PEER_CLOCK_SKEW_SECONDS)
        || payload.expires_at <= now
        || payload.expires_at
            > now.saturating_add(MAX_PEER_LIFETIME_SECONDS + MAX_PEER_CLOCK_SKEW_SECONDS)
        || payload.service_id != manual.service_id
        || payload.conversation_binding != context.conversation_id
        || payload.sender_osl_user_id != manual.peer_osl_user_id
        || payload.recipient_osl_user_id != context.self_osl_id
        || payload.mime_type != expected_mime
        || payload.plaintext_size == 0
        || payload.plaintext_size > ipc::attachment_wire::MAX_ATTACHMENT_BYTES as u64
        || payload.transport_filename
            != format!(
                "osl-{}.mp4",
                payload
                    .attachment_id
                    .strip_prefix("peer-")
                    .unwrap_or("attachment")
            )
        || payload.ciphertext_sha256 != sha256_hex(sealed_bytes)
        || payload.ciphertext_format != "osl-attachment-v3"
        || payload.key_algorithm != "xchacha20-poly1305-ietf"
        || payload.attachment_key.iter().all(|byte| *byte == 0)
    {
        return Err(());
    }
    Ok(())
}

pub fn open_encrypted_attachment(
    core: &HubCoreState,
    broker: &HubBrokerState,
    context_token: &str,
    sender_osl_id: String,
    service_message_id: Option<String>,
    sealed_b64: String,
) -> Result<OpenedHubAttachment, String> {
    broker.require_peer_messaging_context(context_token)?;
    validate_context_id(&sender_osl_id, "sender OSL id")?;
    if sealed_b64.is_empty() || sealed_b64.len() > MAX_ATTACHMENT_B64_BYTES {
        return Err("The encrypted attachment is empty or too large".to_owned());
    }
    let context = broker.context_for(context_token)?;
    let opened = ipc::commands::cmd_osl_open_attachment_v2(
        &core.osl,
        sender_osl_id,
        Some(scope_input(&context)?),
        sealed_b64,
        None,
        service_message_id,
    )?;
    Ok(OpenedHubAttachment {
        plaintext_b64: opened.plaintext_b64,
        original_filename: opened.original_filename,
        mime_type: opened.mime_type,
        context_verified: true,
    })
}

pub fn decrypt_capsule(
    core: &HubCoreState,
    broker: &HubBrokerState,
    context_token: &str,
    sender_osl_id: String,
    service_message_id: Option<String>,
    capsule: String,
) -> Result<String, String> {
    broker.require_peer_messaging_context(context_token)?;
    validate_context_id(&sender_osl_id, "sender OSL id")?;
    if capsule.len() > 256 * 1024 {
        return Err("The encrypted message is too large to open on this device".to_owned());
    }
    let scope = broker.scope_for_context(context_token)?;
    let channel_id = scope
        .channel_id
        .clone()
        .ok_or_else(|| "OSL broker conversation binding is incomplete".to_owned())?;
    ipc::commands::cmd_osl_decrypt_message_v2(
        &core.osl,
        service_message_id,
        channel_id,
        sender_osl_id,
        capsule,
        Some(scope),
        None,
    )
}

pub fn decrypt_local_protected_capsule(
    core: &HubCoreState,
    broker: &HubBrokerState,
    context_token: &str,
    capsule: String,
) -> Result<DecryptedLocalProtectedMessage, String> {
    let dir = keystore::osl_config_dir()
        .map_err(|_| "OSL Privacy account storage is unavailable".to_owned())?;
    decrypt_local_protected_capsule_in_dir(core, broker, context_token, capsule, &dir)
}

/// Remove every loopback-decryption ledger row for the exact active context.
/// This is intentionally separate from platform-message deletion: it only
/// destroys OSL-managed local decryptability for capsules already prepared in
/// this service/account/conversation binding.
pub fn burn_local_protected_context(
    core: &HubCoreState,
    broker: &HubBrokerState,
    context_token: &str,
) -> Result<usize, String> {
    let dir = keystore::osl_config_dir()
        .map_err(|_| "OSL Privacy account storage is unavailable".to_owned())?;
    let context = broker.context_for(context_token)?;
    let (_, file_key) = local_protected_identity(core, &context)?;
    let _transition = broker
        .local_protected_transition
        .lock()
        .map_err(|_| "OSL protected state is unavailable".to_owned())?;
    prune_local_ledger_context(
        &dir.join(LOCAL_PROTECTED_FILE),
        &file_key,
        &local_context_binding(&context),
    )
}

/// Service-burn companion for an immutable indexed manifest. The binding came
/// from the same write-ahead registration that preceded each local-ledger
/// write, so no service profile or platform history is touched here.
pub fn burn_indexed_local_protected_binding(
    core: &HubCoreState,
    context_binding_sha256: &str,
) -> Result<usize, String> {
    if context_binding_sha256.len() != 64
        || !context_binding_sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("OSL indexed local context binding is invalid".to_owned());
    }
    let dir = keystore::osl_config_dir()
        .map_err(|_| "OSL Privacy account storage is unavailable".to_owned())?;
    let file_key = ipc::main_password::get_file_storage_key()
        .ok_or_else(|| "Unlock OSL before burning indexed protected state".to_owned())?;
    if core
        .osl
        .identity
        .lock()
        .map_err(|_| "OSL identity state is unavailable".to_owned())?
        .is_none()
    {
        return Err("OSL identity is not loaded".to_owned());
    }
    prune_local_ledger_context(
        &dir.join(LOCAL_PROTECTED_FILE),
        &file_key,
        context_binding_sha256,
    )
}

fn decrypt_local_protected_capsule_in_dir(
    core: &HubCoreState,
    broker: &HubBrokerState,
    context_token: &str,
    capsule: String,
    dir: &Path,
) -> Result<DecryptedLocalProtectedMessage, String> {
    if capsule.len() > 256 * 1024 {
        return Err("The protected message is too large to open on this device".to_owned());
    }
    let context = broker.context_for(context_token)?;
    let display_policy = crate::security::scope_security(scope_input(&context)?)?;
    require_decrypt_display_enabled(display_policy.decrypt_display_enabled)?;
    let (identity, file_key) = local_protected_identity(core, &context)?;
    let _transition = broker
        .local_protected_transition
        .lock()
        .map_err(|_| "OSL protected state is unavailable".to_owned())?;
    let recovered =
        ipc::wire_v2::decrypt_v2(&capsule, &identity.x25519_secret, &identity.x25519_public)
            .map_err(|_| "The protected message could not be decrypted".to_owned())?;
    if recovered.msg_type != LOCAL_PROTECTED_MESSAGE_TYPE {
        return Err("This is not a protected message from this device".to_owned());
    }
    let payload: LocalProtectedPayload = serde_json::from_slice(&recovered.plaintext)
        .map_err(|_| "OSL protected payload is malformed".to_owned())?;
    if payload.version != LOCAL_PROTECTED_VERSION {
        return Err("OSL protected payload version is unsupported".to_owned());
    }
    let expected_binding = local_context_binding(&context);
    if payload.context_binding != expected_binding {
        return Err("This protected message belongs to another conversation".to_owned());
    }

    let path = dir.join(LOCAL_PROTECTED_FILE);
    let mut ledger = load_local_ledger(&path, &file_key)?;
    let ttl_scope: ipc::scope::Scope = scope_input(&context)?
        .try_into()
        .map_err(|_| "OSL protected scope is invalid".to_owned())?;
    let ttl_file = ipc::scope_ttl_file::load_scope_ttls(&dir.join("scope_ttl.json"));
    let ttl_seconds = ipc::scope_ttl_file::get_scope_ttl(&ttl_file, &ttl_scope.storage_key());
    let now = ipc::main_password::now_unix_secs_pub();
    if ttl_seconds > 0
        && ledger
            .records
            .get(&payload.local_message_id)
            .is_some_and(|record| now.saturating_sub(record.created_at) > i64::from(ttl_seconds))
    {
        ledger.records.remove(&payload.local_message_id);
        write_local_ledger(&path, &ledger, &file_key)?;
        return Err("This protected message has expired".to_owned());
    }
    let record = ledger
        .records
        .get(&payload.local_message_id)
        .ok_or_else(|| {
            "This protected message is not available for this OSL identity".to_owned()
        })?;
    if record.context_binding != expected_binding
        || record.capsule_sha256 != sha256_hex(capsule.as_bytes())
    {
        return Err("OSL could not verify this protected message".to_owned());
    }
    let view_once_consumed = apply_successful_open_policy(
        &mut ledger,
        &payload.local_message_id,
        payload.view_once,
        now,
    )?;
    write_local_ledger(&path, &ledger, &file_key)?;

    Ok(DecryptedLocalProtectedMessage {
        plaintext: payload.plaintext,
        local_message_id: payload.local_message_id,
        protection: LOCAL_PROTECTED_LABEL,
        person_to_person_e2ee: false,
        context_verified: true,
        view_once_consumed,
    })
}

fn require_decrypt_display_enabled(enabled: bool) -> Result<(), String> {
    if enabled {
        Ok(())
    } else {
        Err("Decryption display is off for this conversation".to_owned())
    }
}

fn apply_successful_open_policy(
    ledger: &mut LocalProtectedLedger,
    local_message_id: &str,
    payload_view_once: bool,
    opened_at: i64,
) -> Result<bool, String> {
    let record = ledger
        .records
        .get(local_message_id)
        .ok_or_else(|| "This protected message is unavailable".to_owned())?;
    if record.view_once != payload_view_once {
        return Err("OSL could not verify this protected message policy".to_owned());
    }
    if record.view_once {
        // Successful first open consumes the local authorisation atomically
        // with the ledger write. The plaintext still exists in this return
        // value and may be copied or photographed by its recipient.
        ledger.records.remove(local_message_id);
        Ok(true)
    } else {
        if let Some(record) = ledger.records.get_mut(local_message_id) {
            record.last_opened_at = Some(opened_at);
        }
        Ok(false)
    }
}

fn local_protected_identity(
    core: &HubCoreState,
    context: &HubConversationContext,
) -> Result<(keystore::Identity, [u8; 32]), String> {
    let password = ipc::commands::cmd_osl_password_status()
        .map_err(|_| "OSL password state is unavailable".to_owned())?;
    if !password.is_set {
        return Err("Set the OSL main password before using local protection".to_owned());
    }
    let file_key = ipc::main_password::get_file_storage_key()
        .ok_or_else(|| "Unlock OSL before using local protection".to_owned())?;
    let identity = core
        .osl
        .identity
        .lock()
        .map_err(|_| "OSL identity state is unavailable".to_owned())?
        .clone()
        .ok_or_else(|| "OSL identity is not loaded".to_owned())?;
    if context.self_osl_id != identity.user_id {
        return Err("OSL protected context belongs to another identity".to_owned());
    }
    Ok((identity, file_key))
}

fn local_context_binding(context: &HubConversationContext) -> String {
    let mut hash = Sha256::new();
    hash.update(b"OSL-HUB-LOCAL-PROTECTED-CONTEXT-v1");
    let kind = match context.conversation_kind {
        HubConversationKind::Dm => "dm",
        HubConversationKind::Group => "group",
        HubConversationKind::Channel => "channel",
        HubConversationKind::Space => "space",
    };
    for value in [
        context.service_id.as_str(),
        context.account_id.as_str(),
        kind,
        context.conversation_id.as_str(),
        context.space_id.as_deref().unwrap_or(""),
        context.self_osl_id.as_str(),
    ] {
        hash.update((value.len() as u64).to_be_bytes());
        hash.update(value.as_bytes());
    }
    sha256_hex(&hash.finalize())
}

fn random_local_message_id() -> String {
    let random = crypto::random::random_bytes(16);
    format!("local-{}", hex(&random))
}

fn random_peer_message_id() -> String {
    let random = crypto::random::random_bytes(16);
    format!("peer-{}", hex(&random))
}

fn sha256_hex(value: &[u8]) -> String {
    let digest = Sha256::digest(value);
    hex(&digest)
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn load_local_ledger(path: &Path, file_key: &[u8; 32]) -> Result<LocalProtectedLedger, String> {
    let Some(bytes) = crate::atomic_file::read_recoverable_bounded(
        path,
        MAX_LOCAL_LEDGER_BYTES as u64,
        "OSL protected ledger",
    )?
    else {
        return Ok(LocalProtectedLedger::default());
    };
    if bytes.len() > MAX_LOCAL_LEDGER_BYTES || !ipc::main_password::has_enc_magic(&bytes) {
        return Err("OSL protected ledger is invalid or not encrypted".to_owned());
    }
    let plaintext = ipc::main_password::decrypt_at_rest(&bytes, file_key)
        .map_err(|_| "OSL protected ledger could not be decrypted".to_owned())?;
    let ledger: LocalProtectedLedger = serde_json::from_slice(&plaintext)
        .map_err(|_| "OSL protected ledger is malformed".to_owned())?;
    if ledger.version != LOCAL_PROTECTED_VERSION || ledger.records.len() > MAX_LOCAL_LEDGER_ENTRIES
    {
        return Err("OSL protected ledger version or size is invalid".to_owned());
    }
    Ok(ledger)
}

fn write_local_ledger(
    path: &Path,
    ledger: &LocalProtectedLedger,
    file_key: &[u8; 32],
) -> Result<(), String> {
    let plaintext = serde_json::to_vec(ledger)
        .map_err(|_| "OSL protected ledger could not be encoded".to_owned())?;
    if plaintext.len() > MAX_LOCAL_LEDGER_BYTES {
        return Err("OSL protected ledger exceeds its storage limit".to_owned());
    }
    let encrypted = ipc::main_password::encrypt_at_rest(&plaintext, file_key)
        .map_err(|_| "OSL protected ledger encryption failed".to_owned())?;
    crate::atomic_file::write_recoverable(path, &encrypted, "OSL protected ledger")
}

fn prune_local_ledger_context(
    path: &Path,
    file_key: &[u8; 32],
    context_binding: &str,
) -> Result<usize, String> {
    let mut ledger = load_local_ledger(path, file_key)?;
    let before = ledger.records.len();
    ledger
        .records
        .retain(|_, record| record.context_binding != context_binding);
    let removed = before.saturating_sub(ledger.records.len());
    if removed > 0 {
        write_local_ledger(path, &ledger, file_key)?;
    }
    Ok(removed)
}

fn validate_context(context: &HubConversationContext) -> Result<(), String> {
    if context.service_id != "osl-chat" {
        service_manifest(&context.service_id).map_err(|error| error.to_string())?;
    }
    validate_opaque_id(&context.account_id).map_err(|error| error.to_string())?;
    validate_context_id(&context.conversation_id, "conversation id")?;
    validate_context_id(&context.self_osl_id, "self OSL id")?;
    if matches!(
        context.conversation_kind,
        HubConversationKind::Channel | HubConversationKind::Space
    ) && context.space_id.is_none()
    {
        return Err("OSL broker channel/space context requires a space id".to_owned());
    }
    if let Some(space_id) = &context.space_id {
        validate_context_id(space_id, "space id")?;
    }
    if context.participant_osl_ids.is_empty()
        || context.participant_osl_ids.len() > MAX_PARTICIPANTS
    {
        return Err("OSL broker participant set is empty or too large".to_owned());
    }
    let mut unique = HashSet::with_capacity(context.participant_osl_ids.len());
    for participant in &context.participant_osl_ids {
        validate_context_id(participant, "participant OSL id")?;
        if !unique.insert(participant) {
            return Err("OSL broker participant set contains duplicates".to_owned());
        }
    }
    Ok(())
}

fn validate_context_id(value: &str, label: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > MAX_CONTEXT_ID_BYTES
        || value.chars().any(|character| {
            !(character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | ':'))
        })
    {
        return Err(format!("OSL broker {label} is invalid"));
    }
    Ok(())
}

fn validate_loopback_conversation_id(value: &str) -> Result<(), String> {
    if value.len() < 16 {
        return Err("OSL local conversation id is too short".to_owned());
    }
    validate_context_id(value, "local conversation id")
}

fn scope_input(context: &HubConversationContext) -> Result<ScopeInput, String> {
    let conversation = canonical_component(context, "conversation", &context.conversation_id);
    Ok(match context.conversation_kind {
        HubConversationKind::Dm => ScopeInput {
            kind: ScopeKind::Dm,
            id: conversation.clone(),
            server_id: None,
            channel_id: Some(conversation),
        },
        HubConversationKind::Group => ScopeInput {
            kind: ScopeKind::Gc,
            id: conversation.clone(),
            server_id: None,
            channel_id: Some(conversation),
        },
        HubConversationKind::Channel => {
            let space = canonical_component(
                context,
                "space",
                context
                    .space_id
                    .as_deref()
                    .ok_or_else(|| "OSL broker channel context requires a space id".to_owned())?,
            );
            ScopeInput {
                kind: ScopeKind::ServerChannel,
                id: format!("{space}:{conversation}"),
                server_id: Some(space),
                channel_id: Some(conversation),
            }
        }
        HubConversationKind::Space => {
            let space = canonical_component(
                context,
                "space",
                context
                    .space_id
                    .as_deref()
                    .ok_or_else(|| "OSL broker space context requires a space id".to_owned())?,
            );
            ScopeInput {
                kind: ScopeKind::ServerFull,
                id: space.clone(),
                server_id: Some(space),
                channel_id: None,
            }
        }
    })
}

fn canonical_component(context: &HubConversationContext, kind: &str, value: &str) -> String {
    let mut hash = Sha256::new();
    for part in [
        "OSL-HUB-SCOPE-v1",
        context.service_id.as_str(),
        context.account_id.as_str(),
        kind,
        value,
    ] {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part.as_bytes());
    }
    format!("hub-{}", short_hex(&hash.finalize()))
}

fn manual_dm_channel_binding(
    service_id: &str,
    self_osl_user_id: &str,
    peer_osl_user_id: &str,
) -> Result<String, String> {
    if service_id != "osl-chat" {
        service_manifest(service_id).map_err(|error| error.to_string())?;
    }
    validate_context_id(self_osl_user_id, "self OSL id")?;
    validate_context_id(peer_osl_user_id, "peer OSL id")?;
    if self_osl_user_id == peer_osl_user_id {
        return Err("OSL manual peer cannot be the active identity".to_owned());
    }
    let mut identities = [self_osl_user_id, peer_osl_user_id];
    identities.sort_unstable();
    let mut hash = Sha256::new();
    for part in ["OSL-MANUAL-DM-v1", service_id, identities[0], identities[1]] {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part.as_bytes());
    }
    Ok(format!("manual-dm-{}", short_hex(&hash.finalize())))
}

fn context_token(
    generation: u64,
    host_generation: u64,
    context: &HubConversationContext,
) -> String {
    let mut hash = Sha256::new();
    hash.update(b"OSL-HUB-CONTEXT-v1");
    hash.update(generation.to_be_bytes());
    hash.update(host_generation.to_be_bytes());
    hash.update(canonical_component(
        context,
        "conversation",
        &context.conversation_id,
    ));
    format!("ctx-{generation}-{}", short_hex(&hash.finalize()))
}

fn short_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(32);
    for byte in bytes.iter().take(16) {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ServiceKind;
    use crate::service_host::owner_profile_namespace;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn context(account_id: &str, conversation_id: &str) -> HubConversationContext {
        HubConversationContext {
            service_id: "instagram".to_owned(),
            account_id: account_id.to_owned(),
            conversation_kind: HubConversationKind::Dm,
            conversation_id: conversation_id.to_owned(),
            space_id: None,
            participant_osl_ids: vec!["peer-rose".to_owned(), "self-liam".to_owned()],
            self_osl_id: "self-liam".to_owned(),
        }
    }

    fn temporary_registry() -> std::path::PathBuf {
        ipc::main_password::set_file_storage_key(Some([0x5a; 32]));
        std::env::temp_dir().join(format!(
            "osl-hub-loopback-registry-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn native_overlay_open_batch_schema_is_bounded_and_carries_expiry() {
        assert_eq!(MAX_NATIVE_OVERLAY_OPEN_BATCH, 64);
        let opened = OpenedNativeOverlayText {
            plaintext: "first\n\nthird".to_owned(),
            context_verified: true,
            person_to_person_e2ee: true,
            view_once_consumed: true,
            expires_at: 1_787_000_000,
        };
        let value = serde_json::to_value(OpenedNativeOverlayTextBatch {
            messages: vec![opened],
            pending_view_once: vec![PendingNativeOverlayText {
                message_id: "peer-0123456789abcdef0123456789abcdef".to_owned(),
                expires_at: 1_787_000_100,
                person_to_person_e2ee: true,
            }],
            acknowledgments: Vec::new(),
            fetched: 2,
        })
        .unwrap();
        assert_eq!(value["fetched"], 2);
        assert_eq!(value["messages"][0]["expiresAt"], 1_787_000_000i64);
        assert_eq!(value["messages"][0]["plaintext"], "first\n\nthird");
        assert_eq!(
            value["pendingViewOnce"][0]["messageId"],
            "peer-0123456789abcdef0123456789abcdef"
        );
        assert!(value["pendingViewOnce"][0].get("plaintext").is_none());
    }

    #[test]
    fn view_once_received_receipt_is_bounded_deduplicated_and_expires() {
        let broker = HubBrokerState::default();
        let message_id = "peer-0123456789abcdef0123456789abcdef";
        assert!(!broker.view_once_received_was_sent(message_id, 10).unwrap());
        broker
            .record_view_once_received(message_id, 20, 10)
            .unwrap();
        assert!(broker.view_once_received_was_sent(message_id, 11).unwrap());
        assert!(!broker.view_once_received_was_sent(message_id, 20).unwrap());
    }

    #[test]
    fn native_overlay_attachment_batch_ignores_unrelated_rows_before_valid_attachment() {
        let rows = (0..=MAX_NATIVE_OVERLAY_OPEN_BATCH).collect::<Vec<_>>();
        let valid = collect_valid_bounded(rows, MAX_NATIVE_OVERLAY_OPEN_BATCH, |row| {
            (row == MAX_NATIVE_OVERLAY_OPEN_BATCH).then_some(row)
        });

        assert_eq!(valid, vec![MAX_NATIVE_OVERLAY_OPEN_BATCH]);
    }

    #[test]
    fn native_overlay_ack_is_correlated_and_replay_idempotent() {
        let context = context("discord-personal", "dm-receipt");
        let manual = ManualPeerContext {
            service_id: "discord".to_owned(),
            account_id: context.account_id.clone(),
            person_id: "friend-receipt".to_owned(),
            peer_osl_user_id: "osl-peer-receipt".to_owned(),
            scope: ScopeInput {
                kind: ScopeKind::Dm,
                id: "scope-receipt".to_owned(),
                server_id: None,
                channel_id: Some(context.conversation_id.clone()),
            },
        };
        let mut ledger = NativeOverlayReceiptLedger {
            version: NATIVE_OVERLAY_ACK_VERSION,
            records: BTreeMap::from([(
                "msg-receipt".to_owned(),
                NativeOverlayReceiptRecord {
                    service_id: manual.service_id.clone(),
                    conversation_binding: context.conversation_id.clone(),
                    peer_osl_user_id: manual.peer_osl_user_id.clone(),
                    expires_at: 1_700_003_600,
                    status: NativeOverlayReceiptStatus::Sent,
                    acknowledged_at: 0,
                },
            )]),
        };
        let acknowledgment = NativeOverlayAcknowledgmentPayload {
            version: NATIVE_OVERLAY_ACK_VERSION,
            domain: NATIVE_OVERLAY_ACK_DOMAIN.to_owned(),
            message_id: "msg-receipt".to_owned(),
            status: NativeOverlayAcknowledgmentStatus::Opened,
            acknowledged_at: 1_700_000_010,
            expires_at: 1_700_003_600,
            service_id: manual.service_id.clone(),
            conversation_binding: context.conversation_id.clone(),
            sender_osl_user_id: manual.peer_osl_user_id.clone(),
            recipient_osl_user_id: context.self_osl_id.clone(),
        };
        let first = apply_native_overlay_acknowledgment_record(
            &mut ledger,
            &context,
            &manual,
            &acknowledgment,
        )
        .unwrap();
        let mut replay = acknowledgment;
        replay.acknowledged_at = 1_700_000_020;
        let second =
            apply_native_overlay_acknowledgment_record(&mut ledger, &context, &manual, &replay)
                .unwrap();
        assert_eq!(first.acknowledged_at, 1_700_000_010);
        assert_eq!(second.acknowledged_at, first.acknowledged_at);
        replay.message_id = "msg-unrelated".to_owned();
        assert!(apply_native_overlay_acknowledgment_record(
            &mut ledger,
            &context,
            &manual,
            &replay,
        )
        .is_err());
    }

    #[test]
    fn owned_loopback_context_derives_self_only_and_exact_host_generation() {
        let owner = "osl_owner_aaaaaaaaaaaaaaaa";
        let registry_path = temporary_registry();
        let registry = ServiceRegistryState::load(registry_path.clone());
        let account = registry
            .create_for_owner(owner, ServiceKind::Instagram, "Test".to_owned())
            .unwrap();
        let host = crate::service_host::ServiceHostState::default();
        let namespace = owner_profile_namespace(owner).unwrap();
        let active = host
            .begin_open(&namespace, "instagram", &account.id, "www.instagram.com")
            .unwrap();
        let broker = HubBrokerState::default();
        let lease = activate_owned_local_loopback_context(
            &broker,
            &registry,
            &host,
            owner,
            "instagram",
            &account.id,
            "local-0123456789abcdef".to_owned(),
        )
        .unwrap();
        let bound = broker.context_for(&lease.context_token).unwrap();
        assert_eq!(bound.self_osl_id, owner);
        assert_eq!(bound.participant_osl_ids, vec![owner]);
        assert_eq!(bound.service_id, "instagram");
        assert_eq!(bound.account_id, account.id);
        assert_eq!(lease.host_generation, active.generation);
        assert!(broker
            .validate_active_host(&lease.context_token, &active)
            .is_ok());
        assert!(broker
            .require_local_loopback_context(&lease.context_token)
            .is_ok());
        assert!(broker
            .require_peer_messaging_context(&lease.context_token)
            .is_err());
        let _ = std::fs::remove_file(registry_path);
    }

    #[test]
    fn loopback_activation_rejects_unowned_mismatched_stale_and_nonopaque_inputs() {
        let owner = "osl_owner_aaaaaaaaaaaaaaaa";
        let registry_path = temporary_registry();
        let registry = ServiceRegistryState::load(registry_path.clone());
        let account = registry
            .create_for_owner(owner, ServiceKind::Instagram, "Test".to_owned())
            .unwrap();
        let host = crate::service_host::ServiceHostState::default();
        let namespace = owner_profile_namespace(owner).unwrap();
        host.begin_open(&namespace, "instagram", &account.id, "www.instagram.com")
            .unwrap();
        let broker = HubBrokerState::default();

        let activate = |owner_id: &str, service: &str, account_id: &str, conversation: &str| {
            activate_owned_local_loopback_context(
                &broker,
                &registry,
                &host,
                owner_id,
                service,
                account_id,
                conversation.to_owned(),
            )
        };
        assert!(activate(
            "osl_owner_bbbbbbbbbbbbbbbb",
            "instagram",
            &account.id,
            "local-0123456789abcdef"
        )
        .is_err());
        assert!(activate(owner, "discord", &account.id, "local-0123456789abcdef").is_err());
        assert!(activate(
            owner,
            "instagram",
            "other-account",
            "local-0123456789abcdef"
        )
        .is_err());
        assert!(activate(owner, "instagram", &account.id, "semantic label").is_err());
        assert!(activate(owner, "instagram", &account.id, "too-short").is_err());

        host.next_generation().unwrap();
        assert!(activate(owner, "instagram", &account.id, "local-0123456789abcdef").is_err());
        let _ = std::fs::remove_file(registry_path);
    }

    #[test]
    fn context_switch_invalidates_prior_account_and_conversation() {
        let broker = HubBrokerState::default();
        let first = broker
            .activate(context("instagram-personal", "dm-1"), 7)
            .unwrap();
        let second = broker
            .activate(context("instagram-alt", "dm-2"), 8)
            .unwrap();
        assert!(broker.context_for(&first.context_token).is_err());
        assert_eq!(
            broker
                .context_for(&second.context_token)
                .unwrap()
                .account_id,
            "instagram-alt"
        );
    }

    #[test]
    fn canonical_scopes_are_service_and_account_separated() {
        let first = context("instagram-personal", "dm-1");
        let second = context("instagram-alt", "dm-1");
        assert_ne!(
            scope_input(&first).unwrap().id,
            scope_input(&second).unwrap().id
        );
        assert!(!scope_input(&first).unwrap().id.contains("dm-1"));
    }

    #[test]
    fn manual_peer_scope_is_symmetric_across_different_local_account_ids() {
        let first = HubBrokerState::default();
        let second = HubBrokerState::default();
        let first_lease = first
            .activate_manual_peer(
                "osl-alice",
                &ActiveServiceHost {
                    service_id: "discord".to_owned(),
                    account_id: "client-one-profile".to_owned(),
                    generation: 11,
                    owner_namespace: "owner-one".to_owned(),
                },
                ManualPeerBinding {
                    person_id: "hub-person-bob".to_owned(),
                    peer_osl_user_id: "osl-bob".to_owned(),
                    peer_x25519_public: [2; 32],
                    peer_mlkem768_public: [2; 1184],
                },
            )
            .unwrap();
        let second_lease = second
            .activate_manual_peer(
                "osl-bob",
                &ActiveServiceHost {
                    service_id: "discord".to_owned(),
                    account_id: "completely-different-profile".to_owned(),
                    generation: 23,
                    owner_namespace: "owner-two".to_owned(),
                },
                ManualPeerBinding {
                    person_id: "hub-person-alice".to_owned(),
                    peer_osl_user_id: "osl-alice".to_owned(),
                    peer_x25519_public: [1; 32],
                    peer_mlkem768_public: [1; 1184],
                },
            )
            .unwrap();
        let first_scope = first.scope_for_context(&first_lease.context_token).unwrap();
        let second_scope = second
            .scope_for_context(&second_lease.context_token)
            .unwrap();
        assert_eq!(first_scope.channel_id, second_scope.channel_id);
        assert_ne!(first_scope.id, second_scope.id);
        assert_eq!(
            first_scope.id,
            security::manual_peer_scope_id("discord", "client-one-profile", "hub-person-bob",)
                .unwrap()
        );
        assert_eq!(
            second_scope.id,
            security::manual_peer_scope_id(
                "discord",
                "completely-different-profile",
                "hub-person-alice",
            )
            .unwrap()
        );
        assert_ne!(
            first_scope.id,
            security::manual_peer_scope_id("instagram", "client-one-profile", "hub-person-bob",)
                .unwrap()
        );
    }

    #[test]
    fn manual_peer_context_has_exactly_one_non_self_recipient_and_goes_stale() {
        let broker = HubBrokerState::default();
        let lease = broker
            .activate_manual_peer(
                "osl-alice",
                &ActiveServiceHost {
                    service_id: "discord".to_owned(),
                    account_id: "profile-a".to_owned(),
                    generation: 4,
                    owner_namespace: "owner-a".to_owned(),
                },
                ManualPeerBinding {
                    person_id: "hub-person-bob".to_owned(),
                    peer_osl_user_id: "osl-bob".to_owned(),
                    peer_x25519_public: [2; 32],
                    peer_mlkem768_public: [2; 1184],
                },
            )
            .unwrap();
        let context = broker.context_for(&lease.context_token).unwrap();
        assert_eq!(context.participant_osl_ids, ["hub-person-bob"]);
        assert!(broker
            .require_peer_messaging_context(&lease.context_token)
            .is_err());
        let core = HubCoreState::default();
        assert!(prepare_encrypted_text(
            &core,
            &broker,
            &lease.context_token,
            "generic bypass".to_owned(),
        )
        .is_err());
        assert!(decrypt_capsule(
            &core,
            &broker,
            &lease.context_token,
            "hub-person-bob".to_owned(),
            None,
            "DPC0::AAAA".to_owned(),
        )
        .is_err());
        assert!(prepare_encrypted_attachment(
            &core,
            &broker,
            &lease.context_token,
            "AA==".to_owned(),
            "file.png".to_owned(),
        )
        .is_err());
        assert!(open_encrypted_attachment(
            &core,
            &broker,
            &lease.context_token,
            "hub-person-bob".to_owned(),
            None,
            "AA==".to_owned(),
        )
        .is_err());
        assert_eq!(
            broker
                .manual_permission_target(&lease.context_token, "hub-person-bob", false)
                .unwrap(),
            "hub-person-bob"
        );
        assert!(broker
            .manual_permission_target(&lease.context_token, "hub-person-charlie", false)
            .is_err());
        assert!(broker
            .manual_permission_target(&lease.context_token, "hub-person-bob", true)
            .is_err());
        assert!(!context
            .participant_osl_ids
            .iter()
            .any(|participant| participant == &context.self_osl_id));
        broker
            .activate_manual_peer(
                "osl-alice",
                &ActiveServiceHost {
                    service_id: "discord".to_owned(),
                    account_id: "profile-a".to_owned(),
                    generation: 5,
                    owner_namespace: "owner-a".to_owned(),
                },
                ManualPeerBinding {
                    person_id: "hub-person-charlie".to_owned(),
                    peer_osl_user_id: "osl-charlie".to_owned(),
                    peer_x25519_public: [3; 32],
                    peer_mlkem768_public: [3; 1184],
                },
            )
            .unwrap();
        assert!(broker.manual_peer_for(&lease.context_token).is_err());
    }

    #[test]
    fn native_discord_manual_context_is_synthetic_and_generation_bound() {
        let broker = HubBrokerState::default();
        let active = ActiveServiceHost {
            service_id: "discord".to_owned(),
            account_id: "native-discord-00112233445566778899aabbccddeeff0011223344556677"
                .to_owned(),
            generation: 17,
            owner_namespace: "owner-00112233445566778899aabbccddeeff0011223344556677".to_owned(),
        };
        let activated = activate_owned_native_manual_peer_context(
            &broker,
            "osl-alice",
            &active,
            ManualPeerBinding {
                person_id: "hub-person-bob".to_owned(),
                peer_osl_user_id: "osl-bob".to_owned(),
                peer_x25519_public: [2; 32],
                peer_mlkem768_public: [2; 1184],
            },
        )
        .unwrap();
        assert!(broker
            .validate_active_host(&activated.lease.context_token, &active)
            .is_ok());
        assert_eq!(activated.lease.account_id, active.account_id);
        let mut reattached = active.clone();
        reattached.generation += 1;
        assert!(broker
            .validate_active_host(&activated.lease.context_token, &reattached)
            .is_err());
        let mut untrusted = active;
        untrusted.account_id = "ordinary-profile".to_owned();
        assert!(activate_owned_native_manual_peer_context(
            &HubBrokerState::default(),
            "osl-alice",
            &untrusted,
            ManualPeerBinding {
                person_id: "hub-person-bob".to_owned(),
                peer_osl_user_id: "osl-bob".to_owned(),
                peer_x25519_public: [2; 32],
                peer_mlkem768_public: [2; 1184],
            },
        )
        .is_err());
    }

    #[test]
    fn first_party_osl_chat_uses_only_the_fixed_internal_scope() {
        let broker = HubBrokerState::default();
        let activated = activate_owned_osl_chat_context(
            &broker,
            "osl-alice",
            ManualPeerBinding {
                person_id: "hub-person-bob".to_owned(),
                peer_osl_user_id: "osl-bob".to_owned(),
                peer_x25519_public: [2; 32],
                peer_mlkem768_public: [2; 1184],
            },
        )
        .unwrap();
        assert_eq!(activated.lease.service_id, "osl-chat");
        assert_eq!(activated.lease.account_id, "osl-main");
        assert_eq!(
            broker.active_osl_chat_context_token().unwrap(),
            activated.lease.context_token
        );
        assert!(broker.active_native_manual_context_token().is_err());
        assert_eq!(
            activated.scope.id,
            security::manual_peer_scope_id("osl-chat", "osl-main", "hub-person-bob").unwrap()
        );
        assert!(
            security::manual_peer_scope_id("osl-chat", "other-account", "hub-person-bob").is_err()
        );
        broker.clear_osl_chat_context().unwrap();
        assert!(broker.active_osl_chat_context_token().is_err());
    }

    #[test]
    fn first_party_osl_chat_chunk_roundtrip_preserves_capture_policy() {
        let alice = keystore::generate_identity("osl-alice-chat".to_owned());
        let bob = keystore::generate_identity("osl-bob-chat".to_owned());
        let core = HubCoreState::default();
        *core.osl.identity.lock().unwrap() = Some(alice.clone());
        let binding = ManualPeerBinding {
            person_id: "hub-person-bob".to_owned(),
            peer_osl_user_id: bob.user_id.clone(),
            peer_x25519_public: *bob.x25519_public.as_bytes(),
            peer_mlkem768_public: bob.mlkem_public_bytes,
        };
        let conversation_binding =
            manual_dm_channel_binding("osl-chat", &alice.user_id, &bob.user_id).unwrap();
        let alice_manual = ManualPeerContext {
            service_id: "osl-chat".to_owned(),
            account_id: "osl-main".to_owned(),
            person_id: binding.person_id.clone(),
            peer_osl_user_id: bob.user_id.clone(),
            scope: ScopeInput {
                kind: ScopeKind::Dm,
                id: "alice-chat-scope".to_owned(),
                server_id: None,
                channel_id: Some(conversation_binding.clone()),
            },
        };
        let alice_context = HubConversationContext {
            service_id: "osl-chat".to_owned(),
            account_id: "osl-main".to_owned(),
            conversation_kind: HubConversationKind::Dm,
            conversation_id: conversation_binding.clone(),
            space_id: None,
            participant_osl_ids: vec![binding.person_id.clone()],
            self_osl_id: alice.user_id.clone(),
        };
        let logical_message_id = "peer-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned();
        let wire = prepare_direct_manual_v3(
            &core,
            &binding,
            &alice_manual,
            &alice_context,
            "private chat".to_owned(),
            PeerProtectionPolicy {
                view_once: false,
                require_capture_protection: true,
                created_at: 1_700_000_000,
                expires_at: 1_700_003_600,
            },
            "peer-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned(),
            Some(&NativeTextChunkMeta {
                logical_message_id,
                chunk_index: 0,
                chunk_count: 1,
                whole_sha256: sha256_hex(b"private chat"),
                created_at: 1_700_000_000,
                expires_at: 1_700_003_600,
            }),
        )
        .unwrap();

        *core.osl.identity.lock().unwrap() = Some(bob.clone());
        let bob_manual = ManualPeerContext {
            service_id: "osl-chat".to_owned(),
            account_id: "osl-main".to_owned(),
            person_id: "hub-person-alice".to_owned(),
            peer_osl_user_id: alice.user_id.clone(),
            scope: ScopeInput {
                kind: ScopeKind::Dm,
                id: "bob-chat-scope".to_owned(),
                server_id: None,
                channel_id: Some(conversation_binding.clone()),
            },
        };
        let bob_context = HubConversationContext {
            service_id: "osl-chat".to_owned(),
            account_id: "osl-main".to_owned(),
            conversation_kind: HubConversationKind::Dm,
            conversation_id: conversation_binding,
            space_id: None,
            participant_osl_ids: vec!["hub-person-alice".to_owned()],
            self_osl_id: bob.user_id.clone(),
        };
        let opened = decrypt_direct_manual_v3(&core, &wire).unwrap();
        validate_peer_protected_payload(&opened, &bob_manual, &bob_context, 1_700_000_001).unwrap();
        assert_eq!(opened.plaintext, "private chat");
        assert!(opened.require_capture_protection);
        assert!(!capture_policy_allows_plaintext(&opened, false));
        assert!(capture_policy_allows_plaintext(&opened, true));

        let history_dir = temporary_registry().with_extension("history");
        std::fs::create_dir_all(&history_dir).unwrap();
        let history_store =
            store::MessageStore::open(&history_dir, bob.x25519_secret.as_bytes()).unwrap();
        *core.osl.message_store.lock().unwrap() = Some(history_store);
        ipc::commands::cmd_osl_persist_inbound(
            &core.osl,
            bob_context.conversation_id.clone(),
            opened.logical_message_id.clone().unwrap(),
            alice.user_id.clone(),
            opened.plaintext.clone(),
        )
        .unwrap();
        let history = ipc::commands::cmd_osl_load_channel_history(
            &core.osl,
            bob_context.conversation_id.clone(),
            Some(10),
        )
        .unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].plaintext, "private chat");
        assert_eq!(history[0].sender_osl_user_id, alice.user_id);
        *core.osl.message_store.lock().unwrap() = None;
        std::fs::remove_dir_all(history_dir).unwrap();

        let mut wrong_domain = bob_manual;
        wrong_domain.account_id = "ordinary-account".to_owned();
        assert!(validate_peer_protected_payload(
            &opened,
            &wrong_domain,
            &bob_context,
            1_700_000_001,
        )
        .is_err());
    }

    #[test]
    fn malformed_or_unknown_peer_prose_maps_to_one_generic_error() {
        let absent = peer_prose_token_or_generic(Ok(None)).unwrap_err();
        let malformed =
            peer_prose_token_or_generic(Err(ipc::prose_token::ProseTokenError::NotDpc0Wire))
                .unwrap_err();
        assert_eq!(absent, "This encrypted message could not be opened");
        assert_eq!(malformed, absent);
    }

    #[test]
    fn peer_payload_preserves_multiline_and_rejects_context_rehosting() {
        let manual = ManualPeerContext {
            service_id: "discord".to_owned(),
            account_id: "local-account".to_owned(),
            person_id: "person-bob".to_owned(),
            peer_osl_user_id: "osl-bob".to_owned(),
            scope: ScopeInput {
                kind: ScopeKind::Dm,
                id: "scope-test".to_owned(),
                server_id: None,
                channel_id: Some("manual-dm-test".to_owned()),
            },
        };
        let context = HubConversationContext {
            service_id: "discord".to_owned(),
            account_id: "local-account".to_owned(),
            conversation_kind: HubConversationKind::Dm,
            conversation_id: "manual-dm-test".to_owned(),
            space_id: None,
            participant_osl_ids: vec!["person-bob".to_owned()],
            self_osl_id: "osl-alice".to_owned(),
        };
        let multiline = "first\n\nsecond\nthird";
        let now = 1_700_000_000;
        let mut payload = PeerProtectedPayload {
            version: PEER_PROTECTED_VERSION,
            message_id: "peer-00112233445566778899aabbccddeeff".to_owned(),
            created_at: now - 10,
            expires_at: now + 3_600,
            service_id: "discord".to_owned(),
            conversation_binding: "manual-dm-test".to_owned(),
            sender_osl_user_id: "osl-bob".to_owned(),
            recipient_osl_user_id: "osl-alice".to_owned(),
            plaintext: multiline.to_owned(),
            view_once: true,
            require_capture_protection: true,
            logical_message_id: None,
            chunk_index: None,
            chunk_count: None,
            whole_sha256: None,
        };
        validate_peer_protected_payload(&payload, &manual, &context, now).unwrap();
        assert_eq!(payload.plaintext, multiline);

        payload.service_id = "instagram".to_owned();
        assert!(validate_peer_protected_payload(&payload, &manual, &context, now).is_err());
        payload.service_id = "discord".to_owned();
        payload.conversation_binding = "manual-dm-forwarded".to_owned();
        assert!(validate_peer_protected_payload(&payload, &manual, &context, now).is_err());
        payload.conversation_binding = "manual-dm-test".to_owned();
        payload.recipient_osl_user_id = "osl-charlie".to_owned();
        assert!(validate_peer_protected_payload(&payload, &manual, &context, now).is_err());
        payload.recipient_osl_user_id = "osl-alice".to_owned();
        payload.expires_at = now;
        assert!(validate_peer_protected_payload(&payload, &manual, &context, now).is_err());
        payload.expires_at = now + MAX_PEER_LIFETIME_SECONDS + 1;
        assert!(validate_peer_protected_payload(&payload, &manual, &context, now).is_err());
        payload.expires_at = now + 3_600;
        payload.created_at = now + MAX_PEER_CLOCK_SKEW_SECONDS + 1;
        assert!(validate_peer_protected_payload(&payload, &manual, &context, now).is_err());
    }

    #[test]
    fn v3_content_inspector_rejects_other_versions_controls_and_sender_mismatch() {
        fn fake_wire(version: u8, message_type: u8, sender: [u8; 32]) -> String {
            let mut raw = vec![0u8; 35 + 2 * ipc::wire_v2::SLOT_V3_BYTES + 12 + 16];
            raw[0] = version;
            raw[1] = message_type;
            raw[2..34].copy_from_slice(&sender);
            raw[34] = 2;
            format!("DPC0::{}", STANDARD.encode(raw))
        }

        let selected_friend = [0x31; 32];
        let third_party = [0x42; 32];
        let inspected =
            inspect_v3_content_wire(&fake_wire(3, ipc::wire_v2::MSG_TYPE_CONTENT, third_party))
                .unwrap();
        assert!(!constant_time_eq_32(&inspected.sender_ik, &selected_friend));
        assert!(inspect_v3_content_wire(&fake_wire(
            2,
            ipc::wire_v2::MSG_TYPE_CONTENT,
            selected_friend
        ))
        .is_err());
        assert!(inspect_v3_content_wire(&fake_wire(3, 1, selected_friend)).is_err());
        assert!(inspect_v3_content_wire("DPC0::not-base64").is_err());

        let self_public = [0x21; 32];
        let self_hash =
            ipc::wire_v2::pubkey_hash_prefix(&crypto::x25519::PublicKey::from_bytes(self_public));
        let friend_hash = ipc::wire_v2::pubkey_hash_prefix(&crypto::x25519::PublicKey::from_bytes(
            selected_friend,
        ));
        let valid_receive = InspectedV3Content {
            sender_ik: selected_friend,
            recipient_hashes: vec![friend_hash, self_hash],
        };
        assert!(verify_inspected_manual_v3(
            &valid_receive,
            &self_public,
            &selected_friend,
            &selected_friend
        )
        .is_ok());
        let wrong_recipient = InspectedV3Content {
            sender_ik: selected_friend,
            recipient_hashes: vec![self_hash, [0x99; 8]],
        };
        assert!(verify_inspected_manual_v3(
            &wrong_recipient,
            &self_public,
            &selected_friend,
            &selected_friend
        )
        .is_err());
        assert!(verify_inspected_manual_v3(
            &valid_receive,
            &self_public,
            &selected_friend,
            &third_party
        )
        .is_err());
    }

    #[test]
    fn direct_manual_v3_has_exact_self_peer_recipients_and_peer_opens_it() {
        let alice = keystore::generate_identity("osl-alice-direct".to_owned());
        let bob = keystore::generate_identity("osl-bob-direct".to_owned());
        let core = HubCoreState::default();
        *core.osl.identity.lock().unwrap() = Some(alice.clone());
        let binding = ManualPeerBinding {
            person_id: "hub-person-bob".to_owned(),
            peer_osl_user_id: bob.user_id.clone(),
            peer_x25519_public: *bob.x25519_public.as_bytes(),
            peer_mlkem768_public: bob.mlkem_public_bytes,
        };
        let conversation_binding =
            manual_dm_channel_binding("discord", &alice.user_id, &bob.user_id).unwrap();
        let alice_manual = ManualPeerContext {
            service_id: "discord".to_owned(),
            account_id: "alice-account".to_owned(),
            person_id: binding.person_id.clone(),
            peer_osl_user_id: bob.user_id.clone(),
            scope: ScopeInput {
                kind: ScopeKind::Dm,
                id: "alice-scope".to_owned(),
                server_id: None,
                channel_id: Some(conversation_binding.clone()),
            },
        };
        let alice_context = HubConversationContext {
            service_id: "discord".to_owned(),
            account_id: "alice-account".to_owned(),
            conversation_kind: HubConversationKind::Dm,
            conversation_id: conversation_binding.clone(),
            space_id: None,
            participant_osl_ids: vec![binding.person_id.clone()],
            self_osl_id: alice.user_id.clone(),
        };
        let wire = prepare_direct_manual_v3(
            &core,
            &binding,
            &alice_manual,
            &alice_context,
            "private hello".to_owned(),
            PeerProtectionPolicy {
                view_once: true,
                require_capture_protection: true,
                created_at: 1_700_000_000,
                expires_at: 1_700_003_600,
            },
            "peer-0123456789abcdef0123456789abcdef".to_owned(),
            None,
        )
        .unwrap();
        verify_manual_v3(&core, &binding, &wire, ManualWireSender::SelfIdentity).unwrap();
        *core.osl.identity.lock().unwrap() = Some(bob.clone());
        let bob_manual = ManualPeerContext {
            service_id: "discord".to_owned(),
            account_id: "bob-account".to_owned(),
            person_id: "hub-person-alice".to_owned(),
            peer_osl_user_id: alice.user_id.clone(),
            scope: ScopeInput {
                kind: ScopeKind::Dm,
                id: "bob-scope".to_owned(),
                server_id: None,
                channel_id: Some(conversation_binding.clone()),
            },
        };
        let bob_context = HubConversationContext {
            service_id: "discord".to_owned(),
            account_id: "bob-account".to_owned(),
            conversation_kind: HubConversationKind::Dm,
            conversation_id: conversation_binding.clone(),
            space_id: None,
            participant_osl_ids: vec!["hub-person-alice".to_owned()],
            self_osl_id: bob.user_id.clone(),
        };
        let opened = decrypt_direct_manual_v3(&core, &wire).unwrap();
        validate_peer_protected_payload(&opened, &bob_manual, &bob_context, 1_700_000_001).unwrap();
        assert_eq!(opened.plaintext, "private hello");
        assert!(opened.view_once);

        let alice_local_scope =
            security::manual_peer_scope_id("discord", "alice-account", "hub-person-bob").unwrap();
        let bob_local_scope =
            security::manual_peer_scope_id("discord", "bob-account", "hub-person-alice").unwrap();
        assert_ne!(alice_local_scope, bob_local_scope);
        assert_eq!(
            manual_dm_channel_binding("discord", &alice.user_id, &bob.user_id).unwrap(),
            manual_dm_channel_binding("discord", &bob.user_id, &alice.user_id).unwrap()
        );

        let alice_binding = ManualPeerBinding {
            person_id: "hub-person-alice".to_owned(),
            peer_osl_user_id: alice.user_id.clone(),
            peer_x25519_public: *alice.x25519_public.as_bytes(),
            peer_mlkem768_public: alice.mlkem_public_bytes,
        };
        let reply = prepare_direct_manual_v3(
            &core,
            &alice_binding,
            &bob_manual,
            &bob_context,
            "private reply".to_owned(),
            PeerProtectionPolicy {
                view_once: false,
                require_capture_protection: false,
                created_at: 1_700_000_002,
                expires_at: 1_700_003_602,
            },
            "peer-fedcba9876543210fedcba9876543210".to_owned(),
            None,
        )
        .unwrap();
        verify_manual_v3(
            &core,
            &alice_binding,
            &reply,
            ManualWireSender::SelfIdentity,
        )
        .unwrap();
        *core.osl.identity.lock().unwrap() = Some(alice.clone());
        let opened_reply = decrypt_direct_manual_v3(&core, &reply).unwrap();
        validate_peer_protected_payload(
            &opened_reply,
            &alice_manual,
            &alice_context,
            1_700_000_003,
        )
        .unwrap();
        assert_eq!(opened_reply.plaintext, "private reply");
        assert!(!opened_reply.view_once);
        let inspected_reply = inspect_v3_content_wire(&reply).unwrap();
        verify_inspected_manual_v3(
            &inspected_reply,
            alice.x25519_public.as_bytes(),
            bob.x25519_public.as_bytes(),
            bob.x25519_public.as_bytes(),
        )
        .unwrap();

        // The attachment envelope uses the same exact two-recipient proof but
        // a distinct authenticated message type. Its encrypted policy binds
        // the file bytes, key, peer identities, context, and view-once bit.
        *core.osl.identity.lock().unwrap() = Some(bob.clone());
        let attachment_plaintext = b"private attachment bytes\nwith a second line".to_vec();
        let attachment_key = [0x5a; 32];
        let sealed_bytes = ipc::attachment_wire::seal_attachment_v3(
            crypto::aead::Key::from_bytes(attachment_key),
            &attachment_plaintext,
            "private-note.png",
            &[],
        )
        .unwrap();
        let attachment_id = "peer-11223344556677889900aabbccddeeff".to_owned();
        let transport_filename = "osl-11223344556677889900aabbccddeeff.mp4".to_owned();
        let payload = PeerAttachmentPayload {
            version: PEER_ATTACHMENT_VERSION,
            attachment_id: attachment_id.clone(),
            created_at: 1_700_000_004,
            expires_at: 1_700_003_604,
            service_id: "discord".to_owned(),
            conversation_binding: conversation_binding.clone(),
            sender_osl_user_id: bob.user_id.clone(),
            recipient_osl_user_id: alice.user_id.clone(),
            original_filename: "private-note.png".to_owned(),
            mime_type: "image/png".to_owned(),
            plaintext_size: attachment_plaintext.len() as u64,
            transport_filename,
            ciphertext_sha256: sha256_hex(&sealed_bytes),
            ciphertext_format: "osl-attachment-v3".to_owned(),
            key_algorithm: "xchacha20-poly1305-ietf".to_owned(),
            attachment_key,
            view_once: true,
        };
        let encoded = serde_json::to_vec(&payload).unwrap();
        let attachment_wire = encrypt_direct_manual_v3_payload(
            &core,
            &alice_binding,
            ipc::wire_v2::MSG_TYPE_ATTACHMENT,
            &encoded,
        )
        .unwrap();
        verify_manual_v3_type(
            &core,
            &alice_binding,
            &attachment_wire,
            ManualWireSender::SelfIdentity,
            ipc::wire_v2::MSG_TYPE_ATTACHMENT,
        )
        .unwrap();

        *core.osl.identity.lock().unwrap() = Some(alice.clone());
        verify_manual_v3_type(
            &core,
            &binding,
            &attachment_wire,
            ManualWireSender::Peer,
            ipc::wire_v2::MSG_TYPE_ATTACHMENT,
        )
        .unwrap();
        assert!(verify_manual_v3_type(
            &core,
            &binding,
            &attachment_wire,
            ManualWireSender::SelfIdentity,
            ipc::wire_v2::MSG_TYPE_ATTACHMENT,
        )
        .is_err());
        let opened_bytes = decrypt_direct_manual_v3_payload(
            &core,
            &attachment_wire,
            ipc::wire_v2::MSG_TYPE_ATTACHMENT,
        )
        .unwrap();
        let mut opened_attachment: PeerAttachmentPayload =
            serde_json::from_slice(&opened_bytes).unwrap();
        validate_peer_attachment_payload(
            &opened_attachment,
            &alice_manual,
            &alice_context,
            &sealed_bytes,
            1_700_000_005,
        )
        .unwrap();
        assert!(opened_attachment.view_once);

        let mut wrong_context = alice_context.clone();
        wrong_context.conversation_id = "manual-dm-other".to_owned();
        assert!(validate_peer_attachment_payload(
            &opened_attachment,
            &alice_manual,
            &wrong_context,
            &sealed_bytes,
            1_700_000_005,
        )
        .is_err());
        opened_attachment.sender_osl_user_id = "osl-charlie".to_owned();
        assert!(validate_peer_attachment_payload(
            &opened_attachment,
            &alice_manual,
            &alice_context,
            &sealed_bytes,
            1_700_000_005,
        )
        .is_err());
        opened_attachment.sender_osl_user_id = bob.user_id.clone();
        let mut tampered_ciphertext = sealed_bytes.clone();
        let last = tampered_ciphertext.len() - 1;
        tampered_ciphertext[last] ^= 1;
        assert!(validate_peer_attachment_payload(
            &opened_attachment,
            &alice_manual,
            &alice_context,
            &tampered_ciphertext,
            1_700_000_005,
        )
        .is_err());
        let (cover, embedded_filename, ciphertext) =
            ipc::attachment_wire::open_attachment_v3_split(&sealed_bytes).unwrap();
        assert!(cover.is_empty());
        assert_eq!(embedded_filename, "private-note.png");
        assert_eq!(
            crypto::attachment::decrypt_attachment(
                crypto::aead::Key::from_bytes(opened_attachment.attachment_key),
                &ciphertext,
            )
            .unwrap(),
            attachment_plaintext
        );

        // A sender cannot flip ordinary/view-once policy after encryption.
        let mut tampered_wire = attachment_wire.into_bytes();
        let last = tampered_wire.len() - 1;
        tampered_wire[last] = if tampered_wire[last] == b'A' {
            b'B'
        } else {
            b'A'
        };
        let tampered_wire = String::from_utf8(tampered_wire).unwrap();
        assert!(decrypt_direct_manual_v3_payload(
            &core,
            &tampered_wire,
            ipc::wire_v2::MSG_TYPE_ATTACHMENT,
        )
        .is_err());
    }

    #[test]
    fn native_text_chunks_preserve_boundaries_and_reassemble_only_complete_consistent_groups() {
        let logical = format!("first\n\n{}🙂\nlast", "\\\n".repeat(520_000));
        assert!(logical.len() <= MAX_NATIVE_OVERLAY_LOGICAL_TEXT_BYTES);
        let chunks = split_native_overlay_text(&logical).unwrap();
        assert!(chunks.len() <= MAX_NATIVE_OVERLAY_TEXT_CHUNKS);
        assert!(chunks
            .iter()
            .all(|chunk| chunk.len() <= MAX_NATIVE_OVERLAY_CHUNK_BYTES));
        assert_eq!(chunks.concat(), logical);
        assert!(
            split_native_overlay_text(&"x".repeat(MAX_NATIVE_OVERLAY_LOGICAL_TEXT_BYTES + 1))
                .is_err()
        );

        let alice = keystore::generate_identity("osl-native-chunk-alice".to_owned());
        let bob = keystore::generate_identity("osl-native-chunk-bob".to_owned());
        let core = HubCoreState::default();
        *core.osl.identity.lock().unwrap() = Some(alice.clone());
        let binding = ManualPeerBinding {
            person_id: "hub-person-bob".to_owned(),
            peer_osl_user_id: bob.user_id.clone(),
            peer_x25519_public: *bob.x25519_public.as_bytes(),
            peer_mlkem768_public: bob.mlkem_public_bytes,
        };
        let manual = ManualPeerContext {
            service_id: "discord".to_owned(),
            account_id: "native-discord-test".to_owned(),
            person_id: binding.person_id.clone(),
            peer_osl_user_id: bob.user_id.clone(),
            scope: ScopeInput {
                kind: ScopeKind::Dm,
                id: "native-chunk-scope".to_owned(),
                server_id: None,
                channel_id: Some("native-chunk-conversation".to_owned()),
            },
        };
        let context = HubConversationContext {
            service_id: "discord".to_owned(),
            account_id: manual.account_id.clone(),
            conversation_kind: HubConversationKind::Dm,
            conversation_id: "native-chunk-conversation".to_owned(),
            space_id: None,
            participant_osl_ids: vec![binding.person_id.clone()],
            self_osl_id: alice.user_id.clone(),
        };
        let chunk_plaintext = format!(
            "{}🙂",
            "\n\\".repeat((MAX_NATIVE_OVERLAY_CHUNK_BYTES - 4) / 2)
        );
        assert_eq!(chunk_plaintext.len(), MAX_NATIVE_OVERLAY_CHUNK_BYTES);
        let meta = NativeTextChunkMeta {
            logical_message_id: "peer-11112222333344445555666677778888".to_owned(),
            chunk_index: 0,
            chunk_count: 1,
            whole_sha256: sha256_hex(chunk_plaintext.as_bytes()),
            created_at: 1_700_000_000,
            expires_at: 1_700_003_600,
        };
        let wire = prepare_direct_manual_v3(
            &core,
            &binding,
            &manual,
            &context,
            chunk_plaintext.clone(),
            PeerProtectionPolicy {
                view_once: false,
                require_capture_protection: true,
                created_at: meta.created_at,
                expires_at: meta.expires_at,
            },
            "peer-00001111222233334444555566667777".to_owned(),
            Some(&meta),
        )
        .unwrap();
        let cipher = STANDARD
            .decode(wire.strip_prefix("DPC0::").unwrap())
            .unwrap();
        assert!(cipher.len() <= 64 * 1024);
        *core.osl.identity.lock().unwrap() = Some(bob);
        let decoded = decrypt_direct_manual_v3(&core, &wire).unwrap();
        assert_eq!(decoded.plaintext, chunk_plaintext);
        assert_eq!(
            decoded.logical_message_id.as_deref(),
            Some(meta.logical_message_id.as_str())
        );

        let pieces = ["first\n", "\nsecond🙂", "\nthird"];
        let whole = pieces.concat();
        let mut template = decoded;
        template.logical_message_id = Some("peer-99990000111122223333444455556666".to_owned());
        template.chunk_count = Some(3);
        template.whole_sha256 = Some(sha256_hex(whole.as_bytes()));
        let mut group = NativeTextReassembly {
            template,
            chunks: BTreeMap::from([
                (2, pieces[2].to_owned()),
                (0, pieces[0].to_owned()),
                (1, pieces[1].to_owned()),
            ]),
            inbox_ids: vec!["c".to_owned(), "a".to_owned(), "b".to_owned()],
            bytes: whole.len(),
            invalid: false,
        };
        assert_eq!(
            reassemble_native_text_group(&group).as_deref(),
            Some(whole.as_str())
        );
        group.chunks.insert(1, pieces[1].to_owned());
        assert_eq!(
            reassemble_native_text_group(&group).as_deref(),
            Some(whole.as_str())
        );
        let mut mixed = group.template.clone();
        mixed.whole_sha256 = Some("00".repeat(32));
        assert!(!same_native_text_group(&group.template, &mixed));
        group.chunks.remove(&1);
        assert!(reassemble_native_text_group(&group).is_none());
        group.chunks.insert(1, pieces[1].to_owned());
        group.invalid = true;
        assert!(reassemble_native_text_group(&group).is_none());
    }

    #[test]
    fn native_overlay_notice_is_distinct_peer_authenticated_and_context_bound() {
        let alice = keystore::generate_identity("osl-alice-overlay".to_owned());
        let bob = keystore::generate_identity("osl-bob-overlay".to_owned());
        let core = HubCoreState::default();
        *core.osl.identity.lock().unwrap() = Some(alice.clone());
        let bob_binding = ManualPeerBinding {
            person_id: "hub-person-bob".to_owned(),
            peer_osl_user_id: bob.user_id.clone(),
            peer_x25519_public: *bob.x25519_public.as_bytes(),
            peer_mlkem768_public: bob.mlkem_public_bytes,
        };
        let channel = manual_dm_channel_binding("discord", &alice.user_id, &bob.user_id).unwrap();
        assert_eq!(
            native_overlay_relay_scope_id(&channel).unwrap(),
            native_overlay_relay_scope_id(
                &manual_dm_channel_binding("discord", &bob.user_id, &alice.user_id).unwrap(),
            )
            .unwrap()
        );
        let notice = NativeOverlayRelayNotice {
            version: NATIVE_OVERLAY_RELAY_VERSION,
            domain: NATIVE_OVERLAY_RELAY_DOMAIN.to_owned(),
            created_at: 1_700_000_000,
            expires_at: 1_700_003_600,
            service_id: "discord".to_owned(),
            conversation_binding: channel.clone(),
            sender_osl_user_id: alice.user_id.clone(),
            recipient_osl_user_id: bob.user_id.clone(),
            message_id: "msg-overlay-test".to_owned(),
            cover_pointer: "Quiet mornings make careful plans feel easier.".to_owned(),
        };
        let wire = encrypt_direct_manual_v3_payload(
            &core,
            &bob_binding,
            ipc::wire_v2::MSG_TYPE_NATIVE_OVERLAY_RELAY,
            &serde_json::to_vec(&notice).unwrap(),
        )
        .unwrap();
        verify_manual_v3_type(
            &core,
            &bob_binding,
            &wire,
            ManualWireSender::SelfIdentity,
            ipc::wire_v2::MSG_TYPE_NATIVE_OVERLAY_RELAY,
        )
        .unwrap();
        let bundle = decode_overlay_relay_wire(&wire).unwrap();
        assert!(ipc::wire_v2::is_native_overlay_relay_bundle(&bundle));
        assert!(verify_manual_v3_type(
            &core,
            &bob_binding,
            &wire,
            ManualWireSender::SelfIdentity,
            ipc::wire_v2::MSG_TYPE_CONTENT,
        )
        .is_err());

        let acknowledgment = NativeOverlayAcknowledgmentPayload {
            version: NATIVE_OVERLAY_ACK_VERSION,
            domain: NATIVE_OVERLAY_ACK_DOMAIN.to_owned(),
            message_id: notice.message_id.clone(),
            status: NativeOverlayAcknowledgmentStatus::Opened,
            acknowledged_at: 1_700_000_002,
            expires_at: notice.expires_at,
            service_id: notice.service_id.clone(),
            conversation_binding: notice.conversation_binding.clone(),
            sender_osl_user_id: alice.user_id.clone(),
            recipient_osl_user_id: bob.user_id.clone(),
        };
        let acknowledgment_wire = encrypt_direct_manual_v3_payload(
            &core,
            &bob_binding,
            ipc::wire_v2::MSG_TYPE_NATIVE_OVERLAY_ACK,
            &serde_json::to_vec(&acknowledgment).unwrap(),
        )
        .unwrap();
        let acknowledgment_bundle = decode_native_overlay_ack_wire(&acknowledgment_wire).unwrap();
        assert!(ipc::wire_v2::is_native_overlay_ack_bundle(
            &acknowledgment_bundle
        ));
        assert!(!ipc::wire_v2::is_native_overlay_relay_bundle(
            &acknowledgment_bundle
        ));

        *core.osl.identity.lock().unwrap() = Some(bob.clone());
        let alice_binding = ManualPeerBinding {
            person_id: "hub-person-alice".to_owned(),
            peer_osl_user_id: alice.user_id.clone(),
            peer_x25519_public: *alice.x25519_public.as_bytes(),
            peer_mlkem768_public: alice.mlkem_public_bytes,
        };
        verify_manual_v3_type(
            &core,
            &alice_binding,
            &wire,
            ManualWireSender::Peer,
            ipc::wire_v2::MSG_TYPE_NATIVE_OVERLAY_RELAY,
        )
        .unwrap();
        let opened = decrypt_direct_manual_v3_payload(
            &core,
            &wire,
            ipc::wire_v2::MSG_TYPE_NATIVE_OVERLAY_RELAY,
        )
        .unwrap();
        let opened: NativeOverlayRelayNotice = serde_json::from_slice(&opened).unwrap();
        let manual = ManualPeerContext {
            service_id: "discord".to_owned(),
            account_id: "native-discord-bob".to_owned(),
            person_id: alice_binding.person_id.clone(),
            peer_osl_user_id: alice.user_id.clone(),
            scope: ScopeInput {
                kind: ScopeKind::Dm,
                id: "bob-local-scope".to_owned(),
                server_id: None,
                channel_id: Some(channel.clone()),
            },
        };
        let context = HubConversationContext {
            service_id: "discord".to_owned(),
            account_id: manual.account_id.clone(),
            conversation_kind: HubConversationKind::Dm,
            conversation_id: channel,
            space_id: None,
            participant_osl_ids: vec![manual.person_id.clone()],
            self_osl_id: bob.user_id,
        };
        validate_native_overlay_relay_notice(&opened, &manual, &context, 1_700_000_001).unwrap();
        verify_manual_v3_type(
            &core,
            &alice_binding,
            &acknowledgment_wire,
            ManualWireSender::Peer,
            ipc::wire_v2::MSG_TYPE_NATIVE_OVERLAY_ACK,
        )
        .unwrap();
        let opened_ack = decrypt_direct_manual_v3_payload(
            &core,
            &acknowledgment_wire,
            ipc::wire_v2::MSG_TYPE_NATIVE_OVERLAY_ACK,
        )
        .unwrap();
        let opened_ack: NativeOverlayAcknowledgmentPayload =
            serde_json::from_slice(&opened_ack).unwrap();
        validate_native_overlay_acknowledgment(&opened_ack, &manual, &context, 1_700_000_003)
            .unwrap();
        let encoded_ack = serde_json::to_value(&opened_ack).unwrap();
        assert!(encoded_ack.get("plaintext").is_none());
        let mut wrong_message = opened_ack;
        wrong_message.conversation_binding = "another-conversation".to_owned();
        assert!(validate_native_overlay_acknowledgment(
            &wrong_message,
            &manual,
            &context,
            1_700_000_003,
        )
        .is_err());
        for invalid in [
            NativeOverlayRelayNotice {
                conversation_binding: "another-conversation".to_owned(),
                ..opened.clone()
            },
            NativeOverlayRelayNotice {
                service_id: "telegram".to_owned(),
                ..opened.clone()
            },
            NativeOverlayRelayNotice {
                sender_osl_user_id: "osl-charlie".to_owned(),
                ..opened.clone()
            },
            NativeOverlayRelayNotice {
                recipient_osl_user_id: "osl-charlie".to_owned(),
                ..opened.clone()
            },
            NativeOverlayRelayNotice {
                domain: "another-domain".to_owned(),
                ..opened.clone()
            },
            NativeOverlayRelayNotice {
                expires_at: 1_700_000_001,
                ..opened.clone()
            },
        ] {
            assert!(validate_native_overlay_relay_notice(
                &invalid,
                &manual,
                &context,
                1_700_000_001,
            )
            .is_err());
        }
    }

    #[test]
    fn default_core_fails_closed_instead_of_fabricating_encryption() {
        let broker = HubBrokerState::default();
        let lease = broker
            .activate(context("instagram-personal", "dm-1"), 7)
            .unwrap();
        let core = HubCoreState::default();
        let error =
            prepare_encrypted_text(&core, &broker, &lease.context_token, "hello".to_owned())
                .unwrap_err();
        assert!(error.contains("identity not loaded"));
    }

    #[test]
    fn invalid_participants_and_platform_ids_are_rejected() {
        let mut invalid = context("instagram-personal", "dm-1");
        invalid.participant_osl_ids.push("peer-rose".to_owned());
        assert!(validate_context(&invalid).is_err());
        invalid.participant_osl_ids.pop();
        invalid.service_id = "instagram.evil".to_owned();
        assert!(validate_context(&invalid).is_err());
    }

    #[test]
    fn lease_is_bound_to_exact_active_host_generation() {
        let broker = HubBrokerState::default();
        let lease = broker
            .activate(context("instagram-personal", "dm-1"), 7)
            .unwrap();
        let active = ActiveServiceHost {
            service_id: "instagram".to_owned(),
            account_id: "instagram-personal".to_owned(),
            generation: 7,
            owner_namespace: "owner-test".to_owned(),
        };
        assert!(broker
            .validate_active_host(&lease.context_token, &active)
            .is_ok());
        assert!(broker
            .validate_active_host(
                &lease.context_token,
                &ActiveServiceHost {
                    generation: 8,
                    ..active
                },
            )
            .is_err());
    }

    #[test]
    fn context_burn_prunes_only_matching_local_ledger_rows() {
        let unique = format!(
            "osl-hub-ledger-burn-{}-{}",
            std::process::id(),
            random_local_message_id()
        );
        let dir = std::env::temp_dir().join(unique);
        let path = dir.join(LOCAL_PROTECTED_FILE);
        let file_key = [7_u8; 32];
        let mut records = BTreeMap::new();
        records.insert(
            "local-a".to_owned(),
            LocalProtectedRecord {
                context_binding: "binding-a".to_owned(),
                capsule_sha256: "capsule-a".to_owned(),
                created_at: 1,
                last_opened_at: None,
                view_once: false,
            },
        );
        records.insert(
            "local-b".to_owned(),
            LocalProtectedRecord {
                context_binding: "binding-b".to_owned(),
                capsule_sha256: "capsule-b".to_owned(),
                created_at: 2,
                last_opened_at: None,
                view_once: true,
            },
        );
        write_local_ledger(
            &path,
            &LocalProtectedLedger {
                version: LOCAL_PROTECTED_VERSION,
                records,
            },
            &file_key,
        )
        .unwrap();

        assert_eq!(
            prune_local_ledger_context(&path, &file_key, "binding-a").unwrap(),
            1
        );
        let remaining = load_local_ledger(&path, &file_key).unwrap();
        assert!(!remaining.records.contains_key("local-a"));
        assert!(remaining.records.contains_key("local-b"));
        assert_eq!(
            prune_local_ledger_context(&path, &file_key, "binding-a").unwrap(),
            0
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn view_once_open_consumes_only_the_authorised_record() {
        let mut ledger = LocalProtectedLedger::default();
        ledger.records.insert(
            "once".to_owned(),
            LocalProtectedRecord {
                context_binding: "binding".to_owned(),
                capsule_sha256: "capsule".to_owned(),
                created_at: 1,
                last_opened_at: None,
                view_once: true,
            },
        );
        assert!(apply_successful_open_policy(&mut ledger, "once", true, 2).unwrap());
        assert!(!ledger.records.contains_key("once"));
        assert!(apply_successful_open_policy(&mut ledger, "once", true, 3).is_err());
    }

    #[test]
    fn ordinary_open_is_repeatable_and_policy_tampering_fails_closed() {
        let mut ledger = LocalProtectedLedger::default();
        ledger.records.insert(
            "normal".to_owned(),
            LocalProtectedRecord {
                context_binding: "binding".to_owned(),
                capsule_sha256: "capsule".to_owned(),
                created_at: 1,
                last_opened_at: None,
                view_once: false,
            },
        );
        assert!(apply_successful_open_policy(&mut ledger, "normal", true, 2).is_err());
        assert_eq!(ledger.records["normal"].last_opened_at, None);
        assert!(!apply_successful_open_policy(&mut ledger, "normal", false, 3).unwrap());
        assert_eq!(ledger.records["normal"].last_opened_at, Some(3));
    }

    #[test]
    fn disabled_decrypt_display_fails_before_plaintext_is_returned() {
        assert!(require_decrypt_display_enabled(true).is_ok());
        assert_eq!(
            require_decrypt_display_enabled(false).unwrap_err(),
            "Decryption display is off for this conversation"
        );
    }
}
