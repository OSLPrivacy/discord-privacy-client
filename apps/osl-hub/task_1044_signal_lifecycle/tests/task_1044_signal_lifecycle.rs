use task_1044_signal_lifecycle::signal_lifecycle_commands::{
    cmd_signal_burn_both_sides, cmd_signal_burn_their_side, cmd_signal_burn_your_side,
    cmd_signal_set_timer, cmd_signal_set_view_once, SignalProtectedMessage,
    SignalProtectedMessageStore,
};

const OWNER: &str = "signal-owner-1044";
const OTHER_OWNER: &str = "signal-other-owner-1044";
const NOW: u64 = 1_700_001_044;

fn seeded_store() -> SignalProtectedMessageStore {
    let mut store = SignalProtectedMessageStore::default();
    for id in [
        "signal-own-timer-1044",
        "signal-own-view-once-1044",
        "signal-own-your-side-1044",
        "signal-own-their-side-1044",
        "signal-own-both-sides-1044",
    ] {
        store
            .insert(SignalProtectedMessage::new(id, OWNER))
            .unwrap();
    }
    store
        .insert(SignalProtectedMessage::new(
            "signal-other-1044",
            OTHER_OWNER,
        ))
        .unwrap();
    store
}

#[test]
fn task_1044_signal_commands_return_expiry_and_only_change_owned_targets() {
    let mut store = seeded_store();

    let timer = cmd_signal_set_timer(&mut store, OWNER, "signal-own-timer-1044", 30, NOW)
        .expect("owned Signal timer is accepted");
    assert_eq!(timer.expires_at_unix_seconds, NOW + 30);
    assert!(!timer.view_once);

    let view_once =
        cmd_signal_set_view_once(&mut store, OWNER, "signal-own-view-once-1044", 60, NOW)
            .expect("owned Signal view-once is accepted");
    assert_eq!(view_once.expires_at_unix_seconds, NOW + 60);
    assert!(view_once.view_once);

    let your = cmd_signal_burn_your_side(
        &mut store,
        OWNER,
        vec!["signal-own-your-side-1044".to_owned()],
    )
    .expect("your-side burn accepts exactly the owned row");
    assert_eq!(your.target_message_ids, ["signal-own-your-side-1044"]);
    assert_eq!(
        (your.local_removed_count, your.remote_removed_count),
        (1, 0)
    );

    let their = cmd_signal_burn_their_side(
        &mut store,
        OWNER,
        vec!["signal-own-their-side-1044".to_owned()],
    )
    .expect("their-side burn accepts exactly the owned row");
    assert_eq!(their.target_message_ids, ["signal-own-their-side-1044"]);
    assert_eq!(
        (their.local_removed_count, their.remote_removed_count),
        (0, 1)
    );

    let both = cmd_signal_burn_both_sides(
        &mut store,
        OWNER,
        vec!["signal-own-both-sides-1044".to_owned()],
    )
    .expect("both-sides burn accepts exactly the owned row");
    assert_eq!(both.target_message_ids, ["signal-own-both-sides-1044"]);
    assert_eq!(
        (both.local_removed_count, both.remote_removed_count),
        (1, 1)
    );

    let refusal =
        cmd_signal_burn_both_sides(&mut store, OWNER, vec!["signal-other-1044".to_owned()])
            .expect_err("another person's Signal row is refused by name");
    assert!(refusal.contains("signal-other-1044"));
    let other = store.get("signal-other-1044").unwrap();
    assert!(other.local_present && other.remote_present);

    println!(
        "TASK1044_TIMER_EXPIRY={} TARGET={}",
        timer.expires_at_unix_seconds, timer.target_message_id
    );
    println!(
        "TASK1044_VIEW_ONCE_EXPIRY={} TARGET={}",
        view_once.expires_at_unix_seconds, view_once.target_message_id
    );
    println!(
        "TASK1044_YOUR_SIDE command={} targets={} local_removed={} remote_removed={}",
        your.command,
        your.target_message_ids.join(","),
        your.local_removed_count,
        your.remote_removed_count
    );
    println!(
        "TASK1044_THEIR_SIDE command={} targets={} local_removed={} remote_removed={}",
        their.command,
        their.target_message_ids.join(","),
        their.local_removed_count,
        their.remote_removed_count
    );
    println!(
        "TASK1044_BOTH_SIDES command={} targets={} local_removed={} remote_removed={}",
        both.command,
        both.target_message_ids.join(","),
        both.local_removed_count,
        both.remote_removed_count
    );
    println!("TASK1044_UNOWNED_REFUSAL={refusal}");
    println!(
        "TASK1044_UNOWNED_REMAINS local={} remote={}",
        other.local_present, other.remote_present
    );
}
