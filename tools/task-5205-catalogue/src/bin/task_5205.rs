fn main() {
    match task_5205_catalogue::run_acceptance() {
        Ok(report) => {
            println!(
                "TASK5205 version={} packaged_bytes={} production_keys={} second_locale_registration={} production_entry_points={} invalid_refusals_before_rendering={} changed_values={} changed_production_results={} resolver={}",
                report.version,
                report.packaged_bytes,
                report.production_keys,
                report.second_locale_registrations,
                report.production_entry_points,
                report.invalid_refusals_before_rendering,
                report.changed_values,
                report.changed_production_results,
                report.resolver,
            );
        }
        Err(error) => {
            eprintln!("5205 release check failed: {error}");
            std::process::exit(1);
        }
    }
}
