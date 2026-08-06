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
    );
    println!("STARTUP CHECK: switch reader before setup and core bootstrap");
}
