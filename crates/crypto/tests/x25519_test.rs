use crypto::x25519::{derive_public, diffie_hellman, generate_keypair, PublicKey, SecretKey};

/// RFC 7748 §6.1 — Alice and Bob X25519 test vectors.
///
/// Note the RFC's published private-key bytes are not pre-clamped;
/// libsodium / dryoc clamp internally on scalar-mult, so passing the
/// raw RFC bytes through `derive_public` and `diffie_hellman` must
/// produce the published results.
#[test]
fn rfc7748_section_6_1_alice_bob() {
    let alice_secret = SecretKey::from_bytes(hex32(
        "77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a",
    ));
    let alice_public_expected = hex32(
        "8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a",
    );
    let bob_secret = SecretKey::from_bytes(hex32(
        "5dab087e624a8a4b79e17f8b83800ee66f3bb1292618b6fd1c2f8b27ff88e0eb",
    ));
    let bob_public_expected = hex32(
        "de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f",
    );
    let shared_expected = hex32(
        "4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742",
    );

    let alice_public = derive_public(&alice_secret);
    assert_eq!(
        *alice_public.as_bytes(),
        alice_public_expected,
        "Alice's public key"
    );

    let bob_public = derive_public(&bob_secret);
    assert_eq!(
        *bob_public.as_bytes(),
        bob_public_expected,
        "Bob's public key"
    );

    let alice_shared = diffie_hellman(&alice_secret, &bob_public).expect("DH (Alice)");
    assert_eq!(
        *alice_shared.as_bytes(),
        shared_expected,
        "Alice-side shared secret"
    );

    let bob_shared = diffie_hellman(&bob_secret, &alice_public).expect("DH (Bob)");
    assert_eq!(
        *bob_shared.as_bytes(),
        shared_expected,
        "Bob-side shared secret"
    );
}

#[test]
fn keypair_public_matches_derive_public() {
    let (secret, public) = generate_keypair();
    let derived = derive_public(&secret);
    assert_eq!(public.as_bytes(), derived.as_bytes());
}

#[test]
fn dh_is_symmetric() {
    let (alice_secret, alice_public) = generate_keypair();
    let (bob_secret, bob_public) = generate_keypair();

    let alice_shared = diffie_hellman(&alice_secret, &bob_public).expect("Alice DH");
    let bob_shared = diffie_hellman(&bob_secret, &alice_public).expect("Bob DH");
    assert_eq!(
        alice_shared.as_bytes(),
        bob_shared.as_bytes(),
        "DH must be symmetric"
    );
}

#[test]
fn different_keypairs_yield_different_publics() {
    let (_, p1) = generate_keypair();
    let (_, p2) = generate_keypair();
    assert_ne!(
        p1.as_bytes(),
        p2.as_bytes(),
        "fresh keypairs must produce distinct public keys"
    );
}

#[test]
fn different_keypairs_yield_different_shared_secrets() {
    let (alice_secret, _) = generate_keypair();
    let (_, bob_public_1) = generate_keypair();
    let (_, bob_public_2) = generate_keypair();
    assert_ne!(bob_public_1.as_bytes(), bob_public_2.as_bytes());

    let s1 = diffie_hellman(&alice_secret, &bob_public_1).expect("DH 1");
    let s2 = diffie_hellman(&alice_secret, &bob_public_2).expect("DH 2");
    assert_ne!(s1.as_bytes(), s2.as_bytes());
}

#[test]
fn dh_rejects_identity_point() {
    // The all-zero point is the identity element; scalar-mult yields
    // all-zero output, which libsodium / dryoc detects and reports
    // as an error (small-subgroup contributory-behaviour check).
    let (secret, _) = generate_keypair();
    let identity = PublicKey::from_bytes([0u8; 32]);
    let result = diffie_hellman(&secret, &identity);
    assert!(
        result.is_err(),
        "DH against the identity point must be rejected"
    );
}

#[test]
fn dh_rejects_known_low_order_point() {
    // From libsodium's known low-order curve25519 u-coordinates:
    // an order-8 point that scalar-mult drives to all-zeros under
    // any clamped scalar. Bytes are in little-endian wire format,
    // so the leading `0xe0` is the low byte of the u-coordinate.
    let (secret, _) = generate_keypair();
    let low_order = PublicKey::from_bytes(hex32(
        "e0eb7a7c3b41b8ae1656e3faf19fc46ada098deb9c32b1fd866205165f49b800",
    ));
    let result = diffie_hellman(&secret, &low_order);
    assert!(
        result.is_err(),
        "DH against a known order-8 point must be rejected"
    );
}

fn hex32(s: &str) -> [u8; 32] {
    let v = hex::decode(s).expect("valid hex");
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&v);
    arr
}
