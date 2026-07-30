//! Storage and work bounds.
//!
//! Unbounded skipped-key storage is a known memory-exhaustion vector in
//! Double Ratchet implementations. These tests assert the three bounds
//! documented in `DESIGN.md` hold under adversarial-shaped traffic, and
//! — just as important — that hitting a bound produces a clean refusal
//! rather than a corrupted session.

use osl_ratchet_next::test_support::{established_pair_with_params, seeded_rng};
use osl_ratchet_next::{Error, PqParams, SessionParams, SkipParams};
use rand::Rng;

fn params(skip: SkipParams) -> SessionParams {
    SessionParams {
        pq: PqParams::default(),
        skip,
    }
}

#[test]
fn a_single_absurd_gap_is_refused_without_deriving_anything() {
    let skip = SkipParams {
        max_skip_per_message: 64,
        max_keys_per_chain: 64,
        max_total_keys: 256,
        max_chains: 4,
        max_age: 100_000,
    };
    let (mut alice, mut bob, mut rng) = established_pair_with_params(200, params(skip));

    // Alice runs far ahead; Bob only ever sees the last message.
    let mut last = String::new();
    for i in 0..500u32 {
        last = alice
            .encrypt(0, format!("far {i}").as_bytes(), &mut rng)
            .expect("encrypt");
    }
    let before_nr = bob.receiving_counter();
    let before_keys = bob.skipped_key_count();
    let err = bob.decrypt(&last, &mut rng).expect_err("must refuse");
    assert!(
        matches!(err, Error::SkipLimitExceeded { limit: 64, .. }),
        "expected a skip-limit refusal, got {err:?}"
    );

    // The refusal must be a pure no-op: not one chain-key derivation,
    // not one cached key, no counter movement.
    assert_eq!(bob.receiving_counter(), before_nr, "counter moved");
    assert_eq!(bob.skipped_key_count(), before_keys, "keys were cached");

    // And the session is still alive: Bob can ratchet forward normally.
    let w = bob.encrypt(0, b"bob speaks", &mut rng).expect("encrypt");
    assert_eq!(
        alice.decrypt(&w, &mut rng).expect("decrypt").plaintext,
        b"bob speaks"
    );
}

#[test]
fn skipped_storage_stays_bounded_over_a_long_lossy_run() {
    let skip = SkipParams {
        max_skip_per_message: 128,
        max_keys_per_chain: 128,
        max_total_keys: 400,
        max_chains: 4,
        max_age: 1_000_000,
    };
    let (mut alice, mut bob, mut rng) = established_pair_with_params(201, params(skip));
    let mut driver = seeded_rng(0xB0117);

    let mut peak_keys = 0usize;
    let mut peak_chains = 0usize;

    for i in 0..600u32 {
        // Alice sends bursts with big gaps; Bob occasionally replies so
        // the ratchet keeps stepping and new chains keep appearing.
        let mut deliverable = None;
        for j in 0..driver.gen_range(1..60u32) {
            let w = alice
                .encrypt(0, format!("burst {i}.{j}").as_bytes(), &mut rng)
                .expect("encrypt");
            // Keep only the last of each burst: everything before it is
            // a permanent gap Bob must cache keys for.
            deliverable = Some(w);
        }
        if let Some(w) = deliverable {
            // Some bursts exceed the per-message cap; those must refuse
            // cleanly and leave Bob usable.
            match bob.decrypt(&w, &mut rng) {
                Ok(_) => {}
                Err(Error::SkipLimitExceeded { .. }) => {}
                Err(e) => panic!("unexpected error {e:?}"),
            }
        }
        if driver.gen_bool(0.3) {
            let w = bob.encrypt(0, b"reply", &mut rng).expect("encrypt");
            let _ = alice.decrypt(&w, &mut rng);
        }

        peak_keys = peak_keys.max(bob.skipped_key_count());
        peak_chains = peak_chains.max(bob.skipped_chain_count());
        assert!(
            bob.skipped_key_count() <= skip.max_total_keys,
            "global key cap breached at step {i}: {}",
            bob.skipped_key_count()
        );
        assert!(
            bob.skipped_chain_count() <= skip.max_chains,
            "chain cap breached at step {i}: {}",
            bob.skipped_chain_count()
        );
    }

    println!("peak skipped keys = {peak_keys}, peak chains = {peak_chains}");
    assert!(peak_keys > 100, "the test never actually filled the store");
}

