//! Bigram (n=2) language-model prose-token codec.
//!
//! Replaces the Mode 1 16-template-with-fixed-slots encoder for the
//! prose-token cover (the 24-byte `P`+detect-tag payload that wraps
//! cipher-store references). The output reads more like organic
//! chat — each word is sampled from the conditional distribution
//! P(word | previous word) trained on a small embedded corpus,
//! instead of being slotted into a static skeleton.
//!
//! ## Pipeline
//!
//! Encode:
//!   `bits ──arithmetic-decode──> word indices ──render──> "lol that …"`
//!
//! Decode:
//!   `"lol that …" ──tokenize──> word indices ──arithmetic-encode──> bits`
//!
//! ## Why arithmetic coding (range coder)?
//!
//! At each step the next word's *information content* is
//! `-log2(P(word | prev))`, which is a fractional bit budget. A
//! fixed-bit-per-word scheme can't track that. The range coder
//! threads a 32-bit interval through the model's CDF and emits /
//! consumes bits as the interval narrows past power-of-two
//! boundaries — exact, lossless, no slack.
//!
//! ## Wire / framing
//!
//! Payload is the same 192 bits as Mode 1's prose-token
//! (`TOKEN_ID_BYTES * 8 + TOKEN_MAC_BYTES * 8 = 160 + 32`). The
//! arithmetic decoder emits words until the bit cursor has consumed
//! exactly `TOKEN_PAYLOAD_BITS` of input. Encoder is the inverse: it
//! re-encodes the word stream and returns the recovered bits, or
//! `None` if any token isn't in the vocab.
//!
//! No length prefix on the wire — the bit budget is fixed by
//! `TOKEN_PAYLOAD_BITS`. Both sides agree on the budget out of band.
//!
//! ## Model
//!
//! Trained at first use from [`CORPUS`] via a vocab of
//! [`VOCAB_SIZE`] most-frequent tokens (`OnceLock`-cached). Laplace
//! +1 smoothing guarantees every bigram has positive count so the
//! arithmetic coder never hits a zero-mass partition. Tokens outside
//! the vocab are dropped during training; the encoder only emits
//! in-vocab words by construction.

use std::sync::OnceLock;

/// Vocabulary size. Powers of two are convenient for the renormalization
/// loop but not required — any positive integer works. 128 keeps the
/// cumulative-count table at 128 × 128 × 4 bytes = 64 KiB hot in cache
/// while giving the model enough surface to read like chat.
pub const VOCAB_SIZE: usize = 128;

/// Sentinel index for "beginning of sequence" — the implicit prefix
/// before the first word. Trained as if a sentence break preceded
/// every corpus line.
pub const BOS_IDX: usize = 0;

/// Token payload bit-count this codec encodes / decodes per cover.
/// Must match `mode1::TOKEN_PAYLOAD_BITS` (20-byte `P` + 4-byte detect tag
/// = 24 bytes = 192 bits). Hardcoded here rather than re-exported
/// to avoid a circular module dependency.
pub const TOKEN_PAYLOAD_BITS: u32 = (20 + 4) * 8;

/// Interval precision (bits). We subdivide a `[0, 2^PRECISION)`
/// integer interval by the model CDF until the top [`CHUNK_BITS`] of the
/// interval are pinned.
///
/// Overflow budget: the hot products are `width * total` and
/// `(value - lo) * total`, both bounded by `2^PRECISION * total`.
/// With `total < 2^16` (128 smoothing + real bigram counts) that's
/// `2^126 < 2^128` — safe in `u128`.
const PRECISION: u32 = 110;
const FULL: u128 = 1u128 << PRECISION;

