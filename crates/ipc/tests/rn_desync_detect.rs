use ipc::rn_health::{RnDesyncSymptom, RnPeerHealth, RnSessionHealth};

#[test]
fn detector_enters_desynced_only_for_real_pinned_peer_symptoms() {
    let mut tampered_message = RnPeerHealth::default();
    tampered_message.observe_pinned_symptom(RnDesyncSymptom::AuthFailed);
    assert_eq!(tampered_message.health(), RnSessionHealth::Degraded);
    assert_eq!(tampered_message.consecutive_auth_failures(), 1);

    let mut repeated_auth_failures = RnPeerHealth::default();
    for _ in 0..3 {
        repeated_auth_failures.observe_pinned_symptom(RnDesyncSymptom::AuthFailed);
    }
    assert_eq!(repeated_auth_failures.health(), RnSessionHealth::Desynced);

    let mut missing_pinned_session = RnPeerHealth::default();
    missing_pinned_session.observe_pinned_symptom(RnDesyncSymptom::MissingSession);
    assert_eq!(missing_pinned_session.health(), RnSessionHealth::Desynced);

    let mut skip_bound_refusal = RnPeerHealth::default();
    skip_bound_refusal.observe_pinned_symptom(RnDesyncSymptom::MaxSkipPerMessageRefused);
    assert_eq!(skip_bound_refusal.health(), RnSessionHealth::Desynced);
}
