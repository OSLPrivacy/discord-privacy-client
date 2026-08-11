#[test]
fn exact_shipping_entries_meet_task_5205() {
    let report = task_5205_catalogue::run_acceptance().expect("5205 acceptance");
    let semantic = task_5205_catalogue::semantic::run_semantic_acceptance()
        .expect("5205b semantic acceptance");
    assert_eq!(report.production_entry_points, 2);
    assert_eq!(report.invalid_refusals_before_rendering, 6);
    assert_eq!(report.changed_values, 5);
    assert_eq!(report.changed_production_results, 5);
    assert_eq!(report.second_locale_registrations, 0);
    assert_eq!(report.production_keys, 135);
    assert_eq!(semantic.contracts, 126);
    assert_eq!(semantic.resolved_contracts, 252);
    assert_eq!(
        (
            semantic.task_5205,
            semantic.task_5209,
            semantic.task_5210,
            semantic.task_5211,
            semantic.task_5212_limits,
            semantic.task_5213,
        ),
        (0, 2, 18, 49, 1, 56),
    );
    println!(
        "TASK5205_TEST version={} production_keys={} entry_points={} invalid_refusals={} changed_values={} changed_results={} second_locales={} semantic_oracle={} semantic_contracts={} semantic_resolved={} task_5205={} task_5209={} task_5210={} task_5211={} task_5212_limits={} task_5213={}",
        report.version,
        report.production_keys,
        report.production_entry_points,
        report.invalid_refusals_before_rendering,
        report.changed_values,
        report.changed_production_results,
        report.second_locale_registrations,
        semantic.oracle,
        semantic.contracts,
        semantic.resolved_contracts,
        semantic.task_5205,
        semantic.task_5209,
        semantic.task_5210,
        semantic.task_5211,
        semantic.task_5212_limits,
        semantic.task_5213,
    );
}
