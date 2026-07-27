//! HKDF + AEAD plumbing for the message store.
//!
//! The store does **not** roll new crypto. It composes the same audited
//! primitives the keystore sealer module uses internally:
//!
//! - `crypto::hkdf::derive_32` for key derivation
//!   (HKDF-SHA256 with salt = empty, info = `b"osl-message-store-v1"`).
//! - `crypto::aead::seal` / `crypto::aead::open` for per-row
//!   encryption (XChaCha20-Poly1305 with a fresh random nonce
//!   per `seal` call).
//!
//! ## Per-row layout
//!
//! Each `messages` row stores:
//!
//! - `nonce` (24 bytes) — the random XChaCha20-Poly1305 nonce.
//! - `ciphertext` — the AEAD ciphertext + tag (so any tag failure
//!   surfaces as `StoreError::Corrupted`, not as silent garbage).
//! - `meta_nonce` / `meta_ct` — the row's identifiers and timestamp, sealed.
//! - `*_bi` — keyed blind indexes, the only searchable form of those
//!   identifiers.
//!
//! ## AAD binds row identity
//!
//! The AAD passed to `aead::seal` for the **body** is the row's
//! `discord_message_id` UTF-8 bytes. This binds the ciphertext to its
//! row-identifier so an attacker who shuffles `ciphertext` / `nonce` blobs
//! across rows produces tag failures instead of cross-row plaintext recovery.
//!
//! That AAD is deliberately unchanged by schema v4: the v3→v4 migration
//! rewrites metadata columns only and never re-encrypts a message body, so a
//! body sealed by any earlier build stays openable.

use crate::StoreError;
use crypto::{aead, hkdf, random};

/// HKDF info label for the body/metadata encryption key. Hard-coded; never
/// read from disk. Bumping the suffix ("-v1" → "-v2") forces a re-derive and
/// would invalidate every existing row's ciphertext, so it pairs with a schema
/// migration that re-encrypts under the new key.
const HKDF_INFO: &[u8] = b"osl-message-store-v1";

/// HKDF info label for the **blind index** key. A separate derivation from the
/// encryption key so that compromising one does not hand over the other, and so
/// index values can never be confused with key material.
const INDEX_HKDF_INFO: &[u8] = b"osl-message-store-index-v1";

/// Per-field domains for blind indexes.
///
/// Each field gets its own domain so the *same* string in two roles produces
/// two unrelated indexes. Without this, an observer who noticed
/// `chan_bi == sender_bi` would learn that a channel id equals a sender id and
/// could re-link the graph across columns — recovering by inference part of
/// what blinding is meant to remove.
pub(crate) const BI_MESSAGE_ID: &[u8] = b"osl-store-bi/discord_message_id-v1";
pub(crate) const BI_CHANNEL_ID: &[u8] = b"osl-store-bi/channel_id-v1";
pub(crate) const BI_SENDER_ID: &[u8] = b"osl-store-bi/sender_discord_id-v1";
pub(crate) const BI_CACHE_KEY: &[u8] = b"osl-store-bi/cache_key-v1";

/// Derive the message-store AEAD key from the caller-supplied
/// 32-byte identity secret. Returns an opaque
/// [`aead::Key`] suitable for [`seal`] / [`unseal`].
pub(crate) fn derive_key(identity_secret: &[u8; 32]) -> Result<aead::Key, StoreError> {
    let bytes = hkdf::derive_32(&[], identity_secret, HKDF_INFO)
        .map_err(|e| StoreError::Sealer(format!("HKDF derive: {e}")))?;
    Ok(aead::Key::from_bytes(bytes))
}

/// Derive the blind-index key. Domain-separated from [`derive_key`] by its
/// HKDF `info`, so the two are independent outputs of the same secret.
pub(crate) fn derive_index_key(identity_secret: &[u8; 32]) -> Result<[u8; 32], StoreError> {
    hkdf::derive_32(&[], identity_secret, INDEX_HKDF_INFO)
        .map_err(|e| StoreError::Sealer(format!("HKDF index derive: {e}")))
}

