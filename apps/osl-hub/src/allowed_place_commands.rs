use crate::instagram_story::{
    inspect_instagram_story_publish_control, instagram_story_audience_stable_id,
    invoke_instagram_story_publish, InstagramStoryPublishControl, InstagramStoryPublishInput,
    InstagramStoryPublishReceipt,
};
use crate::security::{self, HubSecurityState};
use ipc::allowed_places::{AllowedPlaceQuery, AllowedPlaceRecord};
use serde::Serialize;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

pub const ALLOWED_PLACE_CLI_FLAG: &str = "--allowed-place";
pub const X_ALLOWED_TEXT_PERMISSIONS: &[&str] = &["placing"];
pub const X_SEND_PERMISSION: &str = "sending";
pub const X_SEND_CHECK_PERMISSION: &str = "checking a send";

const HEADLESS_ALLOWED_PLACE_FILE_KEY: [u8; 32] = [0xA7; 32];
static HEADLESS_ALLOWED_PLACE_LOCK: Mutex<()> = Mutex::new(());

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
    XPermissions {
        ok: bool,
        app: String,
        allowed: Vec<&'static str>,
    },
    XPermissionCheck {
        ok: bool,
        app: String,
        checked: Vec<&'static str>,
    },
    Tick {
        ok: bool,
        state: security::AllowedPlaceDirectionState,
    },
    Compare {
        ok: bool,
        #[serde(flatten)]
        direction: security::AllowedPlaceDirectionState,
    },
    InstagramStoryPublish {
        ok: bool,
        #[serde(flatten)]
        receipt: InstagramStoryPublishReceipt,
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
            exit_code: 1,
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
    with_headless_store(store_dir, |security| {
        let record = security::add_allowed_place_record(security, record)?;
        Ok(AllowedPlaceCommandJson::Add { ok: true, record })
    })
}

pub fn remove_allowed_place_json(
    store_dir: &Path,
    stable_id: String,
) -> Result<AllowedPlaceCommandJson, String> {
    with_headless_store(store_dir, |security| {
        let removed = security::remove_allowed_place_record(security, stable_id.clone())?;
        Ok(AllowedPlaceCommandJson::Remove {
            ok: true,
            stable_id,
            removed,
        })
    })
}

pub fn list_allowed_places_json(store_dir: &Path) -> Result<AllowedPlaceCommandJson, String> {
    with_headless_store(store_dir, |security| {
        let records = security::list_allowed_place_records(security)?;
        Ok(AllowedPlaceCommandJson::List {
            ok: true,
            count: records.len(),
            records,
        })
    })
}

pub fn allowed_place_allowed_json(
    store_dir: &Path,
    query: AllowedPlaceQuery,
) -> Result<AllowedPlaceCommandJson, String> {
    with_headless_store(store_dir, |security| {
        let allowed = security::query_allowed_place_allowed(security, query.clone())?;
        Ok(AllowedPlaceCommandJson::Allowed {
            ok: true,
            allowed,
            query,
        })
    })
}

pub fn allowed_place_tick_json(
    store_dir: &Path,
    app: String,
    kind: String,
    first_account: String,
    second_account: String,
) -> Result<AllowedPlaceCommandJson, String> {
    with_headless_store(store_dir, |security| {
        let state = security::compare_allowed_place_direction_state(
            security,
            app,
            kind,
            first_account,
            second_account,
        )?;
        Ok(AllowedPlaceCommandJson::Tick { ok: true, state })
    })
}

pub fn compare_allowed_place_json(
    store_dir: &Path,
    app: String,
    kind: String,
    first_account: String,
    second_account: String,
) -> Result<AllowedPlaceCommandJson, String> {
    with_headless_store(store_dir, |security| {
        let direction = security::compare_allowed_place_direction_state(
            security,
            app,
            kind,
            first_account,
            second_account,
        )?;
        Ok(AllowedPlaceCommandJson::Compare {
            ok: true,
            direction,
        })
    })
}

pub fn instagram_story_publish_control_json(
    store_dir: &Path,
    input: &InstagramStoryPublishInput,
) -> Result<InstagramStoryPublishControl, String> {
    with_headless_store(store_dir, |security_state| {
        inspect_instagram_story_publish_control(input, |account, member| {
            instagram_story_audience_member_is_allowed(security_state, account, member)
        })
    })
}