#[test]
fn evicted_keys_fail_closed_and_do_not_corrupt_the_session() {
    let skip = SkipParams {
        max_skip_per_message: 4096,
        max_keys_per_chain: 16,
        max_total_keys: 16,
        max_chains: 2,
        max_age: 1_000_000,
    };
    let (mut alice, mut bob, mut rng) = established_pair_with_params(202, params(skip));

    // 40 messages, delivered last-first. Only the most recent 16 gaps
    // can be cached; the rest must be evicted.
    let mut wires = Vec::new();
    for i in 0..40u32 {
        wires.push((
            alice
                .encrypt(0, format!("evict {i}").as_bytes(), &mut rng)
                .expect("encrypt"),
            format!("evict {i}").into_bytes(),
        ));
    }
    let (last, last_body) = wires.pop().expect("last");
    assert_eq!(
        bob.decrypt(&last, &mut rng).expect("decrypt").plaintext,
        last_body
    );
    assert!(bob.skipped_key_count() <= 16);

    let mut opened = 0;
    let mut refused = 0;
    for (wire, body) in wires.iter().rev() {
        match bob.decrypt(wire, &mut rng) {
            Ok(o) => {
                assert_eq!(&o.plaintext, body, "an opened message must be correct");
                opened += 1;
            }
            Err(Error::AuthFailed) => refused += 1,
            Err(e) => panic!("unexpected error {e:?}"),
        }
    }
    assert_eq!(opened, 16, "exactly the cached window should open");
    assert_eq!(refused, 23, "the evicted remainder must fail closed");

    // And the session still works perfectly afterwards.
    let w = alice.encrypt(0, b"after eviction", &mut rng).expect("encrypt");
    assert_eq!(
        bob.decrypt(&w, &mut rng).expect("decrypt").plaintext,
        b"after eviction"
    );
}

#[test]
fn trial_decryption_candidate_set_stays_small() {
    // Header encryption means every inbound message costs a bounded
    // number of AEAD opens. That bound is 2 (current + next chain) plus
    // the retained-chain cap.
    let skip = SkipParams {
        max_chains: 3,
        ..SkipParams::default()
    };
    let (mut alice, mut bob, mut rng) = established_pair_with_params(203, params(skip));

    for round in 0..60u32 {
        // Leave a gap in every chain so every chain enters the store.
        let _ = alice
            .encrypt(0, format!("gap {round}").as_bytes(), &mut rng)
            .expect("encrypt");
        let w = alice
            .encrypt(0, format!("seen {round}").as_bytes(), &mut rng)
            .expect("encrypt");
        bob.decrypt(&w, &mut rng).expect("decrypt");
        let w = bob.encrypt(0, b"step", &mut rng).expect("encrypt");
        alice.decrypt(&w, &mut rng).expect("decrypt");

        assert!(
            bob.skipped_chain_count() <= 3,
            "round {round}: {} chains retained",
            bob.skipped_chain_count()
        );
    }
}

#[test]
fn oversized_blobs_are_refused_before_any_crypto() {
    let (alice, mut bob, mut rng) = established_pair_with_params(204, SessionParams::default());
    let _ = alice;
    let huge = format!("DPC0::{}", "A".repeat(1_000_000));
    assert!(matches!(
        bob.decrypt(&huge, &mut rng),
        Err(Error::PolicyBound(_))
    ));
}

#[test]
fn a_message_larger_than_the_wire_budget_is_refused_on_send() {
    let (mut alice, _bob, mut rng) = established_pair_with_params(205, SessionParams::default());
    let big = vec![0u8; 100_000];
    assert!(matches!(
        alice.encrypt(0, &big, &mut rng),
        Err(Error::PolicyBound(_))
    ));
    // The session survives the refusal.
    assert!(alice.encrypt(0, b"fine", &mut rng).is_ok());
}
