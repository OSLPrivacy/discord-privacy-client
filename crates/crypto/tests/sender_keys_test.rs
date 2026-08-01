//! Sender keys tests — chain advance, encrypted-headers round-trip,
//! skipped-key cache, rotation, AD/version validation, multi-peer
//! routing.
//!
//! Coverage:
//! - Chain key advance changes the underlying state (one-way HKDF
//!   step — bytes differ pre/post advance).
//! - Encrypt + decrypt round-trip on a single chain.
//! - Multiple consecutive sequential messages on one chain.
//! - Out-of-order delivery within window via skipped-key cache.
//! - FIFO eviction at the per-chain cap (1000).
//! - TTL expiry: cached entries past 30 days are swept on next access.
//! - Excessive gap rejected without cache pollution or counter advance.
//! - Replay rejection (consumed cache entries cannot replay; sequential
//!   replays fail the forward search).
//! - Rotation: pre-rotation skipped-key cache survives rotation and
//!   still decrypts old-chain messages.
//! - Rotation: post-rotation state alone (no cache entries) cannot
//!   decrypt pre-rotation messages.
//! - Header tampering rejected; receiver state unchanged.
//! - Wrong session_version rejected with explicit error before the
//!   message AEAD.
//! - `canonical_ad_sender_keys` is deterministic and emits the
//!   expected length-prefix layout.
//! - Multiple peer senders to the same receiver: per-peer
//!   `ReceiverChain` isolation; cross-routing rejected.

use crypto::sender_keys::{
    canonical_ad_sender_keys, PhysicalDeviceId, ReceiverChain, SenderChain, SenderContext,
    SenderKeyState, MAX_RECEIVER_CHAINS_PER_PEER, MAX_SKIPPED_PER_CHAIN, SESSION_VERSION_V1,
    SKIPPED_KEY_TTL,
};
use crypto::x25519;
use std::time::{Duration, SystemTime};

// ---- Chain advance ----

#[test]
fn chain_advance_is_one_way() {
    // Each `encrypt` pulls (MK, HK) from CK_n and advances CK to
    // CK_{n+1}. The underlying bytes must change after every advance,
    // and CK_{n} cannot be derived from CK_{n+1} (HKDF is one-way —
    // we assert structural distinctness here; the cryptographic
    // one-way property is inherited from HKDF-SHA256).
    let mut sender = SenderChain::new().unwrap();
    let ctx = make_sender_ctx(0xaa, b"group", SESSION_VERSION_V1);

    let ck0 = sender.dbg_ck_bytes();
    let _ = sender.encrypt(b"a", &ctx).unwrap();
    let ck1 = sender.dbg_ck_bytes();
    assert_ne!(ck0, ck1, "encrypt must advance the chain");

    let _ = sender.encrypt(b"b", &ctx).unwrap();
    let ck2 = sender.dbg_ck_bytes();
    assert_ne!(ck1, ck2);
    assert_ne!(ck0, ck2);
    assert_eq!(sender.current_n(), 2);
}

// ---- Round-trip ----

#[test]
fn round_trip_single_message() {
    let (mut sender, mut receiver, ctx) = setup();
    let m = sender.encrypt(b"hello, sender keys", &ctx).unwrap();
    assert_eq!(receiver.decrypt(&m, &ctx).unwrap(), b"hello, sender keys");
}

#[test]
fn round_trip_multiple_messages_one_chain() {
    let (mut sender, mut receiver, ctx) = setup();
    for i in 0u32..16 {
        let plaintext = format!("group msg {i}");
        let m = sender.encrypt(plaintext.as_bytes(), &ctx).unwrap();
        assert_eq!(receiver.decrypt(&m, &ctx).unwrap(), plaintext.as_bytes());
    }
    assert_eq!(sender.current_n(), 16);
    assert_eq!(receiver.current_n(), 16);
    assert_eq!(receiver.skipped_count(), 0);
}

// ---- Skipped-key cache ----

