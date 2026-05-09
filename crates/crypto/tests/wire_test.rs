use crypto::aead;
use crypto::ml_kem_768;
use crypto::pqxdh::{self, InitiatorHandshake};
use crypto::random;
use crypto::ratchet::{self, DoubleRatchet, SessionContext, SESSION_VERSION_V1};
use crypto::sender_keys::{
    self, SenderChain, SenderContext, SESSION_VERSION_V1 as SK_SESSION_VERSION_V1,
};
use crypto::wire::{
    decode_initiator_handshake, decode_ratchet_message, decode_sender_keys_message,
    encode_initiator_handshake, encode_ratchet_message, encode_sender_keys_message,
    HANDSHAKE_MAGIC, RATCHET_MAGIC, SENDER_KEYS_MAGIC,
};
use crypto::x25519;

// ---- helpers ----

fn build_ratchet_pair() -> (DoubleRatchet, DoubleRatchet) {
    let (alice_ik_sec, alice_ik_pub) = x25519::generate_keypair();
    let (bob_ik_sec, bob_ik_pub) = x25519::generate_keypair();
    let (bob_spk_sec, bob_spk_pub) = x25519::generate_keypair();
    let (bob_dk, bob_ek) = ml_kem_768::generate_keypair();

    let (alice_sk, handshake) =
        pqxdh::initiate(&alice_ik_sec, &bob_ik_pub, &bob_spk_pub, None, &bob_ek).unwrap();
    let bob_sk = pqxdh::respond(
        &bob_ik_sec,
        &bob_spk_sec,
        None,
        &bob_dk,
        &alice_ik_pub,
        &handshake,
    )
    .unwrap();

    let alice_ctx = SessionContext {
        local_ik_x25519_pub: alice_ik_pub,
        local_ik_mlkem_pub: vec![0xAA; 16],
        peer_ik_x25519_pub: bob_ik_pub,
        peer_ik_mlkem_pub: vec![0xBB; 16],
        conversation_id: b"conv-test".to_vec(),
        session_version: SESSION_VERSION_V1,
    };
    let bob_ctx = SessionContext {
        local_ik_x25519_pub: bob_ik_pub,
        local_ik_mlkem_pub: vec![0xBB; 16],
        peer_ik_x25519_pub: alice_ik_pub,
        peer_ik_mlkem_pub: vec![0xAA; 16],
        conversation_id: b"conv-test".to_vec(),
        session_version: SESSION_VERSION_V1,
    };

    let alice =
        DoubleRatchet::new_initiator(&alice_sk, &bob_spk_pub, alice_ctx).expect("init alice");
    let bob =
        DoubleRatchet::new_responder(&bob_sk, &bob_spk_sec, bob_ctx).expect("init bob");
    (alice, bob)
}

fn build_sender_chain() -> (SenderChain, SenderContext) {
    let chain = SenderChain::new().expect("sender chain");
    let (_, ik_x25519_pub) = x25519::generate_keypair();
    let ctx = SenderContext {
        sender_ik_x25519_pub: ik_x25519_pub,
        sender_ik_mlkem_pub: vec![0xCC; 16],
        group_id: b"group-test".to_vec(),
        session_version: SK_SESSION_VERSION_V1,
    };
    (chain, ctx)
}

// ---- ratchet::EncryptedMessage round trips ----

#[test]
fn ratchet_message_round_trip_through_wire() {
    let (mut alice, mut bob) = build_ratchet_pair();
    let plaintext = b"hello via wire".to_vec();
    let msg = alice.encrypt(&plaintext).expect("encrypt");

    let bytes = encode_ratchet_message(&msg);
    assert_eq!(&bytes[..7], &RATCHET_MAGIC);

    let decoded = decode_ratchet_message(&bytes).expect("decode");
    assert_eq!(decoded.header_nonce, msg.header_nonce);
    assert_eq!(decoded.enc_header, msg.enc_header);
    assert_eq!(decoded.message_nonce, msg.message_nonce);
    assert_eq!(decoded.ciphertext, msg.ciphertext);

    // The decoded message is byte-equal to the original — feed it
    // straight to the receiver and confirm the protocol round-trips
    // through the wire layer.
    let recovered = bob.decrypt(&decoded).expect("ratchet decrypt");
    assert_eq!(recovered, plaintext);
}

#[test]
fn ratchet_decode_rejects_bad_magic() {
    let (mut alice, _bob) = build_ratchet_pair();
    let msg = alice.encrypt(b"x").unwrap();
    let mut bytes = encode_ratchet_message(&msg);
    bytes[0] ^= 0xFF;
    assert!(decode_ratchet_message(&bytes).is_err());
}

