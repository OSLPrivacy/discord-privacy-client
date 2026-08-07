//! TASK 4855: OWNER, MOD and MEMBER are defaults only.

#[path = "../../src/osl_enclave_roles.rs"]
mod osl_enclave_roles;

use osl_enclave_roles::{
    all_persisted_permission_names, enclave_permissions, new_enclave_role_catalog,
    persisted_permission_name_search,
};

#[test]
fn task_4855_owner_mod_member_are_defaults_only_without_admin_bypass() {
    let mut catalog = new_enclave_role_catalog();
    let default_names = catalog.role_names();
    println!(
        "TASK4855_DEFAULT_ROLES count={} names={}",
        default_names.len(),
        default_names.join(",")
    );
    assert_eq!(default_names, ["OWNER", "MOD", "MEMBER"]);
    assert_eq!(catalog.roles().len(), 3);

    catalog
        .add_custom_role("FOURTH", ["read_messages".to_owned()])
        .expect("a person can create a fourth role");
    let fourth_names = catalog.role_names();
    println!(
        "TASK4855_FOURTH_ROLE count={} names={}",
        fourth_names.len(),
        fourth_names.join(",")
    );
    assert_eq!(fourth_names, ["OWNER", "MOD", "MEMBER", "FOURTH"]);

    let all_permission_names = all_persisted_permission_names();
    assert_eq!(enclave_permissions().len(), 40);
    let all_ticked = catalog
        .add_custom_role("ALL_TICKED", all_permission_names)
        .expect("a custom role may tick every permission box")
        .clone();
    println!(
        "TASK4855_ALL_PERMISSION_ROLE role_name={} permission_count={} administrator_flags={}",
        all_ticked.name,
        all_ticked.ticked_permission_count(),
        all_ticked.administrator_flag_count()
    );
    assert_eq!(all_ticked.ticked_permission_count(), 40);
    assert_eq!(all_ticked.administrator_flag_count(), 0);

    let matches = persisted_permission_name_search(&["admin", "administrator", "bypass_overrides"]);
    println!(
        "TASK4855_PERSISTED_PERMISSION_NAME_SEARCH needles=admin,administrator,bypass_overrides matches={}",
        matches.len()
    );
    assert!(matches.is_empty());
}
