//! Phase 9-B1 Task 3: Mode 1 chunking with an authenticated header.
//!
//! Each Mode 1 cover message carries a single chunk produced by
//! splitting the v=3 / v=4 / v=5 wire bytes into pieces no larger
//! than [`CHUNK_PAYLOAD_BYTES`] (86 bytes after the 9-B1a re-tune).
//! Typical chunk counts: v=4 (~1.3 KB wire) → ~16 covers, v=3
//! (~1.25 KB wire) → ~15 covers, v=5 (~130 B wire) → 2 covers.
//! Every chunk is prefixed by a [`ChunkHeader`] of length
//! [`CHUNK_HEADER_BYTES`] (14 bytes):
//!
//! ```text
//! +--------------+-----+-------+-----------+
//! | session_id   | idx | total | hmac8     |
//! | 4 BE bytes   | 1 B | 1 B   | 8 bytes   |
//! +--------------+-----+-------+-----------+
//! ```
//!
//! - `session_id` is randomised per outbound wire (the same value
//!   appears on every chunk of one logical message; the reassembly
//!   buffer groups by it).
//! - `idx` is the zero-indexed chunk position (`< total`).
//! - `total` is the number of chunks in the session (1..=255).
//! - `hmac8` is the leading 8 bytes of
//!   `HMAC-SHA256(chunk_key, session_id || idx || total || payload)`,
//!   where `chunk_key` is derived from the conversation salt via
//!   HKDF over [`CHUNK_HMAC_DOMAIN`].
//!
//! The HMAC binds the metadata to the payload and to the conversation
//! salt, so a man-in-the-middle without the salt cannot rearrange
//! chunks, change the announced `total`, or splice payload bytes
//! between sessions. The 8-byte truncation is the standard RFC 2104
//! posture for short-MAC use.

use crate::ConversationCipher;
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Bytes consumed by one chunk header (4 + 1 + 1 + 8 = 14).
pub const CHUNK_HEADER_BYTES: usize = 14;

/// Bytes available for payload per chunk after the header.
/// 9-B1a: `MODE1_MAX_RAW_LEN (100) - CHUNK_HEADER_BYTES (14) = 86`.
pub const CHUNK_PAYLOAD_BYTES: usize = crate::MODE1_MAX_RAW_LEN - CHUNK_HEADER_BYTES;

/// HKDF info string for the chunk-HMAC key derivation. Same salt
/// produces the same key on encode and decode sides.
pub const CHUNK_HMAC_DOMAIN: &[u8] = b"discord-privacy-client/stego-mode1/chunk-hmac/v1";

/// Maximum chunks one session can carry (the `total` byte is `u8`).
/// Wire payloads never approach this — v=4 at 1350 bytes is ~12
/// chunks — but the cap is enforced anyway to keep the header
/// validation tight.
pub const CHUNK_MAX_TOTAL: u8 = 255;

#[derive(Debug, thiserror::Error)]
pub enum ChunkError {
    #[error("chunk header truncated: need {need} bytes, got {got}")]
    HeaderTruncated { need: usize, got: usize },

    #[error("declared total is zero — every session has at least one chunk")]
    TotalIsZero,

    #[error("chunk index {idx} is not strictly less than total {total}")]
    IndexOutOfRange { idx: u8, total: u8 },

    #[error("chunk payload exceeds per-chunk cap of {max} bytes (got {got})")]
    PayloadOverflow { got: usize, max: usize },

    #[error("HMAC mismatch — chunk header or payload was tampered with")]
    HmacMismatch,
}

/// Parsed view of one chunk's header + payload. Produced by
/// [`parse_chunk`]; consumed by the reassembly buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedChunk {
    pub session_id: u32,
    pub chunk_index: u8,
    pub total_chunks: u8,
    pub payload: Vec<u8>,
}

/// One chunk's serialized bytes — what gets handed to `encode_mode1`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerializedChunk {
    pub session_id: u32,
    pub chunk_index: u8,
    pub total_chunks: u8,
    /// Header || payload, ready to feed to `encode_mode1`.
    pub bytes: Vec<u8>,
}

