use shipping_transition_guard::shipping_transition_guard::{ShippingStateKind, TRANSITION_GATES};

#[test]
fn every_excluded_actor_attacks_and_every_current_authority_succeeds() {
    let report = shipping_transition_guard::task_5302_matrix::run_matrix()
        .expect("TASK5302 matrix must pass");

    assert_eq!(report.rows.len(), 10);
    assert_eq!(
        report.rows.iter().map(|row| row.gate).collect::<Vec<_>>(),
        TRANSITION_GATES
    );
    assert_eq!(report.hostile_actions, 12);
    assert_eq!(report.hostile_state_changes, 0);
    assert_eq!(report.control_state_changes, 10);
    assert_eq!(report.verifier_calls, 22);
    assert_eq!(report.final_state.total(), 10);
    for kind in ShippingStateKind::ALL {
        println!(
            "TASK5302_STATE kind={} count={}",
            kind.name(),
            report.final_state.count(kind)
        );
    }
    for row in &report.rows {
        println!(
            "TASK5302_TEST_ROW gate={} stale_signer={} current_signer={} before_epoch={} after_epoch={} before_grant={} after_grant={} hostile_state_changes={} control_state_changes={}",
            row.gate,
            row.stale_signer,
            row.current_signer,
            row.before_epoch,
            row.after_epoch,
            row.before_grant,
            row.after_grant,
            row.hostile_state_changes,
            row.control_state_changes,
        );
    }
    println!(
        "TASK5302_TEST rows=10 hostile_actions=12 hostile_state_changes=0 current_controls=10 verifier_calls=22 finish_line=CHECKED_OFF"
    );
}
