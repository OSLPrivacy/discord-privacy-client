//! Attachment streaming AEAD.
//!
//! Spec: `docs/design/pqxdh-double-ratchet.md`
//! "Attachment integration (resolved)" + the attachment-bucket entries
//! of the "Padding (mandatory, all messages)" subsection.
//!
//! ## Construction
//!
//! Per the design doc, each attachment gets a fresh AEAD key wrapped
//! under the parent text message's chain key:
//!
//! ```text
//! AttachmentKey = HKDF-SHA256(
//!     salt = MK_n,
//!     ikm  = "attachment-key-wrap",
//!     info = content_id || u32_be(attachment_index)
//! )
//! ```
//!
//! There is **no separate attachment chain** — burning the parent
//! message's wrapped key revokes the attachment by construction.
//!
//! ## Padding
//!
//! Plaintext is padded to the smallest fitting bucket:
//! 256 KB / 1 MB / 5 MB / 10 MB / 25 MB. Padding is applied **before**
//! AEAD across the whole stream, so it lives inside the AEAD ciphertext
//! and is authenticated by the per-chunk tags — it cannot be stripped
//! without invalidating an AEAD tag.
//!
//! Layout of the pre-AEAD plaintext stream (always exactly bucket size):
//!
//! ```text
//! [u64 BE plaintext_length][plaintext bytes][zero padding to bucket boundary]
//! ```
//!
//! ## Streaming AEAD
//!
//! The padded plaintext stream is split into fixed-size chunks and
//! each chunk is AEAD-encrypted independently with XChaCha20-Poly1305.
//! Memory bound: a producer / consumer never holds more than
//! [`ATTACHMENT_CHUNK_SIZE`] bytes of plaintext at once, satisfying
//! the design doc's "≤ 16 KB plaintext window" constraint.
//!
//! Bucket sizes are exact multiples of [`ATTACHMENT_CHUNK_SIZE`], so
//! every chunk — including the last — is exactly [`ATTACHMENT_CHUNK_SIZE`]
//! bytes of plaintext (16 KB) + 16-byte AEAD tag.
//!
//! Per-chunk nonce: `base_nonce[0..20] || u32_be(chunk_index)`. A
//! fresh 20-byte random `base_nonce[0..20]` is drawn per attachment;
//! within an attachment, the chunk index disambiguates. (The
//! attachment key is itself unique per `(MK_n, content_id,
//! attachment_index)` triple via HKDF, so cross-attachment collisions
//! are not a concern even if `base_nonce` were repeated.)
//!
//! Per-chunk AAD binds the canonical [`StreamHeader`] bytes, the
//! chunk index, and a "final" flag — so chunk reordering, swapping
//! between streams, or truncating off the last chunk all break the
//! AEAD tag.
//!
//! ## Wire layout
//!
//! ```text
//! [serialized StreamHeader]
//! [chunk_0 ciphertext + tag]
//! [chunk_1 ciphertext + tag]
//! ...
//! [chunk_{N-1} ciphertext + tag]
//! ```
//!
//! Where `N = bucket_size / ATTACHMENT_CHUNK_SIZE`.

use crate::aead;
use crate::error::{Error, Result};
use crate::hkdf;
use crate::random;

/// Attachment-padding buckets per the design doc.
pub const ATTACHMENT_BUCKETS: &[u64] = &[
    256 * 1024,
    1024 * 1024,
    5 * 1024 * 1024,
    10 * 1024 * 1024,
    25 * 1024 * 1024,
];

/// Per-chunk plaintext window. The streaming construction never holds
/// more than this many bytes of plaintext in memory at once.
pub const ATTACHMENT_CHUNK_SIZE: usize = 16 * 1024;

/// 8-byte big-endian plaintext-length prefix at the very start of the
/// padded plaintext stream.
pub const LENGTH_PREFIX_SIZE: usize = 8;

