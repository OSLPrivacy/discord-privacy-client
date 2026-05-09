use crypto::aead::{open, seal, Key, Nonce};
use crypto::random::{random_aead_key, random_nonce};

#[test]
fn round_trip() {
    let key = random_aead_key();
    let nonce = random_nonce();
    let ad = b"discord-privacy-client/test/v1";
    let plaintext = b"hello sender keys + double ratchet";

    let ciphertext = seal(&key, &nonce, ad, plaintext).expect("seal");
    let recovered = open(&key, &nonce, ad, &ciphertext).expect("open");
    assert_eq!(recovered, plaintext);
}

#[test]
fn open_rejects_wrong_ad() {
    let key = random_aead_key();
    let nonce = random_nonce();
    let plaintext = b"sensitive";

    let ciphertext = seal(&key, &nonce, b"original-ad", plaintext).expect("seal");
    assert!(
        open(&key, &nonce, b"tampered-ad", &ciphertext).is_err(),
        "open with mismatched AD must fail"
    );
}

#[test]
fn open_rejects_tampered_ciphertext() {
    let key = random_aead_key();
    let nonce = random_nonce();
    let plaintext = b"sensitive";
    let ad = b"ad";

    let mut ciphertext = seal(&key, &nonce, ad, plaintext).expect("seal");
    ciphertext[0] ^= 0xFF;
    assert!(
        open(&key, &nonce, ad, &ciphertext).is_err(),
        "open with tampered ciphertext must fail"
    );
}

#[test]
fn open_rejects_tampered_tag() {
    let key = random_aead_key();
    let nonce = random_nonce();
    let plaintext = b"sensitive";
    let ad = b"ad";

    let mut ciphertext = seal(&key, &nonce, ad, plaintext).expect("seal");
    let last = ciphertext.len() - 1;
    ciphertext[last] ^= 0x01;
    assert!(
        open(&key, &nonce, ad, &ciphertext).is_err(),
        "open with tampered tag must fail"
    );
}

#[test]
fn open_rejects_wrong_nonce() {
    let key = random_aead_key();
    let nonce_a = random_nonce();
    let nonce_b = random_nonce();
    assert_ne!(nonce_a, nonce_b);
    let plaintext = b"sensitive";
    let ad = b"ad";

    let ciphertext = seal(&key, &nonce_a, ad, plaintext).expect("seal");
    assert!(
        open(&key, &nonce_b, ad, &ciphertext).is_err(),
        "open with wrong nonce must fail"
    );
}

#[test]
fn open_rejects_wrong_key() {
    let key_a = random_aead_key();
    let key_b = random_aead_key();
    let nonce = random_nonce();
    let plaintext = b"sensitive";
    let ad = b"ad";

    let ciphertext = seal(&key_a, &nonce, ad, plaintext).expect("seal");
    assert!(
        open(&key_b, &nonce, ad, &ciphertext).is_err(),
        "open with wrong key must fail"
    );
}

#[test]
fn open_rejects_truncated_to_below_tag() {
    let key = random_aead_key();
    let nonce = random_nonce();
    let truncated = vec![0u8; 8]; // shorter than 16-byte tag
    assert!(open(&key, &nonce, b"ad", &truncated).is_err());
}

#[test]
fn empty_plaintext_round_trip() {
    let key = random_aead_key();
    let nonce = random_nonce();
    let ad = b"ad";

    let ciphertext = seal(&key, &nonce, ad, b"").expect("seal empty");
    assert_eq!(ciphertext.len(), 16, "empty plaintext yields just the tag");
    let recovered = open(&key, &nonce, ad, &ciphertext).expect("open empty");
    assert_eq!(recovered, b"");
}

/// Known-answer test for XChaCha20-Poly1305-IETF.
///
/// Vector from the IRTF CFRG XChaCha20 draft (Appendix A.3.1):
/// https://datatracker.ietf.org/doc/html/draft-irtf-cfrg-xchacha
#[test]
fn xchacha20poly1305_ietf_test_vector() {
    let key = Key::from_bytes(hex_to_array_32(
        "808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f",
    ));
    let nonce = Nonce::from_bytes(hex_to_array_24(
        "404142434445464748494a4b4c4d4e4f5051525354555657",
    ));
    let ad = hex_to_vec("50515253c0c1c2c3c4c5c6c7");
    let plaintext = hex_to_vec(
        "4c616469657320616e642047656e746c656d656e206f662074686520636c6173\
         73206f66202739393a204966204920636f756c64206f6666657220796f75206f\
         6e6c79206f6e652074697020666f7220746865206675747572652c2073756e73\
         637265656e20776f756c642062652069742e",
    );
    let expected_ct = hex_to_vec(
        "bd6d179d3e83d43b9576579493c0e939572a1700252bfaccbed2902c21396cbb\
         731c7f1b0b4aa6440bf3a82f4eda7e39ae64c6708c54c216cb96b72e1213b452\
         2f8c9ba40db5d945b11b69b982c1bb9e3f3fac2bc369488f76b2383565d3fff9\
         21f9664c97637da9768812f615c68b13b52ec0875924c1c7987947deafd8780a\
         cf49",
    );

    let actual_ct = seal(&key, &nonce, &ad, &plaintext).expect("seal vector");
    assert_eq!(actual_ct, expected_ct, "XChaCha20-Poly1305 KAT mismatch");

    let recovered = open(&key, &nonce, &ad, &actual_ct).expect("open vector");
    assert_eq!(recovered, plaintext);
}

fn hex_to_vec(s: &str) -> Vec<u8> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    hex::decode(s).expect("valid hex")
}

fn hex_to_array_32(s: &str) -> [u8; 32] {
    let v = hex_to_vec(s);
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&v);
    arr
}

fn hex_to_array_24(s: &str) -> [u8; 24] {
    let v = hex_to_vec(s);
    let mut arr = [0u8; 24];
    arr.copy_from_slice(&v);
    arr
}
