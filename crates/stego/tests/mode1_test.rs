//! Integration tests for Mode 1: cross-binary round-trips, length
//! caps against Discord's 2000-char limit, and per-message
//! independence (the design's hard architectural invariant — a
//! decoder must not need any other message to decode a given one).

use stego::{
    decode_mode1, encode_mode1, is_mode1, ConversationCipher, Error, MODE1_MAX_RAW_LEN,
    MODE1_PREFIX,
};

fn cipher(salt: &[u8]) -> ConversationCipher {
    ConversationCipher::from_salt(salt)
}

#[test]
fn fits_under_discord_text_cap_at_max_raw() {
    // 9-B1a: MAX re-tuned to 100 raw bytes so the encoded cover
    // stays under Discord's free-tier 2000-char text-message cap.
    let c = cipher(b"length-cap-test");
    let payload = vec![0xABu8; MODE1_MAX_RAW_LEN];
    let s = encode_mode1(&c, &payload).unwrap();
    assert!(
        s.chars().count() < 2000,
        "Mode 1 at max raw len produced {} chars (>= 2000 — overruns free-tier cap)",
        s.chars().count()
    );
    let got = decode_mode1(&c, &s).unwrap();
    assert_eq!(got, payload);
}

#[test]
fn per_message_independence_each_message_decodes_alone() {
    // The hard invariant: each stego'd message decodes from itself
    // + the conversation cipher, with NO reference to any other
    // message. Discord can reorder / edit / delete messages on its
    // CDN; context-dependent stego breaks the moment a context
    // message is lost. Here we encode three independent payloads,
    // shuffle their textual order, and confirm each decodes.
    let c = cipher(b"independence-test");
    let payloads: Vec<Vec<u8>> = vec![
        b"alpha".to_vec(),
        b"second message".to_vec(),
        b"third!".to_vec(),
    ];
    let encoded: Vec<String> = payloads
        .iter()
        .map(|p| encode_mode1(&c, p).unwrap())
        .collect();

    // Shuffle: decode in reverse order; if the decoder secretly
    // depended on prior state, this would fail.
    for (text, expected) in encoded.iter().rev().zip(payloads.iter().rev()) {
        let got = decode_mode1(&c, text).unwrap();
        assert_eq!(got, *expected);
    }
}

#[test]
fn cross_cipher_decode_does_not_recover_payload() {
    // The decoder treats a Mode 1 message produced under cipher A
    // as cover text under cipher B's permutations. The output is
    // NEVER the original bytes (would defeat per-conversation
    // salt's purpose).
    let a = cipher(b"alice-bob-session");
    let b = cipher(b"alice-carol-session");
    let payload = b"the secret is forty two";
    let encoded = encode_mode1(&a, payload).unwrap();
    match decode_mode1(&b, &encoded) {
        Ok(got) => assert_ne!(got, payload),
        Err(_) => {} // also acceptable
    }
}

#[test]
fn detects_mode1_vs_mode0_prefix() {
    let c = cipher(b"x");
    let mode1 = encode_mode1(&c, b"hi").unwrap();
    assert!(is_mode1(&mode1));
    assert!(mode1.starts_with(MODE1_PREFIX));

    // Mode 0 prefix is not Mode 1.
    assert!(!is_mode1("DPC0::abcd"));
    assert!(!is_mode1("plain"));
}

#[test]
fn round_trip_random_byte_distributions() {
    let c = cipher(b"distribution-test");
    // Various byte distributions: all-zero, all-FF, alternating,
    // shuffled.
    for case in [
        vec![0u8; 7],
        vec![0xFF; 16],
        vec![0xAA, 0x55, 0xAA, 0x55, 0xAA],
        (0..32u8).collect::<Vec<_>>(),
        (0..MODE1_MAX_RAW_LEN as u8).collect::<Vec<_>>(),
    ] {
        let s = encode_mode1(&c, &case).unwrap();
        let got = decode_mode1(&c, &s).unwrap();
        assert_eq!(got, case, "len={}", case.len());
    }
}

// ---- 9-B1a: MODE1_MAX_RAW_LEN tuned to 100 for free-tier fit. ----

#[test]
fn b1_max_raw_len_is_100() {
    assert_eq!(MODE1_MAX_RAW_LEN, 100);
}

#[test]
fn b1_roundtrip_at_exact_100_byte_payload() {
    let c = cipher(b"b1-boundary");
    let payload: Vec<u8> = (0..100u8).collect();
    assert_eq!(payload.len(), MODE1_MAX_RAW_LEN);
    let s = encode_mode1(&c, &payload).unwrap();
    let got = decode_mode1(&c, &s).unwrap();
    assert_eq!(got, payload);
    // 9-B1a: at max raw the cover stays under Discord's free-tier
    // 2000-char cap (≈1700 chars in practice).
    assert!(
        s.chars().count() < 2000,
        "100-byte payload produced {} chars (>= 2000)",
        s.chars().count()
    );
}

#[test]
fn encode_at_new_max_capacity_100_bytes_produces_under_2000_chars() {
    // Explicit regression guard for the 9-B1a free-tier-fit target.
    // Multiple salts to confirm the bound isn't an artifact of one
    // wordlist permutation.
    for salt in [b"salt-1" as &[u8], b"salt-2", b"salt-3"] {
        let c = cipher(salt);
        let payload = vec![0xAAu8; 100];
        let s = encode_mode1(&c, &payload).unwrap();
        assert!(
            s.chars().count() < 2000,
            "salt={:?} produced {} chars",
            salt,
            s.chars().count()
        );
    }
}

#[test]
fn b1_rejects_101_byte_payload_with_too_long_error() {
    let c = cipher(b"b1-oversize");
    let payload = vec![0xCDu8; 101];
    match encode_mode1(&c, &payload) {
        Err(Error::Mode1TooLong { got, max }) => {
            assert_eq!(got, 101);
            assert_eq!(max, MODE1_MAX_RAW_LEN);
            assert_eq!(max, 100);
        }
        other => panic!("expected Mode1TooLong, got {other:?}"),
    }
}

#[test]
fn rejects_corrupted_token_in_middle() {
    let c = cipher(b"corruption-test");
    let s = encode_mode1(&c, b"hello").unwrap();
    let body = s.strip_prefix(MODE1_PREFIX).unwrap();
    // Replace the first non-prefix word with garbage that's neither
    // a known wordlist entry nor a skeleton token.
    let mut tokens: Vec<&str> = body.split_whitespace().collect();
    if !tokens.is_empty() {
        // Find a token that's a wordlist entry (i.e. not a fixed
        // skeleton word). Simplest: replace the second-to-last
        // token; many templates have a slot just before the period.
        let len = tokens.len();
        if len >= 2 {
            tokens[len - 2] = "ZZZZUNKNOWN";
        }
    }
    let corrupted = format!("{MODE1_PREFIX}{}", tokens.join(" "));
    match decode_mode1(&c, &corrupted) {
        Err(Error::Mode1ParseError(_)) => {}
        Ok(_) => panic!("expected parse error, decode succeeded"),
        Err(other) => panic!("expected Mode1ParseError, got {other:?}"),
    }
}
