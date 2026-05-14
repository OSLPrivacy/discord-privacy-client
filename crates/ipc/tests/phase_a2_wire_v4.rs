//! Phase 9-A2 Task 4: wire v=4 encode + decode tests.
//!
//! Each test exercises `encrypt_v4` and `decrypt_v4` against a pair
//! of `DoubleRatchet` instances (alice initiator, bob responder).
//! Bob never actually runs his DR in these tests — they're focused
//! on the wire layer's framing, AAD discipline, and flag handling.
//! The full DR-through-IPC integration lives in
//! `phase_a2_integration_dr_roundtrip.rs`.

use base64::Engine as _;
use crypto::pqxdh::SessionKey;
use crypto::ratchet::{DoubleRatchet, SessionContext, SESSION_VERSION_V1};
use crypto::{ml_kem_768, pqxdh, x25519};
use ipc::wire_v2::{
    decrypt_v3, decrypt_v4, encrypt_v3, encrypt_v4_from_ratchet, RecipientV3, V2Error,
    V4_FLAG_BOOTSTRAP, WIRE_VERSION_V4,
};

struct Setup {
    alice: DoubleRatchet,
    bob: DoubleRatchet,
    alice_ik_sk: x25519::SecretKey,
    alice_ik_pub: x25519::PublicKey,
    bob_ik_sk: x25519::SecretKey,
    bob_recipient: RecipientV3,
    bob_mlkem_dk: ml_kem_768::DecapsulationKey,
}

fn setup() -> Setup {
    let (alice_ik_sk, alice_ik_pub) = x25519::generate_keypair();
    let (bob_ik_sk, bob_ik_pub) = x25519::generate_keypair();
    let (bob_spk_sk, bob_spk_pub) = x25519::generate_keypair();
    let (bob_mlkem_dk, bob_mlkem_ek) = ml_kem_768::generate_keypair();
    let (alice_sk, hs) =
        pqxdh::initiate(&alice_ik_sk, &bob_ik_pub, &bob_spk_pub, None, &bob_mlkem_ek).unwrap();
    let bob_sk: SessionKey = pqxdh::respond(
        &bob_ik_sk,
        &bob_spk_sk,
        None,
        &bob_mlkem_dk,
        &alice_ik_pub,
        &hs,
    )
    .unwrap();
    let alice_ctx = SessionContext {
        local_ik_x25519_pub: alice_ik_pub,
        local_ik_mlkem_pub: vec![0xaa; 1184],
        peer_ik_x25519_pub: bob_ik_pub,
        peer_ik_mlkem_pub: vec![0xbb; 1184],
        conversation_id: b"wire-v4-test".to_vec(),
        session_version: SESSION_VERSION_V1,
    };
    let bob_ctx = SessionContext {
        local_ik_x25519_pub: bob_ik_pub,
        local_ik_mlkem_pub: vec![0xbb; 1184],
        peer_ik_x25519_pub: alice_ik_pub,
        peer_ik_mlkem_pub: vec![0xaa; 1184],
        conversation_id: b"wire-v4-test".to_vec(),
        session_version: SESSION_VERSION_V1,
    };
    let alice = DoubleRatchet::new_initiator(&alice_sk, &bob_spk_pub, alice_ctx).unwrap();
    let bob = DoubleRatchet::new_responder(&bob_sk, &bob_spk_sk, bob_ctx).unwrap();
    let bob_recipient = RecipientV3 {
        x25519_pub: bob_ik_pub,
        mlkem_pub: ml_kem_768::EncapsulationKey::from_bytes(&bob_mlkem_ek.to_bytes()),
    };
    Setup {
        alice,
        bob,
        alice_ik_sk,
        alice_ik_pub,
        bob_ik_sk,
        bob_recipient,
        bob_mlkem_dk,
    }
}

/// Convenience: run one pqxdh::initiate against bob and ship the
/// DR-encrypted `em` as a v=4 wire blob. Mirrors the signature shape
/// `cmd_osl_encrypt_message_v2` uses internally for v=4 sends.
fn seal_v4(s: &Setup, bootstrap: bool, em: &crypto::ratchet::EncryptedMessage) -> String {
    let (session_key, handshake) = pqxdh::initiate(
        &s.alice_ik_sk,
        &s.bob_recipient.x25519_pub,
        &s.bob_recipient.x25519_pub,
        None,
        &s.bob_recipient.mlkem_pub,
    )
    .unwrap();
    encrypt_v4_from_ratchet(
        &s.alice_ik_pub,
        &s.bob_recipient,
        &session_key,
        &handshake,
        ipc::wire_v2::MSG_TYPE_CONTENT,
        bootstrap,
        em,
    )
    .unwrap()
}

