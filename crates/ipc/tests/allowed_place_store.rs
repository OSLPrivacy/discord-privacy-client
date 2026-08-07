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
use ipc::allowed_places::{
    add_allowed_place_record, allowed_place_is_allowed, allowed_places_db_path,
    list_allowed_place_records, remove_allowed_place_record, AllowedPlaceQuery, AllowedPlaceRecord,
};
use rusqlite::Connection;
use tempfile::TempDir;

#[test]
fn add_remove_list_and_allowed_queries_use_the_same_store() {
    let dir = TempDir::new().unwrap();
    let saved = AllowedPlaceRecord::discord_direct_message("account-0107", "place-0107");
    let other = AllowedPlaceRecord::discord_direct_message("account-0107", "place-other");

    let added = add_allowed_place_record(dir.path(), saved.clone()).unwrap();
    assert_eq!(added, saved);

    let query = AllowedPlaceQuery::from(saved.clone());
    let other_query = AllowedPlaceQuery::from(other);
    assert!(allowed_place_is_allowed(dir.path(), &query).unwrap());
    assert!(!allowed_place_is_allowed(dir.path(), &other_query).unwrap());

    let listed = list_allowed_place_records(dir.path()).unwrap();
    assert_eq!(listed, vec![saved.clone()]);

    let conn = Connection::open(allowed_places_db_path(dir.path())).unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM allowed_places", [], |row| row.get(0))
        .unwrap();
    println!("TASK0107 ipc_store_after_add_count={count}");
    assert_eq!(count, 1);

    let removed = remove_allowed_place_record(dir.path(), &saved.stable_id).unwrap();
    assert!(removed);
    assert!(!allowed_place_is_allowed(dir.path(), &query).unwrap());
    let listed_after_remove = list_allowed_place_records(dir.path()).unwrap();
    println!(
        "TASK0107 ipc_store_removed={removed} list_after_remove_count={}",
        listed_after_remove.len()
    );
    assert!(listed_after_remove.is_empty());
}
