//! Phase 7 control-message bodies.
//!
//! Spec: `docs/phase-7-design.md` §§ 3 (burn semantics), 7
//! (receiver-side notification flow), and the
//! `MSG_TYPE_BURN` / `MSG_TYPE_WHITELIST_INVITATION` /
//! `MSG_TYPE_WHITELIST_RESPONSE` constants in
//! [`crate::wire_v2`].
//!
//! ## Wire integration
//!
//! Each struct serializes to a self-describing CBOR byte
//! sequence. The bytes ride the v=2 wire format as the **body**
//! payload — the `type` byte in the v=2 header tells the recv
//! path which struct to deserialize:
//!
//! | `msg_type` | Body shape                               |
//! |-----------:|------------------------------------------|
//! | `0x00`     | UTF-8 plaintext (the only non-control)    |
//! | `0x01`     | CBOR-encoded [`BurnMarker`]              |
//! | `0x02`     | CBOR-encoded [`WhitelistInvitation`]     |
//! | `0x03`     | CBOR-encoded [`WhitelistResponse`]       |
//!
//! ## Serialization choice: CBOR
//!
//! Chosen over a hand-rolled binary format because control
//! messages are admin payloads, not bulk traffic — wire size is
//! noise compared to the AES-GCM framing they're nested in, and
//! schema flexibility matters more (we'll add fields as Phase 7c
//! / 7d wire up the UI). `ciborium` is the actively-maintained
//! pure-Rust implementation (`serde_cbor` is abandoned). CBOR is
//! self-describing, so a future version that adds optional fields
//! parses cleanly against older code via `#[serde(default)]`.
//!
//! ## Fields and their semantics
//!
//! Each struct mirrors the corresponding control-message
//! description in the design doc:
//!
//! - [`BurnMarker`] (§3): "burn for this scope, at this
//!   timestamp." Recipient wipes its decryption capability for
//!   the named scope.
//! - [`WhitelistInvitation`] (§7.1): "I (`from_discord_id`,
//!   `from_pubkey`) am inviting you to decrypt my messages in
//!   this scope, sent at this time." Recipient surfaces a
//!   persistent banner.
//! - [`WhitelistResponse`] (§7.3 / §7.4): "I accept/decline your
//!   invitation for this scope at this time."
//!
//! All timestamps are unix seconds (`i64`), matching the rest
//! of the codebase. `from_pubkey` rides as the raw 32-byte
//! X25519 public key so a recipient who doesn't yet know the
//! sender can populate `peer_map[sender].pubkey` immediately.

use crate::scope::{Scope, ScopeInput};
use crypto::x25519;
use serde::{Deserialize, Serialize};

// ---- Errors ----

#[derive(Debug, thiserror::Error)]
pub enum ControlError {
    /// CBOR encode/decode failed.
    #[error("control message CBOR error: {0}")]
    Cbor(String),

    /// Inner `Scope` validation failed (e.g. missing
    /// server_id/channel_id for a server-channel kind).
    #[error("control message scope invalid: {0}")]
    Scope(#[from] crate::scope::ScopeError),

    /// `from_pubkey` was not 32 bytes.
    #[error("control message pubkey wrong length: got {got}, want 32")]
    BadPubkey { got: usize },
}

// ---- Structs ----

/// Type=0x01: "burn this scope on receipt."
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BurnMarker {
    pub scope: Scope,
    pub burned_at: i64,
}

/// Type=0x02: "I'm inviting you to decrypt my messages in this scope."
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhitelistInvitation {
    pub from_discord_id: String,
    pub from_pubkey: x25519::PublicKey,
    pub scope: Scope,
    pub sent_at: i64,
}

/// Type=0x03: "I accept/decline the invitation for this scope."
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhitelistResponse {
    pub scope: Scope,
    pub accepted: bool,
    pub responded_at: i64,
}

/// Phase 8 type=0x04: "I'm sending you an attachment in this scope."
/// The message-text plaintext for an attachment send is a CBOR-encoded
/// instance of this struct — it carries everything the recipient
/// needs to fetch + decrypt the CDN-hosted blob. The plaintext-side
/// `random_filename` is whatever name we uploaded to Discord with
/// (so the recv side can match it against `attachments[N].filename`
/// when multiple attachments are present in the same message).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentEnvelope {
    pub att_key: [u8; 32],
    pub original_filename: String,
    pub random_filename: String,
    pub mime_type: String,
}

// ---- CBOR wire reps ----
//
// We could derive Serialize/Deserialize directly on the structs
// above, but two of them carry non-serde types (`Scope` uses a
// custom storage_key in JSON; `x25519::PublicKey` is opaque
// bytes). Routing through intermediate "wire" structs keeps the
// public API ergonomic while staying schema-stable.

#[derive(Serialize, Deserialize)]
struct BurnMarkerWire {
    scope: ScopeInput,
    burned_at: i64,
}

