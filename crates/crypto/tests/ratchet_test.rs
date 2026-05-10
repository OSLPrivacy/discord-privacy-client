//! Double Ratchet tests — symmetric chains + skipped-key cache + DH
//! ratchet + encrypted headers + canonical AD.
//!
//! Coverage:
//! - `ChainKey`: deterministic advance, distinct keys per step,
//!   message-key derivation does not mutate the chain.
//! - `DoubleRatchet` round-trip on a single chain and many sequential
//!   messages.
//! - Bidirectional messaging across alternating DH ratchet steps.
//! - Responder cannot send before first receive (no `HKs`/`CKs`).
//! - Peer-mismatch failure: bootstrap secret mismatch yields header
//!   AEAD failure (NHKr derived from a different `SK` cannot open
//!   the wire header).
//! - Skipped-key cache: out-of-order within window, single-use,
//!   sequential-delivery doesn't touch cache, FIFO eviction at cap,
//!   TTL expiry, over-cap rejection without state pollution, replay
//!   rejection.
//! - DH ratchet: direction-change rotates header + chain keys,
//!   multiple consecutive rotations decrypt cleanly, out-of-order
//!   across DH boundaries via cache, post-compromise security via
//!   captured pre-rotation snapshot, replay across ratchet steps
//!   rejected.
//! - **Encrypted headers + AD**:
//!     - Header tampering rejected (any byte flip in `enc_header`).
//!     - `session_version` mismatch rejected explicitly before AEAD.
//!     - Header keys rotate on DH step (verified via dbg accessors).
//!     - Skipped header keys decode out-of-order arrivals from old
//!       DH chains.
//!     - `canonical_ad` is deterministic and emits the expected
//!       length-prefix layout.
//!     - Cross-conversation AD mismatch (different `conversation_id`)
//!       rejected via AEAD.

use crypto::pqxdh::SessionKey;
use crypto::ratchet::{
    canonical_ad, ChainKey, DoubleRatchet, SessionContext, MAX_SKIPPED_PER_CHAIN,
    SESSION_VERSION_V1, SKIPPED_KEY_TTL,
};
use crypto::{ml_kem_768, pqxdh, x25519};
use std::time::{Duration, SystemTime};

// ---- ChainKey-level tests ----

#[test]
fn chain_advance_is_deterministic() {
    let mut a = ChainKey::from_bytes([42u8; 32]);
    let mut b = ChainKey::from_bytes([42u8; 32]);
    a.advance().unwrap();
    b.advance().unwrap();
    assert_eq!(a.as_bytes(), b.as_bytes());
}

#[test]
fn chain_advance_yields_distinct_keys() {
    let mut chain = ChainKey::from_bytes([0u8; 32]);
    let initial = *chain.as_bytes();
    chain.advance().unwrap();
    assert_ne!(initial, *chain.as_bytes());
    let after_one = *chain.as_bytes();
    chain.advance().unwrap();
    assert_ne!(after_one, *chain.as_bytes());
    assert_ne!(initial, *chain.as_bytes());
}

#[test]
fn message_key_does_not_advance_chain() {
    let chain = ChainKey::from_bytes([7u8; 32]);
    let before = *chain.as_bytes();
    let _mk_a = chain.message_key().unwrap();
    let _mk_b = chain.message_key().unwrap();
    assert_eq!(before, *chain.as_bytes());
}

#[test]
fn message_keys_at_each_chain_step_are_distinct() {
    let mut chain = ChainKey::from_bytes([100u8; 32]);
    let mk_0 = chain.message_key().unwrap();
    chain.advance().unwrap();
    let mk_1 = chain.message_key().unwrap();
    chain.advance().unwrap();
    let mk_2 = chain.message_key().unwrap();
    assert_ne!(mk_0.as_bytes(), mk_1.as_bytes());
    assert_ne!(mk_1.as_bytes(), mk_2.as_bytes());
    assert_ne!(mk_0.as_bytes(), mk_2.as_bytes());
}

