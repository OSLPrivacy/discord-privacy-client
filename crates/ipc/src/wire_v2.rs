//! Phase 7 wire format v=2: AES-256-GCM body + per-recipient
//! ECDH-wrapped ephemeral key.
//!
//! Spec: `docs/phase-7-design.md` §4.2.
//!
//! ## On-wire layout
//!
//! `DPC0::<base64(version=2 || type || N || (hash, len, wrapped)* || nonce || ciphertext || tag)>`
//!
//! ```text
//! version          : u8   = 0x02
//! type             : u8   = 0x00 content
//!                         | 0x01 burn marker
//!                         | 0x02 whitelist invitation
//!                         | 0x03 whitelist response
//!                         | 0x04..0xFF reserved
//! recipient_count  : u8   = N (1..=255)
//! recipient_slots  : N × (
//!     pubkey_hash  : [u8; 8]    // first 8 bytes of SHA-256(recipient_pubkey)
//!     wrapped_len  : u16 LE     // 60 in v=2 (12 nonce + 32 key + 16 tag)
//!     wrapped      : [u8; wrapped_len]
//!                                // nonce(12) || GCM(wrap_key, K)
//! )
//! body_nonce       : [u8; 12]   // AES-GCM nonce for the body cipher
//! body_ciphertext  : variable    // AES-256-GCM(K, plaintext, AAD=domain_sep)
//! body_tag         : [u8; 16]   // (last 16 bytes of `aes_gcm::seal` output)
//! ```
//!
//! ## Wrap key derivation
//!
//! Per recipient, the sender computes `ss = X25519(sender_sk,
//! recipient_pk)`, then derives the 32-byte wrap key:
//!
//! ```text
//! wrap_key = HKDF-SHA256(salt=∅, ikm=ss, info="OSL/P7/wrap-key/v2")
//! ```
//!
//! HKDF is required by RFC 7748 §6.1 — the raw X25519 output has
//! known biases and must not be used as a key directly. This is the
//! same construction used by the v=1 path (different info string).
//!
//! ## Nonce choice for wrapped K
//!
//! Each wrapped-K entry prepends a fresh **random 12-byte nonce**
//! to the AES-GCM output. The alternative (deterministic nonce from
//! `H(message_id || recipient_hash)`) was considered and rejected
//! for v=2 because:
//!
//! - The sender does not always know `message_id` at encrypt time
//!   (Discord assigns it on POST response).
//! - A deterministic nonce introduces a footgun if a caller ever
//!   reuses the message_id (drafts, retransmits).
//! - 12 random bytes per recipient is cheap and avoids any
//!   coordination across recipients.
//!
//! The body nonce is also random per `encrypt_v2` call.
//!
//! ## AAD discipline
//!
//! Wrap-leg AAD = `b"OSL/P7/wrap/v2"`, body-leg AAD =
//! `b"OSL/P7/body/v2"`. Static domain separators; v=2 does not bind
//! to message_id (see "Nonce choice" above). Future formats (v=3+)
//! can bind to richer transcript material when the protocol
//! supports it.
//!
//! ## Phase 7a scope
//!
//! 7a implements encode/decode in isolation. The send/recv paths
//! still go through v=1 (`crate::commands::encrypt_osl_phase4_to_pubkeys`
//! / `decrypt_osl_phase4_cover`). Phase 7b wires v=2 through the
//! send/recv pipeline and adds the whitelist scope checks.
//!
//! ## Cipher choice
//!
//! AES-256-GCM (12-byte nonce, 16-byte tag) for both legs. See
//! [`crypto::aes_gcm`] for the rationale (matches NIST SP 800-38D
//! recommendations; trims four bytes per body vs. XChaCha20-Poly1305).

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use crypto::{aead, aes_gcm, hkdf, ml_kem_768, pqxdh, random, x25519};
use sha2::{Digest, Sha256};

/// Wire format version byte for v=2.
pub const WIRE_VERSION_V2: u8 = 0x02;

/// Phase 9-A1 wire format version byte for v=3 (PQ-hybrid wrap).
/// Per-recipient wrap key is derived via pqxdh::initiate
/// (X25519 + ML-KEM-768 → HKDF combiner) rather than raw X25519
/// ECDH. Body cipher (AES-256-GCM) is unchanged from v=2.
pub const WIRE_VERSION_V3: u8 = 0x03;

/// Phase 9-A2 wire format version byte for v=4 (ratcheted single-
/// recipient). Wire shape mirrors v=3 but adds a fixed 84-byte
/// ratchet header (header_nonce(24) || enc_header(60)) between the
/// global header and the slot, and a 1-byte `flags` field inside
/// the global header. N is fixed at 1; multi-recipient sends stay
/// on v=3. See `encrypt_v4` / `decrypt_v4` for the full layout.
pub const WIRE_VERSION_V4: u8 = 0x04;

/// Phase 9-A3 wire format version byte for v=5 (sender-keys
/// multi-recipient). No per-recipient slot — the scope_storage_key
/// in the caller's context tells the recipient which SenderKeyState
/// to consult, and `sender_ik_x25519_pub` in the global header picks
/// the matching ReceiverChain inside that state. Bootstrap is
/// out-of-band via v=4-wrapped SKDMs (`MSG_TYPE_SENDER_KEY_DISTRIBUTION`).
pub const WIRE_VERSION_V5: u8 = 0x05;

/// Message-type byte: ordinary content (post-decrypt rendered as
/// the user-visible message body).
pub const MSG_TYPE_CONTENT: u8 = 0x00;

/// Message-type byte: burn marker. Carries no body content; tells
/// the recipient to wipe its decryption capability for the named
/// scope. Phase 7a accepts the type byte on the wire but does not
/// yet act on it; Phase 7b adds the side-effect handling.
pub const MSG_TYPE_BURN: u8 = 0x01;

// 9-C1: `MSG_TYPE_WHITELIST_INVITATION` (0x02) and
// `MSG_TYPE_WHITELIST_RESPONSE` (0x03) removed alongside the
// invitation handshake. The dispatcher matches literal `0x02 | 0x03`
// in the legacy-ignored arm.

/// Phase 8 message-type byte: attachment envelope. Body is a CBOR-
/// encoded [`AttachmentEnvelope`] carrying the per-attachment AEAD
/// key + the random/original filenames + the MIME type. The actual
/// encrypted attachment lives in Discord's CDN at the URL Discord
/// reports for `attachments[N].url`; the recv side fetches that
/// file, scans for the OSL-ATT1 magic, and decrypts with `att_key`.
pub const MSG_TYPE_ATTACHMENT: u8 = 0x04;

/// Phase 9-A3 message-type byte: sender-keys distribution. Body is a
/// CBOR-encoded [`crate::control_messages::SenderKeyDistribution`]
/// carrying `(scope_storage_key, chain_id, rotation_root)`.
///
/// Probe-3 Option-2 step 1: transport switched from v=4 (one
/// ratcheted message per group member, fragile when any pair's DR
/// desynced) to **v=3 bundled** (one PQ-hybrid multi-recipient
/// message addressed to every member at once). Receiver-side
/// dispatch lives in BOTH `decrypt_v4_recv` (legacy peers still
/// emitting v=4) and the v=2/v=3 `match recovered.msg_type` arm in
/// `cmd_osl_decrypt_message_v2` (the new bundled transport). The
/// SKDM handler `apply_skdm_recv` is shared.
pub const MSG_TYPE_SENDER_KEY_DISTRIBUTION: u8 = 0x05;

/// Auto-recovery message-type byte: "I am awaiting your sender-key
/// for this scope and it never arrived — please re-emit it." Body is
/// a CBOR-encoded [`crate::control_messages::SkdmRequest`]. Ships as a
/// v=2 message (NOT v=4): the requester may have no working v=4
/// ratchet to the sender yet, so the request must use the ratchet-
/// independent PQ-hybrid transport. Still bound to the sender's
/// identity keys, so a network attacker cannot forge one. The sender
/// routes this to a one-shot forced SKDM re-emit for the named scope.
pub const MSG_TYPE_SKDM_REQUEST: u8 = 0x06;

