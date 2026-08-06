use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use osl_privacy_hub::core_bridge::{readiness, HubCoreState};
use osl_privacy_hub::hub_command_surface::with_native_discord_product_send_authority_for_switch;
use osl_privacy_hub::native_discord_adapter::NativeDiscordComposerState;
use osl_privacy_hub::runtime_switches::{
    old_test_only_build_choice_reports, read_startup_test_only_runtime_switches_from_assignments,
    PasswordScreenAccess, SafeSending, TEST_ONLY_RUNTIME_SWITCH_LIST,
};

const PASSWORD: &str = "TASK0014-password";

static SERIAL: Mutex<()> = Mutex::new(());

fn serialize() -> MutexGuard<'static, ()> {
    SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct Profile {
    _temp: tempfile::TempDir,
    base: PathBuf,
}

impl Profile {
    fn password_protected() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let base = temp.path().join("osl-core");
        std::fs::create_dir_all(&base).expect("base dir");
        keystore::set_base_dir_override(Some(base.clone()));
        keystore::set_active_account_dir(None);
        ipc::main_password::set_main_password(&base, PASSWORD).expect("set main password");
        ipc::main_password::set_file_storage_key(None);
        Self { _temp: temp, base }
    }
}

impl Drop for Profile {
    fn drop(&mut self) {
        let _ = &self.base;
        ipc::main_password::set_file_storage_key(None);
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
    }
}

fn switched_startup_set() -> osl_privacy_hub::runtime_switches::ResolvedTestOnlyRunTimeSwitches {
    read_startup_test_only_runtime_switches_from_assignments([
        "password_screen_access=skip-password-screen-for-test",
        "safe_sending=dry-run-send-for-test",
    ])
    .expect("startup switch reader resolves test-only choices")
}

#[test]
fn startup_switch_set_reports_each_old_test_only_choice() {
    let switches = switched_startup_set();
    assert_eq!(
        switches.password_screen_access,
        PasswordScreenAccess::SkipPasswordScreenForTest
    );
    assert_eq!(switches.safe_sending, SafeSending::DryRunSendForTest);

    println!("STARTUP SWITCH LIST: {TEST_ONLY_RUNTIME_SWITCH_LIST}");
    let reports = old_test_only_build_choice_reports(switches);
    for report in &reports {
        println!(
            "STARTUP OLD CHOICE {}={} source={}",
            report.name, report.value, report.source
        );
    }
    assert_eq!(reports.len(), 2);
    assert_eq!(reports[0].name, "password_screen_access");
    assert_eq!(reports[0].value, "skip-password-screen-for-test");
    assert_eq!(reports[0].source, "run-time switches");
    assert_eq!(reports[1].name, "safe_sending");
    assert_eq!(reports[1].value, "dry-run-send-for-test");
    assert_eq!(reports[1].source, "run-time switches");
}

#[test]
fn password_screen_startup_choice_is_read_from_runtime_switches() {
    let _serial = serialize();
    let _profile = Profile::password_protected();

    let default_state = HubCoreState::with_startup_switches(Default::default());
    let default_readiness = readiness(&default_state);
    println!(
        "PASSWORD SCREEN DEFAULT CHOICE: password_gate_required={} unlocked={}",
        default_readiness.password_gate_required, default_readiness.unlocked
    );
    assert!(default_readiness.password_gate_required);
    assert!(!default_readiness.unlocked);

    let switched_state = HubCoreState::with_startup_switches(switched_startup_set());
    let switched_readiness = readiness(&switched_state);
    println!(
        "PASSWORD SCREEN RUNTIME CHOICE: password_screen_access={} password_gate_required={} unlocked={}",
        switched_state.startup_switches().password_screen_access.as_str(),
        switched_readiness.password_gate_required,
        switched_readiness.unlocked
    );
    assert!(!switched_readiness.password_gate_required);
    assert!(switched_readiness.unlocked);
}

#[test]
fn safe_sending_startup_choice_is_read_from_runtime_switches() {
    let composer = NativeDiscordComposerState::default();
    let live = with_native_discord_product_send_authority_for_switch(
        &composer,
        "scope-a",
        None,
        SafeSending::LiveSendRequiresAuthority,
        |authority| Ok(authority.carrier),
    )
    .expect_err("live sending requires a prepared carrier authority");
    let refused_missing_authority =
        live == "The protected message is not ready to send; nothing was placed";
    println!(
        "SAFE SENDING DEFAULT CHOICE: live-send-requires-authority refused_missing_authority={}",
        refused_missing_authority
    );
    assert!(refused_missing_authority);

    let switches = switched_startup_set();
    let dry_run = with_native_discord_product_send_authority_for_switch(
        &composer,
        "scope-a",
        None,
        switches.safe_sending,
        |authority| Ok(authority.carrier),
    )
    .expect("dry-run switch bypasses live carrier posting");
    println!(
        "SAFE SENDING RUNTIME CHOICE: safe_sending={} result=dry-run carrier={}",
        switches.safe_sending.as_str(),
        dry_run
    );
    assert_eq!(dry_run, "TASK0014 dry-run carrier");
}

#[test]
fn shipping_startup_reads_switches_before_managing_core_state() {
    let source =
        std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"))
            .expect("read desktop startup");
    let reader = source
        .find("read_startup_test_only_runtime_switches()")
        .expect("desktop startup reads test-only runtime switches");
    let setup = source
        .find("builder.setup(|app|")
        .expect("desktop startup setup closure exists");
    let bootstrap = source
        .find("HubCoreState::bootstrap_from_disk_with_startup_switches(startup_switches)")
        .expect("core state is bootstrapped with the startup switch set");
    assert!(
        reader < setup,
        "startup switch reader must run before setup"
    );
    assert!(
        setup < bootstrap,
        "setup must manage the startup-resolved core state"
    );
    println!("STARTUP CHECK: switch reader before setup and core bootstrap");
}
