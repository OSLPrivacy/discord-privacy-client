//! Phase 9-A2 Task 1: persistence round-trip for `DoubleRatchet`.
//!
//! Mirror-struct invariants verified here:
//! - A persisted-then-reloaded ratchet keeps the cryptographic pairing
//!   with its counterpart (post-restart sender's next message decrypts
//!   on a never-restarted receiver, and vice versa).
//! - Skipped-message-key entries survive the round trip and still
//!   decrypt the out-of-order arrivals they were cached for.
//! - DH chain indices (sending_counter, receiving_counter,
//!   prev_sending_count) round-trip exactly.
//! - Serde JSON round-trip is lossless.
//! - The on-disk version byte is stamped to the current constant.

use crypto::pqxdh::SessionKey;
use crypto::ratchet::{
    DoubleRatchet, RatchetStateOnDisk, SessionContext, RATCHET_STATE_ON_DISK_VERSION,
    SESSION_VERSION_V1,
};
use crypto::{ml_kem_768, pqxdh, x25519};

struct HandshakeMaterial {
    alice_sk: SessionKey,
    bob_sk: SessionKey,
    bob_spk_secret: x25519::SecretKey,
    bob_spk_pub: x25519::PublicKey,
    alice_ik_pub: x25519::PublicKey,
    bob_ik_pub: x25519::PublicKey,
}

fn handshake_material() -> HandshakeMaterial {
    let (alice_ik_secret, alice_ik_pub) = x25519::generate_keypair();
    let (bob_ik_secret, bob_ik_pub) = x25519::generate_keypair();
    let (bob_spk_secret, bob_spk_pub) = x25519::generate_keypair();
    let (bob_mlkem_dk, bob_mlkem_ek) = ml_kem_768::generate_keypair();
    let (alice_sk, handshake) = pqxdh::initiate(
        &alice_ik_secret,
        &bob_ik_pub,
        &bob_spk_pub,
        None,
        &bob_mlkem_ek,
    )
    .expect("alice initiate");
    let bob_sk = pqxdh::respond(
        &bob_ik_secret,
        &bob_spk_secret,
        None,
        &bob_mlkem_dk,
        &alice_ik_pub,
        &handshake,
    )
    .expect("bob respond");
    HandshakeMaterial {
        alice_sk,
        bob_sk,
        bob_spk_secret,
        bob_spk_pub,
        alice_ik_pub,
        bob_ik_pub,
    }
}

fn ctx_pair(h: &HandshakeMaterial) -> (SessionContext, SessionContext) {
    let alice = SessionContext {
        local_ik_x25519_pub: h.alice_ik_pub,
        local_ik_mlkem_pub: vec![0xaa; 1184],
        peer_ik_x25519_pub: h.bob_ik_pub,
        peer_ik_mlkem_pub: vec![0xbb; 1184],
        conversation_id: b"persist-test".to_vec(),
        session_version: SESSION_VERSION_V1,
    };
    let bob = SessionContext {
        local_ik_x25519_pub: h.bob_ik_pub,
        local_ik_mlkem_pub: vec![0xbb; 1184],
        peer_ik_x25519_pub: h.alice_ik_pub,
        peer_ik_mlkem_pub: vec![0xaa; 1184],
        conversation_id: b"persist-test".to_vec(),
        session_version: SESSION_VERSION_V1,
    };
    (alice, bob)
}

fn pair() -> (DoubleRatchet, DoubleRatchet) {
    let h = handshake_material();
    let (alice_ctx, bob_ctx) = ctx_pair(&h);
    let alice = DoubleRatchet::new_initiator(&h.alice_sk, &h.bob_spk_pub, alice_ctx).unwrap();
    let bob = DoubleRatchet::new_responder(&h.bob_sk, &h.bob_spk_secret, bob_ctx).unwrap();
    (alice, bob)
}