/// Auto-recovery message-type byte: "our v=4 Double Ratchet is
/// desynced — I have reset my side; reset yours so we re-handshake."
/// Body is a CBOR-encoded [`crate::control_messages::SessionReset`].
/// Ships as a v=2 message for the same reason as `MSG_TYPE_SKDM_REQUEST`
/// (the v=4 ratchet is precisely what is broken). Peer-scoped: the
/// peer identity is the v=2 sender; resets that peer's whole v=4 DM
/// ratchet. Honored only under the act-on-symptom + rate-limit guards
/// in the recv handler (see `decrypt_v2_recv` SESSION_RESET arm).
pub const MSG_TYPE_SESSION_RESET: u8 = 0x07;

/// Length of the per-recipient pubkey-hash prefix on the wire
/// (8 bytes = leading bytes of SHA-256(recipient_pubkey)). 8 bytes
/// = 1/2^64 ≈ 5.4e-20 collision probability for the small N this
/// format supports (max 255 recipients per message).
pub const RECIPIENT_HASH_PREFIX_LEN: usize = 8;

/// Length on the wire of a single wrapped-K payload:
/// `nonce(12) || GCM(K=32) || tag(16)` = 60 bytes.
pub const WRAPPED_K_BYTES: usize = aes_gcm::NONCE_SIZE + aes_gcm::KEY_SIZE + aes_gcm::TAG_SIZE;

/// Length on the wire of a single per-recipient slot header:
/// `pubkey_hash(8) || wrapped_len(2) || wrapped(60)` = 70 bytes.
pub const SLOT_BYTES: usize = RECIPIENT_HASH_PREFIX_LEN + 2 + WRAPPED_K_BYTES;

/// Domain-separator AAD for the per-recipient wrap leg.
pub const AD_WRAP_V2: &[u8] = b"OSL/P7/wrap/v2";

/// Domain-separator AAD for the bulk body leg.
pub const AD_BODY_V2: &[u8] = b"OSL/P7/body/v2";

/// HKDF info string for deriving the per-recipient wrap key from
/// the static-static X25519 shared secret.
pub const HKDF_INFO_WRAP_V2: &[u8] = b"OSL/P7/wrap-key/v2";

/// Phase 9-A1 v=3 AADs + HKDF info. Distinct from v=2 so a wrong-
/// version body cipher attempt fails its tag check rather than
/// silently producing garbage.
pub const AD_WRAP_V3: &[u8] = b"OSL/A1/wrap/v3";
pub const AD_BODY_V3: &[u8] = b"OSL/A1/body/v3";
pub const HKDF_INFO_WRAP_V3: &[u8] = b"OSL/A1/wrap-key/v3";

/// Phase 9-A2 v=4 AADs + HKDF info.
/// `AD_HEADER_V4` is a new label not present in v=3 — it binds the
/// 37-byte global header bytes to the wrap AEAD so any tamper on
/// the global header invalidates every slot.
pub const AD_WRAP_V4: &[u8] = b"OSL/A1/wrap/v4";
pub const AD_BODY_V4: &[u8] = b"OSL/A1/body/v4";
pub const AD_HEADER_V4: &[u8] = b"OSL/A1/header/v4";
pub const HKDF_INFO_WRAP_V4: &[u8] = b"OSL/A1/wrap-key/v4";

/// Phase 9-A3 v=5 AAD label. No wrap leg in v=5 (sender-keys
/// inherit identity binding from the SKDM that installed the chain),
/// so only the body AAD label is defined here.
pub const AD_BODY_V5: &[u8] = b"OSL/A3/body/v5";

/// v=5 global header: version(1) + msg_type(1) + sender_ik_x25519_pub(32)
/// + flags(1) = 35 bytes. Same shape as v=3's global header (`N` is
/// implicit at 1-sender-many-recipients).
pub const V5_GLOBAL_HEADER_BYTES: usize = 1 + 1 + 32 + 1;

/// v=5 reserved-bits mask. All bits in the flags byte are reserved
/// in this phase; non-zero rejects.
pub const V5_FLAG_RESERVED_MASK: u8 = 0xFF;

/// v=5 sender-keys header on the wire:
/// `header_nonce(24) || enc_header(16-byte Header + 16-byte AEAD tag = 32)`.
/// Total 56 bytes.
pub const V5_SENDER_KEYS_HEADER_BYTES: usize = 24 + (16 + 16);

/// v=4 global header size: version(1) + msg_type(1) + flags(1) +
/// sender_ik_x25519_pub(32) + N(1) = 36 bytes.
pub const V4_GLOBAL_HEADER_BYTES: usize = 1 + 1 + 1 + 32 + 1;

/// v=4 fixed flags byte. Bit 0 = bootstrap; bits 1..7 = reserved
/// (decode rejects any non-zero reserved bit).
pub const V4_FLAG_BOOTSTRAP: u8 = 0x01;
pub const V4_FLAG_RESERVED_MASK: u8 = 0xFE;

/// v=4 ratchet header on the wire:
/// `header_nonce(24) || enc_header(serialized 44-byte plain header
/// + 16-byte AEAD tag = 60 bytes)`. Total 84 bytes.
pub const V4_RATCHET_NONCE_BYTES: usize = 24;
pub const V4_RATCHET_ENC_HEADER_BYTES: usize = 44 + 16;
pub const V4_RATCHET_HEADER_BYTES: usize = V4_RATCHET_NONCE_BYTES + V4_RATCHET_ENC_HEADER_BYTES;

/// v=3 per-recipient slot layout:
///   [8  bytes : recipient pubkey hash prefix]
///   [32 bytes : sender ephemeral X25519 pub (InitiatorHandshake.ek_x25519_pub)]
///   [2  bytes LE : mlkem_ct_len = 1088]
///   [1088 bytes : ML-KEM-768 ciphertext]
///   [12 bytes : wrap_nonce]
///   [48 bytes : AES-GCM(wrap_key, body_K=32) = 32B ct + 16B tag]
/// Total = 8 + 32 + 2 + 1088 + 12 + 48 = 1190 bytes per recipient.
/// 10-recipient message header overhead ≈ 12 KB.
pub const SLOT_V3_BYTES: usize =
    RECIPIENT_HASH_PREFIX_LEN + 32 + 2 + ml_kem_768::CIPHERTEXT_SIZE + WRAPPED_K_BYTES;

/// Plaintext + metadata recovered from a v=2 wire blob.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecryptedV2 {
    /// Message-type byte (`MSG_TYPE_*`). Phase 7a callers branch on
    /// this; Phase 7b adds dispatch into the control-message
    /// handlers for non-content types.
    pub msg_type: u8,

    /// Recovered plaintext bytes (body of the v=2 message). UTF-8
    /// for content messages by convention; control messages carry
    /// a JSON envelope. Caller decides the interpretation.
    pub plaintext: Vec<u8>,
}

/// Error returned by the v=2 decoder.
///
/// Variants are deliberately distinct from the v=1 `DecodeError`
/// in `crate::commands` so callers (and the version-router) can
/// reason about them separately.
#[derive(Debug, thiserror::Error)]
pub enum V2Error {
    /// The cover string did not carry the `DPC0::` magic prefix —
    /// not an OSL message at all.
    #[error("cover string missing DPC0:: prefix")]
    BadPrefix,

    /// Base64 decode of the cover body failed (truncation or
    /// hand-editing).
    #[error("base64 decode of cover body failed: {0}")]
    Base64(String),

    /// Wire bytes shorter than the minimum framing requires.
    #[error("wire too short: {got} bytes, expected at least {expected}")]
    TooShort { got: usize, expected: usize },

    /// Version byte did not match v=2. The caller (a version
    /// router) should re-route to the v=1 decoder.
    #[error("wire version 0x{got:02x} != 0x{expected:02x} (v=2 decoder rejects)")]
    WrongVersion { got: u8, expected: u8 },

