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
//! | `0x06`     | CBOR-encoded [`SkdmRequest`]              |
//! | `0x07`     | CBOR-encoded [`SessionReset`]             |
//! | `0x08`     | native-overlay relay notice (JSON, broker) |
//! | `0x09`     | native-overlay receipt (JSON, broker)     |
//! | `0x0A`     | CBOR-encoded [`RevocationNotice`]         |
//! | `0x0B`     | CBOR-encoded [`RevocationAck`]            |
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
use std::fmt;

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

    /// A [`RevocationNotice`] carried more explicit message commitments than
    /// [`MAX_REVOCATION_MESSAGE_COMMITMENTS`]. Rejected outright — a burn that
    /// silently dropped targets would report success while leaving content
    /// alive.
    #[error("revocation notice carries too many message commitments: got {got}, max {max}")]
    TooManyCommitments { got: usize, max: usize },
}

// ---- Structs ----

/// Type=0x01: "burn this scope on receipt."
///
/// **Legacy. Do not send from new code.** Two defects, both fixed by
/// [`RevocationNotice`] (type=0x0A):
///
/// 1. `scope` is a plaintext [`Scope`] — server ids, channel ids, peer ids —
///    inside the envelope. `docs/design/burn-contract.md` requires notices to
///    carry "opaque commitments and authenticated metadata only—never
///    plaintext, service names, account handles, chat titles".
/// 2. There is no epoch and no sequence bound, so a receiver cannot tell a
///    fresh burn from a replayed one and cannot limit the damage to content
///    that existed when the burn was issued. The legacy receiver's response
///    was a permanent scope-level flag, which turns one stale or replayed
///    marker into a permanent denial of service on that conversation.
///
/// Kept only so an inbound 0x01 from an old peer can still be *honoured*. See
/// [`crate::revocation::legacy_burn_notice`], which converts one into a bounded
/// revocation at `burn_upto_seq = whatever we currently hold from that sender`
/// — never a permanent flag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BurnMarker {
    pub scope: Scope,
    pub burned_at: i64,
}

/// Type=0x0A: bilateral-burn revocation notice — "destroy the content **I
/// authored** in this conversation up to `burn_upto_seq`."
///
/// # Nothing here is plaintext
///
/// Every field is either a keyed commitment or an integer. The receiver does
/// not learn the scope *from* the notice; it recomputes
/// [`crate::revocation::scope_commitment`] over the conversations it already
/// holds for the authenticated sender and constant-time-compares. That keeps
/// the body free of service names, channel ids and account handles even though
/// it is already inside an authenticated envelope, and it means a notice
/// captured from one pair is meaningless to any other pair (the commitment key
/// is derived from the two identity public keys).
///
/// # Authorisation
///
/// A revocation authorises destruction of the **sender's own** content and
/// nothing else. The sender is the authenticated `encrypt_v3` sender, so no
/// consent grant, quorum or signature-over-scope is needed: you only ever
/// destroy content you authored. This is the rule the legacy client already
/// got right (`wipe_wrapped_keys_in_scope(.., Some(sender_discord_id))` —
/// "Their burn must not blank our own or other members' messages").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevocationNotice {
    /// `HMAC(scope_commit_key, "scope" ‖ storage_key)`. Opaque; the receiver
    /// matches it against its own scopes for this sender.
    pub scope_commitment: [u8; 32],
    /// Sender-local monotonic burn epoch for this (sender, scope). A notice
    /// whose epoch is `<= last_burn_epoch` is a replay and is ignored.
    pub burn_epoch: u64,
    /// Destroy content from this sender in this scope whose authenticated
    /// `send_seq` is `<= burn_upto_seq`. Never widened by a replay, so a
    /// captured notice cannot reach content sent after it was issued.
    pub burn_upto_seq: u64,
    /// Optional explicit per-message commitments
    /// (`HMAC(key, "msg" ‖ scope_commitment ‖ message_id)`), for the case where
    /// the burn targets specific messages rather than a whole prefix. Bounded
    /// by [`MAX_REVOCATION_MESSAGE_COMMITMENTS`]; decode rejects an overlong
    /// list rather than truncating it.
    pub message_commitments: Vec<[u8; 32]>,
    /// `HMAC(scope_commit_key, "burn" ‖ scope_commitment ‖ epoch ‖ upto_seq)`.
    /// Recomputed and constant-time-compared on receipt, so the same id can
    /// never be presented with different parameters.
    pub burn_id: [u8; 32],
    /// Unix seconds the notice was issued. Advisory only — freshness is
    /// enforced by the epoch, not the clock, because a burn must still apply
    /// after an arbitrarily long offline period.
    pub issued_at: i64,
}

