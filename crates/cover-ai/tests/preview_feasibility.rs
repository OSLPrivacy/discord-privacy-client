//! T13-H1 feasibility measurement.
//!
//! A carrier is the canonical encoding of a pointer.  Before upload there is
//! no pointer, so a preview cannot be represented as a carrier which remains
//! valid after upload.  Sending such a preview would expose a carrier whose
//! blob does not exist, and recipients are deliberately not allowed to retry
//! that failure.

use stego::{decode_token, encode_token, ConversationCipher, TOKEN_ID_BYTES};

#[test]
fn a_preview_without_the_final_pointer_cannot_be_the_final_carrier() {
    let cipher = ConversationCipher::from_salt(b"preview-feasibility");
    let scope = b"preview-scope";
    let preview_pointer = [0x11; TOKEN_ID_BYTES];
    let uploaded_pointer = [0x22; TOKEN_ID_BYTES];

    let preview = encode_token(&cipher, scope, &preview_pointer);
    let final_carrier = encode_token(&cipher, scope, &uploaded_pointer);

    assert_ne!(preview, final_carrier, "a carrier changes with its pointer");
    assert_eq!(
        decode_token(&cipher, scope, &preview),
        Some(preview_pointer)
    );
    assert_eq!(
        decode_token(&cipher, scope, &final_carrier),
        Some(uploaded_pointer)
    );
}
