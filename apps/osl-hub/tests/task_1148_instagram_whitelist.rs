#![cfg(feature = "core")]

use osl_privacy_hub::allowed_place_commands::{run_allowed_place_cli, HeadlessCommandResult};
use serde_json::Value;
use std::ffi::OsString;
use tempfile::TempDir;

const ACCOUNT_A: &str = "instagram-account-a-1148";
const ACCOUNT_B: &str = "instagram-account-b-1148";
const KINDS: [&str; 3] = ["direct_message", "group_chat", "public_post"];

fn command(store: &str, operation: &str, arguments: &[(&str, &str)]) -> Value {
    let mut args = vec![
        OsString::from("osl-privacy-hub"),
        OsString::from("--allowed-place"),
        OsString::from(operation),
        OsString::from("--store"),
        OsString::from(store),
    ];
    for (name, value) in arguments {
        args.push(OsString::from(format!("--{name}")));
        args.push(OsString::from(value));
    }
    let HeadlessCommandResult { exit_code, stdout } =
        run_allowed_place_cli(args).expect("allowed-place command is recognized");
    assert_eq!(exit_code, 0, "command failed: {stdout}");
    serde_json::from_str(stdout.trim_end()).expect("command returns one JSON object")
}

fn stable_id(account: &str, kind: &str, peer: &str) -> String {
    format!("instagram:{account}:{kind}:{peer}")
}

fn add(store: &str, account: &str, kind: &str, peer: &str) -> Value {
    let id = stable_id(account, kind, peer);
    command(
        store,
        "add",
        &[
            ("app", "instagram"),
            ("account", account),
            ("kind", kind),
            ("stable-id", &id),
        ],
    )
}

fn remove(store: &str, account: &str, kind: &str, peer: &str) -> Value {
    let id = stable_id(account, kind, peer);
    command(store, "remove", &[("stable-id", &id)])
}

fn compare(store: &str, kind: &str) -> Value {
    command(
        store,
        "compare",
        &[
            ("app", "instagram"),
            ("kind", kind),
            ("first-account", ACCOUNT_A),
            ("second-account", ACCOUNT_B),
        ],
    )
}

#[test]
fn task_1148_commands_store_then_remove_every_instagram_allowance_without_one_way_tick() {
    let dir = TempDir::new().expect("create task 1148 store");
    let store = dir.path().to_string_lossy();
    let mut adds = 0usize;
    let mut removes = 0usize;
    let mut two_way_ticks = 0usize;
    let mut one_way_visible_ticks = 0usize;

    for kind in KINDS {
        let first_add = add(&store, ACCOUNT_A, kind, ACCOUNT_B);
        adds += usize::from(first_add["ok"] == true);
        let first_one_way = compare(&store, kind);
        one_way_visible_ticks += usize::from(first_one_way["verificationState"] == "visible");

        let second_add = add(&store, ACCOUNT_B, kind, ACCOUNT_A);
        adds += usize::from(second_add["ok"] == true);
        let two_way = compare(&store, kind);
        two_way_ticks += usize::from(two_way["verificationState"] == "visible");

        let first_remove = remove(&store, ACCOUNT_A, kind, ACCOUNT_B);
        removes += usize::from(first_remove["removed"] == true);
        let second_one_way = compare(&store, kind);
        one_way_visible_ticks += usize::from(second_one_way["verificationState"] == "visible");

        let second_remove = remove(&store, ACCOUNT_B, kind, ACCOUNT_A);
        removes += usize::from(second_remove["removed"] == true);
        let none = compare(&store, kind);

        assert_eq!(first_add["command"], "add");
        assert_eq!(first_one_way["state"], "one-way");
        assert_eq!(first_one_way["savedDirections"], 1);
        assert_eq!(first_one_way["verificationState"], "hidden");
        assert_eq!(two_way["state"], "two-way");
        assert_eq!(two_way["savedDirections"], 2);
        assert_eq!(two_way["verificationState"], "visible");
        assert_eq!(first_remove["command"], "remove");
        assert_eq!(second_one_way["state"], "one-way");
        assert_eq!(second_one_way["savedDirections"], 1);
        assert_eq!(second_one_way["verificationState"], "hidden");
        assert_eq!(none["state"], "none");
        assert_eq!(none["savedDirections"], 0);
        assert_eq!(none["verificationState"], "hidden");

        println!(
            "TASK1148 kind={kind} add_commands=2 remove_commands=2 two_way_tick={} first_one_way_tick={} second_one_way_tick={} final_state={}",
            two_way["verificationState"].as_str().unwrap_or("missing"),
            first_one_way["verificationState"].as_str().unwrap_or("missing"),
            second_one_way["verificationState"].as_str().unwrap_or("missing"),
            none["state"].as_str().unwrap_or("missing"),
        );
    }

    let final_list = command(&store, "list", &[]);
    println!(
        "TASK1148 kinds={} add_commands={adds} remove_commands={removes} two_way_visible_ticks={two_way_ticks} one_way_visible_ticks={one_way_visible_ticks} final_allowance_count={}",
        KINDS.join(","),
        final_list["count"],
    );

    assert_eq!(adds, 6);
    assert_eq!(removes, 6);
    assert_eq!(two_way_ticks, 3);
    assert_eq!(one_way_visible_ticks, 0);
    assert_eq!(final_list["count"], 0);
}
