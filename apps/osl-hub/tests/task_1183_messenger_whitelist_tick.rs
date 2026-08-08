use std::ffi::OsString;

use osl_privacy_hub::allowed_place_commands::{run_allowed_place_cli, HeadlessCommandResult};
use serde_json::Value;
use tempfile::TempDir;

const APP: &str = "messenger";
const FIRST_ACCOUNT: &str = "messenger-account-1183-a";
const SECOND_ACCOUNT: &str = "messenger-account-1183-b";
const KINDS: [&str; 3] = ["direct_message", "group_chat", "community"];

#[test]
fn task_1183_commands_add_then_remove_each_messenger_allowance_without_one_way_tick() {
    let listed_kinds = ipc::commands::cmd_osl_get_messenger_whitelist_kinds()
        .expect("list Messenger whitelist kinds");
    let listed_ids = listed_kinds
        .iter()
        .map(|kind| kind.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(listed_ids, KINDS);
    let store = TempDir::new().expect("create TASK 1183 Messenger store");
    let store = store.path().to_string_lossy().into_owned();
    let mut added = 0usize;
    let mut removed = 0usize;
    let mut two_way_ticks = 0usize;
    let mut one_way_checks = 0usize;
    let mut one_way_ticks = 0usize;

    for kind in KINDS {
        let first_to_second = stable_id(FIRST_ACCOUNT, kind, SECOND_ACCOUNT);
        let second_to_first = stable_id(SECOND_ACCOUNT, kind, FIRST_ACCOUNT);

        add(&store, FIRST_ACCOUNT, kind, &first_to_second);
        added += 1;
        let first_one_way = tick(&store, kind);
        assert_tick(&first_one_way, "one-way", false);
        one_way_checks += 1;
        one_way_ticks += usize::from(ticked(&first_one_way));

        add(&store, SECOND_ACCOUNT, kind, &second_to_first);
        added += 1;
        let bilateral = tick(&store, kind);
        assert_tick(&bilateral, "two-way", true);
        two_way_ticks += usize::from(ticked(&bilateral));

        remove(&store, &first_to_second);
        removed += 1;
        let second_one_way = tick(&store, kind);
        assert_tick(&second_one_way, "one-way", false);
        one_way_checks += 1;
        one_way_ticks += usize::from(ticked(&second_one_way));

        remove(&store, &second_to_first);
        removed += 1;
        let none = tick(&store, kind);
        assert_tick(&none, "none", false);

        println!(
            "TASK1183 kind={kind} added_directions=2 removed_directions=2 two_way_tick={} first_one_way_tick={} second_one_way_tick={} final_state={}",
            ticked(&bilateral),
            ticked(&first_one_way),
            ticked(&second_one_way),
            none["state"]["state"].as_str().unwrap_or("missing")
        );
    }

    assert_eq!(added, 6);
    assert_eq!(removed, 6);
    assert_eq!(two_way_ticks, 3);
    assert_eq!(one_way_checks, 6);
    assert_eq!(one_way_ticks, 0);
    println!(
        "TASK1183 kinds={} added_allowances={added} removed_allowances={removed} two_way_ticks={two_way_ticks} one_way_checks={one_way_checks} one_way_ticks={one_way_ticks}",
        KINDS.join(",")
    );
}

fn add(store: &str, account: &str, kind: &str, stable_id: &str) {
    let value = command(&[
        "osl-privacy-hub",
        "--allowed-place",
        "add",
        "--store",
        store,
        "--app",
        APP,
        "--account",
        account,
        "--kind",
        kind,
        "--stable-id",
        stable_id,
    ]);
    assert_eq!(value["command"], "add");
    assert_eq!(value["ok"], true);
    assert_eq!(value["record"]["stable_id"], stable_id);
}

fn remove(store: &str, stable_id: &str) {
    let value = command(&[
        "osl-privacy-hub",
        "--allowed-place",
        "remove",
        "--store",
        store,
        "--stable-id",
        stable_id,
    ]);
    assert_eq!(value["command"], "remove");
    assert_eq!(value["ok"], true);
    assert_eq!(value["stableId"], stable_id);
    assert_eq!(value["removed"], true);
}

fn tick(store: &str, kind: &str) -> Value {
    command(&[
        "osl-privacy-hub",
        "--allowed-place",
        "tick",
        "--store",
        store,
        "--app",
        APP,
        "--kind",
        kind,
        "--first-account",
        FIRST_ACCOUNT,
        "--second-account",
        SECOND_ACCOUNT,
    ])
}

fn assert_tick(value: &Value, expected_state: &str, expected_ticked: bool) {
    assert_eq!(value["command"], "tick");
    assert_eq!(value["ok"], true);
    assert_eq!(value["state"]["state"], expected_state);
    assert_eq!(value["state"]["verificationTicked"], expected_ticked);
}

fn ticked(value: &Value) -> bool {
    value["state"]["verificationTicked"]
        .as_bool()
        .expect("tick command returns verificationTicked")
}

fn command(args: &[&str]) -> Value {
    let result: HeadlessCommandResult = run_allowed_place_cli(
        args.iter()
            .map(|argument| OsString::from(argument.to_owned())),
    )
    .expect("allowed-place command recognized");
    assert_eq!(result.exit_code, 0, "{}", result.stdout);
    serde_json::from_str(result.stdout.trim()).expect("command returns one JSON object")
}

fn stable_id(account: &str, kind: &str, peer: &str) -> String {
    format!("{APP}:{account}:{kind}:{peer}")
}
