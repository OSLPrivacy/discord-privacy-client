use ipc::commands::{
    cmd_osl_read_privacy_protection_choices, cmd_osl_save_privacy_level_rule_set,
    PrivacyProtectionChoicesDto,
};
use ipc::AppState;
use std::collections::BTreeSet;

fn decision_values(decision: &PrivacyProtectionChoicesDto) -> String {
    format!(
        "warnings={},cleanup={},app_exceptions={},contact_rules={}",
        decision.warnings, decision.cleanup, decision.app_exceptions, decision.contact_rules
    )
}

#[test]
fn task_0702_each_privacy_level_changes_the_protection_decision_command() {
    let state = AppState::new();
    let expected = [
        (
            "basic",
            "warnings=warnings_off,cleanup=cleanup_off,app_exceptions=app_exceptions_allowed,contact_rules=contacts_optional",
        ),
        (
            "balanced",
            "warnings=before_send_warnings,cleanup=attachment_cleaning_plus_30_day_review,app_exceptions=app_exceptions_reviewed,contact_rules=verified_contacts_suggested",
        ),
        (
            "maximum",
            "warnings=before_send_and_public_post_warnings,cleanup=attachment_cleaning_plus_7_day_review,app_exceptions=app_exceptions_restricted,contact_rules=protected_contacts_required",
        ),
    ];

    let mut recorded = Vec::new();
    for (level, expected_values) in expected {
        let saved = cmd_osl_save_privacy_level_rule_set(&state, level.to_owned(), None)
            .expect("save privacy level");
        assert_eq!(saved.level, level);

        let decision =
            cmd_osl_read_privacy_protection_choices(&state).expect("read protection decision");
        let values = decision_values(&decision);
        println!("TASK0702 privacy_level.{level}.protection_decision={values}");
        assert_eq!(decision.level, level);
        assert_eq!(values, expected_values);
        recorded.push(values);
    }

    let distinct = recorded.iter().collect::<BTreeSet<_>>().len();
    println!(
        "TASK0702 protection_decision.recorded_count={}",
        recorded.len()
    );
    println!("TASK0702 protection_decision.distinct_count={distinct}");
    println!(
        "TASK0702 expected_named_differences=basic:warnings_off|cleanup_off|app_exceptions_allowed|contacts_optional;balanced:before_send_warnings|attachment_cleaning_plus_30_day_review|app_exceptions_reviewed|verified_contacts_suggested;maximum:before_send_and_public_post_warnings|attachment_cleaning_plus_7_day_review|app_exceptions_restricted|protected_contacts_required"
    );

    assert_eq!(recorded.len(), 3);
    assert_eq!(distinct, 3);
}
