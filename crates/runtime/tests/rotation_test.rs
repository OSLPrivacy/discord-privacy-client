use crypto::sender_keys::{SenderChain, SenderContext, SESSION_VERSION_V1};
use crypto::x25519;
use runtime::{
    Clock, MockClock, RotationConfig, RotationController, RotationReason, SuspiciousEventKind,
};
use std::sync::Arc;
use std::time::Duration;

fn build_controller_with_clock(clock: Arc<MockClock>) -> RotationController {
    struct ClockProxy(Arc<MockClock>);
    impl Clock for ClockProxy {
        fn now(&self) -> std::time::Instant {
            self.0.now()
        }
    }
    RotationController::new(Box::new(ClockProxy(clock)), RotationConfig::default())
}

fn build_controller_with_clock_and_config(
    clock: Arc<MockClock>,
    config: RotationConfig,
) -> RotationController {
    struct ClockProxy(Arc<MockClock>);
    impl Clock for ClockProxy {
        fn now(&self) -> std::time::Instant {
            self.0.now()
        }
    }
    RotationController::new(Box::new(ClockProxy(clock)), config)
}

// ---- time trigger ----

#[test]
fn time_trigger_fires_at_configured_interval() {
    let clock = Arc::new(MockClock::new());
    let mut ctrl = build_controller_with_clock(clock.clone());
    assert_eq!(ctrl.check_for_rotation(), None);

    // 59 minutes — not yet.
    clock.advance(Duration::from_secs(59 * 60));
    assert_eq!(ctrl.check_for_rotation(), None);

    // Cross the 1-hour boundary.
    clock.advance(Duration::from_secs(60));
    assert_eq!(ctrl.check_for_rotation(), Some(RotationReason::Time));
}

#[test]
fn time_trigger_resets_after_completion() {
    let clock = Arc::new(MockClock::new());
    let mut ctrl = build_controller_with_clock(clock.clone());

    clock.advance(Duration::from_secs(60 * 60));
    let r = ctrl.check_for_rotation().unwrap();
    assert_eq!(r, RotationReason::Time);
    ctrl.note_rotation_completed();
    assert_eq!(ctrl.check_for_rotation(), None);

    // Need another full hour before re-firing.
    clock.advance(Duration::from_secs(30 * 60));
    assert_eq!(ctrl.check_for_rotation(), None);
    clock.advance(Duration::from_secs(30 * 60));
    assert_eq!(ctrl.check_for_rotation(), Some(RotationReason::Time));
}

// ---- message-count trigger ----

#[test]
fn message_count_trigger_fires_at_500_default() {
    let clock = Arc::new(MockClock::new());
    let mut ctrl = build_controller_with_clock(clock.clone());
    for _ in 0..499 {
        ctrl.note_message_sent();
    }
    assert_eq!(ctrl.check_for_rotation(), None);
    ctrl.note_message_sent();
    assert_eq!(
        ctrl.check_for_rotation(),
        Some(RotationReason::MessageCount)
    );
}

#[test]
fn message_count_resets_after_completion() {
    let clock = Arc::new(MockClock::new());
    let mut ctrl = build_controller_with_clock(clock.clone());
    for _ in 0..500 {
        ctrl.note_message_sent();
    }
    assert_eq!(
        ctrl.check_for_rotation(),
        Some(RotationReason::MessageCount)
    );
    ctrl.note_rotation_completed();
    assert_eq!(ctrl.messages_since_rotation(), 0);
    ctrl.note_message_sent();
    assert_eq!(ctrl.check_for_rotation(), None);
}

// ---- idle trigger (suspicious-flavour) ----

