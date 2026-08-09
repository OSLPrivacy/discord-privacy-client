use std::ffi::OsString;

use ipc::allowed_places::{telegram_whitelist_kinds, AllowedPlaceRecord};
use osl_privacy_hub::allowed_place_commands::{run_allowed_place_cli, HeadlessCommandResult};
use serde_json::Value;
use tempfile::TempDir;

fn command(store: &str, operation: &str, arguments: &[(&str, &str)]) -> HeadlessCommandResult {
    let mut args = vec![
        OsString::from("osl-allowed-place"),
        OsString::from("--allowed-place"),
        OsString::from(operation),
        OsString::from("--store"),
        OsString::from(store),
    ];
    for (name, value) in arguments {
        args.push(OsString::from(format!("--{name}")));
        args.push(OsString::from(value));
    }
    run_allowed_place_cli(args).expect("direct allowed-place command is recognized")
}

fn successful_json(result: HeadlessCommandResult) -> Value {
    assert_eq!(result.exit_code, 0, "command failed: {}", result.stdout);
    let value: Value =
        serde_json::from_str(result.stdout.trim()).expect("direct command returns one JSON object");
    assert_eq!(value["ok"], true);
    value
}

fn record_args(record: &AllowedPlaceRecord) -> Vec<(&str, &str)> {
    vec![
        ("app", record.app.as_str()),
        ("account", record.account.as_str()),
        ("kind", record.kind.as_str()),
        ("stable-id", record.stable_id.as_str()),
    ]
}

fn compare_args<'a>(kind: &'a str, first: &'a str, second: &'a str) -> [(&'a str, &'a str); 4] {
    [
        ("app", "telegram"),
        ("kind", kind),
        ("first-account", first),
        ("second-account", second),
    ]
}

#[test]
fn direct_commands_store_every_telegram_kind_and_never_tick_one_way() {
    let store_dir = TempDir::new().expect("create task 1020 Telegram allowance store");
    let store = store_dir.path().to_string_lossy();
    let kinds = telegram_whitelist_kinds();
    let first = "task-1020-first";
    let second = "task-1020-second";
    let mut two_way_ticks = 0usize;
    let mut one_way_ticks = 0usize;

    for supported in &kinds {
        let forward = AllowedPlaceRecord::telegram(first, &supported.name, second)
            .expect("supported Telegram kind builds a forward allowance");
        let reverse = AllowedPlaceRecord::telegram(second, &supported.name, first)
            .expect("supported Telegram kind builds a reverse allowance");

        let added_forward = successful_json(command(&store, "add", &record_args(&forward)));
        assert_eq!(added_forward["record"]["stable_id"], forward.stable_id);

        let one_way = successful_json(command(
            &store,
            "compare",
            &compare_args(&supported.name, first, second),
        ));
        assert_eq!(one_way["savedDirections"], 1);
        assert_eq!(one_way["whitelistState"], "one-way");
        assert_eq!(one_way["verificationState"], "hidden");
        assert_eq!(one_way["verificationTick"], false);
        one_way_ticks += usize::from(one_way["verificationTick"] == true);

        successful_json(command(&store, "add", &record_args(&reverse)));
        let two_way = successful_json(command(
            &store,
            "compare",
            &compare_args(&supported.name, first, second),
        ));
        assert_eq!(two_way["savedDirections"], 2);
        assert_eq!(two_way["whitelistState"], "two-way");
        assert_eq!(two_way["verificationState"], "visible");
        assert_eq!(two_way["verificationTick"], true);
        two_way_ticks += usize::from(two_way["verificationTick"] == true);

        assert_eq!(
            successful_json(command(
                &store,
                "remove",
                &[("stable-id", forward.stable_id.as_str())],
            ))["removed"],
            true
        );
        assert_eq!(
            successful_json(command(
                &store,
                "remove",
                &[("stable-id", reverse.stable_id.as_str())],
            ))["removed"],
            true
        );
    }

    let direct = AllowedPlaceRecord::telegram(first, "direct_message", "task-1020-direct-peer")
        .expect("direct-message allowance is supported");
    let direct_peer = "task-1020-direct-peer";
    let direct_add = successful_json(command(&store, "add", &record_args(&direct)));
    let direct_one_way = successful_json(command(
        &store,
        "compare",
        &compare_args("direct_message", first, direct_peer),
    ));
    let direct_remove = successful_json(command(
        &store,
        "remove",
        &[("stable-id", direct.stable_id.as_str())],
    ));
    let direct_after_remove = successful_json(command(
        &store,
        "compare",
        &compare_args("direct_message", first, direct_peer),
    ));
    let final_list = successful_json(command(&store, "list", &[]));

    assert_eq!(direct_add["command"], "add");
    assert_eq!(direct_one_way["savedDirections"], 1);
    assert_eq!(direct_one_way["whitelistState"], "one-way");
    assert_eq!(direct_one_way["verificationTick"], false);
    assert_eq!(direct_remove["command"], "remove");
    assert_eq!(direct_remove["removed"], true);
    assert_eq!(direct_after_remove["savedDirections"], 0);
    assert_eq!(direct_after_remove["whitelistState"], "none");
    assert_eq!(direct_after_remove["verificationTick"], false);
    assert_eq!(final_list["count"], 0);

    println!(
        "TASK1020 kinds={} names={} two_way_ticks={} one_way_ticks={}",
        kinds.len(),
        kinds
            .iter()
            .map(|kind| kind.name.as_str())
            .collect::<Vec<_>>()
            .join(","),
        two_way_ticks,
        one_way_ticks
    );
    println!(
        "TASK1020 direct_command added={} saved_directions={} whitelist_state={} one_way_tick={} removed={} saved_after_remove={} tick_after_remove={} remaining={}",
        usize::from(direct_add["record"]["stable_id"] == direct.stable_id),
        direct_one_way["savedDirections"],
        direct_one_way["whitelistState"].as_str().unwrap_or(""),
        direct_one_way["verificationTick"],
        usize::from(direct_remove["removed"] == true),
        direct_after_remove["savedDirections"],
        direct_after_remove["verificationTick"],
        final_list["count"]
    );
}
