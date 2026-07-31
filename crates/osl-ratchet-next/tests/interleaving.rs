//! Property tests over random delivery schedules.
//!
//! The carrier this protocol targets can reorder messages arbitrarily,
//! duplicate them, and drop them **permanently**. These tests build a
//! small network simulator that does all three, driven by a seeded
//! CSPRNG so any failure is replayable from its seed.
//!
//! The invariant under test is the same in every case:
//!
//! > Every message that is delivered exactly once, and whose key has
//! > not been evicted by an explicit storage policy, decrypts to the
//! > plaintext that was encrypted — regardless of what happened to any
//! > other message.
//!
//! Note what is *not* asserted: that a duplicate succeeds (it must
//! not), or that a message whose key the bounded store evicted still
//! works (it must fail, cleanly — see `bounds.rs`).

use osl_ratchet_next::test_support::{established_pair, seeded_rng, Harness};
use osl_ratchet_next::{Error, Session};
use rand::Rng;
use rand_chacha::ChaCha20Rng;

/// One message in flight.
struct InFlight {
    to_bob: bool,
    wire: String,
    plaintext: Vec<u8>,
}

/// Deliver `msg` to the right side and assert the plaintext matches.
fn deliver(
    alice: &mut Session,
    bob: &mut Session,
    rng: &mut ChaCha20Rng,
    msg: &InFlight,
) -> Result<(), Error> {
    let opened = if msg.to_bob {
        bob.decrypt(&msg.wire, rng)?
    } else {
        alice.decrypt(&msg.wire, rng)?
    };
    assert_eq!(
        opened.plaintext, msg.plaintext,
        "delivered message decrypted to the wrong plaintext"
    );
    Ok(())
}

/// Simulate a process restart by round-tripping both parties through
/// the persisted session blob.
fn restart_pair(alice: &mut Session, bob: &mut Session) {
    let alice_state = alice.export_state().expect("export alice");
    let bob_state = bob.export_state().expect("export bob");
    *alice = Session::import_state(&alice_state).expect("import alice");
    *bob = Session::import_state(&bob_state).expect("import bob");
}

#[test]
fn random_interleavings_deliver_correctly() {
    for seed in 0..24u64 {
        let (mut alice, mut bob, mut rng) = established_pair(seed);
        let mut driver = seeded_rng(seed ^ 0xA5A5);
        let mut inflight: Vec<InFlight> = Vec::new();
        let mut sent = 0u32;
        let mut delivered = 0u32;

        for _ in 0..300 {
            // Send from a random side.
            if inflight.len() < 40 && driver.gen_bool(0.6) {
                let to_bob = driver.gen_bool(0.5);
                let body = format!("seed {seed} msg {sent}").into_bytes();
                let wire = if to_bob {
                    alice.encrypt(0, &body, &mut rng)
                } else {
                    bob.encrypt(0, &body, &mut rng)
                }
                .expect("encrypt");
                inflight.push(InFlight {
                    to_bob,
                    wire,
                    plaintext: body,
                });
                sent += 1;
            }

            // Deliver a random in-flight message (out of order).
            if !inflight.is_empty() && driver.gen_bool(0.7) {
                let idx = driver.gen_range(0..inflight.len());
                let msg = inflight.remove(idx);
                deliver(&mut alice, &mut bob, &mut rng, &msg).expect("delivery must succeed");
                delivered += 1;
            }
        }

        // Drain.
        while !inflight.is_empty() {
            let idx = driver.gen_range(0..inflight.len());
            let msg = inflight.remove(idx);
            deliver(&mut alice, &mut bob, &mut rng, &msg).expect("drain delivery");
            delivered += 1;
        }

        assert_eq!(delivered, sent, "seed {seed}: every message must arrive");
        assert!(sent > 50, "seed {seed}: simulation too small to be useful");
    }
}

