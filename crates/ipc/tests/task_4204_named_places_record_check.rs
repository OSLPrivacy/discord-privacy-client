use ipc::named_places::{
    canonical_named_places, validate_named_places, NamedPlace, PlaceDisposition,
};

fn record_fault_count(report: &ipc::named_places::NamedPlacesReport) -> usize {
    report.repeated_rows.len()
        + report.unused_place_rows.len()
        + report.invalid_source_tasks.len()
        + report.missing_rows.len()
        + report.unexpected_rows.len()
}

#[test]
fn task4204_canonical_records_have_zero_record_faults() {
    let rows = canonical_named_places().expect("canonical place rows");
    let report = validate_named_places(&rows);
    let fault_count = record_fault_count(&report);

    println!("TASK4204_TEST_ROW_COUNT={}", report.row_count);
    println!("TASK4204_TEST_FAULT_COUNT={fault_count}");

    assert_eq!(report.row_count, 13);
    assert_eq!(fault_count, 0);
    assert!(report.is_valid());
}

#[test]
fn task4204_duplicate_telegram_supergroup_is_named_repeat() {
    let mut rows = canonical_named_places().expect("canonical place rows");
    let repeated = rows
        .iter()
        .find(|row| row.place == "Telegram supergroup")
        .expect("Telegram supergroup row")
        .clone();
    rows.push(repeated);
    let report = validate_named_places(&rows);

    for row in &report.repeated_rows {
        println!("TASK4204_TEST_REPEAT_PLACE={}", row.place);
    }

    assert_eq!(
        report
            .repeated_rows
            .iter()
            .map(|row| row.place.as_str())
            .collect::<Vec<_>>(),
        vec!["Telegram supergroup"]
    );
    assert!(!report.is_valid());
}

#[test]
fn task4204_signal_broadcast_is_named_unused_place() {
    let mut rows = canonical_named_places().expect("canonical place rows");
    rows.push(NamedPlace {
        source_task: "1029a".to_owned(),
        place: "Signal broadcast".to_owned(),
        disposition: PlaceDisposition::Approved,
    });
    let report = validate_named_places(&rows);

    for row in &report.unused_place_rows {
        println!("TASK4204_TEST_UNUSED_PLACE={}", row.place);
    }

    assert_eq!(
        report
            .unused_place_rows
            .iter()
            .map(|row| row.place.as_str())
            .collect::<Vec<_>>(),
        vec!["Signal broadcast"]
    );
    assert!(!report.is_valid());
}
