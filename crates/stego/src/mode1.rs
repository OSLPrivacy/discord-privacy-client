//! Mode 1: template-based fluent stego.
//!
//! See the crate-level docs for the per-message-independence
//! invariant. Mode 1 satisfies it: every encoded message is decoded
//! purely from `(message, conversation_salt)` with no reference to
//! prior messages.
//!
//! ## Wire format
//!
//! ```text
//! DPC1::<rendered cover sentences>
//! ```
//!
//! Each sentence carries [`mode1_templates::BITS_PER_SENTENCE`] bits
//! of payload. The bit stream begins with a 16-bit big-endian
//! `payload_len_bytes` header — when the decoder has read that many
//! payload bytes plus the 16-bit header, it stops. Trailing bits in
//! the final sentence are unconstrained; the encoder fills them with
//! zeros for stable test output.
//!
//! ## Per-conversation salt
//!
//! `conversation_salt` is an opaque byte string the caller derives
//! from the session (e.g. via HKDF over the ratchet root key). It
//! deterministically permutes the wordlists and the template pool so
//! two different conversations encoding the same ciphertext produce
//! different surface phrasings — purely for fluency, not security
//! (the AEAD provides confidentiality; this layer is a delivery
//! channel).
//!
//! ## Length cap
//!
//! Discord caps free-tier text messages at 2000 chars. Each sentence
//! emits ~25-50 chars; with the 9-B1a cap of 100 raw bytes we emit
//! at most `(100*8 + 16)/20 = 41` sentences ≈ 1.7 KB of cover text,
//! comfortably under Discord's free-tier limit. After the 14-byte
//! chunk header (see [`crate::mode1_chunking`]), this leaves
//! 86 bytes of wire-payload per chunk: a 1.3 KB v=4 wire chunks to
//! ~16 covers, a 1.25 KB v=3 wire to ~15, and a 130 B v=5 wire to
//! 2 covers. [`MODE1_MAX_RAW_LEN`] is 100 bytes total.

use crate::mode1_templates::{
    SlotKind, Template, BITS_PER_SENTENCE, SLOT_BITS, TEMPLATES, TEMPLATES_LEN, TEMPLATE_BITS,
    TOTAL_SLOTS,
};
use crate::mode1_wordlists::{ADJECTIVES, ADJ_COUNT, NOUNS, NOUN_COUNT};
use crate::{Error, Result};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2::Sha256;

/// Mode 1 magic prefix. `DPC1::` echoes the Mode 0 convention.
pub const MODE1_PREFIX: &str = "DPC1::";

/// Maximum raw payload bytes (see module docs). Exceeding produces
/// [`Error::Mode1TooLong`]; callers must split across messages
/// (see [`crate::mode1_chunking`]).
///
/// 9-B1 bumped this from 80 to 128, then 9-B1a re-tuned to 100 so
/// the encoded cover stays under Discord's free-tier 2000-char text
/// limit. After the 14-byte chunk header the per-chunk wire payload
/// is 86 bytes.
pub const MODE1_MAX_RAW_LEN: usize = 100;

/// Domain separator for the HKDF that derives wordlist permutations.
pub const PERMUTATION_DOMAIN: &[u8] = b"discord-privacy-client/stego-mode1/permutation/v1";

/// Immutable per-conversation permutation state.
///
/// Building this is the only nontrivial work in the
/// encode / decode path; callers should construct one
/// [`ConversationCipher`] per session and reuse it across messages.
pub struct ConversationCipher {
    template_perm: [u8; TEMPLATES_LEN],
    template_inv: [u8; TEMPLATES_LEN],
    noun_perm: [u8; NOUN_COUNT],
    noun_inv: [u8; NOUN_COUNT],
    adj_perm: [u8; ADJ_COUNT],
    adj_inv: [u8; ADJ_COUNT],
}

impl ConversationCipher {
    /// Derive per-conversation permutations from the salt. Tests
    /// confirm two ciphers built from the same salt produce
    /// identical output.
    pub fn from_salt(conversation_salt: &[u8]) -> Self {
        let template_perm = derive_permutation::<TEMPLATES_LEN>(conversation_salt, b"templates");
        let noun_perm = derive_permutation::<NOUN_COUNT>(conversation_salt, b"nouns");
        let adj_perm = derive_permutation::<ADJ_COUNT>(conversation_salt, b"adjectives");

        ConversationCipher {
            template_inv: invert(&template_perm),
            template_perm,
            noun_inv: invert(&noun_perm),
            noun_perm,
            adj_inv: invert(&adj_perm),
            adj_perm,
        }
    }

