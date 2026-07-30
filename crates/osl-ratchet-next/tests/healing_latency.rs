//! Post-compromise-security healing latency, measured rather than
//! asserted by hand-waving.
//!
//! `DESIGN.md` publishes a healing bound for the PQ epoch ratchet. That
//! bound is only meaningful if it is measured, so this test measures it
//! and fails if it regresses. The numbers it prints are the ones quoted
//! in the design document.
//!
//! Classical PCS healing is one round trip, exactly as in Signal, and is
//! covered structurally by the ratchet tests; it is not re-measured here.

use osl_ratchet_next::test_support::established_pair_with_params;
use osl_ratchet_next::{PqParams, SessionParams, SkipParams};

/// Returns the round-trip index at which each PQ epoch was mixed.
fn healing_marks(rekey_interval: u32, loss: f64, round_trips: u32) -> Vec<u32> {
    let params = SessionParams {
        pq: PqParams {
            fragment_bytes: 128,
            rekey_interval,
        },
        skip: SkipParams::default(),
    };
    let (mut alice, mut bob, mut rng) = established_pair_with_params(9, params);
    let mut marks = Vec::new();
    let mut last = 0u32;
    // Deterministic loss: drop every `1/loss`-th round trip in both
    // directions. This is harsher than random loss of the same rate
    // because it is periodic — see the aliasing note in `pq.rs`.
    let mut accumulator = 0.0f64;

    for i in 0..round_trips {
        let w = alice.encrypt(0, b"x", &mut rng).expect("encrypt");
        accumulator += loss;
        let drop_this = accumulator >= 1.0;
        if drop_this {
            accumulator -= 1.0;
        } else {
            bob.decrypt(&w, &mut rng).expect("decrypt");
        }
        let w = bob.encrypt(0, b"y", &mut rng).expect("encrypt");
        if !drop_this {
            alice.decrypt(&w, &mut rng).expect("decrypt");
        }
        // A one-step skew is inherent and expected: the party that
        // creates a sending chain commits the mix immediately, while
        // the peer only replays it when a message from that chain
        // arrives. Anything larger would be a real divergence.
        let skew = alice.pq_mixed_epoch().abs_diff(bob.pq_mixed_epoch());
        assert!(
            skew <= 1,
            "PQ mix state diverged by {skew} epochs at round trip {i}"
        );
        // Count an epoch as healed only once *both* sides have folded
        // it in — that is the point the security property holds.
        let both = alice.pq_mixed_epoch().min(bob.pq_mixed_epoch());
        if both > last {
            last = both;
            marks.push(i);
        }
    }
    marks
}

fn gaps(marks: &[u32]) -> Vec<u32> {
    marks.windows(2).map(|w| w[1].saturating_sub(w[0])).collect()
}

#[test]
fn pq_healing_latency_at_default_settings() {
    let marks = healing_marks(64, 0.0, 1200);
    let g = gaps(&marks);
    println!(
        "default (rekey_interval=64, no loss): first mix at round trip {:?}, \
         steady-state period {:?}",
        marks.first(),
        g.first()
    );
    assert!(marks.len() >= 10, "too few epochs to characterise");
    // rekey_interval (64) + ~10 EK fragments + ~9 CT fragments + the
    // ack round trip. Published in DESIGN.md as "~81 round trips".
    for gap in &g {
        assert!(
            (70..=95).contains(gap),
            "PQ healing period regressed: {gap} round trips (expected ~81)"
        );
    }
}

#[test]
fn pq_healing_latency_degrades_gracefully_under_loss() {
    let marks = healing_marks(64, 0.4, 2000);
    let g = gaps(&marks);
    println!(
        "40% periodic loss: first mix at round trip {:?}, periods {:?}",
        marks.first(),
        g.first()
    );
    assert!(
        marks.len() >= 5,
        "PQ healing effectively stalled under 40% loss"
    );
    // Degradation must be proportional, not catastrophic: losing 40% of
    // messages must not cost more than roughly 2x the healing period.
    for gap in &g {
        assert!(
            *gap <= 170,
            "PQ healing degraded worse than 2x under 40% loss: {gap}"
        );
    }
}

#[test]
fn pq_healing_latency_is_tunable() {
    // A deployment that can afford more overhead can heal ~2.5x faster.
    let marks = healing_marks(16, 0.0, 600);
    let g = gaps(&marks);
    println!(
        "aggressive (rekey_interval=16, no loss): steady-state period {:?}",
        g.first()
    );
    assert!(marks.len() >= 10);
    for gap in &g {
        assert!(
            (25..=45).contains(gap),
            "aggressive healing period regressed: {gap} (expected ~33)"
        );
    }
}
