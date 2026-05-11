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
use crypto::{aes_gcm, hkdf, x25519};
use sha2::{Digest, Sha256};

/// Wire format version byte for v=2.
pub const WIRE_VERSION_V2: u8 = 0x02;

/// Message-type byte: ordinary content (post-decrypt rendered as
/// the user-visible message body).
pub const MSG_TYPE_CONTENT: u8 = 0x00;

/// Message-type byte: burn marker. Carries no body content; tells
/// the recipient to wipe its decryption capability for the named
/// scope. Phase 7a accepts the type byte on the wire but does not
/// yet act on it; Phase 7b adds the side-effect handling.
pub const MSG_TYPE_BURN: u8 = 0x01;

/// Message-type byte: whitelist invitation. Body contains a JSON
/// envelope describing the scope being offered. Phase 7b adds the
/// envelope schema + side-effect handling.
pub const MSG_TYPE_WHITELIST_INVITATION: u8 = 0x02;

/// Message-type byte: whitelist response. Body contains a JSON
/// envelope with accept/decline + the originating invitation id.
pub const MSG_TYPE_WHITELIST_RESPONSE: u8 = 0x03;

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
    /// hash matched our own derived hash.
    #[error("not a recipient of this v=2 message")]
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
/// no secret data.
fn pubkey_hash_prefix(pk: &x25519::PublicKey) -> [u8; RECIPIENT_HASH_PREFIX_LEN] {
    let digest = Sha256::digest(pk.as_bytes());
    let mut out = [0u8; RECIPIENT_HASH_PREFIX_LEN];
    out.copy_from_slice(&digest[..RECIPIENT_HASH_PREFIX_LEN]);
    out
}