    fn template_for_index(&self, raw_idx: u8) -> usize {
        self.template_perm[raw_idx as usize] as usize
    }

    fn raw_index_for_template(&self, t_idx: usize) -> u8 {
        self.template_inv[t_idx]
    }

    fn slot_word(&self, kind: SlotKind, raw_idx: u8) -> &'static str {
        match kind {
            SlotKind::Noun => NOUNS[self.noun_perm[raw_idx as usize] as usize],
            SlotKind::Adj => ADJECTIVES[self.adj_perm[raw_idx as usize] as usize],
        }
    }

    fn raw_index_for_slot(&self, kind: SlotKind, word: &str) -> Option<u8> {
        match kind {
            SlotKind::Noun => {
                let pos = NOUNS.iter().position(|w| *w == word)?;
                Some(self.noun_inv[pos])
            }
            SlotKind::Adj => {
                let pos = ADJECTIVES.iter().position(|w| *w == word)?;
                Some(self.adj_inv[pos])
            }
        }
    }
}

/// Encode raw ciphertext bytes as a Mode 1 cover-text message.
pub fn encode_mode1(cipher: &ConversationCipher, ciphertext: &[u8]) -> Result<String> {
    if ciphertext.len() > MODE1_MAX_RAW_LEN {
        return Err(Error::Mode1TooLong {
            got: ciphertext.len(),
            max: MODE1_MAX_RAW_LEN,
        });
    }
    // Bit stream = u16 BE length prefix || ciphertext bytes.
    let mut bits = BitWriter::new();
    bits.write(ciphertext.len() as u32, 16);
    for byte in ciphertext {
        bits.write(*byte as u32, 8);
    }
    // Pad to whole-sentence boundary with zeros so the decoder reads
    // a complete final sentence.
    let total_payload_bits = 16 + (ciphertext.len() * 8) as u32;
    let pad_to = total_payload_bits.div_ceil(BITS_PER_SENTENCE) * BITS_PER_SENTENCE;
    let pad = pad_to - total_payload_bits;
    if pad > 0 {
        bits.write(0, pad);
    }
    let bit_buf = bits.finish();

    let mut out = String::from(MODE1_PREFIX);
    let mut reader = BitReader::new(&bit_buf);
    let mut first = true;
    while reader.bits_remaining() >= BITS_PER_SENTENCE {
        let t_raw = reader
            .read(TEMPLATE_BITS)
            .expect("just-checked remaining bits") as u8;
        let t_idx = cipher.template_for_index(t_raw);
        let template = &TEMPLATES[t_idx];
        let mut slot_idxs: [u8; TOTAL_SLOTS] = [0u8; TOTAL_SLOTS];
        for (i, kind) in template.slots.iter().enumerate() {
            let raw = reader.read(SLOT_BITS).expect("just-checked remaining bits") as u8;
            slot_idxs[i] = raw;
            let _ = kind;
        }

        if !first {
            out.push(' ');
        }
        render_sentence(&mut out, template, &slot_idxs, cipher);
        first = false;
    }

    Ok(out)
}

/// Cheap detection: does this message carry a Mode 1 prefix?
pub fn is_mode1(msg: &str) -> bool {
    msg.starts_with(MODE1_PREFIX)
}

/// Decode a Mode 1 cover-text message back to raw ciphertext bytes.
pub fn decode_mode1(cipher: &ConversationCipher, msg: &str) -> Result<Vec<u8>> {
    let body = msg.strip_prefix(MODE1_PREFIX).ok_or(Error::NotMode1)?;
    let body = body.trim();
    if body.is_empty() {
        return Err(Error::Mode1ParseError("empty body after prefix".into()));
    }

    // Walk the body word-by-word. For each candidate sentence start,
    // try matching every template's skeleton; the first that
    // matches consumes its tokens and emits its bits.
    let tokens: Vec<&str> = body.split_whitespace().collect();
    let mut t_cursor = 0usize;

    let mut writer = BitWriter::new();
    while t_cursor < tokens.len() {
        let consumed = match_one_sentence(&tokens[t_cursor..], cipher, &mut writer)?;
        if consumed == 0 {
            return Err(Error::Mode1ParseError(format!(
                "no template matches starting at token {:?}",
                tokens[t_cursor]
            )));
        }
        t_cursor += consumed;
    }

    // Recover length prefix and payload.
    let bit_buf = writer.finish();
    let mut reader = BitReader::new(&bit_buf);
    let payload_len = reader
        .read(16)
        .ok_or_else(|| Error::Mode1ParseError("no length prefix bits".into()))?
        as usize;
    if payload_len > MODE1_MAX_RAW_LEN {
        return Err(Error::Mode1ParseError(format!(
            "decoded length prefix {payload_len} exceeds cap {MODE1_MAX_RAW_LEN}"
        )));
    }
    let payload_bits = payload_len * 8;
    if reader.bits_remaining() < payload_bits as u32 {
        return Err(Error::Mode1ParseError(format!(
            "claimed payload of {payload_bits} bits exceeds {} bits available",
            reader.bits_remaining()
        )));
    }
    let mut out = Vec::with_capacity(payload_len);
    for _ in 0..payload_len {
        out.push(reader.read(8).expect("bits available") as u8);
    }
    Ok(out)
}

