#![cfg(feature = "core")]

use osl_privacy_hub::core_bridge::{readiness, HubCoreState};
use osl_privacy_hub::hub_command_surface::with_native_discord_product_send_authority_for_switches;
use osl_privacy_hub::native_discord_adapter::{
    DiscordCarrierLayout, DiscordCarrierPadding, DiscordCarrierRowKind, NativeDiscordComposerState,
};
use osl_privacy_hub::runtime_switches::{
    read_startup_test_only_runtime_switches_from_assignments,
    test_only_runtime_switch_choice_report, PASSWORD_SCREEN_ACCESS_SKIP_FOR_TEST,
    PASSWORD_SCREEN_ACCESS_SWITCH, SAFE_SENDING_DRY_RUN_FOR_TEST, SAFE_SENDING_SWITCH,
    TEST_ONLY_RUNTIME_SWITCH_LIST_NAME,
};

const PASSWORD: &str = "aB3!z9-startup-switch";

struct KeystoreReset;

impl Drop for KeystoreReset {
    fn drop(&mut self) {
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

fn measured_layout() -> DiscordCarrierLayout {
    DiscordCarrierLayout {
        content_width_px: 240.0,
        average_grapheme_width_px: 8.0,
        line_height_px: 18.0,
        zoom: 1.0,
        density: 1.0,
        padding: DiscordCarrierPadding::ShapeMatched,
        row_kind: DiscordCarrierRowKind::PlainText,
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
    let switches = read_startup_test_only_runtime_switches_from_assignments([
        "password_screen_access=skip-password-screen-for-test",
        "safe_sending=dry-run-send-for-test",
    ])
    .expect("startup switch reader resolves the old test-only choices");
    let report = test_only_runtime_switch_choice_report(&switches);

    assert_eq!(
        report.password_screen_access,
        PASSWORD_SCREEN_ACCESS_SKIP_FOR_TEST
    );
    assert_eq!(report.safe_sending, SAFE_SENDING_DRY_RUN_FOR_TEST);
    assert_eq!(report.source, "run-time switches");

    println!("STARTUP SWITCH LIST: {TEST_ONLY_RUNTIME_SWITCH_LIST_NAME}");
    println!(
        "STARTUP OLD CHOICE {}={} source={}",
        PASSWORD_SCREEN_ACCESS_SWITCH, report.password_screen_access, report.source
    );
    println!(
        "STARTUP OLD CHOICE {}={} source={}",
        SAFE_SENDING_SWITCH, report.safe_sending, report.source
    );
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
    let _reset = KeystoreReset;
    let storage = tempfile::tempdir().expect("isolated runtime switch profile");
    let core_dir = storage.path().join("osl-core");
    std::fs::create_dir_all(&core_dir).expect("create isolated core dir");
    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(Some(core_dir.clone()));
    ipc::main_password::set_main_password(&core_dir, PASSWORD).expect("write password marker");
    ipc::main_password::set_file_storage_key(None);

    let default_switches =
        read_startup_test_only_runtime_switches_from_assignments([]).expect("defaults resolve");
    let default_state = HubCoreState::bootstrap_from_disk_with_runtime_switches(default_switches);
    let default_readiness = readiness(&default_state);
    assert!(default_readiness.password_gate_required);
    assert!(!default_readiness.unlocked);
    let _serial = serialize();
    let _profile = Profile::password_protected();

    let default_state = HubCoreState::with_startup_switches(Default::default());
    let default_readiness = readiness(&default_state);
    println!(
        "PASSWORD SCREEN DEFAULT CHOICE: password_gate_required={} unlocked={}",
        default_readiness.password_gate_required, default_readiness.unlocked
    );

    let switched = read_startup_test_only_runtime_switches_from_assignments([
        "password_screen_access=skip-password-screen-for-test",
    ])
    .expect("startup switch reader resolves password-screen override");
    let switched_state = HubCoreState::bootstrap_from_disk_with_runtime_switches(switched);
    let switched_readiness = readiness(&switched_state);
    assert!(!switched_readiness.password_gate_required);
    assert!(switched_readiness.unlocked);
    println!(
        "PASSWORD SCREEN RUNTIME CHOICE: {}={} password_gate_required={} unlocked={}",
        PASSWORD_SCREEN_ACCESS_SWITCH,
        PASSWORD_SCREEN_ACCESS_SKIP_FOR_TEST,
        switched_readiness.password_gate_required,
        switched_readiness.unlocked
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
    let default_switches =
        read_startup_test_only_runtime_switches_from_assignments([]).expect("defaults resolve");
    let live = with_native_discord_product_send_authority_for_switches(
        &default_switches,
        &composer,
        "scope-a",
        Some(measured_layout()),
        |_| Ok("live-posted"),
        || Ok("dry-run"),
    );
    assert!(live.is_err());
    println!(
        "SAFE SENDING DEFAULT CHOICE: live-send-requires-authority refused_missing_authority=true"
    );

    let dry_switches = read_startup_test_only_runtime_switches_from_assignments([
        "safe_sending=dry-run-send-for-test",
    ])
    .expect("startup switch reader resolves safe-sending override");
    let dry = with_native_discord_product_send_authority_for_switches(
        &dry_switches,
        &composer,
        "scope-a",
        Some(measured_layout()),
        |_| Ok("live-posted"),
        || Ok("dry-run"),
    )
    .expect("dry-run switch bypasses live carrier posting");
    assert_eq!(dry, "dry-run");
    println!(
        "SAFE SENDING RUNTIME CHOICE: {}={} result={}",
        SAFE_SENDING_SWITCH, SAFE_SENDING_DRY_RUN_FOR_TEST, dry
    );
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
    let source = include_str!("../src/main.rs");
    let startup_reader = source
        .find("read_startup_test_only_runtime_switches()")
        .expect("startup must read the runtime switch set");
    let setup = source
        .find("let builder = builder.setup(move |app| {")
        .expect("startup setup hook must exist");
    let core_bootstrap = source
        .find("HubCoreState::bootstrap_from_disk_with_runtime_switches(")
        .expect("startup must pass the resolved switch set into HubCoreState");
    let switch_argument = source[core_bootstrap..]
        .find("startup_runtime_switches.clone()")
        .map(|offset| core_bootstrap + offset)
        .expect("core bootstrap must receive the startup switch set");
    let legacy_cfg =
        source.find("let password_gate_required = if cfg!(feature = \"discord-qa-shell\")");

    assert!(startup_reader < setup);
    assert!(setup < core_bootstrap && core_bootstrap < switch_argument);
    assert!(
        legacy_cfg.is_none(),
        "password-screen startup check must fail if the old build-selection gate returns"
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
