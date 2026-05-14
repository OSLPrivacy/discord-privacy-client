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
//! | `msg_type` | Body shape                                |
//! |-----------:|-------------------------------------------|
//! | `0x00`     | UTF-8 plaintext (the only non-control)    |
//! | `0x01`     | CBOR-encoded [`BurnMarker`]               |
//! | `0x02`     | (removed in 9-C1: legacy whitelist inv)   |
//! | `0x03`     | (removed in 9-C1: legacy whitelist resp)  |
//! | `0x04`     | CBOR-encoded [`AttachmentEnvelope`]       |
//! | `0x05`     | CBOR-encoded [`SenderKeyDistribution`]    |
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
//!
//! (9-C1: WhitelistInvitation / WhitelistResponse have been
//! removed alongside the invitation handshake. Pre-C1 wire bytes
//! arriving at the recv path return `OSL_RESULT_LEGACY_HANDSHAKE_IGNORED`
//! and are dropped.)
//!
//! All timestamps are unix seconds (`i64`), matching the rest
//! of the codebase. `from_pubkey` rides as the raw 32-byte
//! X25519 public key so a recipient who doesn't yet know the
//! sender can populate `peer_map[sender].pubkey` immediately.

use crate::scope::{Scope, ScopeInput};
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

// 9-C1: `WhitelistInvitation` (0x02) + `WhitelistResponse` (0x03)
// removed alongside the invitation handshake. The recv path now
// surfaces a single "legacy handshake ignored" sentinel for any
// 0x02/0x03 wire bytes still floating around from pre-C1 clients.

/// Phase 8b: a single attachment's metadata inside an
/// [`AttachmentEnvelope`]. The recv side matches `random_filename`
/// against each Discord-reported `attachments[N].filename` to figure
/// out which CDN file to fetch + decrypt with this entry's `att_key`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentEnvelopeEntry {
    pub att_key: [u8; 32],
    pub original_filename: String,
    pub random_filename: String,
    pub mime_type: String,
}

/// Phase 8 type=0x04: "I'm sending you N attachments in this scope."
/// The message-text plaintext for an attachment send is a CBOR-encoded
/// instance of this struct. Each entry carries everything the
/// recipient needs to fetch + decrypt one CDN-hosted blob. Discord
/// allows up to 10 attachments per message; 8b folds them into a
/// single envelope so the cover stays in `payload_json.content`.
///
/// 8b note: this superseded an 8.0 single-attachment shape (struct
/// with att_key/original_filename/random_filename/mime_type fields).
/// Phase 8 had not yet shipped to recipients in production, so 8b
/// breaks compat without a version bump.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentEnvelope {
    pub attachments: Vec<AttachmentEnvelopeEntry>,
}

/// Phase 9-A3 type=0x05: "Here is my sender-keys rotation root for
/// this group/server scope; please install or rotate the receiver
/// chain you hold for me." Sent inside a v=4 message (so the wrap
/// leg provides PQ identity binding); the receiver's v=4 decode path
/// routes the plaintext to the SKDM handler instead of surfacing it
/// as user-visible content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SenderKeyDistribution {
    /// `scope.storage_key()` of the group/server/channel this chain
    /// targets. Stable across both peers because storage_key encodes
    /// only the scope kind + id (no peer-specific perspective).
    pub scope_storage_key: String,
    pub chain_id: u32,
    pub rotation_root: [u8; 32],
    pub sent_at: i64,
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

// 9-C1: `WhitelistInvitationWire` / `WhitelistResponseWire`
// removed. Legacy 0x02/0x03 wire bytes are short-circuited at the
// dispatcher; we never deserialize the payload anymore.

#[derive(Serialize, Deserialize)]
struct AttachmentEnvelopeEntryWire {
    att_key: [u8; 32],
    original_filename: String,
    random_filename: String,
    mime_type: String,
}

#[derive(Serialize, Deserialize)]
struct AttachmentEnvelopeWire {
    attachments: Vec<AttachmentEnvelopeEntryWire>,
}

#[derive(Serialize, Deserialize)]
struct SenderKeyDistributionWire {
    scope_storage_key: String,
    chain_id: u32,
    rotation_root: [u8; 32],
    sent_at: i64,
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

// 9-C1: `serialize_whitelist_invitation` / `deserialize_whitelist_invitation`
// / `serialize_whitelist_response` / `deserialize_whitelist_response` all
// removed. The recv-side dispatcher returns
// OSL_RESULT_LEGACY_HANDSHAKE_IGNORED for any 0x02/0x03 wire bytes.

pub fn serialize_attachment_envelope(m: &AttachmentEnvelope) -> Result<Vec<u8>, ControlError> {
    cbor_encode(&AttachmentEnvelopeWire {
        attachments: m
            .attachments
            .iter()
            .map(|e| AttachmentEnvelopeEntryWire {
                att_key: e.att_key,
                original_filename: e.original_filename.clone(),
                random_filename: e.random_filename.clone(),
                mime_type: e.mime_type.clone(),
            })
            .collect(),
    })
}

pub fn deserialize_attachment_envelope(bytes: &[u8]) -> Result<AttachmentEnvelope, ControlError> {
    let wire: AttachmentEnvelopeWire = cbor_decode(bytes)?;
    Ok(AttachmentEnvelope {
        attachments: wire
            .attachments
            .into_iter()
            .map(|w| AttachmentEnvelopeEntry {
                att_key: w.att_key,
                original_filename: w.original_filename,
                random_filename: w.random_filename,
                mime_type: w.mime_type,
            })
            .collect(),
    })
}

pub fn serialize_sender_key_distribution(
    m: &SenderKeyDistribution,
) -> Result<Vec<u8>, ControlError> {
    cbor_encode(&SenderKeyDistributionWire {
        scope_storage_key: m.scope_storage_key.clone(),
        chain_id: m.chain_id,
        rotation_root: m.rotation_root,
        sent_at: m.sent_at,
    })
}

pub fn deserialize_sender_key_distribution(
    bytes: &[u8],
) -> Result<SenderKeyDistribution, ControlError> {
    let wire: SenderKeyDistributionWire = cbor_decode(bytes)?;
    Ok(SenderKeyDistribution {
        scope_storage_key: wire.scope_storage_key,
        chain_id: wire.chain_id,
        rotation_root: wire.rotation_root,
        sent_at: wire.sent_at,
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

    // 9-C1: invitation/response inline round-trip tests removed
    // alongside the wire types they exercised.
}
