//! Phase 9-A3 Task 4: wire v=5 encode + decode tests.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use crypto::sender_keys::{ReceiverChain, SenderChain, SenderContext, SESSION_VERSION_V1};
use crypto::x25519;
use ipc::wire_v2::{decrypt_v4, decrypt_v5, encrypt_v5, RecipientV3, V2Error, WIRE_VERSION_V5};

fn ctx(seed: u8, group_id: &[u8]) -> SenderContext {
    SenderContext {
        sender_ik_x25519_pub: x25519::PublicKey::from_bytes([seed; 32]),
        sender_ik_mlkem_pub: vec![seed; 1184],
        group_id: group_id.to_vec(),
        session_version: SESSION_VERSION_V1,
    }
}

#[test]
fn v5_roundtrip_single_sender_single_receiver() {
    let mut sender = SenderChain::new().unwrap();
    let mut receiver =
        ReceiverChain::install(sender.current_chain_id(), &sender.rotation_root_bytes()).unwrap();
    let c = ctx(0xab, b"gc:test");
    let em = sender.encrypt(b"hello v=5", &c).unwrap();
    let sender_ik_pub = c.sender_ik_x25519_pub;
    let wire = encrypt_v5(&sender_ik_pub, ipc::wire_v2::MSG_TYPE_CONTENT, 0, &em).unwrap();
    let parsed = decrypt_v5(&wire).expect("decode v=5");
    assert_eq!(parsed.msg_type, ipc::wire_v2::MSG_TYPE_CONTENT);
    assert_eq!(parsed.flags, 0);
    let em2 = crypto::sender_keys::EncryptedMessage {
        header_nonce: parsed.header_nonce,
        enc_header: parsed.enc_header,
        message_nonce: parsed.message_nonce,
        ciphertext: parsed.ciphertext,
    };
    let plain = receiver.decrypt(&em2, &c).unwrap();
    assert_eq!(plain, b"hello v=5");
}

#[test]
fn v5_rejects_nonzero_flags_bits() {
    let mut sender = SenderChain::new().unwrap();
    let c = ctx(0xcd, b"gc:test");
    let em = sender.encrypt(b"hi", &c).unwrap();
    let wire = encrypt_v5(
        &c.sender_ik_x25519_pub,
        ipc::wire_v2::MSG_TYPE_CONTENT,
        0,
        &em,
    )
    .unwrap();
    let mut raw = STANDARD
        .decode(wire.strip_prefix("DPC0::").unwrap())
        .unwrap();
    raw[34] = 0x01; // any nonzero flag
    let tampered = format!("DPC0::{}", STANDARD.encode(&raw));
    let err = decrypt_v5(&tampered).unwrap_err();
    assert!(
        format!("{err}").contains("reserved flags bits"),
        "expected reserved-bits error, got: {err}"
    );
}

#[test]
fn v5_header_tamper_rejected_via_aad() {
    let mut sender = SenderChain::new().unwrap();
    let mut receiver =
        ReceiverChain::install(sender.current_chain_id(), &sender.rotation_root_bytes()).unwrap();
    let c = ctx(0x11, b"gc:tamper");
    let em = sender.encrypt(b"tamper me", &c).unwrap();
    let sender_ik_pub = c.sender_ik_x25519_pub;
    let wire = encrypt_v5(&sender_ik_pub, ipc::wire_v2::MSG_TYPE_CONTENT, 0, &em).unwrap();
    let mut raw = STANDARD
        .decode(wire.strip_prefix("DPC0::").unwrap())
        .unwrap();
    // Flip a byte inside enc_header (offset = 35 global header + 24 nonce).
    raw[35 + 24] ^= 0xFF;
    let tampered = format!("DPC0::{}", STANDARD.encode(&raw));
    // decrypt_v5 parses successfully; the failure happens at the
    // sender-keys layer (HK forward-search exhausts).
    let parsed = decrypt_v5(&tampered).expect("parse stage ok");
    let em2 = crypto::sender_keys::EncryptedMessage {
        header_nonce: parsed.header_nonce,
        enc_header: parsed.enc_header,
        message_nonce: parsed.message_nonce,
        ciphertext: parsed.ciphertext,
    };
    assert!(receiver.decrypt(&em2, &c).is_err());
}

#[test]
fn v5_decode_with_v4_bytes_returns_clear_error() {
    // Build a v=4-ish wire by hand: first byte 0x04.
    let mut raw = vec![0u8; 200];
    raw[0] = 0x04;
    let wire = format!("DPC0::{}", STANDARD.encode(&raw));
    let err = decrypt_v5(&wire).unwrap_err();
    match err {
        V2Error::WrongVersion { got, expected } => {
            assert_eq!(got, 0x04);
            assert_eq!(expected, WIRE_VERSION_V5);
        }
        other => panic!("expected WrongVersion, got {other:?}"),
    }
}