// ==========================================================================
// Prose-token encoding (Phase 2 of the cipher-store pivot).
//
// Encodes an 8-byte cipher-store ID as chat-like cover text with:
//   * NO magic prefix (so Discord content scanners can't pattern-match);
//   * a 4-byte HMAC-SHA256 tag appended to the ID so receivers can
//     distinguish OSL tokens from coincidentally template-matching English
//     (false-positive rate 1 in 2^32 — effectively zero across realistic
//     volumes).
//
// Payload layout: [id (8 B)] [hmac_tag (4 B)] = 12 bytes = 96 bits.
// The current bigram arithmetic codec emits a variable-length word stream;
// the legacy five-template-sentence decoder remains for rollout compatibility.
// ==========================================================================

/// ID bytes carried per prose token. 64 bits is collision-safe for the
/// cipher-store's TTL window (max a few thousand active blobs).
pub const TOKEN_ID_BYTES: usize = 8;

/// HMAC tag bytes appended for receiver-side token detection. This is
/// not sender authentication: the current caller derives the key from
/// public scope metadata.
pub const TOKEN_MAC_BYTES: usize = 4;

/// Total bits in the encoded payload (ID || tag).
pub const TOKEN_PAYLOAD_BITS: u32 = (TOKEN_ID_BYTES as u32 + TOKEN_MAC_BYTES as u32) * 8;

/// Domain separator for the HMAC over the prose-token ID.
pub const TOKEN_MAC_DOMAIN: &[u8] = b"discord-privacy-client/mode1-token/v1";

type HmacSha256 = Hmac<Sha256>;

fn compute_token_tag(mac_key: &[u8], id: &[u8; TOKEN_ID_BYTES]) -> [u8; TOKEN_MAC_BYTES] {
    let mut mac = HmacSha256::new_from_slice(mac_key).expect("HMAC accepts any key length");
    mac.update(TOKEN_MAC_DOMAIN);
    mac.update(id);
    let full = mac.finalize().into_bytes();
    let mut tag = [0u8; TOKEN_MAC_BYTES];
    tag.copy_from_slice(&full[..TOKEN_MAC_BYTES]);
    tag
}

fn token_payload_bits(id: &[u8; TOKEN_ID_BYTES], tag: &[u8; TOKEN_MAC_BYTES]) -> Vec<bool> {
    let mut bits = Vec::with_capacity(TOKEN_PAYLOAD_BITS as usize);
    for &b in id.iter().chain(tag.iter()) {
        for i in (0..8).rev() {
            bits.push((b >> i) & 1 == 1);
        }
    }
    bits
}

/// Encode an 8-byte cipher-store ID as marker-free, chat-like cover text.
/// The small embedded bigram model is not claimed to be statistically
/// indistinguishable from human chat. The 4-byte HMAC tag appended to the
/// ID lets the recipient distinguish an OSL token from ordinary text that
/// happens to use only words in the model vocabulary.
pub fn encode_token(
    _cipher: &ConversationCipher,
    mac_key: &[u8],
    id: &[u8; TOKEN_ID_BYTES],
) -> String {
    // Bigram pivot: the prose cover is now generated by the n=2
    // language-model arithmetic codec (see `crate::bigram`) instead
    // of the 16-template / 2-slot scheme. The payload (id || HMAC
    // tag = 96 bits) is unchanged; only the bit→prose mapping
    // differs. `_cipher` is retained in the signature for API
    // compatibility but is unused — the bigram model is global and
    // the per-conversation distinction is carried by the HMAC tag.
    let tag = compute_token_tag(mac_key, id);
    let bits = token_payload_bits(id, &tag);
    let words = crate::bigram::arithmetic_decode_bits(&bits, TOKEN_PAYLOAD_BITS);
    crate::bigram::render_words(&words)
}

