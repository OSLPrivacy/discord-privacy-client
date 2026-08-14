use std::ffi::OsString;

use ipc::allowed_places::AllowedPlaceRecord;
use ipc::auto_whitelist_rules::WhatsAppWhitelistKind;
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
        ("app", "whatsapp"),
        ("kind", kind),
        ("first-account", first),
        ("second-account", second),
    ]
}

#[test]
fn direct_commands_store_every_whatsapp_kind_and_never_tick_one_way() {
    let store_dir = TempDir::new().expect("create task 1082 WhatsApp allowance store");
    let store = store_dir.path().to_string_lossy();
    let first = "task-1082-first";
    let second = "task-1082-second";
    let mut two_way_ticks = 0usize;
    let mut one_way_ticks = 0usize;
    let mut direct_added = 0usize;
    let mut direct_removed = 0usize;

    for kind in WhatsAppWhitelistKind::ALL {
        let kind_name = kind.allowed_place_kind();
        let forward = AllowedPlaceRecord::whatsapp(first, kind, second);
        let reverse = AllowedPlaceRecord::whatsapp(second, kind, first);

        let added_forward = successful_json(command(&store, "add", &record_args(&forward)));
        assert_eq!(added_forward["record"]["stable_id"], forward.stable_id);
        if kind == WhatsAppWhitelistKind::DirectMessage {
            direct_added += usize::from(added_forward["record"]["stable_id"] == forward.stable_id);
        }

        let one_way = successful_json(command(
            &store,
            "compare",
            &compare_args(kind_name, first, second),
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
            &compare_args(kind_name, first, second),
        ));
        assert_eq!(two_way["savedDirections"], 2);
        assert_eq!(two_way["whitelistState"], "two-way");
        assert_eq!(two_way["verificationState"], "visible");
        assert_eq!(two_way["verificationTick"], true);
        two_way_ticks += usize::from(two_way["verificationTick"] == true);

        let removed_forward = successful_json(command(
            &store,
            "remove",
            &[("stable-id", forward.stable_id.as_str())],
        ));
        assert_eq!(removed_forward["removed"], true);
        if kind == WhatsAppWhitelistKind::DirectMessage {
            direct_removed += usize::from(removed_forward["removed"] == true);
        }
        assert_eq!(
            successful_json(command(
                &store,
                "remove",
                &[("stable-id", reverse.stable_id.as_str())],
            ))["removed"],
            true
        );
    }

    let final_list = successful_json(command(&store, "list", &[]));
    let unsupported = command(
        &store,
        "compare",
        &compare_args("public_post", first, second),
    );
    assert_eq!(unsupported.exit_code, 2);
    let unsupported_json: Value = serde_json::from_str(unsupported.stdout.trim())
        .expect("unsupported-kind refusal is one JSON object");
    assert_eq!(
        unsupported_json["error"],
        "OSL: unknown WhatsApp whitelist kind 'public_post'"
    );
    assert_eq!(direct_added, 1);
    assert_eq!(direct_removed, 1);
    assert_eq!(one_way_ticks, 0);
    assert_eq!(two_way_ticks, 3);
    assert_eq!(final_list["count"], 0);

    println!(
        "TASK1082 kinds=3 names=direct_message,group_chat,channel two_way_ticks={} one_way_ticks={} unsupported_refused={}",
        two_way_ticks, one_way_ticks
        , usize::from(unsupported_json["ok"] == false)
    );
    println!(
        "TASK1082 direct_command added={} removed={} remaining={}",
        direct_added, direct_removed, final_list["count"]
    );
}
