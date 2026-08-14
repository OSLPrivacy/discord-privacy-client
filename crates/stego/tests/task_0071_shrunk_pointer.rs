use std::collections::HashMap;

use hkdf::Hkdf;
use sha2::Sha256;
use stego::{
    decode_shrunk_token, encode_shrunk_token, SHRUNK_TOKEN_ID_BYTES, SHRUNK_TOKEN_PAYLOAD_BITS,
    TOKEN_PAYLOAD_BITS,
};

const SEED_BYTES: usize = 20;
const SEED_HKDF_INFO: &[u8] = b"osl/task-0071/derived-cover-seed/v1";

fn private_message_500_chars() -> String {
    let mut message = String::new();
    while message.len() < 500 {
        message.push_str(
            "private build note: pair alpha confirms the window, keeps the draft local, ",
        );
    }
    message.truncate(500);
    message
}

fn derive_seed(shared_key: &[u8], handle: &[u8; SHRUNK_TOKEN_ID_BYTES]) -> [u8; SEED_BYTES] {
    let hk = Hkdf::<Sha256>::new(Some(handle), shared_key);
    let mut seed = [0u8; SEED_BYTES];
    hk.expand(SEED_HKDF_INFO, &mut seed)
        .expect("HKDF expand to 20 bytes is infallible");
    seed
}

#[test]
fn task_0071_500_char_message_roundtrips_with_80_cover_bits() {
    let shared_key = b"task-0071-paired-shared-key";
    let outsider_key = b"task-0071-outsider-wrong-key";
    let handle = *b"msg0071a";
    let private_message = private_message_500_chars();
    assert_eq!(private_message.chars().count(), 500);

    let seed = derive_seed(shared_key, &handle);
    let mut store = HashMap::new();
    store.insert(seed, private_message.clone());

    let cover_text = encode_shrunk_token(shared_key, &handle);
    let decoded_handle = decode_shrunk_token(shared_key, &cover_text).expect("paired key decodes");
    let recovered_seed = derive_seed(shared_key, &decoded_handle);
    let recovered = store
        .get(&recovered_seed)
        .expect("derived seed addresses stored private text");
    let outsider = decode_shrunk_token(outsider_key, &cover_text);

    println!(
        "TASK0071 private_message_chars={}",
        private_message.chars().count()
    );
    println!("TASK0071 old_cover_bits={TOKEN_PAYLOAD_BITS}");
    println!("TASK0071 new_cover_bits={SHRUNK_TOKEN_PAYLOAD_BITS}");
    println!("TASK0071 cover_text={cover_text}");
    println!(
        "TASK0071 recovered_matches={}",
        recovered == &private_message
    );
    println!("TASK0071 recovered_text={recovered}");
    println!(
        "TASK0071 outsider_decode={}",
        if outsider.is_none() { "None" } else { "Some" }
    );

    assert_eq!(TOKEN_PAYLOAD_BITS, 192);
    assert_eq!(SHRUNK_TOKEN_PAYLOAD_BITS, 80);
    assert_eq!(recovered, &private_message);
    assert!(outsider.is_none());
}
