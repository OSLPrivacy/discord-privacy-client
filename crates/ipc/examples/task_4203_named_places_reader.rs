use ipc::named_places::{
    canonical_named_places, named_places_from_json, validate_named_places, EXPECTED_APPROVED_COUNT,
    EXPECTED_LOOK_ONLY_REFUSED_COUNT, EXPECTED_ROW_COUNT,
};
use std::path::PathBuf;

fn read_rows() -> Result<Vec<ipc::named_places::NamedPlace>, String> {
    match std::env::args_os().nth(1) {
        Some(path) => {
            let path = PathBuf::from(path);
            let contents = std::fs::read_to_string(&path).map_err(|error| {
                format!("OSL places reader refused {}: {error}", path.display())
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
            "TASK4203_ROW source_task={} place={} disposition={}",
            row.source_task,
            row.place,
            row.disposition.as_str()
        );
    }
    println!("TASK4203_ROW_COUNT={}", report.row_count);
    println!("TASK4203_APPROVED_COUNT={}", report.approved_count);
    println!(
        "TASK4203_LOOK_ONLY_REFUSED_COUNT={}",
        report.look_only_refused_count
    );

    if report.is_valid() {
        println!("TASK4203_VALIDATION=ok");
        return;
    }

    eprintln!(
        "TASK4203_VALIDATION=failed expected_rows={} expected_approved={} expected_look_only_refused={}",
        EXPECTED_ROW_COUNT, EXPECTED_APPROVED_COUNT, EXPECTED_LOOK_ONLY_REFUSED_COUNT
    );
    for row in &report.missing_rows {
        eprintln!(
            "TASK4203_MISSING_ROW source_task={} place={} disposition={}",
            row.source_task,
            row.place,
            row.disposition.as_str()
        );
    }
    for row in &report.unexpected_rows {
        eprintln!(
            "TASK4203_UNEXPECTED_ROW source_task={} place={} disposition={}",
            row.source_task,
            row.place,
            row.disposition.as_str()
        );
    }
    for source_task in &report.invalid_source_tasks {
        eprintln!("TASK4203_INVALID_SOURCE_TASK={source_task}");
    }
    std::process::exit(1);
}