/// Derive the per-conversation chunk-HMAC key from the cipher's salt.
fn derive_chunk_hmac_key(salt: &[u8]) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(Some(CHUNK_HMAC_DOMAIN), salt);
    let mut out = [0u8; 32];
    hk.expand(b"chunk-hmac-key", &mut out)
        .expect("hkdf expand 32 bytes");
    out
}

fn compute_hmac_tag(
    key: &[u8; 32],
    session_id: u32,
    chunk_index: u8,
    total_chunks: u8,
    payload: &[u8],
) -> [u8; 8] {
    let mut mac = HmacSha256::new_from_slice(key).expect("hmac key");
    mac.update(&session_id.to_be_bytes());
    mac.update(&[chunk_index, total_chunks]);
    mac.update(payload);
    let full = mac.finalize().into_bytes();
    let mut tag = [0u8; 8];
    tag.copy_from_slice(&full[..8]);
    tag
}

/// Split `wire_bytes` into chunks sized for Mode 1 carriage.
///
/// Caller supplies `session_id` (typically a random `u32`) so the
/// receive-side reassembly buffer can correlate this session's
/// chunks. Returns at least one chunk; an empty `wire_bytes` is
/// still a valid one-chunk session (carries a zero-length payload).
///
/// Note this never errors at the protocol layer: oversized inputs
/// (more than `CHUNK_MAX_TOTAL * CHUNK_PAYLOAD_BYTES = 29 070` bytes)
/// hit a panic, which is fine — every wire format we ship stays
/// well below that, and crossing it would be a bug not a runtime
/// condition.
pub fn chunk_payload(salt: &[u8], session_id: u32, wire_bytes: &[u8]) -> Vec<SerializedChunk> {
    let key = derive_chunk_hmac_key(salt);
    let total = wire_bytes.len().div_ceil(CHUNK_PAYLOAD_BYTES).max(1);
    assert!(
        total <= CHUNK_MAX_TOTAL as usize,
        "chunk_payload: wire input {} bytes overflows {} chunks",
        wire_bytes.len(),
        CHUNK_MAX_TOTAL
    );
    let total_u8 = total as u8;

    let mut out = Vec::with_capacity(total);
    for (idx, slice) in wire_bytes.chunks(CHUNK_PAYLOAD_BYTES).enumerate() {
        let chunk_index = idx as u8;
        let payload: &[u8] = slice;
        let tag = compute_hmac_tag(&key, session_id, chunk_index, total_u8, payload);

        let mut bytes = Vec::with_capacity(CHUNK_HEADER_BYTES + payload.len());
        bytes.extend_from_slice(&session_id.to_be_bytes());
        bytes.push(chunk_index);
        bytes.push(total_u8);
        bytes.extend_from_slice(&tag);
        bytes.extend_from_slice(payload);

        out.push(SerializedChunk {
            session_id,
            chunk_index,
            total_chunks: total_u8,
            bytes,
        });
    }

    // Edge case: empty input produced no slice, so we synthesize a
    // zero-payload chunk above. `chunks(N)` over `&[]` yields no
    // iterations, so out is empty here — push a single zero chunk.
    if out.is_empty() {
        let tag = compute_hmac_tag(&key, session_id, 0, 1, &[]);
        let mut bytes = Vec::with_capacity(CHUNK_HEADER_BYTES);
        bytes.extend_from_slice(&session_id.to_be_bytes());
        bytes.push(0);
        bytes.push(1);
        bytes.extend_from_slice(&tag);
        out.push(SerializedChunk {
            session_id,
            chunk_index: 0,
            total_chunks: 1,
            bytes,
        });
    }
    out
}

/// Convenience: same as [`chunk_payload`] but takes a
/// [`ConversationCipher`]-shaped salt by looking up the salt held
/// implicitly in the caller's environment. Implemented as a thin
/// wrapper so test code can pass either form.
pub fn chunk_payload_with_cipher(
    _cipher: &ConversationCipher,
    salt: &[u8],
    session_id: u32,
    wire_bytes: &[u8],
) -> Vec<SerializedChunk> {
    chunk_payload(salt, session_id, wire_bytes)
}