#[test]
fn idle_trigger_fires_after_six_hours_of_silence() {
    // Default config is time_trigger=1h, idle_trigger=6h. Time would
    // pre-empt idle in `check_for_rotation` (it's checked first by
    // design — time is the harder lower bound). To isolate idle's
    // observable behaviour, push the time trigger out of the way.
    let clock = Arc::new(MockClock::new());
    let cfg = RotationConfig {
        time_trigger: Duration::from_secs(100 * 60 * 60),
        ..RotationConfig::default()
    };
    let mut ctrl = build_controller_with_clock_and_config(clock.clone(), cfg);
    ctrl.note_message_sent();

    // 5h59m — still not idle.
    clock.advance(Duration::from_secs(5 * 60 * 60 + 59 * 60));
    assert_eq!(ctrl.check_for_rotation(), None);

    // Cross the 6-hour mark.
    clock.advance(Duration::from_secs(60));
    let r = ctrl.check_for_rotation().unwrap();
    assert_eq!(r, RotationReason::Suspicious(SuspiciousEventKind::Idle));
}

#[test]
fn idle_trigger_does_not_fire_without_a_message_first() {
    let clock = Arc::new(MockClock::new());
    let mut ctrl = build_controller_with_clock(clock.clone());
    // Advance past the 6h idle threshold without any message.
    clock.advance(Duration::from_secs(7 * 60 * 60));
    // Time trigger fires first (1h elapsed) — let's complete that
    // rotation, then verify idle still doesn't synthesise without a
    // subsequent send.
    let r = ctrl.check_for_rotation();
    assert_eq!(r, Some(RotationReason::Time));
    ctrl.note_rotation_completed();
    clock.advance(Duration::from_secs(7 * 60 * 60));
    // First check fires Time (we crossed 1h again).
    assert_eq!(ctrl.check_for_rotation(), Some(RotationReason::Time));
    ctrl.note_rotation_completed();
    // No messages have ever been sent on this chain — no Idle synthesis.
    // (We're still under 1h since this last rotation.)
    clock.advance(Duration::from_secs(30 * 60));
    assert_eq!(ctrl.check_for_rotation(), None);
}

#[test]
fn idle_trigger_emits_only_once_per_chain() {
    // Same isolation trick as above — push time trigger far out so
    // idle's "fires once per chain" property is observable.
    let clock = Arc::new(MockClock::new());
    let cfg = RotationConfig {
        time_trigger: Duration::from_secs(100 * 60 * 60),
        ..RotationConfig::default()
    };
    let mut ctrl = build_controller_with_clock_and_config(clock.clone(), cfg);
    ctrl.note_message_sent();
    clock.advance(Duration::from_secs(6 * 60 * 60));
    let r = ctrl.check_for_rotation().unwrap();
    assert!(matches!(
        r,
        RotationReason::Suspicious(SuspiciousEventKind::Idle)
    ));
    // Caller hasn't called note_rotation_completed yet; advance past
    // the suspicious cap to rule out cap-suppression and confirm the
    // idle_event_emitted gate prevents re-emission.
    clock.advance(Duration::from_secs(6 * 60 + 1));
    assert_eq!(ctrl.check_for_rotation(), None);
}

// ---- suspicious cap ----

#[test]
fn suspicious_events_capped_at_one_per_five_minutes() {
    let clock = Arc::new(MockClock::new());
    let mut ctrl = build_controller_with_clock(clock.clone());

    ctrl.note_suspicious_event(SuspiciousEventKind::ScreenRecorder);
    let r1 = ctrl.check_for_rotation().unwrap();
    assert_eq!(
        r1,
        RotationReason::Suspicious(SuspiciousEventKind::ScreenRecorder)
    );
    ctrl.note_rotation_completed();

    // 4 minutes in — second event must be queued, not fired.
    clock.advance(Duration::from_secs(4 * 60));
    ctrl.note_suspicious_event(SuspiciousEventKind::UsbCaptureDevice);
    assert_eq!(ctrl.check_for_rotation(), None);

    // Cross the 5-minute boundary — queued event fires now.
    clock.advance(Duration::from_secs(60));
    let r2 = ctrl.check_for_rotation().unwrap();
    assert_eq!(
        r2,
        RotationReason::Suspicious(SuspiciousEventKind::UsbCaptureDevice)
    );
}

