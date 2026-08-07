use ipc::named_places::{
    canonical_named_places, named_places_from_json, validate_named_places, NamedPlace,
    EXPECTED_APPROVED_COUNT, EXPECTED_LOOK_ONLY_REFUSED_COUNT, EXPECTED_ROW_COUNT,
};
use std::collections::BTreeSet;
use std::path::PathBuf;

fn read_rows() -> Result<Vec<NamedPlace>, String> {
    match std::env::args_os().nth(1) {
        Some(path) => {
            let path = PathBuf::from(path);
            let contents = std::fs::read_to_string(&path).map_err(|error| {
                format!(
                    "OSL places record check refused {}: {error}",
                    path.display()
                )
            })?;
            named_places_from_json(&contents)
        }
        None => canonical_named_places(),
    }
}

fn main() {
    let rows = match read_rows() {
        Ok(rows) => rows,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    let report = validate_named_places(&rows);

    for row in &rows {
        println!(
            "TASK4204_ROW source_task={} place={} disposition={}",
            row.source_task,
            row.place,
            row.disposition.as_str()
        );
    }
    println!("TASK4204_ROW_COUNT={}", report.row_count);
    println!("TASK4204_APPROVED_COUNT={}", report.approved_count);
    println!(
        "TASK4204_LOOK_ONLY_REFUSED_COUNT={}",
        report.look_only_refused_count
    );

    let mut fault_count = 0usize;
    if report.row_count != EXPECTED_ROW_COUNT {
        fault_count += 1;
        eprintln!(
            "TASK4204_FAULT rows={} expected_rows={}",
            report.row_count, EXPECTED_ROW_COUNT
        );
    }
    if report.approved_count != EXPECTED_APPROVED_COUNT {
        fault_count += 1;
        eprintln!(
            "TASK4204_FAULT approved={} expected_approved={}",
            report.approved_count, EXPECTED_APPROVED_COUNT
        );
    }
    if report.look_only_refused_count != EXPECTED_LOOK_ONLY_REFUSED_COUNT {
        fault_count += 1;
        eprintln!(
            "TASK4204_FAULT look_only_refused={} expected_look_only_refused={}",
            report.look_only_refused_count, EXPECTED_LOOK_ONLY_REFUSED_COUNT
        );
    }

    let repeated_places: BTreeSet<&str> = report
        .repeated_rows
        .iter()
        .map(|row| row.place.as_str())
        .collect();
    let unused_places: BTreeSet<&str> = report
        .unused_place_rows
        .iter()
        .map(|row| row.place.as_str())
        .collect();

    for row in &report.repeated_rows {
        fault_count += 1;
        eprintln!(
            "TASK4204_FAULT place={} fault=repeat source_task={} disposition={}",
            row.place,
            row.source_task,
            row.disposition.as_str()
        );
    }
    for row in &report.unused_place_rows {
        fault_count += 1;
        eprintln!(
            "TASK4204_FAULT place={} fault=place no task uses source_task={} disposition={}",
            row.place,
            row.source_task,
            row.disposition.as_str()
        );
    }
    for source_task in &report.invalid_source_tasks {
        fault_count += 1;
        eprintln!("TASK4204_FAULT invalid_source_task={source_task}");
    }
    for row in &report.missing_rows {
        fault_count += 1;
        eprintln!(
            "TASK4204_FAULT place={} fault=missing source_task={} disposition={}",
            row.place,
            row.source_task,
            row.disposition.as_str()
        );
    }
    for row in &report.unexpected_rows {
        if repeated_places.contains(row.place.as_str())
            || unused_places.contains(row.place.as_str())
        {
            continue;
        }
        fault_count += 1;
        eprintln!(
            "TASK4204_FAULT place={} fault=unexpected source_task={} disposition={}",
            row.place,
            row.source_task,
            row.disposition.as_str()
        );
    }

    println!("TASK4204_FAULT_COUNT={fault_count}");
    if fault_count == 0 && report.is_valid() {
        return;
    }
    std::process::exit(1);
}
