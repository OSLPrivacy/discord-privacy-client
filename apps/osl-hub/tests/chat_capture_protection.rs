use osl_hub::chat_capture_protection::{CaptureConsent, EffectiveCaptureProtection as Effective};

#[test]
fn t14_t17_bilateral_capture_consent_state_machine() {
    let off = CaptureConsent::new();
    assert_eq!(off.effective, Effective::Off);

    // A local request alone is not a command, and a stale peer update cannot
    // undo the current effective state.
    let local_on = off.with_local_preference(true).state;
    assert_eq!(local_on.effective, Effective::Off);
    let stale_off = local_on.with_peer_preference(false).state;
    assert_eq!(stale_off.effective, Effective::Off);

    let on = stale_off.with_peer_preference(true);
    assert_eq!(on.effective_changed, Some(Effective::On));
    assert_eq!(on.state.effective, Effective::On);

    // One opt-out cannot turn protection off. Both parties must converge.
    let local_off = on.state.with_local_preference(false).state;
    assert_eq!(local_off.effective, Effective::On);
    let off_again = local_off.with_peer_preference(false);
    assert_eq!(off_again.effective_changed, Some(Effective::Off));

    // A matching transition is not reported until the local platform actually
    // enforces it; callers therefore never announce a false effective state.
    let failed = on.state.commit_if_enforced(off_again, false);
    assert_eq!(failed.state, on.state);
    assert_eq!(failed.effective_changed, None);
}
