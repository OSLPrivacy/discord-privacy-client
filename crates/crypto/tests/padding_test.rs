use crypto::padding::{max_text_plaintext_size, pad_text, unpad_text, TEXT_BUCKETS};

#[test]
fn round_trip_short() {
    let plaintext = b"hello";
    let padded = pad_text(plaintext).unwrap();
    assert_eq!(padded.len(), 64, "5-byte plaintext fits in 64-byte bucket");
    let recovered = unpad_text(&padded).unwrap();
    assert_eq!(recovered, plaintext);
}

#[test]
fn round_trip_each_bucket_at_max_capacity() {
    // Largest plaintext that fits in each bucket = bucket - 4 (length prefix).
    for &bucket in TEXT_BUCKETS {
        let plaintext_size = bucket - 4;
        let plaintext = vec![0xABu8; plaintext_size];
        let padded = pad_text(&plaintext).expect("pad");
        assert_eq!(padded.len(), bucket);
        let recovered = unpad_text(&padded).expect("unpad");
        assert_eq!(recovered, plaintext);
    }
}

#[test]
fn promotes_to_next_bucket_when_payload_pushes_past_boundary() {
    // 60 bytes + 4-byte prefix = 64 bytes, fits exactly in the 64-byte bucket.
    let exactly_60 = vec![0u8; 60];
    assert_eq!(pad_text(&exactly_60).unwrap().len(), 64);

    // 61 bytes + 4-byte prefix = 65 bytes; must promote to the 128-byte bucket.
    let over_60 = vec![0u8; 61];
    assert_eq!(pad_text(&over_60).unwrap().len(), 128);
}

#[test]
fn pad_rejects_oversized() {
    let too_big = vec![0u8; max_text_plaintext_size() + 1];
    assert!(pad_text(&too_big).is_err());
}

#[test]
fn pad_to_largest_bucket() {
    let max_size = max_text_plaintext_size();
    let plaintext = vec![0xCDu8; max_size];
    let padded = pad_text(&plaintext).unwrap();
    assert_eq!(padded.len(), 1024);
    let recovered = unpad_text(&padded).unwrap();
    assert_eq!(recovered, plaintext);
}

#[test]
fn empty_plaintext_uses_smallest_bucket() {
    let padded = pad_text(b"").unwrap();
    assert_eq!(padded.len(), 64);
    let recovered = unpad_text(&padded).unwrap();
    assert_eq!(recovered, b"");
}

#[test]
fn unpad_rejects_non_bucket_size() {
    let weird = vec![0u8; 100];
    assert!(unpad_text(&weird).is_err());
}

#[test]
fn unpad_rejects_truncated_below_length_prefix() {
    let truncated = vec![0u8; 3];
    assert!(unpad_text(&truncated).is_err());
}

#[test]
fn unpad_rejects_length_exceeding_capacity() {
    // Build a 64-byte bucket whose length prefix claims 9999 bytes of payload.
    let mut malformed = vec![0u8; 64];
    malformed[..4].copy_from_slice(&9999u32.to_be_bytes());
    assert!(unpad_text(&malformed).is_err());
}

#[test]
fn padding_is_deterministic_in_size_only() {
    // Same plaintext → same bucket size, but content layout (length prefix
    // + plaintext + zeros) is deterministic; this is fine because real
    // adversary observes only AEAD ciphertext, not pre-AEAD plaintext.
    let p = b"hello";
    let a = pad_text(p).unwrap();
    let b = pad_text(p).unwrap();
    assert_eq!(a, b);
}