#[derive(Serialize, Deserialize)]
struct WhitelistInvitationWire {
    from_discord_id: String,
    from_pubkey: [u8; 32],
    scope: ScopeInput,
    sent_at: i64,
}

#[derive(Serialize, Deserialize)]
struct WhitelistResponseWire {
    scope: ScopeInput,
    accepted: bool,
    responded_at: i64,
}

#[derive(Serialize, Deserialize)]
struct AttachmentEnvelopeWire {
    att_key: [u8; 32],
    original_filename: String,
    random_filename: String,
    mime_type: String,
}

// ---- Serialize ----

fn cbor_encode<T: Serialize>(v: &T) -> Result<Vec<u8>, ControlError> {
    let mut buf = Vec::with_capacity(64);
    ciborium::into_writer(v, &mut buf).map_err(|e| ControlError::Cbor(format!("encode: {e}")))?;
    Ok(buf)
}

fn cbor_decode<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T, ControlError> {
    ciborium::from_reader(bytes).map_err(|e| ControlError::Cbor(format!("decode: {e}")))
}

pub fn serialize_burn_marker(m: &BurnMarker) -> Result<Vec<u8>, ControlError> {
    cbor_encode(&BurnMarkerWire {
        scope: ScopeInput::from(&m.scope),
        burned_at: m.burned_at,
    })
}

pub fn deserialize_burn_marker(bytes: &[u8]) -> Result<BurnMarker, ControlError> {
    let wire: BurnMarkerWire = cbor_decode(bytes)?;
    Ok(BurnMarker {
        scope: Scope::try_from(wire.scope)?,
        burned_at: wire.burned_at,
    })
}

pub fn serialize_whitelist_invitation(m: &WhitelistInvitation) -> Result<Vec<u8>, ControlError> {
    cbor_encode(&WhitelistInvitationWire {
        from_discord_id: m.from_discord_id.clone(),
        from_pubkey: *m.from_pubkey.as_bytes(),
        scope: ScopeInput::from(&m.scope),
        sent_at: m.sent_at,
    })
}

pub fn deserialize_whitelist_invitation(bytes: &[u8]) -> Result<WhitelistInvitation, ControlError> {
    let wire: WhitelistInvitationWire = cbor_decode(bytes)?;
    Ok(WhitelistInvitation {
        from_discord_id: wire.from_discord_id,
        from_pubkey: x25519::PublicKey::from_bytes(wire.from_pubkey),
        scope: Scope::try_from(wire.scope)?,
        sent_at: wire.sent_at,
    })
}

pub fn serialize_whitelist_response(m: &WhitelistResponse) -> Result<Vec<u8>, ControlError> {
    cbor_encode(&WhitelistResponseWire {
        scope: ScopeInput::from(&m.scope),
        accepted: m.accepted,
        responded_at: m.responded_at,
    })
}

pub fn deserialize_whitelist_response(bytes: &[u8]) -> Result<WhitelistResponse, ControlError> {
    let wire: WhitelistResponseWire = cbor_decode(bytes)?;
    Ok(WhitelistResponse {
        scope: Scope::try_from(wire.scope)?,
        accepted: wire.accepted,
        responded_at: wire.responded_at,
    })
}

pub fn serialize_attachment_envelope(m: &AttachmentEnvelope) -> Result<Vec<u8>, ControlError> {
    cbor_encode(&AttachmentEnvelopeWire {
        att_key: m.att_key,
        original_filename: m.original_filename.clone(),
        random_filename: m.random_filename.clone(),
        mime_type: m.mime_type.clone(),
    })
}

pub fn deserialize_attachment_envelope(bytes: &[u8]) -> Result<AttachmentEnvelope, ControlError> {
    let wire: AttachmentEnvelopeWire = cbor_decode(bytes)?;
    Ok(AttachmentEnvelope {
        att_key: wire.att_key,
        original_filename: wire.original_filename,
        random_filename: wire.random_filename,
        mime_type: wire.mime_type,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn burn_marker_round_trip_inline() {
        let m = BurnMarker {
            scope: Scope::dm("henry_id"),
            burned_at: 1_700_000_000,
        };
        let bytes = serialize_burn_marker(&m).unwrap();
        let back = deserialize_burn_marker(&bytes).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn invitation_round_trip_inline() {
        let (_sk, pk) = x25519::generate_keypair();
        let m = WhitelistInvitation {
            from_discord_id: "1477008451799482419".to_string(),
            from_pubkey: pk,
            scope: Scope::server_channel("9876", "5432"),
            sent_at: 1_700_000_001,
        };
        let bytes = serialize_whitelist_invitation(&m).unwrap();
        let back = deserialize_whitelist_invitation(&bytes).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn response_round_trip_inline() {
        let m = WhitelistResponse {
            scope: Scope::gc("gc-123"),
            accepted: true,
            responded_at: 1_700_000_002,
        };
        let bytes = serialize_whitelist_response(&m).unwrap();
        let back = deserialize_whitelist_response(&bytes).unwrap();
        assert_eq!(back, m);
    }
}