#[test]
fn mirror_roundtrip_preserves_send_decrypt_pairing() {
    let (mut alice, mut bob) = pair();
    // Drive a few rounds so neither side is in its initial state.
    let m1 = alice.encrypt(b"a1").unwrap();
    bob.decrypt(&m1).unwrap();
    let r1 = bob.encrypt(b"b1").unwrap();
    alice.decrypt(&r1).unwrap();
    let m2 = alice.encrypt(b"a2").unwrap();
    bob.decrypt(&m2).unwrap();

    // Persist + reload alice mid-conversation.
    let disk = RatchetStateOnDisk::from(&alice);
    let mut alice_reloaded: DoubleRatchet = disk.try_into().expect("alice reload");

    // Reloaded alice can still send to live bob.
    let m3 = alice_reloaded.encrypt(b"a3 after restart").unwrap();
    assert_eq!(bob.decrypt(&m3).unwrap(), b"a3 after restart");

    // Symmetrically, persist + reload bob and confirm he can still
    // accept fresh messages from alice.
    let bob_disk = RatchetStateOnDisk::from(&bob);
    let mut bob_reloaded: DoubleRatchet = bob_disk.try_into().expect("bob reload");
    let m4 = alice_reloaded.encrypt(b"a4 to reloaded bob").unwrap();
    assert_eq!(bob_reloaded.decrypt(&m4).unwrap(), b"a4 to reloaded bob");
}

#[test]
fn mirror_roundtrip_preserves_skipped_keys() {
    let (mut alice, mut bob) = pair();
    let a0 = alice.encrypt(b"a0").unwrap();
    let a1 = alice.encrypt(b"a1").unwrap();
    let a2 = alice.encrypt(b"a2").unwrap();
    // Bob receives a1 + a2 out of order — a0 stays cached.
    bob.decrypt(&a1).unwrap();
    bob.decrypt(&a2).unwrap();
    assert!(
        bob.skipped_count() >= 1,
        "expected a0 cached in skipped store"
    );

    // Persist + reload bob.
    let disk = RatchetStateOnDisk::from(&bob);
    let mut bob_reloaded: DoubleRatchet = disk.try_into().expect("bob reload");
    assert_eq!(
        bob_reloaded.skipped_count(),
        bob.skipped_count(),
        "skipped count must survive the round trip"
    );

    // Reloaded bob still recovers the out-of-order a0 via the
    // cached header+message key.
    assert_eq!(bob_reloaded.decrypt(&a0).unwrap(), b"a0");
}

#[test]
fn mirror_roundtrip_preserves_dh_chain_index() {
    let (mut alice, mut bob) = pair();
    // Drive enough traffic to bump counters and trigger at least one
    // DH ratchet step in each direction.
    for i in 0u32..3 {
        let m = alice.encrypt(format!("a{i}").as_bytes()).unwrap();
        bob.decrypt(&m).unwrap();
        let r = bob.encrypt(format!("b{i}").as_bytes()).unwrap();
        alice.decrypt(&r).unwrap();
    }
    // Capture counters pre-persist.
    let (a_send, a_recv) = (alice.sending_counter(), alice.receiving_counter());
    let (b_send, b_recv) = (bob.sending_counter(), bob.receiving_counter());

    let alice_disk = RatchetStateOnDisk::from(&alice);
    let bob_disk = RatchetStateOnDisk::from(&bob);
    let alice_reloaded: DoubleRatchet = alice_disk.try_into().expect("alice reload");
    let bob_reloaded: DoubleRatchet = bob_disk.try_into().expect("bob reload");

    assert_eq!(alice_reloaded.sending_counter(), a_send);
    assert_eq!(alice_reloaded.receiving_counter(), a_recv);
    assert_eq!(bob_reloaded.sending_counter(), b_send);
    assert_eq!(bob_reloaded.receiving_counter(), b_recv);
}

#[test]
fn mirror_serde_json_roundtrip() {
    let (mut alice, mut bob) = pair();
    let m = alice.encrypt(b"hello").unwrap();
    bob.decrypt(&m).unwrap();
    let disk = RatchetStateOnDisk::from(&alice);
    let json = serde_json::to_string(&disk).expect("serialize");
    let reloaded: RatchetStateOnDisk = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(reloaded, disk, "JSON round trip must be lossless");
}

#[test]
fn mirror_version_byte_present_and_correct() {
    let (alice, _bob) = pair();
    let disk = RatchetStateOnDisk::from(&alice);
    assert_eq!(disk.version, RATCHET_STATE_ON_DISK_VERSION);
    // And specifically the documented constant.
    assert_eq!(disk.version, 0x01);
}
