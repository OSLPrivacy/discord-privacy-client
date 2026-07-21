//! First-party text-only OSL Chat transport.
//!
//! This module deliberately owns no provider/native-window authority. It
//! wraps the existing verified-friend prose-token encryption in a second,
//! authenticated direct envelope delivered through the OSL control inbox.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use ipc::scope::Scope;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::broker::{self, HubBrokerState};
use crate::core_bridge::HubCoreState;
use crate::security::{self, HubSecurityState};

const VERSION: u32 = 1;
const MAX_BATCH: usize = 64;
const MAX_TEXT_BYTES: usize = 1_000;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InboxEnvelope {
    version: u32,
    message_id: String,
    cover_text: String,
    expires_at: i64,
    view_once: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedText {
    pub message_id: String,
    pub expires_at: i64,
    pub person_to_person_e2ee: bool,
    pub view_once: bool,
    pub delivered_to_osl_inbox: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedText {
    pub plaintext: String,
    pub context_verified: bool,
    pub person_to_person_e2ee: bool,
    pub view_once_consumed: bool,
    pub expires_at: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedBatch {
    pub messages: Vec<OpenedText>,
    pub pending_view_once: Vec<serde_json::Value>,
    pub acknowledgments: Vec<serde_json::Value>,
    pub fetched: usize,
}

#[derive(Serialize)]
pub struct HistoryRow {
    pub discord_message_id: String,
    pub channel_id: String,
    pub sender_discord_id: String,
    pub sender_osl_user_id: String,
    pub plaintext: String,
    pub decrypted_at: i64,
    pub burned: bool,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs() as i64)
        .unwrap_or(0)
}

fn message_id() -> String {
    let random = crypto::random::random_bytes(16);
    format!(
        "peer-{}",
        random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn transport(
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

fn persist(core: &HubCoreState, row: store::StoredMessage) {
    if let Ok(guard) = core.osl.message_store.lock() {
        if let Some(store) = guard.as_ref() {
            let _ = store.put(&row);
        }
    }
}

pub fn prepare_text(
    core: &HubCoreState,
    security_state: &HubSecurityState,
    broker_state: &HubBrokerState,
    plaintext: String,
    view_once: bool,
) -> Result<PreparedText, String> {
    if plaintext.is_empty() || plaintext.len() > MAX_TEXT_BYTES {
        return Err("OSL Chat text must be between 1 and 1000 UTF-8 bytes".to_owned());
    }
    let snapshot = broker_state.osl_chat_snapshot()?;
    let peer = security::require_manual_peer_scope_approved(
        core,
        "osl-chat",
        "osl-main",
        snapshot.person_id.clone(),
        snapshot.scope.clone(),
    )?;
    let prepared = broker::prepare_peer_prose_text(
        core,
        security_state,
        broker_state,
        &snapshot.context_token,
        plaintext.clone(),
    )?;
    let id = message_id();
    let envelope = InboxEnvelope {
        version: VERSION,
        message_id: id.clone(),
        cover_text: prepared.cover_text,
        expires_at: prepared.expires_at,
        view_once,
    };
    let payload = serde_json::to_vec(&envelope)
        .map_err(|_| "OSL Chat could not encode the message".to_owned())?;
    let identity = core
        .osl
        .identity
        .lock()
        .map_err(|_| "OSL identity state is unavailable".to_owned())?
        .clone()
        .ok_or_else(|| "OSL identity is not loaded".to_owned())?;
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
    let wire = ipc::wire_v2::encrypt_v3(
        &identity.x25519_secret,
        &identity.x25519_public,
        &recipients,
        ipc::wire_v2::MSG_TYPE_CONTENT,
        &payload,
    )
    .map_err(|_| "OSL Chat could not encrypt the message".to_owned())?;
    let bundle = STANDARD
        .decode(
            wire.strip_prefix("DPC0::")
                .ok_or_else(|| "OSL Chat produced an invalid message".to_owned())?,
        )
        .map_err(|_| "OSL Chat produced an invalid message".to_owned())?;
    let scope: Scope = snapshot
        .scope
        .clone()
        .try_into()
        .map_err(|_| "OSL Chat scope is invalid".to_owned())?;
    let client = core
        .osl
        .keyserver
        .lock()
        .map_err(|_| "OSL key server state is unavailable".to_owned())?
        .clone()
        .ok_or_else(|| "OSL key server is unavailable".to_owned())?;
    client
        .post_control_inbox(
            &identity,
            &snapshot.peer_osl_user_id,
            &scope.storage_key(),
            &bundle,
        )
        .map_err(|_| "OSL Chat could not deliver the message".to_owned())?;
    if !view_once {
        persist(
            core,
            store::StoredMessage {
                discord_message_id: id.clone(),
                channel_id: snapshot.conversation_binding,
                sender_discord_id: snapshot.self_osl_user_id.clone(),
                sender_osl_user_id: snapshot.self_osl_user_id,
                plaintext,
                decrypted_at: now(),
                burned: false,
            },
        );
    }
    Ok(PreparedText {
        message_id: id,
        expires_at: prepared.expires_at,
        person_to_person_e2ee: true,
        view_once,
        delivered_to_osl_inbox: true,
    })
}

pub fn open_text(
    core: &HubCoreState,
    _security_state: &HubSecurityState,
    broker_state: &HubBrokerState,
) -> Result<OpenedBatch, String> {
    let snapshot = broker_state.osl_chat_snapshot()?;
    let peer = security::require_manual_peer_scope_approved(
        core,
        "osl-chat",
        "osl-main",
        snapshot.person_id.clone(),
        snapshot.scope.clone(),
    )?;
    let scope: Scope = snapshot
        .scope
        .clone()
        .try_into()
        .map_err(|_| "OSL Chat scope is invalid".to_owned())?;
    let scope_id = scope.storage_key();
    let (identity, client) = transport(core)?;
    let items = client
        .get_control_inbox(&identity)
        .map_err(|_| "OSL Chat could not receive messages".to_owned())?;
    let fetched = items.len().min(MAX_BATCH);
    let mut messages = Vec::new();
    for item in items.into_iter().take(MAX_BATCH) {
        if item.sender_id != snapshot.peer_osl_user_id || item.scope_id != scope_id {
            continue;
        }
        let raw = match STANDARD.decode(&item.bundle_b64) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if raw.len() < 34
            || raw[0] != ipc::wire_v2::WIRE_VERSION_V3
            || raw[1] != ipc::wire_v2::MSG_TYPE_CONTENT
            || raw[2..34] != peer.peer_x25519_public
        {
            continue;
        }
        let wire = format!("DPC0::{}", STANDARD.encode(&raw));
        let opened = match ipc::wire_v2::decrypt_v3(
            &wire,
            &identity.x25519_secret,
            &identity.mlkem_decapsulation_key(),
        ) {
            Ok(value) if value.msg_type == ipc::wire_v2::MSG_TYPE_CONTENT => value,
            _ => continue,
        };
        let envelope: InboxEnvelope = match serde_json::from_slice(&opened.plaintext) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if envelope.version != VERSION
            || envelope.expires_at <= now()
            || !envelope.message_id.starts_with("peer-")
        {
            continue;
        }
        let opened = match broker::open_peer_prose_text(
            core,
            broker_state,
            &snapshot.context_token,
            snapshot.person_id.clone(),
            envelope.cover_text,
        ) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if !envelope.view_once {
            persist(
                core,
                store::StoredMessage {
                    discord_message_id: envelope.message_id.clone(),
                    channel_id: snapshot.conversation_binding.clone(),
                    sender_discord_id: snapshot.person_id.clone(),
                    sender_osl_user_id: snapshot.peer_osl_user_id.clone(),
                    plaintext: opened.plaintext.clone(),
                    decrypted_at: now(),
                    burned: false,
                },
            );
        }
        client
            .delete_control_inbox(&identity, &item.id)
            .map_err(|_| "OSL Chat could not finish receiving the message".to_owned())?;
        messages.push(OpenedText {
            plaintext: opened.plaintext,
            context_verified: true,
            person_to_person_e2ee: true,
            view_once_consumed: envelope.view_once,
            expires_at: envelope.expires_at,
        });
    }
    Ok(OpenedBatch {
        messages,
        pending_view_once: Vec::new(),
        acknowledgments: Vec::new(),
        fetched,
    })
}

pub fn history(
    core: &HubCoreState,
    broker_state: &HubBrokerState,
) -> Result<Vec<HistoryRow>, String> {
    let snapshot = broker_state.osl_chat_snapshot()?;
    let guard = core
        .osl
        .message_store
        .lock()
        .map_err(|_| "OSL Chat history is unavailable".to_owned())?;
    let Some(store) = guard.as_ref() else {
        return Ok(Vec::new());
    };
    let rows = store
        .list_by_channel(&snapshot.conversation_binding, 200)
        .map_err(|_| "OSL Chat history is unavailable".to_owned())?;
    Ok(rows
        .into_iter()
        .map(|row| HistoryRow {
            discord_message_id: row.discord_message_id,
            channel_id: row.channel_id,
            sender_discord_id: row.sender_discord_id,
            sender_osl_user_id: row.sender_osl_user_id,
            plaintext: row.plaintext,
            decrypted_at: row.decrypted_at,
            burned: row.burned,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_ids_are_bounded_opaque_and_unique() {
        let first = message_id();
        let second = message_id();
        assert!(first.starts_with("peer-"));
        assert_eq!(first.len(), 37);
        assert_ne!(first, second);
    }

    #[test]
    fn inbox_envelope_rejects_unknown_fields() {
        let value = serde_json::json!({
            "version": VERSION,
            "messageId": "peer-00000000000000000000000000000000",
            "coverText": "bounded pointer",
            "expiresAt": 100,
            "viewOnce": false,
            "unexpected": true
        });
        assert!(serde_json::from_value::<InboxEnvelope>(value).is_err());
    }
}
