use osl_privacy_hub::named_places::{
    summarize_named_places, task_4203_summary_is_green, NamedPlaceRecord,
    TASK_4203_EXPECTED_APPROVED_COUNT, TASK_4203_EXPECTED_LOOK_ONLY_REFUSED_COUNT,
    TASK_4203_EXPECTED_ROW_COUNT,
};

fn usage() -> ! {
    eprintln!("usage: task_4203_named_places read");
    std::process::exit(2);
}

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let Some("read") = args.first().map(String::as_str) else {
        usage();
    };
    if args.len() != 1 {
        usage();
    }

    let records = load_task_4203_rows().unwrap_or_else(|error| {
        eprintln!("TASK4203_ERROR={error}");
        std::process::exit(1);
    });

    read_task_4203_rows(&records);
}

fn load_task_4203_rows() -> Result<Vec<NamedPlaceRecord>, String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join("named_places_4203.json");
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    serde_json::from_str(&text).map_err(|error| format!("parse {}: {error}", path.display()))
}

fn read_task_4203_rows(records: &[NamedPlaceRecord]) {
    let summary = summarize_named_places(records);

    println!("TASK4203_READER=task_4203_named_places");
    for (index, record) in records.iter().enumerate() {
        println!(
            "TASK4203_ROW index={} app={} place={} state={} source_task={} proving_check={} person_can_do={}",
            index + 1,
            record.app,
            record.place_name,
            record.state.as_str(),
            record.source_task,
            record.proving_check,
            record.person_can_do
        );
    }

    println!("TASK4203_ROW_COUNT={}", summary.row_count);
    println!("TASK4203_APPROVED_COUNT={}", summary.approved_count);
    println!(
        "TASK4203_LOOK_ONLY_REFUSED_COUNT={}",
        summary.look_only_refused_count
    );
    println!("TASK4203_SOURCE_TASK_COUNT={}", summary.source_task_count);
    println!(
        "TASK4203_MISSING_REQUIRED_PLACE_COUNT={}",
        summary.missing_required_place_names.len()
    );
    for place_name in &summary.missing_required_place_names {
        println!("TASK4203_MISSING_REQUIRED_PLACE={place_name}");
    }
    println!(
        "TASK4203_INVALID_SOURCE_TASK_COUNT={}",
        summary.invalid_source_tasks.len()
    );
    for source_task in &summary.invalid_source_tasks {
        println!("TASK4203_INVALID_SOURCE_TASK={source_task}");
    }

    if !task_4203_summary_is_green(&summary) {
        eprintln!(
            "TASK4203_ERROR=finish line mismatch expected_rows={} expected_approved={} expected_look_only_refused={}",
            TASK_4203_EXPECTED_ROW_COUNT,
            TASK_4203_EXPECTED_APPROVED_COUNT,
            TASK_4203_EXPECTED_LOOK_ONLY_REFUSED_COUNT
        );
        std::process::exit(1);
    }
}