/// Compute a keyed blind index for one identifier.
///
/// `HKDF-SHA256(salt = index_key, ikm = value, info = domain)`. With the key as
/// the salt this is an HMAC of the value keyed by `index_key`, expanded to 32
/// bytes — a standard keyed hash built from the audited primitive rather than a
/// new construction.
///
/// The result is deterministic, which is exactly what makes equality lookups
/// work and is also the known limit of the scheme: equal identifiers produce
/// equal indexes, so an observer still sees *that* two rows share a channel,
/// just never *which* channel. Frequency analysis over those groupings remains
/// possible and is documented rather than hidden.
pub(crate) fn blind_index(
    index_key: &[u8; 32],
    domain: &[u8],
    value: &str,
) -> Result<Vec<u8>, StoreError> {
    let out = hkdf::derive_32(index_key, value.as_bytes(), domain)
        .map_err(|e| StoreError::Sealer(format!("blind index derive: {e}")))?;
    Ok(out.to_vec())
}

/// Seal `plaintext` under `key`, binding the AEAD to `aad`.
///
/// Returns `(nonce_bytes, ciphertext_bytes)` for direct insertion
/// as the row's two BLOB columns. Nonce is 24 bytes; ciphertext is
/// `plaintext.len() + 16` bytes (Poly1305 tag).
pub(crate) fn seal(
    key: &aead::Key,
    aad: &[u8],
    plaintext: &[u8],
) -> Result<(Vec<u8>, Vec<u8>), StoreError> {
    let nonce = random::random_nonce();
    let ct = aead::seal(key, &nonce, aad, plaintext)
        .map_err(|e| StoreError::Sealer(format!("AEAD seal: {e}")))?;
    Ok((nonce.as_bytes().to_vec(), ct))
}

/// Unseal a row at runtime. Tag failure produces
/// [`StoreError::Corrupted`] — the canary check at `open` should
/// have caught wrong-secret already, so a tag failure here points
/// at on-disk tampering or a per-row drift.
pub(crate) fn unseal(
    key: &aead::Key,
    aad: &[u8],
    nonce_bytes: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, StoreError> {
    if nonce_bytes.len() != aead::NONCE_SIZE {
        return Err(StoreError::Corrupted(format!(
            "nonce field has wrong length {} (want {})",
            nonce_bytes.len(),
            aead::NONCE_SIZE
        )));
    }
    let mut nb = [0u8; aead::NONCE_SIZE];
    nb.copy_from_slice(nonce_bytes);
    let nonce = aead::Nonce::from_bytes(nb);
    aead::open(key, &nonce, aad, ciphertext)
        .map_err(|_| StoreError::Corrupted("AEAD tag failure".to_string()))
}

/// Same wire as [`unseal`] but with a different error category —
/// used at `open` to validate the canary. AEAD failure here means
/// the caller-supplied secret does not match the one that originally
/// initialised the store, so we surface `Sealer` (a clear
/// "wrong identity_secret" diagnostic) rather than `Corrupted`.
pub(crate) fn unseal_canary(
    key: &aead::Key,
    aad: &[u8],
    nonce_bytes: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, StoreError> {
    if nonce_bytes.len() != aead::NONCE_SIZE {
        return Err(StoreError::Sealer(format!(
            "canary nonce length {} != {}",
            nonce_bytes.len(),
            aead::NONCE_SIZE
        )));
    }
    let mut nb = [0u8; aead::NONCE_SIZE];
    nb.copy_from_slice(nonce_bytes);
    let nonce = aead::Nonce::from_bytes(nb);
    aead::open(key, &nonce, aad, ciphertext)
        .map_err(|_| StoreError::Sealer("wrong identity_secret (canary unseal failed)".to_string()))
}

// ---- canonical metadata encoding ----
//
// Every field is length-prefixed. Raw concatenation would let a byte move
// across a field boundary — `("ab","c")` and `("a","bc")` would encode
// identically — which both corrupts decoding and, because this blob is sealed
// with the row's blind index as AAD, would weaken the authentication it
// provides.

fn push_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u64).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn push_opt(out: &mut Vec<u8>, s: Option<&str>) {
    match s {
        Some(v) => {
            out.push(1);
            push_str(out, v);
        }
        None => out.push(0),
    }
}

struct Reader<'a> {
    buf: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Reader { buf, at: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], StoreError> {
        let end = self
            .at
            .checked_add(n)
            .ok_or_else(|| StoreError::Corrupted("sealed metadata length overflow".to_string()))?;
        if end > self.buf.len() {
            return Err(StoreError::Corrupted(
                "sealed metadata is truncated".to_string(),
            ));
        }
        let out = &self.buf[self.at..end];
        self.at = end;
        Ok(out)
    }

    fn str(&mut self) -> Result<String, StoreError> {
        let len_bytes = self.take(8)?;
        let mut n = [0u8; 8];
        n.copy_from_slice(len_bytes);
        let len = u64::from_le_bytes(n) as usize;
        let raw = self.take(len)?;
        String::from_utf8(raw.to_vec())
            .map_err(|_| StoreError::Corrupted("sealed metadata is not valid UTF-8".to_string()))
    }

    fn opt_str(&mut self) -> Result<Option<String>, StoreError> {
        match self.take(1)?[0] {
            0 => Ok(None),
            1 => Ok(Some(self.str()?)),
            other => Err(StoreError::Corrupted(format!(
                "sealed metadata has invalid presence byte {other}"
            ))),
        }
    }

    fn i64(&mut self) -> Result<i64, StoreError> {
        let raw = self.take(8)?;
        let mut n = [0u8; 8];
        n.copy_from_slice(raw);
        Ok(i64::from_le_bytes(n))
    }
}