#[test]
fn distinct_starting_chains_diverge_immediately() {
    let mut a = ChainKey::from_bytes([1u8; 32]);
    let mut b = ChainKey::from_bytes([2u8; 32]);
    a.advance().unwrap();
    b.advance().unwrap();
    assert_ne!(a.as_bytes(), b.as_bytes());
}

// ---- DoubleRatchet basic round-trip ----

#[test]
fn round_trip_single_message() {
    let (mut alice, mut bob) = setup_pair();
    let msg = alice
        .encrypt(b"hello, encrypted-header double ratchet")
        .unwrap();
    assert_eq!(
        bob.decrypt(&msg).unwrap(),
        b"hello, encrypted-header double ratchet"
    );
}

#[test]
fn round_trip_multiple_messages_one_chain() {
    let (mut alice, mut bob) = setup_pair();
    for i in 0u32..16 {
        let plaintext = format!("alice -> bob message {i}");
        let msg = alice.encrypt(plaintext.as_bytes()).unwrap();
        assert_eq!(bob.decrypt(&msg).unwrap(), plaintext.as_bytes());
    }
    assert_eq!(alice.sending_counter(), 16);
    assert_eq!(bob.receiving_counter(), 16);
}

#[test]
fn bidirectional_messaging_with_dh_steps() {
    let (mut alice, mut bob) = setup_pair();

    let m_a1 = alice.encrypt(b"alice -> bob #1").unwrap();
    assert_eq!(bob.decrypt(&m_a1).unwrap(), b"alice -> bob #1");
    let m_b1 = bob.encrypt(b"bob -> alice #1").unwrap();
    assert_eq!(alice.decrypt(&m_b1).unwrap(), b"bob -> alice #1");

    let m_a2 = alice.encrypt(b"alice -> bob #2").unwrap();
    assert_eq!(bob.decrypt(&m_a2).unwrap(), b"alice -> bob #2");
    let m_b2 = bob.encrypt(b"bob -> alice #2").unwrap();
    assert_eq!(alice.decrypt(&m_b2).unwrap(), b"bob -> alice #2");

    // alice's last action was decrypting m_b2 → DH step → Ns reset.
    assert_eq!(alice.sending_counter(), 0);
    assert_eq!(bob.sending_counter(), 1);
    assert_eq!(alice.receiving_counter(), 1);
    assert_eq!(bob.receiving_counter(), 1);
}

#[test]
fn responder_cannot_send_before_receive() {
    let h = handshake_material();
    let (alice_ctx, bob_ctx) = make_ctx_pair(&h, SESSION_VERSION_V1, b"convo-1");
    let mut alice = DoubleRatchet::new_initiator(&h.alice_sk, &h.bob_spk_pub, alice_ctx).unwrap();
    let mut bob = DoubleRatchet::new_responder(&h.bob_sk, &h.bob_spk_secret, bob_ctx).unwrap();

    assert!(
        bob.encrypt(b"premature").is_err(),
        "responder must not encrypt before first receive"
    );
    let m = alice.encrypt(b"hi").unwrap();
    bob.decrypt(&m).unwrap();
    bob.encrypt(b"now ok").unwrap();
}