/// Try to decode a Discord message as an OSL prose-token. Returns the
/// 8-byte ID iff the recovered HMAC tag verifies under `mac_key`.
/// Returns None on any failure — safe to call on every incoming
/// Discord message in an OSL-enabled scope.
///
/// Tries the bigram codec first (current format); on a parse miss or
/// HMAC mismatch, falls back to the legacy template decoder so covers
/// posted by pre-pivot peers still decode during the rollout window.
pub fn decode_token(
    cipher: &ConversationCipher,
    mac_key: &[u8],
    msg: &str,
) -> Option<[u8; TOKEN_ID_BYTES]> {
    if let Some(id) = decode_token_bigram(mac_key, msg) {
        return Some(id);
    }
    decode_token_template(cipher, mac_key, msg)
}

/// Bigram-codec decode path. Parses the message into vocab indices,
/// re-encodes them to the 96-bit payload, splits id || tag, and
/// verifies the HMAC.
fn decode_token_bigram(mac_key: &[u8], msg: &str) -> Option<[u8; TOKEN_ID_BYTES]> {
    let words = crate::bigram::parse_words(msg)?;
    let bits = crate::bigram::arithmetic_encode_words(&words, TOKEN_PAYLOAD_BITS);
    if bits.len() < TOKEN_PAYLOAD_BITS as usize {
        return None;
    }
    let mut id = [0u8; TOKEN_ID_BYTES];
    for (byte_i, slot) in id.iter_mut().enumerate() {
        let mut v = 0u8;
        for bit_i in 0..8 {
            v = (v << 1) | bits[byte_i * 8 + bit_i] as u8;
        }
        *slot = v;
    }
    let mut tag = [0u8; TOKEN_MAC_BYTES];
    for (byte_i, slot) in tag.iter_mut().enumerate() {
        let mut v = 0u8;
        let base = (TOKEN_ID_BYTES + byte_i) * 8;
        for bit_i in 0..8 {
            v = (v << 1) | bits[base + bit_i] as u8;
        }
        *slot = v;
    }
    let expected = compute_token_tag(mac_key, &id);
    if !constant_time_eq_token(&tag, &expected) {
        return None;
    }

    // Arithmetic decoding stops as soon as the 96 payload bits are
    // pinned. Without this canonical-form check, appending additional
    // in-vocabulary words could leave those high bits unchanged and the
    // modified cover would still validate. Recreate the unique encoder
    // output and require the parsed word stream to match exactly.
    let canonical_bits = token_payload_bits(&id, &expected);
    let canonical_words =
        crate::bigram::arithmetic_decode_bits(&canonical_bits, TOKEN_PAYLOAD_BITS);
    if words != canonical_words {
        return None;
    }
    Some(id)
}

/// Legacy 16-template / 2-slot decode path. Retained so prose covers
/// posted before the bigram pivot still decode.
fn decode_token_template(
    cipher: &ConversationCipher,
    mac_key: &[u8],
    msg: &str,
) -> Option<[u8; TOKEN_ID_BYTES]> {
    let body = msg.trim();
    if body.is_empty() {
        return None;
    }
    let tokens: Vec<&str> = body.split_whitespace().collect();
    let mut t_cursor = 0usize;
    let mut writer = BitWriter::new();
    while t_cursor < tokens.len() {
        let consumed = match match_one_sentence(&tokens[t_cursor..], cipher, &mut writer) {
            Ok(n) => n,
            Err(_) => return None,
        };
        if consumed == 0 {
            return None;
        }
        t_cursor += consumed;
    }
    let bit_buf = writer.finish();
    if bit_buf.bit_count < TOKEN_PAYLOAD_BITS {
        return None;
    }
    let mut reader = BitReader::new(&bit_buf);
    let mut id = [0u8; TOKEN_ID_BYTES];
    for slot in id.iter_mut() {
        *slot = reader.read(8)? as u8;
    }
    let mut tag = [0u8; TOKEN_MAC_BYTES];
    for slot in tag.iter_mut() {
        *slot = reader.read(8)? as u8;
    }
    let expected = compute_token_tag(mac_key, &id);
    if !constant_time_eq_token(&tag, &expected) {
        return None;
    }
    Some(id)
}