    /// Recipient-count byte was zero. A well-formed v=2 encoder
    /// never produces this.
    #[error("recipient count is zero in wire header")]
    ZeroRecipients,

    /// Per-recipient wrapped-K declared length did not match the
    /// expected 60 bytes. Wire is corrupt or written by an encoder
    /// we don't know about.
    #[error("wrapped-K length {got} for slot {slot} != expected {expected}")]
    BadWrappedLen {
        slot: usize,
        got: u16,
        expected: u16,
    },

    /// We are not a recipient of this message — no slot's pubkey
    /// hash matched our own derived hash. Shared verbatim by the
    /// v=2 / v=3 / v=4 decoders, so the wording must stay
    /// version-agnostic (the caller prefixes the actual version,
    /// e.g. "OSL: v=4 decode: <this>").
    #[error("not a recipient of this message")]
    NoMatchingSlot,

    /// A slot matched our hash but the wrap-leg AEAD failed under
    /// the derived key. Indicates ciphertext tampering or a hash
    /// collision (1/2^64) coincidentally selecting a slot that
    /// isn't actually ours.
    #[error("wrap leg AEAD failed for matching slot")]
    WrapAeadFailed,

    /// Body-leg AEAD failed under the unwrapped K. The wrap leg
    /// authenticated successfully, so the wire's K is intact;
    /// failure here points at body tampering.
    #[error("body leg AEAD failed under recovered K")]
    BodyAeadFailed,

    /// Inner primitive (X25519 / HKDF) returned an error not
    /// otherwise classified.
    #[error("crypto primitive error: {0}")]
    Crypto(String),
}

/// Encrypt `plaintext` for `recipients` under wire format v=2.
///
/// Behaviour:
/// 1. Generate a fresh AES-256-GCM key `K` from `OsRng`.
/// 2. Encrypt `plaintext` under `K` with a fresh 12-byte body nonce
///    and AAD = [`AD_BODY_V2`].
/// 3. For each recipient: derive `wrap_key = HKDF(X25519(sender_sk,
///    recipient_pk), info=HKDF_INFO_WRAP_V2)` and AES-GCM-seal `K`
///    under `wrap_key` with a fresh 12-byte wrap nonce and AAD =
///    [`AD_WRAP_V2`]. Slot prefix = first 8 bytes of
///    SHA-256(recipient_pk).
/// 4. Concatenate version || type || N || slots || body_nonce ||
///    body_ct, base64-encode, prepend `DPC0::`.
///
/// `msg_type` may be any value in `0x00..=0xFF`. The decoder
/// surfaces it verbatim via [`DecryptedV2::msg_type`].
///
/// Errors: any inner primitive failure, recipient count > 255 or
/// 0, or the produced wire blob exceeds the base64-encoded length
/// the caller's transport can accept. (This module does not check
/// transport caps; that's the caller's job — `stego::MODE0_MAX_RAW_LEN`
/// remains the v=1 cap; v=2 has no analogous published cap in
/// 7a since the send-path doesn't use v=2 yet.)
pub fn encrypt_v2(
    plaintext: &[u8],
    recipients: &[x25519::PublicKey],
    msg_type: u8,
    sender_privkey: &x25519::SecretKey,
) -> Result<String, V2Error> {
    if recipients.is_empty() {
        return Err(V2Error::ZeroRecipients);
    }
    if recipients.len() > 255 {
        return Err(V2Error::Crypto(format!(
            "recipient count {} exceeds wire-format max of 255",
            recipients.len()
        )));
    }

    // Generate the per-message AES-256-GCM key K.
    let k_bytes = crypto::random::random_bytes(aes_gcm::KEY_SIZE);
    let mut k_arr = [0u8; aes_gcm::KEY_SIZE];
    k_arr.copy_from_slice(&k_bytes);
    let k = aes_gcm::Key::from_bytes(k_arr);

    // Body leg: encrypt plaintext under K with fresh nonce.
    let (body_nonce, body_ct) = aes_gcm::seal(&k, AD_BODY_V2, plaintext)
        .map_err(|e| V2Error::Crypto(format!("body seal: {e}")))?;

    // Wrap leg per recipient.
    let n = recipients.len() as u8;
    let mut wire: Vec<u8> =
        Vec::with_capacity(2 + 1 + (n as usize) * SLOT_BYTES + aes_gcm::NONCE_SIZE + body_ct.len());
    wire.push(WIRE_VERSION_V2);
    wire.push(msg_type);
    wire.push(n);

    for (ix, recipient_pk) in recipients.iter().enumerate() {
        let shared = x25519::diffie_hellman(sender_privkey, recipient_pk)
            .map_err(|e| V2Error::Crypto(format!("slot {ix} ECDH: {e}")))?;
        let wrap_key_bytes = hkdf::derive_32(&[], shared.as_bytes(), HKDF_INFO_WRAP_V2)
            .map_err(|e| V2Error::Crypto(format!("slot {ix} HKDF: {e}")))?;
        let wrap_key = aes_gcm::Key::from_bytes(wrap_key_bytes);
        let (wrap_nonce, wrap_ct) = aes_gcm::seal(&wrap_key, AD_WRAP_V2, k.as_bytes())
            .map_err(|e| V2Error::Crypto(format!("slot {ix} wrap seal: {e}")))?;
        if wrap_ct.len() != aes_gcm::KEY_SIZE + aes_gcm::TAG_SIZE {
            return Err(V2Error::Crypto(format!(
                "slot {ix} wrap ciphertext length {} != {}",
                wrap_ct.len(),
                aes_gcm::KEY_SIZE + aes_gcm::TAG_SIZE
            )));
        }

        let hash = pubkey_hash_prefix(recipient_pk);
        wire.extend_from_slice(&hash);

        // wrapped = nonce || ct; length = 12 + 32 + 16 = 60
        let wrapped_len = WRAPPED_K_BYTES as u16;
        wire.extend_from_slice(&wrapped_len.to_le_bytes());
        wire.extend_from_slice(wrap_nonce.as_bytes());
        wire.extend_from_slice(&wrap_ct);
    }

    wire.extend_from_slice(body_nonce.as_bytes());
    wire.extend_from_slice(&body_ct);

    let b64 = STANDARD.encode(&wire);
    let mut out = String::with_capacity(b64.len() + "DPC0::".len());
    out.push_str("DPC0::");
    out.push_str(&b64);
    Ok(out)
}