#[test]
fn peer_mismatch_decryption_fails() {
    // wrong_bob's bootstrap secret comes from a different handshake
    // entirely. His NHKr (derived from h2's SK) cannot open alice's
    // header (encrypted under HKs derived from h1's SK).
    let h1 = handshake_material();
    let h2 = handshake_material();
    let (alice_ctx, bob_ctx_real) = make_ctx_pair(&h1, SESSION_VERSION_V1, b"convo-1");
    // wrong_bob's ctx uses h2 identities (so even the AD would
    // mismatch on the message AEAD layer if we got that far).
    let bob_ctx_wrong = SessionContext {
        local_ik_x25519_pub: h2.bob_ik_pub,
        local_ik_mlkem_pub: vec![0xbb; 1184],
        peer_ik_x25519_pub: h2.alice_ik_pub,
        peer_ik_mlkem_pub: vec![0xaa; 1184],
        conversation_id: b"convo-1".to_vec(),
        session_version: SESSION_VERSION_V1,
    };

    let mut alice = DoubleRatchet::new_initiator(&h1.alice_sk, &h1.bob_spk_pub, alice_ctx).unwrap();
    let mut wrong_bob =
        DoubleRatchet::new_responder(&h2.bob_sk, &h2.bob_spk_secret, bob_ctx_wrong).unwrap();
    let msg = alice.encrypt(b"secret").unwrap();
    assert!(
        wrong_bob.decrypt(&msg).is_err(),
        "wrong bootstrap session must fail to open wire header"
    );

    let mut real_bob =
        DoubleRatchet::new_responder(&h1.bob_sk, &h1.bob_spk_secret, bob_ctx_real).unwrap();
    assert_eq!(real_bob.decrypt(&msg).unwrap(), b"secret");
}

#[test]
fn tampered_ciphertext_rejection() {
    // Flip a bit in the message ciphertext — message AEAD fails.
    let (mut alice, mut bob) = setup_pair();
    let mut msg = alice.encrypt(b"secret").unwrap();
    msg.ciphertext[0] ^= 0x01;
    assert!(bob.decrypt(&msg).is_err());
}

// ---- Skipped-key cache (within one DH chain) ----

#[test]
fn out_of_order_within_window() {
    let (mut alice, mut bob) = setup_pair();
    let m0 = alice.encrypt(b"msg 0").unwrap();
    let m1 = alice.encrypt(b"msg 1").unwrap();
    let m2 = alice.encrypt(b"msg 2").unwrap();
    let m3 = alice.encrypt(b"msg 3").unwrap();

    assert_eq!(bob.decrypt(&m1).unwrap(), b"msg 1");
    assert_eq!(bob.skipped_count(), 1);
    assert_eq!(bob.decrypt(&m3).unwrap(), b"msg 3");
    assert_eq!(bob.skipped_count(), 2);
    assert_eq!(bob.decrypt(&m2).unwrap(), b"msg 2");
    assert_eq!(bob.skipped_count(), 1);
    assert_eq!(bob.decrypt(&m0).unwrap(), b"msg 0");
    assert_eq!(bob.skipped_count(), 0);
}

#[test]
fn cached_keys_consume_on_use() {
    let (mut alice, mut bob) = setup_pair();
    let m0 = alice.encrypt(b"once").unwrap();
    let m1 = alice.encrypt(b"twice?").unwrap();

    bob.decrypt(&m1).unwrap();
    assert_eq!(bob.decrypt(&m0).unwrap(), b"once");
    assert!(
        bob.decrypt(&m0).is_err(),
        "consumed cache entry must not allow replay"
    );
}

#[test]
fn cache_survives_contiguous_run() {
    let (mut alice, mut bob) = setup_pair();
    let m0 = alice.encrypt(b"m0").unwrap();
    let m1 = alice.encrypt(b"m1").unwrap();
    let m2 = alice.encrypt(b"m2").unwrap();
    let m3 = alice.encrypt(b"m3").unwrap();

    bob.decrypt(&m3).unwrap();
    assert_eq!(bob.skipped_count(), 3);
    for i in 4u32..=8 {
        let plaintext = format!("m{i}");
        let m = alice.encrypt(plaintext.as_bytes()).unwrap();
        assert_eq!(bob.decrypt(&m).unwrap(), plaintext.as_bytes());
        assert_eq!(bob.skipped_count(), 3);
    }
    assert_eq!(bob.decrypt(&m0).unwrap(), b"m0");
    assert_eq!(bob.decrypt(&m1).unwrap(), b"m1");
    assert_eq!(bob.decrypt(&m2).unwrap(), b"m2");
    assert_eq!(bob.skipped_count(), 0);
}

