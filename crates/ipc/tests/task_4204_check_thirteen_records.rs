use ipc::named_places::{
    check_task_4204_real_records, check_task_4204_records_json, Task4204Fault, EXPECTED_ROW_COUNT,
    TASK_4204_REAL_RECORDS_JSON,
};

fn run_child(input: String) -> std::process::Output {
    std::process::Command::new(std::env::current_exe().expect("current test binary"))
        .arg("task_4204_probe_child")
        .arg("--exact")
        .arg("--ignored")
        .arg("--nocapture")
        .env("TASK4204_RECORDS_JSON", input)
        .output()
        .expect("run 4204 probe child")
}

#[test]
fn task_4204_real_records_print_thirteen_rows_and_zero_faults() {
    let report = check_task_4204_real_records().expect("check real records");

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

    assert!(report.is_green(), "{report:?}");
    assert_eq!(report.row_count, EXPECTED_ROW_COUNT);
    assert_eq!(report.fault_count(), 0);
}

#[test]
fn task_4204_repeated_app_and_place_exits_one_naming_both() {
    let mut value: serde_json::Value =
        serde_json::from_str(TASK_4204_REAL_RECORDS_JSON).expect("real records are JSON");
    let rows = value.as_array_mut().expect("real records are an array");
    let first_app = rows[0]["app"].clone();
    let first_place = rows[0]["placeName"].clone();
    rows[1]["app"] = first_app;
    rows[1]["placeName"] = first_place;

    let output = run_child(serde_json::to_string(&value).expect("mutated records stay JSON"));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!(
        "TASK4204_DUPLICATE_EXIT={}",
        output.status.code().unwrap_or(-1)
    );
    println!("TASK4204_DUPLICATE_STDOUT={}", stdout.trim());
    println!("TASK4204_DUPLICATE_STDERR={}", stderr.trim());

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr.contains("TASK4204_FAULT=duplicate_app_place"));
    assert!(stderr.contains("app=Telegram"));
    assert!(stderr.contains("place=Telegram supergroup"));
}

#[test]
fn task_4204_short_row_exits_one_naming_missing_part() {
    let mut value: serde_json::Value =
        serde_json::from_str(TASK_4204_REAL_RECORDS_JSON).expect("real records are JSON");
    value.as_array_mut().expect("real records are an array")[0]
        .as_object_mut()
        .expect("row is an object")
        .remove("provingCheck");

    let output = run_child(serde_json::to_string(&value).expect("mutated records stay JSON"));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!(
        "TASK4204_MISSING_PART_EXIT={}",
        output.status.code().unwrap_or(-1)
    );
    println!("TASK4204_MISSING_PART_STDOUT={}", stdout.trim());
    println!("TASK4204_MISSING_PART_STDERR={}", stderr.trim());

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr.contains("TASK4204_FAULT=missing_part"));
    assert!(stderr.contains("part=provingCheck"));
}

#[test]
fn task_4204_invented_place_exits_one_naming_place() {
    let mut value: serde_json::Value =
        serde_json::from_str(TASK_4204_REAL_RECORDS_JSON).expect("real records are JSON");
    value.as_array_mut().expect("real records are an array")[0]["placeName"] =
        serde_json::Value::String("Invented nowhere".to_owned());

    let output = run_child(serde_json::to_string(&value).expect("mutated records stay JSON"));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!(
        "TASK4204_INVENTED_PLACE_EXIT={}",
        output.status.code().unwrap_or(-1)
    );
    println!("TASK4204_INVENTED_PLACE_STDOUT={}", stdout.trim());
    println!("TASK4204_INVENTED_PLACE_STDERR={}", stderr.trim());

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr.contains("TASK4204_FAULT=invented_place"));
    assert!(stderr.contains("place=Invented nowhere"));
}

#[test]
#[ignore]
fn task_4204_probe_child() {
    let input = std::env::var("TASK4204_RECORDS_JSON").expect("child records JSON is supplied");
    let report = check_task_4204_records_json(&input).expect("check child records");
    println!("TASK4204_ROW_COUNT={}", report.row_count);
    println!("TASK4204_FAULT_COUNT={}", report.fault_count());
    for fault in &report.faults {
        eprintln!("{}", fault.report_line());
    }

    if report.faults.is_empty() {
        std::process::exit(0);
    }
    std::process::exit(1);
}

#[test]
fn task_4204_fault_lines_are_exact_for_judge_strings() {
    assert_eq!(
        Task4204Fault::DuplicateAppPlace {
            app: "Telegram".to_owned(),
            place_name: "Telegram supergroup".to_owned(),
        }
        .report_line(),
        "TASK4204_FAULT=duplicate_app_place app=Telegram place=Telegram supergroup"
    );
    assert_eq!(
        Task4204Fault::MissingPart {
            row: 1,
            part: "provingCheck",
        }
        .report_line(),
        "TASK4204_FAULT=missing_part row=1 part=provingCheck"
    );
    assert_eq!(
        Task4204Fault::InventedPlace {
            place_name: "Invented nowhere".to_owned(),
        }
        .report_line(),
        "TASK4204_FAULT=invented_place place=Invented nowhere"
    );
}
