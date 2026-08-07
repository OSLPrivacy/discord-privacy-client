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
use ipc::allowed_places::{
    add_allowed_place_record, allowed_places_db_path, is_allowed_place_record,
    remove_allowed_place_record, AllowedPlaceRecord,
};
use ipc::commands::{
    cmd_osl_trace_allowed_place_protected_message_path, compare_allowed_place_direction_state,
    ProtectedPlaceAction,
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

#[test]
fn removing_either_allowed_place_direction_hides_verification_immediately() {
    let dir = TempDir::new().unwrap();
    let first_to_second =
        AllowedPlaceRecord::discord_direct_message("900000000000017201", "900000000000017202");
    let second_to_first =
        AllowedPlaceRecord::discord_direct_message("900000000000017202", "900000000000017201");

    add_allowed_place_record(dir.path(), first_to_second.clone()).unwrap();
    add_allowed_place_record(dir.path(), second_to_first.clone()).unwrap();

    let visible = compare_allowed_place_direction_state(
        dir.path().to_path_buf(),
        first_to_second.clone(),
        second_to_first.clone(),
    )
    .unwrap();
    println!(
        "TASK0172 verification_command=compare_allowed_place_direction_state phase=both-present saved_directions={} whitelist_state={} verification_state={} first_to_second={} second_to_first={}",
        visible.saved_directions,
        visible.whitelist_state,
        visible.verification_state,
        visible.first_to_second,
        visible.second_to_first,
    );
    assert_eq!(visible.saved_directions, 2);
    assert_eq!(visible.whitelist_state, "two-way");
    assert_eq!(visible.verification_state, "visible");
    assert!(visible.first_to_second);
    assert!(visible.second_to_first);

    let removed_first =
        remove_allowed_place_record(dir.path(), &first_to_second.stable_id).unwrap();
    let first_removed = compare_allowed_place_direction_state(
        dir.path().to_path_buf(),
        first_to_second.clone(),
        second_to_first.clone(),
    )
    .unwrap();
    println!(
        "TASK0172 removed_direction=first_to_second removed={} removed_id={} remaining_id={} saved_directions={} whitelist_state={} verification_state={} first_to_second={} second_to_first={}",
        removed_first,
        first_to_second.stable_id,
        second_to_first.stable_id,
        first_removed.saved_directions,
        first_removed.whitelist_state,
        first_removed.verification_state,
        first_removed.first_to_second,
        first_removed.second_to_first,
    );
    assert!(removed_first);
    assert_eq!(first_removed.saved_directions, 1);
    assert_eq!(first_removed.whitelist_state, "one-way");
    assert_eq!(first_removed.verification_state, "hidden");
    assert!(!first_removed.first_to_second);
    assert!(first_removed.second_to_first);

    add_allowed_place_record(dir.path(), first_to_second.clone()).unwrap();
    let reset_visible = compare_allowed_place_direction_state(
        dir.path().to_path_buf(),
        first_to_second.clone(),
        second_to_first.clone(),
    )
    .unwrap();
    println!(
        "TASK0172 phase=reset-both-present saved_directions={} whitelist_state={} verification_state={} first_to_second={} second_to_first={}",
        reset_visible.saved_directions,
        reset_visible.whitelist_state,
        reset_visible.verification_state,
        reset_visible.first_to_second,
        reset_visible.second_to_first,
    );
    assert_eq!(reset_visible.verification_state, "visible");

    let removed_second =
        remove_allowed_place_record(dir.path(), &second_to_first.stable_id).unwrap();
    let second_removed = compare_allowed_place_direction_state(
        dir.path().to_path_buf(),
        first_to_second.clone(),
        second_to_first.clone(),
    )
    .unwrap();
    println!(
        "TASK0172 removed_direction=second_to_first removed={} removed_id={} remaining_id={} saved_directions={} whitelist_state={} verification_state={} first_to_second={} second_to_first={}",
        removed_second,
        second_to_first.stable_id,
        first_to_second.stable_id,
        second_removed.saved_directions,
        second_removed.whitelist_state,
        second_removed.verification_state,
        second_removed.first_to_second,
        second_removed.second_to_first,
    );
    assert!(removed_second);
    assert_eq!(second_removed.saved_directions, 1);
    assert_eq!(second_removed.whitelist_state, "one-way");
    assert_eq!(second_removed.verification_state, "hidden");
    assert!(second_removed.first_to_second);
    assert!(!second_removed.second_to_first);
}
