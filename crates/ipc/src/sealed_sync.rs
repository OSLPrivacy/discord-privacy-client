//! TASK 4817: the sealed self-message sync path.
//!
//! Sync stays on the ordinary self-message path defined by TASK 4807: a sync
//! record is an ordinary `MSG_TYPE_CONTENT` wire-v3 message addressed to the
//! account's own devices. This module adds the confidentiality half of that
//! contract:
//!
//! * Every value is classified by the TASK 4811 registry **before** it is
//!   serialized, so a refused or unclassified kind never reaches a buffer that
//!   a relay could see.
//! * The serialized body is padded to a shipping text bucket
//!   ([`crypto::padding::TEXT_BUCKETS`]) and sealed with
//!   [`crate::wire_v2::encrypt_v3`] to the destination device key before it is
//!   handed to any transport. The relay is handed base64 of a PQ-hybrid
//!   AEAD envelope and nothing else.
//! * Merge is unreachable without an authenticated open.
//!   [`merge_authenticated`] takes an [`AuthenticatedSyncBody`], and the only
//!   constructor for that type is [`open_sync_message`], which requires the
//!   destination device's secret keys, a matching AEAD tag and the pinned
//!   sender identity key. There is no plaintext merge entry point.
//!
//! Ordinary user messages on the same conversation are sealed by the same
//! [`seal_content_v3`] helper, so the version byte, message kind, recipient
//! count, header length and size class a server can see are produced by one
//! code path for both.

use crate::ordinary_sync::{server_visible_v3_header, OrdinarySyncPayload, ServerVisibleHeader};
use crate::sync_policy::{check_sync_payload_before_send, SyncPayloadCheckError, SyncRoute};
use crate::wire_v2::{self, RecipientV3, V2Error, MSG_TYPE_CONTENT};
use crypto::{aes_gcm, padding, x25519};
use serde::{Deserialize, Serialize};

/// Errors from the sealed sync path. Every variant is a refusal to move
/// bytes; none of them degrade to an unsealed send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SealedSyncError {
    /// The TASK 4811 registry refused this kind before serialization.
    Policy(SyncPayloadCheckError),
    /// Body too large for a shipping text bucket.
    Padding(String),
    /// Wire-format seal/open failure, including a failed AEAD tag.
    Wire(String),
    /// Body serialization/deserialization failure.
    Serialize(String),
    /// The opened message was not an ordinary content message.
    WrongMessageKind(u8),
    /// The envelope did not carry a body of a server-visible size class.
    UnknownSizeClass(usize),
}

impl std::fmt::Display for SealedSyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SealedSyncError::Policy(err) => write!(f, "sync policy refused the payload: {err:?}"),
            SealedSyncError::Padding(err) => write!(f, "sync body padding: {err}"),
            SealedSyncError::Wire(err) => write!(f, "sync wire: {err}"),
            SealedSyncError::Serialize(err) => write!(f, "sync body serialization: {err}"),
            SealedSyncError::WrongMessageKind(kind) => {
                write!(f, "sync body carried message kind {kind:#04x}")
            }
            SealedSyncError::UnknownSizeClass(len) => {
                write!(f, "envelope body length {len} is not a text bucket")
            }
        }
    }
}

impl From<V2Error> for SealedSyncError {
    fn from(err: V2Error) -> Self {
        SealedSyncError::Wire(format!("{err:?}"))
    }
}

/// One allowed-kind value as it exists on the source device.
///
/// `field` is the persisted field name and `value` the stored value. Both are
/// plaintext on the source device and neither may appear on a relay surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncValue {
    pub kind: String,
    pub field: String,
    pub value: String,
    pub counter: u64,
    pub device_id: String,
}

impl SyncValue {
    pub fn new(
        kind: impl Into<String>,
        field: impl Into<String>,
        value: impl Into<String>,
        counter: u64,
        device_id: impl Into<String>,
    ) -> Self {
        Self {
            kind: kind.into(),
            field: field.into(),
            value: value.into(),
            counter,
            device_id: device_id.into(),
        }
    }
}

/// A sealed message plus the source-side facts a confidentiality audit needs.
///
/// `source_serialization` is the plaintext body the sender serialized. It never
/// leaves the source device; it is returned so a caller can prove the captured
/// wire body is not that serialization.
#[derive(Debug, Clone)]
pub struct SealedSyncMessage {
    pub wire: String,
    pub source_serialization: Vec<u8>,
    pub padded_plaintext_len: usize,
}

/// Serialize the body a sync message carries.
///
/// This is the "source serialization" a relay must never observe: field names
/// paired with their values, in the clear, exactly as the source device holds
/// them.
pub fn serialize_sync_body(payload: &OrdinarySyncPayload) -> Result<Vec<u8>, SealedSyncError> {
    serde_json::to_vec(payload).map_err(|err| SealedSyncError::Serialize(err.to_string()))
}