const ATTACHMENT_KEY_WRAP_IKM: &[u8] = b"attachment-key-wrap";

/// Wire-format version of the attachment stream encoding. Embedded in
/// every stream header and validated on receive.
pub const STREAM_VERSION_V1: u32 = 1;

/// Magic bytes prefix on the stream header — `"DPCATT"` + version byte
/// — so a corrupted or wrong-format input fails fast before AEAD.
const STREAM_MAGIC: [u8; 7] = *b"DPCATT\x01";

/// Largest plaintext that fits in any attachment bucket.
pub fn max_attachment_plaintext_size() -> u64 {
    *ATTACHMENT_BUCKETS
        .last()
        .expect("ATTACHMENT_BUCKETS is non-empty")
        - LENGTH_PREFIX_SIZE as u64
}

fn pick_bucket(plaintext_len: u64) -> Result<u64> {
    let needed =
        plaintext_len
            .checked_add(LENGTH_PREFIX_SIZE as u64)
            .ok_or(Error::PaddingOverflow {
                max: max_attachment_plaintext_size() as usize,
                got: usize::MAX,
            })?;
    ATTACHMENT_BUCKETS
        .iter()
        .copied()
        .find(|&b| b >= needed)
        .ok_or(Error::PaddingOverflow {
            max: max_attachment_plaintext_size() as usize,
            got: plaintext_len as usize,
        })
}

/// Wrap an attachment key under the current message-chain key per the
/// design doc:
///
/// ```text
/// AttachmentKey = HKDF-SHA256(
///     salt = MK_n,
///     ikm  = "attachment-key-wrap",
///     info = content_id || u32_be(attachment_index)
/// )
/// ```
pub fn wrap_attachment_key(
    mk: &aead::Key,
    content_id: &[u8],
    attachment_index: u32,
) -> Result<aead::Key> {
    let mut info = Vec::with_capacity(content_id.len() + 4);
    info.extend_from_slice(content_id);
    info.extend_from_slice(&attachment_index.to_be_bytes());
    let bytes = hkdf::derive_32(mk.as_bytes(), ATTACHMENT_KEY_WRAP_IKM, &info)?;
    Ok(aead::Key::from_bytes(bytes))
}

/// Public stream header. Plaintext on the wire (the per-chunk AAD
/// binds it to every chunk's tag, so any tampering with the header
/// invalidates the next chunk decrypt).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamHeader {
    pub version: u32,
    pub bucket_size: u64,
    pub plaintext_len: u64,
    pub chunk_size: u32,
    pub total_chunks: u32,
    pub base_nonce_prefix: [u8; 20],
    pub content_id: Vec<u8>,
    pub attachment_index: u32,
}