/// Payload bits carried by one arithmetic-coded chunk.
///
/// A `u128` interval cannot pin a 192-bit payload in one pass, which is why
/// the pointer used to fall out of the arithmetic coder entirely into the
/// fixed-width substitution table below. Chunking removes that cliff: the
/// payload is split into `ceil(target_bits / CHUNK_BITS)` chunks, each coded
/// by the real model, and the word streams are concatenated. The model
/// context (`prev`) carries across a chunk boundary, so the prose does not
/// restart mid-cover; only the interval resets.
///
/// Chunk boundaries are **self-delimiting** — both directions stop a chunk on
/// the same `(lo, hi)` predicate, so no length prefix or separator reaches
/// the wire.
///
/// ## Why 64 and not `PRECISION`
///
/// `bits_to_value` aims at the **centre** of the target block rather than its
/// base, which makes termination provable rather than empirical: with
/// `shift = PRECISION - CHUNK_BITS`, any interval of width `<= 2^(shift-1)`
/// that contains the centre is necessarily inside the block, so the chunk is
/// pinned. Therefore every subdivision runs with `width > 2^(shift-1)`, i.e.
/// `> 2^45` at `CHUNK_BITS = 64`, against a row total below `2^16` — no step
/// can round a bucket down to zero width. Widening the chunk toward
/// `PRECISION` shrinks that margin to nothing (at `CHUNK_BITS = 110`,
/// `shift = 0` and a bucket collapses immediately). 64 also divides the
/// 192-bit pointer exactly.
pub const CHUNK_BITS: u32 = 64;
/// Safety bound on emitted word count. The interval-subdivision loop
/// is guaranteed to terminate (width strictly decreases by a factor
/// < 1 each step), but a near-deterministic model could in principle
/// stretch the cover; this caps it. ~96 bits / ~1 bit-per-word worst
/// case ≈ 96, so 256 is comfortable headroom.
const MAX_WORDS: usize = 256;

/// LEGACY (receive-only as of B0-06). The wide pointer codec carries six
/// payload bits in each cover word, so the 24-byte pointer-v2 carrier had a
/// fixed 32-word floor. This is the format that produced uniform word salad;
/// senders no longer emit it, but [`legacy_wide_encode_words`] is still
/// reachable from the decoder so covers posted by pre-B0-06 peers keep
/// decoding during the rollout window.
pub const WIDE_TOKEN_WORD_BITS: usize = 6;
pub const WIDE_TOKEN_WORDS: usize = (TOKEN_PAYLOAD_BITS as usize).div_ceil(WIDE_TOKEN_WORD_BITS);

/// Training corpus. Curated chat-style English; each line ends in a
/// period so BOS-bigrams reflect sentence starts. Kept under 6 KB so
/// the OnceLock training pass is sub-millisecond. Lines are
/// lowercased, single-space-separated, ASCII-only.
const CORPUS: &str = include_str!("bigram_corpus.txt");

/// Bigram model: vocab + cumulative-count table.
#[derive(Debug)]
pub struct BigramModel {
    pub vocab: [&'static str; VOCAB_SIZE],
    /// `index_of[word] = vocab idx` for fast tokenization.
    pub index_of: std::collections::HashMap<&'static str, usize>,
    /// `cum[prev][next]` = sum of counts(prev → 0..=next). Stored
    /// as u32 to keep the 64-bit cum × range product well under
    /// 2^64 even with VOCAB_SIZE^2 entries laplace-smoothed.
    pub cum: Vec<[u32; VOCAB_SIZE]>,
}

static MODEL: OnceLock<BigramModel> = OnceLock::new();

pub fn model() -> &'static BigramModel {
    MODEL.get_or_init(train_from_corpus)
}

