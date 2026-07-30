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
//! - `nonce` / `ciphertext` — body ciphertext under a unique random content key.
//! - `wrapped_key_nonce` / `wrapped_key` — that content key authenticated and
//!   wrapped under the store master key.
//! - `meta_nonce` / `meta_ct` — the row's identifiers and timestamp, sealed.
//! - `*_bi` — keyed blind indexes, the only searchable form of those
//!   identifiers.
//!
//! ## AAD binds row identity
//!
//! Body, wrapper, and metadata use separate AAD domains. Every one includes
//! the row's blind-index selector and content version, so cross-row transplant
//! and stale partial replay fail authentication.

use crate::StoreError;
use crypto::{aead, hkdf, random};

/// HKDF info label for the store master key. It seals metadata and wraps
/// per-record message and attachment content keys. Hard-coded; never read from
/// disk.
const HKDF_INFO: &[u8] = b"osl-message-store-v1";

/// HKDF info label for the **blind index** key. A separate derivation from the
/// encryption key so that compromising one does not hand over the other, and so
/// index values can never be confused with key material.
const INDEX_HKDF_INFO: &[u8] = b"osl-message-store-index-v1";
const ANCHOR_STORE_ID_HKDF_INFO: &[u8] = b"osl-message-store-anchor/store-id-v1";
const ANCHOR_DIGEST_HKDF_INFO: &[u8] = b"osl-message-store-anchor/digest-key-v1";
const ANCHOR_DIGEST_DOMAIN: &[u8] = b"osl-message-store-anchor/digest-v1";

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

const MESSAGE_BODY_AAD_V5: &[u8] = b"osl-store/body/v5/message";
const MESSAGE_WRAP_AAD_V5: &[u8] = b"osl-store/wrap/v5/message";
const MESSAGE_META_AAD_V5: &[u8] = b"osl-store/meta/v5/message";
const ATTACHMENT_BODY_AAD_V6: &[u8] = b"osl-store/body/v6/attachment";
const ATTACHMENT_WRAP_AAD_V6: &[u8] = b"osl-store/wrap/v6/attachment";
const ATTACHMENT_META_AAD_V6: &[u8] = b"osl-store/meta/v6/attachment";
const ATTACHMENT_COMMITMENT_V6: &[u8] = b"osl-store/commitment/v6/attachment";
const ATTACHMENT_MANIFEST_AAD_V7: &[u8] = b"osl-store/manifest/v7/attachment";
const ATTACHMENT_MANIFEST_FORMAT_V7: u32 = 1;

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

pub(crate) fn derive_anchor_material(
    identity_secret: &[u8; 32],
) -> Result<([u8; 32], [u8; 32]), StoreError> {
    let store_id = hkdf::derive_32(&[], identity_secret, ANCHOR_STORE_ID_HKDF_INFO)
        .map_err(|e| StoreError::Anchor(format!("anchor store-id derive: {e}")))?;
    let digest_key = hkdf::derive_32(&[], identity_secret, ANCHOR_DIGEST_HKDF_INFO)
        .map_err(|e| StoreError::Anchor(format!("anchor digest-key derive: {e}")))?;
    Ok((store_id, digest_key))
}

