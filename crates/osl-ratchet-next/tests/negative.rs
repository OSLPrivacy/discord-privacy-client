//! Negative tests: everything that must fail, must fail *closed*.
//!
//! "Fail closed" here means two things, and both are asserted:
//!
//! 1. The operation returns an error (never a wrong plaintext).
//! 2. The session is left **bit-for-bit unchanged**, so a hostile or
//!    corrupted message cannot wedge a conversation. This is the
//!    property that makes an injected-garbage denial-of-service
//!    impossible: an attacker who cannot forge a header cannot even
//!    make the receiver spend a chain-key derivation.

use osl_ratchet_next::test_support::{established_pair, seeded_rng, tamper_byte, Harness};
use osl_ratchet_next::{Error, Session};
use rand::RngCore;

/// Flip one bit at every byte offset of a real message. Every variant
/// must be rejected, and the session must survive all of them.
#[test]
fn every_single_bit_flip_is_rejected_and_the_session_survives() {
    let (mut alice, mut bob, mut rng) = established_pair(300);
    let good = alice.encrypt(0, b"the quick brown fox", &mut rng).expect("encrypt");
    let len = osl_ratchet_next::test_support::wire_len(&good);
    assert!(len > 80);

    let before = bob.export_state().expect("export");
    let mut rejected = 0;
    for offset in 0..len {
        for bit in [0x01u8, 0x80] {
            let Some(bad) = tamper_byte(&good, offset, bit) else {
                continue;
            };
            if bad == good {
                continue;
            }
            match bob.decrypt(&bad, &mut rng) {
                Ok(o) => panic!("byte {offset} bit {bit:#x} decrypted to {:?}", o.plaintext),
                Err(_) => rejected += 1,
            }
            assert_eq!(
                bob.export_state().expect("export"),
                before,
                "byte {offset} bit {bit:#x} mutated the session"
            );
        }
    }
    assert!(rejected >= len, "not enough variants exercised: {rejected}");

    // The untampered original still opens afterwards.
    assert_eq!(
        bob.decrypt(&good, &mut rng).expect("decrypt").plaintext,
        b"the quick brown fox"
    );
}

#[test]
fn truncation_at_every_length_is_rejected() {
    use base64::Engine as _;
    let (mut alice, mut bob, mut rng) = established_pair(301);
    let good = alice.encrypt(0, b"truncate me", &mut rng).expect("encrypt");
    let raw = base64::engine::general_purpose::STANDARD
        .decode(good.strip_prefix("DPC0::").expect("prefix"))
        .expect("b64");

    let before = bob.export_state().expect("export");
    for cut in 0..raw.len() {
        let wire = format!(
            "DPC0::{}",
            base64::engine::general_purpose::STANDARD.encode(&raw[..cut])
        );
        assert!(
            bob.decrypt(&wire, &mut rng).is_err(),
            "truncation to {cut} bytes was accepted"
        );
        assert_eq!(bob.export_state().expect("export"), before);
    }
    // Appended garbage is rejected too (the body AEAD covers it).
    let mut extended = raw.clone();
    extended.extend_from_slice(b"extra");
    let wire = format!(
        "DPC0::{}",
        base64::engine::general_purpose::STANDARD.encode(&extended)
    );
    assert!(bob.decrypt(&wire, &mut rng).is_err());
}

#[test]
fn a_header_from_one_message_on_the_body_of_another_is_rejected() {
    use base64::Engine as _;
    let (mut alice, mut bob, mut rng) = established_pair(302);
    let a = alice.encrypt(0, b"message A", &mut rng).expect("encrypt");
    let b = alice.encrypt(0, b"message B", &mut rng).expect("encrypt");

    let dec = |w: &str| {
        base64::engine::general_purpose::STANDARD
            .decode(w.strip_prefix("DPC0::").expect("prefix"))
            .expect("b64")
    };
    let (ra, rb) = (dec(&a), dec(&b));
    // Header region is version(1)+flags(1)+nonce(12)+len(1)+ct; the
    // bodies are the tails. Splice A's framing onto B's body.
    let split = ra.len() - 27; // 11 plaintext bytes + 16 tag
    let mut spliced = ra[..split].to_vec();
    spliced.extend_from_slice(&rb[rb.len() - 27..]);
    let wire = format!(
        "DPC0::{}",
        base64::engine::general_purpose::STANDARD.encode(&spliced)
    );
    assert!(
        bob.decrypt(&wire, &mut rng).is_err(),
        "a spliced header/body pair was accepted"
    );
    // Both originals still work.
    assert_eq!(bob.decrypt(&a, &mut rng).expect("a").plaintext, b"message A");
    assert_eq!(bob.decrypt(&b, &mut rng).expect("b").plaintext, b"message B");
}

#[test]
fn a_message_from_a_different_session_is_rejected() {
    let (mut alice, _b1, mut rng) = established_pair(303);
    let (_a2, mut bob2, _r2) = established_pair(304);
    let wire = alice.encrypt(0, b"wrong session", &mut rng).expect("encrypt");
    assert_eq!(bob2.decrypt(&wire, &mut rng), Err(Error::AuthFailed));
}