fn train_from_corpus() -> BigramModel {
    // Step 1: tokenize the corpus and pick the top-(VOCAB_SIZE-1)
    // most-frequent tokens. Reserve slot 0 for BOS.
    let raw_tokens: Vec<&'static str> = tokenize_corpus(CORPUS);
    let mut freq: std::collections::HashMap<&'static str, u32> = std::collections::HashMap::new();
    for &t in &raw_tokens {
        *freq.entry(t).or_insert(0) += 1;
    }
    let mut by_freq: Vec<(&'static str, u32)> = freq.into_iter().collect();
    // Sort by frequency desc, then alphabetic for determinism.
    by_freq.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    by_freq.truncate(VOCAB_SIZE - 1);

    let mut vocab: [&'static str; VOCAB_SIZE] = [""; VOCAB_SIZE];
    vocab[BOS_IDX] = "<BOS>";
    for (i, (w, _)) in by_freq.iter().enumerate() {
        vocab[i + 1] = *w;
    }

    let mut index_of: std::collections::HashMap<&'static str, usize> =
        std::collections::HashMap::with_capacity(VOCAB_SIZE);
    for (i, w) in vocab.iter().enumerate() {
        index_of.insert(*w, i);
    }

    // Step 2: count bigrams. Tokens NOT in the vocab break the
    // sentence chain (the next in-vocab token follows BOS, same as
    // sentence start). This keeps the model coherent without
    // contaminating bigrams across out-of-vocab gaps.
    let mut counts: Vec<[u32; VOCAB_SIZE]> = Vec::with_capacity(VOCAB_SIZE);
    for _ in 0..VOCAB_SIZE {
        counts.push([0u32; VOCAB_SIZE]);
    }
    let mut prev_idx = BOS_IDX;
    for line in CORPUS.lines() {
        prev_idx = BOS_IDX;
        for tok in line.split_ascii_whitespace() {
            let tok = strip_punct(tok);
            if tok.is_empty() {
                continue;
            }
            match index_of.get(tok) {
                Some(&i) => {
                    counts[prev_idx][i] = counts[prev_idx][i].saturating_add(1);
                    prev_idx = i;
                }
                None => {
                    // Out-of-vocab token: skip and treat next
                    // in-vocab as a sentence restart.
                    prev_idx = BOS_IDX;
                }
            }
        }
    }
    let _ = prev_idx; // tolerate unused trailing

    // Step 3: scaled smoothing. The corpus is small, so raw bigram
    // counts are tiny (most real pairs occur 1-5×). A flat Laplace
    // +1 over 128 cells would then bury the real structure under
    // the smoothing floor and the cover reads like word salad.
    // Instead scale every observed count by `COUNT_SCALE` first,
    // THEN add the +1 floor: a real pair at count 2 becomes
    // 2·SCALE+1, dominating the unseen-pair floor of 1, so the
    // model follows the corpus's actual word order while still
    // assigning every transition non-zero mass (required so the
    // arithmetic coder never partitions on a zero slice).
    //
    // Trade-off: a sharper model lowers per-word entropy, so the
    // cover needs more words to carry the fixed 96-bit payload.
    // SCALE=32 lands ~3-4 bits/word → ~25-30 word covers, which
    // read naturally without being absurdly long.
    const COUNT_SCALE: u32 = 32;
    for row in counts.iter_mut() {
        for cell in row.iter_mut() {
            *cell = cell.saturating_mul(COUNT_SCALE).saturating_add(1);
        }
        // Force BOS to zero probability as a SUCCESSOR. BOS is only
        // ever a sentence-start prefix, never a real next-word, but
        // the +1 floor above would otherwise give it mass — and
        // `render_words` skips it, so a decoded-then-rendered cover
        // that emitted BOS would lose a word and fail to re-encode.
        // Zeroing its column makes `find_word_by_boundary` unable to
        // ever select it (its bucket has zero width).
        row[BOS_IDX] = 0;
    }

    // Step 4: build cumulative counts row-wise. cum[prev][next] =
    // sum of counts[prev][0..=next]. Total per row = cum[prev]
    // [VOCAB_SIZE - 1]; reused at runtime as the partition
    // denominator.
    let mut cum: Vec<[u32; VOCAB_SIZE]> = Vec::with_capacity(VOCAB_SIZE);
    for row in counts.iter() {
        let mut acc = 0u32;
        let mut crow = [0u32; VOCAB_SIZE];
        for (i, &c) in row.iter().enumerate() {
            acc = acc.saturating_add(c);
            crow[i] = acc;
        }
        cum.push(crow);
    }

    BigramModel {
        vocab,
        index_of,
        cum,
    }
}

fn tokenize_corpus(s: &'static str) -> Vec<&'static str> {
    // SAFETY: CORPUS is &'static str, every byte-slice produced by
    // `split_ascii_whitespace + strip_punct` is also &'static.
    let mut out = Vec::new();
    for line in s.lines() {
        for tok in line.split_ascii_whitespace() {
            let tok = strip_punct(tok);
            if !tok.is_empty() {
                out.push(tok);
            }
        }
    }
    out
}

fn strip_punct(tok: &'static str) -> &'static str {
    // Strip leading + trailing ASCII punctuation. Conservative —
    // doesn't touch interior punctuation (so "don't", "isn't" pass
    // through as the obvious tokens).
    let bytes = tok.as_bytes();
    let mut lo = 0usize;
    let mut hi = bytes.len();
    while lo < hi && is_punct(bytes[lo]) {
        lo += 1;
    }
    while hi > lo && is_punct(bytes[hi - 1]) {
        hi -= 1;
    }
    // SAFETY: lo/hi are at ASCII boundaries (ASCII punct + ASCII
    // whitespace splits).
    unsafe { std::str::from_utf8_unchecked(&bytes[lo..hi]) }
}

fn is_punct(b: u8) -> bool {
    matches!(
        b,
        b'.' | b',' | b'!' | b'?' | b';' | b':' | b'"' | b'(' | b')' | b'[' | b']'
    )
}

// ============================================================
// Arithmetic coding core (interval subdivision)
// ============================================================
//
// We work over a fixed `[0, 2^PRECISION)` integer interval and
// subdivide it by the model CDF. The payload's `TOKEN_PAYLOAD_BITS`
// bits become the high bits of a target value `V` inside the
// interval; the encoder walks word-by-word, each step picking the
// word whose CDF bucket contains `V` and narrowing `[lo, hi)` to
// that bucket. It stops when the top `TOKEN_PAYLOAD_BITS` of `lo`
// and `hi-1` agree — at which point the word sequence uniquely
// determines those payload bits. The decoder replays the same
// walk from the word sequence and reads the pinned top bits back
// out. Strictly-decreasing width (every bucket prob < 1 under
// Laplace smoothing) guarantees termination; `MAX_WORDS` is a
// belt-and-suspenders cap.

/// Pack `target_bits` booleans (MSB-first) into the high bits of a
/// `u128` target value inside `[0, 2^PRECISION)`, then aim at the
/// **centre** of the resulting block rather than its base.
///
/// The block for payload `p` is `[p << shift, (p+1) << shift)`. Aiming at
/// the base leaves the target on a block boundary, so an interval that
/// contains it can straddle two blocks no matter how narrow it gets — the
/// subdivision then has to keep going until the interval collapses, and a
/// collapsed interval rounds buckets to zero width. Aiming at the centre
/// makes "width <= 2^(shift-1)" sufficient for containment, which bounds
/// the loop and keeps every subdivision far above the row total. See
/// [`CHUNK_BITS`].
fn bits_to_value(bits: &[bool], target_bits: u32) -> u128 {
    let mut payload: u128 = 0;
    for i in 0..target_bits as usize {
        let b = if i < bits.len() { bits[i] } else { false };
        payload = (payload << 1) | b as u128;
    }
    // Left-align so the payload occupies the top `target_bits` of
    // the PRECISION-bit interval; low bits address within the block.
    let shift = PRECISION - target_bits;
    let base = payload << shift;
    if shift == 0 {
        base
    } else {
        base | (1u128 << (shift - 1))
    }
}

/// Read the pinned top `target_bits` of `lo` back out as a MSB-first
/// boolean vector.
fn value_to_bits(lo: u128, target_bits: u32) -> Vec<bool> {
    let payload = lo >> (PRECISION - target_bits);
    let mut out = Vec::with_capacity(target_bits as usize);
    for i in (0..target_bits).rev() {
        out.push((payload >> i) & 1 == 1);
    }
    out
}

/// Decode a bit stream into a word-index sequence using the bigram
/// model. Returns the word indices in emission order — variable
/// length, typically ~15-25 for a 96-bit payload on a 128-word
/// flat-ish model.
pub fn arithmetic_decode_bits(bits: &[bool], target_bits: u32) -> Vec<usize> {
    if target_bits == 0 {
        return Vec::new();
    }
    let model = model();
    let mut out: Vec<usize> = Vec::new();
    // Model context carries across chunk boundaries so the cover reads as
    // one continuous utterance; only the interval restarts.
    let mut prev = BOS_IDX;

    let mut coded = 0u32;
    while coded < target_bits {
        let chunk_bits = (target_bits - coded).min(CHUNK_BITS);
        let chunk: Vec<bool> = (0..chunk_bits as usize)
            .map(|i| bits.get(coded as usize + i).copied().unwrap_or(false))
            .collect();
        let value = bits_to_value(&chunk, chunk_bits);
        let shift = PRECISION - chunk_bits;

        let mut lo: u128 = 0;
        let mut hi: u128 = FULL; // exclusive upper bound

        while out.len() < MAX_WORDS {
            // Stop once the top `chunk_bits` of the interval are
            // pinned: lo and (hi-1) share their high bits.
            if (lo >> shift) == ((hi - 1) >> shift) {
                break;
            }
            let width = hi - lo;
            let total = model.cum[prev][VOCAB_SIZE - 1] as u128;
            // Unreachable for a centred target (see `CHUNK_BITS`): the chunk
            // pins while width is still above 2^(shift-1) >> total. Bail
            // rather than subdivide a width that cannot hold every bucket.
            if width <= total {
                break;
            }
            // Pick the word whose NARROWED bucket actually contains
            // `value`. Bucket w spans [lo + ⌊width·cum[w-1]/total⌋,
            // lo + ⌊width·cum[w]/total⌋). We can't search in CDF space
            // (the floor in the narrowing shifts boundaries by up to a
            // unit), so search directly on the floored boundary:
            // smallest w with ⌊width·cum[w]/total⌋ > value - lo.
            let offset = value - lo;
            let word = find_word_by_boundary(&model.cum[prev], width, total, offset);
            let cum_lo = if word == 0 {
                0u128
            } else {
                model.cum[prev][word - 1] as u128
            };
            let cum_hi = model.cum[prev][word] as u128;
            let new_lo = lo + (width * cum_lo) / total;
            let new_hi = lo + (width * cum_hi) / total;
            lo = new_lo;
            hi = new_hi;
            out.push(word);
            prev = word;
        }
        coded += chunk_bits;
    }
    out
}

/// Encode a word-index sequence back into the original bit stream.
/// Exact inverse of [`arithmetic_decode_bits`]: replays the same
/// interval walk, then reads the pinned top `target_bits` of the
/// final `lo`. Returns a zero-filled vector on an out-of-vocab
/// index (surfaced as a decode miss upstream).
pub fn arithmetic_encode_words(words: &[usize], target_bits: u32) -> Vec<bool> {
    if target_bits == 0 {
        return Vec::new();
    }
    let model = model();
    let mut bits: Vec<bool> = Vec::with_capacity(target_bits as usize);
    let mut prev = BOS_IDX;
    let mut cursor = 0usize;

    let mut coded = 0u32;
    while coded < target_bits {
        let chunk_bits = (target_bits - coded).min(CHUNK_BITS);
        let shift = PRECISION - chunk_bits;
        let mut lo: u128 = 0;
        let mut hi: u128 = FULL;

        // Same stopping predicate as the decoder, evaluated on the same
        // (lo, hi) — that is what makes the chunk boundary self-delimiting
        // without a marker on the wire.
        while (lo >> shift) != ((hi - 1) >> shift) {
            let width = hi - lo;
            let total = model.cum[prev][VOCAB_SIZE - 1] as u128;
            // Arbitrary in-vocabulary prose (every inbound Discord message
            // reaches here) can narrow the interval past the point where the
            // model's buckets still fit in it. Our own covers never can — the
            // centred target pins the chunk while width is still above
            // 2^(shift-1). Reject instead of subdividing a degenerate width,
            // which would underflow `hi - 1` on the next pass.
            if width <= total {
                return vec![false; target_bits as usize];
            }
            let Some(&word) = words.get(cursor) else {
                // Ran out of words before the chunk pinned: not our cover.
                return vec![false; target_bits as usize];
            };
            if word >= VOCAB_SIZE {
                return vec![false; target_bits as usize];
            }
            cursor += 1;
            let cum_lo = if word == 0 {
                0u128
            } else {
                model.cum[prev][word - 1] as u128
            };
            let cum_hi = model.cum[prev][word] as u128;
            let new_lo = lo + (width * cum_lo) / total;
            let new_hi = lo + (width * cum_hi) / total;
            if new_hi <= new_lo {
                // Zero-mass bucket (BOS as a successor). Not producible by
                // the decoder, so this word stream is not one of our covers.
                return vec![false; target_bits as usize];
            }
            lo = new_lo;
            hi = new_hi;
            prev = word;
        }
        bits.extend(value_to_bits(lo, chunk_bits));
        coded += chunk_bits;
    }

    if cursor != words.len() {
        // Trailing words the chunk walk never consumed: not a canonical cover.
        return vec![false; target_bits as usize];
    }
    bits
}

// LEGACY, RECEIVE ONLY (B0-06). Before chunking, payloads wider than the
// arithmetic interval precision fell back to this lossless 6-bit packing, and
// the 192-bit shipping pointer always did. It ignores the language model
// entirely — each 6-bit group indexes the first 64 vocabulary words — so its
// output is uniform word salad at ~8.8 bits/word of model surprisal against a
// ~5.05 bits/word model. Senders no longer emit it. It is retained, and still
// reachable from `mode1::decode_token`, so covers posted by pre-B0-06 peers
// keep decoding. Do not delete: that is the rollout compatibility path.
pub fn legacy_wide_decode_bits(bits: &[bool], target_bits: u32) -> Vec<usize> {
    bits.iter()
        .copied()
        .chain(std::iter::repeat(false))
        .take(target_bits as usize)
        .collect::<Vec<_>>()
        .chunks(WIDE_TOKEN_WORD_BITS)
        .map(|chunk| {
            1 + chunk
                .iter()
                .fold(0usize, |value, bit| (value << 1) | *bit as usize)
        })
        .collect()
}

/// LEGACY, RECEIVE ONLY (B0-06). Inverse of [`legacy_wide_decode_bits`].
///
/// The word count is fixed by the format at `ceil(target_bits / 6)`. It must
/// be checked before the loop, not implied by it: this function is fed every
/// inbound Discord message (via `mode1::decode_token`), and a longer stream
/// used to underflow `target_bits - index * WIDE_TOKEN_WORD_BITS` on the
/// 33rd word — a remotely reachable panic in debug builds and a masked shift
/// in release. Any in-vocabulary sentence over 32 words drawn from the first
/// 64 vocabulary slots reached it.
pub fn legacy_wide_encode_words(words: &[usize], target_bits: u32) -> Vec<bool> {
    if words.len() != (target_bits as usize).div_ceil(WIDE_TOKEN_WORD_BITS) {
        return vec![false; target_bits as usize];
    }
    let mut bits = Vec::with_capacity(target_bits as usize);
    for (index, &word) in words.iter().enumerate() {
        if !(1..=64).contains(&word) {
            return vec![false; target_bits as usize];
        }
        let value = word - 1;
        let width = (target_bits as usize - index * WIDE_TOKEN_WORD_BITS)
            .min(WIDE_TOKEN_WORD_BITS);
        for shift in (0..width).rev() {
            bits.push((value >> shift) & 1 == 1);
        }
    }
    bits.truncate(target_bits as usize);
    if bits.len() == target_bits as usize {
        bits
    } else {
        vec![false; target_bits as usize]
    }
}

/// Smallest word index `w` whose floored narrowed boundary
/// `⌊width·cum[w]/total⌋` strictly exceeds `offset` — i.e. the
/// bucket that contains `offset` under the exact same arithmetic
/// the narrowing step uses. Monotonic in `w`, so binary search is
/// valid.
fn find_word_by_boundary(
    cum_row: &[u32; VOCAB_SIZE],
    width: u128,
    total: u128,
    offset: u128,
) -> usize {
    let mut lo = 0usize;
    let mut hi = VOCAB_SIZE - 1;
    while lo < hi {
        let mid = (lo + hi) / 2;
        let boundary = (width * cum_row[mid] as u128) / total;
        if boundary > offset {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    lo
}

// ============================================================
// String <-> word-index helpers
// ============================================================

/// Render a word-index sequence as a space-separated lowercase
/// string with a trailing period. Skips BOS (index 0) silently in
/// case it ever appears.
pub fn render_words(words: &[usize]) -> String {
    let model = model();
    let mut out = String::with_capacity(words.len() * 6);
    let mut first = true;
    for &w in words {
        if w == BOS_IDX || w >= VOCAB_SIZE {
            continue;
        }
        if !first {
            out.push(' ');
        }
        out.push_str(model.vocab[w]);
        first = false;
    }
    if !out.is_empty() {
        out.push('.');
    }
    out
}

/// Parse a candidate string into vocab indices. Returns None if
/// any token isn't in the vocab — that's the cheap "this isn't an
/// OSL prose-token" rejection path callers rely on.
pub fn parse_words(s: &str) -> Option<Vec<usize>> {
    let model = model();
    let mut out = Vec::new();
    for tok in s.split_ascii_whitespace() {
        let mut t = tok.to_ascii_lowercase();
        // Strip leading + trailing punctuation.
        while let Some(c) = t.chars().last() {
            if matches!(c, '.' | ',' | '!' | '?' | ';' | ':' | '"' | ')' | ']') {
                t.pop();
            } else {
                break;
            }
        }
        while let Some(c) = t.chars().next() {
            if matches!(c, '"' | '(' | '[') {
                t.remove(0);
            } else {
                break;
            }
        }
        if t.is_empty() {
            continue;
        }
        let idx = *model.index_of.get(t.as_str())?;
        out.push(idx);
    }
    if out.is_empty() {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_initialises() {
        let m = model();
        // BOS occupies slot 0; the remaining 127 slots come from
        // the corpus.
        assert_eq!(m.vocab[BOS_IDX], "<BOS>");
        for i in 1..VOCAB_SIZE {
            assert!(!m.vocab[i].is_empty(), "vocab slot {i} empty");
        }
        // Every row of cum must end with a positive total
        // (smoothing guarantee).
        for (i, row) in m.cum.iter().enumerate() {
            assert!(row[VOCAB_SIZE - 1] > 0, "row {i} total non-positive");
        }
    }

    #[test]
    fn arithmetic_round_trip_zero_payload() {
        // 96 zero bits should encode and decode round-trip via the
        // word-stream representation.
        let bits = vec![false; TOKEN_PAYLOAD_BITS as usize];
        let words = arithmetic_decode_bits(&bits, TOKEN_PAYLOAD_BITS);
        assert!(!words.is_empty());
        let back = arithmetic_encode_words(&words, TOKEN_PAYLOAD_BITS);
        assert_eq!(back, bits);
    }

    #[test]
    fn arithmetic_round_trip_alternating() {
        let bits: Vec<bool> = (0..TOKEN_PAYLOAD_BITS).map(|i| i % 2 == 0).collect();
        let words = arithmetic_decode_bits(&bits, TOKEN_PAYLOAD_BITS);
        let back = arithmetic_encode_words(&words, TOKEN_PAYLOAD_BITS);
        assert_eq!(back, bits);
    }

    #[test]
    fn arithmetic_round_trip_random() {
        // Deterministic LCG so the test stays repeatable.
        let mut x: u64 = 0xdead_beef_cafe_babe;
        let bits: Vec<bool> = (0..TOKEN_PAYLOAD_BITS)
            .map(|_| {
                x = x
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                (x >> 63) & 1 == 1
            })
            .collect();
        let words = arithmetic_decode_bits(&bits, TOKEN_PAYLOAD_BITS);
        let back = arithmetic_encode_words(&words, TOKEN_PAYLOAD_BITS);
        assert_eq!(back, bits);
    }

    #[test]
    fn ten_thousand_payloads_round_trip_within_word_bound() {
        // Exercise interval-boundary rounding far beyond the handful of
        // human-readable fixtures. This is deterministic and covers 960k
        // generated payload bits without depending on an RNG crate.
        let mut x = 0x6a09_e667_f3bc_c909u64;
        for case in 0..10_000 {
            let bits: Vec<bool> = (0..TOKEN_PAYLOAD_BITS)
                .map(|_| {
                    x ^= x << 13;
                    x ^= x >> 7;
                    x ^= x << 17;
                    x & 1 == 1
                })
                .collect();
            let words = arithmetic_decode_bits(&bits, TOKEN_PAYLOAD_BITS);
            assert!(
                !words.is_empty() && words.len() < MAX_WORDS,
                "case {case} exhausted the word bound"
            );
            assert_eq!(
                arithmetic_encode_words(&words, TOKEN_PAYLOAD_BITS),
                bits,
                "case {case} failed interval round-trip"
            );
        }
    }

    #[test]
    fn sample_cover_length_is_reasonable() {
        // Regression guard on cover length: a 96-bit payload on the
        // SCALE=32 model should land in a sane chat-length window
        // (not 5 words, not 200). If the model sharpens/flattens
        // dramatically this catches it. Also prints samples under
        // `--nocapture` for eyeballing readability.
        for seed in [
            0x1111_2222_3333_4444u64,
            0xdead_beef_0000_0001,
            0xffff_0000_aaaa_5555,
        ] {
            let mut x = seed;
            let bits: Vec<bool> = (0..TOKEN_PAYLOAD_BITS)
                .map(|_| {
                    x = x
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407);
                    (x >> 63) & 1 == 1
                })
                .collect();
            let words = arithmetic_decode_bits(&bits, TOKEN_PAYLOAD_BITS);
            let s = render_words(&words);
            println!("[{} words] {}", words.len(), s);
            assert!(
                (8..=64).contains(&words.len()),
                "cover length {} out of expected band",
                words.len()
            );
        }
    }

    #[test]
    fn render_then_parse_round_trip() {
        let bits: Vec<bool> = (0..TOKEN_PAYLOAD_BITS).map(|i| i % 3 == 0).collect();
        let words = arithmetic_decode_bits(&bits, TOKEN_PAYLOAD_BITS);
        let s = render_words(&words);
        let parsed = parse_words(&s).expect("parse own output");
        assert_eq!(parsed, words);
    }
}