pub(crate) fn anchor_digest(
    digest_key: &[u8; 32],
    canonical_state: &[u8],
) -> Result<[u8; 32], StoreError> {
    hkdf::derive_32(digest_key, canonical_state, ANCHOR_DIGEST_DOMAIN)
        .map_err(|e| StoreError::Anchor(format!("anchor digest: {e}")))
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

fn versioned_aad(domain: &[u8], selector: &[u8], version: i64) -> Vec<u8> {
    let mut aad = Vec::with_capacity(domain.len() + 8 + selector.len() + 8);
    aad.extend_from_slice(domain);
    aad.extend_from_slice(&(selector.len() as u64).to_le_bytes());
    aad.extend_from_slice(selector);
    aad.extend_from_slice(&version.to_le_bytes());
    aad
}

pub(crate) fn message_meta_aad(mid_bi: &[u8], version: i64) -> Vec<u8> {
    versioned_aad(MESSAGE_META_AAD_V5, mid_bi, version)
}

pub(crate) struct SealedMessageBody {
    pub(crate) nonce: Vec<u8>,
    pub(crate) ciphertext: Vec<u8>,
    pub(crate) wrapped_key_nonce: Vec<u8>,
    pub(crate) wrapped_key: Vec<u8>,
}

/// Seal one message body under a fresh, one-record content key, then wrap that
/// key under the store master key. The body and envelope use distinct,
/// versioned AAD domains so neither can be transplanted across rows or record
/// types.
pub(crate) fn seal_message_body(
    master_key: &aead::Key,
    mid_bi: &[u8],
    version: i64,
    plaintext: &[u8],
) -> Result<SealedMessageBody, StoreError> {
    let content_key = random::random_aead_key();
    let (nonce, ciphertext) = seal(
        &content_key,
        &versioned_aad(MESSAGE_BODY_AAD_V5, mid_bi, version),
        plaintext,
    )?;
    let (wrapped_key_nonce, wrapped_key) = seal(
        master_key,
        &versioned_aad(MESSAGE_WRAP_AAD_V5, mid_bi, version),
        content_key.as_bytes(),
    )?;
    Ok(SealedMessageBody {
        nonce,
        ciphertext,
        wrapped_key_nonce,
        wrapped_key,
    })
}

pub(crate) fn unseal_message_body(
    master_key: &aead::Key,
    mid_bi: &[u8],
    version: i64,
    wrapped_key_nonce: &[u8],
    wrapped_key: &[u8],
    nonce: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, StoreError> {
    let key_bytes = unseal(
        master_key,
        &versioned_aad(MESSAGE_WRAP_AAD_V5, mid_bi, version),
        wrapped_key_nonce,
        wrapped_key,
    )?;
    let key_array: [u8; aead::KEY_SIZE] = key_bytes.try_into().map_err(|bytes: Vec<u8>| {
        StoreError::Corrupted(format!(
            "wrapped message content key has length {} (want {})",
            bytes.len(),
            aead::KEY_SIZE
        ))
    })?;
    let content_key = aead::Key::from_bytes(key_array);
    unseal(
        &content_key,
        &versioned_aad(MESSAGE_BODY_AAD_V5, mid_bi, version),
        nonce,
        ciphertext,
    )
}

fn push_aad_part(out: &mut Vec<u8>, part: &[u8]) {
    out.extend_from_slice(&(part.len() as u64).to_le_bytes());
    out.extend_from_slice(part);
}

fn attachment_aad(
    domain: &[u8],
    ck_bi: &[u8],
    mid_bi: &[u8],
    seq: i64,
    version: i64,
    metadata_digest: Option<&[u8; 32]>,
    body_digest: Option<&[u8; 32]>,
) -> Vec<u8> {
    let mut aad = Vec::with_capacity(
        domain.len()
            + ck_bi.len()
            + mid_bi.len()
            + 24
            + 32 * usize::from(metadata_digest.is_some())
            + 32 * usize::from(body_digest.is_some()),
    );
    push_aad_part(&mut aad, domain);
    push_aad_part(&mut aad, ck_bi);
    push_aad_part(&mut aad, mid_bi);
    aad.extend_from_slice(&seq.to_le_bytes());
    aad.extend_from_slice(&version.to_le_bytes());
    if let Some(digest) = metadata_digest {
        aad.extend_from_slice(digest);
    }
    if let Some(digest) = body_digest {
        aad.extend_from_slice(digest);
    }
    aad
}

pub(crate) fn attachment_commitment(parts: &[&[u8]]) -> [u8; 32] {
    let mut canonical = Vec::new();
    for part in parts {
        canonical.extend_from_slice(&(part.len() as u64).to_le_bytes());
        canonical.extend_from_slice(part);
    }
    // HKDF-SHA256 with a fixed domain and canonical input is used here as a
    // collision-resistant commitment, matching the crate's existing
    // blind-index construction and avoiding a second hashing implementation.
    hkdf::derive_32(&[], &canonical, ATTACHMENT_COMMITMENT_V6)
        .expect("fixed-size HKDF commitment cannot fail")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AttachmentManifestEntry {
    pub(crate) ck_bi: Vec<u8>,
    pub(crate) seq: i64,
    pub(crate) content_version: i64,
    pub(crate) metadata_commitment: [u8; 32],
    pub(crate) body_commitment: [u8; 32],
    pub(crate) wrapper_commitment: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AttachmentManifest {
    /// `false` means the inventory was observed during migration and cannot
    /// prove that an attachment absent before migration was never cached.
    /// It still authenticates every row observed at migration time.
    pub(crate) complete: bool,
    pub(crate) generation: i64,
    pub(crate) entries: Vec<AttachmentManifestEntry>,
}

fn attachment_manifest_aad(mid_bi: &[u8], complete: bool, generation: i64) -> Vec<u8> {
    let mut aad = Vec::new();
    push_aad_part(&mut aad, ATTACHMENT_MANIFEST_AAD_V7);
    push_aad_part(&mut aad, mid_bi);
    aad.push(u8::from(complete));
    aad.extend_from_slice(&generation.to_le_bytes());
    aad
}

pub(crate) fn attachment_manifest_entry(
    ck_bi: Vec<u8>,
    seq: i64,
    content_version: i64,
    metadata: &[u8],
    nonce: &[u8],
    ciphertext: &[u8],
    wrapped_key_nonce: &[u8],
    wrapped_key: &[u8],
) -> AttachmentManifestEntry {
    AttachmentManifestEntry {
        ck_bi,
        seq,
        content_version,
        metadata_commitment: attachment_commitment(&[metadata]),
        body_commitment: attachment_commitment(&[nonce, ciphertext]),
        wrapper_commitment: attachment_commitment(&[wrapped_key_nonce, wrapped_key]),
    }
}

pub(crate) fn seal_attachment_manifest(
    master_key: &aead::Key,
    mid_bi: &[u8],
    manifest: &AttachmentManifest,
) -> Result<(Vec<u8>, Vec<u8>), StoreError> {
    if manifest.generation < 1 {
        return Err(StoreError::Corrupted(
            "attachment manifest generation must be positive".to_string(),
        ));
    }
    seal(
        master_key,
        &attachment_manifest_aad(mid_bi, manifest.complete, manifest.generation),
        &encode_attachment_manifest(manifest),
    )
}

pub(crate) fn unseal_attachment_manifest(
    master_key: &aead::Key,
    mid_bi: &[u8],
    complete: bool,
    generation: i64,
    nonce: &[u8],
    ciphertext: &[u8],
) -> Result<AttachmentManifest, StoreError> {
    if generation < 1 {
        return Err(StoreError::Corrupted(
            "attachment manifest generation must be positive".to_string(),
        ));
    }
    let plaintext = unseal(
        master_key,
        &attachment_manifest_aad(mid_bi, complete, generation),
        nonce,
        ciphertext,
    )?;
    let manifest = decode_attachment_manifest(&plaintext)?;
    if manifest.complete != complete || manifest.generation != generation {
        return Err(StoreError::Corrupted(
            "attachment manifest columns do not match authenticated contents".to_string(),
        ));
    }
    Ok(manifest)
}

pub(crate) fn attachment_meta_aad(ck_bi: &[u8], mid_bi: &[u8], seq: i64, version: i64) -> Vec<u8> {
    attachment_aad(
        ATTACHMENT_META_AAD_V6,
        ck_bi,
        mid_bi,
        seq,
        version,
        None,
        None,
    )
}

pub(crate) struct SealedAttachmentBody {
    pub(crate) meta_nonce: Vec<u8>,
    pub(crate) meta_ct: Vec<u8>,
    pub(crate) nonce: Vec<u8>,
    pub(crate) ciphertext: Vec<u8>,
    pub(crate) wrapped_key_nonce: Vec<u8>,
    pub(crate) wrapped_key: Vec<u8>,
}

/// Seal one attachment under a fresh content key.
///
/// The metadata AEAD authenticates record type, attachment selector, owning
/// message selector, ordering position, and content version. The body repeats
/// that context and commits to the canonical metadata. The master-key wrapper
/// repeats it again and additionally commits to the exact body nonce and
/// ciphertext. Moving any subset of those columns across rows or versions
/// therefore fails before plaintext is released.
pub(crate) fn seal_attachment_body(
    master_key: &aead::Key,
    ck_bi: &[u8],
    mid_bi: &[u8],
    seq: i64,
    version: i64,
    metadata: &[u8],
    plaintext: &[u8],
) -> Result<SealedAttachmentBody, StoreError> {
    let metadata_digest = attachment_commitment(&[metadata]);
    let (meta_nonce, meta_ct) = seal(
        master_key,
        &attachment_meta_aad(ck_bi, mid_bi, seq, version),
        metadata,
    )?;
    let content_key = random::random_aead_key();
    let (nonce, ciphertext) = seal(
        &content_key,
        &attachment_aad(
            ATTACHMENT_BODY_AAD_V6,
            ck_bi,
            mid_bi,
            seq,
            version,
            Some(&metadata_digest),
            None,
        ),
        plaintext,
    )?;
    let body_digest = attachment_commitment(&[&nonce, &ciphertext]);
    let (wrapped_key_nonce, wrapped_key) = seal(
        master_key,
        &attachment_aad(
            ATTACHMENT_WRAP_AAD_V6,
            ck_bi,
            mid_bi,
            seq,
            version,
            Some(&metadata_digest),
            Some(&body_digest),
        ),
        content_key.as_bytes(),
    )?;
    Ok(SealedAttachmentBody {
        meta_nonce,
        meta_ct,
        nonce,
        ciphertext,
        wrapped_key_nonce,
        wrapped_key,
    })
}

pub(crate) fn unseal_attachment_body(
    master_key: &aead::Key,
    ck_bi: &[u8],
    mid_bi: &[u8],
    seq: i64,
    version: i64,
    metadata: &[u8],
    wrapped_key_nonce: &[u8],
    wrapped_key: &[u8],
    nonce: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, StoreError> {
    let metadata_digest = attachment_commitment(&[metadata]);
    let body_digest = attachment_commitment(&[nonce, ciphertext]);
    let key_bytes = unseal(
        master_key,
        &attachment_aad(
            ATTACHMENT_WRAP_AAD_V6,
            ck_bi,
            mid_bi,
            seq,
            version,
            Some(&metadata_digest),
            Some(&body_digest),
        ),
        wrapped_key_nonce,
        wrapped_key,
    )?;
    let key_array: [u8; aead::KEY_SIZE] = key_bytes.try_into().map_err(|bytes: Vec<u8>| {
        StoreError::Corrupted(format!(
            "wrapped attachment content key has length {} (want {})",
            bytes.len(),
            aead::KEY_SIZE
        ))
    })?;
    let content_key = aead::Key::from_bytes(key_array);
    unseal(
        &content_key,
        &attachment_aad(
            ATTACHMENT_BODY_AAD_V6,
            ck_bi,
            mid_bi,
            seq,
            version,
            Some(&metadata_digest),
            None,
        ),
        nonce,
        ciphertext,
    )
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

    /// Refuse leftover bytes.
    ///
    /// Without this, a decoder that consumes its own fields and stops is a
    /// type-confusion oracle: attachment metadata begins with the same four
    /// length-prefixed strings and an `i64` as message metadata, so an
    /// attachment blob transplanted into a message row decoded cleanly — its
    /// cache key read as a message id, its MIME as an OSL identity — and the
    /// AEAD verified, because the AEAD only ever saw opaque bytes. Requiring
    /// exhaustion makes the two encodings unambiguous without changing either
    /// of them.
    fn end(&self) -> Result<(), StoreError> {
        if self.at != self.buf.len() {
            return Err(StoreError::Corrupted(format!(
                "sealed metadata has {} trailing bytes — it is not the record \
                 type this row claims",
                self.buf.len() - self.at
            )));
        }
        Ok(())
    }

    fn i64(&mut self) -> Result<i64, StoreError> {
        let raw = self.take(8)?;
        let mut n = [0u8; 8];
        n.copy_from_slice(raw);
        Ok(i64::from_le_bytes(n))
    }

    fn u32(&mut self) -> Result<u32, StoreError> {
        let raw = self.take(4)?;
        let mut n = [0u8; 4];
        n.copy_from_slice(raw);
        Ok(u32::from_le_bytes(n))
    }

    fn bytes(&mut self) -> Result<Vec<u8>, StoreError> {
        let raw = self.take(8)?;
        let mut n = [0u8; 8];
        n.copy_from_slice(raw);
        let len = usize::try_from(u64::from_le_bytes(n)).map_err(|_| {
            StoreError::Corrupted("sealed manifest length does not fit memory".to_string())
        })?;
        Ok(self.take(len)?.to_vec())
    }
}

fn push_bytes(out: &mut Vec<u8>, value: &[u8]) {
    out.extend_from_slice(&(value.len() as u64).to_le_bytes());
    out.extend_from_slice(value);
}

pub(crate) fn encode_attachment_manifest(manifest: &AttachmentManifest) -> Vec<u8> {
    let mut out = Vec::with_capacity(32 + manifest.entries.len() * 160);
    out.extend_from_slice(&ATTACHMENT_MANIFEST_FORMAT_V7.to_le_bytes());
    out.push(u8::from(manifest.complete));
    out.extend_from_slice(&manifest.generation.to_le_bytes());
    out.extend_from_slice(&(manifest.entries.len() as u64).to_le_bytes());
    for entry in &manifest.entries {
        push_bytes(&mut out, &entry.ck_bi);
        out.extend_from_slice(&entry.seq.to_le_bytes());
        out.extend_from_slice(&entry.content_version.to_le_bytes());
        out.extend_from_slice(&entry.metadata_commitment);
        out.extend_from_slice(&entry.body_commitment);
        out.extend_from_slice(&entry.wrapper_commitment);
    }
    out
}

pub(crate) fn decode_attachment_manifest(
    plaintext: &[u8],
) -> Result<AttachmentManifest, StoreError> {
    let mut reader = Reader::new(plaintext);
    if reader.u32()? != ATTACHMENT_MANIFEST_FORMAT_V7 {
        return Err(StoreError::Corrupted(
            "unsupported attachment manifest format".to_string(),
        ));
    }
    let complete = match reader.take(1)?[0] {
        0 => false,
        1 => true,
        _ => {
            return Err(StoreError::Corrupted(
                "attachment manifest has invalid coverage marker".to_string(),
            ))
        }
    };
    let generation = reader.i64()?;
    let raw_count = reader.take(8)?;
    let mut count_bytes = [0u8; 8];
    count_bytes.copy_from_slice(raw_count);
    let count = usize::try_from(u64::from_le_bytes(count_bytes)).map_err(|_| {
        StoreError::Corrupted("attachment manifest count does not fit memory".to_string())
    })?;
    // A manifest cannot legitimately contain more entries than bytes left;
    // bounding before allocation keeps a forged count from becoming an OOM.
    if count > reader.buf.len().saturating_sub(reader.at) / 152 {
        return Err(StoreError::Corrupted(
            "attachment manifest count exceeds encoded entries".to_string(),
        ));
    }
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let ck_bi = reader.bytes()?;
        if ck_bi.len() != 32 {
            return Err(StoreError::Corrupted(
                "attachment manifest selector has wrong length".to_string(),
            ));
        }
        let seq = reader.i64()?;
        let content_version = reader.i64()?;
        let mut metadata_commitment = [0u8; 32];
        metadata_commitment.copy_from_slice(reader.take(32)?);
        let mut body_commitment = [0u8; 32];
        body_commitment.copy_from_slice(reader.take(32)?);
        let mut wrapper_commitment = [0u8; 32];
        wrapper_commitment.copy_from_slice(reader.take(32)?);
        entries.push(AttachmentManifestEntry {
            ck_bi,
            seq,
            content_version,
            metadata_commitment,
            body_commitment,
            wrapper_commitment,
        });
    }
    reader.end()?;
    Ok(AttachmentManifest {
        complete,
        generation,
        entries,
    })
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
    let out = MessageMeta {
        discord_message_id: r.str()?,
        channel_id: r.str()?,
        sender_discord_id: r.str()?,
        sender_osl_user_id: r.str()?,
        decrypted_at: r.i64()?,
    };
    r.end()?;
    Ok(out)
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
    let out = AttachmentMeta {
        cache_key: r.str()?,
        discord_message_id: r.str()?,
        random_filename: r.str()?,
        mime: r.str()?,
        byte_len: r.i64()?,
        created_at: r.i64()?,
        scope_type: r.opt_str()?,
        scope_id: r.opt_str()?,
        sender_discord_id: r.opt_str()?,
    };
    r.end()?;
    Ok(out)
}
