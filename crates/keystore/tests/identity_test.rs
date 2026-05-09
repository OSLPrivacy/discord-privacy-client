use crypto::{ed25519, ml_kem_768};
use keystore::{generate_identity, Identity};

#[test]
fn generate_yields_consistent_keypair() {
    let identity = generate_identity("alice@discord".to_string());
    assert_eq!(identity.user_id, "alice@discord");

    // X25519 public derives deterministically from the secret.
    let derived = crypto::x25519::derive_public(&identity.x25519_secret);
    assert_eq!(derived.as_bytes(), identity.x25519_public.as_bytes());

    // Ed25519 public derives deterministically from the secret.
    let ed_derived = ed25519::derive_public(&identity.ed25519_secret);
    assert_eq!(ed_derived.as_bytes(), identity.ed25519_public.as_bytes());

    // Ed25519 sign/verify round-trip works.
    let sig = ed25519::sign(&identity.ed25519_secret, b"test");
    assert!(ed25519::verify(&identity.ed25519_public, b"test", &sig).unwrap());

    // ML-KEM round-trip: encapsulating to the public key + decapsulating
    // with the secret key must produce a matching shared secret.
    let ek = identity.mlkem_encapsulation_key();
    let dk = identity.mlkem_decapsulation_key();
    let (ct, ss_a) = ml_kem_768::encapsulate(&ek).unwrap();
    let ss_b = ml_kem_768::decapsulate(&dk, &ct).unwrap();
    assert_eq!(ss_a.as_bytes(), ss_b.as_bytes());
}

#[test]
fn generated_identities_are_distinct() {
    let a = generate_identity("a".to_string());
    let b = generate_identity("b".to_string());
    assert_ne!(a.x25519_public.as_bytes(), b.x25519_public.as_bytes());
    assert_ne!(a.ed25519_public.as_bytes(), b.ed25519_public.as_bytes());
    assert_ne!(a.mlkem_public_bytes, b.mlkem_public_bytes);
}

#[test]
fn from_bytes_round_trip() {
    let original = generate_identity("liam".to_string());
    let mut x_secret = [0u8; crypto::x25519::SECRET_KEY_SIZE];
    x_secret.copy_from_slice(original.x25519_secret.as_bytes());
    let mut x_public = [0u8; crypto::x25519::PUBLIC_KEY_SIZE];
    x_public.copy_from_slice(original.x25519_public.as_bytes());
    let mut ed_secret = [0u8; ed25519::SECRET_KEY_SIZE];
    ed_secret.copy_from_slice(original.ed25519_secret.as_bytes());
    let mut ed_public = [0u8; ed25519::PUBLIC_KEY_SIZE];
    ed_public.copy_from_slice(original.ed25519_public.as_bytes());
    let mut mlkem_secret = [0u8; ml_kem_768::DECAPSULATION_KEY_SIZE];
    mlkem_secret.copy_from_slice(original.mlkem_secret_bytes());
    let mlkem_public = original.mlkem_public_bytes;

    let rebuilt = Identity::from_bytes(
        original.user_id.clone(),
        x_secret,
        x_public,
        ed_secret,
        ed_public,
        mlkem_secret,
        mlkem_public,
    );
    assert_eq!(rebuilt.user_id, original.user_id);
    assert_eq!(
        rebuilt.x25519_public.as_bytes(),
        original.x25519_public.as_bytes(),
    );
    assert_eq!(
        rebuilt.ed25519_public.as_bytes(),
        original.ed25519_public.as_bytes(),
    );
    assert_eq!(rebuilt.mlkem_public_bytes, original.mlkem_public_bytes);

    // ML-KEM keypair survives the round-trip.
    let ek = rebuilt.mlkem_encapsulation_key();
    let dk = rebuilt.mlkem_decapsulation_key();
    let (ct, ss_a) = ml_kem_768::encapsulate(&ek).unwrap();
    let ss_b = ml_kem_768::decapsulate(&dk, &ct).unwrap();
    assert_eq!(ss_a.as_bytes(), ss_b.as_bytes());

    // Ed25519 sign/verify works after round-trip too.
    let sig = ed25519::sign(&rebuilt.ed25519_secret, b"after roundtrip");
    assert!(ed25519::verify(&rebuilt.ed25519_public, b"after roundtrip", &sig).unwrap());
}