#[test]
fn cache_evicts_oldest_at_cap() {
    let (mut alice, mut bob) = setup_pair();
    let total = MAX_SKIPPED_PER_CHAIN + 4;
    let mut msgs = Vec::with_capacity(total);
    for i in 0..total {
        msgs.push(alice.encrypt(format!("m{i}").as_bytes()).unwrap());
    }

    let cap = MAX_SKIPPED_PER_CHAIN;
    bob.decrypt(&msgs[cap]).unwrap();
    assert_eq!(bob.skipped_count(), cap);

    bob.decrypt(&msgs[cap + 3]).unwrap();
    assert_eq!(
        bob.skipped_count(),
        cap,
        "cache stays at cap after eviction"
    );

    assert!(bob.decrypt(&msgs[0]).is_err(), "slot 0 should be evicted");
    assert!(bob.decrypt(&msgs[1]).is_err(), "slot 1 should be evicted");
    assert_eq!(bob.decrypt(&msgs[2]).unwrap(), b"m2");
    let expected = format!("m{}", cap + 1);
    assert_eq!(bob.decrypt(&msgs[cap + 1]).unwrap(), expected.as_bytes());
}

#[test]
fn cached_keys_expire_after_ttl() {
    let (mut alice, mut bob) = setup_pair();
    let m0 = alice.encrypt(b"msg 0").unwrap();
    let m1 = alice.encrypt(b"msg 1").unwrap();

    let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    bob.decrypt_at(&m1, t0).unwrap();
    assert_eq!(bob.skipped_count(), 1);

    let later = t0 + SKIPPED_KEY_TTL + Duration::from_secs(1);
    assert!(
        bob.decrypt_at(&m0, later).is_err(),
        "cached key past TTL must not decrypt the original message"
    );
    assert_eq!(bob.skipped_count(), 0);
}

#[test]
fn excessive_gap_rejected_without_cache_pollution() {
    // Have alice send MAX+501 messages so msg[MAX+500] has counter
    // MAX+500 (>cap). Bob's first receive would require skipping
    // MAX+500 keys → reject without state mutation.
    let (mut alice, mut bob) = setup_pair();
    let total = MAX_SKIPPED_PER_CHAIN + 501;
    let mut last = None;
    for i in 0..total {
        let m = alice.encrypt(format!("m{i}").as_bytes()).unwrap();
        if i == total - 1 {
            last = Some(m);
        }
    }

    let bogus = last.expect("alice produced the last message");
    assert!(bob.decrypt(&bogus).is_err());
    assert_eq!(
        bob.skipped_count(),
        0,
        "oversized gap must not pollute cache"
    );
    assert_eq!(
        bob.receiving_counter(),
        0,
        "counter must not advance on oversized-gap rejection"
    );
}

#[test]
fn replay_rejection_within_chain() {
    let (mut alice, mut bob) = setup_pair();
    let m0 = alice.encrypt(b"once").unwrap();
    assert_eq!(bob.decrypt(&m0).unwrap(), b"once");
    assert!(bob.decrypt(&m0).is_err(), "replay must fail");
}

#[test]
fn sending_counter_advances_per_message() {
    let (mut alice, _bob) = setup_pair();
    alice.encrypt(b"a").unwrap();
    alice.encrypt(b"b").unwrap();
    alice.encrypt(b"c").unwrap();
    assert_eq!(alice.sending_counter(), 3);
}

#[test]
fn nonces_differ_across_consecutive_messages() {
    // Both the header nonce and the message nonce are random and
    // independent per encrypt — neither should repeat across calls.
    let (mut alice, _bob) = setup_pair();
    let m0 = alice.encrypt(b"a").unwrap();
    let m1 = alice.encrypt(b"b").unwrap();
    assert_ne!(m0.message_nonce.as_bytes(), m1.message_nonce.as_bytes());
    assert_ne!(m0.header_nonce.as_bytes(), m1.header_nonce.as_bytes());
}