/// Type=0x0B: revocation receipt. `(burn_id, applied)` and nothing else.
///
/// `applied == true` means "this burn is in force on my side". It is returned
/// both when the burn was applied by this notice and when it had already been
/// applied, so the ack reveals nothing about whether the peer still held the
/// content. `applied == false` means the notice was refused (bad `burn_id`,
/// id reused with different parameters, or a full replay journal) and the
/// sender should surface **Not acknowledged**, never "Deleted".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevocationAck {
    pub burn_id: [u8; 32],
    pub applied: bool,
}

/// Upper bound on `RevocationNotice::message_commitments`. Decode rejects a
/// longer list instead of silently dropping entries: a truncated burn would
/// leave content alive while reporting success.
pub const MAX_REVOCATION_MESSAGE_COMMITMENTS: usize = 256;

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
#[derive(Clone, PartialEq, Eq)]
pub struct SenderKeyDistribution {
    /// `scope.storage_key()` of the group/server/channel this chain
    /// targets. Stable across both peers because storage_key encodes
    /// only the scope kind + id (no peer-specific perspective).
    pub scope_storage_key: String,
    pub chain_id: u32,
    pub rotation_root: [u8; 32],
    pub physical_device_id: [u8; 32],
    pub sent_at: i64,
}

impl fmt::Debug for SenderKeyDistribution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SenderKeyDistribution")
            .field(
                "scope_storage_key",
                &crate::log_id::log_id(&self.scope_storage_key),
            )
            .field("chain_id", &self.chain_id)
            .field("rotation_root", &"[REDACTED]")
            .field("physical_device_id", &"[REDACTED]")
            .field("sent_at", &self.sent_at)
            .finish()
    }
}

/// Auto-recovery type=0x06: "I have been unable to decrypt your v=5
/// messages in this scope because I never received (or lost) your
/// sender-key — please re-emit the SKDM for this scope to me." Sent
/// inside a **v=2** message (ratchet-independent: the requester may
/// have no usable v=4 session to the sender). The receiving sender
/// force-re-emits one SKDM for `scope_storage_key` to the requester,
/// bypassing its "already installed" short-circuit.
///
/// `nonce` + `requested_at` exist for the recv-side replay/staleness
/// and rate-limit guards (a forged or replayed request must not be
/// able to amplify SKDM traffic). They carry no secret material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkdmRequest {
    /// `scope.storage_key()` of the scope whose SKDM is missing.
    pub scope_storage_key: String,
    /// Unix seconds; recv side drops requests outside its freshness
    /// window.
    pub requested_at: i64,
    /// Random per-request id; recv side dedupes replays within the
    /// freshness window.
    pub nonce: [u8; 16],
}

/// Auto-recovery type=0x07: "our v=4 Double Ratchet is desynced — I
/// have already dropped my ratchet_state for you; drop yours too so
/// the next v=4 send re-handshakes (`new_initiator` ↔ `new_responder`)."
/// Sent inside a **v=2** message because the v=4 ratchet itself is the
/// broken thing. Peer-scoped: the peer is the v=2 message sender, so
/// no scope field is needed — it targets that peer's whole v=4 DM
/// session (the same state shared by every group SKDM to that peer).
///
/// `nonce` + `requested_at` back the recv-side replay/staleness and
/// rate-limit guards: honoring a SESSION_RESET costs one re-handshake,
/// so an unthrottled forge/replay would be a decrypt-denial DoS. The
/// recv handler additionally requires an independent local decrypt
/// failure from the same peer before acting (act-on-symptom).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionReset {
    /// Unix seconds; recv side drops resets outside its freshness
    /// window.
    pub requested_at: i64,
    /// Random per-request id; recv side dedupes replays within the
    /// freshness window.
    pub nonce: [u8; 16],
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
    physical_device_id: [u8; 32],
    sent_at: i64,
}

