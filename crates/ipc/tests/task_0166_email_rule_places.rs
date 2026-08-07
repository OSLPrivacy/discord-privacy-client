use ipc::allowed_places::{allowed_places_db_path, AllowedPlaceRecord};
use ipc::commands::{
    cmd_osl_new_place, cmd_osl_read_auto_whitelist_rule, cmd_osl_save_auto_whitelist_rule,
};
use ipc::state::AppState;
use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;

static CONFIG_DIR_LOCK: Mutex<()> = Mutex::new(());

const ACCOUNT_ID: &str = "email-account-0166";
const FRIEND_ADDRESS: &str = "friend0166@example.test";
const DOMAIN: &str = "example.test";

struct ConfigDirGuard;

impl Drop for ConfigDirGuard {
    fn drop(&mut self) {
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
        ipc::main_password::set_file_storage_key(None);
    }
}

fn use_temp_config_dir(dir: &Path) -> ConfigDirGuard {
    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(Some(dir.to_path_buf()));
    ipc::main_password::set_file_storage_key(Some([0x66; 32]));
    ConfigDirGuard
}

fn email_address_place() -> AllowedPlaceRecord {
    AllowedPlaceRecord::email_address(ACCOUNT_ID, FRIEND_ADDRESS)
}

fn email_domain_place() -> AllowedPlaceRecord {
    AllowedPlaceRecord::email_domain(ACCOUNT_ID, DOMAIN)
}

fn allowed_place_rows(dir: &Path) -> Vec<(String, String, String)> {
    let path = allowed_places_db_path(dir);
    if !path.exists() {
        return Vec::new();
    }
    let conn = Connection::open(path).expect("open allowed places db");
    let mut stmt = conn
        .prepare("SELECT app, kind, stable_id FROM allowed_places ORDER BY stable_id")
        .expect("prepare allowed places query");
    stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .expect("query allowed places")
        .map(|row| row.expect("allowed place row"))
        .collect()
}

#[test]
fn email_address_and_domain_rule_lookups_use_distinct_allowed_place_records() {
    let _lock = CONFIG_DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let _config_dir = use_temp_config_dir(dir.path());
    let state = AppState::new();
    state
        .friend_ids
        .lock()
        .expect("friend_ids mutex poisoned")
        .push(FRIEND_ADDRESS.to_owned());

    cmd_osl_save_auto_whitelist_rule(
        &state,
        "email-address".to_owned(),
        "only if a friend".to_owned(),
        None,
    )
    .expect("save email-address rule");
    cmd_osl_save_auto_whitelist_rule(&state, "email-domain".to_owned(), "never".to_owned(), None)
        .expect("save email-domain rule");

    let address_lookup =
        cmd_osl_read_auto_whitelist_rule(&state, "email-address".to_owned()).unwrap();
    let domain_lookup =
        cmd_osl_read_auto_whitelist_rule(&state, "email-domain".to_owned()).unwrap();
    println!(
        "TASK0166 direct_lookup address={} domain={}",
        address_lookup.choice, domain_lookup.choice
    );
    assert_eq!(address_lookup.app_kind, "email_address");
    assert_eq!(domain_lookup.app_kind, "email_domain");
    assert_eq!(address_lookup.choice, "only if a friend");
    assert_eq!(domain_lookup.choice, "never");
    assert_ne!(address_lookup.choice, domain_lookup.choice);

    let address = cmd_osl_new_place(&state, email_address_place(), None).unwrap();
    let after_address = allowed_place_rows(dir.path());
    let domain = cmd_osl_new_place(&state, email_domain_place(), None).unwrap();
    let after_domain = allowed_place_rows(dir.path());

    println!(
        "TASK0166 place_lookup address_stable_id={} address_rule={} address_result={} domain_stable_id={} domain_rule={} domain_result={} rows_after_address={} rows_after_domain={}",
        address.stable_id,
        address.rule_choice,
        address.result,
        domain.stable_id,
        domain.rule_choice,
        domain.result,
        after_address.len(),
        after_domain.len()
    );
    println!("TASK0166 allowed_place_rows={after_domain:?}");

    assert_eq!(address.app_kind, "email_address");
    assert_eq!(address.rule_choice, "only if a friend");
    assert_eq!(address.result, "allowed");
    assert_eq!(after_address.len(), 1);
    assert_eq!(
        after_address[0],
        (
            "email".to_owned(),
            "email_address".to_owned(),
            format!("email:{ACCOUNT_ID}:email_address:{FRIEND_ADDRESS}")
        )
    );
    assert_eq!(domain.app_kind, "email_domain");
    assert_eq!(domain.rule_choice, "never");
    assert_eq!(domain.result, "unlisted");
    assert_eq!(after_domain, after_address);
}