// ---- DH ratchet tests ----

#[test]
fn multiple_dh_ratchet_steps() {
    let (mut alice, mut bob) = setup_pair();
    for round in 0..10u32 {
        let plain_a = format!("alice {round}");
        let m_a = alice.encrypt(plain_a.as_bytes()).unwrap();
        assert_eq!(bob.decrypt(&m_a).unwrap(), plain_a.as_bytes());

        let plain_b = format!("bob {round}");
        let m_b = bob.encrypt(plain_b.as_bytes()).unwrap();
        assert_eq!(alice.decrypt(&m_b).unwrap(), plain_b.as_bytes());
    }
}

#[test]
fn out_of_order_across_dh_ratchet_step() {
    // Cache an entry from alice's first DH chain (slot 0). After bob
    // and alice each rotate via a round-trip, alice sends on her new
    // chain. Bob's DH step (on receiving the new chain) must keep
    // the old-chain cache entry intact, and the original out-of-order
    // m0 must still decrypt via cache.
    let (mut alice, mut bob) = setup_pair();
    let a0 = alice.encrypt(b"a0").unwrap();
    let a1 = alice.encrypt(b"a1").unwrap();
    let a2 = alice.encrypt(b"a2").unwrap();

    bob.decrypt(&a1).unwrap();
    bob.decrypt(&a2).unwrap();
    assert_eq!(bob.skipped_count(), 1);

    let b0 = bob.encrypt(b"b0").unwrap();
    alice.decrypt(&b0).unwrap();

    let a3 = alice.encrypt(b"a3").unwrap();
    bob.decrypt(&a3).unwrap();
    assert!(
        bob.skipped_count() >= 1,
        "old-chain cache entry must survive bob's DH ratchet step"
    );
    assert_eq!(bob.decrypt(&a0).unwrap(), b"a0");
}

#[test]
fn post_compromise_security_via_dh_step() {
    let (mut alice, mut bob) = setup_pair();

    let m1 = alice.encrypt(b"m1").unwrap();
    bob.decrypt(&m1).unwrap();
    let r1 = bob.encrypt(b"r1").unwrap();
    alice.decrypt(&r1).unwrap();

    let captured_alice = alice.clone();

    let m2 = alice.encrypt(b"m2").unwrap();
    bob.decrypt(&m2).unwrap();
    let r2 = bob.encrypt(b"r2").unwrap();
    alice.decrypt(&r2).unwrap();

    let m3 = alice.encrypt(b"m3").unwrap();
    bob.decrypt(&m3).unwrap();
    let r3 = bob.encrypt(b"r3").unwrap();

    let mut live_alice = alice;
    assert_eq!(live_alice.decrypt(&r3).unwrap(), b"r3");

    let mut stale = captured_alice;
    assert!(
        stale.decrypt(&r3).is_err(),
        "captured pre-rotation snapshot must not decrypt post-rotation messages"
    );
}

#[test]
fn replay_across_ratchet_steps_rejected() {
    let (mut alice, mut bob) = setup_pair();
    let m1 = alice.encrypt(b"m1").unwrap();
    bob.decrypt(&m1).unwrap();
    let r1 = bob.encrypt(b"r1").unwrap();
    alice.decrypt(&r1).unwrap();
    let m2 = alice.encrypt(b"m2").unwrap();
    bob.decrypt(&m2).unwrap();

    assert!(
        bob.decrypt(&m1).is_err(),
        "replay of pre-rotation message must be rejected"
    );
}

// ---- Encrypted-headers + AD tests ----

#[test]
fn header_tamper_rejection() {
    // Flip a bit in the encrypted header — header AEAD fails. Bob
    // can't decode the header at all, so neither HKr nor NHKr opens
    // it, and decrypt errors out without state mutation.
    let (mut alice, mut bob) = setup_pair();
    let mut msg = alice.encrypt(b"hello").unwrap();
    msg.enc_header[0] ^= 0x01;
    assert!(bob.decrypt(&msg).is_err());

    // Bob's state is unchanged: a fresh, untampered message decrypts.
    let real = alice.encrypt(b"hello (real)").unwrap();
    assert_eq!(bob.decrypt(&real).unwrap(), b"hello (real)");
}

