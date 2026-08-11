#[test]
fn exact_shipping_entries_meet_task_5205() {
    let report = task_5205_catalogue::run_acceptance().expect("5205 acceptance");
    assert_eq!(report.production_entry_points, 2);
    assert_eq!(report.invalid_refusals_before_rendering, 6);
    assert_eq!(report.changed_values, 5);
    assert_eq!(report.changed_production_results, 5);
    assert_eq!(report.second_locale_registrations, 0);
    println!(
        "TASK5205_TEST version={} production_keys={} entry_points={} invalid_refusals={} changed_values={} changed_results={} second_locales={}",
        report.version,
        report.production_keys,
        report.production_entry_points,
        report.invalid_refusals_before_rendering,
        report.changed_values,
        report.changed_production_results,
        report.second_locale_registrations,
    );
}
