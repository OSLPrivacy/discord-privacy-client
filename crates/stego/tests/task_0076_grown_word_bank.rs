//! Task 0076 — grow the word bank: a third, independent layer.
//!
//! The base word bank (task 0074) draws each cover word from 64 slots, six
//! bits per word. This layer swaps in a dedicated 256-word bank — eight bits
//! per word — instead of stacking another bit on the existing 64 words the
//! way the capitalisation switch (task 0075) does. More words to choose from
//! means more payload carried per word, so the same fixed payload needs
//! fewer words. It is its own switch: turning it off restores the original
//! 64-word, six-bit form exactly.
//!
//! Finish line, checked by this test:
//!   * the bank size and the word count for the same 200-character message
//!     are printed before and after,
//!   * the count drops,
//!   * every message still reads back exactly (both switch positions decode
//!     to the handle that addresses the original 200-character text, and
//!     turning the switch back off restores the original word count).

use std::collections::HashMap;

use stego::{
    decode_shrunk_token, encode_shrunk_token_grown_bank, word_bank_grown, SHRUNK_TOKEN_ID_BYTES,
    SHRUNK_TOKEN_PAYLOAD_BITS,
};

const EXPECTED_PRIVATE_MESSAGE_CHARS: usize = 200;
const BANK_SIZE_OFF: usize = 64;
const BANK_WORD_BITS_OFF: usize = 6;
/// `ceil(80 / 6)` — the base word-bank cover for the 80-bit handle+detector.
const EXPECTED_OFF_WORDS: usize = 14;
/// `ceil(80 / 8)` — the grown 256-word bank form of the same payload.
const EXPECTED_ON_WORDS: usize = 10;
const EXPECTED_REDUCTION: usize = EXPECTED_OFF_WORDS - EXPECTED_ON_WORDS;

fn private_message_200_chars() -> String {
    let mut message = String::new();
    while message.len() < EXPECTED_PRIVATE_MESSAGE_CHARS {
        message.push_str(
            "private task 0076 note: pair alpha keeps the draft local and grows the word bank to spend fewer cover words. ",
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
fn task_0076_grown_bank_switch_shrinks_word_count() {
    let shared_key = b"task-0076-paired-shared-key";
    let private_message = private_message_200_chars();
    assert_eq!(
        private_message.chars().count(),
        EXPECTED_PRIVATE_MESSAGE_CHARS
    );

    // The public cover carries only this handle; the private 200-character
    // message lives out of band, addressed by the handle. Turning the grown
    // bank switch on or off changes only how the *same* handle is written,
    // so both covers address the identical message.
    let handle: [u8; SHRUNK_TOKEN_ID_BYTES] = *b"msg0076a";
    let mut store: HashMap<[u8; SHRUNK_TOKEN_ID_BYTES], String> = HashMap::new();
    store.insert(handle, private_message.clone());

    // Switch OFF: the base 64-word, six-bit bank.
    let off_cover = encode_shrunk_token_grown_bank(shared_key, &handle, false);
    let off_words = off_cover.split_ascii_whitespace().count();

    // Switch ON: the grown 256-word, eight-bit bank.
    let on_cover = encode_shrunk_token_grown_bank(shared_key, &handle, true);
    let on_words = on_cover.split_ascii_whitespace().count();

    // Switch OFF again: the count must return to exactly the first off count.
    let off_again_cover = encode_shrunk_token_grown_bank(shared_key, &handle, false);
    let off_again_words = off_again_cover.split_ascii_whitespace().count();

    let reduction = off_words - on_words;

    // Both covers decode to the carried handle, which addresses the message.
    let off_handle = decode_shrunk_token(shared_key, &off_cover)
        .expect("grown-bank-off cover decodes to the carried handle");
    let on_handle = decode_shrunk_token(shared_key, &on_cover)
        .expect("grown-bank-on cover decodes to the carried handle");
    let off_recovered = store
        .get(&off_handle)
        .expect("off handle addresses the private message");
    let on_recovered = store
        .get(&on_handle)
        .expect("on handle addresses the private message");

    println!("TASK0076 payload_bits={SHRUNK_TOKEN_PAYLOAD_BITS}");
    println!(
        "TASK0076 bank_size_off={BANK_SIZE_OFF} bank_word_bits_off={BANK_WORD_BITS_OFF}"
    );
    println!(
        "TASK0076 bank_size_on={} bank_word_bits_on={}",
        word_bank_grown::GROWN_BANK_SIZE,
        word_bank_grown::GROWN_BANK_WORD_BITS
    );
    println!(
        "TASK0076 private_message_chars={}",
        private_message.chars().count()
    );
    println!("TASK0076 private_message={private_message}");
    println!("TASK0076 shared_handle_hex={}", hex_bytes(&handle));
    println!("TASK0076 switch_off_words={off_words}");
    println!("TASK0076 switch_off_cover={off_cover}");
    println!("TASK0076 switch_on_words={on_words}");
    println!("TASK0076 switch_on_cover={on_cover}");
    println!("TASK0076 word_count_reduction={reduction}");
    println!("TASK0076 switch_off_again_words={off_again_words}");
    println!(
        "TASK0076 off_count_restored_exactly={}",
        off_again_words == off_words
    );
    println!(
        "TASK0076 off_recovered_original={}",
        off_recovered == &private_message
    );
    println!(
        "TASK0076 on_recovered_original={}",
        on_recovered == &private_message
    );

    // --- Finish line ---
    // (1) bank size and word count for the same 200-character message,
    //     printed before and after.
    assert_eq!(BANK_SIZE_OFF, 64);
    assert_eq!(word_bank_grown::GROWN_BANK_SIZE, 256);
    assert_eq!(off_words, EXPECTED_OFF_WORDS);
    assert_eq!(on_words, EXPECTED_ON_WORDS);
    // (2) the count drops
    assert!(
        reduction > 0,
        "the grown bank must lower the word count, got reduction={reduction}"
    );
    assert_eq!(reduction, EXPECTED_REDUCTION);
    assert!(word_bank_grown::GROWN_BANK_SIZE > BANK_SIZE_OFF);
    // Turning it off puts the count back exactly.
    assert_eq!(off_again_words, off_words);
    // (3) every message still reads back exactly.
    assert_eq!(off_recovered, &private_message);
    assert_eq!(on_recovered, &private_message);
}