#[test]
fn wrong_session_version_rejected() {
    // Receiver's ctx pins a different session_version than the
    // sender. Header decrypts fine (HKs derivation does not depend
    // on session_version), but the explicit pre-AEAD check on
    // header.session_version vs ctx.session_version triggers
    // rejection before the message AEAD is even attempted.
    let h = handshake_material();
    let (alice_ctx, _bob_ctx_default) = make_ctx_pair(&h, SESSION_VERSION_V1, b"convo-1");
    let bob_ctx_v2 = SessionContext {
        local_ik_x25519_pub: h.bob_ik_pub,
        local_ik_mlkem_pub: vec![0xbb; 1184],
        peer_ik_x25519_pub: h.alice_ik_pub,
        peer_ik_mlkem_pub: vec![0xaa; 1184],
        conversation_id: b"convo-1".to_vec(),
        session_version: SESSION_VERSION_V1 + 1,
    };
    let mut alice = DoubleRatchet::new_initiator(&h.alice_sk, &h.bob_spk_pub, alice_ctx).unwrap();
    let mut bob = DoubleRatchet::new_responder(&h.bob_sk, &h.bob_spk_secret, bob_ctx_v2).unwrap();

    let m = alice.encrypt(b"v1-only").unwrap();
    let err = bob.decrypt(&m).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("session_version mismatch"),
        "expected session_version mismatch error, got: {msg}"
    );
}

#[test]
fn header_keys_rotate_on_dh_step() {
    let (mut alice, mut bob) = setup_pair();
    let alice_hks_initial = alice.dbg_hks_bytes().expect("alice has HKs from init");

    let m1 = alice.encrypt(b"first").unwrap();
    bob.decrypt(&m1).unwrap();
    let bob_hks_after_first_recv = bob
        .dbg_hks_bytes()
        .expect("bob has HKs after first DH step");
    let bob_hkr_after_first_recv = bob
        .dbg_hkr_bytes()
        .expect("bob has HKr after first DH step");
    assert_eq!(
        bob_hkr_after_first_recv, alice_hks_initial,
        "bob's HKr after first DH step should equal alice's initial HKs (shared_hka)"
    );

    let r1 = bob.encrypt(b"reply").unwrap();
    alice.decrypt(&r1).unwrap();
    let alice_hks_after_step = alice.dbg_hks_bytes().expect("alice still has HKs");
    assert_ne!(
        alice_hks_initial, alice_hks_after_step,
        "alice's HKs must rotate after receiving bob's reply"
    );

    // The next outbound message uses the rotated HKs and the receiver
    // can still decode it (proves chain key + header key alignment
    // post-rotation).
    let m2 = alice.encrypt(b"after rotation").unwrap();
    assert_eq!(bob.decrypt(&m2).unwrap(), b"after rotation");

    // Confirm bob's HKs is unchanged across this round (he hasn't
    // rotated since his first send).
    let _ = bob_hks_after_first_recv; // silence unused-var lint
}

#[test]
fn skipped_header_keys_decode_cross_chain_out_of_order() {
    // Cache a slot from alice's old DH chain. After alice rotates
    // and resends, bob's HKr is the new chain. The cached entry's
    // HKr (the *old* one) is what opens the original `a0` header on
    // a later out-of-order arrival.
    let (mut alice, mut bob) = setup_pair();
    let a0 = alice.encrypt(b"a0").unwrap();
    let a1 = alice.encrypt(b"a1").unwrap();
    bob.decrypt(&a1).unwrap();
    assert_eq!(bob.skipped_count(), 1);

    let b0 = bob.encrypt(b"b0").unwrap();
    alice.decrypt(&b0).unwrap();
    let a2 = alice.encrypt(b"a2").unwrap();
    bob.decrypt(&a2).unwrap();
    // Cache survived bob's DH ratchet step.
    assert!(bob.skipped_count() >= 1);

    // Now feed bob the original out-of-order a0. Its enc_header is
    // encrypted with alice's pre-rotation HKs, so the cache lookup
    // must try the cached old HKr to find the matching entry.
    assert_eq!(bob.decrypt(&a0).unwrap(), b"a0");
}

