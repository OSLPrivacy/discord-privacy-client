//! TASK 0110 harness: drives the real allowed-place command functions from the
//! command line so the UI wiring test can run `add`, `remove` and `allowed`
//! against the durable store instead of a stand-in.
//!
//! Usage:
//!   task_0110_allowed_place_cli add     --store <dir> --app <app> --account <acct> --kind <kind> --stable-id <id>
//!   task_0110_allowed_place_cli remove  --store <dir> --stable-id <id>
//!   task_0110_allowed_place_cli allowed --store <dir> --app <app> --account <acct> --kind <kind> --stable-id <id>
//!
//! Prints one JSON object per run and exits 0 on success, 1 on refusal.

use ipc::allowed_places::{
    add_allowed_place_record, allowed_place_is_allowed, remove_allowed_place_record,
    AllowedPlaceQuery, AllowedPlaceRecord,
};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match run(&args) {
        Ok(json) => println!("{json}"),
        Err(error) => {
            println!(
                "{}",
                serde_json::json!({ "ok": false, "error": error })
            );
            std::process::exit(1);
        }
    }
}

fn run(args: &[String]) -> Result<String, String> {
    let command = args.first().cloned().unwrap_or_default();
    let mut values = BTreeMap::new();
    let mut index = 1usize;
    while index + 1 < args.len() + 1 {
        let Some(key) = args.get(index) else { break };
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("missing value for {key}"))?;
        if !key.starts_with("--") {
            return Err(format!("unexpected argument {key}"));
        }
        values.insert(key.trim_start_matches("--").to_owned(), value.clone());
        index += 2;
    }
    let store = values
        .get("store")
        .map(PathBuf::from)
        .ok_or_else(|| "missing --store".to_owned())?;
    let required = |key: &str| -> Result<String, String> {
        values
            .get(key)
            .cloned()
            .ok_or_else(|| format!("missing --{key}"))
    };

    match command.as_str() {
        "add" => {
            let record = AllowedPlaceRecord::from_parts(
                required("app")?,
                required("account")?,
                required("kind")?,
                required("stable-id")?,
            );
            add_allowed_place_record(&store, &record).map_err(|error| error.to_string())?;
            Ok(serde_json::json!({
                "command": "add",
                "ok": true,
                "record": { "stableId": record.stable_id },
            })
            .to_string())
        }
        "remove" => {
            let stable_id = required("stable-id")?;
            let removed = remove_allowed_place_record(&store, &stable_id)
                .map_err(|error| error.to_string())?;
            Ok(serde_json::json!({
                "command": "remove",
                "ok": true,
                "stableId": stable_id,
                "removed": removed,
            })
            .to_string())
        }
        "allowed" => {
            let query = AllowedPlaceQuery {
                app: required("app")?,
                account: required("account")?,
                kind: required("kind")?,
                stable_id: required("stable-id")?,
            };
            let allowed =
                allowed_place_is_allowed(&store, &query).map_err(|error| error.to_string())?;
            Ok(serde_json::json!({
                "command": "allowed",
                "ok": true,
                "allowed": allowed,
            })
            .to_string())
        }
        _ => Err("usage: task_0110_allowed_place_cli <add|remove|allowed> --store <dir> [--app --account --kind --stable-id]".to_owned()),
    }
}