/// Decrypt a v=2 wire blob.
///
/// Behaviour:
/// 1. Strip `DPC0::` prefix, base64-decode body.
/// 2. Read `version` — error with [`V2Error::WrongVersion`] if not
///    0x02. A version-routing caller maps this to a v=1 retry.
/// 3. Read `type`, recipient count.
/// 4. Compute our pubkey hash (first 8 bytes of SHA-256(our pub)).
///    Walk slots; on hash match, derive `wrap_key = HKDF(X25519(
///    our_sk, sender_pk), info)` and AES-GCM-unwrap to recover K.
///    Continue past hash-collision misses (1/2^64) until a wrap
///    succeeds.
/// 5. Read body nonce + ciphertext+tag; AES-GCM-unseal under K.
/// 6. Return `(msg_type, plaintext)`.
///
/// The decoder is constant-time-ish across slots whose hash
/// matches ours: it doesn't break on first wrap-leg success but
/// continues iterating to avoid leaking which slot is ours when
/// multiple hash-collide. (Cost: one extra AEAD attempt per
/// collision; collisions are vanishingly rare at 8-byte prefix
/// width.) Slots whose hash doesn't match are skipped — `hash` is
/// public (sender writes it in the clear), so iterating non-matches
/// is wasted work, not a leak.
pub fn decrypt_v2(
    wire: &str,
    our_privkey: &x25519::SecretKey,
    sender_pubkey: &x25519::PublicKey,
) -> Result<DecryptedV2, V2Error> {
    let body = wire.strip_prefix("DPC0::").ok_or(V2Error::BadPrefix)?;
    let raw = STANDARD
        .decode(body)
        .map_err(|e| V2Error::Base64(e.to_string()))?;

    // version + type + N + at least one slot + body nonce + tag
    let min_framing = 3 + SLOT_BYTES + aes_gcm::NONCE_SIZE + aes_gcm::TAG_SIZE;
    if raw.len() < min_framing {
        return Err(V2Error::TooShort {
            got: raw.len(),
            expected: min_framing,
        });
    }

    let version = raw[0];
    if version != WIRE_VERSION_V2 {
        return Err(V2Error::WrongVersion {
            got: version,
            expected: WIRE_VERSION_V2,
        });
    }
    let msg_type = raw[1];
    let n = raw[2] as usize;
    if n == 0 {
        return Err(V2Error::ZeroRecipients);
    }
    let slots_end = 3 + n * SLOT_BYTES;
    if raw.len() < slots_end + aes_gcm::NONCE_SIZE + aes_gcm::TAG_SIZE {
        return Err(V2Error::TooShort {
            got: raw.len(),
            expected: slots_end + aes_gcm::NONCE_SIZE + aes_gcm::TAG_SIZE,
        });
    }

    // Compute our pubkey hash and the per-sender wrap key (which
    // is the same across all of our slots in this message).
    let our_pub = x25519::derive_public(our_privkey);
    let our_hash = pubkey_hash_prefix(&our_pub);
    let shared = x25519::diffie_hellman(our_privkey, sender_pubkey)
        .map_err(|e| V2Error::Crypto(format!("ECDH: {e}")))?;
    let wrap_key_bytes = hkdf::derive_32(&[], shared.as_bytes(), HKDF_INFO_WRAP_V2)
        .map_err(|e| V2Error::Crypto(format!("HKDF: {e}")))?;
    let wrap_key = aes_gcm::Key::from_bytes(wrap_key_bytes);

    // Walk slots; on hash match, attempt wrap unseal. Don't break
    // on first success — see fn-doc.
    let mut recovered_k: Option<aes_gcm::Key> = None;
    for slot_ix in 0..n {
        let base = 3 + slot_ix * SLOT_BYTES;
        let hash = &raw[base..base + RECIPIENT_HASH_PREFIX_LEN];
        if hash != our_hash {
            continue;
        }
        let len_off = base + RECIPIENT_HASH_PREFIX_LEN;
        let wrapped_len = u16::from_le_bytes([raw[len_off], raw[len_off + 1]]);
        if wrapped_len as usize != WRAPPED_K_BYTES {
            return Err(V2Error::BadWrappedLen {
                slot: slot_ix,
                got: wrapped_len,
                expected: WRAPPED_K_BYTES as u16,
            });
        }
        let nonce_off = len_off + 2;
        let nonce_end = nonce_off + aes_gcm::NONCE_SIZE;
        let ct_end = nonce_end + aes_gcm::KEY_SIZE + aes_gcm::TAG_SIZE;
        let mut nonce_bytes = [0u8; aes_gcm::NONCE_SIZE];
        nonce_bytes.copy_from_slice(&raw[nonce_off..nonce_end]);
        let nonce = aes_gcm::Nonce::from_bytes(nonce_bytes);
        let wrap_ct = &raw[nonce_end..ct_end];

        if let Ok(k_bytes) = aes_gcm::open(&wrap_key, &nonce, AD_WRAP_V2, wrap_ct) {
            if k_bytes.len() == aes_gcm::KEY_SIZE && recovered_k.is_none() {
                let mut k_arr = [0u8; aes_gcm::KEY_SIZE];
                k_arr.copy_from_slice(&k_bytes);
                recovered_k = Some(aes_gcm::Key::from_bytes(k_arr));
            }
            // Length mismatch on a "successful" open is
            // pathological; treat as not-our-slot and keep going.
        }
        // Failed wrap unseal on a matching hash means a 1/2^64
        // collision: this slot isn't actually ours. Keep going.
    }
    let k = recovered_k.ok_or(V2Error::NoMatchingSlot)?;

    // Body leg.
    let body_nonce_off = slots_end;
    let body_nonce_end = body_nonce_off + aes_gcm::NONCE_SIZE;
    let mut body_nonce_bytes = [0u8; aes_gcm::NONCE_SIZE];
    body_nonce_bytes.copy_from_slice(&raw[body_nonce_off..body_nonce_end]);
    let body_nonce = aes_gcm::Nonce::from_bytes(body_nonce_bytes);
    let body_ct = &raw[body_nonce_end..];

    let plaintext =
        aes_gcm::open(&k, &body_nonce, AD_BODY_V2, body_ct).map_err(|_| V2Error::BodyAeadFailed)?;

    Ok(DecryptedV2 {
        msg_type,
        plaintext,
    })
}

/// First 8 bytes of SHA-256(`pk` bytes) — the per-recipient slot
/// prefix used to find our wrap entry in a multi-recipient v=2
/// blob. Public information (sender writes it in the clear); leaks
/// no secret data. `pub` so the v=4 send/receive diagnostic logs
/// can print the exact value the wire carries / the slot scan
/// compares (send-vs-receive stale-key triage).
pub fn pubkey_hash_prefix(pk: &x25519::PublicKey) -> [u8; RECIPIENT_HASH_PREFIX_LEN] {
    let digest = Sha256::digest(pk.as_bytes());
    let mut out = [0u8; RECIPIENT_HASH_PREFIX_LEN];
    out.copy_from_slice(&digest[..RECIPIENT_HASH_PREFIX_LEN]);
    out
}

// ============================================================
// Phase 9-A1: v=3 PQ-hybrid wire format
// ============================================================

/// Phase 9-A1: recipient bundle for v=3. Carries both the X25519
/// ik pubkey (needed for slot hash + pqxdh DH legs) and the ML-KEM
/// encap key (needed for the PQ leg). The send-side capability
/// check (whitelist::recipients_for_scope_v3) returns these for
/// every member of the scope whitelist; if any member lacks an
/// ML-KEM pubkey the check errors out (no v=2 fallback in
/// production per locked policy).
#[derive(Clone)]
pub struct RecipientV3 {
    pub x25519_pub: x25519::PublicKey,
    pub mlkem_pub: ml_kem_768::EncapsulationKey,
}