fn constant_time_eq_token(a: &[u8; TOKEN_MAC_BYTES], b: &[u8; TOKEN_MAC_BYTES]) -> bool {
    let mut diff = 0u8;
    for i in 0..TOKEN_MAC_BYTES {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

/// Try to match the next sentence against every template under the
/// cipher's current permutation. On match, append the recovered bits
/// to `writer` and return how many tokens were consumed; on no
/// match, return 0.
fn match_one_sentence(
    tokens: &[&str],
    cipher: &ConversationCipher,
    writer: &mut BitWriter,
) -> Result<usize> {
    for (t_idx, template) in TEMPLATES.iter().enumerate() {
        if let Some((slot_words, consumed)) = match_template(template, tokens) {
            // Map template index back through the inverse permutation.
            let raw = cipher.raw_index_for_template(t_idx) as u32;
            writer.write(raw, TEMPLATE_BITS);
            for (kind, word) in template.slots.iter().zip(slot_words.iter()) {
                let slot_raw = cipher.raw_index_for_slot(*kind, word).ok_or_else(|| {
                    Error::Mode1ParseError(format!(
                        "decoder saw {kind:?} slot value {word:?} \
                             which is not in the wordlist"
                    ))
                })?;
                writer.write(slot_raw as u32, SLOT_BITS);
            }
            return Ok(consumed);
        }
    }
    Ok(0)
}

/// Walk the template skeleton against `tokens`, treating each
/// `SLOT_TOKEN` marker as "consume one token here". Returns
/// `(slot_words, tokens_consumed)` on full match; `None` otherwise.
fn match_template<'a>(template: &Template, tokens: &[&'a str]) -> Option<(Vec<&'a str>, usize)> {
    let mut t_cursor = 0usize;
    let mut s_cursor = 0usize;
    let mut slot_words = Vec::with_capacity(template.slots.len());
    while s_cursor < template.skeleton.len() {
        let expected = template.skeleton[s_cursor];
        if expected == crate::mode1_templates::SLOT_TOKEN {
            if t_cursor >= tokens.len() {
                return None;
            }
            slot_words.push(tokens[t_cursor]);
            t_cursor += 1;
            s_cursor += 1;
            continue;
        }
        // Compare ignoring trailing punctuation on either side. The
        // skeleton's punctuation is part of the fixed token; the
        // input token is exactly what `split_whitespace` produced.
        if t_cursor >= tokens.len() {
            return None;
        }
        if tokens[t_cursor] != expected {
            return None;
        }
        t_cursor += 1;
        s_cursor += 1;
    }
    Some((slot_words, t_cursor))
}

fn render_sentence(
    out: &mut String,
    template: &Template,
    slot_raws: &[u8],
    cipher: &ConversationCipher,
) {
    let mut slot_iter = template.slots.iter().zip(slot_raws.iter());
    let mut first_token = true;
    for token in template.skeleton {
        if !first_token {
            out.push(' ');
        }
        first_token = false;
        if *token == crate::mode1_templates::SLOT_TOKEN {
            let (kind, raw) = slot_iter
                .next()
                .expect("template slots line up with slot_raws by construction");
            out.push_str(cipher.slot_word(*kind, *raw));
        } else {
            out.push_str(token);
        }
    }
    debug_assert!(slot_iter.next().is_none(), "leftover slots");
}

// ---- bit accumulator helpers ----

/// MSB-first bit accumulator.
struct BitWriter {
    buf: Vec<u8>,
    /// Number of bits currently *valid* in the buffer (the low
    /// `8 - (bit_count % 8)` bits of the final byte are unused
    /// trailing zeros until more bits arrive).
    bit_count: u32,
}

impl BitWriter {
    fn new() -> Self {
        BitWriter {
            buf: Vec::new(),
            bit_count: 0,
        }
    }

    /// Append `n_bits` bits from `value` (LSB-aligned within
    /// `value`). MSB-first into the byte buffer.
    fn write(&mut self, value: u32, n_bits: u32) {
        debug_assert!(n_bits <= 32);
        for i in (0..n_bits).rev() {
            let bit = ((value >> i) & 1) as u8;
            let byte_idx = (self.bit_count / 8) as usize;
            let bit_in_byte = 7 - (self.bit_count % 8) as u8;
            if byte_idx == self.buf.len() {
                self.buf.push(0);
            }
            self.buf[byte_idx] |= bit << bit_in_byte;
            self.bit_count += 1;
        }
    }

    fn finish(self) -> BitBuffer {
        BitBuffer {
            buf: self.buf,
            bit_count: self.bit_count,
        }
    }
}

/// Owned bit buffer (a [`BitReader`]'s backing store).
struct BitBuffer {
    buf: Vec<u8>,
    bit_count: u32,
}

