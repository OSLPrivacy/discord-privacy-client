use ipc::named_places::{
    canonical_named_places, named_places_from_json, validate_named_places, PlaceDisposition,
    EXPECTED_APPROVED_COUNT, EXPECTED_LOOK_ONLY_REFUSED_COUNT, EXPECTED_ROW_COUNT,
};

#[test]
fn task4203_canonical_reader_has_thirteen_honest_rows() {
    let rows = canonical_named_places().expect("canonical place rows");
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

    assert!(report.is_valid(), "{report:?}");
    assert_eq!(report.row_count, EXPECTED_ROW_COUNT);
    assert_eq!(report.approved_count, EXPECTED_APPROVED_COUNT);
    assert_eq!(
        report.look_only_refused_count,
        EXPECTED_LOOK_ONLY_REFUSED_COUNT
    );
    assert!(rows.iter().all(|row| !row.source_task.is_empty()));
}

#[test]
fn task4203_missing_telegram_supergroup_goes_red_with_exact_counts() {
    let mut rows = canonical_named_places().expect("canonical place rows");
    rows.retain(|row| row.place != "Telegram supergroup");
    let report = validate_named_places(&rows);

    println!("TASK4203_BREAK_ROW_COUNT={}", report.row_count);
    println!("TASK4203_BREAK_APPROVED_COUNT={}", report.approved_count);
    println!(
        "TASK4203_BREAK_LOOK_ONLY_REFUSED_COUNT={}",
        report.look_only_refused_count
    );
    for row in &report.missing_rows {
        println!(
            "TASK4203_BREAK_MISSING_ROW source_task={} place={} disposition={}",
            row.source_task,
            row.place,
            row.disposition.as_str()
        );
    }

    assert!(!report.is_valid());
    assert_eq!(report.row_count, 12);
    assert_eq!(report.approved_count, 10);
    assert_eq!(report.look_only_refused_count, 2);
    assert!(report.missing_rows.iter().any(|row| {
        row.source_task == "1027"
            && row.place == "Telegram supergroup"
            && row.disposition == PlaceDisposition::Approved
    }));
}

#[test]
fn task4203_rejects_unknown_source_tasks() {
    let json = r#"{
      "schema": "osl-allowed-places-v1",
      "places": [
        {
          "source_task": "0000",
          "place": "Telegram supergroup",
          "disposition": "approved"
        }
      ]
    }"#;
    let rows = named_places_from_json(json).expect("parse rows");
    let report = validate_named_places(&rows);

    println!(
        "TASK4203_INVALID_SOURCE_TASK_COUNT={}",
        report.invalid_source_tasks.len()
    );

    assert_eq!(
        report.invalid_source_tasks,
        vec!["0000:Telegram supergroup"]
    );
    assert!(!report.is_valid());
}
