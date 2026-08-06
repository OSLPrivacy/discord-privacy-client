use ipc::commands::{
    cmd_osl_create_private_contact_link, cmd_osl_private_contact_link_status,
    cmd_osl_use_private_contact_link,
};
use ipc::state::AppState;
use std::path::Path;
use std::sync::Mutex;

static CONFIG_DIR_LOCK: Mutex<()> = Mutex::new(());

const PERSON_ID: &str = "person-0310";

struct ConfigDirGuard;

impl Drop for ConfigDirGuard {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(None);
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
    }
}

fn use_temp_config_dir(dir: &Path) -> ConfigDirGuard {
    ipc::main_password::set_file_storage_key(None);
    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(Some(dir.to_path_buf()));
    ConfigDirGuard
}

#[test]
fn two_private_contact_links_for_one_person_are_distinct_and_one_use_only() {
    let _lock = CONFIG_DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("tempdir");
    let _config_dir = use_temp_config_dir(dir.path());
    let state = AppState::new();

    let first = cmd_osl_create_private_contact_link(&state, PERSON_ID.to_string())
        .expect("create first private contact link");
    let second = cmd_osl_create_private_contact_link(&state, PERSON_ID.to_string())
        .expect("create second private contact link");

    assert_eq!(first.person_id, PERSON_ID);
    assert_eq!(second.person_id, PERSON_ID);
    assert_ne!(first.link_value, second.link_value);
    assert_eq!(first.uses_allowed, 1);
    assert_eq!(second.uses_allowed, 1);

    let first_status = cmd_osl_private_contact_link_status(&state, first.link_value.clone())
        .expect("first link status");
    let second_status = cmd_osl_private_contact_link_status(&state, second.link_value.clone())
        .expect("second link status");
    assert_eq!(first_status.uses_allowed, 1);
    assert_eq!(second_status.uses_allowed, 1);
    assert_eq!(first_status.uses_recorded, 0);
    assert_eq!(second_status.uses_recorded, 0);

    let first_use =
        cmd_osl_use_private_contact_link(&state, first.link_value.clone()).expect("first link use");
    let second_use = cmd_osl_use_private_contact_link(&state, second.link_value.clone())
        .expect("second link use");
    assert_eq!(first_use.uses_allowed, 1);
    assert_eq!(second_use.uses_allowed, 1);
    assert_eq!(first_use.uses_recorded, 1);
    assert_eq!(second_use.uses_recorded, 1);

    let first_reuse = cmd_osl_use_private_contact_link(&state, first.link_value.clone())
        .expect_err("first link second use must refuse");
    let first_after = cmd_osl_private_contact_link_status(&state, first.link_value.clone())
        .expect("first link status after refused reuse");
    let second_after = cmd_osl_private_contact_link_status(&state, second.link_value.clone())
        .expect("second link status after use");
    assert_eq!(first_reuse, "OSL: private contact link is already used");
    assert_eq!(first_after.uses_recorded, 1);
    assert_eq!(second_after.uses_recorded, 1);
    assert!(first_after.used);
    assert!(second_after.used);

    let first_json = serde_json::to_value(&first).expect("serialize first link");
    let second_json = serde_json::to_value(&second).expect("serialize second link");
    assert!(first_json.get("username").is_none());
    assert!(second_json.get("username").is_none());
    assert!(first_json.get("publicUsername").is_none());
    assert!(second_json.get("publicUsername").is_none());

    println!(
        "TASK_0310_PRIVATE_CONTACT_LINKS person_id={} first_link={} second_link={} links_different={} first_uses_allowed={} second_uses_allowed={} first_uses_recorded={} second_uses_recorded={} first_reuse_refused={} public_username_created=false",
        PERSON_ID,
        first.link_value,
        second.link_value,
        first.link_value != second.link_value,
        first_after.uses_allowed,
        second_after.uses_allowed,
        first_after.uses_recorded,
        second_after.uses_recorded,
        first_reuse
    );
}