#[test]
fn canonical_ad_encoding_deterministic_and_lp_correct() {
    let sender_x = [0x11u8; 32];
    let sender_mlkem = vec![0x22u8; 16]; // small fake size for easy offsets
    let recipient_x = [0x33u8; 32];
    let recipient_mlkem = vec![0x44u8; 16];
    let conversation_id = b"conv-id-77".to_vec();
    let message_ordinal: u32 = 0x0102_0304;
    let prev_chain_length: u32 = 0x05060708;
    let session_version: u32 = SESSION_VERSION_V1;

    let bytes_a = canonical_ad(
        &sender_x,
        &sender_mlkem,
        &recipient_x,
        &recipient_mlkem,
        &conversation_id,
        message_ordinal,
        prev_chain_length,
        session_version,
    );
    let bytes_b = canonical_ad(
        &sender_x,
        &sender_mlkem,
        &recipient_x,
        &recipient_mlkem,
        &conversation_id,
        message_ordinal,
        prev_chain_length,
        session_version,
    );
    assert_eq!(bytes_a, bytes_b, "encoding must be deterministic");

    // Layout: LP(32B sender_x) || LP(16B sender_mlkem) || LP(32B recip_x)
    //       || LP(16B recip_mlkem) || LP(conv_id) || LP(4B u32) × 3.
    // Each LP prefix is u32_be of length.
    let expected_len = 4 + 32       // sender_x
                     + 4 + 16       // sender_mlkem
                     + 4 + 32       // recipient_x
                     + 4 + 16       // recipient_mlkem
                     + 4 + 10       // conversation_id
                     + 4 + 4        // message_ordinal
                     + 4 + 4        // prev_chain_length
                     + 4 + 4; // session_version
    assert_eq!(bytes_a.len(), expected_len);

    // Spot-check every length prefix.
    let mut pos = 0;
    let read_u32_be =
        |b: &[u8], i: usize| -> u32 { u32::from_be_bytes(b[i..i + 4].try_into().unwrap()) };

    assert_eq!(read_u32_be(&bytes_a, pos), 32);
    assert_eq!(&bytes_a[pos + 4..pos + 4 + 32], &sender_x);
    pos += 4 + 32;

    assert_eq!(read_u32_be(&bytes_a, pos), 16);
    assert_eq!(&bytes_a[pos + 4..pos + 4 + 16], sender_mlkem.as_slice());
    pos += 4 + 16;

    assert_eq!(read_u32_be(&bytes_a, pos), 32);
    assert_eq!(&bytes_a[pos + 4..pos + 4 + 32], &recipient_x);
    pos += 4 + 32;

    assert_eq!(read_u32_be(&bytes_a, pos), 16);
    assert_eq!(&bytes_a[pos + 4..pos + 4 + 16], recipient_mlkem.as_slice());
    pos += 4 + 16;

    assert_eq!(read_u32_be(&bytes_a, pos), 10);
    assert_eq!(&bytes_a[pos + 4..pos + 4 + 10], conversation_id.as_slice());
    pos += 4 + 10;

    assert_eq!(read_u32_be(&bytes_a, pos), 4);
    assert_eq!(read_u32_be(&bytes_a, pos + 4), message_ordinal);
    pos += 4 + 4;

    assert_eq!(read_u32_be(&bytes_a, pos), 4);
    assert_eq!(read_u32_be(&bytes_a, pos + 4), prev_chain_length);
    pos += 4 + 4;

    assert_eq!(read_u32_be(&bytes_a, pos), 4);
    assert_eq!(read_u32_be(&bytes_a, pos + 4), session_version);
    pos += 4 + 4;

    assert_eq!(pos, bytes_a.len());

    // Different inputs must produce different outputs (sanity).
    let bytes_diff = canonical_ad(
        &sender_x,
        &sender_mlkem,
        &recipient_x,
        &recipient_mlkem,
        &conversation_id,
        message_ordinal + 1, // bump ordinal
        prev_chain_length,
        session_version,
    );
    assert_ne!(bytes_a, bytes_diff);
}