#[test]
fn v4_roundtrip_single_recipient() {
    let mut s = setup();
    let em = s.alice.encrypt(b"hello v=4").unwrap();
    let (session_key, handshake) = pqxdh::initiate(
        &s.alice_ik_sk,
        &s.bob_recipient.x25519_pub,
        &s.bob_recipient.x25519_pub,
        None,
        &s.bob_recipient.mlkem_pub,
    )
    .unwrap();
    let wire = encrypt_v4_from_ratchet(
        &s.alice_ik_pub,
        &s.bob_recipient,
        &session_key,
        &handshake,
        ipc::wire_v2::MSG_TYPE_CONTENT,
        true,
        &em,
    )
    .expect("encode");
    let parsed = decrypt_v4(&wire, &s.bob_ik_sk, &s.bob_mlkem_dk).expect("decode");
    assert!(parsed.bootstrap, "flags.bootstrap=1 should survive");
    assert_eq!(parsed.msg_type, ipc::wire_v2::MSG_TYPE_CONTENT);
    // Reconstruct the DR-shaped EncryptedMessage and run bob's DR
    // to confirm the wire faithfully carried the ratchet artefacts.
    let em_recovered = crypto::ratchet::EncryptedMessage {
        header_nonce: parsed.enc_header_nonce,
        enc_header: parsed.enc_header,
        message_nonce: parsed.body_nonce,
        ciphertext: parsed.body_ct,
    };
    assert_eq!(s.bob.decrypt(&em_recovered).unwrap(), b"hello v=4");
}

#[test]
fn v4_rejects_n_not_equal_to_1() {
    let mut s = setup();
    let em = s.alice.encrypt(b"forge N=2").unwrap();
    let (session_key, handshake) = pqxdh::initiate(
        &s.alice_ik_sk,
        &s.bob_recipient.x25519_pub,
        &s.bob_recipient.x25519_pub,
        None,
        &s.bob_recipient.mlkem_pub,
    )
    .unwrap();
    let wire = encrypt_v4_from_ratchet(
        &s.alice_ik_pub,
        &s.bob_recipient,
        &session_key,
        &handshake,
        ipc::wire_v2::MSG_TYPE_CONTENT,
        true,
        &em,
    )
    .unwrap();
    // Manually flip the N byte (offset 35 in the global header).
    let mut raw = base64::engine::general_purpose::STANDARD
        .decode(wire.strip_prefix("DPC0::").unwrap())
        .unwrap();
    raw[35] = 2;
    let tampered = format!(
        "DPC0::{}",
        base64::engine::general_purpose::STANDARD.encode(&raw)
    );
    let err = decrypt_v4(&tampered, &s.bob_ik_sk, &s.bob_mlkem_dk).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("N must be 1"),
        "expected N-must-be-1 error, got: {msg}"
    );
}

#[test]
fn v4_rejects_reserved_flags_bits() {
    let mut s = setup();
    let em = s.alice.encrypt(b"hi").unwrap();
    let wire = seal_v4(&s, false, &em);
    let mut raw = base64::engine::general_purpose::STANDARD
        .decode(wire.strip_prefix("DPC0::").unwrap())
        .unwrap();
    // Set bit 1 (reserved) of the flags byte (offset 2).
    raw[2] |= 0x02;
    let tampered = format!(
        "DPC0::{}",
        base64::engine::general_purpose::STANDARD.encode(&raw)
    );
    let err = decrypt_v4(&tampered, &s.bob_ik_sk, &s.bob_mlkem_dk).unwrap_err();
    assert!(
        format!("{err}").contains("reserved flags bits"),
        "expected reserved-bits error, got: {err}"
    );
}

#[test]
fn v4_bootstrap_flag_roundtrips() {
    let mut s = setup();
    let em = s.alice.encrypt(b"continuation").unwrap();
    let wire = seal_v4(&s, false, &em);
    let parsed = decrypt_v4(&wire, &s.bob_ik_sk, &s.bob_mlkem_dk).unwrap();
    assert!(!parsed.bootstrap, "bootstrap=false must round-trip");
    let em2 = s.alice.encrypt(b"second").unwrap();
    let wire2 = seal_v4(&s, true, &em2);
    let parsed2 = decrypt_v4(&wire2, &s.bob_ik_sk, &s.bob_mlkem_dk).unwrap();
    assert!(parsed2.bootstrap, "bootstrap=true must round-trip");
    assert_eq!(parsed2.bootstrap, (V4_FLAG_BOOTSTRAP & 0x01) != 0);
}