#[derive(Serialize, Deserialize)]
struct SkdmRequestWire {
    scope_storage_key: String,
    requested_at: i64,
    nonce: [u8; 16],
}

#[derive(Serialize, Deserialize)]
struct SessionResetWire {
    requested_at: i64,
    nonce: [u8; 16],
}

#[derive(Serialize, Deserialize)]
struct RevocationNoticeWire {
    scope_commitment: [u8; 32],
    burn_epoch: u64,
    burn_upto_seq: u64,
    #[serde(default)]
    message_commitments: Vec<[u8; 32]>,
    burn_id: [u8; 32],
    issued_at: i64,
}

#[derive(Serialize, Deserialize)]
struct RevocationAckWire {
    burn_id: [u8; 32],
    applied: bool,
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
        physical_device_id: m.physical_device_id,
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
        physical_device_id: wire.physical_device_id,
        sent_at: wire.sent_at,
    })
}

pub fn serialize_skdm_request(m: &SkdmRequest) -> Result<Vec<u8>, ControlError> {
    cbor_encode(&SkdmRequestWire {
        scope_storage_key: m.scope_storage_key.clone(),
        requested_at: m.requested_at,
        nonce: m.nonce,
    })
}

pub fn deserialize_skdm_request(bytes: &[u8]) -> Result<SkdmRequest, ControlError> {
    let wire: SkdmRequestWire = cbor_decode(bytes)?;
    Ok(SkdmRequest {
        scope_storage_key: wire.scope_storage_key,
        requested_at: wire.requested_at,
        nonce: wire.nonce,
    })
}

pub fn serialize_session_reset(m: &SessionReset) -> Result<Vec<u8>, ControlError> {
    cbor_encode(&SessionResetWire {
        requested_at: m.requested_at,
        nonce: m.nonce,
    })
}

pub fn deserialize_session_reset(bytes: &[u8]) -> Result<SessionReset, ControlError> {
    let wire: SessionResetWire = cbor_decode(bytes)?;
    Ok(SessionReset {
        requested_at: wire.requested_at,
        nonce: wire.nonce,
    })
}

pub fn serialize_revocation_notice(m: &RevocationNotice) -> Result<Vec<u8>, ControlError> {
    if m.message_commitments.len() > MAX_REVOCATION_MESSAGE_COMMITMENTS {
        return Err(ControlError::TooManyCommitments {
            got: m.message_commitments.len(),
            max: MAX_REVOCATION_MESSAGE_COMMITMENTS,
        });
    }
    cbor_encode(&RevocationNoticeWire {
        scope_commitment: m.scope_commitment,
        burn_epoch: m.burn_epoch,
        burn_upto_seq: m.burn_upto_seq,
        message_commitments: m.message_commitments.clone(),
        burn_id: m.burn_id,
        issued_at: m.issued_at,
    })
}

pub fn deserialize_revocation_notice(bytes: &[u8]) -> Result<RevocationNotice, ControlError> {
    let wire: RevocationNoticeWire = cbor_decode(bytes)?;
    if wire.message_commitments.len() > MAX_REVOCATION_MESSAGE_COMMITMENTS {
        return Err(ControlError::TooManyCommitments {
            got: wire.message_commitments.len(),
            max: MAX_REVOCATION_MESSAGE_COMMITMENTS,
        });
    }
    Ok(RevocationNotice {
        scope_commitment: wire.scope_commitment,
        burn_epoch: wire.burn_epoch,
        burn_upto_seq: wire.burn_upto_seq,
        message_commitments: wire.message_commitments,
        burn_id: wire.burn_id,
        issued_at: wire.issued_at,
    })
}

