use ipc::allowed_places::{
    add_allowed_place_record, allowed_place_is_allowed, list_allowed_place_records,
    remove_allowed_place_record, AllowedPlaceQuery, AllowedPlaceRecord,
};
use serde::Serialize;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub const ALLOWED_PLACE_CLI_FLAG: &str = "--allowed-place";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "command")]
pub enum AllowedPlaceCommandJson {
    Add {
        ok: bool,
        record: AllowedPlaceRecord,
    },
    Remove {
        ok: bool,
        #[serde(rename = "stableId")]
        stable_id: String,
        removed: bool,
    },
    List {
        ok: bool,
        count: usize,
        records: Vec<AllowedPlaceRecord>,
    },
    Allowed {
        ok: bool,
        allowed: bool,
        query: AllowedPlaceQuery,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AllowedPlaceErrorJson {
    ok: bool,
    command: String,
    error: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadlessCommandResult {
    pub exit_code: i32,
    pub stdout: String,
}

pub fn run_allowed_place_cli_from_env() -> Option<i32> {
    let result = run_allowed_place_cli(std::env::args_os())?;
    print!("{}", result.stdout);
    Some(result.exit_code)
}

pub fn run_allowed_place_cli<I>(args: I) -> Option<HeadlessCommandResult>
where
    I: IntoIterator<Item = OsString>,
{
    let args = args
        .into_iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let flag = args.iter().position(|arg| arg == ALLOWED_PLACE_CLI_FLAG)?;
    let command = args.get(flag + 1).cloned().unwrap_or_default();
    let rest = &args[flag + 2..];
    let rendered = match run_allowed_place_command(&command, rest) {
        Ok(value) => HeadlessCommandResult {
            exit_code: 0,
            stdout: format_json_line(&value),
        },
        Err(error) => HeadlessCommandResult {
            exit_code: 2,
            stdout: format_json_line(&AllowedPlaceErrorJson {
                ok: false,
                command: if command.is_empty() {
                    "unknown".to_owned()
                } else {
                    command
                },
                error,
            }),
        },
    };
    Some(rendered)
}

pub fn add_allowed_place_json(
    store_dir: &Path,
    record: AllowedPlaceRecord,
) -> Result<AllowedPlaceCommandJson, String> {
    let record = add_allowed_place_record(store_dir, record).map_err(|error| error.to_string())?;
    Ok(AllowedPlaceCommandJson::Add { ok: true, record })
}

pub fn remove_allowed_place_json(
    store_dir: &Path,
    stable_id: String,
) -> Result<AllowedPlaceCommandJson, String> {
    let removed =
        remove_allowed_place_record(store_dir, &stable_id).map_err(|error| error.to_string())?;
    Ok(AllowedPlaceCommandJson::Remove {
        ok: true,
        stable_id,
        removed,
    })
}

pub fn list_allowed_places_json(store_dir: &Path) -> Result<AllowedPlaceCommandJson, String> {
    let records = list_allowed_place_records(store_dir).map_err(|error| error.to_string())?;
    Ok(AllowedPlaceCommandJson::List {
        ok: true,
        count: records.len(),
        records,
    })
}

pub fn allowed_place_allowed_json(
    store_dir: &Path,
    query: AllowedPlaceQuery,
) -> Result<AllowedPlaceCommandJson, String> {
    let allowed = allowed_place_is_allowed(store_dir, &query).map_err(|error| error.to_string())?;
    Ok(AllowedPlaceCommandJson::Allowed {
        ok: true,
        allowed,
        query,
    })
}

fn run_allowed_place_command(
    command: &str,
    args: &[String],
) -> Result<AllowedPlaceCommandJson, String> {
    let parsed = ParsedArgs::parse(args)?;
    match command {
        "add" => add_allowed_place_json(&parsed.store, parsed.record()?),
        "remove" => remove_allowed_place_json(&parsed.store, parsed.required("stable-id")?),
        "list" => list_allowed_places_json(&parsed.store),
        "allowed" => allowed_place_allowed_json(&parsed.store, parsed.query()?),
        _ => Err(
            "usage: --allowed-place <add|remove|list|allowed> --store <dir> [--app <app> --account <account> --kind <kind> --stable-id <stable-id>]"
                .to_owned(),
        ),
    }
}

#[derive(Debug)]
struct ParsedArgs {
    store: PathBuf,
    values: std::collections::BTreeMap<String, String>,
}

impl ParsedArgs {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut values = std::collections::BTreeMap::new();
        let mut index = 0usize;
        while index < args.len() {
            let key = args
                .get(index)
                .ok_or_else(|| "missing allowed-place argument".to_owned())?;
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
            .remove("store")
            .map(PathBuf::from)
            .ok_or_else(|| "missing --store".to_owned())?;
        Ok(Self { store, values })
    }

    fn record(&self) -> Result<AllowedPlaceRecord, String> {
        Ok(AllowedPlaceRecord {
            app: self.required("app")?,
            account: self.required("account")?,
            kind: self.required("kind")?,
            stable_id: self.required("stable-id")?,
        })
    }

    fn query(&self) -> Result<AllowedPlaceQuery, String> {
        Ok(AllowedPlaceQuery {
            app: self.required("app")?,
            account: self.required("account")?,
            kind: self.required("kind")?,
            stable_id: self.required("stable-id")?,
        })
    }

    fn required(&self, key: &str) -> Result<String, String> {
        self.values
            .get(key)
            .cloned()
            .ok_or_else(|| format!("missing --{key}"))
    }
}

fn format_json_line<T: Serialize>(value: &T) -> String {
    format!(
        "{}\n",
        serde_json::to_string(value).expect("allowed-place JSON response serializes")
    )
}

#[cfg(test)]
mod tests {
    use super::run_allowed_place_cli;
    use serde_json::Value;
    use std::ffi::OsString;
    use tempfile::TempDir;

    fn args(items: &[&str]) -> Vec<OsString> {
        items.iter().map(OsString::from).collect()
    }

    fn json(stdout: &str) -> Value {
        serde_json::from_str(stdout.trim_end()).expect("stdout is one JSON object")
    }

    #[test]
    fn allowed_place_headless_commands_return_json_without_gui_setup() {
        let dir = TempDir::new().unwrap();
        let store = dir.path().to_string_lossy();
        let stable_id = "discord:account-0107:direct_message:place-0107";

        let add = run_allowed_place_cli(args(&[
            "osl-privacy-hub",
            "--allowed-place",
            "add",
            "--store",
            &store,
            "--app",
            "discord",
            "--account",
            "account-0107",
            "--kind",
            "direct_message",
            "--stable-id",
            stable_id,
        ]))
        .expect("allowed-place command recognized");
        assert_eq!(add.exit_code, 0);
        let add_json = json(&add.stdout);
        assert_eq!(add_json["command"], "add");
        assert_eq!(add_json["ok"], true);
        assert_eq!(add_json["record"]["stableId"], stable_id);

        let allowed = run_allowed_place_cli(args(&[
            "osl-privacy-hub",
            "--allowed-place",
            "allowed",
            "--store",
            &store,
            "--app",
            "discord",
            "--account",
            "account-0107",
            "--kind",
            "direct_message",
            "--stable-id",
            stable_id,
        ]))
        .expect("allowed-place command recognized");
        assert_eq!(allowed.exit_code, 0);
        let allowed_json = json(&allowed.stdout);
        assert_eq!(allowed_json["command"], "allowed");
        assert_eq!(allowed_json["allowed"], true);

        let list = run_allowed_place_cli(args(&[
            "osl-privacy-hub",
            "--allowed-place",
            "list",
            "--store",
            &store,
        ]))
        .expect("allowed-place command recognized");
        assert_eq!(list.exit_code, 0);
        let list_json = json(&list.stdout);
        assert_eq!(list_json["command"], "list");
        assert_eq!(list_json["count"], 1);
        assert_eq!(list_json["records"][0]["stableId"], stable_id);

        let remove = run_allowed_place_cli(args(&[
            "osl-privacy-hub",
            "--allowed-place",
            "remove",
            "--store",
            &store,
            "--stable-id",
            stable_id,
        ]))
        .expect("allowed-place command recognized");
        assert_eq!(remove.exit_code, 0);
        let remove_json = json(&remove.stdout);
        assert_eq!(remove_json["command"], "remove");
        assert_eq!(remove_json["removed"], true);

        let denied = run_allowed_place_cli(args(&[
            "osl-privacy-hub",
            "--allowed-place",
            "allowed",
            "--store",
            &store,
            "--app",
            "discord",
            "--account",
            "account-0107",
            "--kind",
            "direct_message",
            "--stable-id",
            stable_id,
        ]))
        .expect("allowed-place command recognized");
        assert_eq!(denied.exit_code, 0);
        let denied_json = json(&denied.stdout);
        assert_eq!(denied_json["command"], "allowed");
        assert_eq!(denied_json["allowed"], false);
    }
}