impl StreamHeader {
    /// Serialize to wire bytes:
    ///
    /// ```text
    /// magic (7 B = "DPCATT\x01")
    /// version (u32 BE)
    /// bucket_size (u64 BE)
    /// plaintext_len (u64 BE)
    /// chunk_size (u32 BE)
    /// total_chunks (u32 BE)
    /// base_nonce_prefix (20 B)
    /// attachment_index (u32 BE)
    /// content_id (u32 BE length || bytes)
    /// ```
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(
            STREAM_MAGIC.len() + 4 + 8 + 8 + 4 + 4 + 20 + 4 + 4 + self.content_id.len(),
        );
        buf.extend_from_slice(&STREAM_MAGIC);
        buf.extend_from_slice(&self.version.to_be_bytes());
        buf.extend_from_slice(&self.bucket_size.to_be_bytes());
        buf.extend_from_slice(&self.plaintext_len.to_be_bytes());
        buf.extend_from_slice(&self.chunk_size.to_be_bytes());
        buf.extend_from_slice(&self.total_chunks.to_be_bytes());
        buf.extend_from_slice(&self.base_nonce_prefix);
        buf.extend_from_slice(&self.attachment_index.to_be_bytes());
        buf.extend_from_slice(&(self.content_id.len() as u32).to_be_bytes());
        buf.extend_from_slice(&self.content_id);
        buf
    }

    /// Parse a header from the start of a byte slice. Returns the
    /// header and the number of bytes consumed.
    pub fn deserialize(bytes: &[u8]) -> Result<(Self, usize)> {
        let mut off = 0;
        if bytes.len() < STREAM_MAGIC.len() {
            return Err(Error::Internal(
                "attachment stream header: too short for magic".into(),
            ));
        }
        if bytes[..STREAM_MAGIC.len()] != STREAM_MAGIC {
            return Err(Error::Internal(
                "attachment stream header: bad magic / version byte".into(),
            ));
        }
        off += STREAM_MAGIC.len();
        let need = 4 + 8 + 8 + 4 + 4 + 20 + 4 + 4;
        if bytes.len() < off + need {
            return Err(Error::Internal(
                "attachment stream header: truncated fixed fields".into(),
            ));
        }
        let version = u32::from_be_bytes(bytes[off..off + 4].try_into().unwrap());
        off += 4;
        let bucket_size = u64::from_be_bytes(bytes[off..off + 8].try_into().unwrap());
        off += 8;
        let plaintext_len = u64::from_be_bytes(bytes[off..off + 8].try_into().unwrap());
        off += 8;
        let chunk_size = u32::from_be_bytes(bytes[off..off + 4].try_into().unwrap());
        off += 4;
        let total_chunks = u32::from_be_bytes(bytes[off..off + 4].try_into().unwrap());
        off += 4;
        let mut base_nonce_prefix = [0u8; 20];
        base_nonce_prefix.copy_from_slice(&bytes[off..off + 20]);
        off += 20;
        let attachment_index = u32::from_be_bytes(bytes[off..off + 4].try_into().unwrap());
        off += 4;
        let cid_len = u32::from_be_bytes(bytes[off..off + 4].try_into().unwrap()) as usize;
        off += 4;
        if bytes.len() < off + cid_len {
            return Err(Error::Internal(
                "attachment stream header: truncated content_id".into(),
            ));
        }
        let content_id = bytes[off..off + cid_len].to_vec();
        off += cid_len;

        // Sanity-check the structural invariants against the bucket
        // table. Receivers reject before AEAD if anything is wrong.
        if version != STREAM_VERSION_V1 {
            return Err(Error::Internal(format!(
                "attachment stream header: unsupported version {version}"
            )));
        }
        if !ATTACHMENT_BUCKETS.contains(&bucket_size) {
            return Err(Error::Internal(format!(
                "attachment stream header: bucket_size {bucket_size} is not a recognised bucket"
            )));
        }
        if chunk_size as usize != ATTACHMENT_CHUNK_SIZE {
            return Err(Error::Internal(format!(
                "attachment stream header: chunk_size {chunk_size} != expected {ATTACHMENT_CHUNK_SIZE}"
            )));
        }
        let expected_total = bucket_size / chunk_size as u64;
        if expected_total != total_chunks as u64 {
            return Err(Error::Internal(format!(
                "attachment stream header: total_chunks {} != bucket_size/chunk_size {}",
                total_chunks, expected_total
            )));
        }
        if plaintext_len + LENGTH_PREFIX_SIZE as u64 > bucket_size {
            return Err(Error::Internal(format!(
                "attachment stream header: plaintext_len {} exceeds bucket_size {} payload \
                 capacity",
                plaintext_len,
                bucket_size - LENGTH_PREFIX_SIZE as u64
            )));
        }

        Ok((
            StreamHeader {
                version,
                bucket_size,
                plaintext_len,
                chunk_size,
                total_chunks,
                base_nonce_prefix,
                content_id,
                attachment_index,
            },
            off,
        ))
    }
}

