//! End-to-end tests for the PQ epoch ratchet at the session level.
//!
//! The claim under test is the one the whole design exists for:
//!
//! > Fresh ML-KEM-768 entropy is folded into the root key repeatedly
//! > over the life of a session, both sides agree on exactly when, and
//! > the process never blocks or stalls message delivery — not under
//! > reordering, not under permanent loss.

use osl_ratchet_next::test_support::{established_pair_with_params, seeded_rng, wire_len};
use osl_ratchet_next::{PqParams, SessionParams, SkipParams};
use rand::Rng;

fn fast_pq() -> SessionParams {
    SessionParams {
        pq: PqParams {
            fragment_bytes: 128,
            // Deliberately aggressive so a test-sized conversation
            // exercises several epochs.
            rekey_interval: 4,
        },
        skip: SkipParams::default(),
    }
}

#[test]
fn pq_epochs_advance_and_both_sides_agree() {
    let (mut alice, mut bob, mut rng) = established_pair_with_params(101, fast_pq());
    for i in 0..600u32 {
        let body = format!("ping {i}").into_bytes();
        let w = alice.encrypt(0, &body, &mut rng).expect("encrypt");
        assert_eq!(bob.decrypt(&w, &mut rng).expect("decrypt").plaintext, body);

        let body = format!("pong {i}").into_bytes();
        let w = bob.encrypt(0, &body, &mut rng).expect("encrypt");
        assert_eq!(
            alice.decrypt(&w, &mut rng).expect("decrypt").plaintext,
            body
        );
    }

    assert!(
        alice.pq_mixed_epoch() >= 4,
        "PQ ratchet did not heal: alice mixed epoch {}",
        alice.pq_mixed_epoch()
    );
    assert_eq!(
        alice.pq_mixed_epoch(),
        bob.pq_mixed_epoch(),
        "the two sides disagree about which PQ epochs are mixed"
    );
    // Ownership alternates, so both sides must have contributed fresh
    // ML-KEM keypairs — not just one of them.
    assert!(alice.pq_mixed_epoch() >= 2);
}

#[test]
fn pq_healing_survives_heavy_permanent_loss() {
    let (mut alice, mut bob, mut rng) = established_pair_with_params(102, fast_pq());
    let mut driver = seeded_rng(0x105501);

    for i in 0..1500u32 {
        // 40% of everything vanishes forever, in both directions.
        let body = format!("a{i}").into_bytes();
        let w = alice.encrypt(0, &body, &mut rng).expect("encrypt");
        if !driver.gen_bool(0.4) {
            assert_eq!(bob.decrypt(&w, &mut rng).expect("decrypt").plaintext, body);
        }
        let body = format!("b{i}").into_bytes();
        let w = bob.encrypt(0, &body, &mut rng).expect("encrypt");
        if !driver.gen_bool(0.4) {
            assert_eq!(
                alice.decrypt(&w, &mut rng).expect("decrypt").plaintext,
                body
            );
        }
    }

    assert!(
        alice.pq_mixed_epoch() >= 2,
        "PQ ratchet stalled under 40% permanent loss (mixed {})",
        alice.pq_mixed_epoch()
    );
    assert_eq!(alice.pq_mixed_epoch(), bob.pq_mixed_epoch());
}

/// The specific failure mode the round-robin rotation exists to avoid:
/// a carrier that drops on a fixed period aliasing with the fragment
/// count so that the same indices are lost forever.
#[test]
fn pq_healing_survives_periodic_loss() {
    for period in [2usize, 3, 5, 10] {
        let (mut alice, mut bob, mut rng) = established_pair_with_params(103, fast_pq());
        let mut n = 0usize;
        for i in 0..1200u32 {
            n += 1;
            let body = format!("a{i}").into_bytes();
            let w = alice.encrypt(0, &body, &mut rng).expect("encrypt");
            if n % period != 0 {
                assert_eq!(bob.decrypt(&w, &mut rng).expect("decrypt").plaintext, body);
            }
            let body = format!("b{i}").into_bytes();
            let w = bob.encrypt(0, &body, &mut rng).expect("encrypt");
            if n % period != 0 {
                assert_eq!(
                    alice.decrypt(&w, &mut rng).expect("decrypt").plaintext,
                    body
                );
            }
        }
        assert!(
            alice.pq_mixed_epoch() >= 2,
            "period {period}: PQ ratchet starved (mixed {})",
            alice.pq_mixed_epoch()
        );
    }
}