pub fn serialize_revocation_ack(m: &RevocationAck) -> Result<Vec<u8>, ControlError> {
    cbor_encode(&RevocationAckWire {
        burn_id: m.burn_id,
        applied: m.applied,
    })
}

pub fn deserialize_revocation_ack(bytes: &[u8]) -> Result<RevocationAck, ControlError> {
    let wire: RevocationAckWire = cbor_decode(bytes)?;
    Ok(RevocationAck {
        burn_id: wire.burn_id,
        applied: wire.applied,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revocation_notice_round_trip_inline() {
        let m = RevocationNotice {
            scope_commitment: [3u8; 32],
            burn_epoch: 7,
            burn_upto_seq: 41,
            message_commitments: vec![[4u8; 32], [5u8; 32]],
            burn_id: [6u8; 32],
            issued_at: 1_700_000_000,
        };
        let bytes = serialize_revocation_notice(&m).unwrap();
        let back = deserialize_revocation_notice(&bytes).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn revocation_notice_rejects_overlong_commitment_list() {
        let m = RevocationNotice {
            scope_commitment: [3u8; 32],
            burn_epoch: 1,
            burn_upto_seq: 1,
            message_commitments: vec![[0u8; 32]; MAX_REVOCATION_MESSAGE_COMMITMENTS + 1],
            burn_id: [6u8; 32],
            issued_at: 1,
        };
        assert!(matches!(
            serialize_revocation_notice(&m),
            Err(ControlError::TooManyCommitments { .. })
        ));
        // And the decoder rejects too, so a hostile peer cannot bypass the
        // sender-side check by hand-crafting the CBOR.
        let hostile = cbor_encode(&RevocationNoticeWire {
            scope_commitment: [3u8; 32],
            burn_epoch: 1,
            burn_upto_seq: 1,
            message_commitments: vec![[0u8; 32]; MAX_REVOCATION_MESSAGE_COMMITMENTS + 1],
            burn_id: [6u8; 32],
            issued_at: 1,
        })
        .unwrap();
        assert!(matches!(
            deserialize_revocation_notice(&hostile),
            Err(ControlError::TooManyCommitments { .. })
        ));
    }

    #[test]
    fn revocation_ack_round_trip_inline() {
        for applied in [true, false] {
            let m = RevocationAck {
                burn_id: [8u8; 32],
                applied,
            };
            let bytes = serialize_revocation_ack(&m).unwrap();
            assert_eq!(deserialize_revocation_ack(&bytes).unwrap(), m);
        }
    }

    /// The ack must carry nothing beyond `(burn_id, applied)`. If a future
    /// edit adds a field, this size assertion fails and forces a review of
    /// whether the new field is an oracle.
    #[test]
    fn revocation_ack_carries_only_two_fields() {
        let bytes = serialize_revocation_ack(&RevocationAck {
            burn_id: [0u8; 32],
            applied: true,
        })
        .unwrap();
        let value: ciborium::value::Value = ciborium::from_reader(&bytes[..]).unwrap();
        let ciborium::value::Value::Map(entries) = value else {
            panic!("revocation ack must encode as a CBOR map");
        };
        assert_eq!(entries.len(), 2);
    }

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

    #[test]
    fn skdm_request_round_trip_inline() {
        let m = SkdmRequest {
            scope_storage_key: "gc:1502771310428819569".to_string(),
            requested_at: 1_700_000_000,
            nonce: [7u8; 16],
        };
        let bytes = serialize_skdm_request(&m).unwrap();
        let back = deserialize_skdm_request(&bytes).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn session_reset_round_trip_inline() {
        let m = SessionReset {
            requested_at: 1_700_000_001,
            nonce: [9u8; 16],
        };
        let bytes = serialize_session_reset(&m).unwrap();
        let back = deserialize_session_reset(&bytes).unwrap();
        assert_eq!(back, m);
    }
}