#[test]
fn v4_decode_with_v5_bytes_returns_clear_error() {
    let mut sender = SenderChain::new().unwrap();
    let c = ctx(0x22, b"gc:test");
    let em = sender.encrypt(b"v=5 content", &c).unwrap();
    let wire = encrypt_v5(
        &c.sender_ik_x25519_pub,
        ipc::wire_v2::MSG_TYPE_CONTENT,
        0,
        &em,
    )
    .unwrap();
    // decrypt_v4 needs sk/dk; build dummy values and confirm the
    // version check fires before any crypto.
    let (sk, _pk) = x25519::generate_keypair();
    let (dk, _ek) = crypto::ml_kem_768::generate_keypair();
    let err = decrypt_v4(&wire, &sk, &dk).unwrap_err();
    match err {
        V2Error::WrongVersion { got, expected } => {
            assert_eq!(got, WIRE_VERSION_V5);
            assert_eq!(expected, 0x04);
        }
        other => panic!("expected WrongVersion, got {other:?}"),
    }
}

#[test]
fn peek_wire_version_returns_5_for_v5_bytes() {
    let mut sender = SenderChain::new().unwrap();
    let c = ctx(0x33, b"gc:peek");
    let em = sender.encrypt(b"hi", &c).unwrap();
    let wire = encrypt_v5(
        &c.sender_ik_x25519_pub,
        ipc::wire_v2::MSG_TYPE_CONTENT,
        0,
        &em,
    )
    .unwrap();
    let body = wire.strip_prefix("DPC0::").unwrap();
    let raw = STANDARD.decode(body).unwrap();
    assert_eq!(raw[0], WIRE_VERSION_V5);
    assert_eq!(raw[0], 0x05);
}

#[test]
fn v5_replay_within_chain_rejected() {
    let mut sender = SenderChain::new().unwrap();
    let mut receiver =
        ReceiverChain::install(sender.current_chain_id(), &sender.rotation_root_bytes()).unwrap();
    let c = ctx(0x44, b"gc:replay");
    let em = sender.encrypt(b"once", &c).unwrap();
    let sender_ik_pub = c.sender_ik_x25519_pub;
    let wire = encrypt_v5(&sender_ik_pub, ipc::wire_v2::MSG_TYPE_CONTENT, 0, &em).unwrap();

    let parsed = decrypt_v5(&wire).unwrap();
    let em2 = crypto::sender_keys::EncryptedMessage {
        header_nonce: parsed.header_nonce,
        enc_header: parsed.enc_header,
        message_nonce: parsed.message_nonce,
        ciphertext: parsed.ciphertext,
    };
    assert_eq!(receiver.decrypt(&em2, &c).unwrap(), b"once");
    // Same wire again — sender-keys forward search exhausts.
    let parsed2 = decrypt_v5(&wire).unwrap();
    let em3 = crypto::sender_keys::EncryptedMessage {
        header_nonce: parsed2.header_nonce,
        enc_header: parsed2.enc_header,
        message_nonce: parsed2.message_nonce,
        ciphertext: parsed2.ciphertext,
    };
    assert!(receiver.decrypt(&em3, &c).is_err(), "replay must fail");
}

#[test]
fn v5_out_of_order_within_window_succeeds() {
    let mut sender = SenderChain::new().unwrap();
    let mut receiver =
        ReceiverChain::install(sender.current_chain_id(), &sender.rotation_root_bytes()).unwrap();
    let c = ctx(0x55, b"gc:ooo");
    let em0 = sender.encrypt(b"m0", &c).unwrap();
    let em1 = sender.encrypt(b"m1", &c).unwrap();
    let em2 = sender.encrypt(b"m2", &c).unwrap();
    let sender_ik_pub = c.sender_ik_x25519_pub;
    let wires: Vec<String> = [&em0, &em1, &em2]
        .iter()
        .map(|e| encrypt_v5(&sender_ik_pub, ipc::wire_v2::MSG_TYPE_CONTENT, 0, e).unwrap())
        .collect();
    // Receive in reverse order; the sender-keys skipped cache handles it.
    let mk = |w: &str| -> crypto::sender_keys::EncryptedMessage {
        let p = decrypt_v5(w).unwrap();
        crypto::sender_keys::EncryptedMessage {
            header_nonce: p.header_nonce,
            enc_header: p.enc_header,
            message_nonce: p.message_nonce,
            ciphertext: p.ciphertext,
        }
    };
    assert_eq!(receiver.decrypt(&mk(&wires[2]), &c).unwrap(), b"m2");
    assert_eq!(receiver.decrypt(&mk(&wires[0]), &c).unwrap(), b"m0");
    assert_eq!(receiver.decrypt(&mk(&wires[1]), &c).unwrap(), b"m1");
}

// Drop unused import warning sentinel — RecipientV3 is referenced by
// other tests in this crate; we import it for shape continuity.
#[allow(dead_code)]
fn _unused() -> RecipientV3 {
    unreachable!()
}
