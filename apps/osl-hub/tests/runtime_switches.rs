#![cfg(feature = "core")]

use osl_privacy_hub::runtime_switches::{
    resolve_test_only_runtime_switches, test_only_runtime_switch_list,
    validate_runtime_switch_list_name, RunTimeSwitchError, PASSWORD_SCREEN_ACCESS_REQUIRED,
    PASSWORD_SCREEN_ACCESS_SKIP_FOR_TEST, PASSWORD_SCREEN_ACCESS_SWITCH,
    SAFE_SENDING_DRY_RUN_FOR_TEST, SAFE_SENDING_LIVE_AUTHORITY_REQUIRED, SAFE_SENDING_SWITCH,
    TEST_ONLY_RUNTIME_SWITCH_LIST_NAME,
};

#[test]
fn one_named_test_only_runtime_switch_list_has_defaults_and_allowed_values() {
    let list = test_only_runtime_switch_list();
    assert_eq!(list.name, TEST_ONLY_RUNTIME_SWITCH_LIST_NAME);
    assert_eq!(
        list.switches.len(),
        2,
        "password-screen access and safe sending must be the named list"
    );

    let names: Vec<&str> = list.switches.iter().map(|switch| switch.name).collect();
    assert_eq!(names, [PASSWORD_SCREEN_ACCESS_SWITCH, SAFE_SENDING_SWITCH]);

    for switch in list.switches {
        assert!(!switch.default_value.is_empty());
        assert!(
            switch.allowed_values.contains(&switch.default_value),
            "{} default must be one of its allowed values",
            switch.name
        );
        assert!(
            !switch.allowed_values.is_empty(),
            "{} must declare allowed values",
            switch.name
        );
    }

    assert_eq!(
        list.switches[0].allowed_values,
        [
            PASSWORD_SCREEN_ACCESS_REQUIRED,
            PASSWORD_SCREEN_ACCESS_SKIP_FOR_TEST
        ]
    );
    assert_eq!(
        list.switches[1].allowed_values,
        [
            SAFE_SENDING_LIVE_AUTHORITY_REQUIRED,
            SAFE_SENDING_DRY_RUN_FOR_TEST
        ]
    );

    let defaults = resolve_test_only_runtime_switches(&[]).expect("defaults resolve");
    assert_eq!(
        defaults.password_screen_access,
        PASSWORD_SCREEN_ACCESS_REQUIRED
    );
    assert_eq!(defaults.safe_sending, SAFE_SENDING_LIVE_AUTHORITY_REQUIRED);

    println!("RUN-TIME SWITCH LIST: {}", list.name);
    println!("SWITCH COUNT: {}", list.switches.len());
    for switch in list.switches {
        println!(
            "{} default={} allowed={} behavior={}",
            switch.name,
            switch.default_value,
            switch.allowed_values.join("|"),
            switch.behavior
        );
    }
    println!(
        "DEFAULT {}={}",
        PASSWORD_SCREEN_ACCESS_SWITCH, defaults.password_screen_access
    );
    println!("DEFAULT {}={}", SAFE_SENDING_SWITCH, defaults.safe_sending);
}

#[test]
fn allowed_runtime_switch_overrides_resolve_and_unknown_values_are_rejected() {
    validate_runtime_switch_list_name(TEST_ONLY_RUNTIME_SWITCH_LIST_NAME)
        .expect("named list is accepted");

    let switched = resolve_test_only_runtime_switches(&[
        (
            PASSWORD_SCREEN_ACCESS_SWITCH,
            PASSWORD_SCREEN_ACCESS_SKIP_FOR_TEST,
        ),
        (SAFE_SENDING_SWITCH, SAFE_SENDING_DRY_RUN_FOR_TEST),
    ])
    .expect("allowed test-only overrides resolve");
    assert_eq!(
        switched.password_screen_access,
        PASSWORD_SCREEN_ACCESS_SKIP_FOR_TEST
    );
    assert_eq!(switched.safe_sending, SAFE_SENDING_DRY_RUN_FOR_TEST);
    println!(
        "ALLOWED OVERRIDE {}={}",
        PASSWORD_SCREEN_ACCESS_SWITCH, switched.password_screen_access
    );
    println!(
        "ALLOWED OVERRIDE {}={}",
        SAFE_SENDING_SWITCH, switched.safe_sending
    );

    let rejected = resolve_test_only_runtime_switches(&[(SAFE_SENDING_SWITCH, "unsafe-send")])
        .expect_err("unknown safe-sending value must be rejected");
    assert!(matches!(rejected, RunTimeSwitchError::UnknownValue { .. }));
    let rejected = rejected.to_string();
    assert!(rejected.contains("unknown value \"unsafe-send\""));
    assert!(rejected.contains(SAFE_SENDING_SWITCH));
    assert!(rejected.contains(TEST_ONLY_RUNTIME_SWITCH_LIST_NAME));
    println!("UNKNOWN VALUE REJECTED: {rejected}");
}
