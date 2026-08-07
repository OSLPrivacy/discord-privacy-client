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
use ipc::allowed_places::{
    add_allowed_place_record, allowed_places_db_path, is_allowed_place_record,
    read_allowed_place_record, remove_allowed_place_record, AllowedPlaceRecord,
    AllowedPlaceStoreError,
};
use ipc::commands::{
    cmd_osl_run_allowed_place_action, cmd_osl_trace_allowed_place_protected_message_path,
    compare_allowed_place_direction_state, AllowedPlaceAction, ProtectedPlaceAction,
};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
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
fn allowed_place_command_boundary_rejects_bad_stable_ids() {
    let apricot_record =
        AllowedPlaceRecord::discord_direct_message("900000000000010800", "APRICOT-0108");
    let added_record =
        AllowedPlaceRecord::discord_direct_message("900000000000010800", "APRICOT-0108-added");

    run_task_0108_case("add", &apricot_record, |dir| {
        let before = store_state(dir);
        add_allowed_place_record(dir, added_record.clone()).unwrap();
        let after = store_state(dir);
        println!(
            "TASK0108 valid_add record={} before_count={} after_count={} before_fingerprint={} after_fingerprint={}",
            added_record.stable_id, before.count, after.count, before.fingerprint, after.fingerprint
        );
        assert_eq!(before.count, 1);
        assert_eq!(after.count, 2);
        after
    });

    run_task_0108_case("read", &apricot_record, |dir| {
        let before = store_state(dir);
        let read = read_allowed_place_record(dir, &apricot_record.stable_id)
            .unwrap()
            .expect("APRICOT-0108 must be readable");
        let after = store_state(dir);
        println!(
            "TASK0108 valid_read record={} before_count={} after_count={} before_fingerprint={} after_fingerprint={}",
            read.stable_id, before.count, after.count, before.fingerprint, after.fingerprint
        );
        assert_eq!(read, apricot_record);
        assert_eq!(before.count, 1);
        assert_eq!(after.count, 1);
        assert_eq!(after.fingerprint, before.fingerprint);
        after
    });

    run_task_0108_case("remove", &apricot_record, |dir| {
        let before = store_state(dir);
        let removed = remove_allowed_place_record(dir, &apricot_record.stable_id).unwrap();
        let after = store_state(dir);
        println!(
            "TASK0108 valid_remove record={} removed={} before_count={} after_count={} before_fingerprint={} after_fingerprint={}",
            apricot_record.stable_id, removed, before.count, after.count, before.fingerprint, after.fingerprint
        );
        assert!(removed);
        assert_eq!(before.count, 1);
        assert_eq!(after.count, 0);
        after
    });
}

