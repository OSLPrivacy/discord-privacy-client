//! TASK 4860: every enclave permission goes through one ordered resolver.

#[allow(dead_code)]
#[path = "../../src/osl_enclave_roles.rs"]
mod osl_enclave_roles;

use std::collections::BTreeSet;

use osl_enclave_roles::{
    new_enclave_role_catalog, EnclavePermissionDecision, EnclavePermissionDecisionRule,
    EnclavePermissionResolution, EnclavePermissionResolveInput, MEMBER_ROLE_NAME, OWNER_ROLE_NAME,
};

#[derive(Debug)]
struct Fixture<'a> {
    name: &'a str,
    permission_name: &'a str,
    key_present: bool,
    relay_sanctioned: bool,
    channel_allow_permission_names: BTreeSet<String>,
    channel_deny_permission_names: BTreeSet<String>,
    role_name: &'a str,
    expected_decision: EnclavePermissionDecision,
    expected_rule: EnclavePermissionDecisionRule,
    expected_denied_by: Option<&'static str>,
}

impl<'a> Fixture<'a> {
    fn input(
        &'a self,
        catalog: &'a osl_enclave_roles::EnclaveRoleCatalog,
    ) -> EnclavePermissionResolveInput<'a> {
        EnclavePermissionResolveInput {
            permission_name: self.permission_name,
            key_present: self.key_present,
            relay_sanctioned: self.relay_sanctioned,
            channel_allow_permission_names: &self.channel_allow_permission_names,
            channel_deny_permission_names: &self.channel_deny_permission_names,
            role_name: self.role_name,
            role_catalog: catalog,
        }
    }
}

fn set(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

fn print_fixture(name: &str, resolution: &EnclavePermissionResolution) {
    let decision = if resolution.allowed() {
        "allowed"
    } else {
        "denied"
    };
    let denied_by = resolution.denied_by().unwrap_or("NONE");
    println!(
        "TASK4860 fixture={name} decision={decision} decided_by={} denied_by={denied_by}",
        resolution.decided_by()
    );
}

#[test]
fn one_resolver_answers_six_named_fixtures_in_precedence_order() {
    let catalog = new_enclave_role_catalog();
    let fixtures = vec![
        Fixture {
            name: "missing_key_denies_key",
            permission_name: "missing_fixture_permission",
            key_present: false,
            relay_sanctioned: true,
            channel_allow_permission_names: set(&["missing_fixture_permission"]),
            channel_deny_permission_names: set(&["missing_fixture_permission"]),
            role_name: OWNER_ROLE_NAME,
            expected_decision: EnclavePermissionDecision::Denied,
            expected_rule: EnclavePermissionDecisionRule::Key,
            expected_denied_by: Some("KEY"),
        },
        Fixture {
            name: "relay_sanction_denies_relay",
            permission_name: "send_messages",
            key_present: true,
            relay_sanctioned: true,
            channel_allow_permission_names: set(&["send_messages"]),
            channel_deny_permission_names: set(&["send_messages"]),
            role_name: OWNER_ROLE_NAME,
            expected_decision: EnclavePermissionDecision::Denied,
            expected_rule: EnclavePermissionDecisionRule::Relay,
            expected_denied_by: Some("RELAY"),
        },
        Fixture {
            name: "channel_deny_with_channel_allow",
            permission_name: "send_messages",
            key_present: true,
            relay_sanctioned: false,
            channel_allow_permission_names: set(&["send_messages"]),
            channel_deny_permission_names: set(&["send_messages"]),
            role_name: OWNER_ROLE_NAME,
            expected_decision: EnclavePermissionDecision::Denied,
            expected_rule: EnclavePermissionDecisionRule::ChannelDeny,
            expected_denied_by: Some("CHANNEL_DENY"),
        },
        Fixture {
            name: "channel_allow_beats_role_grants",
            permission_name: "manage_roles",
            key_present: true,
            relay_sanctioned: false,
            channel_allow_permission_names: set(&["manage_roles"]),
            channel_deny_permission_names: BTreeSet::new(),
            role_name: MEMBER_ROLE_NAME,
            expected_decision: EnclavePermissionDecision::Allowed,
            expected_rule: EnclavePermissionDecisionRule::ChannelAllow,
            expected_denied_by: None,
        },
        Fixture {
            name: "role_grants_answer",
            permission_name: "read_messages",
            key_present: true,
            relay_sanctioned: false,
            channel_allow_permission_names: BTreeSet::new(),
            channel_deny_permission_names: BTreeSet::new(),
            role_name: MEMBER_ROLE_NAME,
            expected_decision: EnclavePermissionDecision::Allowed,
            expected_rule: EnclavePermissionDecisionRule::RoleGrants,
            expected_denied_by: None,
        },
        Fixture {
            name: "otherwise_off",
            permission_name: "manage_roles",
            key_present: true,
            relay_sanctioned: false,
            channel_allow_permission_names: BTreeSet::new(),
            channel_deny_permission_names: BTreeSet::new(),
            role_name: MEMBER_ROLE_NAME,
            expected_decision: EnclavePermissionDecision::Denied,
            expected_rule: EnclavePermissionDecisionRule::Off,
            expected_denied_by: Some("OFF"),
        },
    ];

    let mut names_in_order = Vec::new();
    for fixture in &fixtures {
        let resolution = osl_enclave_roles::resolve_enclave_permission(fixture.input(&catalog));
        print_fixture(fixture.name, &resolution);
        assert_eq!(
            resolution.decision, fixture.expected_decision,
            "{fixture:?}"
        );
        assert_eq!(resolution.rule, fixture.expected_rule, "{fixture:?}");
        assert_eq!(
            resolution.denied_by(),
            fixture.expected_denied_by,
            "{fixture:?}"
        );
        names_in_order.push(fixture.name);
    }

    assert_eq!(
        names_in_order,
        vec![
            "missing_key_denies_key",
            "relay_sanction_denies_relay",
            "channel_deny_with_channel_allow",
            "channel_allow_beats_role_grants",
            "role_grants_answer",
            "otherwise_off"
        ]
    );
}
