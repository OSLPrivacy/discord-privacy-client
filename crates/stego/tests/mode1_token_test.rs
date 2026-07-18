//! Integration tests for the Phase 2 prose-token encoder/decoder.
//!
//! Properties verified:
//!   * Roundtrip: encode(id) → decode → same id.
//!   * Detection: decode rejects plain English (no false positive).
//!   * Token detection: decode rejects a mismatched HMAC key.
//!   * Canonicality: extra in-vocabulary words are rejected.
//!   * Length bound: a compact variable-length bigram word stream.

use stego::{decode_token, encode_token, ConversationCipher, TOKEN_ID_BYTES};

fn cipher(salt: &[u8]) -> ConversationCipher {
    ConversationCipher::from_salt(salt)
}

#[test]
fn roundtrip_recovers_id() {
    let c = cipher(b"token-roundtrip-test");
    let key = b"mac-key-32-bytes-padded________";
    let id: [u8; TOKEN_ID_BYTES] = [0x07, 0x74, 0xc9, 0x22, 0xdf, 0x45, 0x04, 0x7f];
    let cover = encode_token(&c, key, &id);
    let got = decode_token(&c, key, &cover).expect("decode_token succeeded");
    assert_eq!(got, id);
}

#[test]
fn output_is_short_and_prefix_free() {
    let c = cipher(b"token-length-test");
    let key = b"mac-key-32-bytes-padded________";
    let id: [u8; TOKEN_ID_BYTES] = [0u8; TOKEN_ID_BYTES];
    let cover = encode_token(&c, key, &id);
    assert!(
        !cover.starts_with("DPC"),
        "prose-token must have no magic prefix"
    );
    let n = cover.chars().count();
    assert!(
        n < 500,
        "prose token should be a short paragraph (~125-250 chars), got {n}"
    );
    let word_count = cover.split_ascii_whitespace().count();
    assert!(
        (8..=64).contains(&word_count),
        "expected a compact word stream, got {word_count} words: {cover:?}"
    );
    assert!(cover.ends_with('.'), "cover should end with punctuation");
}

#[test]
fn decode_rejects_plain_english() {
    let c = cipher(b"token-detect-test");
    let key = b"mac-key-32-bytes-padded________";
    // A plain English sentence that doesn't match any Mode 1 template
    // skeleton -- decoder must return None, not panic.
    let plain = "hey what's up, can you grab lunch tomorrow?";
    assert!(decode_token(&c, key, plain).is_none());
    // Also: an empty message.
    assert!(decode_token(&c, key, "").is_none());
    assert!(decode_token(&c, key, "   ").is_none());
}

#[test]
fn decode_rejects_wrong_mac_key() {
    let c = cipher(b"token-mac-test");
    let key_a = b"key-A-32-bytes-padded___________";
    let key_b = b"key-B-32-bytes-padded-DIFFERENT_";
    let id: [u8; TOKEN_ID_BYTES] = [1, 2, 3, 4, 5, 6, 7, 8];
    let cover = encode_token(&c, key_a, &id);
    // Decoding with the matching key succeeds...
    assert_eq!(decode_token(&c, key_a, &cover).unwrap(), id);
    // ...and the wrong key returns None (the parse succeeds but the
    // HMAC tag mismatches).
    assert!(decode_token(&c, key_b, &cover).is_none());
}

#[test]
fn current_codec_uses_the_scope_key_not_legacy_cipher_permutations() {
    let key = b"mac-key-32-bytes-padded________";
    let id: [u8; TOKEN_ID_BYTES] = [9, 9, 9, 9, 9, 9, 9, 9];
    let c1 = cipher(b"conversation-1");
    let c2 = cipher(b"conversation-2");
    let cover1 = encode_token(&c1, key, &id);
    let cover2 = encode_token(&c2, key, &id);
    // The current bigram format has one global language model. Scope
    // separation is supplied by `key` in production; ConversationCipher
    // remains in this API only so legacy template covers can be decoded.
    assert_eq!(cover1, cover2);
    assert_eq!(decode_token(&c2, key, &cover1), Some(id));
}

#[test]
fn decode_rejects_noncanonical_extension() {
    let c = cipher(b"token-canonical-test");
    let key = b"mac-key-32-bytes-padded________";
    let id: [u8; TOKEN_ID_BYTES] = [0x42; TOKEN_ID_BYTES];
    let cover = encode_token(&c, key, &id);
    assert_eq!(decode_token(&c, key, &cover), Some(id));

    // `lol` is in the embedded vocabulary. Before the canonical-form
    // check, extra words could narrow the already-pinned interval while
    // preserving the same tag-valid 96-bit payload.
    let extended = format!("{cover} lol");
    assert!(decode_token(&c, key, &extended).is_none());
}

#[test]
fn many_ids_all_roundtrip() {
    let c = cipher(b"token-bulk-test");
    let key = b"mac-key-32-bytes-padded________";
    for n in 0u64..256 {
        let id = n.to_be_bytes();
        let cover = encode_token(&c, key, &id);
        let got = decode_token(&c, key, &cover).expect("each id roundtrips");
        assert_eq!(got, id);
    }
}