fn run_task_0108_case(
    command: &str,
    apricot_record: &AllowedPlaceRecord,
    valid_call: impl FnOnce(&Path) -> StoreState,
) {
    let base = TempDir::new().unwrap();
    add_allowed_place_record(base.path(), apricot_record.clone()).unwrap();
    let base_state = store_state(base.path());
    let readable = read_allowed_place_record(base.path(), &apricot_record.stable_id)
        .unwrap()
        .expect("APRICOT-0108 must be readable");
    println!(
        "TASK0108 command={command} seed_readable={} seed_count={} seed_fingerprint={}",
        readable.stable_id, base_state.count, base_state.fingerprint
    );
    assert_eq!(readable.stable_id, apricot_record.stable_id);
    assert_eq!(base_state.count, 1);

    let good = TempDir::new().unwrap();
    copy_store(base.path(), good.path());
    let good_after = valid_call(good.path());
    assert_eq!(store_state(good.path()), good_after);

    for bad_id in ["", "APRICOT-0108"] {
        let bad = TempDir::new().unwrap();
        copy_store(base.path(), bad.path());
        let before_bad = store_state(bad.path());
        let err = match command {
            "add" => {
                let mut bad_record = apricot_record.clone();
                bad_record.stable_id = bad_id.to_string();
                add_allowed_place_record(bad.path(), bad_record).unwrap_err()
            }
            "read" => read_allowed_place_record(bad.path(), bad_id).unwrap_err(),
            "remove" => remove_allowed_place_record(bad.path(), bad_id).unwrap_err(),
            _ => unreachable!("unknown TASK0108 command"),
        };
        let after_bad = store_state(bad.path());
        println!(
            "TASK0108 bad_{command} stable_id={bad_id:?} refused=\"{err}\" before_count={} after_count={} before_fingerprint={} after_fingerprint={}",
            before_bad.count,
            after_bad.count,
            before_bad.fingerprint,
            after_bad.fingerprint
        );
        assert!(matches!(
            err,
            AllowedPlaceStoreError::InvalidStableId { .. }
        ));
        assert_eq!(before_bad, after_bad);
        assert_eq!(after_bad, base_state);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StoreState {
    count: i64,
    fingerprint: String,
}

fn store_state(dir: &Path) -> StoreState {
    let conn = Connection::open(allowed_places_db_path(dir)).unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM allowed_places", [], |row| row.get(0))
        .unwrap();
    let mut stmt = conn
        .prepare("SELECT app, account, kind, stable_id FROM allowed_places ORDER BY stable_id ASC")
        .unwrap();
    let rows: Vec<String> = stmt
        .query_map([], |row| {
            Ok(format!(
                "{}\t{}\t{}\t{}",
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?
            ))
        })
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap();
    let mut hasher = Sha256::new();
    for row in rows {
        hasher.update(row.as_bytes());
        hasher.update(b"\n");
    }
    StoreState {
        count,
        fingerprint: format!("{:x}", hasher.finalize()),
    }
}

fn copy_store(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    fs::copy(allowed_places_db_path(from), allowed_places_db_path(to)).unwrap();
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
fn allowed_place_actions_refuse_one_character_near_match_ids() {
    let dir = TempDir::new().unwrap();
    let orchid_record =
        AllowedPlaceRecord::discord_direct_message("900000000000012500", "ORCHID-0125");
    let mut near_match_record = orchid_record.clone();
    near_match_record.stable_id = orchid_record
        .stable_id
        .replace("ORCHID-0125", "ORCHID-0126");
    assert_eq!(
        hamming_distance(&orchid_record.stable_id, &near_match_record.stable_id),
        1
    );

    add_allowed_place_record(dir.path(), orchid_record.clone()).unwrap();
    let readable = read_allowed_place_record(dir.path(), &orchid_record.stable_id)
        .unwrap()
        .expect("ORCHID-0125 must be readable");
    let before = store_state(dir.path());
    let mut action_count = 0usize;
    println!(
        "TASK0125 seed_readable={} before_action_count={} store_count={} fingerprint={}",
        readable.stable_id, action_count, before.count, before.fingerprint
    );
    assert_eq!(readable, orchid_record);
    assert_eq!(action_count, 0);

    for (action, expected_item, item_key) in [
        (AllowedPlaceAction::Read, "read", "read"),
        (AllowedPlaceAction::Prepare, "draft", "draft"),
        (AllowedPlaceAction::Place, "sent item", "sent_item"),
        (AllowedPlaceAction::Scrub, "Scrub item", "Scrub_item"),
    ] {
        let receipt = cmd_osl_run_allowed_place_action(
            dir.path().to_path_buf(),
            action,
            orchid_record.clone(),
        )
        .unwrap_or_else(|err| {
            panic!(
                "TASK0125 exact-ID action error action={} stable_id={} err={err}",
                action.as_str(),
                orchid_record.stable_id
            )
        });
        action_count += 1;
        println!(
            "TASK0125 exact_id action={} named_{item_key}={:?} stable_id={} action_count={}",
            receipt.action, receipt.item_name, receipt.stable_id, action_count
        );
        assert_eq!(receipt.stable_id, orchid_record.stable_id);
        assert_eq!(receipt.action, action.as_str());
        assert_eq!(receipt.item_name, expected_item);
    }
    assert_eq!(action_count, 4);

    for action in [
        AllowedPlaceAction::Read,
        AllowedPlaceAction::Prepare,
        AllowedPlaceAction::Place,
        AllowedPlaceAction::Scrub,
    ] {
        let before_near_match = store_state(dir.path());
        let refused = cmd_osl_run_allowed_place_action(
            dir.path().to_path_buf(),
            action,
            near_match_record.clone(),
        )
        .unwrap_err();
        let after_near_match = store_state(dir.path());
        println!(
            "TASK0125 near_match action={} stable_id={} refused={refused:?} action_count={} before_fingerprint={} after_fingerprint={}",
            action.as_str(),
            near_match_record.stable_id,
            action_count,
            before_near_match.fingerprint,
            after_near_match.fingerprint
        );
        assert!(refused.contains("place not allowed"));
        assert!(refused.contains(&near_match_record.stable_id));
        assert_eq!(action_count, 4);
        assert_eq!(after_near_match, before_near_match);
        assert_eq!(after_near_match, before);
    }

    let final_readable = read_allowed_place_record(dir.path(), &orchid_record.stable_id)
        .unwrap()
        .expect("ORCHID-0125 must remain readable");
    let after = store_state(dir.path());
    println!(
        "TASK0125 final_readable={} final_action_count={} final_fingerprint={} original_fingerprint={}",
        final_readable.stable_id, action_count, after.fingerprint, before.fingerprint
    );
    assert_eq!(final_readable, orchid_record);
    assert_eq!(action_count, 4);
    assert_eq!(after, before);
}

fn hamming_distance(left: &str, right: &str) -> usize {
    assert_eq!(left.len(), right.len());
    left.bytes()
        .zip(right.bytes())
        .filter(|(left, right)| left != right)
        .count()
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
