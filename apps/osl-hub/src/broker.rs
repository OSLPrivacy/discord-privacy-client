//! Account- and conversation-bound trusted broker for the original OSL core.
//!
//! Platform pages never receive this command surface. A trusted local adapter
//! activates one exact service/account/conversation context and gets a
//! generation-bound lease. Switching context invalidates the prior lease,
//! preventing a prepared capsule from being reused in another account or chat.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::sync::Mutex;

use ipc::scope::{ScopeInput, ScopeKind};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::core_bridge::HubCoreState;
use crate::service_host::{service_manifest, validate_opaque_id, ActiveServiceHost};
use crate::service_scope_index::ServiceScopeRegistration;
use crate::services::{service_kind_from_id, ServiceRegistryState};

const MAX_CONTEXT_ID_BYTES: usize = 160;
const MAX_PARTICIPANTS: usize = 512;
const MAX_TEXT_BYTES: usize = 1_000;
const MAX_ATTACHMENT_B64_BYTES: usize = 32 * 1024 * 1024;
const MAX_LOCAL_LEDGER_BYTES: usize = 2 * 1024 * 1024;
const MAX_LOCAL_LEDGER_ENTRIES: usize = 4_096;
const LOCAL_PROTECTED_VERSION: u32 = 1;
const LOCAL_PROTECTED_MESSAGE_TYPE: u8 = 0x80;
const LOCAL_PROTECTED_FILE: &str = "hub_local_protected.json";
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
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum ContextAuthority {
    PeerMessaging,
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
}

impl HubBrokerState {
    pub fn activate(
        &self,
        context: HubConversationContext,
        host_generation: u64,
    ) -> Result<ContextLease, String> {
        self.activate_with_authority(context, host_generation, ContextAuthority::PeerMessaging)
    }

    fn activate_with_authority(
        &self,
        context: HubConversationContext,
        host_generation: u64,
        authority: ContextAuthority,
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
        self.activate_with_authority(context, active.generation, ContextAuthority::LocalLoopback)
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
        scope_input(&self.context_for(context_token)?)
    }

    pub fn service_scope_registration(
        &self,
        context_token: &str,
    ) -> Result<ServiceScopeRegistration, String> {
        let context = self.context_for(context_token)?;
        let scope = scope_input(&context)?;
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedCoreMessage {
    pub messages: Vec<String>,
    pub control_messages: Vec<String>,
    pub session_id: Option<u32>,
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

#[derive(Serialize, Deserialize)]
struct LocalProtectedPayload {
    version: u32,
    local_message_id: String,
    context_binding: String,
    plaintext: String,
    #[serde(default)]
    view_once: bool,
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
    let scope = scope_input(&context)?;
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
    let context = broker.context_for(context_token)?;
    let scope = scope_input(&context)?;
    let channel_id = canonical_component(&context, "conversation", &context.conversation_id);
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
    service_manifest(&context.service_id).map_err(|error| error.to_string())?;
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
