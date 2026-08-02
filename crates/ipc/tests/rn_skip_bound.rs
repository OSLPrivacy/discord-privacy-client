use ipc::rn_health::{RnDesyncSymptom, RnPeerHealth, RnRecoveryOffer, RnSessionHealth};
use osl_ratchet_next::test_support::established_pair_with_params;
use osl_ratchet_next::{Error, SessionParams, SkipParams};

#[test]
fn skipped_key_ceiling_is_degraded_with_a_rehandshake_offer() {
    let params = SessionParams {
        skip: SkipParams {
            max_skip_per_message: 64,
            ..SkipParams::default()
        },
        ..SessionParams::default()
    };
    let (mut sender, mut receiver, mut rng) = established_pair_with_params(600, params);

    let mut last_wire = String::new();
    for sequence in 0..600 {
        last_wire = sender
            .encrypt(0, format!("one-way {sequence}").as_bytes(), &mut rng)
            .expect("sender encrypts each one-way message");
    }

    let refusal = receiver
        .decrypt(&last_wire, &mut rng)
        .expect_err("the final message exceeds the skipped-key ceiling");
    assert!(
        matches!(refusal, Error::SkipLimitExceeded { .. }),
        "the bounded receiver must refuse the oversized gap, got {refusal:?}"
    );

    let mut health = RnPeerHealth::default();
    health.observe_pinned_symptom(RnDesyncSymptom::MaxSkipPerMessageRefused);

    assert_eq!(health.health(), RnSessionHealth::Degraded);
    assert_eq!(
        health.recovery_offer(),
        Some(RnRecoveryOffer::ReestablishSession)
    );
}