#[test]
fn pq_healing_survives_reordering() {
    let (mut alice, mut bob, mut rng) = established_pair_with_params(104, fast_pq());
    let mut driver = seeded_rng(0x0DDE_u64);
    let mut to_bob: Vec<(String, Vec<u8>)> = Vec::new();
    let mut to_alice: Vec<(String, Vec<u8>)> = Vec::new();

    for i in 0..900u32 {
        let body = format!("a{i}").into_bytes();
        to_bob.push((alice.encrypt(0, &body, &mut rng).expect("encrypt"), body));
        let body = format!("b{i}").into_bytes();
        to_alice.push((bob.encrypt(0, &body, &mut rng).expect("encrypt"), body));

        if to_bob.len() > 5 {
            let idx = driver.gen_range(0..to_bob.len());
            let (w, body) = to_bob.remove(idx);
            assert_eq!(bob.decrypt(&w, &mut rng).expect("decrypt").plaintext, body);
        }
        if to_alice.len() > 5 {
            let idx = driver.gen_range(0..to_alice.len());
            let (w, body) = to_alice.remove(idx);
            assert_eq!(
                alice.decrypt(&w, &mut rng).expect("decrypt").plaintext,
                body
            );
        }
    }
    assert!(alice.pq_mixed_epoch() >= 2);
    assert_eq!(alice.pq_mixed_epoch(), bob.pq_mixed_epoch());
}

/// The wire-cost claim: PQ material costs nothing on the overwhelming
/// majority of messages, and the amortised cost is small.
#[test]
fn pq_wire_cost_is_amortised() {
    let params = SessionParams {
        pq: PqParams {
            fragment_bytes: 128,
            rekey_interval: 64,
        },
        skip: SkipParams::default(),
    };
    let (mut alice, mut bob, mut rng) = established_pair_with_params(105, params);

    // Measure *overhead*: wire bytes minus the plaintext they carry.
    const PLAINTEXT: usize = 4;
    let mut overheads = Vec::new();
    for i in 0..1000u32 {
        let w = alice
            .encrypt(0, format!("{i:04}").as_bytes(), &mut rng)
            .expect("encrypt");
        overheads.push(wire_len(&w) - PLAINTEXT);
        bob.decrypt(&w, &mut rng).expect("decrypt");
        let w = bob
            .encrypt(0, format!("{i:04}").as_bytes(), &mut rng)
            .expect("encrypt");
        alice.decrypt(&w, &mut rng).expect("decrypt");
    }

    let base = *overheads.iter().min().expect("sizes");
    let peak = *overheads.iter().max().expect("sizes");
    let total: usize = overheads.iter().sum();
    let mean = total / overheads.len();
    let carrying = overheads.iter().filter(|s| **s > base + 32).count();

    println!(
        "v=6 overhead bytes: base={base} peak={peak} mean={mean} \
         fragment-carrying={carrying}/1000 (base64 inflates by 4/3)"
    );

    // Documented budget. These are assertions, not decoration: if the
    // overhead regresses past them the design's adoptability argument
    // on a low-bandwidth carrier no longer holds. For scale, a
    // libsignal Double Ratchet message is roughly 50-60 bytes of
    // overhead; the extra here buys header encryption and a full
    // 16-byte tag instead of a truncated 8-byte MAC.
    assert!(base <= 90, "baseline overhead regressed: {base} bytes");
    assert!(
        peak <= base + 160,
        "fragment-carrying overhead regressed: {peak}"
    );
    assert!(
        mean <= base + 25,
        "amortised PQ cost regressed: mean {mean} vs base {base}"
    );
    assert!(
        carrying < 250,
        "too many messages carry fragments: {carrying}/1000"
    );
    assert!(alice.pq_mixed_epoch() >= 4, "not enough epochs to be fair");
}

/// A conversation that only ever goes one way cannot complete a PQ
/// epoch, because the ciphertext has no return path. This is a real
/// limitation and is asserted here so it stays visible rather than
/// being quietly assumed away.
#[test]
fn unidirectional_traffic_cannot_complete_a_pq_epoch() {
    let (mut alice, mut bob, mut rng) = established_pair_with_params(106, fast_pq());
    for i in 0..500u32 {
        let w = alice
            .encrypt(0, format!("{i}").as_bytes(), &mut rng)
            .expect("encrypt");
        bob.decrypt(&w, &mut rng).expect("decrypt");
    }
    assert_eq!(
        alice.pq_mixed_epoch(),
        0,
        "a one-way conversation cannot heal post-quantum; see DESIGN.md"
    );
    // ...but the moment the peer replies, healing resumes.
    for _ in 0..300u32 {
        let w = bob.encrypt(0, b"r", &mut rng).expect("encrypt");
        alice.decrypt(&w, &mut rng).expect("decrypt");
        let w = alice.encrypt(0, b"s", &mut rng).expect("encrypt");
        bob.decrypt(&w, &mut rng).expect("decrypt");
    }
    assert!(alice.pq_mixed_epoch() >= 1);
}
