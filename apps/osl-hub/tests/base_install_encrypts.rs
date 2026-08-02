//! T15-T25: a base install retains an encrypted word-bank path.
//!
//! The carrier names a ciphertext-store entry; it never contains the protected
//! message.  This proof starts from the persisted component state with no
//! optional payloads, then exercises the actual AEAD and built-in word-bank
//! codec used for that hand-off.

#![cfg(feature = "core")]

use crypto::aead::{open, seal, Key, Nonce};
use osl_privacy_hub::components::{ComponentId, ComponentStore};
use std::collections::BTreeMap;
use stego::{decode_token, encode_token, ConversationCipher, TOKEN_ID_BYTES};

#[test]
fn base_install_encrypts_through_the_word_bank_carrier() {
    let directory = tempfile::tempdir().expect("make an isolated base install");
    let components = ComponentStore::new(directory.path());
    let installed = components.list().expect("read a new install's components");

    assert_eq!(installed.len(), 1, "a new install has no optional payloads");
    assert_eq!(installed[0].id.as_str(), ComponentId::WORD_BANK_CARRIER);
    assert!(installed[0].installed);
    assert!(!installed[0].removable);

    let plaintext = b"a private message must survive a small install";
    let key = Key::from_bytes([0x31; 32]);
    let nonce = Nonce::from_bytes([0x47; 24]);
    let associated_data = b"osl/base-install/v1";
    let ciphertext = seal(&key, &nonce, associated_data, plaintext)
        .expect("the base install encrypts a protected message");
    assert_ne!(ciphertext.as_slice(), plaintext);

    let blob_id: [u8; TOKEN_ID_BYTES] = [0x53, 0x92, 0x10, 0xef, 0x64, 0x28, 0xba, 0x7c];
    let mut cipher_store = BTreeMap::new();
    cipher_store.insert(blob_id, ciphertext);

    let word_bank = ConversationCipher::from_salt(b"base-install-word-bank-scope");
    let carrier_key = b"base-install-carrier-mac-key-v1";
    let carrier = encode_token(&word_bank, carrier_key, &blob_id);
    assert!(
        !carrier.contains(std::str::from_utf8(plaintext).expect("fixture is UTF-8")),
        "the public word-bank carrier must not expose the protected message"
    );

    let recovered_id = decode_token(&word_bank, carrier_key, &carrier)
        .expect("the built-in word-bank carrier decodes without an optional model");
    let recovered_ciphertext = cipher_store
        .get(&recovered_id)
        .expect("the decoded carrier points at the encrypted message");
    let opened = open(&key, &nonce, associated_data, recovered_ciphertext)
        .expect("the encrypted message opens from the carrier-selected blob");

    assert_eq!(opened, plaintext);
}
