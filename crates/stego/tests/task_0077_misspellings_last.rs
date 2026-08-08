//! Task 0077 — add misspellings as the fourth and last shrinking layer.
//!
//! This is deliberately the only layer that writes visibly incorrect words,
//! so its typed switch defaults to off until Liam has looked at the output.
//! The test proves the same 200-character private message is addressed by
//! both forms, the on form uses fewer cover words, and off restores the exact
//! original count and cover.

use std::collections::HashMap;

use serde::Deserialize;
use stego::{
    decode_shrunk_token, encode_shrunk_token_misspellings, misspell, word_bank_grown,
    MisspellingChoice, SHRUNK_TOKEN_ID_BYTES, SHRUNK_TOKEN_PAYLOAD_BITS,
};

const EXPECTED_PRIVATE_MESSAGE_CHARS: usize = 200;
const EXPECTED_OFF_WORDS: usize = 10;
const EXPECTED_ON_WORDS: usize = 9;
const EXPECTED_REDUCTION: usize = EXPECTED_OFF_WORDS - EXPECTED_ON_WORDS;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FreshInstallSettings {
    misspellings: MisspellingChoice,
}

fn private_message_200_chars() -> String {
    let mut message = String::new();
    while message.len() < EXPECTED_PRIVATE_MESSAGE_CHARS {
        message.push_str(
            "private task 0077 note: pair alpha keeps the draft local and leaves visible misspellings off until Liam reviews them. ",
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
fn task_0077_misspelling_switch_is_last_default_off_layer() {
    let shared_key = b"task-0077-paired-shared-key";
    let private_message = private_message_200_chars();
    assert_eq!(
        private_message.chars().count(),
        EXPECTED_PRIVATE_MESSAGE_CHARS
    );

    // An absent settings file and a legacy settings object with no new field
    // must both resolve to Off on a fresh install/update.
    let fresh_default = MisspellingChoice::default();
    let missing_field: FreshInstallSettings =
        serde_json::from_str("{}").expect("fresh settings object deserializes");

    let handle: [u8; SHRUNK_TOKEN_ID_BYTES] = *b"msg0077a";
    let mut store: HashMap<[u8; SHRUNK_TOKEN_ID_BYTES], String> = HashMap::new();
    store.insert(handle, private_message.clone());

    // Switch OFF: byte-for-byte task-0076 grown-bank output, 8 bits/word.
    let off_cover = encode_shrunk_token_misspellings(shared_key, &handle, fresh_default);
    let off_words = off_cover.split_ascii_whitespace().count();

    // Switch ON: 256 correct + 256 misspelled forms, 9 bits/word.
    let on_cover = encode_shrunk_token_misspellings(shared_key, &handle, MisspellingChoice::On);
    let on_words = on_cover.split_ascii_whitespace().count();
    let on_forms = misspell::parse_misspelled_forms(&on_cover)
        .expect("misspelling-on cover uses only registered written forms");
    let visible_misspellings = misspell::misspelled_count(&on_forms);

    // Switch OFF again: both the count and the layer-3 cover must return.
    let off_again_cover =
        encode_shrunk_token_misspellings(shared_key, &handle, MisspellingChoice::Off);
    let off_again_words = off_again_cover.split_ascii_whitespace().count();
    let reduction = off_words - on_words;

    let off_handle = decode_shrunk_token(shared_key, &off_cover)
        .expect("misspelling-off cover decodes to the carried handle");
    let on_handle = decode_shrunk_token(shared_key, &on_cover)
        .expect("misspelling-on cover decodes to the carried handle");
    let off_recovered = store
        .get(&off_handle)
        .expect("off handle addresses the private message");
    let on_recovered = store
        .get(&on_handle)
        .expect("on handle addresses the private message");

    println!("TASK0077 payload_bits={SHRUNK_TOKEN_PAYLOAD_BITS}");
    println!(
        "TASK0077 forms_off={} word_bits_off={} forms_on={} word_bits_on={}",
        word_bank_grown::GROWN_BANK_SIZE,
        word_bank_grown::GROWN_BANK_WORD_BITS,
        misspell::MISSPELL_FORMS,
        misspell::MISSPELL_WORD_BITS
    );
    println!(
        "TASK0077 private_message_chars={}",
        private_message.chars().count()
    );
    println!("TASK0077 private_message={private_message}");
    println!("TASK0077 shared_handle_hex={}", hex_bytes(&handle));
    println!("TASK0077 fresh_install_default={}", fresh_default.label());
    println!(
        "TASK0077 missing_settings_field_default={}",
        missing_field.misspellings.label()
    );
    println!("TASK0077 switch_off_words={off_words}");
    println!("TASK0077 switch_off_cover={off_cover}");
    println!("TASK0077 switch_on_words={on_words}");
    println!("TASK0077 switch_on_cover={on_cover}");
    println!("TASK0077 visible_misspellings={visible_misspellings}");
    println!("TASK0077 word_count_reduction={reduction}");
    println!("TASK0077 switch_off_again_words={off_again_words}");
    println!(
        "TASK0077 off_count_restored_exactly={}",
        off_again_words == off_words
    );
    println!(
        "TASK0077 off_cover_restored_exactly={}",
        off_again_cover == off_cover
    );
    println!(
        "TASK0077 off_recovered_original={}",
        off_recovered == &private_message
    );
    println!(
        "TASK0077 on_recovered_original={}",
        on_recovered == &private_message
    );
    println!("TASK0077 switch_off_recovered_text={off_recovered}");
    println!("TASK0077 switch_on_recovered_text={on_recovered}");

    assert_eq!(fresh_default, MisspellingChoice::Off);
    assert_eq!(missing_field.misspellings, MisspellingChoice::Off);
    assert_eq!(off_words, EXPECTED_OFF_WORDS);
    assert_eq!(on_words, EXPECTED_ON_WORDS);
    assert_eq!(reduction, EXPECTED_REDUCTION);
    assert!(
        visible_misspellings > 0,
        "the on cover must visibly exercise the misspelling layer"
    );
    assert_eq!(off_again_words, off_words);
    assert_eq!(off_again_cover, off_cover);
    assert_eq!(off_recovered, &private_message);
    assert_eq!(on_recovered, &private_message);
}
