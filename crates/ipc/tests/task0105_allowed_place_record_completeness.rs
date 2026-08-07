use ipc::allowed_places::{
    add_allowed_place_record, count_allowed_place_records, read_allowed_place_record,
    AllowedPlaceRecord,
};
use rusqlite::Connection;
use tempfile::tempdir;

fn record(place_id: &str, display_name: &str) -> AllowedPlaceRecord {
    AllowedPlaceRecord {
        app: "discord".to_string(),
        account: "local-0105".to_string(),
        kind: "direct_message".to_string(),
        stable_id: format!("discord:local-0105:direct_message:{place_id}"),
        place_name: display_name.to_string(),
        person_name: display_name.to_string(),
    }
}

fn stored_record_bytes(app_data_dir: &std::path::Path) -> Vec<u8> {
    let conn = Connection::open(app_data_dir.join("allowed_places.sqlite")).expect("open store");
    let mut stmt = conn
        .prepare(
            "SELECT app, account, kind, stable_id, place_name, person_name \
             FROM allowed_places ORDER BY stable_id",
        )
        .expect("prepare byte-stable read");
    let rows = stmt
        .query_map([], |row| {
            Ok(format!(
                "{}\0{}\0{}\0{}\0{}\0{}\n",
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?
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
    let readable = read_allowed_place_record(&app_data_dir, &marble.stable_id)
        .expect("read MARBLE-0105")
        .expect("MARBLE-0105 exists");
    let before_count = count_allowed_place_records(&app_data_dir).expect("count after seed");
    println!("TASK0105 seed.readable={}", readable.place_name);
    println!("TASK0105 count.before={before_count}");

    let place = record("place-0105", "PLACE-0105");
    add_allowed_place_record(&app_data_dir, &place).expect("add PLACE-0105");
    let complete = read_allowed_place_record(&app_data_dir, &place.stable_id)
        .expect("read PLACE-0105")
        .expect("PLACE-0105 exists");
    let complete_count = count_allowed_place_records(&app_data_dir).expect("count after add");
    println!("TASK0105 complete_add.name={}", complete.place_name);
    println!("TASK0105 count.after_complete={complete_count}");

    assert_eq!(readable, marble);
    assert_eq!(before_count, 1);
    assert_eq!(complete, place);
    assert_eq!(complete_count, 2);

    let saved_before_bad_calls = stored_record_bytes(&app_data_dir);
    let bad_cases = [
        (
            "app",
            AllowedPlaceRecord {
                app: String::new(),
                ..place.clone()
            },
        ),
        (
            "account",
            AllowedPlaceRecord {
                account: String::new(),
                ..place.clone()
            },
        ),
        (
            "kind",
            AllowedPlaceRecord {
                kind: String::new(),
                ..place.clone()
            },
        ),
        (
            "stable_id",
            AllowedPlaceRecord {
                stable_id: String::new(),
                ..place.clone()
            },
        ),
        (
            "place_name",
            AllowedPlaceRecord {
                place_name: "bad\0place".to_string(),
                ..place.clone()
            },
        ),
        (
            "person_name",
            AllowedPlaceRecord {
                person_name: "bad\0person".to_string(),
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
        let marble_after = read_allowed_place_record(&app_data_dir, &marble.stable_id)
            .expect("read MARBLE-0105 after refusal")
            .expect("MARBLE-0105 still exists");
        let place_after = read_allowed_place_record(&app_data_dir, &place.stable_id)
            .expect("read PLACE-0105 after refusal")
            .expect("PLACE-0105 still exists");
        let bytes_unchanged = saved_after_bad == saved_before_bad_calls;

        println!("TASK0105 bad_call.changed_field={field}");
        println!("TASK0105 bad_call.error={error}");
        println!("TASK0105 bad_call.count_after={count_after_bad}");
        println!("TASK0105 bad_call.records_byte_for_byte_unchanged={bytes_unchanged}");

        assert!(
            error.to_string().contains(field),
            "error {error:?} must name changed field {field}"
        );
        assert_eq!(count_after_bad, 2);
        assert_eq!(marble_after, marble);
        assert_eq!(place_after, place);
        assert!(bytes_unchanged);
    }
}