#[test]
fn duplicates_are_rejected_and_leave_the_session_usable() {
    for seed in 0..8u64 {
        let (mut alice, mut bob, mut rng) = established_pair(seed);
        let mut driver = seeded_rng(seed ^ 0xD00D);
        let mut inflight: Vec<InFlight> = Vec::new();
        // Already-delivered messages, held back for a *late* replay.
        let mut spent: Vec<InFlight> = Vec::new();

        for i in 0..120u32 {
            if driver.gen_bool(0.7) {
                let to_bob = driver.gen_bool(0.5);
                let body = format!("dup {i}").into_bytes();
                let wire = if to_bob {
                    alice.encrypt(0, &body, &mut rng)
                } else {
                    bob.encrypt(0, &body, &mut rng)
                }
                .expect("encrypt");
                inflight.push(InFlight {
                    to_bob,
                    wire,
                    plaintext: body,
                });
            }
            if !inflight.is_empty() && driver.gen_bool(0.6) {
                let idx = driver.gen_range(0..inflight.len());
                let msg = inflight.remove(idx);
                deliver(&mut alice, &mut bob, &mut rng, &msg).expect("first delivery");
                // Immediate replay must be rejected.
                assert!(
                    deliver(&mut alice, &mut bob, &mut rng, &msg).is_err(),
                    "immediate replay must be rejected"
                );
                spent.push(msg);
            }
            // Replay something delivered a long time ago.
            if !spent.is_empty() && driver.gen_bool(0.3) {
                let idx = driver.gen_range(0..spent.len());
                let old = &spent[idx];
                let r = if old.to_bob {
                    bob.decrypt(&old.wire, &mut rng)
                } else {
                    alice.decrypt(&old.wire, &mut rng)
                };
                assert!(r.is_err(), "delayed replay must be rejected");
            }
        }

        // Everything still in flight is fresh and must still open.
        for msg in inflight {
            deliver(&mut alice, &mut bob, &mut rng, &msg).expect("fresh message after replays");
        }
        assert!(spent.len() > 20, "seed {seed}: too few replays exercised");
    }
}

#[test]
fn permanent_loss_never_stalls_the_session() {
    for seed in 0..16u64 {
        let (mut alice, mut bob, mut rng) = established_pair(seed);
        let mut driver = seeded_rng(seed ^ 0x1055);
        let mut inflight: Vec<InFlight> = Vec::new();
        let mut delivered = 0u32;
        let mut dropped = 0u32;

        for i in 0..400u32 {
            let to_bob = driver.gen_bool(0.5);
            let body = format!("loss {i}").into_bytes();
            let wire = if to_bob {
                alice.encrypt(0, &body, &mut rng)
            } else {
                bob.encrypt(0, &body, &mut rng)
            }
            .expect("encrypt");

            // 35% of messages vanish forever.
            if driver.gen_bool(0.35) {
                dropped += 1;
                continue;
            }
            inflight.push(InFlight {
                to_bob,
                wire,
                plaintext: body,
            });

            if inflight.len() > 6 {
                let idx = driver.gen_range(0..inflight.len());
                let msg = inflight.remove(idx);
                deliver(&mut alice, &mut bob, &mut rng, &msg).expect("survivor must decrypt");
                delivered += 1;
            }
        }
        while let Some(msg) = inflight.pop() {
            deliver(&mut alice, &mut bob, &mut rng, &msg).expect("drain");
            delivered += 1;
        }

        assert!(dropped > 80, "seed {seed}: loss rate too low to prove much");
        assert!(delivered > 180, "seed {seed}: too few survivors");

        // And the session is still fully functional afterwards.
        let wire = alice.encrypt(0, b"still alive", &mut rng).expect("encrypt");
        assert_eq!(
            bob.decrypt(&wire, &mut rng).expect("decrypt").plaintext,
            b"still alive"
        );
    }
}

#[test]
fn an_entire_chain_lost_forever_does_not_wedge_the_ratchet() {
    let (mut alice, mut bob, mut rng) = established_pair(77);

    // Alice sends a whole chain into the void.
    for i in 0..50u32 {
        let _ = alice
            .encrypt(0, format!("void {i}").as_bytes(), &mut rng)
            .expect("encrypt");
    }
    // Bob, who has heard nothing, keeps sending on his existing chain.
    for i in 0..20u32 {
        let w = bob
            .encrypt(0, format!("bob {i}").as_bytes(), &mut rng)
            .expect("encrypt");
        assert_eq!(
            alice
                .decrypt(&w, &mut rng)
                .expect("alice decrypt")
                .plaintext,
            format!("bob {i}").as_bytes()
        );
    }
    // Alice ratchets and sends again; Bob must be able to catch up in
    // one step even though 50 of her messages are gone forever.
    let w = alice
        .encrypt(0, b"after the void", &mut rng)
        .expect("encrypt");
    assert_eq!(
        bob.decrypt(&w, &mut rng).expect("bob decrypt").plaintext,
        b"after the void"
    );
}

