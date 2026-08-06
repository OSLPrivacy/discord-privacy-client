use ipc::allowed_places::{add_allowed_place_record, allowed_places_db_path, AllowedPlaceRecord};
use rusqlite::Connection;
use tempfile::TempDir;

#[test]
fn adding_two_allowed_places_leaves_direct_store_count_two() {
    let dir = TempDir::new().unwrap();

    add_allowed_place_record(
        dir.path(),
        AllowedPlaceRecord::discord_direct_message("900000000000000001", "900000000000000003"),
    )
    .unwrap();
    add_allowed_place_record(
        dir.path(),
        AllowedPlaceRecord::discord_direct_message("900000000000000001", "900000000000000004"),
    )
    .unwrap();

    let conn = Connection::open(allowed_places_db_path(dir.path())).unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM allowed_places", [], |row| row.get(0))
        .unwrap();

    println!("TASK0101 allowed_place_store.count={count}");
    assert_eq!(count, 2);
}