/// MSB-first bit reader over an immutable byte slice.
struct BitReader<'a> {
    buf: &'a BitBuffer,
    cursor: u32,
}

impl<'a> BitReader<'a> {
    fn new(buf: &'a BitBuffer) -> Self {
        BitReader { buf, cursor: 0 }
    }

    fn bits_remaining(&self) -> u32 {
        self.buf.bit_count.saturating_sub(self.cursor)
    }

    fn read(&mut self, n_bits: u32) -> Option<u32> {
        if self.bits_remaining() < n_bits {
            return None;
        }
        let mut v = 0u32;
        for _ in 0..n_bits {
            let byte_idx = (self.cursor / 8) as usize;
            let bit_in_byte = 7 - (self.cursor % 8) as u8;
            let bit = (self.buf.buf[byte_idx] >> bit_in_byte) & 1;
            v = (v << 1) | bit as u32;
            self.cursor += 1;
        }
        Some(v)
    }
}

// ---- permutation derivation ----

/// HKDF-derive a permutation of `[0..N)`. Same salt + same `info`
/// → same permutation; different salts → uncorrelated permutations.
/// Must be deterministic: encode and decode build the same cipher
/// from the same salt.
fn derive_permutation<const N: usize>(conversation_salt: &[u8], info: &[u8]) -> [u8; N] {
    debug_assert!(N <= 256, "permutation derivation only supports N <= 256");
    // Seed bytes — N stream bytes for the Fisher-Yates shuffle. We
    // use HKDF over (salt = PERMUTATION_DOMAIN || info,
    // ikm = conversation_salt) for a clean domain split.
    let mut salt = Vec::with_capacity(PERMUTATION_DOMAIN.len() + 1 + info.len());
    salt.extend_from_slice(PERMUTATION_DOMAIN);
    salt.push(b'/');
    salt.extend_from_slice(info);
    let hk = Hkdf::<Sha256>::new(Some(&salt), conversation_salt);
    // Use 2 bytes per swap to widen the modulo bias surface (the
    // Fisher-Yates `j = rand % (i+1)` step). 2 × N up to 512 bytes
    // for N=256 — well within HKDF-Expand limits.
    let mut stream = vec![0u8; N * 2];
    hk.expand(b"perm-stream", &mut stream).expect("hkdf expand");

    let mut perm: [u8; N] = [0u8; N];
    for (i, slot) in perm.iter_mut().enumerate() {
        *slot = i as u8;
    }
    // Fisher-Yates from i = N-1 down to 1.
    for i in (1..N).rev() {
        let r = u16::from_be_bytes([stream[2 * i], stream[2 * i + 1]]) as usize;
        let j = r % (i + 1);
        perm.swap(i, j);
    }
    perm
}

