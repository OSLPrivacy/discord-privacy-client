//! Guarantees the Discord carrier row relies on.
//!
//! The row OSL types into Discord is the wordbank flagtext produced here, so
//! these tests pin the three properties the send path assumes: the encoding
//! round-trips exactly, the carrier is derived from the message rather than
//! being a constant, and the surface text reads as chat prose instead of an
//! obvious ciphertext blob.

use stego::{
    decode_mode1, decode_token, encode_mode1, encode_token, is_mode1, ConversationCipher,
    MODE1_MAX_RAW_LEN, MODE1_PREFIX, TOKEN_ID_BYTES,
};

/// The placeholder corpus the carrier used to be. It must never reappear.
const RETIRED_PLACEHOLDERS: [&str; 4] = [
    "OSL protected message.",
    "Private message protected.",
    "Protected placeholder.",
    "OSL private placeholder.",
];

fn cipher() -> ConversationCipher {
    ConversationCipher::from_salt(b"dm:1234567890123456789")
}

fn pointer(seed: u8) -> [u8; TOKEN_ID_BYTES] {
    let mut id = [0u8; TOKEN_ID_BYTES];
    for (index, byte) in id.iter_mut().enumerate() {
        *byte = seed
            .wrapping_mul(37)
            .wrapping_add((index as u8).wrapping_mul(11));
    }
    id
}

#[test]
fn wordbank_encoding_round_trips_the_exact_payload() {
    let cipher = cipher();
    for payload in [
        vec![0u8],
        vec![0xff; 7],
        b"encrypted-wire-bytes-stand-in".to_vec(),
        (0..MODE1_MAX_RAW_LEN as u8).collect::<Vec<_>>(),
    ] {
        let cover = encode_mode1(&cipher, &payload).expect("payload is within the wordbank cap");
        assert!(is_mode1(&cover));
        assert_eq!(
            decode_mode1(&cipher, &cover).expect("wordbank cover decodes"),
            payload,
            "the wordbank round trip must be byte-exact"
        );
    }
}

#[test]
fn prose_token_round_trips_the_exact_pointer() {
    let cipher = cipher();
    let mac_key = b"per-scope-mac-key";
    for seed in 0u8..24 {
        let id = pointer(seed);
        let flagtext = encode_token(&cipher, mac_key, &id);
        assert_eq!(
            decode_token(&cipher, mac_key, &flagtext),
            Some(id),
            "the carrier must decode back to the pointer it was built from"
        );
    }
}

#[test]
fn a_wrong_scope_key_recovers_nothing_rather_than_the_wrong_pointer() {
    let cipher = cipher();
    let flagtext = encode_token(&cipher, b"sender-scope-key", &pointer(3));
    assert_eq!(decode_token(&cipher, b"other-scope-key", &flagtext), None);
}

#[test]
fn the_carrier_is_never_a_constant_across_messages() {
    let cipher = cipher();
    let mac_key = b"per-scope-mac-key";
    let mut seen = Vec::new();
    for seed in 0u8..16 {
        let flagtext = encode_token(&cipher, mac_key, &pointer(seed));
        assert!(
            !seen.contains(&flagtext),
            "two different messages produced the same carrier"
        );
        seen.push(flagtext);
    }

    // The same holds for the payload-carrying wordbank encoder.
    let first = encode_mode1(&cipher, b"first message wire").expect("encodes");
    let second = encode_mode1(&cipher, b"second message wire").expect("encodes");
    assert_ne!(first, second);
}

#[test]
fn the_carrier_reads_as_chat_prose_not_base64_or_hex() {
    let cipher = cipher();
    let mac_key = b"per-scope-mac-key";
    for seed in 0u8..16 {
        let flagtext = encode_token(&cipher, mac_key, &pointer(seed));
        assert!(!flagtext.is_empty());
        // Marker-free: nothing for a content scanner to pattern-match on.
        assert!(!flagtext.starts_with(MODE1_PREFIX));
        assert!(!flagtext.contains("DPC"));
        assert!(!flagtext.contains("::"));
        // Base64 and hex alphabets never appear: no padding, no mixed case, no
        // digits, and every token is a pronounceable lowercase word.
        for character in flagtext.chars() {
            assert!(
                character.is_ascii_lowercase()
                    || matches!(character, ' ' | '\'' | '.' | ',' | '?' | '!'),
                "carrier character {character:?} is not chat prose"
            );
        }
        let words: Vec<&str> = flagtext.split_whitespace().collect();
        assert!(words.len() >= 8, "carrier should read as a sentence");
        assert!(
            words.iter().all(|word| word.chars().count() <= 12),
            "no word-shaped run long enough to look like an encoded blob"
        );
        // Single line: the send path types it without any line break.
        assert!(!flagtext.contains('\n'));
        assert!(!flagtext.contains('\r'));
        for placeholder in RETIRED_PLACEHOLDERS {
            assert!(!flagtext.contains(placeholder));
        }
    }
}

#[test]
fn the_pointer_bytes_never_appear_verbatim_in_the_carrier() {
    let cipher = cipher();
    let mac_key = b"per-scope-mac-key";
    for seed in 0u8..16 {
        let id = pointer(seed);
        let flagtext = encode_token(&cipher, mac_key, &id);
        assert!(
            !flagtext.as_bytes().windows(id.len()).any(|w| w == id),
            "the carrier must encode the pointer, not embed it"
        );
    }
}