/// Phase 9-A1: encode a v=3 wire blob.
///
/// Global header:
///   [version=0x03 | msg_type | sender_ik_x25519_pub(32) | N]
/// Then N × [SLOT_V3_BYTES] (see SLOT_V3_BYTES doc for layout).
/// Trailer: [body_nonce(12) | body_ct + 16B tag].
///
/// Per recipient: pqxdh::initiate(sender_ik_sk, recip_ik /*as ik+spk*/,
/// None /*opk*/, recip_mlkem) → (SessionKey, InitiatorHandshake).
/// wrap_key = HKDF(SessionKey, info=HKDF_INFO_WRAP_V3). The fresh
/// 32-byte body_K is sealed under wrap_key per recipient; body
/// itself sealed under body_K once.
pub fn encrypt_v3(
    sender_ik_sk: &x25519::SecretKey,
    sender_ik_pub: &x25519::PublicKey,
    recipients: &[RecipientV3],
    msg_type: u8,
    plaintext: &[u8],
) -> Result<String, V2Error> {
    if recipients.is_empty() {
        return Err(V2Error::ZeroRecipients);
    }
    if recipients.len() > 255 {
        return Err(V2Error::Crypto(format!(
            "v=3 recipient count {} exceeds u8 max 255",
            recipients.len()
        )));
    }

    // Fresh body key K. Sealed once; per-recipient wrap covers the
    // same K under each recipient's wrap_key.
    let k_bytes = random::random_bytes(aes_gcm::KEY_SIZE);
    let mut k_arr = [0u8; aes_gcm::KEY_SIZE];
    k_arr.copy_from_slice(&k_bytes);
    let k = aes_gcm::Key::from_bytes(k_arr);

    let (body_nonce, body_ct) = aes_gcm::seal(&k, AD_BODY_V3, plaintext)
        .map_err(|e| V2Error::Crypto(format!("v3 body seal: {e}")))?;

    let n = recipients.len() as u8;
    let mut wire: Vec<u8> = Vec::with_capacity(
        // version + msg_type + sender_ik(32) + N + slots + body
        4 + 32 + (n as usize) * SLOT_V3_BYTES + aes_gcm::NONCE_SIZE + body_ct.len(),
    );
    wire.push(WIRE_VERSION_V3);
    wire.push(msg_type);
    wire.extend_from_slice(sender_ik_pub.as_bytes());
    wire.push(n);

    for (ix, recip) in recipients.iter().enumerate() {
        // pqxdh::initiate: recipient_ik plays both ik and spk
        // roles in OSL's flow (we don't run a separate signed-
        // prekey publication infrastructure). OPK is always None.
        let (session_key, handshake) = pqxdh::initiate(
            sender_ik_sk,
            &recip.x25519_pub,
            &recip.x25519_pub,
            None,
            &recip.mlkem_pub,
        )
        .map_err(|e| V2Error::Crypto(format!("slot {ix} pqxdh::initiate: {e}")))?;

        let wrap_key_bytes = hkdf::derive_32(&[], session_key.as_bytes(), HKDF_INFO_WRAP_V3)
            .map_err(|e| V2Error::Crypto(format!("slot {ix} wrap HKDF: {e}")))?;
        let wrap_key = aes_gcm::Key::from_bytes(wrap_key_bytes);
        let (wrap_nonce, wrap_ct) = aes_gcm::seal(&wrap_key, AD_WRAP_V3, k.as_bytes())
            .map_err(|e| V2Error::Crypto(format!("slot {ix} wrap seal: {e}")))?;
        if wrap_ct.len() != aes_gcm::KEY_SIZE + aes_gcm::TAG_SIZE {
            return Err(V2Error::Crypto(format!(
                "slot {ix} wrap ct length {} != {}",
                wrap_ct.len(),
                aes_gcm::KEY_SIZE + aes_gcm::TAG_SIZE
            )));
        }

        let hash = pubkey_hash_prefix(&recip.x25519_pub);
        wire.extend_from_slice(&hash);
        wire.extend_from_slice(handshake.ek_x25519_pub.as_bytes());
        let ct_bytes = handshake.mlkem_ciphertext.to_bytes();
        let ct_len = ct_bytes.len() as u16;
        wire.extend_from_slice(&ct_len.to_le_bytes());
        wire.extend_from_slice(&ct_bytes);
        wire.extend_from_slice(wrap_nonce.as_bytes());
        wire.extend_from_slice(&wrap_ct);
    }

    wire.extend_from_slice(body_nonce.as_bytes());
    wire.extend_from_slice(&body_ct);

    let b64 = STANDARD.encode(&wire);
    let mut out = String::with_capacity(b64.len() + "DPC0::".len());
    out.push_str("DPC0::");
    out.push_str(&b64);
    Ok(out)
}

/// Phase 9-A1: decode a v=3 wire blob.
///
/// Reverses encrypt_v3. Receiver finds its slot via pubkey-hash
/// prefix, reconstructs the InitiatorHandshake from slot fields,
/// calls pqxdh::respond to recover the same SessionKey, derives
/// wrap_key via HKDF, unwraps body_K, decrypts body under body_K.
///
/// Returns DecryptedV2 (msg_type + plaintext) so callers can share
/// the v=2 result type — the recovered shape is identical.
pub fn decrypt_v3(
    wire: &str,
    recipient_ik_sk: &x25519::SecretKey,
    recipient_mlkem_sk: &ml_kem_768::DecapsulationKey,
) -> Result<DecryptedV2, V2Error> {
    let body = wire.strip_prefix("DPC0::").ok_or(V2Error::BadPrefix)?;
    let raw = STANDARD
        .decode(body)
        .map_err(|e| V2Error::Base64(e.to_string()))?;

    // version + msg_type + sender_ik(32) + N + ≥1 slot + body_nonce + tag
    let min_framing = 4 + 32 + SLOT_V3_BYTES + aes_gcm::NONCE_SIZE + aes_gcm::TAG_SIZE;
    if raw.len() < min_framing {
        return Err(V2Error::TooShort {
            got: raw.len(),
            expected: min_framing,
        });
    }
    if raw[0] != WIRE_VERSION_V3 {
        return Err(V2Error::WrongVersion {
            got: raw[0],
            expected: WIRE_VERSION_V3,
        });
    }
    let msg_type = raw[1];
    let mut sender_ik_bytes = [0u8; 32];
    sender_ik_bytes.copy_from_slice(&raw[2..34]);
    let sender_ik_pub = x25519::PublicKey::from_bytes(sender_ik_bytes);
    let n = raw[34] as usize;
    if n == 0 {
        return Err(V2Error::ZeroRecipients);
    }
    let slots_start = 35;
    let slots_end = slots_start + n * SLOT_V3_BYTES;
    if raw.len() < slots_end + aes_gcm::NONCE_SIZE + aes_gcm::TAG_SIZE {
        return Err(V2Error::TooShort {
            got: raw.len(),
            expected: slots_end + aes_gcm::NONCE_SIZE + aes_gcm::TAG_SIZE,
        });
    }

    let our_pub = x25519::derive_public(recipient_ik_sk);
    let our_hash = pubkey_hash_prefix(&our_pub);

    let mut recovered_k: Option<aes_gcm::Key> = None;
    for slot_ix in 0..n {
        let base = slots_start + slot_ix * SLOT_V3_BYTES;
        let hash = &raw[base..base + RECIPIENT_HASH_PREFIX_LEN];
        if hash != our_hash {
            continue;
        }
        let mut p = base + RECIPIENT_HASH_PREFIX_LEN;
        let mut ek_bytes = [0u8; 32];
        ek_bytes.copy_from_slice(&raw[p..p + 32]);
        let ek_x25519_pub = x25519::PublicKey::from_bytes(ek_bytes);
        p += 32;
        let ct_len = u16::from_le_bytes([raw[p], raw[p + 1]]) as usize;
        p += 2;
        if ct_len != ml_kem_768::CIPHERTEXT_SIZE {
            return Err(V2Error::Crypto(format!(
                "slot {slot_ix} mlkem_ct_len {ct_len} != {}",
                ml_kem_768::CIPHERTEXT_SIZE
            )));
        }
        let mut ct_bytes = [0u8; ml_kem_768::CIPHERTEXT_SIZE];
        ct_bytes.copy_from_slice(&raw[p..p + ct_len]);
        let mlkem_ciphertext = ml_kem_768::Ciphertext::from_bytes(&ct_bytes);
        p += ct_len;
        let mut wrap_nonce_bytes = [0u8; aes_gcm::NONCE_SIZE];
        wrap_nonce_bytes.copy_from_slice(&raw[p..p + aes_gcm::NONCE_SIZE]);
        let wrap_nonce = aes_gcm::Nonce::from_bytes(wrap_nonce_bytes);
        p += aes_gcm::NONCE_SIZE;
        let wrap_ct = &raw[p..p + aes_gcm::KEY_SIZE + aes_gcm::TAG_SIZE];

        let handshake = pqxdh::InitiatorHandshake {
            ek_x25519_pub,
            mlkem_ciphertext,
            no_opk: true,
            opk_id: None,
        };
        let session_key = pqxdh::respond(
            recipient_ik_sk,
            recipient_ik_sk,
            None,
            recipient_mlkem_sk,
            &sender_ik_pub,
            &handshake,
        )
        .map_err(|e| V2Error::Crypto(format!("slot {slot_ix} pqxdh::respond: {e}")))?;
        let wrap_key_bytes = hkdf::derive_32(&[], session_key.as_bytes(), HKDF_INFO_WRAP_V3)
            .map_err(|e| V2Error::Crypto(format!("slot {slot_ix} wrap HKDF: {e}")))?;
        let wrap_key = aes_gcm::Key::from_bytes(wrap_key_bytes);
        if let Ok(k_bytes) = aes_gcm::open(&wrap_key, &wrap_nonce, AD_WRAP_V3, wrap_ct) {
            if k_bytes.len() == aes_gcm::KEY_SIZE {
                let mut k_arr = [0u8; aes_gcm::KEY_SIZE];
                k_arr.copy_from_slice(&k_bytes);
                recovered_k = Some(aes_gcm::Key::from_bytes(k_arr));
                // Note: matches v=2's "don't break early on hash
                // collision" comment — but here pqxdh::respond is
                // expensive (ML-KEM decap), so we DO break on the
                // first successful unwrap. Hash collisions at 1/2^64
                // are not worth the ML-KEM round on every slot.
                break;
            }
        }
    }

    let k = recovered_k.ok_or(V2Error::NoMatchingSlot)?;

    let mut body_nonce_bytes = [0u8; aes_gcm::NONCE_SIZE];
    body_nonce_bytes.copy_from_slice(&raw[slots_end..slots_end + aes_gcm::NONCE_SIZE]);
    let body_nonce = aes_gcm::Nonce::from_bytes(body_nonce_bytes);
    let body_ct = &raw[slots_end + aes_gcm::NONCE_SIZE..];

    let plaintext = aes_gcm::open(&k, &body_nonce, AD_BODY_V3, body_ct)
        .map_err(|e| V2Error::Crypto(format!("v3 body open: {e}")))?;

    Ok(DecryptedV2 {
        msg_type,
        plaintext,
    })
}

