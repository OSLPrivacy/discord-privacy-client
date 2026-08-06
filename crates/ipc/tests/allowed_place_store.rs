use ipc::allowed_places::{add_allowed_place_record, AllowedPlaceRecord};
use rusqlite::Connection;

#[test]
fn adding_two_allowed_places_leaves_direct_store_count_two() {
    let tmp = tempfile::tempdir().unwrap();

    let first =
        AllowedPlaceRecord::discord_direct_message("900000000000000001", "900000000000000003");
    let second =
        AllowedPlaceRecord::discord_direct_message("900000000000000001", "900000000000000004");

    add_allowed_place_record(tmp.path(), &first).unwrap();
    add_allowed_place_record(tmp.path(), &second).unwrap();

    let conn = Connection::open(tmp.path().join("allowed_places.sqlite")).unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM allowed_places", [], |row| row.get(0))
        .unwrap();
    println!("TASK0101 allowed_place_store.count={count}");

    assert_eq!(count, 2);
}
