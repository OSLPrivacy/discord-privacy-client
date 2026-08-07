//! TASK 4856: future permissions are off for existing roles until explicitly ticked.

#[allow(dead_code)]
#[path = "../../src/osl_enclave_roles.rs"]
mod osl_enclave_roles;

use std::collections::BTreeMap;

use osl_enclave_roles::{
    all_persisted_permission_names, new_enclave_role_catalog,
    new_enclave_role_catalog_from_template_values, MEMBER_ROLE_NAME, MOD_ROLE_NAME,
    OWNER_ROLE_NAME,
};

const FUTURE_PERMISSION_NAME: &str = "test_future_power";
const FUTURE_PERMISSION_LABEL: &str = "test future power";

#[test]
fn task_4856_future_permission_stays_off_for_existing_roles() {
    let mut existing = new_enclave_role_catalog();
    existing
        .add_custom_role("CUSTOM_ALPHA", ["read_messages".to_owned()])
        .expect("fixture custom role can tick one old permission");
    existing
        .add_custom_role("CUSTOM_ALL_OLD", all_persisted_permission_names())
        .expect("fixture custom role can tick every old permission");

    let previous_permission_names = all_persisted_permission_names();
    let mut current_permission_names = previous_permission_names.clone();
    current_permission_names.insert(FUTURE_PERMISSION_NAME.to_owned());
    let report = existing.migrate_existing_roles_for_introduced_permissions(
        &previous_permission_names,
        &current_permission_names,
    );
    existing
        .require_introduced_permissions_off(&report.introduced_permission_names)
        .expect("migration keeps introduced permissions off");

    let existing_states = role_future_permission_states(&existing);
    let off_count = existing_states
        .values()
        .filter(|state| **state == "off")
        .count();
    println!(
        "TASK4856_EXISTING_ROLE_READS permission_label=\"{}\" role_count={} off_count={} states={}",
        FUTURE_PERMISSION_LABEL,
        report.existing_role_count,
        off_count,
        render_states(&existing_states)
    );
    assert_eq!(existing_states.len(), 5);
    assert_eq!(off_count, 5);
    assert_eq!(
        render_states(&existing_states),
        "CUSTOM_ALL_OLD:off,CUSTOM_ALPHA:off,MEMBER:off,MOD:off,OWNER:off"
    );

    let mod_defaults = existing
        .role(MOD_ROLE_NAME)
        .expect("MOD exists")
        .ticked_permission_names
        .clone();
    let member_defaults = existing
        .role(MEMBER_ROLE_NAME)
        .expect("MEMBER exists")
        .ticked_permission_names
        .clone();
    let new_enclave = new_enclave_role_catalog_from_template_values(
        current_permission_names,
        mod_defaults,
        member_defaults,
    )
    .expect("new enclave template accepts the future permission");
    let new_states = role_future_permission_states(&new_enclave);
    println!(
        "TASK4856_NEW_ENCLAVE_DEFAULTS permission_label=\"{}\" states={}",
        FUTURE_PERMISSION_LABEL,
        render_states(&new_states)
    );
    assert_eq!(render_states(&new_states), "MEMBER:off,MOD:off,OWNER:on");

    let mut bad_existing = existing.clone();
    bad_existing
        .set_permission_for_role(OWNER_ROLE_NAME, FUTURE_PERMISSION_NAME, true)
        .expect("OWNER exists");
    let error = bad_existing
        .require_introduced_permissions_off(&report.introduced_permission_names)
        .expect_err("migration check must fail if OWNER absorbs the future permission");
    println!("TASK4856_BAD_OWNER_CHECK error=\"{error}\"");
    assert!(error.contains("OWNER"));
}

fn role_future_permission_states(
    catalog: &osl_enclave_roles::EnclaveRoleCatalog,
) -> BTreeMap<&'static str, &'static str> {
    [
        OWNER_ROLE_NAME,
        MOD_ROLE_NAME,
        MEMBER_ROLE_NAME,
        "CUSTOM_ALPHA",
        "CUSTOM_ALL_OLD",
    ]
    .into_iter()
    .filter_map(|role_name| {
        catalog.role(role_name).map(|role| {
            (
                role_name,
                if role.permission_is_ticked(FUTURE_PERMISSION_NAME) {
                    "on"
                } else {
                    "off"
                },
            )
        })
    })
    .collect::<BTreeMap<_, _>>()
}

fn render_states(states: &BTreeMap<&'static str, &'static str>) -> String {
    states
        .iter()
        .map(|(role_name, state)| format!("{role_name}:{state}"))
        .collect::<Vec<_>>()
        .join(",")
}
