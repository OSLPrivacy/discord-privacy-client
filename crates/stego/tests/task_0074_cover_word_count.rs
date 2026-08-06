use std::collections::HashMap;

use hkdf::Hkdf;
use sha2::Sha256;
use stego::{
    bigram, compute_token_tag, decode_shrunk_token, decode_token, encode_shrunk_token,
    ConversationCipher, SHRUNK_TOKEN_ID_BYTES, TOKEN_ID_BYTES, TOKEN_PAYLOAD_BITS,
};

const SEED_HKDF_INFO: &[u8] = b"osl/task-0071/derived-cover-seed/v1";
const EXPECTED_PRIVATE_MESSAGE_CHARS: usize = 200;
const EXPECTED_BEFORE_WORDS: usize = 32;
const EXPECTED_AFTER_MAX_WORDS: usize = 14;

fn private_message_200_chars() -> String {
    let mut message = String::new();
    while message.len() < EXPECTED_PRIVATE_MESSAGE_CHARS {
        message.push_str(
            "private task 0074 note: pair alpha keeps the draft local and checks the smaller cover budget before sending. ",
        );
    }
    message.truncate(EXPECTED_PRIVATE_MESSAGE_CHARS);
    message
}

fn derive_seed(shared_key: &[u8], handle: &[u8; SHRUNK_TOKEN_ID_BYTES]) -> [u8; TOKEN_ID_BYTES] {
    let hk = Hkdf::<Sha256>::new(Some(handle), shared_key);
    let mut seed = [0u8; TOKEN_ID_BYTES];
    hk.expand(SEED_HKDF_INFO, &mut seed)
        .expect("HKDF expand to 20 bytes is infallible");
    seed
}

fn legacy_wide_cover(mac_key: &[u8], seed: &[u8; TOKEN_ID_BYTES]) -> String {
    let tag = compute_token_tag(mac_key, seed);
    let mut bits = Vec::with_capacity(TOKEN_PAYLOAD_BITS as usize);
    for &byte in seed.iter().chain(tag.iter()) {
        for offset in (0..8).rev() {
            bits.push((byte >> offset) & 1 == 1);
        }
    }
    bigram::render_words(&bigram::legacy_wide_decode_bits(&bits, TOKEN_PAYLOAD_BITS))
}

#[test]
fn task_0074_measures_cover_words_before_and_after_shrink() {
    let shared_key = b"task-0074-paired-shared-key";
    let cipher = ConversationCipher::from_salt(b"task-0074-cover-word-count");
    let private_message = private_message_200_chars();
    assert_eq!(
        private_message.chars().count(),
        EXPECTED_PRIVATE_MESSAGE_CHARS
    );

    let handle = *b"msg0074a";
    let seed = derive_seed(shared_key, &handle);
    let before_cover = legacy_wide_cover(shared_key, &seed);
    let after_cover = encode_shrunk_token(shared_key, &handle);
    let before_words = before_cover.split_ascii_whitespace().count();
    let after_words = after_cover.split_ascii_whitespace().count();

    let mut store = HashMap::new();
    store.insert(seed, private_message.clone());

    let before_seed = decode_token(&cipher, shared_key, &before_cover)
        .expect("pre-shrink legacy cover decodes to the carried seed");
    let before_recovered = store
        .get(&before_seed)
        .expect("pre-shrink seed addresses the private message");

    let after_handle = decode_shrunk_token(shared_key, &after_cover)
        .expect("post-shrink cover decodes to the carried handle");
    let after_seed = derive_seed(shared_key, &after_handle);
    let after_recovered = store
        .get(&after_seed)
        .expect("post-shrink derived seed addresses the private message");

    println!("TASK0074 expected_before_words={EXPECTED_BEFORE_WORDS}");
    println!("TASK0074 expected_after_max_words={EXPECTED_AFTER_MAX_WORDS}");
    println!(
        "TASK0074 private_message_chars={}",
        private_message.chars().count()
    );
    println!("TASK0074 private_message={private_message}");
    println!("TASK0074 shared_handle_hex={}", hex_bytes(&handle));
    println!("TASK0074 before_cover_words={before_words}");
    println!("TASK0074 before_cover_text={before_cover}");
    println!("TASK0074 after_cover_words={after_words}");
    println!("TASK0074 after_cover_text={after_cover}");
    println!(
        "TASK0074 same_message_recovered_before={}",
        before_recovered == &private_message
    );
    println!(
        "TASK0074 same_message_recovered_after={}",
        after_recovered == &private_message
    );

    assert_eq!(before_words, EXPECTED_BEFORE_WORDS);
    assert!(after_words <= EXPECTED_AFTER_MAX_WORDS);
    assert_eq!(before_recovered, &private_message);
    assert_eq!(after_recovered, &private_message);
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}
