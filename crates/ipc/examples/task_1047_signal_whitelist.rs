use ipc::signal_whitelist::{
    run_signal_allowance_command, signal_allowance_direction_state, SignalAllowanceAction,
    SignalAllowanceKind,
};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DirectCommandReport {
    direct_added_stable_id: String,
    direct_removed_stable_id: String,
    direct_add_changed: bool,
    direct_remove_changed: bool,
    direct_allowed_after_add: bool,
    direct_allowed_after_remove: bool,
    direct_one_way_serialized_tick_fields: usize,
    direct_one_way_tick: &'static str,
    direct_two_way_tick: &'static str,
    direct_after_remove_tick: &'static str,
    group_saved_directions: u8,
    group_tick: &'static str,
}

fn main() {
    let store = parse_store().unwrap_or_else(|error| fail(&error));
    std::fs::create_dir_all(&store).unwrap_or_else(|error| {
        fail(&format!(
            "TASK1047 direct command could not create store: {error}"
        ))
    });

    let direct_add = command(
        &store,
        SignalAllowanceAction::Add,
        SignalAllowanceKind::DirectMessage,
        "signal-account-1047-a",
        "signal-account-1047-b",
    );
    let direct_one_way = state(
        &store,
        SignalAllowanceKind::DirectMessage,
        "signal-account-1047-a",
        "signal-account-1047-b",
    );
    let reciprocal_add = command(
        &store,
        SignalAllowanceAction::Add,
        SignalAllowanceKind::DirectMessage,
        "signal-account-1047-b",
        "signal-account-1047-a",
    );
    let direct_two_way = state(
        &store,
        SignalAllowanceKind::DirectMessage,
        "signal-account-1047-a",
        "signal-account-1047-b",
    );
    let direct_remove = command(
        &store,
        SignalAllowanceAction::Remove,
        SignalAllowanceKind::DirectMessage,
        "signal-account-1047-a",
        "signal-account-1047-b",
    );
    let direct_after_remove = state(
        &store,
        SignalAllowanceKind::DirectMessage,
        "signal-account-1047-a",
        "signal-account-1047-b",
    );

    command(
        &store,
        SignalAllowanceAction::Add,
        SignalAllowanceKind::GroupChat,
        "signal-group-member-1047-a",
        "signal-group-member-1047-b",
    );
    command(
        &store,
        SignalAllowanceAction::Add,
        SignalAllowanceKind::GroupChat,
        "signal-group-member-1047-b",
        "signal-group-member-1047-a",
    );
    let group = state(
        &store,
        SignalAllowanceKind::GroupChat,
        "signal-group-member-1047-a",
        "signal-group-member-1047-b",
    );

    assert!(direct_add.changed && direct_add.allowed_after);
    assert_eq!(direct_add.stable_id, direct_remove.stable_id);
    assert_eq!(direct_one_way.saved_directions, 1);
    assert!(direct_one_way.verification_tick.is_none());
    assert!(reciprocal_add.changed);
    assert_eq!(direct_two_way.saved_directions, 2);
    assert!(direct_two_way.verification_tick.is_some());
    assert!(direct_remove.changed && !direct_remove.allowed_after);
    assert_eq!(direct_after_remove.saved_directions, 1);
    assert!(direct_after_remove.verification_tick.is_none());
    assert_eq!(group.saved_directions, 2);
    assert!(group.verification_tick.is_some());

    let report = DirectCommandReport {
        direct_added_stable_id: direct_add.stable_id,
        direct_removed_stable_id: direct_remove.stable_id,
        direct_add_changed: direct_add.changed,
        direct_remove_changed: direct_remove.changed,
        direct_allowed_after_add: direct_add.allowed_after,
        direct_allowed_after_remove: direct_remove.allowed_after,
        direct_one_way_serialized_tick_fields: usize::from(
            serde_json::to_value(&direct_one_way)
                .expect("TASK1047 one-way state serializes")
                .get("verificationTick")
                .is_some(),
        ),
        direct_one_way_tick: tick_name(direct_one_way.verification_tick.is_some()),
        direct_two_way_tick: tick_name(direct_two_way.verification_tick.is_some()),
        direct_after_remove_tick: tick_name(direct_after_remove.verification_tick.is_some()),
        group_saved_directions: group.saved_directions,
        group_tick: tick_name(group.verification_tick.is_some()),
    };
    println!(
        "{}",
        serde_json::to_string(&report).expect("TASK1047 report serializes")
    );
}

fn parse_store() -> Result<PathBuf, String> {
    let mut args = std::env::args_os().skip(1);
    match (args.next(), args.next(), args.next()) {
        (Some(flag), Some(path), None) if flag == "--store" => Ok(PathBuf::from(path)),
        _ => Err("usage: task_1047_signal_whitelist --store <directory>".to_owned()),
    }
}

fn command(
    store: &std::path::Path,
    action: SignalAllowanceAction,
    kind: SignalAllowanceKind,
    first: &str,
    second: &str,
) -> ipc::signal_whitelist::SignalAllowanceCommandReceipt {
    run_signal_allowance_command(store, action, kind, first, second)
        .unwrap_or_else(|error| fail(&error))
}

fn state(
    store: &std::path::Path,
    kind: SignalAllowanceKind,
    first: &str,
    second: &str,
) -> ipc::signal_whitelist::SignalAllowanceDirectionState {
    signal_allowance_direction_state(store, kind, first, second)
        .unwrap_or_else(|error| fail(&error))
}

fn tick_name(present: bool) -> &'static str {
    if present {
        "present"
    } else {
        "withheld"
    }
}

fn fail(message: &str) -> ! {
    eprintln!("TASK1047_ERROR={message}");
    std::process::exit(1)
}