#[test]
fn out_of_order_within_window() {
    let (mut sender, mut receiver, ctx) = setup();
    let m0 = sender.encrypt(b"m0", &ctx).unwrap();
    let m1 = sender.encrypt(b"m1", &ctx).unwrap();
    let m2 = sender.encrypt(b"m2", &ctx).unwrap();
    let m3 = sender.encrypt(b"m3", &ctx).unwrap();

    assert_eq!(receiver.decrypt(&m1, &ctx).unwrap(), b"m1");
    assert_eq!(receiver.skipped_count(), 1);
    assert_eq!(receiver.decrypt(&m3, &ctx).unwrap(), b"m3");
    assert_eq!(receiver.skipped_count(), 2);
    assert_eq!(receiver.decrypt(&m2, &ctx).unwrap(), b"m2");
    assert_eq!(receiver.skipped_count(), 1);
    assert_eq!(receiver.decrypt(&m0, &ctx).unwrap(), b"m0");
    assert_eq!(receiver.skipped_count(), 0);
}

#[test]
fn cache_evicts_oldest_at_cap() {
    let (mut sender, mut receiver, ctx) = setup();
    let total = MAX_SKIPPED_PER_CHAIN + 4;
    let mut msgs = Vec::with_capacity(total);
    for i in 0..total {
        msgs.push(sender.encrypt(format!("m{i}").as_bytes(), &ctx).unwrap());
    }

    let cap = MAX_SKIPPED_PER_CHAIN;
    receiver.decrypt(&msgs[cap], &ctx).unwrap();
    assert_eq!(receiver.skipped_count(), cap);

    receiver.decrypt(&msgs[cap + 3], &ctx).unwrap();
    assert_eq!(
        receiver.skipped_count(),
        cap,
        "cache stays at cap after eviction"
    );

    assert!(receiver.decrypt(&msgs[0], &ctx).is_err(), "slot 0 evicted");
    assert!(receiver.decrypt(&msgs[1], &ctx).is_err(), "slot 1 evicted");
    assert_eq!(receiver.decrypt(&msgs[2], &ctx).unwrap(), b"m2");
    let expected = format!("m{}", cap + 1);
    assert_eq!(
        receiver.decrypt(&msgs[cap + 1], &ctx).unwrap(),
        expected.as_bytes()
    );
}

#[test]
fn cached_keys_expire_after_ttl() {
    let (mut sender, mut receiver, ctx) = setup();
    let m0 = sender.encrypt(b"m0", &ctx).unwrap();
    let m1 = sender.encrypt(b"m1", &ctx).unwrap();

    let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    receiver.decrypt_at(&m1, &ctx, t0).unwrap();
    assert_eq!(receiver.skipped_count(), 1, "slot 0 cached at t0");

    let later = t0 + SKIPPED_KEY_TTL + Duration::from_secs(1);
    assert!(
        receiver.decrypt_at(&m0, &ctx, later).is_err(),
        "cached key past TTL must not decrypt the original message"
    );
    assert_eq!(receiver.skipped_count(), 0, "expired entry must be swept");
}

#[test]
fn excessive_gap_rejected_without_cache_pollution() {
    // Sender produces MAX+501 messages so the last one is at slot
    // MAX+500. Receiver's first decrypt of that single message would
    // require advancing past MAX+500 slots, which exceeds the cap.
    let (mut sender, mut receiver, ctx) = setup();
    let total = MAX_SKIPPED_PER_CHAIN + 501;
    let mut last = None;
    for i in 0..total {
        let m = sender.encrypt(format!("m{i}").as_bytes(), &ctx).unwrap();
        if i == total - 1 {
            last = Some(m);
        }
    }

    let bogus = last.expect("sender produced the last message");
    assert!(receiver.decrypt(&bogus, &ctx).is_err());
    assert_eq!(
        receiver.skipped_count(),
        0,
        "oversized gap must not pollute cache"
    );
    assert_eq!(
        receiver.current_n(),
        0,
        "counter must not advance on oversized-gap rejection"
    );
}

#[test]
fn replay_rejection() {
    let (mut sender, mut receiver, ctx) = setup();
    let m0 = sender.encrypt(b"once", &ctx).unwrap();
    assert_eq!(receiver.decrypt(&m0, &ctx).unwrap(), b"once");

    // Replay attempt: receiver has advanced past slot 0 sequentially
    // (cache empty), and the forward search from slot 1 onward
    // cannot match a slot-0 header. Reject.
    assert!(
        receiver.decrypt(&m0, &ctx).is_err(),
        "sequential replay must be rejected"
    );
}

