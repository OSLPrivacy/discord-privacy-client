//! T1-T33: pointer-v2 has enough shaped cover capacity for its full 24-byte wire.

use stego::{
    bigram::WIDE_TOKEN_WORDS, decode_token, encode_token_shaped, rendered_rows,
    ConversationCipher, RowBudget, TOKEN_PAYLOAD_BITS,
};

#[test]
fn t1_t33_wide_pointer_round_trips_through_the_rebudgeted_cover() {
    let pointer = [0x5a; 20];
    let cipher = ConversationCipher::from_salt(b"t1-t33-cover-capacity");
    let detect_key = b"t1-t33-private-detect-key";
    let shaped = encode_token_shaped(&cipher, detect_key, &pointer, RowBudget::new(1, 40));

    assert_eq!(TOKEN_PAYLOAD_BITS, 192);
    assert_eq!(WIDE_TOKEN_WORDS, 32);
    assert_eq!(shaped.text().split_whitespace().count(), WIDE_TOKEN_WORDS);
    assert_eq!(shaped.rows, rendered_rows(shaped.text(), 40));
    assert_eq!(decode_token(&cipher, detect_key, shaped.text()), Some(pointer));
}
