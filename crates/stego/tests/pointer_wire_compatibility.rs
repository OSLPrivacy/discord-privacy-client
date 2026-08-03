//! B0-06: the send-side pointer wire format changed. This pins what that
//! does and does not break.
//!
//! Before B0-06 every pointer cover was emitted by the 6-bit / 64-word
//! substitution table, because the 192-bit payload overflowed the arithmetic
//! interval. Senders now emit chunked arithmetic-coded prose. A pre-B0-06
//! peer therefore cannot read a new cover — that is a genuine, unavoidable
//! break and it is asserted here so nobody rediscovers it in the field.
//! The reverse direction is preserved: a new build still decodes covers
//! produced by the old one.

use stego::bigram;
use stego::{decode_token, encode_token, ConversationCipher, TOKEN_ID_BYTES, TOKEN_PAYLOAD_BITS};

/// Reproduces exactly what a pre-B0-06 sender put on the wire: the payload
/// bits straight through the 6-bit substitution table, rendered.
fn legacy_cover(mac_key: &[u8], id: &[u8; TOKEN_ID_BYTES]) -> String {
    let tag = stego::compute_token_tag(mac_key, id);
    let mut bits = Vec::with_capacity(TOKEN_PAYLOAD_BITS as usize);
    for &b in id.iter().chain(tag.iter()) {
        for i in (0..8).rev() {
            bits.push((b >> i) & 1 == 1);
        }
    }
    let words = bigram::legacy_wide_decode_bits(&bits, TOKEN_PAYLOAD_BITS);
    bigram::render_words(&words)
}

#[test]
fn a_cover_from_the_old_build_still_decodes_on_the_new_build() {
    let cipher = ConversationCipher::from_salt(b"b0-06-compat");
    let key = b"b0-06-compat-detect-key";
    for n in 0..16u8 {
        let mut id = [0u8; TOKEN_ID_BYTES];
        for (i, slot) in id.iter_mut().enumerate() {
            *slot = (i as u8).wrapping_mul(17).wrapping_add(n.wrapping_mul(211));
        }
        let old = legacy_cover(key, &id);
        assert_eq!(
            old.split_ascii_whitespace().count(),
            bigram::WIDE_TOKEN_WORDS,
            "the legacy carrier is a fixed 32 words"
        );
        assert_eq!(
            decode_token(&cipher, key, &old),
            Some(id),
            "pointer {n}: a pre-B0-06 cover must still decode"
        );
    }
}

#[test]
fn the_send_side_format_changed_and_this_test_says_so_out_loud() {
    // Documented break: a pre-B0-06 receiver runs only the substitution
    // decoder, which reads a fixed 32 words. New covers are not 32 words and
    // do not carry the payload in 6-bit groups, so an old peer sees ordinary
    // chat and recovers nothing. Anyone changing the rollout plan should see
    // this fail if the send format is reverted.
    let cipher = ConversationCipher::from_salt(b"b0-06-compat");
    let key = b"b0-06-compat-detect-key";
    let id = [0x5a; TOKEN_ID_BYTES];
    let new_cover = encode_token(&cipher, key, &id);

    let words = bigram::parse_words(&new_cover).expect("own cover parses");
    let legacy_bits = bigram::legacy_wide_encode_words(&words, TOKEN_PAYLOAD_BITS);
    let true_bits = {
        let tag = stego::compute_token_tag(key, &id);
        let mut bits = Vec::new();
        for &b in id.iter().chain(tag.iter()) {
            for i in (0..8).rev() {
                bits.push((b >> i) & 1 == 1);
            }
        }
        bits
    };
    assert_ne!(
        legacy_bits, true_bits,
        "an old peer must not be assumed to read the new cover"
    );
}

#[test]
fn ordinary_in_vocabulary_chat_is_rejected_without_panicking() {
    // `decode_token` runs on every inbound message in an OSL scope, so
    // `arithmetic_encode_words` is fed arbitrary prose. Long runs of common
    // words narrow the coding interval far past anything our own covers
    // reach; the walk must reject rather than subdivide a degenerate
    // interval (which underflows `hi - 1`).
    let cipher = ConversationCipher::from_salt(b"b0-06-hostile");
    let key = b"b0-06-hostile-key";
    let model = bigram::model();

    for len in [1usize, 2, 8, 32, 64, 128, 200, 400] {
        for start in [1usize, 2, 3, 5, 9, 17, 33, 65] {
            // A long run of in-vocabulary words: exactly what makes the
            // interval collapse, since each word is high-probability.
            let text = (0..len)
                .map(|i| model.vocab[(start + i) % (bigram::VOCAB_SIZE - 1) + 1])
                .collect::<Vec<_>>()
                .join(" ");
            let parsed = bigram::parse_words(&text).expect("all words are in vocab");
            // Must not panic, and must not fabricate a payload.
            let _ = bigram::arithmetic_encode_words(&parsed, TOKEN_PAYLOAD_BITS);
            assert_eq!(
                decode_token(&cipher, key, &text),
                None,
                "plain chat ({len} words from {start}) must not decode as a pointer"
            );
        }
    }

    // The same, with a repeated single high-probability word — the fastest
    // route to a collapsed interval.
    for len in [16usize, 64, 256, 512] {
        let text = vec!["the"; len].join(" ");
        if let Some(parsed) = bigram::parse_words(&text) {
            let _ = bigram::arithmetic_encode_words(&parsed, TOKEN_PAYLOAD_BITS);
        }
        assert_eq!(decode_token(&cipher, key, &text), None);
    }
}