#[test]
fn previous_chain_inflight_messages_survive_later_dh_ratchet_step() {
    let (mut alice, mut bob, mut rng) = established_pair(78);

    let mut old_chain = Vec::new();
    for i in 0..4u32 {
        let body = format!("old chain {i}").into_bytes();
        old_chain.push((alice.encrypt(0, &body, &mut rng).expect("encrypt"), body));
    }

    let (first_old_wire, first_old_body) = old_chain.remove(0);
    assert_eq!(
        bob.decrypt(&first_old_wire, &mut rng)
            .expect("bob enters old Alice chain")
            .plaintext,
        first_old_body
    );

    let bob_reply = bob
        .encrypt(0, b"force Alice ratchet", &mut rng)
        .expect("encrypt");
    assert_eq!(
        alice
            .decrypt(&bob_reply, &mut rng)
            .expect("alice enters Bob's next chain")
            .plaintext,
        b"force Alice ratchet"
    );

    let newer = alice
        .encrypt(0, b"new Alice chain before old traffic", &mut rng)
        .expect("encrypt");
    assert_eq!(
        bob.decrypt(&newer, &mut rng)
            .expect("bob enters Alice's newer chain")
            .plaintext,
        b"new Alice chain before old traffic"
    );

    for (wire, body) in old_chain {
        assert_eq!(
            bob.decrypt(&wire, &mut rng)
                .expect("late old-chain message must use drained key")
                .plaintext,
            body
        );
    }
}

#[test]
fn long_unidirectional_run() {
    // A very long one-way burst: the symmetric chain must keep working
    // with no DH ratchet steps at all, and storage must not grow.
    let (mut alice, mut bob, mut rng) = established_pair(5);
    const N: u32 = 4000;
    for i in 0..N {
        let body = format!("uni {i}").into_bytes();
        let w = alice.encrypt(0, &body, &mut rng).expect("encrypt");
        assert_eq!(bob.decrypt(&w, &mut rng).expect("decrypt").plaintext, body);
    }
    assert_eq!(alice.sending_counter(), N);
    assert_eq!(bob.receiving_counter(), N);
    assert_eq!(
        bob.skipped_key_count(),
        0,
        "in-order delivery must cache nothing"
    );

    // The reverse direction still works afterwards.
    let w = bob.encrypt(0, b"reply at last", &mut rng).expect("encrypt");
    assert_eq!(
        alice.decrypt(&w, &mut rng).expect("decrypt").plaintext,
        b"reply at last"
    );
}

#[test]
fn reverse_order_delivery_within_a_chain() {
    let (mut alice, mut bob, mut rng) = established_pair(6);
    let mut wires = Vec::new();
    for i in 0..200u32 {
        let body = format!("rev {i}").into_bytes();
        wires.push((alice.encrypt(0, &body, &mut rng).expect("encrypt"), body));
    }
    for (wire, body) in wires.iter().rev() {
        assert_eq!(
            bob.decrypt(wire, &mut rng).expect("decrypt").plaintext,
            *body
        );
    }
    assert_eq!(bob.skipped_key_count(), 0, "all gaps eventually filled");
}

#[test]
fn bootstrap_phase_tolerates_reordering_and_loss() {
    // Before Bob has replied, Alice repeats the bootstrap preamble on
    // every message. Any one of them must be able to establish Bob's
    // session, in any order, after arbitrary loss.
    for seed in 0..8u64 {
        let mut h = Harness::new(seed);
        let mut driver = seeded_rng(seed ^ 0xB007);
        let mut wires = Vec::new();
        for i in 0..12u32 {
            let body = format!("boot {i}").into_bytes();
            wires.push((h.alice_send(&body).expect("send"), body));
        }
        assert!(h.alice.has_bootstrap_pending());

        // Drop a random half, shuffle the rest.
        let mut kept: Vec<_> = wires.into_iter().filter(|_| driver.gen_bool(0.5)).collect();
        for i in (1..kept.len()).rev() {
            let j = driver.gen_range(0..=i);
            kept.swap(i, j);
        }
        if kept.is_empty() {
            continue;
        }
        for (wire, body) in &kept {
            assert_eq!(
                h.bob_recv(wire).expect("bob recv").plaintext,
                *body,
                "seed {seed}"
            );
        }

        // Bob replies; Alice stops sending the preamble.
        let w = h.bob_send(b"ack").expect("bob send");
        assert_eq!(h.alice_recv(&w).expect("alice recv").plaintext, b"ack");
        assert!(!h.alice.has_bootstrap_pending());
    }
}

