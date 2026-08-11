#[test]
fn fixed_oracle_covers_every_enumerated_downstream_contract() {
    let report = task_5205_catalogue::semantic::run_semantic_acceptance()
        .expect("5205b fixed semantic oracle acceptance");
    assert_eq!(report.oracle, "osl.english-semantics.5205b.v1");
    assert_eq!(task_5205_catalogue::INDEPENDENT_PRODUCTION_KEYS.len(), 135);
    assert_eq!(report.contracts, 126);
    assert_eq!(report.production_entry_points, 2);
    assert_eq!(report.resolved_contracts, 252);
    assert_eq!(report.task_5205, 0);
    assert_eq!(report.task_5209, 2);
    assert_eq!(report.task_5210, 18);
    assert_eq!(report.task_5211, 49);
    assert_eq!(report.task_5212_limits, 1);
    assert_eq!(report.task_5213, 56);
    assert_eq!(
        task_5205_catalogue::semantic::TASK_5212_LIMIT,
        "Keyboard and screen-reader operation is verified only for sending a message and changing a two-state setting; other controls may require a pointer."
    );
    println!(
        "TASK5205B_TEST oracle={} production_keys={} semantic_contracts={} entry_points={} resolved={} task_5205={} task_5209={} task_5210={} task_5211={} task_5212_limits={} task_5213={} limit={:?}",
        report.oracle,
        task_5205_catalogue::INDEPENDENT_PRODUCTION_KEYS.len(),
        report.contracts,
        report.production_entry_points,
        report.resolved_contracts,
        report.task_5205,
        report.task_5209,
        report.task_5210,
        report.task_5211,
        report.task_5212_limits,
        report.task_5213,
        task_5205_catalogue::semantic::TASK_5212_LIMIT,
    );
}