/// Build the sync body for one allowed kind.
pub fn sync_body_for(
    value: &SyncValue,
    source_device_id: &str,
    target_device_id: &str,
) -> OrdinarySyncPayload {
    let mut fields = serde_json::Map::new();
    fields.insert(
        value.field.clone(),
        serde_json::Value::String(value.value.clone()),
    );
    let mut stamp = serde_json::Map::new();
    stamp.insert(
        "counter".to_owned(),
        serde_json::Value::from(value.counter),
    );
    stamp.insert(
        "device_id".to_owned(),
        serde_json::Value::String(value.device_id.clone()),
    );
    let mut body = serde_json::Map::new();
    body.insert("kind".to_owned(), serde_json::Value::String(value.kind.clone()));
    body.insert("fields".to_owned(), serde_json::Value::Object(fields));
    body.insert("stamp".to_owned(), serde_json::Value::Object(stamp));
    OrdinarySyncPayload::new(
        source_device_id,
        target_device_id,
        serde_json::Value::Object(body),
    )
}

/// Seal one ordinary content plaintext to the destination device keys.
///
/// Both sync records and ordinary user messages go through here, so a server
/// sees one wire version, one message kind, one header shape and one size-class
/// ladder for both.
pub fn seal_content_v3(
    plaintext: &[u8],
    sender_ik_sk: &x25519::SecretKey,
    sender_ik_pub: &x25519::PublicKey,
    recipients: &[RecipientV3],
) -> Result<SealedSyncMessage, SealedSyncError> {
    let padded = padding::pad_text(plaintext)
        .map_err(|err| SealedSyncError::Padding(format!("{err:?}")))?;
    let wire = wire_v2::encrypt_v3(
        sender_ik_sk,
        sender_ik_pub,
        recipients,
        MSG_TYPE_CONTENT,
        &padded,
    )?;
    Ok(SealedSyncMessage {
        wire,
        source_serialization: plaintext.to_vec(),
        padded_plaintext_len: padded.len(),
    })
}

/// Classify, serialize, pad and seal one allowed sync value.
///
/// The registry check runs first and on failure returns before any buffer
/// holding the value exists outside this call.
pub fn seal_allowed_sync_value(
    value: &SyncValue,
    source_device_id: &str,
    target_device_id: &str,
    sender_ik_sk: &x25519::SecretKey,
    sender_ik_pub: &x25519::PublicKey,
    recipients: &[RecipientV3],
) -> Result<SealedSyncMessage, SealedSyncError> {
    check_sync_payload_before_send(SyncRoute::Automatic, false, [value.kind.as_str()])
        .map_err(SealedSyncError::Policy)?;
    let payload = sync_body_for(value, source_device_id, target_device_id);
    let body = serialize_sync_body(&payload)?;
    seal_content_v3(&body, sender_ik_sk, sender_ik_pub, recipients)
}

/// Seal an ordinary user message on the same self-conversation.
pub fn seal_ordinary_message(
    text: &str,
    sender_ik_sk: &x25519::SecretKey,
    sender_ik_pub: &x25519::PublicKey,
    recipients: &[RecipientV3],
) -> Result<SealedSyncMessage, SealedSyncError> {
    seal_content_v3(text.as_bytes(), sender_ik_sk, sender_ik_pub, recipients)
}

/// A sync body that has already authenticated.
///
/// Constructed only by [`open_sync_message`], which requires the destination
/// device's X25519 and ML-KEM secrets, a body whose AEAD tag verifies, and a
/// sender identity key equal to the pinned paired device. Holding one of these
/// is the proof that the bytes came from the paired device unmodified; merge
/// takes it by reference and there is no other way in.
#[derive(Debug, Clone)]
pub struct AuthenticatedSyncBody {
    payload: OrdinarySyncPayload,
    sender_ik: [u8; 32],
    kind: String,
}

impl AuthenticatedSyncBody {
    pub fn payload(&self) -> &OrdinarySyncPayload {
        &self.payload
    }

    pub fn sender_ik(&self) -> &[u8; 32] {
        &self.sender_ik
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }
}

