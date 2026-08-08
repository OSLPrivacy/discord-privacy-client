//! TASK 4858: the ability view resolves the real grants and explains every no.

#[allow(dead_code)]
#[path = "../../src/osl_enclave_roles.rs"]
mod osl_enclave_roles;

#[allow(dead_code)]
#[path = "../../src/osl_enclave_role_ability.rs"]
mod osl_enclave_role_ability;

#[allow(dead_code)]
#[path = "../fixtures/task_4858_role_ability_fixture.rs"]
mod fixture;

use std::collections::BTreeSet;

use fixture::{
    fixture_ability_view, EXPECTED_ALLOWED_COUNT, EXPECTED_DENIED_COUNT, EXPECTED_ROW_COUNT,
};
use osl_enclave_role_ability::{
    ENCLAVE_ROLE_ABILITY_REASONS, REASON_CHANNEL_OVERRIDE_DENY, REASON_NO_CHANNEL_KEY,
    REASON_PERMISSION_OFF, REASON_RELAY_TIMEOUT_ACTIVE, REASON_ROLE_LIMIT_LOWER,
};
use osl_enclave_roles::enclave_permissions;

#[test]
fn the_fixture_role_shows_seventeen_allowed_and_twenty_three_denied_rows_with_reasons() {
    let view = fixture_ability_view();

    println!(
        "TASK4858_VIEW role={} channel=#{} rows={} allowed={} denied={}",
        view.role_name,
        view.channel_name,
        view.row_count(),
        view.allowed_count(),
        view.denied_count()
    );

    // The view covers the whole catalogue, one row per permission, in order.
    let catalogue_names: Vec<&str> = enclave_permissions()
        .iter()
        .map(|permission| permission.persisted_name)
        .collect();
    let view_names: Vec<&str> = view.rows.iter().map(|row| row.permission_name).collect();
    assert_eq!(view_names, catalogue_names);
    assert_eq!(view.row_count(), EXPECTED_ROW_COUNT);
    assert_eq!(view.allowed_count(), EXPECTED_ALLOWED_COUNT);
    assert_eq!(view.denied_count(), EXPECTED_DENIED_COUNT);

    // Every denied row carries one of the five plain reasons plus its numbers.
    assert_eq!(
        view.denied_rows_without_reason(),
        Vec::<&'static str>::new(),
        "a denied row was drawn with no reason on it"
    );

    let counts = view.denied_reason_counts();
    for reason in ENCLAVE_ROLE_ABILITY_REASONS {
        let count = counts.get(reason).copied().unwrap_or_default();
        println!("TASK4858_REASON reason=\"{reason}\" denied_rows={count}");
        assert!(
            count > 0,
            "the fixture must show the reason {reason} on at least one denied row"
        );
    }
    assert_eq!(counts.values().sum::<usize>(), EXPECTED_DENIED_COUNT);

    // The exact reasons the finish line names, on the rows they belong to.
    assert_reason(&view, "rotate_keys", REASON_NO_CHANNEL_KEY);
    assert_reason(&view, "view_security_events", REASON_NO_CHANNEL_KEY);
    assert_reason(&view, "attach_files", REASON_CHANNEL_OVERRIDE_DENY);
    assert_reason(&view, "publish_announcements", REASON_CHANNEL_OVERRIDE_DENY);
    assert_reason(&view, "react_messages", REASON_RELAY_TIMEOUT_ACTIVE);
    assert_reason(&view, "start_voice", REASON_RELAY_TIMEOUT_ACTIVE);
    assert_reason(&view, "send_messages", REASON_ROLE_LIMIT_LOWER);
    assert_reason(&view, "mute_members", REASON_ROLE_LIMIT_LOWER);
    assert_reason(&view, "invite_members", REASON_ROLE_LIMIT_LOWER);
    assert_reason(&view, "manage_roles", REASON_PERMISSION_OFF);
    assert_reason(&view, "record_meetings", REASON_PERMISSION_OFF);

    for row in view.rows.iter().filter(|row| !row.allowed) {
        println!(
            "TASK4858_DENIED permission={} decided_by={} reason=\"{}\" detail=\"{}\"",
            row.permission_name,
            row.decided_by,
            row.reason.unwrap_or_default(),
            row.detail.as_deref().unwrap_or_default()
        );
    }
    for row in view.rows.iter().filter(|row| row.allowed) {
        println!(
            "TASK4858_ALLOWED permission={} decided_by={}",
            row.permission_name, row.decided_by
        );
        assert!(row.reason.is_none());
    }
}

#[test]
fn allowed_rows_never_carry_a_reason_and_denied_rows_always_do() {
    let view = fixture_ability_view();
    let reasons: BTreeSet<&str> = ENCLAVE_ROLE_ABILITY_REASONS.into_iter().collect();
    for row in &view.rows {
        if row.allowed {
            assert!(
                row.reason.is_none(),
                "{} was allowed with a reason",
                row.permission_name
            );
            assert!(row.detail.is_none());
        } else {
            let reason = row
                .reason
                .unwrap_or_else(|| panic!("{} was denied with no reason", row.permission_name));
            assert!(
                reasons.contains(reason),
                "{reason} is not a catalogue reason"
            );
            assert!(!row.detail.as_deref().unwrap_or_default().trim().is_empty());
        }
    }
    println!(
        "TASK4858_REASON_INTEGRITY denied_without_reason={} allowed_with_reason={}",
        view.denied_rows_without_reason().len(),
        view.rows
            .iter()
            .filter(|row| row.allowed && row.reason.is_some())
            .count()
    );
}

#[test]
fn the_view_reads_the_relay_budget_without_spending_it() {
    // Building the screen twice must give the same answer: previewing what a
    // role can do is not itself an action.
    let first = fixture_ability_view();
    let second = fixture_ability_view();
    assert_eq!(first, second);
    println!(
        "TASK4858_NON_MUTATING second_render_allowed={} second_render_denied={}",
        second.allowed_count(),
        second.denied_count()
    );
}

fn assert_reason(
    view: &osl_enclave_role_ability::EnclaveRoleAbilityView,
    permission_name: &str,
    expected_reason: &str,
) {
    let row = view
        .rows
        .iter()
        .find(|row| row.permission_name == permission_name)
        .unwrap_or_else(|| panic!("{permission_name} is not in the ability view"));
    assert!(!row.allowed, "{permission_name} was expected to be denied");
    assert_eq!(
        row.reason,
        Some(expected_reason),
        "{permission_name} showed the wrong reason"
    );
}
