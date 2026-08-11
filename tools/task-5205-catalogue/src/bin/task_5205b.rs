use std::{env, fs, process};

fn refuse(error: impl std::fmt::Display) -> ! {
    eprintln!("5205b: {error}");
    process::exit(1)
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.as_slice() {
        [command] if command == "semantic-control" => {
            let report = task_5205_catalogue::semantic::run_semantic_acceptance()
                .unwrap_or_else(|error| refuse(error));
            println!(
                "TASK5205B_SEMANTIC oracle={} contracts={} entry_points={} resolved={} task_5205={} task_5209={} task_5210={} task_5211={} task_5212_limits={} task_5213={} limit={:?}",
                report.oracle,
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
        [command, caller, path] if command == "check-semantic" => {
            let source = fs::read_to_string(path).unwrap_or_else(|error| {
                refuse(format!("caller={caller} key=<catalogue>: {path}: {error}"))
            });
            match task_5205_catalogue::semantic::check_external_for(caller, &source) {
                Ok(count) => println!(
                    "5205b semantic accepted caller={caller} contracts={count} oracle={}",
                    task_5205_catalogue::semantic::FIXED_ORACLE_ID,
                ),
                Err(error) => refuse(error),
            }
        }
        [command, caller, catalogue_path, oracle_path]
            if command == "attempt-self-derived-oracle" =>
        {
            let source = fs::read_to_string(catalogue_path).unwrap_or_else(|error| {
                refuse(format!("caller={caller} key=<catalogue>: {catalogue_path}: {error}"))
            });
            let oracle = fs::read_to_string(oracle_path).unwrap_or_else(|error| {
                refuse(format!("caller={caller} key=<oracle>: {oracle_path}: {error}"))
            });
            // The candidate still has to be structurally valid through the real
            // production loader. Its proposed oracle never becomes authority.
            let loaded = match caller.as_str() {
                task_5205_catalogue::WINDOWS_CATALOGUE_CALLER => {
                    task_5205_catalogue::load_external_windows_catalogue(&source)
                }
                task_5205_catalogue::SERVICE_CATALOGUE_CALLER => {
                    task_5205_catalogue::load_external_service_catalogue(&source)
                }
                _ => refuse("unknown production entry caller"),
            };
            let catalogue = loaded.unwrap_or_else(|error| {
                refuse(task_5205_catalogue::semantic::enrich_structural_error(&error))
            });
            task_5205_catalogue::semantic::reject_self_derived_oracle(&catalogue, &oracle)
                .unwrap_or_else(|error| refuse(error));
        }
        [command, caller, path] if command == "check-external" => {
            let source = fs::read_to_string(path).unwrap_or_else(|error| {
                refuse(format!(
                    "production_entry_caller={caller} key=<catalogue> fallback=disabled: {path}: {error}"
                ))
            });
            let production_caller = match caller.as_str() {
                "windows" => task_5205_catalogue::WINDOWS_CATALOGUE_CALLER,
                "service" => task_5205_catalogue::SERVICE_CATALOGUE_CALLER,
                value => value,
            };
            match task_5205_catalogue::semantic::check_external_for(production_caller, &source) {
                Ok(contracts) => println!(
                    "5205b semantic accepted production_entry_caller={production_caller} contracts={contracts} oracle={} fallback=disabled",
                    task_5205_catalogue::semantic::FIXED_ORACLE_ID,
                ),
                Err(error) => refuse(error),
            }
        }
        [command, caller] if command == "attempt-bypass" => refuse(format!(
            "key=<catalogue> production_entry_caller={caller} semantic_owner=production.entry expected_meaning_id=5205.strict-production-entry actual_meaning_id=parallel-permissive-loader expected_severity=release-blocking actual_severity=release-blocking expected_disposition=refuse actual_disposition=bypass syntactic_keys=valid catalogue_backed=true collision=false self_derived=false fallback=parallel-permissive-loader"
        )),
        _ => refuse("production_entry_caller=<cli> key=<catalogue> fallback=disabled: usage: task-5205b semantic-control | check-external <windows|service|production-caller> <path> | check-semantic <production-caller> <path> | attempt-self-derived-oracle <production-caller> <catalogue> <oracle> | attempt-bypass <production-caller>"),
    }
}
