use stego::{
    decode_mode0, encode_mode0, is_mode0, Error, MODE0_MAX_RAW_LEN, MODE0_PREFIX,
};

#[test]
fn round_trip_empty_payload() {
    let wire = encode_mode0(&[]).unwrap();
    assert_eq!(wire, MODE0_PREFIX);
    let recovered = decode_mode0(&wire).unwrap();
    assert!(recovered.is_empty());
}

#[test]
fn round_trip_arbitrary_bytes() {
    let payload: Vec<u8> = (0u8..=255u8).collect();
    let wire = encode_mode0(&payload).unwrap();
    assert!(is_mode0(&wire));
    let recovered = decode_mode0(&wire).unwrap();
    assert_eq!(recovered, payload);
}

#[test]
fn round_trip_at_max_raw_len() {
    let payload = vec![0xCDu8; MODE0_MAX_RAW_LEN];
    let wire = encode_mode0(&payload).unwrap();
    let recovered = decode_mode0(&wire).unwrap();
    assert_eq!(recovered, payload);
}

#[test]
fn rejects_oversized_payload() {
    let payload = vec![0u8; MODE0_MAX_RAW_LEN + 1];
    let res = encode_mode0(&payload);
    assert!(matches!(
        res,
        Err(Error::Mode0TooLong { got, max })
            if got == MODE0_MAX_RAW_LEN + 1 && max == MODE0_MAX_RAW_LEN
    ));
}

#[test]
fn detects_prefix_correctly() {
    assert!(is_mode0("DPC0::AAAA"));
    assert!(is_mode0("DPC0::"));
    assert!(!is_mode0(""));
    assert!(!is_mode0("hello world"));
    assert!(!is_mode0("DPC1::AAAA"));
    assert!(!is_mode0("dpc0::AAAA"), "case-sensitive prefix");
    assert!(!is_mode0(" DPC0::AAAA"), "leading whitespace must not match");
}

#[test]
fn decode_rejects_non_mode0_prefix() {
    assert!(matches!(decode_mode0("hello"), Err(Error::NotMode0)));
    assert!(matches!(decode_mode0("DPC1::AAAA"), Err(Error::NotMode0)));
    assert!(matches!(decode_mode0(""), Err(Error::NotMode0)));
}

#[test]
fn decode_rejects_invalid_base64() {
    let bad = format!("{MODE0_PREFIX}!!!not-base64!!!");
    assert!(matches!(decode_mode0(&bad), Err(Error::Mode0Base64(_))));
}

#[test]
fn discord_safe_charset() {
    // Confirm Mode 0 only emits characters that survive Discord's
    // text rendering: alphanumerics, +, /, =, plus the prefix.
    let payload = vec![0xFFu8; 100];
    let wire = encode_mode0(&payload).unwrap();
    for c in wire.chars() {
        assert!(
            c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=' | ':'),
            "Mode 0 emitted non-Discord-safe char {c:?}"
        );
    }
}

#[test]
fn fits_under_discord_2000_char_limit() {
    // Worst case: encode the cap; verify wire length stays under
    // Discord's 2000-char message limit.
    let payload = vec![0xEEu8; 1400];
    let wire = encode_mode0(&payload).unwrap();
    assert!(
        wire.chars().count() < 2000,
        "Mode 0 wire {} chars exceeds Discord's 2000-char limit",
        wire.chars().count()
    );
}

#[test]
fn per_message_independence_each_message_decodes_alone() {
    // Per-message independence is a hard architectural requirement.
    // Encode three independent payloads; decode them in REVERSE order;
    // confirm decoding does not depend on prior messages.
    let payloads: Vec<Vec<u8>> = vec![
        b"first message".to_vec(),
        b"second message with different content".to_vec(),
        (0u8..=99u8).collect(),
    ];
    let wires: Vec<String> = payloads
        .iter()
        .map(|p| encode_mode0(p).unwrap())
        .collect();
    for (wire, expected) in wires.iter().zip(payloads.iter()).rev() {
        let recovered = decode_mode0(wire).unwrap();
        assert_eq!(&recovered, expected);
    }
}

#[test]
fn encoding_is_deterministic() {
    // Mode 0 is a pure base64 wrapper — same input must produce same
    // output every call (no random padding, no nonce, etc.).
    let payload = b"deterministic check";
    let a = encode_mode0(payload).unwrap();
    let b = encode_mode0(payload).unwrap();
    assert_eq!(a, b);
}