#[test]
fn ratchet_decode_rejects_wrong_version_byte() {
    let (mut alice, _bob) = build_ratchet_pair();
    let msg = alice.encrypt(b"x").unwrap();
    let mut bytes = encode_ratchet_message(&msg);
    // Trailing byte of the magic = 0x01; flip to 0x02.
    bytes[6] = 0x02;
    assert!(decode_ratchet_message(&bytes).is_err());
}

#[test]
fn ratchet_decode_rejects_truncated_input() {
    let (mut alice, _bob) = build_ratchet_pair();
    let msg = alice.encrypt(b"x").unwrap();
    let bytes = encode_ratchet_message(&msg);
    for cut in [0, 5, 7, 30, 60, bytes.len() - 1] {
        let truncated = &bytes[..cut];
        assert!(
            decode_ratchet_message(truncated).is_err(),
            "expected error truncating to {cut} bytes"
        );
    }
}

#[test]
fn ratchet_decode_rejects_trailing_garbage() {
    let (mut alice, _bob) = build_ratchet_pair();
    let msg = alice.encrypt(b"x").unwrap();
    let mut bytes = encode_ratchet_message(&msg);
    bytes.push(0xFF);
    assert!(decode_ratchet_message(&bytes).is_err());
}

#[test]
fn ratchet_decode_rejects_oversized_length_prefix() {
    let (mut alice, _bob) = build_ratchet_pair();
    let msg = alice.encrypt(b"x").unwrap();
    let mut bytes = encode_ratchet_message(&msg);
    // enc_header_len starts at offset 7 + 24 = 31. Replace with a
    // huge number that runs past the end of the input.
    let len_off = 7 + 24;
    bytes[len_off..len_off + 4].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(decode_ratchet_message(&bytes).is_err());
}

// ---- sender_keys::EncryptedMessage round trips ----

#[test]
fn sender_keys_message_round_trip_through_wire() {
    let (mut chain, ctx) = build_sender_chain();
    let plaintext = b"group hello".to_vec();
    let msg = chain.encrypt(&plaintext, &ctx).expect("encrypt");

    let bytes = encode_sender_keys_message(&msg);
    assert_eq!(&bytes[..7], &SENDER_KEYS_MAGIC);

    let decoded = decode_sender_keys_message(&bytes).expect("decode");
    assert_eq!(decoded.header_nonce, msg.header_nonce);
    assert_eq!(decoded.enc_header, msg.enc_header);
    assert_eq!(decoded.message_nonce, msg.message_nonce);
    assert_eq!(decoded.ciphertext, msg.ciphertext);

    // Round-trip through a receiver chain to confirm protocol survives
    // the wire layer.
    let mut receiver = sender_keys::ReceiverChain::install(
        chain.current_chain_id(),
        &chain.rotation_root_bytes(),
    )
    .expect("receiver install");
    let recovered = receiver.decrypt(&decoded, &ctx).expect("decrypt");
    assert_eq!(recovered, plaintext);
}

#[test]
fn sender_keys_and_ratchet_have_distinct_magic() {
    let (mut alice, _bob) = build_ratchet_pair();
    let r_msg = alice.encrypt(b"r").unwrap();
    let r_bytes = encode_ratchet_message(&r_msg);
    // Decoding sender-keys envelope as ratchet must fail — distinct
    // magic prevents conflation.
    assert!(decode_sender_keys_message(&r_bytes).is_err());

    let (mut chain, ctx) = build_sender_chain();
    let s_msg = chain.encrypt(b"s", &ctx).unwrap();
    let s_bytes = encode_sender_keys_message(&s_msg);
    assert!(decode_ratchet_message(&s_bytes).is_err());
}

// ---- pqxdh::InitiatorHandshake round trips ----

fn make_real_handshake() -> (InitiatorHandshake, pqxdh::SessionKey) {
    // Build a real handshake by running PQXDH end-to-end. The session
    // key isn't needed for wire tests but proves the handshake type
    // actually carries valid bytes.
    let (alice_ik_sec, _alice_ik_pub) = x25519::generate_keypair();
    let (bob_ik_sec, bob_ik_pub) = x25519::generate_keypair();
    let (bob_spk_sec, bob_spk_pub) = x25519::generate_keypair();
    let (bob_opk_sec, bob_opk_pub) = x25519::generate_keypair();
    let (bob_dk, bob_ek) = ml_kem_768::generate_keypair();
    let _ = (bob_ik_sec, bob_spk_sec, bob_opk_sec, bob_dk);

    let (sk, handshake) = pqxdh::initiate(
        &alice_ik_sec,
        &bob_ik_pub,
        &bob_spk_pub,
        Some((42u32, &bob_opk_pub)),
        &bob_ek,
    )
    .expect("initiate");
    (handshake, sk)
}