#[test]
fn many_suspicious_events_collapse_to_one_rotation() {
    let clock = Arc::new(MockClock::new());
    let mut ctrl = build_controller_with_clock(clock.clone());

    // Burst within the cap window.
    ctrl.note_suspicious_event(SuspiciousEventKind::ScreenRecorder);
    ctrl.note_suspicious_event(SuspiciousEventKind::UsbCaptureDevice);
    ctrl.note_suspicious_event(SuspiciousEventKind::Suspend);
    ctrl.note_suspicious_event(SuspiciousEventKind::Other);

    let _r = ctrl.check_for_rotation().unwrap();
    ctrl.note_rotation_completed();
    assert_eq!(ctrl.queued_suspicious_count(), 0);

    // No subsequent rotation should fire from the same burst.
    clock.advance(Duration::from_secs(30 * 60));
    assert_eq!(ctrl.check_for_rotation(), None);
}

// ---- cap-exempt triggers ----

#[test]
fn time_trigger_fires_even_if_suspicious_cap_active() {
    let clock = Arc::new(MockClock::new());
    let mut ctrl = build_controller_with_clock(clock.clone());

    ctrl.note_suspicious_event(SuspiciousEventKind::ScreenRecorder);
    let _ = ctrl.check_for_rotation().unwrap();
    ctrl.note_rotation_completed();

    // The suspicious cap is now active for 5 minutes. Advance past
    // 1h to trigger time — it must fire even though we're well under
    // the suspicious-cap window if cap were applied.
    //
    // (Cap clock starts at the moment of the suspicious rotation.
    //  After 1h, that cap has long expired — but we want to confirm
    //  Time is path-independent of suspicious-cap state regardless.)
    clock.advance(Duration::from_secs(60 * 60));
    let r = ctrl.check_for_rotation();
    assert_eq!(r, Some(RotationReason::Time));
}

#[test]
fn message_count_fires_even_if_suspicious_cap_active() {
    let clock = Arc::new(MockClock::new());
    let mut ctrl = build_controller_with_clock(clock.clone());

    ctrl.note_suspicious_event(SuspiciousEventKind::ScreenRecorder);
    let _ = ctrl.check_for_rotation().unwrap();
    ctrl.note_rotation_completed();

    // Cap is active for the next 5 minutes; send 500 messages well
    // within that window.
    for _ in 0..500 {
        ctrl.note_message_sent();
    }
    let r = ctrl.check_for_rotation();
    assert_eq!(r, Some(RotationReason::MessageCount));
}

// ---- forced (membership / recipient) ----

#[test]
fn membership_change_triggers_immediate_rotation() {
    let clock = Arc::new(MockClock::new());
    let mut ctrl = build_controller_with_clock(clock.clone());
    ctrl.note_membership_change();
    assert_eq!(ctrl.check_for_rotation(), Some(RotationReason::Membership));
}

#[test]
fn recipient_request_triggers_immediate_rotation() {
    let clock = Arc::new(MockClock::new());
    let mut ctrl = build_controller_with_clock(clock.clone());
    ctrl.note_recipient_request();
    assert_eq!(ctrl.check_for_rotation(), Some(RotationReason::Recipient));
}

#[test]
fn forced_rotations_stack_and_drain_one_per_check() {
    let clock = Arc::new(MockClock::new());
    let mut ctrl = build_controller_with_clock(clock.clone());
    ctrl.note_membership_change();
    ctrl.note_recipient_request();
    assert_eq!(ctrl.check_for_rotation(), Some(RotationReason::Membership));
    ctrl.note_rotation_completed();
    assert_eq!(ctrl.check_for_rotation(), Some(RotationReason::Recipient));
}

#[test]
fn membership_fires_even_during_suspicious_cap_window() {
    let clock = Arc::new(MockClock::new());
    let mut ctrl = build_controller_with_clock(clock.clone());
    ctrl.note_suspicious_event(SuspiciousEventKind::ScreenRecorder);
    let _ = ctrl.check_for_rotation().unwrap();
    ctrl.note_rotation_completed();

    clock.advance(Duration::from_secs(60));
    ctrl.note_membership_change();
    assert_eq!(ctrl.check_for_rotation(), Some(RotationReason::Membership));
}

// ---- precedence ----