// ============================================================
// Phase 9-A2: v=4 ratcheted single-recipient wire format
// ============================================================

/// Encode a v=4 wire blob.
///
/// Layout (bytes):
/// ```text
///   global header (36 bytes):
///     [ version(1)=0x04 | msg_type(1) | flags(1)
///       | sender_ik_x25519_pub(32) | N(1)=1 ]
///   ratchet header (84 bytes):
///     [ header_nonce(24) | enc_header(60) ]
///   slot (SLOT_V3_BYTES = 1190):
///     [ pubkey_hash(8) | ek_x25519(32) | mlkem_ct_len(2)=1088
///       | mlkem_ct(1088) | wrap_nonce(12) | wrap_ct(48) ]
///   trailer:
///     [ body_nonce(24) | body_ct + 16B tag ]
/// ```
///
/// The caller pre-runs `DoubleRatchet::encrypt` and passes the
/// resulting `EncryptedMessage` parts in: `enc_header_nonce`,
/// `enc_header` (the AEAD-sealed ratchet header), and `body_em` —
/// where `body_em.message_nonce` becomes the wire's `body_nonce` and
/// `body_em.ciphertext` becomes the wire's `body_ct`. The DR's body
/// AEAD already binds `canonical_AD || enc_header` as AAD so the
/// wire's role is to add identity-bound transport.
///
/// The PQXDH slot's wrap leg seals a 32-byte sentinel under
/// `wrap_key = HKDF(SessionKey, HKDF_INFO_WRAP_V4)` with AAD
/// `AD_WRAP_V4 || global_header`. The sentinel itself carries no
/// information — the auth tag is the carrier. A tamper that flips
/// the bootstrap flag bit (or rewrites any byte of the global
/// header) invalidates the wrap tag and the recipient rejects
/// before touching the DR.
pub fn encrypt_v4(
    sender_ik_pub: &x25519::PublicKey,
    recipient: &RecipientV3,
    session_key: &pqxdh::SessionKey,
    handshake: &pqxdh::InitiatorHandshake,
    msg_type: u8,
    flags: u8,
    enc_header_nonce: &aead::Nonce,
    enc_header: &[u8],
    body_nonce: &aead::Nonce,
    body_ct: &[u8],
) -> Result<String, V2Error> {
    if (flags & V4_FLAG_RESERVED_MASK) != 0 {
        return Err(V2Error::Crypto(format!(
            "v=4 encode: reserved flags bits set in 0x{flags:02x}"
        )));
    }
    if enc_header.len() != V4_RATCHET_ENC_HEADER_BYTES {
        return Err(V2Error::Crypto(format!(
            "v=4 encode: enc_header length {} != expected {}",
            enc_header.len(),
            V4_RATCHET_ENC_HEADER_BYTES
        )));
    }

    let n: u8 = 1;
    let mut global_header = Vec::with_capacity(V4_GLOBAL_HEADER_BYTES);
    global_header.push(WIRE_VERSION_V4);
    global_header.push(msg_type);
    global_header.push(flags);
    global_header.extend_from_slice(sender_ik_pub.as_bytes());
    global_header.push(n);
    debug_assert_eq!(global_header.len(), V4_GLOBAL_HEADER_BYTES);

    let wrap_key_bytes = hkdf::derive_32(&[], session_key.as_bytes(), HKDF_INFO_WRAP_V4)
        .map_err(|e| V2Error::Crypto(format!("v=4 wrap HKDF: {e}")))?;
    let wrap_key = aes_gcm::Key::from_bytes(wrap_key_bytes);
    let mut wrap_aad = Vec::with_capacity(AD_WRAP_V4.len() + V4_GLOBAL_HEADER_BYTES);
    wrap_aad.extend_from_slice(AD_WRAP_V4);
    wrap_aad.extend_from_slice(&global_header);
    let sentinel = [0u8; 32];
    let (wrap_nonce, wrap_ct) = aes_gcm::seal(&wrap_key, &wrap_aad, &sentinel)
        .map_err(|e| V2Error::Crypto(format!("v=4 wrap seal: {e}")))?;

    let mut wire = Vec::with_capacity(
        V4_GLOBAL_HEADER_BYTES
            + V4_RATCHET_HEADER_BYTES
            + SLOT_V3_BYTES
            + aead::NONCE_SIZE
            + body_ct.len(),
    );
    wire.extend_from_slice(&global_header);
    wire.extend_from_slice(enc_header_nonce.as_bytes());
    wire.extend_from_slice(enc_header);
    let hash = pubkey_hash_prefix(&recipient.x25519_pub);
    wire.extend_from_slice(&hash);
    wire.extend_from_slice(handshake.ek_x25519_pub.as_bytes());
    let mlkem_ct_bytes = handshake.mlkem_ciphertext.to_bytes();
    let mlkem_ct_len = mlkem_ct_bytes.len() as u16;
    wire.extend_from_slice(&mlkem_ct_len.to_le_bytes());
    wire.extend_from_slice(&mlkem_ct_bytes);
    wire.extend_from_slice(wrap_nonce.as_bytes());
    wire.extend_from_slice(&wrap_ct);
    wire.extend_from_slice(body_nonce.as_bytes());
    wire.extend_from_slice(body_ct);

    let b64 = STANDARD.encode(&wire);
    let mut out = String::with_capacity(b64.len() + "DPC0::".len());
    out.push_str("DPC0::");
    out.push_str(&b64);
    Ok(out)
}

/// Caller-side helper: takes the pre-computed PQXDH session_key +
/// handshake (so the wrap leg and DR bootstrap share ONE pqxdh run)
/// and packages the DR's `EncryptedMessage` onto the wire. Reduces
/// ceremony at the IPC dispatch site.
pub fn encrypt_v4_from_ratchet(
    sender_ik_pub: &x25519::PublicKey,
    recipient: &RecipientV3,
    session_key: &pqxdh::SessionKey,
    handshake: &pqxdh::InitiatorHandshake,
    msg_type: u8,
    bootstrap: bool,
    em: &crypto::ratchet::EncryptedMessage,
) -> Result<String, V2Error> {
    let flags = if bootstrap { V4_FLAG_BOOTSTRAP } else { 0 };
    encrypt_v4(
        sender_ik_pub,
        recipient,
        session_key,
        handshake,
        msg_type,
        flags,
        &em.header_nonce,
        &em.enc_header,
        &em.message_nonce,
        &em.ciphertext,
    )
}

