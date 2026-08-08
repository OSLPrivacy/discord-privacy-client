//! TASK 0112 harness: drives the real allowed-place command functions from the
//! command line so the break-the-target test can seed, press, count and dump
//! against the durable store the whitelist button targets.
//!
//! Usage:
//!   task_0112_allowed_place_cli add     --store <dir> --app <app> --account <acct> --kind <kind> --stable-id <id>
//!   task_0112_allowed_place_cli allowed --store <dir> --app <app> --account <acct> --kind <kind> --stable-id <id>
//!   task_0112_allowed_place_cli read    --store <dir> --stable-id <id>
//!   task_0112_allowed_place_cli count   --store <dir>
//!   task_0112_allowed_place_cli dump    --store <dir> --stable-id <id>
//!
//! Prints one JSON object per run and exits 0 on success, 1 on refusal. The
//! `dump` command prints the stored row's raw column values so a test can
//! compare a saved record byte for byte across presses.

use ipc::allowed_places::{
    add_allowed_place_record, allowed_place_is_allowed, allowed_places_db_path,
    list_allowed_place_records, read_allowed_place_record, AllowedPlaceQuery, AllowedPlaceRecord,
};
use rusqlite::{params, Connection};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match run(&args) {
        Ok(json) => println!("{json}"),
        Err(error) => {
            println!("{}", serde_json::json!({ "ok": false, "error": error }));
            std::process::exit(1);
        }
    }
}

fn run(args: &[String]) -> Result<String, String> {
    let command = args.first().cloned().unwrap_or_default();
    let mut values = BTreeMap::new();
    let mut index = 1usize;
    while let Some(key) = args.get(index) {
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
        "read" => {
            let stable_id = required("stable-id")?;
            let record = read_allowed_place_record(&store, &stable_id)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| format!("allowed-place record not found: {stable_id}"))?;
            Ok(serde_json::json!({
                "command": "read",
                "ok": true,
                "record": {
                    "app": record.app,
                    "account": record.account,
                    "kind": record.kind,
                    "stableId": record.stable_id,
                },
            })
            .to_string())
        }
        "count" => {
            let count = list_allowed_place_records(&store)
                .map_err(|error| error.to_string())?
                .len();
            Ok(serde_json::json!({
                "command": "count",
                "ok": true,
                "count": count,
            })
            .to_string())
        }
        "dump" => {
            let stable_id = required("stable-id")?;
            let conn = Connection::open(allowed_places_db_path(&store))
                .map_err(|error| error.to_string())?;
            let row = conn
                .query_row(
                    "SELECT app, account, kind, stable_id, place_name, person_name
                     FROM allowed_places WHERE stable_id = ?1",
                    params![stable_id],
                    |row| {
                        Ok([
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                            row.get::<_, String>(5)?,
                        ])
                    },
                )
                .map_err(|error| error.to_string())?;
            Ok(serde_json::json!({
                "command": "dump",
                "ok": true,
                "row": row,
            })
            .to_string())
        }
        _ => Err(
            "usage: task_0112_allowed_place_cli <add|allowed|read|count|dump> --store <dir> [--app --account --kind --stable-id]"
                .to_owned(),
        ),
    }
}
