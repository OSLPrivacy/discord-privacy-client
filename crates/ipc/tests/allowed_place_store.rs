use ipc::allowed_places::{
    add_allowed_place_record, allowed_places_db_path, is_allowed_place_record,
    remove_allowed_place_record, AllowedPlaceRecord,
};
use ipc::commands::{cmd_osl_trace_allowed_place_protected_message_path, ProtectedPlaceAction};
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

#[test]
fn removing_one_allowed_place_by_stable_id_leaves_the_other_unchanged() {
    let dir = TempDir::new().unwrap();
    let removed_record =
        AllowedPlaceRecord::discord_direct_message("900000000000000001", "900000000000000003");
    let remaining_record =
        AllowedPlaceRecord::discord_direct_message("900000000000000001", "900000000000000004");

    add_allowed_place_record(dir.path(), removed_record.clone()).unwrap();
    add_allowed_place_record(dir.path(), remaining_record.clone()).unwrap();

    let removed = remove_allowed_place_record(dir.path(), &removed_record.stable_id).unwrap();

    let conn = Connection::open(allowed_places_db_path(dir.path())).unwrap();
    let records: Vec<AllowedPlaceRecord> = conn
        .prepare("SELECT app, account, kind, stable_id FROM allowed_places ORDER BY stable_id ASC")
        .unwrap()
        .query_map([], |row| {
            Ok(AllowedPlaceRecord {
                app: row.get(0)?,
                account: row.get(1)?,
                kind: row.get(2)?,
                stable_id: row.get(3)?,
            })
        })
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap();

    println!(
        "TASK0102 removed={} remaining_count={} removed_id={} remaining_id={} remaining_app={} remaining_account={} remaining_kind={}",
        removed,
        records.len(),
        removed_record.stable_id,
        records[0].stable_id,
        records[0].app,
        records[0].account,
        records[0].kind,
    );

    assert!(removed);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0], remaining_record);
}

#[test]
fn allowed_place_command_trace_reaches_protected_message_path() {
    let dir = TempDir::new().unwrap();
    let allowed_record =
        AllowedPlaceRecord::discord_direct_message("900000000000000120", "LANTERN-0120");
    let unlisted_record =
        AllowedPlaceRecord::discord_direct_message("900000000000000120", "LANTERN-0120-moved");

    add_allowed_place_record(dir.path(), allowed_record.clone()).unwrap();
    assert!(is_allowed_place_record(dir.path(), &allowed_record).unwrap());
    assert!(!is_allowed_place_record(dir.path(), &unlisted_record).unwrap());

    let trace = cmd_osl_trace_allowed_place_protected_message_path(
        dir.path().to_path_buf(),
        ProtectedPlaceAction::Send,
        allowed_record.clone(),
    )
    .unwrap();
    for line in &trace {
        println!("{line}");
    }

    let refused = cmd_osl_trace_allowed_place_protected_message_path(
        dir.path().to_path_buf(),
        ProtectedPlaceAction::Send,
        unlisted_record.clone(),
    )
    .unwrap_err();
    println!(
        "TASK0120 unlisted stable_id={} refused={refused}",
        unlisted_record.stable_id
    );

    assert!(trace.iter().any(|line| {
        line == &format!(
            "TASK0120 protected-message path reached action=send stable_id={}",
            allowed_record.stable_id
        )
    }));
    assert!(refused.contains("allowed-place check refused"));
    assert!(refused.contains(&unlisted_record.stable_id));
}
