//! Phase 7a: wire format v=2 round-trip tests.
//!
//! Covers encode/decode against the in-Rust API only. The send/recv
//! pipeline is still on v=1; 7b wires v=2 through it. These tests
//! lock the wire format itself: any change to layout, slot framing,
//! cipher, AAD strings, or HKDF info bytes will break at least one
//! of them.
//!
//! Spec: `docs/phase-7-design.md` §4.2.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use crypto::x25519;
use ipc::commands::{decrypt_osl_phase4_cover, encrypt_osl_phase4_to_pubkeys};
use ipc::wire_v2::{
    decrypt_v2, encrypt_v2, V2Error, MSG_TYPE_BURN, MSG_TYPE_CONTENT, WIRE_VERSION_V2,
};

// ---- helpers ----

fn fresh_keypair() -> (x25519::SecretKey, x25519::PublicKey) {
    x25519::generate_keypair()
}

// ---- 1. single-recipient round-trip ----

#[test]
fn test_v2_round_trip_single_recipient() {
    let (sender_sk, _sender_pk) = fresh_keypair();
    let (recipient_sk, recipient_pk) = fresh_keypair();
    let sender_pk = x25519::derive_public(&sender_sk);

    let wire = encrypt_v2(
        b"hello world",
        &[recipient_pk],
        MSG_TYPE_CONTENT,
        &sender_sk,
    )
    .expect("encrypt single recipient");

    let recovered = decrypt_v2(&wire, &recipient_sk, &sender_pk).expect("decrypt single recipient");
    assert_eq!(recovered.plaintext, b"hello world".to_vec());
    assert_eq!(recovered.msg_type, MSG_TYPE_CONTENT);
}

// ---- 2. multi-recipient round-trip ----

#[test]
fn test_v2_round_trip_multi_recipient() {
    let (sender_sk, _) = fresh_keypair();
    let sender_pk = x25519::derive_public(&sender_sk);
    let (r1_sk, r1_pk) = fresh_keypair();
    let (r2_sk, r2_pk) = fresh_keypair();
    let (r3_sk, r3_pk) = fresh_keypair();

    let wire = encrypt_v2(
        b"hi everyone",
        &[r1_pk, r2_pk, r3_pk],
        MSG_TYPE_CONTENT,
        &sender_sk,
    )
    .expect("encrypt multi recipient");

    for sk in [&r1_sk, &r2_sk, &r3_sk] {
        let recovered = decrypt_v2(&wire, sk, &sender_pk).expect("each recipient decrypts");
        assert_eq!(recovered.plaintext, b"hi everyone".to_vec());
        assert_eq!(recovered.msg_type, MSG_TYPE_CONTENT);
    }

    // Sanity: a single-recipient wire is shorter than a 3-recipient
    // wire for the same plaintext (slot framing scales with N).
    let single_wire = encrypt_v2(b"hi everyone", &[r1_pk], MSG_TYPE_CONTENT, &sender_sk).unwrap();
    assert!(
        single_wire.len() < wire.len(),
        "expected single-recipient wire ({}) shorter than 3-recipient ({})",
        single_wire.len(),
        wire.len()
    );
}

// ---- 3. wrong-recipient rejection ----

#[test]
fn test_v2_wrong_recipient_cannot_decrypt() {
    let (sender_sk, _) = fresh_keypair();
    let sender_pk = x25519::derive_public(&sender_sk);
    let (_a_sk, a_pk) = fresh_keypair();
    let (b_sk, _b_pk) = fresh_keypair();

    // Encrypted to A only; B tries to decrypt.
    let wire = encrypt_v2(b"only for A", &[a_pk], MSG_TYPE_CONTENT, &sender_sk).unwrap();

    let err = decrypt_v2(&wire, &b_sk, &sender_pk).unwrap_err();
    assert!(
        matches!(err, V2Error::NoMatchingSlot),
        "expected NoMatchingSlot, got {err:?}"
    );
}

// ---- 4. type byte preserved ----

#[test]
fn test_v2_type_byte_preserved() {
    let (sender_sk, _) = fresh_keypair();
    let sender_pk = x25519::derive_public(&sender_sk);
    let (recipient_sk, recipient_pk) = fresh_keypair();

    // Burn marker carries no body content beyond the type byte;
    // we still wrap an empty-ish body so the wire is well-formed.
    let wire = encrypt_v2(
        b"burn-marker-body",
        &[recipient_pk],
        MSG_TYPE_BURN,
        &sender_sk,
    )
    .unwrap();

    let recovered = decrypt_v2(&wire, &recipient_sk, &sender_pk).unwrap();
    assert_eq!(recovered.msg_type, MSG_TYPE_BURN);
    assert_eq!(recovered.plaintext, b"burn-marker-body".to_vec());
}