pub fn publish_instagram_story_json(
    store_dir: &Path,
    input: &InstagramStoryPublishInput,
) -> Result<AllowedPlaceCommandJson, String> {
    with_headless_store(store_dir, |security_state| {
        let receipt = invoke_instagram_story_publish(input, |account, member| {
            instagram_story_audience_member_is_allowed(security_state, account, member)
        })?;
        Ok(AllowedPlaceCommandJson::InstagramStoryPublish { ok: true, receipt })
    })
}

fn instagram_story_audience_member_is_allowed(
    security_state: &HubSecurityState,
    account: &str,
    member: &str,
) -> Result<bool, String> {
    security::query_allowed_place_allowed(
        security_state,
        AllowedPlaceQuery {
            app: "instagram".to_owned(),
            account: account.to_owned(),
            kind: crate::instagram_story::INSTAGRAM_STORY_ALLOWED_PLACE_KIND.to_owned(),
            stable_id: instagram_story_audience_stable_id(account, member),
        },
    )
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
        "x-permissions" => x_permissions_json(),
        "x-send" => x_send_json(),
        "x-permission-check" => x_permission_check_json(),
        "tick" => allowed_place_tick_json(
            &parsed.store,
            parsed.required("app")?,
            parsed.required("kind")?,
            parsed.required("first-account")?,
            parsed.required("second-account")?,
        ),
        "compare" => compare_allowed_place_json(
            &parsed.store,
            parsed.required("app")?,
            parsed.required("kind")?,
            parsed.required("first-account")?,
            parsed.required("second-account")?,
        ),
        "instagram-story-publish" => {
            let input = parsed.instagram_story_input()?;
            publish_instagram_story_json(&parsed.store, &input)
        },
        _ => Err(
            "usage: --allowed-place <add|remove|list|allowed|x-permissions|x-send|x-permission-check|tick|compare|instagram-story-publish> --store <dir> [--app <app> --account <account> --kind <kind> --stable-id <stable-id> --first-account <account> --second-account <account> --audience <member,...> --published-at <seconds> --osl-expires-at <seconds> --effective-expires-at <seconds>]"
                .to_owned(),
        ),
    }
}

fn x_permissions_json() -> Result<AllowedPlaceCommandJson, String> {
    Ok(AllowedPlaceCommandJson::XPermissions {
        ok: true,
        app: "x".to_owned(),
        allowed: X_ALLOWED_TEXT_PERMISSIONS.to_vec(),
    })
}

fn x_send_json() -> Result<AllowedPlaceCommandJson, String> {
    Err(format!(
        "OSL X send refused: missing permission {X_SEND_PERMISSION}"
    ))
}

fn x_permission_check_json() -> Result<AllowedPlaceCommandJson, String> {
    x_permission_check_for(X_ALLOWED_TEXT_PERMISSIONS)?;
    Ok(AllowedPlaceCommandJson::XPermissionCheck {
        ok: true,
        app: "x".to_owned(),
        checked: X_ALLOWED_TEXT_PERMISSIONS.to_vec(),
    })
}

fn x_permission_check_for(permissions: &[&str]) -> Result<(), String> {
    if permissions.contains(&X_SEND_PERMISSION) || permissions.contains(&X_SEND_CHECK_PERMISSION) {
        return Err(format!(
            "OSL X permission guard failed: X allowed permissions must not include {X_SEND_PERMISSION} or {X_SEND_CHECK_PERMISSION} until a real X send has been watched"
        ));
    }
    Ok(())
}

fn with_headless_store<T>(
    store_dir: &Path,
    f: impl FnOnce(&HubSecurityState) -> Result<T, String>,
) -> Result<T, String> {
    std::fs::create_dir_all(store_dir)
        .map_err(|error| format!("OSL allowed-place storage is unavailable: {error}"))?;
    let _guard = HeadlessStoreGuard::new(store_dir.to_path_buf())?;
    f(&HubSecurityState::default())
}