/// Parse one chunk's serialized bytes back into a [`ParsedChunk`].
/// Verifies the HMAC against the cipher's salt; any tamper attempt
/// surfaces as [`ChunkError::HmacMismatch`].
pub fn parse_chunk(salt: &[u8], bytes: &[u8]) -> Result<ParsedChunk, ChunkError> {
    if bytes.len() < CHUNK_HEADER_BYTES {
        return Err(ChunkError::HeaderTruncated {
            need: CHUNK_HEADER_BYTES,
            got: bytes.len(),
        });
    }
    let mut session_id_bytes = [0u8; 4];
    session_id_bytes.copy_from_slice(&bytes[0..4]);
    let session_id = u32::from_be_bytes(session_id_bytes);
    let chunk_index = bytes[4];
    let total_chunks = bytes[5];
    let mut tag = [0u8; 8];
    tag.copy_from_slice(&bytes[6..14]);
    let payload_bytes = &bytes[CHUNK_HEADER_BYTES..];

    if total_chunks == 0 {
        return Err(ChunkError::TotalIsZero);
    }
    if chunk_index >= total_chunks {
        return Err(ChunkError::IndexOutOfRange {
            idx: chunk_index,
            total: total_chunks,
        });
    }
    if payload_bytes.len() > CHUNK_PAYLOAD_BYTES {
        return Err(ChunkError::PayloadOverflow {
            got: payload_bytes.len(),
            max: CHUNK_PAYLOAD_BYTES,
        });
    }

    let key = derive_chunk_hmac_key(salt);
    let expected = compute_hmac_tag(&key, session_id, chunk_index, total_chunks, payload_bytes);
    // Constant-time compare.
    let mut diff: u8 = 0;
    for (a, b) in tag.iter().zip(expected.iter()) {
        diff |= a ^ b;
    }
    if diff != 0 {
        return Err(ChunkError::HmacMismatch);
    }

    Ok(ParsedChunk {
        session_id,
        chunk_index,
        total_chunks,
        payload: payload_bytes.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SALT: &[u8] = b"test-salt";

    #[test]
    fn chunk_payload_size_constants_match_spec() {
        assert_eq!(CHUNK_HEADER_BYTES, 14);
        assert_eq!(CHUNK_PAYLOAD_BYTES, 100 - 14);
        assert_eq!(CHUNK_PAYLOAD_BYTES, 86);
    }

    #[test]
    fn single_chunk_roundtrip_small_payload() {
        let chunks = chunk_payload(SALT, 0xDEADBEEF, b"hello");
        assert_eq!(chunks.len(), 1);
        let c0 = &chunks[0];
        assert_eq!(c0.session_id, 0xDEADBEEF);
        assert_eq!(c0.chunk_index, 0);
        assert_eq!(c0.total_chunks, 1);
        let parsed = parse_chunk(SALT, &c0.bytes).unwrap();
        assert_eq!(parsed.session_id, 0xDEADBEEF);
        assert_eq!(parsed.chunk_index, 0);
        assert_eq!(parsed.total_chunks, 1);
        assert_eq!(parsed.payload, b"hello");
    }

    #[test]
    fn empty_input_produces_one_zero_length_chunk() {
        let chunks = chunk_payload(SALT, 1, b"");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].total_chunks, 1);
        let parsed = parse_chunk(SALT, &chunks[0].bytes).unwrap();
        assert_eq!(parsed.payload, b"");
    }

    #[test]
    fn multi_chunk_split_for_oversized_wire() {
        // 300 bytes → ceil(300/86) = 4 chunks.
        let wire: Vec<u8> = (0..300u16).map(|i| (i & 0xFF) as u8).collect();
        let chunks = chunk_payload(SALT, 42, &wire);
        assert_eq!(chunks.len(), 4);
        assert_eq!(chunks[0].total_chunks, 4);
        for (i, c) in chunks.iter().enumerate() {
            assert_eq!(c.chunk_index as usize, i);
        }

        // Reassemble by parsing each.
        let mut got = Vec::new();
        for c in &chunks {
            let parsed = parse_chunk(SALT, &c.bytes).unwrap();
            got.extend_from_slice(&parsed.payload);
        }
        assert_eq!(got, wire);
    }

    #[test]
    fn hmac_tamper_in_payload_is_rejected() {
        let chunks = chunk_payload(SALT, 7, b"hello world");
        let mut tampered = chunks[0].bytes.clone();
        // Flip a byte in payload.
        let pi = CHUNK_HEADER_BYTES + 2;
        tampered[pi] ^= 0xFF;
        match parse_chunk(SALT, &tampered) {
            Err(ChunkError::HmacMismatch) => {}
            other => panic!("expected HmacMismatch, got {other:?}"),
        }
    }

    #[test]
    fn hmac_tamper_in_header_index_is_rejected() {
        // Multi-chunk session so chunk_index can be bumped to another
        // valid range value (9 < total=2 would fail range check first;
        // we want an index where HMAC is the rejecting layer).
        let wire = vec![0xABu8; 1500];
        let chunks = chunk_payload(SALT, 7, &wire);
        assert!(chunks[0].total_chunks > 4);
        let mut tampered = chunks[0].bytes.clone();
        tampered[4] = 3; // chunk_index byte — within range, wrong value
        match parse_chunk(SALT, &tampered) {
            Err(ChunkError::HmacMismatch) => {}
            other => panic!("expected HmacMismatch, got {other:?}"),
        }
    }

    #[test]
    fn wrong_salt_rejects_parse() {
        let chunks = chunk_payload(b"salt-A", 7, b"hello");
        match parse_chunk(b"salt-B", &chunks[0].bytes) {
            Err(ChunkError::HmacMismatch) => {}
            other => panic!("expected HmacMismatch, got {other:?}"),
        }
    }

    #[test]
    fn truncated_input_below_header_rejects() {
        let chunks = chunk_payload(SALT, 7, b"hello");
        let trunc = &chunks[0].bytes[..10]; // less than 14
        match parse_chunk(SALT, trunc) {
            Err(ChunkError::HeaderTruncated { need, got }) => {
                assert_eq!(need, 14);
                assert_eq!(got, 10);
            }
            other => panic!("expected HeaderTruncated, got {other:?}"),
        }
    }

    #[test]
    fn total_zero_rejected() {
        // Build a header by hand with total = 0; HMAC over that
        // combination is unknown to us so we fail at total-check
        // before HMAC.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&0u32.to_be_bytes());
        bytes.push(0); // idx
        bytes.push(0); // total = invalid
        bytes.extend_from_slice(&[0u8; 8]);
        match parse_chunk(SALT, &bytes) {
            Err(ChunkError::TotalIsZero) => {}
            other => panic!("expected TotalIsZero, got {other:?}"),
        }
    }

    #[test]
    fn each_chunk_fits_under_mode1_raw_cap() {
        let wire = vec![0xCDu8; 1500];
        let chunks = chunk_payload(SALT, 0xFEED, &wire);
        for c in &chunks {
            assert!(
                c.bytes.len() <= crate::MODE1_MAX_RAW_LEN,
                "chunk {} is {} bytes, exceeds MODE1_MAX_RAW_LEN {}",
                c.chunk_index,
                c.bytes.len(),
                crate::MODE1_MAX_RAW_LEN
            );
        }
    }

    #[test]
    fn chunks_produce_output_under_2000_chars() {
        // 9-B1a: every chunk's encode_mode1 cover must stay under
        // Discord's free-tier 2000-char message cap. Split a 1KB
        // wire, encode each chunk, and confirm the bound holds.
        let wire = vec![0x5Au8; 1024];
        let chunks = chunk_payload(SALT, 0xC0DE, &wire);
        let cipher = crate::ConversationCipher::from_salt(SALT);
        assert!(chunks.len() >= 12, "1KB at 86B/chunk should chunk to ≥12");
        for c in &chunks {
            let cover = crate::encode_mode1(&cipher, &c.bytes).expect("encode_mode1");
            assert!(
                cover.chars().count() < 2000,
                "chunk {} encoded to {} chars (>= 2000)",
                c.chunk_index,
                cover.chars().count()
            );
        }
    }
}