/// `nonce_i = base_nonce_prefix (20 B) || u32_be(chunk_index)`.
fn chunk_nonce(prefix: &[u8; 20], chunk_index: u32) -> aead::Nonce {
    let mut bytes = [0u8; aead::NONCE_SIZE];
    bytes[..20].copy_from_slice(prefix);
    bytes[20..].copy_from_slice(&chunk_index.to_be_bytes());
    aead::Nonce::from_bytes(bytes)
}

/// Per-chunk AAD: length-prefixed header bytes + chunk index +
/// is-final flag.
fn chunk_aad(header_bytes: &[u8], chunk_index: u32, is_final: bool) -> Vec<u8> {
    let mut buf = Vec::with_capacity(4 + header_bytes.len() + 4 + 1);
    buf.extend_from_slice(&(header_bytes.len() as u32).to_be_bytes());
    buf.extend_from_slice(header_bytes);
    buf.extend_from_slice(&chunk_index.to_be_bytes());
    buf.push(if is_final { 1 } else { 0 });
    buf
}

/// Streaming attachment encryptor. Holds at most one
/// [`ATTACHMENT_CHUNK_SIZE`]-byte plaintext chunk at a time.
///
/// Usage:
/// 1. [`StreamEncryptor::new`] — returns the encryptor and the
///    serialized [`StreamHeader`] bytes (the caller writes them out
///    first, then streams chunks).
/// 2. [`StreamEncryptor::write`] — feed plaintext bytes; emits zero
///    or more complete ciphertext chunks. Buffers up to one chunk
///    internally.
/// 3. [`StreamEncryptor::finalize`] — flushes the in-progress chunk,
///    emits zero-padded chunks up to the bucket boundary, and asserts
///    the total chunk count matches the header.
pub struct StreamEncryptor {
    key: aead::Key,
    header: StreamHeader,
    header_bytes: Vec<u8>,
    chunk_index: u32,
    plaintext_consumed: u64,
    pending: Vec<u8>,
    length_prefix_pending: [u8; LENGTH_PREFIX_SIZE],
    length_prefix_emitted: bool,
}

impl StreamEncryptor {
    pub fn new(
        key: aead::Key,
        plaintext_len: u64,
        content_id: Vec<u8>,
        attachment_index: u32,
    ) -> Result<(Self, Vec<u8>)> {
        let bucket_size = pick_bucket(plaintext_len)?;
        let total_chunks = (bucket_size / ATTACHMENT_CHUNK_SIZE as u64) as u32;
        let mut prefix = [0u8; 20];
        let randoms = random::random_bytes(20);
        prefix.copy_from_slice(&randoms);

        let header = StreamHeader {
            version: STREAM_VERSION_V1,
            bucket_size,
            plaintext_len,
            chunk_size: ATTACHMENT_CHUNK_SIZE as u32,
            total_chunks,
            base_nonce_prefix: prefix,
            content_id,
            attachment_index,
        };
        let header_bytes = header.serialize();

        Ok((
            StreamEncryptor {
                key,
                length_prefix_pending: plaintext_len.to_be_bytes(),
                header,
                header_bytes: header_bytes.clone(),
                chunk_index: 0,
                plaintext_consumed: 0,
                pending: Vec::with_capacity(ATTACHMENT_CHUNK_SIZE),
                length_prefix_emitted: false,
            },
            header_bytes,
        ))
    }

    pub fn header(&self) -> &StreamHeader {
        &self.header
    }

    fn ensure_length_prefix(&mut self) {
        if !self.length_prefix_emitted {
            self.pending.extend_from_slice(&self.length_prefix_pending);
            self.length_prefix_emitted = true;
        }
    }

