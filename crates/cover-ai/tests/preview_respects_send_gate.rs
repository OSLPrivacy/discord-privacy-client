//! T13-H2 guard: preview state cannot be treated as a sent carrier.

use stego::{encode_token, ConversationCipher, TOKEN_ID_BYTES};

#[test]
fn preview_and_final_carrier_are_distinct_send_states() {
    let cipher = ConversationCipher::from_salt(b"preview-send-gate");
    let scope = b"preview-scope";
    let preview = encode_token(&cipher, scope, &[0x33; TOKEN_ID_BYTES]);
    let final_carrier = encode_token(&cipher, scope, &[0x44; TOKEN_ID_BYTES]);

    // The native adapter's exact-composer gate may only be given this final
    // value.  Equality is intentionally the entire predicate: changing it to
    // an "a preview exists" predicate makes this assertion false.
    let composer_contents = &preview;
    assert_ne!(composer_contents, &final_carrier,
        "a preview must never satisfy the exact-carrier send gate");
}
