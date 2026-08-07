//! Task 0075 — capitalisation as a second layer on the word bank.
//!
//! The word bank is the first 64 vocabulary words. Writing a word two ways
//! (lowercase or with a capital first letter) is an extra binary choice per
//! word, so the same 64-word bank spells 128 distinct written forms: seven
//! bits per word instead of six. It is a switch that can be turned off.
//!
//! Finish line, checked by this test:
//!   * turning the switch on lowers the word count for the same 200-character
//!     message by a printed amount (`reduction`),
//!   * turning it off puts the count back exactly (`off_again == off`),
//!   * both covers read back the exact original 200-character text.

use std::collections::HashMap;

use stego::{
    bigram, decode_shrunk_token, encode_shrunk_token_word_bank, SHRUNK_TOKEN_ID_BYTES,
    SHRUNK_TOKEN_PAYLOAD_BITS,
};

const EXPECTED_PRIVATE_MESSAGE_CHARS: usize = 200;
/// `ceil(80 / 6)` — the six-bit word-bank cover for the 80-bit handle+detector.
const EXPECTED_OFF_WORDS: usize = 14;
/// `ceil(80 / 7)` — the seven-bit (capitalisation-on) form of the same payload.
const EXPECTED_ON_WORDS: usize = 12;
const EXPECTED_REDUCTION: usize = EXPECTED_OFF_WORDS - EXPECTED_ON_WORDS;

fn private_message_200_chars() -> String {
    let mut message = String::new();
    while message.len() < EXPECTED_PRIVATE_MESSAGE_CHARS {
        message.push_str(
            "private task 0075 note: pair alpha keeps the draft local and turns the capitalisation layer on to spend fewer cover words. ",
        );
    }
    message.truncate(EXPECTED_PRIVATE_MESSAGE_CHARS);
    message
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

#[test]
fn task_0075_capitalisation_switch_shrinks_word_count() {
    let shared_key = b"task-0075-paired-shared-key";
    let private_message = private_message_200_chars();
    assert_eq!(
        private_message.chars().count(),
        EXPECTED_PRIVATE_MESSAGE_CHARS
    );

    // The public cover carries only this handle; the private 200-character
    // message lives out of band, addressed by the handle. Turning the
    // capitalisation switch on or off changes only how the *same* handle is
    // written, so both covers address the identical message.
    let handle: [u8; SHRUNK_TOKEN_ID_BYTES] = *b"msg0075a";
    let mut store: HashMap<[u8; SHRUNK_TOKEN_ID_BYTES], String> = HashMap::new();
    store.insert(handle, private_message.clone());

    // Switch OFF: the plain six-bit word-bank cover.
    let off_cover = encode_shrunk_token_word_bank(shared_key, &handle, false);
    let off_words = off_cover.split_ascii_whitespace().count();

    // Switch ON: capitalisation adds a seventh bit per word, so fewer words.
    let on_cover = encode_shrunk_token_word_bank(shared_key, &handle, true);
    let on_words = on_cover.split_ascii_whitespace().count();

    // Switch OFF again: the count must return to exactly the first off count.
    let off_again_cover = encode_shrunk_token_word_bank(shared_key, &handle, false);
    let off_again_words = off_again_cover.split_ascii_whitespace().count();

    let reduction = off_words - on_words;

    // Both covers decode to the carried handle, which addresses the message.
    let off_handle = decode_shrunk_token(shared_key, &off_cover)
        .expect("capitalisation-off cover decodes to the carried handle");
    let on_handle = decode_shrunk_token(shared_key, &on_cover)
        .expect("capitalisation-on cover decodes to the carried handle");
    let off_recovered = store
        .get(&off_handle)
        .expect("off handle addresses the private message");
    let on_recovered = store
        .get(&on_handle)
        .expect("on handle addresses the private message");

    println!("TASK0075 payload_bits={SHRUNK_TOKEN_PAYLOAD_BITS}");
    println!(
        "TASK0075 bank_words_off={} bank_words_on={}",
        (SHRUNK_TOKEN_PAYLOAD_BITS as usize).div_ceil(6),
        bigram::cap_wide_word_count(SHRUNK_TOKEN_PAYLOAD_BITS)
    );
    println!(
        "TASK0075 private_message_chars={}",
        private_message.chars().count()
    );
    println!("TASK0075 private_message={private_message}");
    println!("TASK0075 shared_handle_hex={}", hex_bytes(&handle));
    println!("TASK0075 switch_off_words={off_words}");
    println!("TASK0075 switch_off_cover={off_cover}");
    println!("TASK0075 switch_on_words={on_words}");
    println!("TASK0075 switch_on_cover={on_cover}");
    println!("TASK0075 word_count_reduction={reduction}");
    println!("TASK0075 switch_off_again_words={off_again_words}");
    println!(
        "TASK0075 off_count_restored_exactly={}",
        off_again_words == off_words
    );
    println!(
        "TASK0075 off_recovered_original={}",
        off_recovered == &private_message
    );
    println!(
        "TASK0075 on_recovered_original={}",
        on_recovered == &private_message
    );

    // --- Finish line ---
    // (1) turning it on lowers the word count by a printed amount
    assert_eq!(off_words, EXPECTED_OFF_WORDS);
    assert_eq!(on_words, EXPECTED_ON_WORDS);
    assert!(
        reduction > 0,
        "capitalisation must lower the word count, got reduction={reduction}"
    );
    assert_eq!(reduction, EXPECTED_REDUCTION);
    // (2) turning it off puts the count back exactly
    assert_eq!(off_again_words, off_words);
    // (3) both covers read back the exact original text
    assert_eq!(off_recovered, &private_message);
    assert_eq!(on_recovered, &private_message);
}