    fn emit_chunk(&mut self, chunk_bytes: &[u8]) -> Result<Vec<u8>> {
        debug_assert_eq!(chunk_bytes.len(), ATTACHMENT_CHUNK_SIZE);
        if self.chunk_index >= self.header.total_chunks {
            return Err(Error::Internal(format!(
                "attachment encrypt: chunk index {} >= total_chunks {}",
                self.chunk_index, self.header.total_chunks
            )));
        }
        let is_final = self.chunk_index + 1 == self.header.total_chunks;
        let nonce = chunk_nonce(&self.header.base_nonce_prefix, self.chunk_index);
        let aad = chunk_aad(&self.header_bytes, self.chunk_index, is_final);
        let ct = aead::seal(&self.key, &nonce, &aad, chunk_bytes)?;
        self.chunk_index = self
            .chunk_index
            .checked_add(1)
            .ok_or_else(|| Error::Internal("attachment encrypt: chunk index overflow".into()))?;
        Ok(ct)
    }

    /// Feed `plaintext` bytes. Returns the concatenated ciphertext for
    /// any complete chunks emitted by this call. May return an empty
    /// vector if the call did not fill a chunk.
    pub fn write(&mut self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let new_consumed = self
            .plaintext_consumed
            .saturating_add(plaintext.len() as u64);
        if new_consumed > self.header.plaintext_len {
            return Err(Error::Internal(format!(
                "attachment encrypt: write({}) past declared plaintext_len {} (already consumed {})",
                plaintext.len(),
                self.header.plaintext_len,
                self.plaintext_consumed
            )));
        }
        self.plaintext_consumed = new_consumed;

        let mut out = Vec::new();
        self.ensure_length_prefix();

        let mut input = plaintext;
        while !input.is_empty() {
            let space = ATTACHMENT_CHUNK_SIZE - self.pending.len();
            let take = std::cmp::min(space, input.len());
            self.pending.extend_from_slice(&input[..take]);
            input = &input[take..];
            if self.pending.len() == ATTACHMENT_CHUNK_SIZE {
                let chunk =
                    std::mem::replace(&mut self.pending, Vec::with_capacity(ATTACHMENT_CHUNK_SIZE));
                let ct = self.emit_chunk(&chunk)?;
                out.extend_from_slice(&ct);
            }
        }
        Ok(out)
    }

    /// Flush remaining plaintext (zero-padded to the bucket boundary)
    /// and emit all remaining chunks. Errors if fewer plaintext bytes
    /// were fed via `write` than the declared `plaintext_len`.
    pub fn finalize(mut self) -> Result<Vec<u8>> {
        if self.plaintext_consumed != self.header.plaintext_len {
            return Err(Error::Internal(format!(
                "attachment encrypt: finalize with consumed {} != declared plaintext_len {}",
                self.plaintext_consumed, self.header.plaintext_len
            )));
        }
        self.ensure_length_prefix();

        let mut out = Vec::new();
        // Flush whatever's in `pending`, padded with zeros.
        if !self.pending.is_empty() {
            self.pending.resize(ATTACHMENT_CHUNK_SIZE, 0);
            let chunk = std::mem::replace(&mut self.pending, Vec::new());
            let ct = self.emit_chunk(&chunk)?;
            out.extend_from_slice(&ct);
        }
        // Emit any remaining all-zero chunks up to bucket size.
        let zero_chunk = vec![0u8; ATTACHMENT_CHUNK_SIZE];
        while self.chunk_index < self.header.total_chunks {
            let ct = self.emit_chunk(&zero_chunk)?;
            out.extend_from_slice(&ct);
        }
        if self.chunk_index != self.header.total_chunks {
            return Err(Error::Internal(format!(
                "attachment encrypt: finalize with chunk_index {} != total_chunks {}",
                self.chunk_index, self.header.total_chunks
            )));
        }
        Ok(out)
    }
}

