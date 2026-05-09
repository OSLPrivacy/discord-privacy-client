use crypto::ed25519;

#[test]
fn round_trip_sign_verify() {
    let (sec, pub_key) = ed25519::generate_keypair();
    let msg = b"hello prekey infrastructure";
    let sig = ed25519::sign(&sec, msg);
    assert!(ed25519::verify(&pub_key, msg, &sig).unwrap());
}

#[test]
fn verify_rejects_wrong_message() {
    let (sec, pub_key) = ed25519::generate_keypair();
    let sig = ed25519::sign(&sec, b"original");
    assert!(!ed25519::verify(&pub_key, b"tampered", &sig).unwrap());
}

#[test]
fn verify_rejects_wrong_key() {
    let (sec, _) = ed25519::generate_keypair();
    let (_, other_pub) = ed25519::generate_keypair();
    let sig = ed25519::sign(&sec, b"x");
    assert!(!ed25519::verify(&other_pub, b"x", &sig).unwrap());
}

#[test]
fn verify_rejects_tampered_signature() {
    let (sec, pub_key) = ed25519::generate_keypair();
    let mut sig = ed25519::sign(&sec, b"x");
    let mut bytes = *sig.as_bytes();
    bytes[0] ^= 0xFF;
    sig = ed25519::Signature::from_bytes(bytes);
    assert!(!ed25519::verify(&pub_key, b"x", &sig).unwrap());
}

#[test]
fn derive_public_matches_keygen_output() {
    let (sec, expected_pub) = ed25519::generate_keypair();
    let derived = ed25519::derive_public(&sec);
    assert_eq!(derived.as_bytes(), expected_pub.as_bytes());
}

#[test]
fn distinct_keypairs_yield_distinct_publics() {
    let (_, a) = ed25519::generate_keypair();
    let (_, b) = ed25519::generate_keypair();
    assert_ne!(a.as_bytes(), b.as_bytes());
}

#[test]
fn signature_is_deterministic_per_message() {
    // Ed25519 is deterministic by spec — same key + message → same
    // signature.
    let (sec, _) = ed25519::generate_keypair();
    let s1 = ed25519::sign(&sec, b"deterministic");
    let s2 = ed25519::sign(&sec, b"deterministic");
    assert_eq!(s1.as_bytes(), s2.as_bytes());
}

#[test]
fn signature_size_constants() {
    assert_eq!(ed25519::SECRET_KEY_SIZE, 32);
    assert_eq!(ed25519::PUBLIC_KEY_SIZE, 32);
    assert_eq!(ed25519::SIGNATURE_SIZE, 64);
}

#[test]
fn rfc_8032_test_vector_1() {
    // RFC 8032 §7.1 Test 1: the empty message.
    use hex::decode as h;
    let secret_bytes: [u8; 32] = h(
        "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
    )
    .unwrap()
    .try_into()
    .unwrap();
    let expected_pub: [u8; 32] = h(
        "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a",
    )
    .unwrap()
    .try_into()
    .unwrap();
    let expected_sig: [u8; 64] = h(
        "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b",
    )
    .unwrap()
    .try_into()
    .unwrap();

    let sec = ed25519::SecretKey::from_bytes(secret_bytes);
    let derived = ed25519::derive_public(&sec);
    assert_eq!(derived.as_bytes(), &expected_pub);

    let sig = ed25519::sign(&sec, &[]);
    assert_eq!(sig.as_bytes(), &expected_sig);

    let pub_key = ed25519::PublicKey::from_bytes(expected_pub);
    assert!(ed25519::verify(&pub_key, &[], &sig).unwrap());
}