#[test]
fn v4_header_tamper_rejected_via_aad() {
    let mut s = setup();
    let em = s.alice.encrypt(b"tamper me").unwrap();
    let (session_key, handshake) = pqxdh::initiate(
        &s.alice_ik_sk,
        &s.bob_recipient.x25519_pub,
        &s.bob_recipient.x25519_pub,
        None,
        &s.bob_recipient.mlkem_pub,
    )
    .unwrap();
    let wire = encrypt_v4_from_ratchet(
        &s.alice_ik_pub,
        &s.bob_recipient,
        &session_key,
        &handshake,
        ipc::wire_v2::MSG_TYPE_CONTENT,
        true,
        &em,
    )
    .unwrap();
    let mut raw = base64::engine::general_purpose::STANDARD
        .decode(wire.strip_prefix("DPC0::").unwrap())
        .unwrap();
    // Flip msg_type byte (offset 1). Doesn't change framing, but
    // breaks the wrap AAD which binds the whole global header.
    raw[1] ^= 0xFF;
    let tampered = format!(
        "DPC0::{}",
        base64::engine::general_purpose::STANDARD.encode(&raw)
    );
    let err = decrypt_v4(&tampered, &s.bob_ik_sk, &s.bob_mlkem_dk).unwrap_err();
    assert!(
        matches!(err, V2Error::WrapAeadFailed),
        "expected WrapAeadFailed (AAD includes global header), got: {err:?}"
    );
}

#[test]
fn v4_decode_with_v3_bytes_returns_clear_error() {
    // Build a v=3 wire, feed it to decrypt_v4. v=3's version byte is
    // 0x03; v=4 decode rejects with WrongVersion.
    let s = setup();
    let recips = vec![s.bob_recipient.clone()];
    let v3_wire = encrypt_v3(
        &s.alice_ik_sk,
        &s.alice_ik_pub,
        &recips,
        ipc::wire_v2::MSG_TYPE_CONTENT,
        b"v=3 content",
    )
    .unwrap();
    let err = decrypt_v4(&v3_wire, &s.bob_ik_sk, &s.bob_mlkem_dk).unwrap_err();
    match err {
        V2Error::WrongVersion { got, expected } => {
            assert_eq!(got, 0x03);
            assert_eq!(expected, WIRE_VERSION_V4);
        }
        other => panic!("expected WrongVersion, got: {other:?}"),
    }
}

#[test]
fn v3_decode_with_v4_bytes_returns_clear_error() {
    let mut s = setup();
    let em = s.alice.encrypt(b"v=4 content").unwrap();
    let (session_key, handshake) = pqxdh::initiate(
        &s.alice_ik_sk,
        &s.bob_recipient.x25519_pub,
        &s.bob_recipient.x25519_pub,
        None,
        &s.bob_recipient.mlkem_pub,
    )
    .unwrap();
    let wire = encrypt_v4_from_ratchet(
        &s.alice_ik_pub,
        &s.bob_recipient,
        &session_key,
        &handshake,
        ipc::wire_v2::MSG_TYPE_CONTENT,
        true,
        &em,
    )
    .unwrap();
    let err = decrypt_v3(&wire, &s.bob_ik_sk, &s.bob_mlkem_dk).unwrap_err();
    match err {
        V2Error::WrongVersion { got, expected } => {
            assert_eq!(got, WIRE_VERSION_V4);
            assert_eq!(expected, 0x03);
        }
        other => panic!("expected WrongVersion, got: {other:?}"),
    }
}

#[test]
fn peek_wire_version_returns_4_for_v4_bytes() {
    // peek_wire_version is private to commands.rs; here we replicate
    // its single statement (read first byte of base64-decoded body)
    // to confirm v=4 wire bytes have 0x04 at position 0.
    let mut s = setup();
    let em = s.alice.encrypt(b"hi").unwrap();
    let (session_key, handshake) = pqxdh::initiate(
        &s.alice_ik_sk,
        &s.bob_recipient.x25519_pub,
        &s.bob_recipient.x25519_pub,
        None,
        &s.bob_recipient.mlkem_pub,
    )
    .unwrap();
    let wire = encrypt_v4_from_ratchet(
        &s.alice_ik_pub,
        &s.bob_recipient,
        &session_key,
        &handshake,
        ipc::wire_v2::MSG_TYPE_CONTENT,
        true,
        &em,
    )
    .unwrap();
    let body = wire.strip_prefix("DPC0::").unwrap();
    let raw = base64::engine::general_purpose::STANDARD
        .decode(body)
        .unwrap();
    assert_eq!(raw[0], WIRE_VERSION_V4);
    assert_eq!(raw[0], 0x04);
}