// ---- Rotation ----

#[test]
fn rotation_old_chain_messages_decode_via_cache() {
    // Cache an entry from chain 0 (slot 0). After both sides rotate
    // to chain 1, the cached entry's `(chain_id=0, n=0)` is still
    // looked up and decrypts the original out-of-order message.
    let (mut sender, mut receiver, ctx) = setup();

    let m0 = sender.encrypt(b"m0", &ctx).unwrap();
    let m1 = sender.encrypt(b"m1", &ctx).unwrap();

    // Receiver gets m1 first → caches slot 0 (chain_id=0, n=0).
    receiver.decrypt(&m1, &ctx).unwrap();
    assert_eq!(receiver.skipped_count(), 1);

    // Sender rotates; receiver follows with the new (chain_id, root).
    sender.rotate().unwrap();
    receiver
        .rotate_to(sender.current_chain_id(), &sender.rotation_root_bytes())
        .unwrap();
    assert_eq!(
        receiver.skipped_count(),
        1,
        "skipped-key cache survives rotation"
    );

    // Late-arriving old-chain m0 still decrypts via the cached entry.
    assert_eq!(receiver.decrypt(&m0, &ctx).unwrap(), b"m0");
    assert_eq!(receiver.skipped_count(), 0);
}

#[test]
fn rotation_state_cannot_decrypt_old_chain_without_cache() {
    // Sequential pre-rotation delivery leaves the cache empty.
    // After rotation, the only chain state the receiver has is the
    // new chain — it has no way to derive any chain-0 message key,
    // so a chain-0 message arrives DOA.
    let (mut sender, mut receiver, ctx) = setup();

    let m0 = sender.encrypt(b"m0", &ctx).unwrap();
    receiver.decrypt(&m0, &ctx).unwrap();
    assert_eq!(receiver.skipped_count(), 0);

    sender.rotate().unwrap();
    receiver
        .rotate_to(sender.current_chain_id(), &sender.rotation_root_bytes())
        .unwrap();
    assert_eq!(receiver.skipped_count(), 0);

    // Replay / late delivery of m0 — no cache entry, current chain
    // doesn't have the right keys → reject.
    assert!(
        receiver.decrypt(&m0, &ctx).is_err(),
        "post-rotation state without cache cannot decrypt pre-rotation messages"
    );
}

// ---- Encrypted headers + AD ----

#[test]
fn header_tamper_rejection() {
    let (mut sender, mut receiver, ctx) = setup();
    let mut msg = sender.encrypt(b"hello", &ctx).unwrap();
    msg.enc_header[0] ^= 0x01;
    assert!(receiver.decrypt(&msg, &ctx).is_err());

    // Receiver state is unchanged: a fresh, untampered message
    // decrypts. The new message has counter=1 (sender already
    // advanced); the receiver's forward search caches slot 0 then
    // matches at slot 1.
    let real = sender.encrypt(b"real", &ctx).unwrap();
    assert_eq!(receiver.decrypt(&real, &ctx).unwrap(), b"real");
}

#[test]
fn wrong_session_version_rejected() {
    let mut sender = SenderChain::new().unwrap();
    let ctx_v1 = make_sender_ctx(0xaa, b"group", SESSION_VERSION_V1);
    let ctx_v2 = make_sender_ctx(0xaa, b"group", SESSION_VERSION_V1 + 1);

    let mut receiver = ReceiverChain::install(
        sender.current_chain_id(),
        &sender.rotation_root_bytes(),
        sender.physical_device_id(),
    )
    .unwrap();
    let m = sender.encrypt(b"v1-only", &ctx_v1).unwrap();
    let err = receiver.decrypt(&m, &ctx_v2).unwrap_err();
    let err_msg = format!("{err}");
    assert!(
        err_msg.contains("session_version mismatch"),
        "expected session_version mismatch error, got: {err_msg}"
    );
}