struct HeadlessStoreGuard {
    previous_active_account_dir: Option<PathBuf>,
    previous_file_key: Option<[u8; 32]>,
    _serial: MutexGuard<'static, ()>,
}

impl HeadlessStoreGuard {
    fn new(store_dir: PathBuf) -> Result<Self, String> {
        let serial = HEADLESS_ALLOWED_PLACE_LOCK
            .lock()
            .map_err(|_| "OSL allowed-place command state is unavailable".to_owned())?;
        let previous_active_account_dir = keystore::active_account_dir();
        let previous_file_key = ipc::main_password::get_file_storage_key();
        keystore::set_active_account_dir(Some(store_dir));
        ipc::main_password::set_file_storage_key(Some(HEADLESS_ALLOWED_PLACE_FILE_KEY));
        Ok(Self {
            previous_active_account_dir,
            previous_file_key,
            _serial: serial,
        })
    }
}

impl Drop for HeadlessStoreGuard {
    fn drop(&mut self) {
        keystore::set_active_account_dir(self.previous_active_account_dir.clone());
        ipc::main_password::set_file_storage_key(self.previous_file_key);
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
        Ok(AllowedPlaceRecord::from_parts(
            self.required("app")?,
            self.required("account")?,
            self.required("kind")?,
            self.required("stable-id")?,
        ))
    }

    fn query(&self) -> Result<AllowedPlaceQuery, String> {
        Ok(AllowedPlaceQuery {
            app: self.required("app")?,
            account: self.required("account")?,
            kind: self.required("kind")?,
            stable_id: self.required("stable-id")?,
        })
    }

    fn instagram_story_input(&self) -> Result<InstagramStoryPublishInput, String> {
        let audience = self.required("audience")?;
        let selected_audience = audience.split(',').map(str::to_owned).collect::<Vec<_>>();
        Ok(InstagramStoryPublishInput {
            account: self.required("account")?,
            selected_audience,
            published_at: self.required_i64("published-at")?,
            osl_expires_at: self.required_i64("osl-expires-at")?,
            presented_effective_expires_at: Some(self.required_i64("effective-expires-at")?),
        })
    }

