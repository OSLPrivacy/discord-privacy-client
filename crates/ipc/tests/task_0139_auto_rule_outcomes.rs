use ipc::allowed_places::{allowed_places_db_path, AllowedPlaceRecord};
use ipc::commands::{cmd_osl_new_place, cmd_osl_save_auto_whitelist_rule};
use ipc::state::AppState;
use rusqlite::Connection;
use std::path::Path;

const SELF_ID: &str = "900000000000013900";
const NEVER_ID: &str = "900000000000013901";
const PENDING_ID: &str = "900000000000013902";
const ALLOWED_ID: &str = "900000000000013903";
const FRIEND_ID: &str = "900000000000013904";

fn allowed_place_count(dir: &Path) -> i64 {
    let path = allowed_places_db_path(dir);
    if !path.exists() {
        return 0;
    }
    let conn = Connection::open(path).expect("open allowed places db");
    conn.query_row("SELECT COUNT(*) FROM allowed_places", [], |row| row.get(0))
        .expect("count allowed places")
}

#[test]
fn task_0139_new_place_returns_each_saved_auto_rule_outcome_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::new();
    let mut outcomes = Vec::new();

    cmd_osl_save_auto_whitelist_rule(&state, "discord".to_owned(), "never".to_owned(), None)
        .expect("save never rule");
    let never = cmd_osl_new_place(
        &state,
        AllowedPlaceRecord::discord_direct_message(SELF_ID, NEVER_ID),
        Some(dir.path().to_path_buf()),
    )
    .expect("call new-place for never rule");
    assert_eq!(never.rule, "never");
    assert_eq!(never.status, "unlisted");
    assert!(!never.prompt);
    outcomes.push("never-skipped");

    cmd_osl_save_auto_whitelist_rule(&state, "discord".to_owned(), "ask me".to_owned(), None)
        .expect("save ask-me rule");
    let pending = cmd_osl_new_place(
        &state,
        AllowedPlaceRecord::discord_direct_message(SELF_ID, PENDING_ID),
        Some(dir.path().to_path_buf()),
    )
    .expect("call new-place for ask-me rule");
    let allow_request = pending
        .allow_request
        .as_ref()
        .expect("ask-me rule returns pending allow request");
    assert_eq!(pending.rule, "ask me");
    assert_eq!(pending.status, "pending_allow_request");
    assert!(pending.prompt);
    assert_eq!(allow_request.allowed_choices, vec!["allow", "deny"]);
    outcomes.push("pending");

    cmd_osl_save_auto_whitelist_rule(&state, "discord".to_owned(), "always".to_owned(), None)
        .expect("save always rule");
    let before_allowed = allowed_place_count(dir.path());
    let allowed = cmd_osl_new_place(
        &state,
        AllowedPlaceRecord::discord_direct_message(SELF_ID, ALLOWED_ID),
        Some(dir.path().to_path_buf()),
    )
    .expect("call new-place for always rule");
    let after_allowed = allowed_place_count(dir.path());
    assert_eq!(allowed.rule, "always");
    assert_eq!(allowed.status, "allowed");
    assert_eq!(after_allowed - before_allowed, 1);
    outcomes.push("allowed");

    state
        .friend_ids
        .lock()
        .expect("friend_ids mutex poisoned")
        .push(FRIEND_ID.to_owned());
    cmd_osl_save_auto_whitelist_rule(
        &state,
        "discord".to_owned(),
        "only if a friend".to_owned(),
        None,
    )
    .expect("save only-if-friend rule");
    let before_friend = allowed_place_count(dir.path());
    let friend = cmd_osl_new_place(
        &state,
        AllowedPlaceRecord::discord_direct_message(SELF_ID, FRIEND_ID),
        Some(dir.path().to_path_buf()),
    )
    .expect("call new-place for only-if-friend rule");
    let after_friend = allowed_place_count(dir.path());
    assert_eq!(friend.rule, "only if a friend");
    assert_eq!(friend.status, "allowed");
    assert_eq!(after_friend - before_friend, 1);
    outcomes.push("friend-only");

    println!("TASK0139 outcome[0]={}", outcomes[0]);
    println!("TASK0139 outcome[1]={}", outcomes[1]);
    println!("TASK0139 outcome[2]={}", outcomes[2]);
    println!("TASK0139 outcome[3]={}", outcomes[3]);
    println!("TASK0139 outcomes={}", outcomes.join(","));

    assert_eq!(
        outcomes,
        vec!["never-skipped", "pending", "allowed", "friend-only"]
    );
}