#[test]
fn canonical_ad_encoding_deterministic_and_lp_correct() {
    let sender_x = [0x11u8; 32];
    let sender_mlkem = vec![0x22u8; 16]; // small fake size for easy offset checks
    let physical_device_id =
        crypto::sender_keys::PhysicalDeviceId::from_bytes([0x33u8; 32]).unwrap();
    let group_id = b"group-7".to_vec();
    let chain_id: u32 = 0x0102_0304;
    let n: u32 = 0x0506_0708;
    let prev: u32 = 0x09ab_cdef;
    let version: u32 = SESSION_VERSION_V1;

    let bytes_a = canonical_ad_sender_keys(
        &sender_x,
        &sender_mlkem,
        &physical_device_id,
        &group_id,
        chain_id,
        n,
        prev,
        version,
    );
    let bytes_b = canonical_ad_sender_keys(
        &sender_x,
        &sender_mlkem,
        &physical_device_id,
        &group_id,
        chain_id,
        n,
        prev,
        version,
    );
    assert_eq!(bytes_a, bytes_b, "encoding must be deterministic");

    // Layout: LP(32B sender_x) || LP(16B sender_mlkem)
    //       || LP(32B physical_device_id) || LP(7B group_id)
    //       || LP(4B chain_id_be) || LP(4B n_be)
    //       || LP(4B prev_be) || LP(4B version_be).
    let expected_len = 4 + 32   // sender_x
                     + 4 + 16   // sender_mlkem
                     + 4 + 32   // physical_device_id
                     + 4 + 7    // group_id
                     + 4 + 4    // chain_id
                     + 4 + 4    // n
                     + 4 + 4    // prev_chain_length
                     + 4 + 4; // session_version
    assert_eq!(bytes_a.len(), expected_len);

    let read_u32_be = |b: &[u8], i: usize| u32::from_be_bytes(b[i..i + 4].try_into().unwrap());
    let mut pos = 0;

    assert_eq!(read_u32_be(&bytes_a, pos), 32);
    assert_eq!(&bytes_a[pos + 4..pos + 4 + 32], &sender_x);
    pos += 4 + 32;

    assert_eq!(read_u32_be(&bytes_a, pos), 16);
    assert_eq!(&bytes_a[pos + 4..pos + 4 + 16], sender_mlkem.as_slice());
    pos += 4 + 16;

    assert_eq!(read_u32_be(&bytes_a, pos), 32);
    assert_eq!(
        &bytes_a[pos + 4..pos + 4 + 32],
        physical_device_id.as_bytes()
    );
    pos += 4 + 32;

    assert_eq!(read_u32_be(&bytes_a, pos), 7);
    assert_eq!(&bytes_a[pos + 4..pos + 4 + 7], group_id.as_slice());
    pos += 4 + 7;

    assert_eq!(read_u32_be(&bytes_a, pos), 4);
    assert_eq!(read_u32_be(&bytes_a, pos + 4), chain_id);
    pos += 4 + 4;

    assert_eq!(read_u32_be(&bytes_a, pos), 4);
    assert_eq!(read_u32_be(&bytes_a, pos + 4), n);
    pos += 4 + 4;

    assert_eq!(read_u32_be(&bytes_a, pos), 4);
    assert_eq!(read_u32_be(&bytes_a, pos + 4), prev);
    pos += 4 + 4;

    assert_eq!(read_u32_be(&bytes_a, pos), 4);
    assert_eq!(read_u32_be(&bytes_a, pos + 4), version);
    pos += 4 + 4;

    assert_eq!(pos, bytes_a.len());

    // Sensitivity: any single-field flip changes the bytes.
    let bumped = canonical_ad_sender_keys(
        &sender_x,
        &sender_mlkem,
        &physical_device_id,
        &group_id,
        chain_id,
        n + 1,
        prev,
        version,
    );
    assert_ne!(bytes_a, bumped);
}

// ---- Multi-peer routing ----