/// Parsed v=4 wire bytes plus the recovered PQXDH `SessionKey` —
/// hand-off type for the IPC layer. The caller:
/// 1. Verifies the wrap-leg sentinel auth tag (`verify_sentinel` on
///    this type).
/// 2. Bootstraps OR loads the live `DoubleRatchet` per the
///    `bootstrap` flag, using `session_key` as the seed when
///    bootstrapping.
/// 3. Constructs a `ratchet::EncryptedMessage` from `enc_header_nonce`
///    + `enc_header` + `body_nonce` + `body_ct` and calls
///    `dr.decrypt(&em)` to recover the plaintext.
pub struct ParsedV4 {
    pub msg_type: u8,
    pub bootstrap: bool,
    pub sender_ik_pub: x25519::PublicKey,
    pub enc_header_nonce: aead::Nonce,
    pub enc_header: Vec<u8>,
    pub body_nonce: aead::Nonce,
    pub body_ct: Vec<u8>,
    /// PQXDH session key — used as the DR bootstrap seed on first
    /// message; discarded on continuation messages.
    pub session_key: crypto::pqxdh::SessionKey,
}

impl std::fmt::Debug for ParsedV4 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParsedV4")
            .field("msg_type", &format_args!("0x{:02x}", self.msg_type))
            .field("bootstrap", &self.bootstrap)
            .field("enc_header_len", &self.enc_header.len())
            .field("body_ct_len", &self.body_ct.len())
            .field("session_key", &"<redacted>")
            .finish()
    }
}

/// Phase 9-A2: parse a v=4 wire blob, run the PQXDH wrap leg, and
/// return [`ParsedV4`] for the caller to drive through its DR.
///
/// Rejects (with no DR / state mutation) for:
/// - wrong version byte
/// - N != 1
/// - reserved flags bits
/// - hash mismatch on the slot (`NoMatchingSlot`)
/// - wrap-leg AEAD failure (sentinel didn't unseal)
pub fn decrypt_v4(
    wire: &str,
    recipient_ik_sk: &x25519::SecretKey,
    recipient_mlkem_sk: &ml_kem_768::DecapsulationKey,
) -> Result<ParsedV4, V2Error> {
    let body = wire.strip_prefix("DPC0::").ok_or(V2Error::BadPrefix)?;
    let raw = STANDARD
        .decode(body)
        .map_err(|e| V2Error::Base64(e.to_string()))?;

    // Check version byte BEFORE length checks. A v=3 wire is
    // ~1264 bytes vs v=4's ~1350-byte minimum; failing on length
    // first would surface a misleading `TooShort` error for what is
    // really just the wrong wire version.
    if raw.is_empty() {
        return Err(V2Error::TooShort {
            got: 0,
            expected: 1,
        });
    }
    if raw[0] != WIRE_VERSION_V4 {
        return Err(V2Error::WrongVersion {
            got: raw[0],
            expected: WIRE_VERSION_V4,
        });
    }
    let min_framing = V4_GLOBAL_HEADER_BYTES
        + V4_RATCHET_HEADER_BYTES
        + SLOT_V3_BYTES
        + aead::NONCE_SIZE
        + aes_gcm::TAG_SIZE;
    if raw.len() < min_framing {
        return Err(V2Error::TooShort {
            got: raw.len(),
            expected: min_framing,
        });
    }
    let msg_type = raw[1];
    let flags = raw[2];
    if (flags & V4_FLAG_RESERVED_MASK) != 0 {
        return Err(V2Error::Crypto(format!(
            "v=4 decode: reserved flags bits set in 0x{flags:02x}"
        )));
    }
    let bootstrap = (flags & V4_FLAG_BOOTSTRAP) != 0;
    let mut sender_pub_bytes = [0u8; 32];
    sender_pub_bytes.copy_from_slice(&raw[3..35]);
    let sender_ik_pub = x25519::PublicKey::from_bytes(sender_pub_bytes);
    let n = raw[35] as usize;
    if n != 1 {
        return Err(V2Error::Crypto(format!(
            "v=4 decode: N must be 1 (got {n}) — multi-recipient sends route through v=3"
        )));
    }
    let global_header_bytes = raw[..V4_GLOBAL_HEADER_BYTES].to_vec();

    let rh_start = V4_GLOBAL_HEADER_BYTES;
    let rh_end = rh_start + V4_RATCHET_HEADER_BYTES;
    let mut header_nonce_bytes = [0u8; V4_RATCHET_NONCE_BYTES];
    header_nonce_bytes.copy_from_slice(&raw[rh_start..rh_start + V4_RATCHET_NONCE_BYTES]);
    let enc_header_nonce = aead::Nonce::from_bytes(header_nonce_bytes);
    let enc_header = raw[rh_start + V4_RATCHET_NONCE_BYTES..rh_end].to_vec();

    let slot_start = rh_end;
    let slot_end = slot_start + SLOT_V3_BYTES;
    if raw.len() < slot_end + aead::NONCE_SIZE + aes_gcm::TAG_SIZE {
        return Err(V2Error::TooShort {
            got: raw.len(),
            expected: slot_end + aead::NONCE_SIZE + aes_gcm::TAG_SIZE,
        });
    }
    let our_pub = x25519::derive_public(recipient_ik_sk);
    let our_hash = pubkey_hash_prefix(&our_pub);
    let mut p = slot_start;
    if raw[p..p + RECIPIENT_HASH_PREFIX_LEN] != our_hash {
        return Err(V2Error::NoMatchingSlot);
    }
    p += RECIPIENT_HASH_PREFIX_LEN;
    let mut ek_bytes = [0u8; 32];
    ek_bytes.copy_from_slice(&raw[p..p + 32]);
    let ek_x25519_pub = x25519::PublicKey::from_bytes(ek_bytes);
    p += 32;
    let ct_len = u16::from_le_bytes([raw[p], raw[p + 1]]) as usize;
    p += 2;
    if ct_len != ml_kem_768::CIPHERTEXT_SIZE {
        return Err(V2Error::Crypto(format!(
            "v=4 mlkem_ct_len {ct_len} != {}",
            ml_kem_768::CIPHERTEXT_SIZE
        )));
    }
    let mut ct_bytes = [0u8; ml_kem_768::CIPHERTEXT_SIZE];
    ct_bytes.copy_from_slice(&raw[p..p + ct_len]);
    let mlkem_ciphertext = ml_kem_768::Ciphertext::from_bytes(&ct_bytes);
    p += ct_len;
    let mut wrap_nonce_bytes = [0u8; aes_gcm::NONCE_SIZE];
    wrap_nonce_bytes.copy_from_slice(&raw[p..p + aes_gcm::NONCE_SIZE]);
    let wrap_nonce = aes_gcm::Nonce::from_bytes(wrap_nonce_bytes);
    p += aes_gcm::NONCE_SIZE;
    let wrap_ct = &raw[p..p + aes_gcm::KEY_SIZE + aes_gcm::TAG_SIZE];

    let handshake = pqxdh::InitiatorHandshake {
        ek_x25519_pub,
        mlkem_ciphertext,
        no_opk: true,
        opk_id: None,
    };
    let session_key = pqxdh::respond(
        recipient_ik_sk,
        recipient_ik_sk,
        None,
        recipient_mlkem_sk,
        &sender_ik_pub,
        &handshake,
    )
    .map_err(|e| V2Error::Crypto(format!("v=4 pqxdh::respond: {e}")))?;
    let wrap_key_bytes = hkdf::derive_32(&[], session_key.as_bytes(), HKDF_INFO_WRAP_V4)
        .map_err(|e| V2Error::Crypto(format!("v=4 wrap HKDF: {e}")))?;
    let wrap_key = aes_gcm::Key::from_bytes(wrap_key_bytes);
    let mut wrap_aad = Vec::with_capacity(AD_WRAP_V4.len() + V4_GLOBAL_HEADER_BYTES);
    wrap_aad.extend_from_slice(AD_WRAP_V4);
    wrap_aad.extend_from_slice(&global_header_bytes);
    let sentinel = aes_gcm::open(&wrap_key, &wrap_nonce, &wrap_aad, wrap_ct)
        .map_err(|_| V2Error::WrapAeadFailed)?;
    if sentinel.len() != 32 || sentinel.iter().any(|&b| b != 0) {
        return Err(V2Error::Crypto(
            "v=4 wrap sentinel not 32 zero bytes".to_string(),
        ));
    }

    let body_nonce_off = slot_end;
    let body_nonce_end = body_nonce_off + aead::NONCE_SIZE;
    let mut body_nonce_bytes = [0u8; aead::NONCE_SIZE];
    body_nonce_bytes.copy_from_slice(&raw[body_nonce_off..body_nonce_end]);
    let body_nonce = aead::Nonce::from_bytes(body_nonce_bytes);
    let body_ct = raw[body_nonce_end..].to_vec();

    Ok(ParsedV4 {
        msg_type,
        bootstrap,
        sender_ik_pub,
        enc_header_nonce,
        enc_header,
        body_nonce,
        body_ct,
        session_key,
    })
}

