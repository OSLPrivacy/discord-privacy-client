use ipc::allowed_places::{
    add_allowed_place_record, count_allowed_place_records, get_allowed_place_record,
    AllowedPlaceRecord,
};
use rusqlite::Connection;
use tempfile::tempdir;

fn record(place_id: &str, display_name: &str) -> AllowedPlaceRecord {
    AllowedPlaceRecord {
        app_kind: "discord".to_string(),
        place_kind: "direct_message".to_string(),
        place_id: place_id.to_string(),
        display_name: Some(display_name.to_string()),
        found_at_unix_secs: 1_900_000_105,
    }
}

fn stored_record_bytes(app_data_dir: &std::path::Path) -> Vec<u8> {
    let conn = Connection::open(app_data_dir.join("allowed_places.sqlite")).expect("open store");
    let mut stmt = conn
        .prepare(
            "SELECT app_kind, place_kind, place_id, display_name, found_at_unix_secs \
             FROM allowed_places ORDER BY id",
        )
        .expect("prepare byte-stable read");
    let rows = stmt
        .query_map([], |row| {
            Ok(format!(
                "{}\0{}\0{}\0{}\0{}\n",
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                row.get::<_, i64>(4)?
            ))
        })
        .expect("query byte-stable records");
    let mut bytes = Vec::new();
    for row in rows {
        bytes.extend_from_slice(row.expect("read row").as_bytes());
    }
    bytes
}

#[test]
fn incomplete_allowed_place_records_are_refused_without_mutating_saved_records() {
    let dirs = tempdir().expect("temp dirs");
    let app_data_dir = dirs.path().join("app-data");

    let marble = record("marble-0105", "MARBLE-0105");
    add_allowed_place_record(&app_data_dir, &marble).expect("seed MARBLE-0105");
    let readable =
        get_allowed_place_record(&app_data_dir, "discord", "direct_message", "marble-0105")
            .expect("read MARBLE-0105")
            .expect("MARBLE-0105 exists");
    let before_count = count_allowed_place_records(&app_data_dir).expect("count after seed");
    println!(
        "TASK0105 seed.readable={}",
        readable.display_name.as_deref().unwrap_or("")
    );
    println!("TASK0105 count.before={before_count}");

    let place = record("place-0105", "PLACE-0105");
    add_allowed_place_record(&app_data_dir, &place).expect("add PLACE-0105");
    let complete =
        get_allowed_place_record(&app_data_dir, "discord", "direct_message", "place-0105")
            .expect("read PLACE-0105")
            .expect("PLACE-0105 exists");
    let complete_count = count_allowed_place_records(&app_data_dir).expect("count after add");
    println!(
        "TASK0105 complete_add.name={}",
        complete.display_name.as_deref().unwrap_or("")
    );
    println!("TASK0105 count.after_complete={complete_count}");

    assert_eq!(readable, marble);
    assert_eq!(before_count, 1);
    assert_eq!(complete, place);
    assert_eq!(complete_count, 2);

    let saved_before_bad_calls = stored_record_bytes(&app_data_dir);
    let bad_cases = [
        (
            "app_kind",
            AllowedPlaceRecord {
                app_kind: String::new(),
                ..place.clone()
            },
        ),
        (
            "place_kind",
            AllowedPlaceRecord {
                place_kind: String::new(),
                ..place.clone()
            },
        ),
        (
            "place_id",
            AllowedPlaceRecord {
                place_id: String::new(),
                ..place.clone()
            },
        ),
        (
            "display_name",
            AllowedPlaceRecord {
                display_name: None,
                ..place.clone()
            },
        ),
    ];

    for (field, bad) in bad_cases {
        let error = add_allowed_place_record(&app_data_dir, &bad)
            .expect_err("incomplete record must be refused");
        let count_after_bad =
            count_allowed_place_records(&app_data_dir).expect("count after refused add");
        let saved_after_bad = stored_record_bytes(&app_data_dir);
        let marble_after =
            get_allowed_place_record(&app_data_dir, "discord", "direct_message", "marble-0105")
                .expect("read MARBLE-0105 after refusal")
                .expect("MARBLE-0105 still exists");
        let place_after =
            get_allowed_place_record(&app_data_dir, "discord", "direct_message", "place-0105")
                .expect("read PLACE-0105 after refusal")
                .expect("PLACE-0105 still exists");
        let bytes_unchanged = saved_after_bad == saved_before_bad_calls;

        println!("TASK0105 bad_call.changed_field={field}");
        println!("TASK0105 bad_call.error={error}");
        println!("TASK0105 bad_call.count_after={count_after_bad}");
        println!("TASK0105 bad_call.records_byte_for_byte_unchanged={bytes_unchanged}");

        assert!(
            error.contains(field),
            "error {error:?} must name changed field {field}"
        );
        assert_eq!(count_after_bad, 2);
        assert_eq!(marble_after, marble);
        assert_eq!(place_after, place);
        assert!(bytes_unchanged);
    }
}
