use ipc::signal_whitelist::{
    run_signal_allowance_command, signal_allowance_direction_state, SignalAllowanceAction,
    SignalAllowanceKind,
};
use tempfile::TempDir;

#[test]
fn task_1047_direct_commands_store_both_signal_kinds_and_withhold_one_way_tick() {
    let store = TempDir::new().expect("create TASK 1047 store");

    prove_kind(
        store.path(),
        SignalAllowanceKind::DirectMessage,
        "signal-account-1047-a",
        "signal-account-1047-b",
    );
    prove_kind(
        store.path(),
        SignalAllowanceKind::GroupChat,
        "signal-group-member-1047-a",
        "signal-group-member-1047-b",
    );
}

fn prove_kind(store: &std::path::Path, kind: SignalAllowanceKind, first: &str, second: &str) {
    let first_add =
        run_signal_allowance_command(store, SignalAllowanceAction::Add, kind, first, second)
            .expect("direct add command succeeds");
    assert!(first_add.changed);
    assert!(first_add.allowed_after);

    let one_way =
        signal_allowance_direction_state(store, kind, first, second).expect("one-way state reads");
    let one_way_json = serde_json::to_value(&one_way).expect("one-way state serializes");
    assert_eq!(one_way.saved_directions, 1);
    assert_eq!(one_way.state, "one-way");
    assert!(one_way.verification_tick.is_none());
    assert!(one_way_json.get("verificationTick").is_none());

    let reciprocal_add =
        run_signal_allowance_command(store, SignalAllowanceAction::Add, kind, second, first)
            .expect("reciprocal add command succeeds");
    assert!(reciprocal_add.changed);

    let two_way =
        signal_allowance_direction_state(store, kind, first, second).expect("two-way state reads");
    assert_eq!(two_way.saved_directions, 2);
    assert_eq!(two_way.state, "two-way");
    assert!(two_way.verification_tick.is_some());

    let first_remove =
        run_signal_allowance_command(store, SignalAllowanceAction::Remove, kind, first, second)
            .expect("direct remove command succeeds");
    assert!(first_remove.changed);
    assert!(!first_remove.allowed_after);

    let after_remove = signal_allowance_direction_state(store, kind, first, second)
        .expect("post-remove state reads");
    let after_remove_json =
        serde_json::to_value(&after_remove).expect("post-remove state serializes");
    assert_eq!(after_remove.saved_directions, 1);
    assert_eq!(after_remove.state, "one-way");
    assert!(after_remove.verification_tick.is_none());
    assert!(after_remove_json.get("verificationTick").is_none());

    let reciprocal_remove =
        run_signal_allowance_command(store, SignalAllowanceAction::Remove, kind, second, first)
            .expect("reciprocal remove command succeeds");
    assert!(reciprocal_remove.changed);

    println!(
        "TASK1047 kind={} direct_add_changed={} reciprocal_add_changed={} two_way_saved_directions={} two_way_tick={} direct_remove_changed={} post_remove_saved_directions={} post_remove_tick={} serialized_one_way_tick_fields={}",
        kind.as_str(),
        first_add.changed,
        reciprocal_add.changed,
        two_way.saved_directions,
        two_way.verification_tick.is_some(),
        first_remove.changed,
        after_remove.saved_directions,
        if after_remove.verification_tick.is_some() { "present" } else { "withheld" },
        usize::from(one_way_json.get("verificationTick").is_some())
            + usize::from(after_remove_json.get("verificationTick").is_some()),
    );
}