/// Streaming attachment decryptor. Counterpart to [`StreamEncryptor`].
///
/// Usage:
/// 1. [`StreamDecryptor::new`] — feed the wire bytes; the header is
///    parsed and the decryptor is returned along with the number of
///    header bytes consumed.
/// 2. [`StreamDecryptor::write`] — feed ciphertext bytes; emits the
///    plaintext for any complete chunks decrypted (with the length
///    prefix consumed and trailing padding stripped on the final
///    chunk).
/// 3. [`StreamDecryptor::finalize`] — confirms the expected total
///    chunk count was consumed.
pub struct StreamDecryptor {
    key: aead::Key,
    header: StreamHeader,
    header_bytes: Vec<u8>,
    chunk_index: u32,
    pending: Vec<u8>,
    plaintext_emitted: u64,
    length_prefix_consumed: bool,
}

const PER_CHUNK_CIPHERTEXT_SIZE: usize = ATTACHMENT_CHUNK_SIZE + aead::TAG_SIZE;

impl StreamDecryptor {
    /// Parse the stream header from the start of `bytes` and build a
    /// decryptor. Returns the decryptor and the number of bytes
    /// consumed for the header (the caller passes the remaining bytes
    /// — the chunked ciphertext — to [`Self::write`]).
    pub fn new(key: aead::Key, bytes: &[u8]) -> Result<(Self, usize)> {
        let (header, consumed) = StreamHeader::deserialize(bytes)?;
        let header_bytes = header.serialize();
        debug_assert_eq!(header_bytes.len(), consumed);
        Ok((
            StreamDecryptor {
                key,
                header,
                header_bytes,
                chunk_index: 0,
                pending: Vec::with_capacity(PER_CHUNK_CIPHERTEXT_SIZE),
                plaintext_emitted: 0,
                length_prefix_consumed: false,
            },
            consumed,
        ))
    }

    pub fn header(&self) -> &StreamHeader {
        &self.header
    }

    fn decrypt_chunk(&mut self, ct: &[u8]) -> Result<Vec<u8>> {
        debug_assert_eq!(ct.len(), PER_CHUNK_CIPHERTEXT_SIZE);
        if self.chunk_index >= self.header.total_chunks {
            return Err(Error::Internal(format!(
                "attachment decrypt: chunk index {} >= total_chunks {}",
                self.chunk_index, self.header.total_chunks
            )));
        }
        let is_final = self.chunk_index + 1 == self.header.total_chunks;
        let nonce = chunk_nonce(&self.header.base_nonce_prefix, self.chunk_index);
        let aad = chunk_aad(&self.header_bytes, self.chunk_index, is_final);
        let pt = aead::open(&self.key, &nonce, &aad, ct)?;
        self.chunk_index = self
            .chunk_index
            .checked_add(1)
            .ok_or_else(|| Error::Internal("attachment decrypt: chunk index overflow".into()))?;
        Ok(pt)
    }

    /// Strip the leading length prefix on first call; emit only as
    /// many user plaintext bytes as `plaintext_len` declares; discard
    /// the rest as authenticated zero-padding.
    fn user_bytes_from_chunk(&mut self, mut chunk: Vec<u8>) -> Result<Vec<u8>> {
        if !self.length_prefix_consumed {
            if chunk.len() < LENGTH_PREFIX_SIZE {
                return Err(Error::Internal(
                    "attachment decrypt: chunk shorter than length prefix (impossible if header validated)"
                        .into(),
                ));
            }
            let mut len_bytes = [0u8; LENGTH_PREFIX_SIZE];
            len_bytes.copy_from_slice(&chunk[..LENGTH_PREFIX_SIZE]);
            let claimed = u64::from_be_bytes(len_bytes);
            if claimed != self.header.plaintext_len {
                return Err(Error::Internal(format!(
                    "attachment decrypt: in-stream length {} disagrees with header.plaintext_len {}",
                    claimed, self.header.plaintext_len
                )));
            }
            chunk.drain(..LENGTH_PREFIX_SIZE);
            self.length_prefix_consumed = true;
        }
        let want_more = self.header.plaintext_len - self.plaintext_emitted;
        let take = std::cmp::min(chunk.len() as u64, want_more) as usize;
        let user = chunk[..take].to_vec();
        // chunk[take..] is authenticated zero-padding; drop it.
        self.plaintext_emitted += take as u64;
        Ok(user)
    }

