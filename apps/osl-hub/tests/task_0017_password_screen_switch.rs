use osl_privacy_hub::core_bridge::{self, HubCoreState};
use osl_privacy_hub::runtime_switches::read_startup_test_only_runtime_switches;

struct KeystoreReset;

impl Drop for KeystoreReset {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(None);
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
    }
}

fn onboarding_title_from_direct_state_command(state: &HubCoreState) -> &'static str {
    let readiness = core_bridge::readiness(state);
    if readiness.bootstrap_status == "setupRequired" && readiness.identity_loaded {
        "Create a password"
    } else {
        "skipped password screen"
    }
}

#[test]
fn password_screen_enabled_reports_create_password() {
    let _reset = KeystoreReset;
    let root = tempfile::TempDir::new().expect("create isolated OSL root");
    let base = root.path().join("osl-core");
    std::fs::create_dir_all(&base).expect("create isolated OSL base");
    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(Some(base));
    ipc::main_password::set_file_storage_key(None);

    let switches =
        read_startup_test_only_runtime_switches().expect("read task 0017 runtime switch");
    let state = HubCoreState::bootstrap_from_disk_with_runtime_switches(switches);
    {
        let mut identity = state.osl.identity.lock().expect("identity lock");
        *identity = Some(keystore::generate_identity("task-0017-owner".to_owned()));
    }

    let title = onboarding_title_from_direct_state_command(&state);
    println!(
        "TASK0017 direct_onboarding_state_command=get_core_readiness build_features=core,discord-qa-shell password_screen_access={} onboarding_title={} check_passed={}",
        switches.password_screen_access.as_str(),
        title,
        title == "Create a password"
    );

    assert_eq!(
        title, "Create a password",
        "turning password_screen_access off must make this check fail"
    );
}