#[test]
fn state_export_import_survives_an_interleaved_run() {
    let (mut alice, mut bob, mut rng) = established_pair(31);
    let mut driver = seeded_rng(0xE7A7);
    let mut inflight: Vec<InFlight> = Vec::new();
    let mut spent: Vec<InFlight> = Vec::new();
    let mut delivered = 0u32;
    let mut restarts = 0u32;
    let mut immediate_replays = 0u32;
    let mut restart_replays = 0u32;
    let mut delayed_replays = 0u32;
    let mut max_inflight = 0usize;
    let mut delivered_to_bob = 0u32;
    let mut delivered_to_alice = 0u32;

    for i in 0..180u32 {
        // Bursts create same-chain gaps; mixed directions create DH
        // ratchet steps while old chain messages are still in flight.
        let send_count = if i % 13 == 0 { 4 } else { 1 };
        for j in 0..send_count {
            let to_bob = driver.gen_bool(0.5);
            let body = format!("persist {i}.{j}").into_bytes();
            let wire = if to_bob {
                alice.encrypt(0, &body, &mut rng)
            } else {
                bob.encrypt(0, &body, &mut rng)
            }
            .expect("encrypt");
            inflight.push(InFlight {
                to_bob,
                wire,
                plaintext: body,
            });
        }
        max_inflight = max_inflight.max(inflight.len());

        if i % 3 == 0 {
            restart_pair(&mut alice, &mut bob);
            restarts += 1;
        }

        let deliveries = if driver.gen_bool(0.25) { 2 } else { 1 };
        for _ in 0..deliveries {
            if inflight.len() <= 24 {
                break;
            }
            // Bias toward the oldest in-flight message. A purely random pick
            // has unbounded tail latency, so a message could linger past
            // SkipParams::max_chains (5) DH chains -- at which point the
            // receiver has legitimately evicted its skipped-key bucket and
            // AuthFailed is the CORRECT answer, not a persistence bug. This
            // test is about export/import not corrupting the session, so it
            // keeps delivery strict and instead stops the queue from
            // demanding retention the crate deliberately refuses to promise.
            let idx = if driver.gen_bool(0.5) {
                0
            } else {
                driver.gen_range(0..inflight.len())
            };
            let msg = inflight.remove(idx);
            deliver(&mut alice, &mut bob, &mut rng, &msg).expect("deliver after restore");
            delivered += 1;
            if msg.to_bob {
                delivered_to_bob += 1;
            } else {
                delivered_to_alice += 1;
            }

            assert!(
                deliver(&mut alice, &mut bob, &mut rng, &msg).is_err(),
                "immediate replay must be rejected before restart"
            );
            immediate_replays += 1;

            restart_pair(&mut alice, &mut bob);
            restarts += 1;
            assert!(
                deliver(&mut alice, &mut bob, &mut rng, &msg).is_err(),
                "replay must stay rejected after simulated process restart"
            );
            restart_replays += 1;
            spent.push(msg);
        }

        if i % 5 == 0 && !spent.is_empty() {
            restart_pair(&mut alice, &mut bob);
            restarts += 1;
            let idx = driver.gen_range(0..spent.len());
            let old = &spent[idx];
            assert!(
                deliver(&mut alice, &mut bob, &mut rng, old).is_err(),
                "delayed replay must stay rejected after more reordering and restarts"
            );
            delayed_replays += 1;
        }
    }

    while !inflight.is_empty() {
        restart_pair(&mut alice, &mut bob);
        restarts += 1;
        let idx = driver.gen_range(0..inflight.len());
        let msg = inflight.remove(idx);
        deliver(&mut alice, &mut bob, &mut rng, &msg).expect("drain after restore");
        delivered += 1;
        if msg.to_bob {
            delivered_to_bob += 1;
        } else {
            delivered_to_alice += 1;
        }
        assert!(
            deliver(&mut alice, &mut bob, &mut rng, &msg).is_err(),
            "drained message replay must be rejected"
        );
        immediate_replays += 1;
        spent.push(msg);
    }

    restart_pair(&mut alice, &mut bob);
    restarts += 1;

    let w = alice
        .encrypt(0, b"fresh after replay storm", &mut rng)
        .expect("alice encrypt after replay storm");
    assert_eq!(
        bob.decrypt(&w, &mut rng)
            .expect("bob decrypt after replay storm")
            .plaintext,
        b"fresh after replay storm"
    );
    let w = bob
        .encrypt(0, b"fresh reply after replay storm", &mut rng)
        .expect("bob encrypt after replay storm");
    assert_eq!(
        alice
            .decrypt(&w, &mut rng)
            .expect("alice decrypt after replay storm")
            .plaintext,
        b"fresh reply after replay storm"
    );

    assert!(delivered > 180, "too few interleaved deliveries");
    assert!(
        delivered_to_bob > 80,
        "too few post-restore deliveries into Bob's receive chains"
    );
    assert!(
        delivered_to_alice > 80,
        "too few post-restore deliveries into Alice's receive chains"
    );
    assert!(
        max_inflight > 20,
        "run did not keep enough messages in flight"
    );
    assert!(restarts > 100, "too few simulated process restarts");
    assert!(immediate_replays > 150, "too few immediate replay refusals");
    assert!(
        restart_replays > 100,
        "too few post-restart replay refusals"
    );
    assert!(delayed_replays > 25, "too few delayed replay refusals");
}