/// Open a sealed sync message with the destination device's keys.
///
/// Authentication happens here and only here: `decrypt_v3_for_sender` refuses a
/// body whose AEAD tag does not verify and refuses a sender identity key other
/// than `expected_sender_ik`. The 4811 registry is re-checked on the receiving
/// side so an envelope cannot smuggle a refused kind past classification.
pub fn open_sync_message(
    wire: &str,
    recipient_ik_sk: &x25519::SecretKey,
    recipient_mlkem_sk: &crypto::ml_kem_768::DecapsulationKey,
    expected_sender_ik: &x25519::PublicKey,
) -> Result<AuthenticatedSyncBody, SealedSyncError> {
    let decrypted = wire_v2::decrypt_v3_for_sender(
        wire,
        recipient_ik_sk,
        recipient_mlkem_sk,
        expected_sender_ik,
    )?;
    if decrypted.msg_type != MSG_TYPE_CONTENT {
        return Err(SealedSyncError::WrongMessageKind(decrypted.msg_type));
    }
    let body = padding::unpad_text(&decrypted.plaintext)
        .map_err(|err| SealedSyncError::Padding(format!("{err:?}")))?;
    let payload: OrdinarySyncPayload =
        serde_json::from_slice(&body).map_err(|err| SealedSyncError::Serialize(err.to_string()))?;
    let kind = payload
        .body
        .get("kind")
        .and_then(|kind| kind.as_str())
        .ok_or_else(|| SealedSyncError::Serialize("sync body has no kind".to_owned()))?
        .to_owned();
    check_sync_payload_before_send(SyncRoute::Automatic, false, [kind.as_str()])
        .map_err(SealedSyncError::Policy)?;
    Ok(AuthenticatedSyncBody {
        payload,
        sender_ik: *expected_sender_ik.as_bytes(),
        kind,
    })
}

/// Merge an authenticated sync body into the receiving device's field state.
///
/// Takes [`AuthenticatedSyncBody`] rather than bytes: a caller that has not
/// opened and authenticated the envelope has nothing to pass. Merge itself is
/// TASK 4807's rule — later stamp wins per field, nothing else is disturbed.
pub fn merge_authenticated(
    state: &mut crate::ordinary_sync::FieldWiseState,
    authenticated: &AuthenticatedSyncBody,
) -> Result<usize, SealedSyncError> {
    let body = authenticated.payload().body.clone();
    let counter = body
        .get("stamp")
        .and_then(|stamp| stamp.get("counter"))
        .and_then(|counter| counter.as_u64())
        .ok_or_else(|| SealedSyncError::Serialize("sync body has no stamp counter".to_owned()))?;
    let device_id = body
        .get("stamp")
        .and_then(|stamp| stamp.get("device_id"))
        .and_then(|device| device.as_str())
        .ok_or_else(|| SealedSyncError::Serialize("sync body has no stamp device".to_owned()))?
        .to_owned();
    let fields = body
        .get("fields")
        .and_then(|fields| fields.as_object())
        .ok_or_else(|| SealedSyncError::Serialize("sync body has no fields".to_owned()))?;
    let mut merged = 0usize;
    for (name, value) in fields {
        state.set_field(
            name.clone(),
            value.clone(),
            crate::ordinary_sync::VersionStamp::new(counter, device_id.clone()),
        );
        merged += 1;
    }
    Ok(merged)
}

/// The server-visible header of a sealed message (TASK 4807's view).
pub fn server_visible_header(wire: &str) -> Result<ServerVisibleHeader, SealedSyncError> {
    Ok(server_visible_v3_header(wire)?)
}

/// The size class a server can see, computed only from the envelope bytes.
///
/// The body ciphertext is the padded plaintext plus one AEAD tag, and padding
/// runs before the seal, so the recoverable number is a text bucket and not the
/// true body length.
pub fn server_visible_size_class(wire: &str) -> Result<usize, SealedSyncError> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine as _;

    let header = server_visible_header(wire)?;
    let raw = STANDARD
        .decode(
            wire.strip_prefix("DPC0::")
                .ok_or_else(|| SealedSyncError::Wire("missing DPC0 prefix".to_owned()))?,
        )
        .map_err(|err| SealedSyncError::Wire(err.to_string()))?;
    let trailer = raw
        .len()
        .checked_sub(header.header_bytes + aes_gcm::NONCE_SIZE + aes_gcm::TAG_SIZE)
        .ok_or(SealedSyncError::UnknownSizeClass(raw.len()))?;
    if !padding::TEXT_BUCKETS.contains(&trailer) {
        return Err(SealedSyncError::UnknownSizeClass(trailer));
    }
    Ok(trailer)
}

/// The receiving device's at-rest sync state.
///
/// Everything the destination device keeps between runs — its identity seed,
/// the pinned paired-device identity key and the merged sync fields — is
/// sealed under the device key through [`crate::secure_local_store`], so a
/// restart re-derives the device keys and re-opens the merged state rather
/// than reading anything in the clear.
pub mod at_rest {
    use crate::secure_local_store::{open_record, seal_record, RecordId};
    use crypto::aead::Key as AeadKey;
    use std::path::Path;