#[test]
fn forced_takes_precedence_over_time_and_message_count() {
    let clock = Arc::new(MockClock::new());
    let mut ctrl = build_controller_with_clock(clock.clone());
    // Set up: time elapsed AND message count over AND forced pending.
    clock.advance(Duration::from_secs(2 * 60 * 60));
    for _ in 0..600 {
        ctrl.note_message_sent();
    }
    ctrl.note_membership_change();
    let r = ctrl.check_for_rotation();
    assert_eq!(r, Some(RotationReason::Membership));
}

// ---- invariant: cap_exempt classification ----

#[test]
fn rotation_reason_cap_exempt_classification() {
    assert!(RotationReason::Time.is_cap_exempt());
    assert!(RotationReason::MessageCount.is_cap_exempt());
    assert!(RotationReason::Membership.is_cap_exempt());
    assert!(RotationReason::Recipient.is_cap_exempt());
    assert!(!RotationReason::Suspicious(SuspiciousEventKind::Idle).is_cap_exempt());
    assert!(!RotationReason::Suspicious(SuspiciousEventKind::ScreenRecorder).is_cap_exempt());
    assert!(!RotationReason::Suspicious(SuspiciousEventKind::UsbCaptureDevice).is_cap_exempt());
    assert!(!RotationReason::Suspicious(SuspiciousEventKind::Suspend).is_cap_exempt());
    assert!(!RotationReason::Suspicious(SuspiciousEventKind::Duress).is_cap_exempt());
    assert!(!RotationReason::Suspicious(SuspiciousEventKind::Other).is_cap_exempt());
}

// ---- end-to-end with the real SenderChain ----

#[test]
fn controller_drives_real_sender_chain_rotate() {
    // The controller doesn't talk to the SenderChain directly, but
    // we verify the integration shape: caller polls controller, on
    // Some(reason) calls SenderChain::rotate, then notes completion.
    let clock = Arc::new(MockClock::new());
    let mut ctrl = build_controller_with_clock(clock.clone());

    let mut chain = SenderChain::new().expect("sender chain");
    let initial_chain_id = chain.current_chain_id();
    let (_sec, ik_pub) = x25519::generate_keypair();
    let ctx = SenderContext {
        sender_ik_x25519_pub: ik_pub,
        sender_ik_mlkem_pub: vec![0u8; 16],
        group_id: b"g1".to_vec(),
        session_version: SESSION_VERSION_V1,
    };

    // Drive 500 sends; controller should fire MessageCount on the
    // 500th and the caller responds by rotating + noting completion.
    for i in 0..500 {
        let _ct = chain.encrypt(&[i as u8], &ctx).expect("encrypt");
        ctrl.note_message_sent();
    }
    assert_eq!(
        ctrl.check_for_rotation(),
        Some(RotationReason::MessageCount),
    );

    chain.rotate().expect("rotate");
    ctrl.note_rotation_completed();

    assert_eq!(chain.current_chain_id(), initial_chain_id + 1);
    assert_eq!(chain.current_n(), 0);
    assert_eq!(ctrl.messages_since_rotation(), 0);
}

// ---- non-default config ----

#[test]
fn shorter_idle_trigger_via_config() {
    let clock = Arc::new(MockClock::new());
    let cfg = RotationConfig {
        idle_trigger: Duration::from_secs(30 * 60),
        ..RotationConfig::default()
    };
    let mut ctrl = build_controller_with_clock_and_config(clock.clone(), cfg);
    ctrl.note_message_sent();
    clock.advance(Duration::from_secs(29 * 60));
    assert_eq!(ctrl.check_for_rotation(), None);
    clock.advance(Duration::from_secs(60));
    let r = ctrl.check_for_rotation().unwrap();
    assert_eq!(r, RotationReason::Suspicious(SuspiciousEventKind::Idle));
}

#[test]
fn shorter_time_trigger_via_config() {
    let clock = Arc::new(MockClock::new());
    let cfg = RotationConfig {
        time_trigger: Duration::from_secs(10 * 60),
        ..RotationConfig::default()
    };
    let mut ctrl = build_controller_with_clock_and_config(clock.clone(), cfg);
    clock.advance(Duration::from_secs(10 * 60));
    assert_eq!(ctrl.check_for_rotation(), Some(RotationReason::Time));
}
