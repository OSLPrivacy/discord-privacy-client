use osl_privacy_hub::chat_capture_protection::{
    CaptureConsent, ChatCaptureProtectionState, EffectiveCaptureProtection as Effective,
};

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

#[test]
fn t14_t17_shipping_state_wires_local_and_peer_transitions() {
    let state = ChatCaptureProtectionState::default();

    let local_on = state.apply_local_preference("person-a", true, true);
    assert_eq!(local_on.effective_changed, None);
    assert_eq!(local_on.state.effective, Effective::Off);

    let on = state.apply_peer_preference("person-a", true, true);
    assert_eq!(on.effective_changed, Some(Effective::On));
    assert_eq!(
        state.ensure_conversation("person-a").effective,
        Effective::On
    );

    let local_off = state.apply_local_preference("person-a", false, true);
    assert_eq!(local_off.effective_changed, None);
    assert_eq!(local_off.state.effective, Effective::On);

    let off = state.apply_peer_preference("person-a", false, true);
    assert_eq!(off.effective_changed, Some(Effective::Off));
    assert_eq!(
        state.ensure_conversation("person-a").effective,
        Effective::Off
    );
}

#[test]
fn t14_t17_shipping_state_refuses_to_announce_unenforced_effective_change() {
    let state = ChatCaptureProtectionState::default();
    state.apply_local_preference("person-a", true, true);

    let failed_on = state.apply_peer_preference("person-a", true, false);
    assert_eq!(failed_on.effective_changed, None);
    assert_eq!(failed_on.state.effective, Effective::Off);
    assert!(failed_on.state.local_opt_in);
    assert!(!failed_on.state.peer_opt_in);
}