    /// Feed `ciphertext` bytes. Returns the user plaintext for any
    /// chunks fully decrypted by this call. Trailing bytes shorter than
    /// one chunk are buffered.
    pub fn write(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        let mut input = ciphertext;
        while !input.is_empty() {
            let space = PER_CHUNK_CIPHERTEXT_SIZE - self.pending.len();
            let take = std::cmp::min(space, input.len());
            self.pending.extend_from_slice(&input[..take]);
            input = &input[take..];
            if self.pending.len() == PER_CHUNK_CIPHERTEXT_SIZE {
                let chunk_ct = std::mem::replace(
                    &mut self.pending,
                    Vec::with_capacity(PER_CHUNK_CIPHERTEXT_SIZE),
                );
                let pt = self.decrypt_chunk(&chunk_ct)?;
                let user = self.user_bytes_from_chunk(pt)?;
                out.extend_from_slice(&user);
            }
        }
        Ok(out)
    }

    /// Confirm all chunks were consumed. Errors if the input was
    /// truncated mid-chunk or if the wrong number of chunks was fed.
    pub fn finalize(self) -> Result<()> {
        if !self.pending.is_empty() {
            return Err(Error::Internal(format!(
                "attachment decrypt: trailing partial chunk ({} bytes)",
                self.pending.len()
            )));
        }
        if self.chunk_index != self.header.total_chunks {
            return Err(Error::Internal(format!(
                "attachment decrypt: only {} of {} chunks consumed",
                self.chunk_index, self.header.total_chunks
            )));
        }
        if self.plaintext_emitted != self.header.plaintext_len {
            return Err(Error::Internal(format!(
                "attachment decrypt: emitted {} of declared {} plaintext bytes",
                self.plaintext_emitted, self.header.plaintext_len
            )));
        }
        Ok(())
    }
}

/// Convenience whole-buffer encrypt — internally chunked, never holds
/// more than [`ATTACHMENT_CHUNK_SIZE`] bytes of plaintext at a time.
/// Returns the full wire bytes (header || all chunk ciphertexts).
pub fn encrypt_attachment(
    key: aead::Key,
    plaintext: &[u8],
    content_id: Vec<u8>,
    attachment_index: u32,
) -> Result<Vec<u8>> {
    let (mut enc, header_bytes) =
        StreamEncryptor::new(key, plaintext.len() as u64, content_id, attachment_index)?;
    let mut out = header_bytes;
    let mut off = 0;
    while off < plaintext.len() {
        let take = std::cmp::min(ATTACHMENT_CHUNK_SIZE, plaintext.len() - off);
        let ct = enc.write(&plaintext[off..off + take])?;
        out.extend_from_slice(&ct);
        off += take;
    }
    let tail = enc.finalize()?;
    out.extend_from_slice(&tail);
    Ok(out)
}

/// Convenience whole-buffer decrypt — counterpart to
/// [`encrypt_attachment`]. Returns the original plaintext.
pub fn decrypt_attachment(key: aead::Key, wire: &[u8]) -> Result<Vec<u8>> {
    let (mut dec, header_consumed) = StreamDecryptor::new(key, wire)?;
    let mut out = Vec::with_capacity(dec.header().plaintext_len as usize);
    let body = &wire[header_consumed..];
    // Stream the body through the decryptor in 16 KB ciphertext chunks
    // so we honour the streaming-window invariant even via the
    // convenience API.
    let mut off = 0;
    while off < body.len() {
        let take = std::cmp::min(PER_CHUNK_CIPHERTEXT_SIZE, body.len() - off);
        let pt = dec.write(&body[off..off + take])?;
        out.extend_from_slice(&pt);
        off += take;
    }
    dec.finalize()?;
    Ok(out)
}