#[test]
fn multiple_peer_senders_to_same_receiver() {
    let mut alice_sender = SenderChain::new().unwrap();
    let mut bob_sender = SenderChain::new().unwrap();

    let alice_ctx = make_sender_ctx(0x11, b"group-1", SESSION_VERSION_V1);
    let bob_ctx = make_sender_ctx(0x22, b"group-1", SESSION_VERSION_V1);

    let mut carol = SenderKeyState::new();
    carol
        .install_receiver(
            b"alice".to_vec(),
            0,
            &alice_sender.rotation_root_bytes(),
            alice_sender.physical_device_id(),
        )
        .unwrap();
    carol
        .install_receiver(
            b"bob".to_vec(),
            0,
            &bob_sender.rotation_root_bytes(),
            bob_sender.physical_device_id(),
        )
        .unwrap();

    let m_a = alice_sender.encrypt(b"from alice", &alice_ctx).unwrap();
    let m_b = bob_sender.encrypt(b"from bob", &bob_ctx).unwrap();

    assert_eq!(
        carol.decrypt_from(b"alice", &m_a, &alice_ctx).unwrap(),
        b"from alice"
    );
    assert_eq!(
        carol.decrypt_from(b"bob", &m_b, &bob_ctx).unwrap(),
        b"from bob"
    );

    // Both send another round; per-peer chains stay isolated.
    let m_a2 = alice_sender.encrypt(b"alice 2", &alice_ctx).unwrap();
    let m_b2 = bob_sender.encrypt(b"bob 2", &bob_ctx).unwrap();
    assert_eq!(
        carol.decrypt_from(b"alice", &m_a2, &alice_ctx).unwrap(),
        b"alice 2"
    );
    assert_eq!(
        carol.decrypt_from(b"bob", &m_b2, &bob_ctx).unwrap(),
        b"bob 2"
    );

    // Cross-route: feeding alice's message to bob's chain fails —
    // bob's CK chain has unrelated keys, so the forward search
    // exhausts without a header match.
    let m_a3 = alice_sender.encrypt(b"alice 3", &alice_ctx).unwrap();
    assert!(
        carol.decrypt_from(b"bob", &m_a3, &alice_ctx).is_err(),
        "wrong receiver chain must reject the message"
    );
    // Alice's own chain still works.
    assert_eq!(
        carol.decrypt_from(b"alice", &m_a3, &alice_ctx).unwrap(),
        b"alice 3"
    );
}

#[test]
fn receiver_chains_per_peer_are_bounded_against_forged_device_ids() {
    let mut state = SenderKeyState::new();
    let root = [0xa5; 32];

    for id in 1u8..=200 {
        let installed = state
            .install_receiver(
                b"attacker-controlled-peer".to_vec(),
                0,
                &root,
                PhysicalDeviceId::from_bytes([id; 32]).unwrap(),
            )
            .unwrap();
        let expected_eviction = if usize::from(id) > MAX_RECEIVER_CHAINS_PER_PEER {
            Some(
                PhysicalDeviceId::from_bytes(
                    [(usize::from(id) - MAX_RECEIVER_CHAINS_PER_PEER) as u8; 32],
                )
                .unwrap(),
            )
        } else {
            None
        };
        assert_eq!(installed.evicted_physical_device_id, expected_eviction);
    }

    assert!(state
        .receiver_chain_for_physical_device(
            b"attacker-controlled-peer",
            PhysicalDeviceId::from_bytes([1; 32]).unwrap(),
        )
        .is_none());
    assert!(state
        .receiver_chain_for_physical_device(
            b"attacker-controlled-peer",
            PhysicalDeviceId::from_bytes([200; 32]).unwrap(),
        )
        .is_some());
    assert_eq!(
        (1u8..=200)
            .filter(|id| {
                state
                    .receiver_chain_for_physical_device(
                        b"attacker-controlled-peer",
                        PhysicalDeviceId::from_bytes([*id; 32]).unwrap(),
                    )
                    .is_some()
            })
            .count(),
        MAX_RECEIVER_CHAINS_PER_PEER,
    );
}

