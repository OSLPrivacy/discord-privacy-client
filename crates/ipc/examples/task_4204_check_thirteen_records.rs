use ipc::named_places::{check_task_4204_real_records, check_task_4204_records_json};
use std::path::PathBuf;

fn main() {
    let report = match std::env::args_os().nth(1) {
        Some(path) => {
            let path = PathBuf::from(path);
            let input = std::fs::read_to_string(&path).unwrap_or_else(|error| {
                eprintln!("TASK4204_ERROR=read {}: {error}", path.display());
                std::process::exit(1);
            });
            check_task_4204_records_json(&input)
        }
        None => check_task_4204_real_records(),
    }
    .unwrap_or_else(|error| {
        eprintln!("TASK4204_ERROR={error}");
        std::process::exit(1);
    });

    for (index, record) in report.records.iter().enumerate() {
        println!(
            "TASK4204_ROW index={} app={} place={} state={} source_task={} proving_check={}",
            index + 1,
            record.app,
            record.place_name,
            record.state.as_str(),
            record.source_task,
            record.proving_check
        );
    }
    println!("TASK4204_ROW_COUNT={}", report.row_count);
    println!("TASK4204_FAULT_COUNT={}", report.fault_count());

    if report.is_green() {
        println!("TASK4204_CHECK=ok");
        return;
    }

    for fault in &report.faults {
        eprintln!("{}", fault.report_line());
    }
    std::process::exit(1);
}
