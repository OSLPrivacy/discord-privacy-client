//! T1-T32: the prose carrier is exactly a 160-bit seed plus detection tag.

use stego::{decode_token, encode_token, ConversationCipher, DETECT_TAG_BYTES, TOKEN_ID_BYTES, TOKEN_PAYLOAD_BITS};

#[test]
fn t1_t32_carrier_round_trips_a_24_byte_seed_plus_detect_tag_payload() {
    let pointer = [0xa5; TOKEN_ID_BYTES];
    let cipher = ConversationCipher::from_salt(b"t1-t32-pointer-wire");
    let detect_key = b"private-detect-key-for-t1-t32";

    assert_eq!(TOKEN_ID_BYTES, 20, "P must be 160 bits on the carrier");
    assert_eq!(DETECT_TAG_BYTES, 4, "detect_tag must be 32 bits");
    assert_eq!(TOKEN_PAYLOAD_BITS, 24 * 8, "the carrier is [P:20][detect_tag:4]");

    let cover = encode_token(&cipher, detect_key, &pointer);
    assert_eq!(decode_token(&cipher, detect_key, &cover), Some(pointer));
    assert!(decode_token(&cipher, b"different-detect-key", &cover).is_none());
}