#[test]
fn malformed_framing_is_rejected_by_category() {
    let (mut alice, mut bob, mut rng) = established_pair(305);
    let good = alice.encrypt(0, b"x", &mut rng).expect("encrypt");

    assert_eq!(bob.decrypt("no prefix at all", &mut rng), Err(Error::BadPrefix));
    assert_eq!(bob.decrypt("DPC0::!!!not base64!!!", &mut rng), Err(Error::Base64));
    assert_eq!(bob.decrypt("DPC0::", &mut rng).err(), Some(Error::Malformed("empty blob")));

    // A v=3 blob must report WrongVersion so a router can fall through
    // to the existing decoders rather than treating it as corruption.
    use base64::Engine as _;
    let v3 = format!(
        "DPC0::{}",
        base64::engine::general_purpose::STANDARD.encode([0x03u8, 0x00, 0x01, 0x02])
    );
    assert_eq!(
        bob.decrypt(&v3, &mut rng),
        Err(Error::WrongVersion {
            got: 0x03,
            expected: osl_ratchet_next::WIRE_VERSION_RN,
        })
    );

    // The real message still works after all of that.
    assert_eq!(bob.decrypt(&good, &mut rng).expect("decrypt").plaintext, b"x");
}

#[test]
fn random_garbage_never_panics_and_never_opens() {
    let (mut alice, mut bob, mut rng) = established_pair(306);
    let good = alice.encrypt(0, b"real", &mut rng).expect("encrypt");
    let before = bob.export_state().expect("export");

    let mut junk = seeded_rng(0x6A6B);
    use base64::Engine as _;
    for len in 0..400usize {
        let mut buf = vec![0u8; len];
        junk.fill_bytes(&mut buf);
        for version in [0x06u8, 0x03, 0xff] {
            if let Some(first) = buf.first_mut() {
                *first = version;
            }
            let wire = format!(
                "DPC0::{}",
                base64::engine::general_purpose::STANDARD.encode(&buf)
            );
            assert!(bob.decrypt(&wire, &mut rng).is_err(), "junk accepted");
        }
    }
    assert_eq!(bob.export_state().expect("export"), before);
    assert_eq!(bob.decrypt(&good, &mut rng).expect("decrypt").plaintext, b"real");
}

#[test]
fn a_forged_bootstrap_cannot_establish_a_session() {
    let mut h = Harness::new(307);
    let real = h.alice_send(b"legit").expect("send");
    let len = osl_ratchet_next::test_support::wire_len(&real);

    // Tamper inside the preamble region (bytes 2..1154): the header
    // AEAD covers it, so every variant must fail.
    let mut rng = seeded_rng(1);
    for offset in [2usize, 40, 70, 200, 900, 1100] {
        if offset >= len {
            continue;
        }
        let bad = tamper_byte(&real, offset, 0x01).expect("tamper");
        assert!(
            Session::accept(&h.bob_prekeys, &bad, h.params, &mut rng).is_err(),
            "forged preamble at {offset} was accepted"
        );
    }
    // The real one still establishes.
    assert_eq!(h.bob_recv(&real).expect("recv").plaintext, b"legit");
}

#[test]
fn a_non_bootstrap_message_cannot_start_a_session() {
    let mut h = Harness::established(308);
    let w = h.alice_send(b"mid-conversation").expect("send");
    let mut rng = seeded_rng(2);
    // Alice has stopped attaching the preamble, so `accept` must refuse
    // rather than silently producing a broken session.
    assert!(matches!(
        Session::accept(&h.bob_prekeys, &w, h.params, &mut rng),
        Err(Error::Malformed(_))
    ));
}

#[test]
fn corrupt_session_state_is_refused_not_mis_parsed() {
    let (alice, _b, _r) = established_pair(309);
    let good = alice.export_state().expect("export");
    assert!(Session::import_state(&good).is_ok());

    // Wrong format version.
    let mut wrong = good.clone();
    if let Some(b) = wrong.first_mut() {
        *b = 0xEE;
    }
    assert!(matches!(
        Session::import_state(&wrong),
        Err(Error::BadStateFormat)
    ));

    // Truncation at every length.
    for cut in 0..good.len() {
        assert!(
            Session::import_state(&good[..cut]).is_err(),
            "truncated state at {cut} was accepted"
        );
    }
    // Trailing bytes.
    let mut extra = good.clone();
    extra.push(0);
    assert!(matches!(
        Session::import_state(&extra),
        Err(Error::BadStateFormat)
    ));
}

#[test]
fn a_restored_session_cannot_reuse_a_consumed_message_key() {
    // Rolling a session back to an older state must not turn a
    // previously-consumed message into a replayable one *for the
    // rolled-forward session*. (Rollback of the persisted blob itself
    // is out of scope — see THREAT-MODEL.md.)
    let (mut alice, mut bob, mut rng) = established_pair(310);
    let w = alice.encrypt(0, b"once", &mut rng).expect("encrypt");
    assert_eq!(bob.decrypt(&w, &mut rng).expect("decrypt").plaintext, b"once");
    let after = bob.export_state().expect("export");

    let mut restored = Session::import_state(&after).expect("import");
    assert_eq!(restored.decrypt(&w, &mut rng), Err(Error::AuthFailed));
}