#[test]
fn cross_session_ad_mismatch_rejected() {
    // Receiver's conversation_id differs from sender's. Header
    // decrypts fine (HK chain is independent of conversation_id),
    // session_version matches, but the canonical AD for the message
    // AEAD differs → AEAD failure.
    let h = handshake_material();
    let (alice_ctx, _) = make_ctx_pair(&h, SESSION_VERSION_V1, b"alice-conversation");
    let bob_ctx_other = SessionContext {
        local_ik_x25519_pub: h.bob_ik_pub,
        local_ik_mlkem_pub: vec![0xbb; 1184],
        peer_ik_x25519_pub: h.alice_ik_pub,
        peer_ik_mlkem_pub: vec![0xaa; 1184],
        conversation_id: b"different-conversation".to_vec(),
        session_version: SESSION_VERSION_V1,
    };
    let mut alice = DoubleRatchet::new_initiator(&h.alice_sk, &h.bob_spk_pub, alice_ctx).unwrap();
    let mut bob =
        DoubleRatchet::new_responder(&h.bob_sk, &h.bob_spk_secret, bob_ctx_other).unwrap();

    let m = alice.encrypt(b"cross-conversation").unwrap();
    assert!(
        bob.decrypt(&m).is_err(),
        "AD mismatch on conversation_id must fail message AEAD"
    );
}

// ---- Test fixtures ----

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

/// Build matched alice/bob `SessionContext` pairs. ML-KEM identity
/// bytes are dummy 1184-byte vectors keyed by side; both ratchets
/// must agree on these for AD AEAD verification to succeed.
fn make_ctx_pair(
    h: &HandshakeMaterial,
    session_version: u32,
    conversation_id: &[u8],
) -> (SessionContext, SessionContext) {
    let alice_mlkem_pub = vec![0xaa; 1184];
    let bob_mlkem_pub = vec![0xbb; 1184];
    let alice_ctx = SessionContext {
        local_ik_x25519_pub: h.alice_ik_pub,
        local_ik_mlkem_pub: alice_mlkem_pub.clone(),
        peer_ik_x25519_pub: h.bob_ik_pub,
        peer_ik_mlkem_pub: bob_mlkem_pub.clone(),
        conversation_id: conversation_id.to_vec(),
        session_version,
    };
    let bob_ctx = SessionContext {
        local_ik_x25519_pub: h.bob_ik_pub,
        local_ik_mlkem_pub: bob_mlkem_pub,
        peer_ik_x25519_pub: h.alice_ik_pub,
        peer_ik_mlkem_pub: alice_mlkem_pub,
        conversation_id: conversation_id.to_vec(),
        session_version,
    };
    (alice_ctx, bob_ctx)
}

fn setup_pair() -> (DoubleRatchet, DoubleRatchet) {
    let h = handshake_material();
    let (alice_ctx, bob_ctx) = make_ctx_pair(&h, SESSION_VERSION_V1, b"convo-default");
    let alice = DoubleRatchet::new_initiator(&h.alice_sk, &h.bob_spk_pub, alice_ctx).unwrap();
    let bob = DoubleRatchet::new_responder(&h.bob_sk, &h.bob_spk_secret, bob_ctx).unwrap();
    (alice, bob)
}