fn invert<const N: usize>(perm: &[u8; N]) -> [u8; N] {
    let mut inv: [u8; N] = [0u8; N];
    for (i, &p) in perm.iter().enumerate() {
        inv[p as usize] = i as u8;
    }
    inv
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn cipher() -> ConversationCipher {
        ConversationCipher::from_salt(b"unit-test-salt-1")
    }

    #[test]
    fn round_trip_zero_bytes() {
        let c = cipher();
        let s = encode_mode1(&c, b"").unwrap();
        assert!(s.starts_with(MODE1_PREFIX));
        let got = decode_mode1(&c, &s).unwrap();
        assert_eq!(got, b"");
    }

    #[test]
    fn token_bigram_round_trip_recovers_id() {
        let c = cipher();
        let mac_key = b"prose-token-mac-key-unit-test";
        for seed in [0u8, 1, 0x5a, 0xff] {
            let id: [u8; TOKEN_ID_BYTES] =
                std::array::from_fn(|i| (i as u8).wrapping_mul(37).wrapping_add(seed));
            let cover = encode_token(&c, mac_key, &id);
            // Cover is bare prose — no DPC marker, no base64.
            assert!(
                !cover.contains("DPC0"),
                "cover leaked a wire marker: {cover}"
            );
            assert!(!cover.is_empty());
            let recovered =
                decode_token(&c, mac_key, &cover).expect("bigram cover must decode back to the id");
            assert_eq!(recovered, id, "id round-trip failed for seed={seed}");
        }
    }

    #[test]
    fn token_wrong_mac_key_rejects() {
        let c = cipher();
        let id: [u8; TOKEN_ID_BYTES] = [9, 8, 7, 6, 5, 4, 3, 2];
        let cover = encode_token(&c, b"sender-key", &id);
        // A different conversation's mac_key must NOT validate the
        // tag — the HMAC is the "this is OUR token" gate.
        assert!(decode_token(&c, b"different-key", &cover).is_none());
    }

    #[test]
    fn token_plain_chat_is_not_a_false_positive() {
        let c = cipher();
        let mac_key = b"prose-token-mac-key-unit-test";
        // Organic chat that happens to be all in-vocab should fail
        // the HMAC gate (1-in-2^32 accidental match).
        for msg in [
            "lol that is so true honestly",
            "i think we should grab dinner this weekend",
            "ok sounds good talk to you later",
        ] {
            assert!(
                decode_token(&c, mac_key, msg).is_none(),
                "plain chat false-positived as a token: {msg}"
            );
        }
    }

    #[test]
    fn round_trip_one_byte() {
        let c = cipher();
        for b in [0u8, 1, 0x7F, 0xFF] {
            let s = encode_mode1(&c, &[b]).unwrap();
            let got = decode_mode1(&c, &s).unwrap();
            assert_eq!(got, vec![b]);
        }
    }

    #[test]
    fn round_trip_multiple_lengths() {
        let c = cipher();
        for len in [2usize, 5, 10, 32, MODE1_MAX_RAW_LEN] {
            let payload: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_mul(31)).collect();
            let s = encode_mode1(&c, &payload).unwrap();
            let got = decode_mode1(&c, &s).unwrap();
            assert_eq!(got, payload, "round-trip failed at len={len}");
        }
    }

    #[test]
    fn rejects_oversize_payload() {
        let c = cipher();
        let big = vec![0u8; MODE1_MAX_RAW_LEN + 1];
        match encode_mode1(&c, &big) {
            Err(Error::Mode1TooLong { got, max }) => {
                assert_eq!(got, MODE1_MAX_RAW_LEN + 1);
                assert_eq!(max, MODE1_MAX_RAW_LEN);
            }
            other => panic!("expected Mode1TooLong, got {other:?}"),
        }
    }

    #[test]
    fn rejects_missing_prefix() {
        let c = cipher();
        let err = decode_mode1(&c, "Today loud saw a apple.").unwrap_err();
        assert!(matches!(err, Error::NotMode1));
    }

    #[test]
    fn rejects_empty_after_prefix() {
        let c = cipher();
        let err = decode_mode1(&c, "DPC1::").unwrap_err();
        assert!(matches!(err, Error::Mode1ParseError(_)));
    }

    #[test]
    fn different_salts_produce_different_cover_text() {
        let c1 = ConversationCipher::from_salt(b"convo-1");
        let c2 = ConversationCipher::from_salt(b"convo-2");
        let payload = b"hello world";
        let s1 = encode_mode1(&c1, payload).unwrap();
        let s2 = encode_mode1(&c2, payload).unwrap();
        assert_ne!(s1, s2);
        // And cross-decoding fails or yields something else (the
        // payload bytes won't survive, but the tokeniser may still
        // read cover text → we don't promise anything specific
        // beyond "not the original payload").
        let got1 = decode_mode1(&c1, &s1).unwrap();
        assert_eq!(got1, payload);
        let got_cross = decode_mode1(&c2, &s1);
        // Authentication failure is also an acceptable cross-seed result.
        if let Ok(bytes) = got_cross {
            assert_ne!(bytes, payload);
        }
    }

    #[test]
    fn same_salt_is_deterministic() {
        let c1 = ConversationCipher::from_salt(b"deterministic");
        let c2 = ConversationCipher::from_salt(b"deterministic");
        let payload = b"\x01\x02\x03\x04\x05";
        assert_eq!(
            encode_mode1(&c1, payload).unwrap(),
            encode_mode1(&c2, payload).unwrap()
        );
    }

    #[test]
    fn is_mode1_detects_prefix() {
        assert!(is_mode1("DPC1::Today loud saw a apple."));
        assert!(!is_mode1("DPC0::abcd"));
        assert!(!is_mode1("plain text"));
    }

    #[test]
    fn permutation_is_a_bijection_for_templates() {
        let c = cipher();
        let mut seen = HashSet::new();
        for raw in 0..TEMPLATES_LEN as u8 {
            let t = c.template_for_index(raw);
            seen.insert(t);
        }
        assert_eq!(seen.len(), TEMPLATES_LEN);
    }

    #[test]
    fn permutation_is_a_bijection_for_nouns() {
        let c = cipher();
        let mut seen = HashSet::new();
        for raw in 0..NOUN_COUNT {
            let w = c.slot_word(SlotKind::Noun, raw as u8);
            seen.insert(w);
        }
        assert_eq!(seen.len(), NOUN_COUNT);
    }

    #[test]
    fn cover_text_uses_only_known_words_and_skeleton_tokens() {
        let c = cipher();
        let payload = b"\xDE\xAD\xBE\xEF\x00\x42";
        let s = encode_mode1(&c, payload).unwrap();
        let body = s.strip_prefix(MODE1_PREFIX).unwrap();
        let known: HashSet<&str> = NOUNS
            .iter()
            .copied()
            .chain(ADJECTIVES.iter().copied())
            .chain(template_skeleton_tokens())
            .collect();
        for tok in body.split_whitespace() {
            assert!(
                known.contains(tok),
                "unknown token in cover text: {tok:?}\n full: {body}"
            );
        }
    }

    /// Every fixed token that appears in any template's skeleton
    /// (slot markers excluded). Used to validate that decoded text
    /// only contains tokens we know.
    fn template_skeleton_tokens() -> HashSet<&'static str> {
        let mut s = HashSet::new();
        for t in TEMPLATES.iter() {
            for tok in t.skeleton {
                if *tok != crate::mode1_templates::SLOT_TOKEN {
                    s.insert(*tok);
                }
            }
        }
        s
    }

    #[test]
    fn template_pool_size_matches_expected_bit_budget() {
        assert_eq!(TEMPLATES.len(), TEMPLATES_LEN);
        assert_eq!(TEMPLATES_LEN.count_ones(), 1);
        assert_eq!(BITS_PER_SENTENCE, 4 + 2 * 8);
        for t in TEMPLATES.iter() {
            assert_eq!(t.slots.len(), TOTAL_SLOTS);
            assert_eq!(
                t.skeleton
                    .iter()
                    .filter(|s| **s == crate::mode1_templates::SLOT_TOKEN)
                    .count(),
                TOTAL_SLOTS
            );
        }
    }

    #[test]
    fn wordlists_disjoint_from_template_skeletons() {
        let known_words: HashSet<&str> = NOUNS
            .iter()
            .copied()
            .chain(ADJECTIVES.iter().copied())
            .collect();
        for (i, t) in TEMPLATES.iter().enumerate() {
            for tok in t.skeleton {
                if *tok != crate::mode1_templates::SLOT_TOKEN {
                    assert!(
                        !known_words.contains(*tok),
                        "template {i} skeleton token {tok:?} collides with a wordlist entry — \
                         decoder cannot disambiguate"
                    );
                }
            }
        }
    }

    /// 9-MODE1-FIX: guard against accidental duplicate entries
    /// introduced by the wordbank scrub. Each slot index encodes 8
    /// bits, so any actual collision would silently corrupt decode
    /// (two different bytes producing the same word, then ambiguous
    /// inverse). The existing `compass`/`compass2`-style suffixed
    /// entries are distinct strings and pass this check naturally —
    /// the test only catches *exact* duplicates.
    #[test]
    fn wordlist_no_collisions_within_list() {
        let noun_set: HashSet<&str> = NOUNS.iter().copied().collect();
        assert_eq!(
            noun_set.len(),
            NOUNS.len(),
            "NOUNS contains a duplicate entry"
        );
        let adj_set: HashSet<&str> = ADJECTIVES.iter().copied().collect();
        assert_eq!(
            adj_set.len(),
            ADJECTIVES.len(),
            "ADJECTIVES contains a duplicate entry"
        );
    }

    #[test]
    fn template_skeletons_are_unique() {
        let mut seen = HashSet::new();
        for (i, t) in TEMPLATES.iter().enumerate() {
            // Compare the *fixed-token* skeleton (slot markers
            // collapsed to a single sentinel kind label) — two
            // templates with the same fixed text but different slot
            // kinds would produce decode ambiguity.
            let key: Vec<String> = t
                .skeleton
                .iter()
                .enumerate()
                .map(|(j, tok)| {
                    if *tok == crate::mode1_templates::SLOT_TOKEN {
                        // Slot kind is part of the disambiguation key.
                        format!("__slot:{:?}", t.slots[slot_index_within(t, j)])
                    } else {
                        tok.to_string()
                    }
                })
                .collect();
            assert!(seen.insert(key), "duplicate skeleton at template {i}");
        }
    }

    /// For a slot at skeleton position `pos`, return its index in
    /// `template.slots`.
    fn slot_index_within(template: &Template, pos: usize) -> usize {
        template.skeleton[..pos]
            .iter()
            .filter(|s| **s == crate::mode1_templates::SLOT_TOKEN)
            .count()
    }
}
