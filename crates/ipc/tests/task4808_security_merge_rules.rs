use ipc::ordinary_sync::{
    check_security_field_coverage, merge_security_fields, FieldValue, FieldWiseState,
    SecurityFieldValue, VersionStamp, SECURITY_MERGE_RULES,
};
use serde_json::json;
use std::collections::BTreeMap;

fn stamp(counter: u64, device: &str) -> VersionStamp {
    VersionStamp::new(counter, device)
}

fn security_field(name: &str, value: &str, counter: u64, device: &str) -> SecurityFieldValue {
    SecurityFieldValue::new(name, value, stamp(counter, device))
}

fn ordinary_state(field_name: &str, value: &str, counter: u64, device: &str) -> FieldWiseState {
    FieldWiseState::from_fields([(
        field_name.to_owned(),
        FieldValue::new(json!(value), stamp(counter, device)),
    )])
}

#[test]
fn task4808_security_choices_never_merge_toward_more_access() {
    let pairs = [
        ("permission_state", "revoked", "allowed"),
        ("identity_verification", "unverified", "verified"),
        ("person_block", "blocked", "unblocked"),
        ("request_disposition", "refused", "permitted"),
    ];

    let mut runs = 0usize;
    let mut exceptions = 0usize;
    for (field_name, safer, more_access) in pairs {
        for (label, left, right) in [
            (
                "safer-left",
                security_field(field_name, safer, 1, "old-safe-device"),
                security_field(field_name, more_access, 99, "late-more-access-device"),
            ),
            (
                "safer-right",
                security_field(field_name, more_access, 99, "late-more-access-device"),
                security_field(field_name, safer, 1, "old-safe-device"),
            ),
        ] {
            let merged = merge_security_fields([left], [right]).expect("security merge");
            let winner = &merged
                .merged
                .get(field_name)
                .expect("merged named field")
                .value;
            let route = merged
                .decisions
                .iter()
                .find(|decision| decision.field_name == field_name)
                .expect("decision for named field")
                .route_name();
            println!("TASK4808 pair={field_name} order={label} result={winner} route={route}");
            runs += 1;
            if winner != safer || route != "safer-wins" {
                exceptions += 1;
            }
        }
    }
    assert_eq!(runs, 8);
    assert_eq!(exceptions, 0);
    println!("TASK4808 named_pair_runs={runs}");
    println!("TASK4808 named_pair_exceptions={exceptions}");

    let ordinary_reallows =
        ordinary_state("permission_state", "revoked", 1, "owner").merge(&ordinary_state(
            "permission_state",
            "allowed",
            48 * 60 * 60,
            "stolen-device-clock-ahead",
        ));
    assert_eq!(
        ordinary_reallows.get("permission_state"),
        Some(&json!("allowed"))
    );
    println!(
        "TASK4808 ordinary_lww_clock_ahead_result={}",
        ordinary_reallows.get("permission_state").unwrap()
    );

    let stolen_clock_merge = merge_security_fields(
        [security_field("permission_state", "revoked", 1, "owner")],
        [security_field(
            "permission_state",
            "allowed",
            48 * 60 * 60,
            "stolen-device-clock-ahead",
        )],
    )
    .expect("security merge keeps revocation");
    let stolen_clock_result = &stolen_clock_merge
        .merged
        .get("permission_state")
        .expect("permission state")
        .value;
    assert_eq!(stolen_clock_result, "revoked");
    println!(
        "TASK4808 stolen_clock_ahead_hours=48 attempted=revoked_to_allowed merged={stolen_clock_result}"
    );

    let field_names: Vec<&str> = SECURITY_MERGE_RULES
        .iter()
        .map(|rule| rule.field_name)
        .collect();
    let coverage = check_security_field_coverage(field_names).expect("all fields covered");
    let coverage_by_name: BTreeMap<_, _> = coverage
        .iter()
        .map(|field| (field.field_name.as_str(), field.route.as_str()))
        .collect();
    for rule in SECURITY_MERGE_RULES {
        let route = coverage_by_name
            .get(rule.field_name)
            .expect("coverage entry");
        assert_eq!(*route, "safer-wins");
        println!(
            "TASK4808 security_field={} route={} ordinary_route=last-writer-wins blocked=true",
            rule.field_name, route
        );
    }
    println!("TASK4808 security_fields_checked={}", coverage.len());

    let missing_rule = check_security_field_coverage(
        SECURITY_MERGE_RULES
            .iter()
            .map(|rule| rule.field_name)
            .chain(["new_security_field_without_rule"]),
    )
    .expect_err("new security field without a rule must fail closed");
    let missing_rule_text = missing_rule.to_string();
    assert!(missing_rule_text.contains("new_security_field_without_rule"));
    println!("TASK4808 missing_rule_exit=1 field=new_security_field_without_rule error={missing_rule_text}");
}