#[test]
fn successful_receiver_decrypt_refreshes_lru_before_eviction() {
    let peer = b"peer-with-many-devices";
    let ctx = make_sender_ctx(0x61, b"group-1", SESSION_VERSION_V1);
    let mut state = SenderKeyState::new();
    let mut first_sender = None;

    for id in 1..=MAX_RECEIVER_CHAINS_PER_PEER as u8 {
        let sender =
            SenderChain::new_for_physical_device(PhysicalDeviceId::from_bytes([id; 32]).unwrap())
                .unwrap();
        state
            .install_receiver(
                peer.to_vec(),
                sender.current_chain_id(),
                &sender.rotation_root_bytes(),
                sender.physical_device_id(),
            )
            .unwrap();
        if id == 1 {
            first_sender = Some(sender);
        }
    }

    let first_sender = first_sender.as_mut().unwrap();
    let message = first_sender.encrypt(b"refresh device one", &ctx).unwrap();
    assert_eq!(
        state.decrypt_from(peer, &message, &ctx).unwrap(),
        b"refresh device one"
    );

    let new_device =
        PhysicalDeviceId::from_bytes([(MAX_RECEIVER_CHAINS_PER_PEER + 1) as u8; 32]).unwrap();
    let evicted = state
        .install_receiver(peer.to_vec(), 0, &[0x3c; 32], new_device)
        .unwrap();
    assert_eq!(
        evicted.evicted_physical_device_id,
        Some(PhysicalDeviceId::from_bytes([2; 32]).unwrap()),
    );
}

// ---- Test fixtures ----

fn make_sender_ctx(ik_seed: u8, group_id: &[u8], session_version: u32) -> SenderContext {
    SenderContext {
        sender_ik_x25519_pub: x25519::PublicKey::from_bytes([ik_seed; 32]),
        sender_ik_mlkem_pub: vec![ik_seed; 1184],
        group_id: group_id.to_vec(),
        session_version,
    }
}

fn setup() -> (SenderChain, ReceiverChain, SenderContext) {
    let sender = SenderChain::new().unwrap();
    let receiver = ReceiverChain::install(
        sender.current_chain_id(),
        &sender.rotation_root_bytes(),
        sender.physical_device_id(),
    )
    .unwrap();
    let ctx = make_sender_ctx(0xaa, b"group-default", SESSION_VERSION_V1);
    (sender, receiver, ctx)
}

#[test]
fn a_replayed_skdm_cannot_rewind_the_receiver_chain() {
    // A captured SKDM is replayable by any channel observer, so `rotate_to` must
    // refuse to move the receiver BACKWARDS or sideways. Before the monotone
    // guard, replaying an SKDM with its original chain_id re-derived ck_0 and
    // reset n to 0, after which every message already delivered on that chain
    // decrypted again -- rewinding the very counter the replay refusal depends on.
    let (mut sender, mut receiver, ctx) = setup();

    let m0 = sender.encrypt(b"m0", &ctx).unwrap();
    receiver.decrypt(&m0, &ctx).unwrap();

    let old_chain = sender.current_chain_id();
    let old_root = sender.rotation_root_bytes();

    // A legitimate forward rotation is still accepted.
    sender.rotate().unwrap();
    receiver
        .rotate_to(sender.current_chain_id(), &sender.rotation_root_bytes())
        .expect("a forward rotation must still be accepted");
    let now_chain = sender.current_chain_id();
    assert!(now_chain > old_chain);

    // Replaying the ORIGINAL SKDM must be refused, not silently honoured.
    assert!(
        receiver.rotate_to(old_chain, &old_root).is_err(),
        "replaying an older SKDM must be refused"
    );
    // Re-offering the CURRENT chain must be a NO-OP, not an error: the send loop
    // periodically re-emits the same (chain_id, rotation_root) so a receiver that
    // missed the first SKDM recovers, and apply_skdm_recv is specified idempotent.
    // Erroring here turns every self-heal re-emit into a dispatch failure.
    receiver
        .rotate_to(now_chain, &sender.rotation_root_bytes())
        .expect("re-applying the current chain must be an idempotent no-op");

    // And the refusal must have left the receiver's state untouched, so the
    // replayed m0 still cannot be re-decrypted.
    assert!(
        receiver.decrypt(&m0, &ctx).is_err(),
        "a message already delivered must not decrypt again after a refused rotation"
    );
}