// ============================================================
// Phase 9-A3: v=5 sender-keys multi-recipient wire format
// ============================================================

/// Parsed v=5 wire bytes — hand-off type for the IPC layer. The
/// caller looks up `SenderKeyState` for the relevant scope, picks
/// the `ReceiverChain` matching `sender_ik_pub`, reconstructs the
/// sender-keys `EncryptedMessage`, and calls `chain.decrypt(...)`.
pub struct ParsedV5 {
    pub msg_type: u8,
    pub sender_ik_pub: x25519::PublicKey,
    pub flags: u8,
    pub header_nonce: aead::Nonce,
    pub enc_header: Vec<u8>,
    pub message_nonce: aead::Nonce,
    pub ciphertext: Vec<u8>,
    pub global_header_bytes: Vec<u8>,
}

impl std::fmt::Debug for ParsedV5 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParsedV5")
            .field("msg_type", &format_args!("0x{:02x}", self.msg_type))
            .field("flags", &format_args!("0x{:02x}", self.flags))
            .field("enc_header_len", &self.enc_header.len())
            .field("body_ct_len", &self.ciphertext.len())
            .finish()
    }
}

/// Phase 9-A3: encode a v=5 wire blob. The caller pre-ran the
/// sender-keys `SenderChain::encrypt` (or `SenderKeyState::encrypt`)
/// and passes the resulting `sender_keys::EncryptedMessage` parts
/// in. The body cipher uses an AES-GCM-style nonce here, but the
/// sender-keys construction uses XChaCha20-Poly1305 (24-byte nonce,
/// 16-byte tag) for both header and body — so `header_nonce` and
/// `message_nonce` are both 24 bytes (`aead::Nonce`).
///
/// Layout:
/// ```text
///   global header (35 bytes):
///     [ version(1)=0x05 | msg_type(1) | sender_ik_x25519_pub(32) | flags(1) ]
///   sender-keys header (56 bytes):
///     [ header_nonce(24) | enc_header(32 = 16 header_bytes + 16 AEAD tag) ]
///   trailer:
///     [ message_nonce(24) | ciphertext + 16B AEAD tag ]
/// ```
///
/// The body AEAD's AAD inside `sender_keys::SenderChain::encrypt`
/// is `canonical_ad_sender_keys(...) || enc_header`. The wire-layer
/// adds no extra AAD — the canonical AD already binds sender_ik +
/// group_id + chain_id + n, and the global header's bytes are
/// implicitly authenticated via `sender_ik_x25519_pub` (any tamper
/// on that field selects the wrong ReceiverChain on decode and the
/// AEAD fails).
pub fn encrypt_v5(
    sender_ik_pub: &x25519::PublicKey,
    msg_type: u8,
    flags: u8,
    em: &crypto::sender_keys::EncryptedMessage,
) -> Result<String, V2Error> {
    if (flags & V5_FLAG_RESERVED_MASK) != 0 {
        return Err(V2Error::Crypto(format!(
            "v=5 encode: reserved flags bits set in 0x{flags:02x}"
        )));
    }
    if em.enc_header.len() != 32 {
        return Err(V2Error::Crypto(format!(
            "v=5 encode: enc_header length {} != expected 32",
            em.enc_header.len()
        )));
    }

    let mut wire = Vec::with_capacity(
        V5_GLOBAL_HEADER_BYTES
            + V5_SENDER_KEYS_HEADER_BYTES
            + aead::NONCE_SIZE
            + em.ciphertext.len(),
    );
    wire.push(WIRE_VERSION_V5);
    wire.push(msg_type);
    wire.extend_from_slice(sender_ik_pub.as_bytes());
    wire.push(flags);
    wire.extend_from_slice(em.header_nonce.as_bytes());
    wire.extend_from_slice(&em.enc_header);
    wire.extend_from_slice(em.message_nonce.as_bytes());
    wire.extend_from_slice(&em.ciphertext);

    let b64 = STANDARD.encode(&wire);
    let mut out = String::with_capacity(b64.len() + "DPC0::".len());
    out.push_str("DPC0::");
    out.push_str(&b64);
    Ok(out)
}

/// Phase 9-A3: parse a v=5 wire blob. Returns [`ParsedV5`] for the
/// caller to drive through its `SenderKeyState`. Validates:
/// - DPC0:: prefix + base64 round-trip
/// - version byte = 0x05 (checked before length so wrong-version
///   wires get a clean WrongVersion error)
/// - flags reserved bits all zero
/// - exact length matches the fixed framing
pub fn decrypt_v5(wire: &str) -> Result<ParsedV5, V2Error> {
    let body = wire.strip_prefix("DPC0::").ok_or(V2Error::BadPrefix)?;
    let raw = STANDARD
        .decode(body)
        .map_err(|e| V2Error::Base64(e.to_string()))?;

    if raw.is_empty() {
        return Err(V2Error::TooShort {
            got: 0,
            expected: 1,
        });
    }
    if raw[0] != WIRE_VERSION_V5 {
        return Err(V2Error::WrongVersion {
            got: raw[0],
            expected: WIRE_VERSION_V5,
        });
    }
    let min_framing = V5_GLOBAL_HEADER_BYTES + V5_SENDER_KEYS_HEADER_BYTES + aead::NONCE_SIZE + 16;
    if raw.len() < min_framing {
        return Err(V2Error::TooShort {
            got: raw.len(),
            expected: min_framing,
        });
    }
    let msg_type = raw[1];
    let mut sender_pub_bytes = [0u8; 32];
    sender_pub_bytes.copy_from_slice(&raw[2..34]);
    let sender_ik_pub = x25519::PublicKey::from_bytes(sender_pub_bytes);
    let flags = raw[34];
    if (flags & V5_FLAG_RESERVED_MASK) != 0 {
        return Err(V2Error::Crypto(format!(
            "v=5 decode: reserved flags bits set in 0x{flags:02x}"
        )));
    }
    let global_header_bytes = raw[..V5_GLOBAL_HEADER_BYTES].to_vec();

    let rh_start = V5_GLOBAL_HEADER_BYTES;
    let mut header_nonce_bytes = [0u8; aead::NONCE_SIZE];
    header_nonce_bytes.copy_from_slice(&raw[rh_start..rh_start + aead::NONCE_SIZE]);
    let header_nonce = aead::Nonce::from_bytes(header_nonce_bytes);
    let enc_header =
        raw[rh_start + aead::NONCE_SIZE..rh_start + V5_SENDER_KEYS_HEADER_BYTES].to_vec();

    let body_off = rh_start + V5_SENDER_KEYS_HEADER_BYTES;
    let body_nonce_end = body_off + aead::NONCE_SIZE;
    let mut body_nonce_bytes = [0u8; aead::NONCE_SIZE];
    body_nonce_bytes.copy_from_slice(&raw[body_off..body_nonce_end]);
    let message_nonce = aead::Nonce::from_bytes(body_nonce_bytes);
    let ciphertext = raw[body_nonce_end..].to_vec();

    Ok(ParsedV5 {
        msg_type,
        sender_ik_pub,
        flags,
        header_nonce,
        enc_header,
        message_nonce,
        ciphertext,
        global_header_bytes,
    })
}