/// Plaintext metadata of one `messages` row, held only in memory.
pub(crate) struct MessageMeta {
    pub discord_message_id: String,
    pub channel_id: String,
    pub sender_discord_id: String,
    pub sender_osl_user_id: String,
    pub decrypted_at: i64,
}

pub(crate) fn encode_message_meta(m: &MessageMeta) -> Vec<u8> {
    let mut out = Vec::with_capacity(128);
    push_str(&mut out, &m.discord_message_id);
    push_str(&mut out, &m.channel_id);
    push_str(&mut out, &m.sender_discord_id);
    push_str(&mut out, &m.sender_osl_user_id);
    out.extend_from_slice(&m.decrypted_at.to_le_bytes());
    out
}

pub(crate) fn decode_message_meta(buf: &[u8]) -> Result<MessageMeta, StoreError> {
    let mut r = Reader::new(buf);
    Ok(MessageMeta {
        discord_message_id: r.str()?,
        channel_id: r.str()?,
        sender_discord_id: r.str()?,
        sender_osl_user_id: r.str()?,
        decrypted_at: r.i64()?,
    })
}

/// Plaintext metadata of one `attachments` row, held only in memory.
pub(crate) struct AttachmentMeta {
    pub cache_key: String,
    pub discord_message_id: String,
    pub random_filename: String,
    pub mime: String,
    pub byte_len: i64,
    pub created_at: i64,
    pub scope_type: Option<String>,
    pub scope_id: Option<String>,
    pub sender_discord_id: Option<String>,
}

pub(crate) fn encode_attachment_meta(m: &AttachmentMeta) -> Vec<u8> {
    let mut out = Vec::with_capacity(160);
    push_str(&mut out, &m.cache_key);
    push_str(&mut out, &m.discord_message_id);
    push_str(&mut out, &m.random_filename);
    push_str(&mut out, &m.mime);
    out.extend_from_slice(&m.byte_len.to_le_bytes());
    out.extend_from_slice(&m.created_at.to_le_bytes());
    push_opt(&mut out, m.scope_type.as_deref());
    push_opt(&mut out, m.scope_id.as_deref());
    push_opt(&mut out, m.sender_discord_id.as_deref());
    out
}

pub(crate) fn decode_attachment_meta(buf: &[u8]) -> Result<AttachmentMeta, StoreError> {
    let mut r = Reader::new(buf);
    Ok(AttachmentMeta {
        cache_key: r.str()?,
        discord_message_id: r.str()?,
        random_filename: r.str()?,
        mime: r.str()?,
        byte_len: r.i64()?,
        created_at: r.i64()?,
        scope_type: r.opt_str()?,
        scope_id: r.opt_str()?,
        sender_discord_id: r.opt_str()?,
    })
}
