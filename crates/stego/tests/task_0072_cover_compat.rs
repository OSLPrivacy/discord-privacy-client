use std::collections::HashMap;

use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use stego::{
    decode_cover_message_token, encode_shrunk_token, encode_token, ConversationCipher,
    CoverMessageToken, Error, NEW_COVER_MESSAGE_VERSION, OLD_COVER_MESSAGE_VERSION,
    SHRUNK_TOKEN_ID_BYTES, TOKEN_ID_BYTES,
};

const SEED_HKDF_INFO: &[u8] = b"osl/task-0071/derived-cover-seed/v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SavedCoverMessage {
    version: u8,
    cover_text: String,
    original_text: String,
}

fn derive_seed(shared_key: &[u8], handle: &[u8; SHRUNK_TOKEN_ID_BYTES]) -> [u8; TOKEN_ID_BYTES] {
    let hk = Hkdf::<Sha256>::new(Some(handle), shared_key);
    let mut seed = [0u8; TOKEN_ID_BYTES];
    hk.expand(SEED_HKDF_INFO, &mut seed)
        .expect("HKDF expand to 20 bytes is infallible");
    seed
}

fn read_saved_cover_message(
    cipher: &ConversationCipher,
    shared_key: &[u8],
    store: &HashMap<[u8; TOKEN_ID_BYTES], String>,
    saved: &SavedCoverMessage,
) -> Result<String, Error> {
    let seed =
        match decode_cover_message_token(cipher, shared_key, saved.version, &saved.cover_text)? {
            CoverMessageToken::OldSeed(seed) => seed,
            CoverMessageToken::SharedKeyHandle(handle) => derive_seed(shared_key, &handle),
        };

    store
        .get(&seed)
        .cloned()
        .ok_or_else(|| Error::Mode1ParseError("saved cover referenced missing text".to_owned()))
}

#[test]
fn task_0072_saved_old_and_new_cover_messages_read_exactly() {
    let shared_key = b"task-0072-paired-shared-key";
    let cipher = ConversationCipher::from_salt(b"task-0072-cover-compat-scope");

    let old_seed = [0x72; TOKEN_ID_BYTES];
    let old_text = "TASK0072 old saved cover original text";
    let old_cover = encode_token(&cipher, shared_key, &old_seed);

    let new_handle = *b"msg0072n";
    let new_seed = derive_seed(shared_key, &new_handle);
    let new_text = "TASK0072 new saved cover original text";
    let new_cover = encode_shrunk_token(shared_key, &new_handle);

    let mut store = HashMap::new();
    store.insert(old_seed, old_text.to_owned());
    store.insert(new_seed, new_text.to_owned());

    let saved = vec![
        SavedCoverMessage {
            version: OLD_COVER_MESSAGE_VERSION,
            cover_text: old_cover,
            original_text: old_text.to_owned(),
        },
        SavedCoverMessage {
            version: NEW_COVER_MESSAGE_VERSION,
            cover_text: new_cover,
            original_text: new_text.to_owned(),
        },
    ];

    let path = std::env::temp_dir().join(format!(
        "osl-task-0072-cover-compat-{}.json",
        std::process::id()
    ));
    std::fs::write(&path, serde_json::to_vec_pretty(&saved).unwrap()).unwrap();
    let loaded: Vec<SavedCoverMessage> =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let _ = std::fs::remove_file(&path);

    let old_loaded = loaded
        .iter()
        .find(|message| message.version == OLD_COVER_MESSAGE_VERSION)
        .expect("saved old cover exists");
    let new_loaded = loaded
        .iter()
        .find(|message| message.version == NEW_COVER_MESSAGE_VERSION)
        .expect("saved new cover exists");

    let old_read = read_saved_cover_message(&cipher, shared_key, &store, old_loaded)
        .expect("old saved cover reads");
    let new_read = read_saved_cover_message(&cipher, shared_key, &store, new_loaded)
        .expect("new saved cover reads");
    let old_wrong_read_count = loaded
        .iter()
        .filter(|message| message.version == OLD_COVER_MESSAGE_VERSION)
        .filter(|message| {
            read_saved_cover_message(&cipher, shared_key, &store, message)
                .ok()
                .as_deref()
                != Some(message.original_text.as_str())
        })
        .count();

    let unknown = SavedCoverMessage {
        version: 99,
        cover_text: new_loaded.cover_text.clone(),
        original_text: "must not be read".to_owned(),
    };
    let unknown_error =
        read_saved_cover_message(&cipher, shared_key, &store, &unknown).unwrap_err();
    let unknown_error_text = unknown_error.to_string();
    let unknown_error_name = match unknown_error {
        Error::UnknownCoverMessageVersion(99) => "UnknownCoverMessageVersion",
        other => panic!("unexpected unknown-version error: {other:?}"),
    };

    println!(
        "TASK0072 saved_old_original_text={}",
        old_loaded.original_text
    );
    println!("TASK0072 saved_old_read_text={old_read}");
    println!(
        "TASK0072 saved_old_exact_match={}",
        old_read == old_loaded.original_text
    );
    println!(
        "TASK0072 saved_new_original_text={}",
        new_loaded.original_text
    );
    println!("TASK0072 saved_new_read_text={new_read}");
    println!(
        "TASK0072 saved_new_exact_match={}",
        new_read == new_loaded.original_text
    );
    println!("TASK0072 unknown_version_error_name={unknown_error_name}");
    println!("TASK0072 unknown_version_error_text={unknown_error_text}");
    println!("TASK0072 old_wrong_read_count={old_wrong_read_count}");

    assert_eq!(old_read, old_loaded.original_text);
    assert_eq!(new_read, new_loaded.original_text);
    assert_eq!(old_wrong_read_count, 0);
}