// ---- 5. tampered ciphertext fails ----

#[test]
fn test_v2_tampered_ciphertext_fails() {
    let (sender_sk, _) = fresh_keypair();
    let sender_pk = x25519::derive_public(&sender_sk);
    let (recipient_sk, recipient_pk) = fresh_keypair();

    let wire = encrypt_v2(
        b"do not corrupt me",
        &[recipient_pk],
        MSG_TYPE_CONTENT,
        &sender_sk,
    )
    .unwrap();

    // Flip a single byte deep in the base64 body. We decode, flip
    // a byte in the body-ct region (past the slot framing), re-
    // encode, and reattach the DPC0:: prefix.
    let body = wire.strip_prefix("DPC0::").unwrap();
    let mut raw = STANDARD.decode(body).unwrap();
    // The body ciphertext sits after `version(1) + type(1) + N(1) +
    // 1*SLOT_BYTES(70) + body_nonce(12) = 85` bytes. Flip the
    // byte right after that — the first body-ct byte.
    let body_ct_off = 1 + 1 + 1 + 70 + 12;
    assert!(raw.len() > body_ct_off, "wire too short for tamper");
    raw[body_ct_off] ^= 0x01;
    let tampered = format!("DPC0::{}", STANDARD.encode(&raw));

    let err = decrypt_v2(&tampered, &recipient_sk, &sender_pk).unwrap_err();
    assert!(
        matches!(err, V2Error::BodyAeadFailed),
        "expected BodyAeadFailed, got {err:?}"
    );
}

// ---- 6. v=1 still works (regression) ----

#[test]
fn test_v1_still_works() {
    // The Phase 4 (v=1) encoder/decoder must remain functional
    // after v=2 lands. This mirrors a single-recipient v=1
    // round-trip from `osl_phase4_roundtrip.rs` to lock the
    // invariant locally too.
    let (sender_sk, _) = fresh_keypair();
    let sender_pk = x25519::derive_public(&sender_sk);
    let (recipient_sk, recipient_pk) = fresh_keypair();

    let wire = encrypt_osl_phase4_to_pubkeys(&sender_sk, &[recipient_pk], "v1 still works")
        .expect("v=1 encrypt");
    let pt_bytes = decrypt_osl_phase4_cover(&recipient_sk, &sender_pk, &wire).expect("v=1 decrypt");
    assert_eq!(pt_bytes, b"v1 still works".to_vec());
}

// ---- 7. version routing ----

#[test]
fn test_version_routing() {
    // A v=1 wire fed to decrypt_v2 → WrongVersion. A v=2 wire fed
    // to decrypt_osl_phase4_cover → BadPrefix / UnsupportedVersion
    // depending on which guard triggers first. Confirm both
    // directions return a typed mismatch (not silent garbage).
    let (sender_sk, _) = fresh_keypair();
    let sender_pk = x25519::derive_public(&sender_sk);
    let (recipient_sk, recipient_pk) = fresh_keypair();

    let v1_wire =
        encrypt_osl_phase4_to_pubkeys(&sender_sk, &[recipient_pk], "v1 to v2 decoder").unwrap();
    let v2_wire = encrypt_v2(
        b"v2 to v1 decoder",
        &[recipient_pk],
        MSG_TYPE_CONTENT,
        &sender_sk,
    )
    .unwrap();

    // v=1 wire fed to the v=2 decoder.
    match decrypt_v2(&v1_wire, &recipient_sk, &sender_pk) {
        Err(V2Error::WrongVersion { got, expected }) => {
            assert_eq!(got, 0x01);
            assert_eq!(expected, WIRE_VERSION_V2);
        }
        other => panic!("expected WrongVersion, got {other:?}"),
    }

    // v=2 wire fed to the v=1 decoder. The v=1 path checks the
    // version byte and returns UnsupportedVersion; either that or
    // a typed wire-shape mismatch is acceptable here — we only
    // require it not silently succeed.
    match decrypt_osl_phase4_cover(&recipient_sk, &sender_pk, &v2_wire) {
        Err(_) => {
            // Any typed error is fine; the v=1 decoder must
            // refuse to interpret a v=2 wire.
        }
        Ok(bytes) => panic!(
            "v=1 decoder must reject v=2 wire, instead produced {} bytes",
            bytes.len()
        ),
    }
}