    fn required_i64(&self, key: &str) -> Result<i64, String> {
        self.required(key)?
            .parse::<i64>()
            .map_err(|_| format!("invalid --{key}"))
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

    const INSTAGRAM_KINDS: [&str; 3] = ["direct_message", "group_chat", "channel"];
    const X_KINDS: [&str; 4] = [
        "direct_message",
        "public_post",
        "group_direct_message",
        "reply",
    ];

    fn args(items: &[&str]) -> Vec<OsString> {
        items.iter().map(OsString::from).collect()
    }

    fn json(stdout: &str) -> Value {
        serde_json::from_str(stdout.trim_end()).expect("stdout is one JSON object")
    }

    #[test]
    fn allowed_place_headless_commands_return_json_without_gui_setup() {
        let _serial = crate::global_keystore_test_lock();
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

        println!(
            "TASK0107 headless_allowed_place_json add={} allowed={} list_count={} remove={} allowed_after_remove={} window_opened=false",
            add_json["record"]["stableId"].as_str().unwrap_or(""),
            allowed_json["allowed"].as_bool().unwrap_or(false),
            list_json["count"].as_u64().unwrap_or(0),
            remove_json["removed"].as_bool().unwrap_or(false),
            denied_json["allowed"].as_bool().unwrap_or(true)
        );
    }

    #[test]
    fn instagram_allowed_place_fixtures_accept_three_kinds_and_reject_server() {
        let _serial = crate::global_keystore_test_lock();
        let dir = TempDir::new().unwrap();
        let store = dir.path().to_string_lossy();
        let mut resolved = Vec::new();

        for kind in INSTAGRAM_KINDS {
            let stable_id = format!("instagram:account-0161:{kind}:place-0161-{kind}");
            let add = run_allowed_place_cli(args(&[
                "osl-privacy-hub",
                "--allowed-place",
                "add",
                "--store",
                &store,
                "--app",
                "instagram",
                "--account",
                "account-0161",
                "--kind",
                kind,
                "--stable-id",
                &stable_id,
            ]))
            .expect("allowed-place command recognized");
            assert_eq!(add.exit_code, 0);

            let allowed = run_allowed_place_cli(args(&[
                "osl-privacy-hub",
                "--allowed-place",
                "allowed",
                "--store",
                &store,
                "--app",
                "instagram",
                "--account",
                "account-0161",
                "--kind",
                kind,
                "--stable-id",
                &stable_id,
            ]))
            .expect("allowed-place command recognized");
            assert_eq!(allowed.exit_code, 0);
            let allowed_json = json(&allowed.stdout);
            assert_eq!(allowed_json["allowed"], true);
            resolved.push(kind);
        }

        let rejected = run_allowed_place_cli(args(&[
            "osl-privacy-hub",
            "--allowed-place",
            "add",
            "--store",
            &store,
            "--app",
            "instagram",
            "--account",
            "account-0161",
            "--kind",
            "server",
            "--stable-id",
            "instagram:account-0161:server:place-0161-server",
        ]))
        .expect("allowed-place command recognized");
        assert_eq!(rejected.exit_code, 1);
        let rejected_json = json(&rejected.stdout);
        assert_eq!(rejected_json["ok"], false);
        assert_eq!(
            rejected_json["error"],
            "OSL Instagram allowed-place kind is invalid"
        );

        println!(
            "TASK0161 instagram_allowed_place_kinds created={} resolved={} kinds={} rejected_kind=server rejected_exit_code={}",
            INSTAGRAM_KINDS.len(),
            resolved.len(),
            resolved.join(","),
            rejected.exit_code
        );
    }

    #[test]
    fn task_3743_x_allowed_place_fixtures_accept_four_kinds_and_reject_invented() {
        let _serial = crate::global_keystore_test_lock();
        let dir = TempDir::new().unwrap();
        let store = dir.path().to_string_lossy();
        let mut resolved = Vec::new();

        for kind in X_KINDS {
            let stable_id = format!("x:account-3743:{kind}:place-3743-{kind}");
            let add = run_allowed_place_cli(args(&[
                "osl-privacy-hub",
                "--allowed-place",
                "add",
                "--store",
                &store,
                "--app",
                "x",
                "--account",
                "account-3743",
                "--kind",
                kind,
                "--stable-id",
                &stable_id,
            ]))
            .expect("allowed-place command recognized");
            assert_eq!(add.exit_code, 0);

            let allowed = run_allowed_place_cli(args(&[
                "osl-privacy-hub",
                "--allowed-place",
                "allowed",
                "--store",
                &store,
                "--app",
                "x",
                "--account",
                "account-3743",
                "--kind",
                kind,
                "--stable-id",
                &stable_id,
            ]))
            .expect("allowed-place command recognized");
            assert_eq!(allowed.exit_code, 0);
            let allowed_json = json(&allowed.stdout);
            assert_eq!(allowed_json["allowed"], true);
            resolved.push(kind);
        }

        let rejected = run_allowed_place_cli(args(&[
            "osl-privacy-hub",
            "--allowed-place",
            "add",
            "--store",
            &store,
            "--app",
            "x",
            "--account",
            "account-3743",
            "--kind",
            "invented_kind",
            "--stable-id",
            "x:account-3743:invented_kind:place-3743-invented-kind",
        ]))
        .expect("allowed-place command recognized");
        assert_eq!(rejected.exit_code, 1);
        let rejected_json = json(&rejected.stdout);
        assert_eq!(rejected_json["ok"], false);
        assert_eq!(
            rejected_json["error"],
            "OSL X allowed-place kind is invalid"
        );

        println!(
            "TASK3743 x_allowed_place_kinds created={} resolved={} kinds={} rejected_kind=invented_kind rejected_exit_code={}",
            X_KINDS.len(),
            resolved.len(),
            resolved.join(","),
            rejected.exit_code
        );
        for (task_id, kind) in [("1125", "group_direct_message"), ("1127", "reply")] {
            let result = if resolved.contains(&kind) {
                "allowed"
            } else {
                "refusal"
            };
            println!("TASK3743_TASK_{task_id}_KIND={kind} RESULT={result}");
            assert_eq!(result, "allowed");
        }
    }
}

pub struct AllowedPlaceCliResult {
    pub stdout: String,
    pub exit_code: i32,
}
