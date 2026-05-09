use crypto::hkdf::{derive, derive_32};

/// RFC 5869 §A.1 — Test Case 1: Basic test case with SHA-256.
#[test]
fn rfc5869_test_case_1() {
    let ikm = hex_to_vec("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b");
    let salt = hex_to_vec("000102030405060708090a0b0c");
    let info = hex_to_vec("f0f1f2f3f4f5f6f7f8f9");
    let expected = hex_to_vec(
        "3cb25f25faacd57a90434f64d0362f2a\
         2d2d0a90cf1a5a4c5db02d56ecc4c5bf\
         34007208d5b887185865",
    );

    let okm = derive(&salt, &ikm, &info, 42).expect("derive");
    assert_eq!(okm, expected, "RFC 5869 test case 1 mismatch");
}

/// RFC 5869 §A.3 — Test Case 3: Test with SHA-256 and zero-length salt/info.
#[test]
fn rfc5869_test_case_3_empty_salt_and_info() {
    let ikm = hex_to_vec("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b");
    let salt: Vec<u8> = vec![];
    let info: Vec<u8> = vec![];
    let expected = hex_to_vec(
        "8da4e775a563c18f715f802a063c5a31\
         b8a11f5c5ee1879ec3454e5f3c738d2d\
         9d201395faa4b61a96c8",
    );

    let okm = derive(&salt, &ikm, &info, 42).expect("derive empty salt+info");
    assert_eq!(okm, expected, "RFC 5869 test case 3 mismatch");
}

#[test]
fn derive_32_returns_thirty_two_bytes() {
    let key = derive_32(b"salt", b"ikm", b"info").expect("derive_32");
    assert_eq!(key.len(), 32);
}

#[test]
fn different_info_yields_different_okm() {
    let salt = b"salt";
    let ikm = b"input key material";
    let a = derive(salt, ikm, b"label-a", 32).unwrap();
    let b = derive(salt, ikm, b"label-b", 32).unwrap();
    assert_ne!(a, b, "domain separation must produce distinct OKM");
}

fn hex_to_vec(s: &str) -> Vec<u8> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    hex::decode(s).expect("valid hex")
}