#[test]
fn handshake_round_trip_with_opk() {
    let (handshake, _sk) = make_real_handshake();
    let bytes = encode_initiator_handshake(&handshake);
    assert_eq!(&bytes[..7], &HANDSHAKE_MAGIC);

    let decoded = decode_initiator_handshake(&bytes).expect("decode");
    assert_eq!(decoded.ek_x25519_pub.as_bytes(), handshake.ek_x25519_pub.as_bytes());
    assert_eq!(
        decoded.mlkem_ciphertext.to_bytes(),
        handshake.mlkem_ciphertext.to_bytes()
    );
    assert_eq!(decoded.no_opk, handshake.no_opk);
    assert_eq!(decoded.opk_id, handshake.opk_id);
}

#[test]
fn handshake_round_trip_no_opk() {
    let (alice_ik_sec, _alice_ik_pub) = x25519::generate_keypair();
    let (_bob_ik_sec, bob_ik_pub) = x25519::generate_keypair();
    let (_bob_spk_sec, bob_spk_pub) = x25519::generate_keypair();
    let (_bob_dk, bob_ek) = ml_kem_768::generate_keypair();

    let (_sk, handshake) = pqxdh::initiate(
        &alice_ik_sec,
        &bob_ik_pub,
        &bob_spk_pub,
        None,
        &bob_ek,
    )
    .expect("initiate no-opk");
    assert!(handshake.no_opk);
    assert!(handshake.opk_id.is_none());

    let bytes = encode_initiator_handshake(&handshake);
    let decoded = decode_initiator_handshake(&bytes).expect("decode");
    assert!(decoded.no_opk);
    assert!(decoded.opk_id.is_none());
    assert_eq!(decoded.ek_x25519_pub.as_bytes(), handshake.ek_x25519_pub.as_bytes());
    assert_eq!(
        decoded.mlkem_ciphertext.to_bytes(),
        handshake.mlkem_ciphertext.to_bytes()
    );
}

#[test]
fn handshake_decode_rejects_bad_magic() {
    let (handshake, _) = make_real_handshake();
    let mut bytes = encode_initiator_handshake(&handshake);
    bytes[0] = b'X';
    assert!(decode_initiator_handshake(&bytes).is_err());
}

#[test]
fn handshake_decode_rejects_truncated() {
    let (handshake, _) = make_real_handshake();
    let bytes = encode_initiator_handshake(&handshake);
    for cut in [0, 6, 39, 100, bytes.len() - 1] {
        assert!(decode_initiator_handshake(&bytes[..cut]).is_err());
    }
}

#[test]
fn handshake_decode_rejects_wrong_mlkem_ciphertext_length() {
    let (handshake, _) = make_real_handshake();
    let bytes = encode_initiator_handshake(&handshake);
    // mlkem_ct_len starts at offset 7 + 32 = 39.
    let len_off = 7 + 32;
    let mut tampered = bytes.clone();
    tampered[len_off..len_off + 4].copy_from_slice(&100u32.to_be_bytes());
    // Now declared-len = 100 but actual remainder is much larger;
    // either the declared length-mismatch or the trailing-bytes check
    // should fire.
    assert!(decode_initiator_handshake(&tampered).is_err());
}

#[test]
fn handshake_decode_rejects_no_opk_with_opk_id_present() {
    let (handshake, _) = make_real_handshake();
    // Build a wire form by hand that violates the consistency rule.
    let mut bytes = encode_initiator_handshake(&handshake);
    // The no_opk byte is at offset 7 + 32 + 4 + 1088 = 1131.
    let no_opk_off = 7 + 32 + 4 + ml_kem_768::CIPHERTEXT_SIZE;
    bytes[no_opk_off] = 1; // claim no_opk = true while still carrying opk_id
    assert!(decode_initiator_handshake(&bytes).is_err());
}

#[test]
fn handshake_decode_rejects_invalid_no_opk_byte() {
    let (handshake, _) = make_real_handshake();
    let mut bytes = encode_initiator_handshake(&handshake);
    let no_opk_off = 7 + 32 + 4 + ml_kem_768::CIPHERTEXT_SIZE;
    bytes[no_opk_off] = 0x77;
    assert!(decode_initiator_handshake(&bytes).is_err());
}

