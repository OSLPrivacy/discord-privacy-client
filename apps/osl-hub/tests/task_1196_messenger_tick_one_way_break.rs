use std::path::Path;

use ipc::allowed_places::{AllowedPlaceQuery, AllowedPlaceRecord};
use osl_privacy_hub::allowed_place_commands::{
    add_allowed_place_json, allowed_place_allowed_json, allowed_place_tick_json,
    list_allowed_places_json, remove_allowed_place_json,
};
use serde::Serialize;
use serde_json::Value;
use tempfile::TempDir;
use uuid::Uuid;

const APP: &str = "messenger";
const KIND: &str = "direct_message";
const FIRST_ACCOUNT: &str = "messenger-account-1196-a";

// TASK 1196b changes this only in a disposable test copy. Keeping the break at
// the exact action under test proves that the assertions notice a pair and tick
// which incorrectly survive removal.
const SKIP_MARKED_DIRECTION_REMOVAL: bool = false;

#[test]
fn task_1196_removing_one_marked_direction_breaks_the_pair_but_keeps_the_sender_rule() {
    let store = TempDir::new().expect("create TASK 1196 Messenger store");
    let mark = format!("TASK1196-MARK-{}", Uuid::new_v4().simple());
    let second_account = mark.clone();

    let marked_direction = AllowedPlaceRecord::messenger(FIRST_ACCOUNT, KIND, &second_account)
        .expect("create marked Messenger direction");
    let marked_stable_id = marked_direction.stable_id.clone();
    assert_eq!(marked_direction.person_name, mark);

    let permitted_sender_rule = AllowedPlaceRecord::messenger(&second_account, KIND, FIRST_ACCOUNT)
        .expect("create permitted Messenger sender rule");
    let permitted_sender_stable_id = permitted_sender_rule.stable_id.clone();

    add(store.path(), marked_direction);
    add(store.path(), permitted_sender_rule.clone());

    let before_records = list(store.path());
    let before_exact_mark_count = exact_mark_count(&before_records, &mark);
    let before_exact_mark = exact_mark(&before_records, &mark);
    let before_tick = tick(store.path(), &second_account);
    let before_pair_count = verification_tick_count(&before_tick);

    assert_eq!(before_exact_mark.as_deref(), Some(mark.as_str()));
    assert_eq!(before_exact_mark_count, 1);
    assert_eq!(before_pair_count, 1);
    assert_eq!(before_tick["state"]["state"], "two-way");

    let removed_once = if SKIP_MARKED_DIRECTION_REMOVAL {
        false
    } else {
        remove(store.path(), &marked_stable_id)
    };

    let after_records = list(store.path());
    let after_exact_mark_count = exact_mark_count(&after_records, &mark);
    let after_tick = tick(store.path(), &second_account);
    let after_pair_count = verification_tick_count(&after_tick);
    let permitted_sender_query = allowed(store.path(), &permitted_sender_rule);
    let permitted_sender_record_count = after_records["records"]
        .as_array()
        .expect("list returns records")
        .iter()
        .filter(|record| record["stable_id"] == permitted_sender_stable_id)
        .count();

    assert!(
        removed_once && after_exact_mark_count == 0 && after_pair_count == 0,
        "TASK1196 one-way break failed: removed_once={removed_once} marked_pair_count={after_exact_mark_count} verification_tick_count={after_pair_count}"
    );
    assert_eq!(
        after_exact_mark_count, 0,
        "TASK1196 marked pair remained present"
    );
    assert_eq!(
        after_pair_count, 0,
        "TASK1196 verification tick remained present"
    );
    assert_eq!(after_tick["state"]["state"], "one-way");
    assert_eq!(after_tick["state"]["verificationTicked"], false);
    assert_eq!(after_records["count"], 1);
    assert_eq!(permitted_sender_record_count, 1);
    assert_eq!(permitted_sender_query["allowed"], true);

    println!(
        "TASK1196 mark={mark} exact_mark_readable={} exact_mark_count_before={before_exact_mark_count} pair_count_before={before_pair_count} removed_once={removed_once} exact_mark_count_after={after_exact_mark_count} pair_count_after={after_pair_count} verification_tick_after={} remaining_record_count={} permitted_sender_rule_count={permitted_sender_record_count} permitted_sender_allowed={} permitted_sender_stable_id={permitted_sender_stable_id}",
        before_exact_mark.as_deref() == Some(mark.as_str()),
        after_tick["state"]["verificationTicked"],
        after_records["count"],
        permitted_sender_query["allowed"],
    );
}

fn add(store: &Path, record: AllowedPlaceRecord) {
    let value = json(add_allowed_place_json(store, record).expect("add allowed Messenger record"));
    assert_eq!(value["command"], "add");
    assert_eq!(value["ok"], true);
}

fn remove(store: &Path, stable_id: &str) -> bool {
    let value = json(
        remove_allowed_place_json(store, stable_id.to_owned())
            .expect("remove marked Messenger direction"),
    );
    assert_eq!(value["command"], "remove");
    value["removed"]
        .as_bool()
        .expect("remove returns a boolean")
}

fn list(store: &Path) -> Value {
    let value = json(list_allowed_places_json(store).expect("list allowed Messenger records"));
    assert_eq!(value["command"], "list");
    assert_eq!(value["ok"], true);
    value
}

fn tick(store: &Path, second_account: &str) -> Value {
    let value = json(
        allowed_place_tick_json(
            store,
            APP.to_owned(),
            KIND.to_owned(),
            FIRST_ACCOUNT.to_owned(),
            second_account.to_owned(),
        )
        .expect("query Messenger verification tick"),
    );
    assert_eq!(value["command"], "tick");
    assert_eq!(value["ok"], true);
    value
}

fn allowed(store: &Path, record: &AllowedPlaceRecord) -> Value {
    let value = json(
        allowed_place_allowed_json(
            store,
            AllowedPlaceQuery {
                app: record.app.clone(),
                account: record.account.clone(),
                kind: record.kind.clone(),
                stable_id: record.stable_id.clone(),
            },
        )
        .expect("query permitted Messenger sender rule"),
    );
    assert_eq!(value["command"], "allowed");
    assert_eq!(value["ok"], true);
    value
}

fn exact_mark_count(list: &Value, mark: &str) -> usize {
    list["records"]
        .as_array()
        .expect("list returns records")
        .iter()
        .filter(|record| record["person_name"] == mark)
        .count()
}

fn exact_mark(list: &Value, mark: &str) -> Option<String> {
    list["records"]
        .as_array()
        .expect("list returns records")
        .iter()
        .find_map(|record| {
            (record["person_name"] == mark)
                .then(|| record["person_name"].as_str().map(str::to_owned))
                .flatten()
        })
}

fn verification_tick_count(tick: &Value) -> usize {
    usize::from(
        tick["state"]["verificationTicked"]
            .as_bool()
            .expect("tick returns verificationTicked"),
    )
}

fn json(value: impl Serialize) -> Value {
    serde_json::to_value(value).expect("allowed-place command serializes")
}
