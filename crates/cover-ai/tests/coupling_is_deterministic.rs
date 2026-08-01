//! The prose carrier is a canonical rendering of the token payload, not
//! separately generated cover text.  These are intentionally black-box
//! properties of the public stego API.

use stego::{decode_token, encode_token, ConversationCipher, TOKEN_ID_BYTES};

fn cipher(salt: &[u8]) -> ConversationCipher {
    ConversationCipher::from_salt(salt)
}

#[test]
fn cover_text_is_canonical_for_a_scoped_pointer() {
    let id: [u8; TOKEN_ID_BYTES] = [0x9a, 0x17, 0x53, 0xce, 0x42, 0x00, 0x7d, 0xf1];
    let mac_key = b"coupling-test-scope-key";
    let first_cipher = cipher(b"first-conversation-cipher");
    let second_cipher = cipher(b"second-conversation-cipher");

    let first = encode_token(&first_cipher, mac_key, &id);
    let repeated = encode_token(&first_cipher, mac_key, &id);
    let across_cipher = encode_token(&second_cipher, mac_key, &id);

    assert_eq!(first, repeated, "the same scoped pointer must render identically");
    assert_eq!(first, across_cipher, "the cipher parameter must not alter the cover");
    assert_eq!(decode_token(&first_cipher, mac_key, &first), Some(id));

    let mut words: Vec<_> = first.split_ascii_whitespace().map(str::to_owned).collect();
    assert!(!words.is_empty(), "a rendered cover must contain at least one word");
    words[0] = if words[0] == "lol" { "yeah" } else { "lol" }.to_owned();
    let one_word_edit = words.join(" ");

    assert_eq!(
        decode_token(&first_cipher, mac_key, &one_word_edit),
        None,
        "a one-word edit must no longer decode as the pointer"
    );
}

#[test]
fn cover_text_changes_when_the_scope_key_changes() {
    let id: [u8; TOKEN_ID_BYTES] = [0x9a, 0x17, 0x53, 0xce, 0x42, 0x00, 0x7d, 0xf1];
    let cover = encode_token(&cipher(b"test-cipher"), b"first-scope-key", &id);
    let other = encode_token(&cipher(b"test-cipher"), b"second-scope-key", &id);

    assert_ne!(cover, other, "the scope key is part of the rendered token payload");
}