#[test]
fn handshake_decode_rejects_invalid_opk_flag_byte() {
    let (handshake, _) = make_real_handshake();
    let mut bytes = encode_initiator_handshake(&handshake);
    let opk_flag_off = 7 + 32 + 4 + ml_kem_768::CIPHERTEXT_SIZE + 1;
    bytes[opk_flag_off] = 0x77;
    assert!(decode_initiator_handshake(&bytes).is_err());
}

// ---- inner Header round trips ----

#[test]
fn ratchet_header_byte_round_trip() {
    let (_sec, pub_key) = x25519::generate_keypair();
    let header = ratchet::Header {
        dh_pub: pub_key,
        prev_chain_length: 7,
        counter: 42,
        session_version: SESSION_VERSION_V1,
    };
    let bytes = header.to_bytes();
    assert_eq!(bytes.len(), ratchet::HEADER_BYTES);
    let recovered = ratchet::Header::from_bytes(&bytes).expect("ratchet header decode");
    assert_eq!(recovered, header);
}

#[test]
fn ratchet_header_decode_rejects_wrong_length() {
    let too_short = vec![0u8; ratchet::HEADER_BYTES - 1];
    let too_long = vec![0u8; ratchet::HEADER_BYTES + 1];
    assert!(ratchet::Header::from_bytes(&too_short).is_err());
    assert!(ratchet::Header::from_bytes(&too_long).is_err());
}

#[test]
fn sender_keys_header_byte_round_trip() {
    let header = sender_keys::Header {
        chain_id: 3,
        n: 12,
        prev_chain_length: 5,
        session_version: SK_SESSION_VERSION_V1,
    };
    let bytes = header.to_bytes();
    assert_eq!(bytes.len(), sender_keys::HEADER_BYTES);
    let recovered =
        sender_keys::Header::from_bytes(&bytes).expect("sender keys header decode");
    assert_eq!(recovered, header);
}

#[test]
fn sender_keys_header_decode_rejects_wrong_length() {
    assert!(sender_keys::Header::from_bytes(&[0u8; 15]).is_err());
    assert!(sender_keys::Header::from_bytes(&[0u8; 17]).is_err());
}

// ---- corner cases ----

#[test]
fn ratchet_message_with_random_byte_payloads_round_trips() {
    // Stress: build a synthetic EncryptedMessage out of random bytes
    // and confirm encode/decode is a perfect inverse, even when we
    // skip the actual ratchet machinery.
    let header_nonce = aead::Nonce::from_bytes({
        let mut a = [0u8; aead::NONCE_SIZE];
        a.copy_from_slice(&random::random_bytes(aead::NONCE_SIZE));
        a
    });
    let message_nonce = aead::Nonce::from_bytes({
        let mut a = [0u8; aead::NONCE_SIZE];
        a.copy_from_slice(&random::random_bytes(aead::NONCE_SIZE));
        a
    });
    let enc_header = random::random_bytes(60);
    let ciphertext = random::random_bytes(2_000);
    let msg = ratchet::EncryptedMessage {
        header_nonce,
        enc_header: enc_header.clone(),
        message_nonce,
        ciphertext: ciphertext.clone(),
    };
    let bytes = encode_ratchet_message(&msg);
    let decoded = decode_ratchet_message(&bytes).unwrap();
    assert_eq!(decoded.header_nonce, header_nonce);
    assert_eq!(decoded.enc_header, enc_header);
    assert_eq!(decoded.message_nonce, message_nonce);
    assert_eq!(decoded.ciphertext, ciphertext);
}

#[test]
fn empty_enc_header_and_ciphertext_round_trip() {
    // Defensive: an EncryptedMessage with empty enc_header /
    // ciphertext is a malformed real protocol bundle, but the wire
    // codec must still be a perfect inverse — protocol validation
    // happens above us.
    let msg = ratchet::EncryptedMessage {
        header_nonce: aead::Nonce::from_bytes([0u8; aead::NONCE_SIZE]),
        enc_header: Vec::new(),
        message_nonce: aead::Nonce::from_bytes([0u8; aead::NONCE_SIZE]),
        ciphertext: Vec::new(),
    };
    let bytes = encode_ratchet_message(&msg);
    let decoded = decode_ratchet_message(&bytes).unwrap();
    assert!(decoded.enc_header.is_empty());
    assert!(decoded.ciphertext.is_empty());
}