    pub const NAMESPACE: &str = "task4817-sync";

    fn record_id(name: &str) -> RecordId {
        RecordId::new("task4817-sync", name.to_owned())
    }

    /// Seal `plaintext` under the device key and write it into `dir`.
    pub fn write_record(
        dir: &Path,
        device_key: &[u8; 32],
        name: &str,
        plaintext: &[u8],
    ) -> Result<(), String> {
        let key = AeadKey::from_bytes(*device_key);
        let blob = seal_record(&key, &record_id(name), plaintext).map_err(|err| err.to_string())?;
        std::fs::create_dir_all(dir).map_err(|err| err.to_string())?;
        std::fs::write(dir.join(format!("{name}.bin")), blob).map_err(|err| err.to_string())
    }

    /// Open a record written by [`write_record`]. A wrong device key or a
    /// modified blob fails authentication instead of returning bytes.
    pub fn read_record(dir: &Path, device_key: &[u8; 32], name: &str) -> Result<Vec<u8>, String> {
        let key = AeadKey::from_bytes(*device_key);
        let blob = std::fs::read(dir.join(format!("{name}.bin"))).map_err(|err| err.to_string())?;
        open_record(&key, &record_id(name), &blob).map_err(|err| err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ordinary_sync::FieldWiseState;
    use crypto::ml_kem_768;

    fn device() -> (
        x25519::SecretKey,
        x25519::PublicKey,
        ml_kem_768::DecapsulationKey,
        ml_kem_768::EncapsulationKey,
    ) {
        let (sk, pk) = x25519::generate_keypair();
        let (dk, ek) = ml_kem_768::generate_keypair();
        (sk, pk, dk, ek)
    }

    #[test]
    fn seals_an_allowed_kind_and_merges_only_after_authenticating() {
        let (a_sk, a_pk, _a_dk, a_ek) = device();
        let (b_sk, b_pk, b_dk, b_ek) = device();
        let recipients = vec![
            RecipientV3 {
                x25519_pub: a_pk,
                mlkem_pub: a_ek,
            },
            RecipientV3 {
                x25519_pub: b_pk,
                mlkem_pub: b_ek,
            },
        ];
        let value = SyncValue::new("friend_roster", "roster_digest", "MARKED-VALUE", 7, "A");
        let sealed =
            seal_allowed_sync_value(&value, "A", "B", &a_sk, &a_pk, &recipients).expect("seal");
        assert!(sealed.wire.starts_with("DPC0::"));
        assert!(!sealed.wire.contains("MARKED-VALUE"));

        let authenticated = open_sync_message(&sealed.wire, &b_sk, &b_dk, &a_pk).expect("open");
        let mut state = FieldWiseState::default();
        assert_eq!(merge_authenticated(&mut state, &authenticated).expect("merge"), 1);
        assert_eq!(
            state.get("roster_digest").and_then(|value| value.as_str()),
            Some("MARKED-VALUE")
        );
    }

    #[test]
    fn refuses_a_refused_kind_before_it_is_serialized() {
        let (a_sk, a_pk, _a_dk, a_ek) = device();
        let recipients = vec![RecipientV3 {
            x25519_pub: a_pk,
            mlkem_pub: a_ek,
        }];
        let value = SyncValue::new("carrier_sign_ins", "token", "MARKED-VALUE", 1, "A");
        let err = seal_allowed_sync_value(&value, "A", "B", &a_sk, &a_pk, &recipients)
            .expect_err("refused kind must not seal");
        assert!(matches!(err, SealedSyncError::Policy(_)));
    }

    #[test]
    fn a_tampered_body_never_authenticates() {
        let (a_sk, a_pk, _a_dk, a_ek) = device();
        let (b_sk, b_pk, b_dk, b_ek) = device();
        let recipients = vec![
            RecipientV3 {
                x25519_pub: a_pk,
                mlkem_pub: a_ek,
            },
            RecipientV3 {
                x25519_pub: b_pk,
                mlkem_pub: b_ek,
            },
        ];
        let value = SyncValue::new("friend_roster", "roster_digest", "MARKED-VALUE", 7, "A");
        let sealed =
            seal_allowed_sync_value(&value, "A", "B", &a_sk, &a_pk, &recipients).expect("seal");

        use base64::engine::general_purpose::STANDARD;
        use base64::Engine as _;
        let mut raw = STANDARD
            .decode(sealed.wire.strip_prefix("DPC0::").expect("prefix"))
            .expect("base64");
        let last = raw.len() - 1;
        raw[last] ^= 0x01;
        let tampered = format!("DPC0::{}", STANDARD.encode(&raw));
        assert!(open_sync_message(&tampered, &b_sk, &b_dk, &a_pk).is_err());
    }
}
